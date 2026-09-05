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
#[path = "../profile.rs"]
pub mod profile;
pub mod program;

use bindings::GeneratedProject;
use delivery::Destination;
use generated::{CounterCommand, CounterContext, CounterState, CounterValue};
use laws::{CounterLaws, NoExternalProofs};
use program::CounterProgram;
use std::path::Path;
use zeno_fcis_authority::{
    AuthorizationDecodeLimits, CatalogCommitAuthority, ExecutionBinding, GenesisPolicyBinding,
    StateDomainBinding,
};
use zeno_fcis_codec::{CanonicalEncode, Domain};
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::{RustCryptoSha256, verify_approved_provider};
use zeno_fcis_laws::{LawLimits, verify_project_laws};
use zeno_fcis_patch::hash_value;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::{IdempotentDestination, SqliteShell};
use zeno_fcis_transition::TransitionLimits;

pub type Authority =
    CatalogCommitAuthority<RustCryptoSha256, CounterProgram, CounterLaws, Destination>;
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
            failures: CounterValue(0),
        },
        ValidationLimits::default(),
    ))?;
    let domain = checked(Domain::new("example/durable-counter/state", 1))?;
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
        |name: &str| profile::digest("example/durable-counter/local-policy", name.as_bytes());
    checked(CatalogCommitAuthority::try_new(
        project.catalog(),
        checked(StateDomainBinding::try_new(
            "example/durable-counter/state",
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
            label("count=0; failures=0; bounds=0..3"),
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
pub fn create(path: &Path, authority: &Authority, destination: Destination) -> AppResult<Shell> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(project.admit_root::<RustCryptoSha256>(
        &CounterState {
            count: CounterValue(0),
            failures: CounterValue(0),
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

/// Local tutorial inputs. A deployment must authenticate principal and context before admission.
pub fn invoke(
    shell: &mut Shell,
    authority: &Authority,
    command: CounterCommand,
    allowed: bool,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let pre = checked(CounterState::try_from_value(snapshot.state().clone()))?;
    let pre = checked(project.admit_root::<RustCryptoSha256>(&pre, ValidationLimits::default()))?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default()))?;
    let context =
        checked(project.admit_context::<RustCryptoSha256>(
            &CounterContext(allowed),
            ValidationLimits::default(),
        ))?;
    let witness = checked(authority.admit_invocation(
        pre,
        command.admitted().clone(),
        context.admitted().clone(),
        profile::digest(
            "example/durable-counter/principal",
            b"local tutorial operator",
        ),
        profile::digest(
            "example/durable-counter/authentication",
            b"trusted local input; no remote authentication",
        ),
        profile::digest("example/durable-counter/replay", replay.as_bytes()),
    ))?;
    let (candidate, outcome) = match checked(authority.execute(witness))? {
        Decision::Accept(accept) => (accept.into_candidate(), "Accept"),
        Decision::CommittedFailure(failure) => (failure.into_parts().0, "CommittedFailure"),
        Decision::Reject(_) => return Ok("Reject"),
    };
    // Reconstruct the same exact invocation, rather than assigning an old replay ID to new input.
    let bytes = checked(candidate.canonical_bytes())?;
    if checked(shell.commit(candidate))? != CommitStatus::Committed {
        return Err("expected first publication".into());
    }
    let replayed = checked(
        authority.reauthorize_canonical_transition(&bytes, AuthorizationDecodeLimits::default()),
    )?;
    if checked(shell.commit(replayed))? != CommitStatus::IdempotentReplay {
        return Err("exact replay was not idempotent".into());
    }
    Ok(outcome)
}

/// Exercises commit, rejected input, intentional committing failure, restart, and delivery retry.
pub fn journey(path: &Path) -> AppResult<String> {
    let authority = authority()?;
    let mut destination = Destination::default();
    let mut shell = create(path, &authority, destination.clone())?;
    if invoke(
        &mut shell,
        &authority,
        CounterCommand::Increment,
        true,
        "increment-1",
    )? != "Accept"
    {
        return Err("accept journey".into());
    }
    let before = checked(shell.snapshot())?;
    if invoke(
        &mut shell,
        &authority,
        CounterCommand::Increment,
        false,
        "denied-1",
    )? != "Reject"
    {
        return Err("reject journey".into());
    }
    let after = checked(shell.snapshot())?;
    if before != after {
        return Err("rejection published state, replay or outbox data".into());
    }
    if invoke(
        &mut shell,
        &authority,
        CounterCommand::RecordFailure,
        true,
        "failure-1",
    )? != "CommittedFailure"
    {
        return Err("failure journey".into());
    }
    let pending = checked(shell.next_pending())?.ok_or("missing pending delivery")?;
    // Model interruption after the destination records delivery but before SQLite acknowledges it.
    checked(destination.deliver(pending.delivery_id(), pending.entry_hash(), pending.entry()))?;
    drop(shell);
    let mut shell = checked(Shell::open_existing(
        path,
        &authority,
        authority.bind_delivery_interpreter(destination.clone()),
    ))?;
    while checked(shell.deliver_next())? {}
    checked(shell.acknowledge(pending.delivery_id(), pending.entry_hash()))?;
    let snapshot = checked(shell.snapshot())?;
    let state = checked(CounterState::try_from_value(snapshot.state().clone()))?;
    if (
        state.count.0,
        state.failures.0,
        snapshot.bundle_count(),
        snapshot.replay_count(),
        snapshot.pending_outbox(),
        destination.delivered_count(),
    ) != (1, 1, 2, 2, 0, 2)
    {
        return Err("lifecycle differs from the independent expected result".into());
    }
    Ok("Accept, Reject, CommittedFailure; replay and restart checked; count=1 failures=1 bundles=2 deliveries=2 pending=0".into())
}
