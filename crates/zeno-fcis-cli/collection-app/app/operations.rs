//! Local journal export and checked restore. No credential is written to a journal.
use super::{Application, context_file, hex, identity, selected_parties};
use serde_json::{Value, json};
use std::{ffi::OsString, fs, io::Read as _, path::Path};
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
/// Return Some only for a completed local operation; other arguments belong to the server.
pub(super) fn run(authority: &Authority<'_>, arguments: &[OsString]) -> Result<Option<()>, String> {
    match arguments {
        [flag, path] if flag == "--journal" || flag == "--audit" => {
            let app = Application::open(authority, Path::new(path), contexts()?)
                .map_err(|e| format!("audit: {e:?}"))?;
            let value = if flag == "--audit" {
                json!({"status":"audited","head":head(&app)?})
            } else {
                json!({"format":"zeno-fcis/collection-journal/2","contract":hex(identity(authority).as_bytes()),"entries":app.journal().map_err(|e|format!("journal: {e:?}"))?,"head":head(&app)?})
            };
            println!("{value}");
            Ok(Some(()))
        }
        [flag, source, destination] if flag == "--replay" => {
            let destination = Path::new(destination);
            if destination.try_exists().map_err(|e| e.to_string())? {
                return Err("restore requires a new destination".into());
            }
            let mut bytes = Vec::new();
            fs::File::open(source)
                .and_then(|file| file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes))
                .map_err(|e| e.to_string())?;
            if bytes.len() > 64 * 1024 * 1024 {
                return Err("journal exceeds 64 MiB".into());
            }
            let document: Value = serde_json::from_slice(&bytes).map_err(|_| "journal encoding")?;
            if document.as_object().map(|o| o.len()) != Some(4)
                || document["format"] != "zeno-fcis/collection-journal/2"
                || document["contract"] != hex(identity(authority).as_bytes())
            {
                return Err("journal binding".into());
            }
            let entries = document["entries"].as_array().ok_or("journal entries")?;
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
                app.replay(entries)
                    .map_err(|e| format!("replay mismatch: {e:?}"))?;
                app.audit().map_err(|e| format!("restored audit: {e:?}"))?;
                let actual = head(&app)?;
                if actual != document["head"] {
                    return Err("journal head mismatch".into());
                }
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
