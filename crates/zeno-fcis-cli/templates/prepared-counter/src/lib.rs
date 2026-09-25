#![forbid(unsafe_code)]

pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/counter.rs"));
}
pub mod bindings {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/rust/project.rs"));
}
pub mod delivery;
pub mod laws;
#[cfg(feature = "sqlite")]
pub mod prepare;
#[path = "../profile.rs"]
pub mod profile;
pub mod program;

use bindings::GeneratedProject;
use delivery::Destination;
use generated::{CounterState, CounterValue};
use laws::{CounterLaws, NoExternalProofs};
use program::CounterProgram;
#[cfg(feature = "sqlite")]
use std::path::Path;
use zeno_fcis_authority::{
    CatalogCommitAuthority, ExecutionBinding, GenesisPolicyBinding, StateDomainBinding,
};
use zeno_fcis_codec::Domain;
use zeno_fcis_crypto::{RustCryptoSha256, verify_approved_provider};
use zeno_fcis_laws::{LawLimits, verify_project_laws};
use zeno_fcis_patch::hash_value;
use zeno_fcis_schema::ValidationLimits;
#[cfg(feature = "sqlite")]
use zeno_fcis_shell::IdempotentDestination;
#[cfg(feature = "sqlite")]
use zeno_fcis_shell_sqlite::SqliteShell;
use zeno_fcis_transition::TransitionLimits;

pub type Authority =
    CatalogCommitAuthority<RustCryptoSha256, CounterProgram, CounterLaws, Destination>;
#[cfg(feature = "sqlite")]
pub type Shell = SqliteShell<CounterProgram, CounterLaws, Destination>;
pub type AppResult<T> = Result<T, String>;

fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> AppResult<T> {
    value.map_err(|error| format!("{error:?}"))
}

pub fn authority() -> AppResult<Authority> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(project.admit_root::<RustCryptoSha256>(
        &CounterState {
            count: CounterValue(0),
        },
        ValidationLimits::default(),
    ))?;
    let domain = checked(Domain::new("example/prepared-counter/state", 1))?;
    let initial_root = checked(hash_value::<RustCryptoSha256>(
        domain,
        initial.value().value(),
    ))?;
    let laws = checked(verify_project_laws::<RustCryptoSha256, _, _>(
        project.catalog(),
        profile::manifest(),
        profile::source_hash(),
        vec![],
        LawLimits::default(),
        profile::checker_hash(),
        CounterLaws::default(),
        &NoExternalProofs,
    ))?;
    let label =
        |name: &str| profile::digest("example/prepared-counter/local-policy", name.as_bytes());
    checked(CatalogCommitAuthority::try_new(
        project.catalog(),
        checked(StateDomainBinding::try_new(
            "example/prepared-counter/state",
            1,
        ))?,
        checked(ExecutionBinding::try_new(
            profile::program_hash(),
            label("RustCrypto SHA-256 with library known-answer admission; no build attestation"),
            label("MemoryDestination exact ID and entry hash"),
            label("local tutorial deployment"),
            label("exact invocation replay ID and complete bundle"),
        ))?,
        checked(GenesisPolicyBinding::try_new(
            initial_root,
            profile::source_hash(),
            label("count=0; bounds=0..3"),
            label("runtime-checked genesis; no external proof"),
            label("local tutorial deployment"),
        ))?,
        TransitionLimits::default(),
        &checked(verify_approved_provider::<RustCryptoSha256>())?,
        laws,
        CounterProgram,
    ))
}

/// Creates the exact reviewed genesis. An existing database is rejected by the shell.
#[cfg(feature = "sqlite")]
pub fn create(path: &Path, authority: &Authority, destination: Destination) -> AppResult<Shell> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(project.admit_root::<RustCryptoSha256>(
        &CounterState {
            count: CounterValue(0),
        },
        ValidationLimits::default(),
    ))?;
    let genesis = checked(authority.authorize_genesis(initial))?;
    checked(Shell::create(
        path,
        authority,
        genesis,
        authority.bind_delivery_interpreter(destination),
    ))
}

/// Runs bounded preparation, exact replay, restart and interrupted delivery.
#[cfg(feature = "sqlite")]
pub fn journey(path: &Path) -> AppResult<String> {
    use prepare::{MAX_PUBLICATION_BYTES, PreparedBatch, command};
    use zeno_fcis_authority::AuthorizationDecodeLimits;
    use zeno_fcis_shell::CommitStatus;

    let authority = authority()?;
    let mut destination = Destination::default();
    let mut shell = create(path, &authority, destination.clone())?;
    let before = checked(shell.snapshot())?;
    let mut batch =
        PreparedBatch::start(&authority, &before, &command([1, 1, 1]), true, "batch-1")?;
    batch.advance(0, 1)?;
    if checked(shell.snapshot())? != before {
        return Err("partial work published data".into());
    }
    batch.advance(1, 2)?;
    let publication = batch.publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)?;
    if publication.status != CommitStatus::Committed {
        return Err("first publication was not committed".into());
    }
    let replay = checked(authority.reauthorize_canonical_transition(
        &publication.authorization,
        AuthorizationDecodeLimits::default(),
    ))?;
    if checked(shell.commit(replay))? != CommitStatus::IdempotentReplay {
        return Err("exact replay was not idempotent".into());
    }
    let pending = checked(shell.next_pending())?.ok_or("missing notification")?;
    checked(destination.deliver(pending.delivery_id(), pending.entry_hash(), pending.entry()))?;
    drop(shell);
    let mut shell = checked(Shell::open_existing(
        path,
        &authority,
        authority.bind_delivery_interpreter(destination.clone()),
    ))?;
    while checked(shell.deliver_next())? {}
    let mut exit = PreparedBatch::start(
        &authority,
        &checked(shell.snapshot())?,
        &command([-1, -1, -1]),
        true,
        "exit-1",
    )?;
    exit.advance(0, 3)?;
    exit.publish(&mut shell, &authority, true, MAX_PUBLICATION_BYTES, None)?;
    while checked(shell.deliver_next())? {}
    let snapshot = checked(shell.snapshot())?;
    let state = checked(CounterState::try_from_value(snapshot.state().clone()))?;
    if (
        state.count.0,
        snapshot.version(),
        snapshot.bundle_count(),
        snapshot.replay_count(),
        snapshot.pending_outbox(),
        destination.delivered_count(),
    ) != (0, 2, 2, 2, 0, 2)
    {
        return Err("complete lifecycle differs from expected result".into());
    }
    Ok(format!(
        "{{\"status\":\"passed\",\"count\":0,\"bundles\":2,\"deliveries\":2,\"pending\":0,\"publication_bytes\":{}}}",
        publication.sizes.total()
    ))
}
