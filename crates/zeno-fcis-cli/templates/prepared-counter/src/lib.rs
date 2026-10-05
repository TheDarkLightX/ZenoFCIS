#![forbid(unsafe_code)]

/// Complete original schema, decision, laws and genuine checked authority.
pub mod v2_contract;
pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/counter.rs"));
}
pub mod bindings {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/rust/project.rs"));
}
pub mod delivery;
#[cfg(feature = "sqlite")]
pub mod prepare;
#[path = "../profile.rs"]
pub mod profile;

#[cfg(feature = "sqlite")]
use bindings::GeneratedProject;
#[cfg(feature = "sqlite")]
use generated::{CounterState, CounterValue};
#[cfg(feature = "sqlite")]
use std::path::Path;
#[cfg(feature = "sqlite")]
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, decode_envelope};
#[cfg(feature = "sqlite")]
use zeno_fcis_crypto::RustCryptoSha256;
#[cfg(feature = "sqlite")]
use zeno_fcis_schema::ValidationLimits;
#[cfg(feature = "sqlite")]
use zeno_fcis_synthesis::finite::v2_authority::PublicationOutcome;
use zeno_fcis_synthesis::finite::v2_composition as composition;

/// Borrows a descriptor admitted by the actual library-owned constructors.
pub type Authority<'p> = zeno_fcis_synthesis::finite::v2_authority::Authority<'p>;
#[cfg(feature = "sqlite")]
pub type Shell<'a, 'p> = zeno_fcis_shell_sqlite::v2::V2SqliteShell<'a, 'p>;
pub type AppResult<T> = Result<T, String>;
#[cfg(feature = "sqlite")]
fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> AppResult<T> {
    value.map_err(|error| format!("{error:?}"))
}

/// Bind the complete original policy; caller-authored callbacks confer no authority.
/// # Errors
/// Returns the actual catalog or authority refusal.
pub fn authority<'p>(descriptor: &'p composition::Descriptor<'p>) -> AppResult<Authority<'p>> {
    v2_contract::checked_authority(descriptor).map_err(|error| format!("{error:?}"))
}

/// Checked genuine genesis stores the complete admitted H once in schema9.
/// # Errors
/// Returns schema, genesis, existing-store or SQLite failure.
#[cfg(feature = "sqlite")]
pub fn create<'a, 'p>(path: &Path, authority: &'a Authority<'p>) -> AppResult<Shell<'a, 'p>> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(project.admit_root::<RustCryptoSha256>(
        &CounterState {
            count: CounterValue(0),
        },
        ValidationLimits::default(),
    ))?;
    let initial = checked(initial.envelope().canonical_bytes())?;
    let genesis = match authority.publish_genesis(&initial) {
        PublicationOutcome::Commit(genesis) => genesis,
        other => return Err(format!("genuine genesis refused: {other:?}")),
    };
    checked(Shell::create(path, authority, genesis))
}

/// Decode an already checked envelope solely for typed input and display.
/// # Errors
/// Returns malformed frame or typed presentation failure.
#[cfg(feature = "sqlite")]
pub fn decode_state(original: &[u8]) -> AppResult<CounterState> {
    let envelope = checked(decode_envelope(original, DecodeLimits::default()))?;
    checked(CounterState::try_from_value(envelope.into_value()))
}

/// Original complete preparation, exact replay, restart and interrupted delivery.
/// The checked cursor grants computation only; the scoped guard fully reevaluates
/// the original operation before the complete bounded schema9 commit.
/// # Errors
/// Returns actual admission, preparation, capacity, freshness or durable failure.
#[cfg(feature = "sqlite")]
pub fn journey(path: &Path) -> AppResult<String> {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let destination = delivery::Destination::default();
    let mut shell = create(path, &authority)?;
    let before = checked(shell.snapshot())?;
    let mut batch = prepare::PreparedBatch::start(
        &authority,
        &before,
        &prepare::command([1, 1, 1]),
        true,
        "batch-1",
    )?;
    batch.advance(0, 1)?;
    if checked(shell.snapshot())? != before {
        return Err("partial work published data".into());
    }
    batch.advance(1, 2)?;
    let publication = batch.publish(
        &mut shell,
        &authority,
        true,
        prepare::MAX_PUBLICATION_BYTES,
        None,
    )?;
    if publication.status != zeno_fcis_shell::CommitStatus::Committed {
        return Err("first publication was not committed".into());
    }
    if publication.replay(&mut shell, &authority)?.status()
        != zeno_fcis_shell::CommitStatus::IdempotentReplay
    {
        return Err("exact replay was not idempotent".into());
    }
    checked(shell.deliver_next_memory_unacknowledged(&mut destination.memory()))?
        .ok_or("missing notification")?;
    drop(shell);
    let mut shell = checked(Shell::open(path, &authority))?;
    while checked(shell.deliver_next_memory(&mut destination.memory()))? {}
    let mut exit = prepare::PreparedBatch::start(
        &authority,
        &checked(shell.snapshot())?,
        &prepare::command([-1, -1, -1]),
        true,
        "exit-1",
    )?;
    exit.advance(0, 3)?;
    exit.publish(
        &mut shell,
        &authority,
        true,
        prepare::MAX_PUBLICATION_BYTES,
        None,
    )?;
    while checked(shell.deliver_next_memory(&mut destination.memory()))? {}
    let snapshot = checked(shell.snapshot())?;
    if (
        decode_state(snapshot.state())?.count.0,
        snapshot.version(),
        snapshot.bundle_count(),
        snapshot.replay_count(),
        snapshot.pending(),
        destination.delivered_count(),
    ) != (0, 2, 2, 2, 0, 2)
    {
        return Err("complete lifecycle differs from expected result".into());
    }
    Ok(format!(
        "{{\"status\":\"passed\",\"count\":0,\"bundles\":2,\"deliveries\":2,\"pending\":0,\"publication_bytes\":{}}}",
        checked(publication.sizes.total())?
    ))
}
