//! Local journal export and checked restore. No credential is written to a journal.
use super::journal::{Reader, VALUE_LIMIT};
use super::{Application, context_file, hex, identity, previous, selected_parties};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
};
use zeno_fcis_codec::Hash32;
use zeno_fcis_synthesis::finite::v2_authority::Authority;

fn contexts() -> Result<Vec<(Hash32, Vec<String>)>, String> {
    if let Some(parties) = selected_parties()? {
        Ok(parties
            .into_iter()
            .map(|party| (party.principal, party.context))
            .collect())
    } else {
        Ok(vec![(Hash32::new([7; 32]), context_file()?)])
    }
}

fn head(app: &Application<'_, '_>) -> Result<Value, String> {
    let roots = app
        .layout
        .capacities()
        .keys()
        .map(|map| {
            Ok((
                map.to_string(),
                hex(&app.store().root(*map).map_err(|e| e.to_string())?),
            ))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>, String>>()?;
    Ok(
        json!({"revision":app.store().version().map_err(|e|e.to_string())?.to_string(),"scalars":hex(&app.store().scalar_state().map_err(|e|e.to_string())?),"roots":roots}),
    )
}

fn write_journal(app: &Application<'_, '_>, mut output: impl Write) -> Result<(), String> {
    let expected = head(app)?;
    let version = app.store().version().map_err(|e| e.to_string())?;
    // Retain the compact Value serializer's canonical field order and newline.
    write!(
        output,
        "{{\"contract\":\"{}\",\"entries\":[",
        hex(identity(app.authority).as_bytes())
    )
    .map_err(|e| e.to_string())?;
    for sequence in 1..=version {
        if sequence > 1 {
            write!(output, ",").map_err(|e| e.to_string())?;
        }
        let row = app
            .journal_entry(sequence)
            .map_err(|e| format!("journal: {e:?}"))?;
        serde_json::to_writer(&mut output, &row).map_err(|e| e.to_string())?;
    }
    if head(app)? != expected {
        return Err("store changed during export".into());
    }
    writeln!(
        output,
        "],\"format\":\"zeno-fcis/collection-journal/2\",\"head\":{expected}}}"
    )
    .map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())
}

/// Page the unchanged JSON document. No unverified prefix is publishable.
fn read_journal(app: &mut Application<'_, '_>, input: impl BufRead) -> Result<Value, String> {
    if app.store().version().map_err(|e| e.to_string())? != 0 {
        return Err("restore requires an empty store".into());
    }
    let mut reader = Reader::new(input);
    reader.token(b'{')?;
    let mut fields = BTreeSet::new();
    let mut expected = None;
    while reader.more(b'}', fields.is_empty())? {
        let name = reader.value(64)?;
        let name = name.as_str().ok_or("journal field")?.to_owned();
        if !fields.insert(name.clone()) {
            return Err("duplicate journal field".into());
        }
        reader.token(b':')?;
        match name.as_str() {
            "entries" => {
                reader.token(b'[')?;
                let mut first = true;
                while reader.more(b']', first)? {
                    first = false;
                    let row = reader.value(VALUE_LIMIT)?;
                    app.replay_entry(&row)
                        .map_err(|e| format!("replay mismatch: {e:?}"))?;
                }
            }
            "format" => {
                if reader.value(VALUE_LIMIT)? != "zeno-fcis/collection-journal/2" {
                    return Err("journal binding".into());
                }
            }
            "contract" => {
                let contract = reader.value(VALUE_LIMIT)?;
                if contract != hex(identity(app.authority).as_bytes())
                    && contract != hex(previous::identity(app.authority).as_bytes())
                {
                    return Err("journal binding".into());
                }
            }
            "head" => expected = Some(reader.value(VALUE_LIMIT)?),
            _ => return Err("unknown journal field".into()),
        }
    }
    reader.end()?;
    if fields.len() != 4 {
        return Err("journal fields".into());
    }
    let actual = head(app)?;
    if expected.as_ref() != Some(&actual) {
        return Err("journal head mismatch".into());
    }
    Ok(actual)
}

/// Return Some only for a completed local operation; other arguments belong to the server.
pub(super) fn run(authority: &Authority<'_>, arguments: &[OsString]) -> Result<Option<()>, String> {
    match arguments {
        [flag, path] if flag == "--journal" || flag == "--audit" => {
            let app = Application::open(authority, Path::new(path), contexts()?)
                .map_err(|e| format!("audit: {e:?}"))?;
            if flag == "--audit" {
                println!("{}", json!({"status":"audited","head":head(&app)?}));
            } else {
                write_journal(&app, std::io::stdout().lock())?;
            }
            Ok(Some(()))
        }
        [flag, source, destination] if flag == "--replay" => {
            let destination = Path::new(destination);
            if destination.try_exists().map_err(|e| e.to_string())? {
                return Err("restore requires a new destination".into());
            }
            let source = BufReader::new(fs::File::open(source).map_err(|e| e.to_string())?);
            let parent = destination
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let staging = parent.join(format!(".collection-replay-{}-{nonce}", std::process::id()));
            fs::create_dir(&staging).map_err(|e| e.to_string())?;
            let result = (|| {
                let staged = staging.join("store.sqlite");
                let mut app = Application::open(authority, &staged, contexts()?)
                    .map_err(|e| format!("restore: {e:?}"))?;
                let actual = read_journal(&mut app, source)?;
                app.audit().map_err(|e| format!("restored audit: {e:?}"))?;
                drop(app);
                fs::File::open(&staged)
                    .and_then(|file| file.sync_all())
                    .map_err(|e| e.to_string())?;
                // Publishing a link refuses an existing destination, including a race.
                fs::hard_link(&staged, destination).map_err(|e| e.to_string())?;
                fs::File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|e| e.to_string())?;
                println!("{}", json!({"status":"replayed","head":actual}));
                Ok(Some(()))
            })();
            let removed = fs::remove_dir_all(&staging).map_err(|e| e.to_string());
            match result {
                Err(error) => Err(error),
                Ok(value) => {
                    removed?;
                    Ok(value)
                }
            }
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_shell_sqlite::collections::CollectionStore;

    fn with_empty(test: impl FnOnce(&mut Application<'_, '_>)) {
        let contract = crate::v2_contract::Contract::new();
        let descriptor = contract.descriptor();
        let authority = crate::authority(&descriptor).unwrap_or_else(|e| panic!("{e}"));
        let context: Vec<String> = serde_json::from_str(include_str!("../../context.json"))
            .unwrap_or_else(|e| panic!("{e}"));
        let store = CollectionStore::temporary().unwrap_or_else(|e| panic!("{e}"));
        let mut app =
            Application::with_store(&authority, store, vec![(Hash32::new([7; 32]), context)])
                .unwrap_or_else(|e| panic!("{e:?}"));
        test(&mut app);
    }

    #[test]
    fn paged_head_from_another_position_is_refused() {
        with_empty(|app| {
            let mut bytes = Vec::new();
            assert!(write_journal(app, &mut bytes).is_ok());
            let mut document: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            document["head"]["revision"] = json!("1");
            let data = serde_json::to_vec(&document).unwrap_or_default();
            assert!(
                read_journal(app, data.as_slice()).is_err(),
                "head from another journal position was accepted"
            );
        });
    }

    #[test]
    fn paged_document_checks_all_fields_and_trailing_bytes() {
        with_empty(|app| {
            let mut bytes = Vec::new();
            assert!(write_journal(app, &mut bytes).is_ok());
            assert!(read_journal(app, bytes.as_slice()).is_ok());
            let document: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            for (field, bad) in [
                ("contract", json!("00")),
                ("format", json!("unknown")),
                ("head", json!(null)),
                ("extra", json!(null)),
            ] {
                let mut changed = document.clone();
                changed[field] = bad;
                assert!(
                    read_journal(app, changed.to_string().as_bytes()).is_err(),
                    "accepted {field}"
                );
            }
            for bad in [
                b"{}".as_slice(),
                b"{\"entries\":[],}".as_slice(),
                b"{\"entries\":[,]}".as_slice(),
                b"{\"entries\":[],\"entries\":[]}".as_slice(),
            ] {
                assert!(
                    read_journal(app, bad).is_err(),
                    "malformed document accepted"
                );
            }
            bytes.push(b'x');
            assert!(
                read_journal(app, bytes.as_slice()).is_err(),
                "trailing data accepted"
            );
            // A valid old object order is retained; binding need not precede entries.
            let reordered = format!(
                "{{\"head\":{},\"format\":{},\"entries\":[],\"contract\":{}}}",
                document["head"], document["format"], document["contract"]
            );
            assert!(read_journal(app, reordered.as_bytes()).is_ok());
        });
    }
}
