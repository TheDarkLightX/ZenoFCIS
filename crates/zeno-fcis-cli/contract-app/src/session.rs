//! One persistent session: genuine genesis, publication, exact replay and
//! outbox delivery through the library Authority and the SQLite v2 shell.

use std::path::Path;

use zeno_fcis_codec::{Domain, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::{MemoryDestination, v2::V2SqliteShell};
use zeno_fcis_synthesis::finite::{v2_authority::PublicationOutcome, v2_composition as c};

use crate::{AppResult, Example, authority, compare, genesis, genesis_numbers, v2_contract, wire};

/// What a session did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Summary {
    /// Each decision's class, in order.
    pub decisions: Vec<&'static str>,
    /// Bundles in the store, genesis included.
    pub bundles: u64,
    /// Outbox entries still pending.
    pub pending: u64,
    /// Outbox entries the destination received.
    pub deliveries: usize,
}

impl Summary {
    /// One JSON object.
    #[must_use]
    pub fn json(&self) -> String {
        let decisions: Vec<String> = self
            .decisions
            .iter()
            .map(|decision| format!("\"{decision}\""))
            .collect();
        format!(
            "{{\"status\":\"passed\",\"decisions\":[{}],\"bundles\":{},\"pending\":{},\"deliveries\":{}}}",
            decisions.join(","),
            self.bundles,
            self.pending,
            self.deliveries
        )
    }
}

fn replay_id(example: &Example) -> AppResult<Hash32> {
    let domain = Domain::new(concat!(env!("CARGO_PKG_NAME"), "/replay"), 1)
        .map_err(|error| format!("replay domain: {error:?}"))?;
    commitment::<RustCryptoSha256>(domain, format!("example-{}", example.line).as_bytes())
        .map_err(|error| format!("replay id: {error:?}"))
}

fn class_name(class: c::Class) -> &'static str {
    match class {
        c::Class::Accept => "Accept",
        c::Class::Reject => "Reject",
        c::Class::CommittedFailure => "CommittedFailure",
        _ => "Unsupported",
    }
}

/// Publishes genesis into a new database at `path`, then takes, while one
/// remains, the first example whose pre-state is the current state:
/// rejects must publish nothing, and each other decision is committed, then
/// recomputed from the same inputs and committed again as an idempotent
/// replay. Then the outbox is delivered, the first entry across a reopen of
/// the database before its acknowledgment.
///
/// # Errors
/// Returns the first refusal, difference or store failure.
pub fn journey(path: &Path, examples: &[Example]) -> AppResult<Summary> {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let initial = genesis()?;
    let genesis = match authority.publish_genesis(&initial) {
        PublicationOutcome::Commit(publication) => publication,
        other => return Err(format!("genesis: {other:?}")),
    };
    let store = |error| format!("store: {error:?}");
    let mut shell = V2SqliteShell::create(path, &authority, genesis).map_err(store)?;
    let mut state = genesis_numbers()?;
    let mut remaining: Vec<&Example> = examples.iter().collect();
    let mut decisions = Vec::new();
    while let Some(position) = remaining
        .iter()
        .position(|example| example.inputs.get(..state.len()) == Some(state.as_slice()))
    {
        let example = remaining.remove(position);
        let raw = wire(&descriptor, &example.inputs)?;
        let original = c::Raw {
            state: &raw[0],
            command: &raw[1],
            context: &raw[2],
        };
        let before = shell.snapshot().map_err(store)?;
        if before.state() != original.state {
            return Err(format!("line {}: the stored state differs", example.line));
        }
        match authority.publish(original) {
            PublicationOutcome::Commit(publication) => {
                let candidate = publication
                    .evaluation()
                    .result()
                    .map_err(|refusal| format!("line {}: {refusal:?}", example.line))?;
                compare(&descriptor, candidate, example)?;
                let subject = publication.subject().to_vec();
                let replay = replay_id(example)?;
                if shell.commit(replay, publication).map_err(store)?.status()
                    != CommitStatus::Committed
                {
                    return Err(format!("line {}: not a first publication", example.line));
                }
                let replayed = match authority.replay_publication(original, &subject) {
                    PublicationOutcome::Commit(publication) => publication,
                    other => return Err(format!("line {}: replay: {other:?}", example.line)),
                };
                if shell.commit(replay, replayed).map_err(store)?.status()
                    != CommitStatus::IdempotentReplay
                {
                    return Err(format!("line {}: replay was not idempotent", example.line));
                }
                state.clone_from(&example.post);
            }
            PublicationOutcome::Reject(evaluation) => {
                let candidate = evaluation
                    .result()
                    .map_err(|refusal| format!("line {}: {refusal:?}", example.line))?;
                compare(&descriptor, candidate, example)?;
                if shell.snapshot().map_err(store)? != before {
                    return Err(format!("line {}: a reject changed the store", example.line));
                }
            }
            other => return Err(format!("line {}: {other:?}", example.line)),
        }
        decisions.push(class_name(example.class));
    }
    let mut destination = MemoryDestination::default();
    if let Some(pending) = shell.next_pending().map_err(store)? {
        // An interruption after the destination records the first delivery
        // but before the store acknowledges it.
        let delivered = shell
            .deliver_next_memory_unacknowledged(&mut destination)
            .map_err(store)?;
        if delivered != Some((pending.delivery_id(), pending.entry_hash())) {
            return Err("the destination received a different entry".to_owned());
        }
        drop(shell);
        shell = V2SqliteShell::open(path, &authority).map_err(store)?;
        while shell.deliver_next_memory(&mut destination).map_err(store)? {}
        shell
            .acknowledge(pending.delivery_id(), pending.entry_hash())
            .map_err(store)?;
    }
    let snapshot = shell.snapshot().map_err(store)?;
    Ok(Summary {
        decisions,
        bundles: snapshot.bundle_count(),
        pending: snapshot.pending(),
        deliveries: destination.delivered_count(),
    })
}
