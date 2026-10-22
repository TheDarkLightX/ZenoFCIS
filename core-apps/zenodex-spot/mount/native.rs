// Compiled as a child of the generated SQLite session. No economic rules here.
use super::*;
use zeno_fcis_shell_sqlite::v2::PublicationBundle;

fn numbers(values: &[i128]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn optional_bytes(bytes: Option<&[u8]>) -> String {
    bytes.map_or_else(|| "null".to_owned(), |bytes| format!("\"{}\"", hex(bytes)))
}

fn report(
    descriptor: &c::Descriptor<'_>,
    evaluation: &zeno_fcis_synthesis::finite::v2_authority::Evaluation<'_>,
    patch: Option<&[u8]>,
    bundle: Option<&PublicationBundle>,
    sequence: Option<u64>,
) -> AppResult<String> {
    let raw = evaluation.raw();
    let pre = state_numbers(descriptor, raw.state)?;
    let decision = decision_of(
        descriptor,
        evaluation.result().map_err(|e| format!("core: {e:?}"))?,
        &pre,
    )?;
    let deliveries = decision
        .outbox
        .iter()
        .map(|(channel, fields)| {
            format!(
                "[{},{}]",
                channel,
                numbers(&fields.iter().map(|(_, n)| *n).collect::<Vec<_>>())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        concat!(
            "{{\"schema\":\"zeno-fcis/mounted-decision/1\",",
            "\"class\":\"{}\",\"reason\":{},\"pre\":{},\"post\":{},\"deliveries\":[{}],",
            "\"authorization\":\"{}\",\"patch\":{},\"commit_plan\":{},\"outbox\":{},\"receipt\":{},\"commit\":{}}}"
        ),
        decision.class,
        decision
            .reason
            .map_or_else(|| "null".to_owned(), |r| r.to_string()),
        numbers(&pre),
        numbers(&decision.post),
        deliveries,
        hex(evaluation
            .subject()
            .map_err(|e| format!("subject: {e:?}"))?),
        optional_bytes(patch),
        optional_bytes(bundle.map(PublicationBundle::canonical_bytes)),
        optional_bytes(bundle.map(PublicationBundle::outbox)),
        optional_bytes(bundle.map(PublicationBundle::receipt)),
        sequence.map_or_else(|| "null".to_owned(), |n| n.to_string())
    ))
}

fn published_report(
    authority: &Authority<'_>,
    raw: c::Raw<'_>,
    bundle: Option<&PublicationBundle>,
    sequence: Option<u64>,
) -> AppResult<String> {
    match authority.publish(raw) {
        PublicationOutcome::Commit(publication) => report(
            authority.descriptor(),
            publication.evaluation(),
            Some(publication.poststate()),
            bundle,
            sequence,
        ),
        PublicationOutcome::Reject(evaluation) => {
            report(authority.descriptor(), &evaluation, None, None, None)
        }
        other => Err(format!("publication: {other:?}")),
    }
}

/// Native probe on explicitly supplied state; no durable publication authority.
pub fn probe(pre: &[i128], command: &[i128], context: &[i128]) -> AppResult<String> {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let inputs = pre
        .iter()
        .chain(command)
        .chain(context)
        .copied()
        .collect::<Vec<_>>();
    let raw = wire(&descriptor, &inputs)?;
    published_report(
        &authority,
        c::Raw {
            state: &raw[0],
            command: &raw[1],
            context: &raw[2],
        },
        None,
        None,
    )
}

/// Stable caller key. The checked durable history, rather than an HTTP cache,
/// owns replay. Serializes callers before snapshot acquisition; SQLite CAS remains.
pub fn submit_keyed(
    path: &Path,
    key: &str,
    command: &[i128],
    context: &[i128],
) -> AppResult<String> {
    let key = Hash32::new(digest(key)?);
    let mut journal = submission_journal(path).map_err(|e| e.to_string())?;
    with_lineage(|lineage| {
        let history = lineage.audit_history_read_only(path).map_err(store)?;
        let publishing = lineage.authorities().last().ok_or("empty lineage")?;
        let descriptor = publishing.descriptor();
        if history.contract_version != lineage.versions() {
            return Err("upgrade required".to_owned());
        }
        let mut shell = match lineage.open(path).map_err(store)? {
            Opened::V10(Store::Current(shell)) => shell,
            _ => return Err("current SQLite store required".to_owned()),
        };
        if let Some(previous) = history.commits.iter().find(|row| row.replay_id == key) {
            let pre = state_numbers(descriptor, &previous.prestate)?;
            let raw = framed(descriptor, &pre, command, context, &previous.prestate)?;
            if raw[1] != previous.command || raw[2] != previous.context {
                return Err("retry key belongs to different authenticated command".to_owned());
            }
            let original = c::Raw {
                state: &raw[0],
                command: &raw[1],
                context: &raw[2],
            };
            return match publishing.publish(original) {
                PublicationOutcome::Commit(p) => {
                    let text = report(
                        descriptor,
                        p.evaluation(),
                        Some(p.poststate()),
                        Some(&previous.bundle),
                        Some(previous.sequence),
                    )?;
                    let receipt = shell.commit(key, p).map_err(store)?;
                    if receipt.status() != CommitStatus::IdempotentReplay
                        || receipt.version() != previous.sequence
                    {
                        return Err("retry did not replay original receipt".to_owned());
                    }
                    Ok(text)
                }
                _ => Err("committed command no longer commits".to_owned()),
            };
        }
        let snapshot = shell.snapshot().map_err(store)?;
        let pre = state_numbers(descriptor, snapshot.state())?;
        let raw = framed(descriptor, &pre, command, context, snapshot.state())?;
        let original = c::Raw {
            state: &raw[0],
            command: &raw[1],
            context: &raw[2],
        };
        match publishing.publish(original) {
            PublicationOutcome::Commit(publication) => {
                let bundle = shell
                    .publication_bundle(snapshot.version(), key, &publication)
                    .map_err(store)?;
                let sequence = snapshot
                    .version()
                    .checked_add(1)
                    .ok_or("sequence overflow")?;
                let text = report(
                    descriptor,
                    publication.evaluation(),
                    Some(publication.poststate()),
                    Some(&bundle),
                    Some(sequence),
                )?;
                let kept = settle_journal(&mut journal, path, snapshot.version())
                    .map_err(|e| e.to_string())?;
                append_journal(
                    &mut journal,
                    path,
                    &Submission {
                        sequence,
                        contract_version: lineage.versions(),
                        command: command.to_vec(),
                        context: context.to_vec(),
                    },
                )
                .map_err(|e| e.to_string())?;
                // Only a compile-time test feature enables a process-death barrier.
                #[cfg(feature = "mount-test")]
                if let Some(marker) = std::env::var_os("SPOT_PAUSE_BEFORE_COMMIT") {
                    std::fs::write(marker, b"decision-ready").map_err(|e| e.to_string())?;
                    loop {
                        std::thread::park();
                    }
                }
                if let Err(error) = shell.commit_at(snapshot.version(), key, publication) {
                    let refusal = store(error);
                    truncate_journal(&journal, path, kept)
                        .map_err(|e| format!("{refusal}; journal: {e}"))?;
                    return Err(refusal);
                }
                Ok(text)
            }
            PublicationOutcome::Reject(evaluation) => {
                report(descriptor, &evaluation, None, None, None)
            }
            other => Err(format!("publication: {other:?}")),
        }
    })
}

/// Minimal process adapter: owned argv in, native JSON report out.
pub fn run(arguments: &[String]) -> AppResult<String> {
    let parse = |s: &str| -> AppResult<Vec<i128>> {
        s.split_whitespace()
            .map(|n| n.parse().map_err(|_| "integer required".to_owned()))
            .collect()
    };
    match arguments {
        [mode, path] if mode == "history" => history(Path::new(path))
            .map(|history| {
                format!(
                    "{{\"entries\":{},\"commits\":{}}}",
                    history.entries.len(),
                    history.head.commits
                )
            })
            .map_err(|e| e.to_string()),
        [mode, path] if mode == "init" => init(Path::new(path))
            .map(|head| head.json())
            .map_err(|e| e.to_string()),
        [mode, path] if mode == "state" => current(Path::new(path))
            .map(|current| {
                format!(
                    "{{\"state\":{},\"commits\":{},\"pending\":{}}}",
                    numbers(&current.state),
                    current.head.commits,
                    current.head.pending
                )
            })
            .map_err(|e| e.to_string()),
        [mode, pre, command, context] if mode == "probe" => {
            probe(&parse(pre)?, &parse(command)?, &parse(context)?)
        }
        [mode, path, key, command, context] if mode == "keyed" => {
            submit_keyed(Path::new(path), key, &parse(command)?, &parse(context)?)
        }
        _ => Err(
            "expected probe PRE COMMAND CONTEXT or keyed DATABASE KEY COMMAND CONTEXT".to_owned(),
        ),
    }
}
