#![forbid(unsafe_code)]

pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/account.rs"));
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
use generated::{
    Account, AccountCommand, AdminFlag, Attempts, LockDeadline, RequestContext, UnixTime,
};
use laws::{AccountLaws, NoExternalProofs};
use program::AccountProgram;
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
    CatalogCommitAuthority<RustCryptoSha256, AccountProgram, AccountLaws, Destination>;
pub type Shell = SqliteShell<AccountProgram, AccountLaws, Destination>;
pub type AppResult<T> = Result<T, String>;

fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> AppResult<T> {
    value.map_err(|error| format!("{error:?}"))
}

/// The account before any request: no failures, no lock, time zero.
pub fn genesis_state() -> Account {
    Account {
        failed_attempts: Attempts(0),
        locked_until: LockDeadline(0),
        last_seen: UnixTime(0),
    }
}

/// The context a request carries: the current time and the administrator flag.
pub fn context(now: i128, admin: bool) -> RequestContext {
    RequestContext {
        now: UnixTime(now),
        admin: AdminFlag(admin),
    }
}

pub fn authority() -> AppResult<Authority> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(
        project.admit_root::<RustCryptoSha256>(&genesis_state(), ValidationLimits::default()),
    )?;
    let domain = checked(Domain::new("example/account-lockout/state", 1))?;
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
        AccountLaws::default(),
        &NoExternalProofs,
    ))?;
    let label =
        |name: &str| profile::digest("example/account-lockout/local-policy", name.as_bytes());
    checked(CatalogCommitAuthority::try_new(
        project.catalog(),
        checked(StateDomainBinding::try_new(
            "example/account-lockout/state",
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
            label("failed_attempts=0; locked_until=0; last_seen=0"),
            label("runtime-checked genesis; no external proof"),
            label("local tutorial deployment"),
        ))?,
        TransitionLimits::default(),
        &checked(verify_approved_provider::<RustCryptoSha256>())?,
        laws,
        AccountProgram,
    ))
}

/// Creates the exact reviewed genesis. An existing database is rejected by the shell.
pub fn create(path: &Path, authority: &Authority, destination: Destination) -> AppResult<Shell> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(
        project.admit_root::<RustCryptoSha256>(&genesis_state(), ValidationLimits::default()),
    )?;
    let genesis = checked(authority.authorize_genesis(initial))?;
    checked(Shell::create(
        path,
        authority,
        genesis,
        authority.bind_delivery_interpreter(destination),
    ))
}

/// Decides one request and, if it commits, publishes it and checks its exact replay.
///
/// Local tutorial inputs: a deployment must authenticate the principal and the
/// context, including the time and the administrator flag, before admission.
pub fn invoke(
    shell: &mut Shell,
    authority: &Authority,
    command: AccountCommand,
    context: RequestContext,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let pre = checked(Account::try_from_value(snapshot.state().clone()))?;
    let pre = checked(project.admit_root::<RustCryptoSha256>(&pre, ValidationLimits::default()))?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default()))?;
    let context =
        checked(project.admit_context::<RustCryptoSha256>(&context, ValidationLimits::default()))?;
    let witness = checked(authority.admit_invocation(
        pre,
        command.admitted().clone(),
        context.admitted().clone(),
        profile::digest(
            "example/account-lockout/principal",
            b"local tutorial account holder",
        ),
        profile::digest(
            "example/account-lockout/authentication",
            b"trusted local input; no remote authentication",
        ),
        profile::digest("example/account-lockout/replay", replay.as_bytes()),
    ))?;
    let (candidate, outcome) = match checked(authority.execute(witness))? {
        Decision::Accept(accept) => (accept.into_candidate(), "Accept"),
        Decision::CommittedFailure(failure) => (failure.into_parts().0, "CommittedFailure"),
        Decision::Reject(_) => return Ok("Reject"),
    };
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

/// Locks the account with three failed logins, refuses logins during the lock,
/// refuses a stale clock and a non-administrator unlock, lets a login through
/// when the lock expires, and unlocks through an administrator. It then
/// interrupts alert delivery, reopens the database, and finishes delivery.
///
/// Returns a JSON summary.
pub fn journey(path: &Path) -> AppResult<String> {
    let authority = authority()?;
    let mut destination = Destination::default();
    let mut shell = create(path, &authority, destination.clone())?;
    let steps = [
        (AccountCommand::LoginFailed, 1000, false, "CommittedFailure"),
        (AccountCommand::LoginFailed, 1010, false, "CommittedFailure"),
        (AccountCommand::LoginFailed, 1020, false, "CommittedFailure"),
        (AccountCommand::LoginSucceeded, 1500, false, "Reject"),
        (AccountCommand::LoginSucceeded, 1000, false, "Reject"),
        (AccountCommand::AdminUnlock, 1600, false, "Reject"),
        (AccountCommand::LoginSucceeded, 1920, false, "Accept"),
        (AccountCommand::LoginFailed, 2000, false, "CommittedFailure"),
        (AccountCommand::AdminUnlock, 2010, true, "Accept"),
    ];
    let mut outcomes = Vec::new();
    for (step, (command, now, admin, expected)) in steps.into_iter().enumerate() {
        let before = checked(shell.snapshot())?;
        let outcome = invoke(
            &mut shell,
            &authority,
            command,
            context(now, admin),
            &format!("journey-{step}"),
        )?;
        if outcome != expected {
            return Err(format!("step {step}: expected {expected}, got {outcome}"));
        }
        if outcome == "Reject" && checked(shell.snapshot())? != before {
            return Err(format!("step {step}: a rejection published data"));
        }
        outcomes.push(format!("\"{outcome}\""));
    }
    let pending = checked(shell.next_pending())?.ok_or("missing pending alert")?;
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
    let state = checked(Account::try_from_value(snapshot.state().clone()))?;
    let summary = (
        state.failed_attempts.0,
        state.locked_until.0,
        state.last_seen.0,
        snapshot.bundle_count(),
        snapshot.pending_outbox(),
        destination.delivered_count(),
    );
    if summary != (0, 0, 2010, 6, 0, 2) {
        return Err(format!(
            "lifecycle differs from the expected result: {summary:?}"
        ));
    }
    Ok(format!(
        "{{\"status\":\"passed\",\"decisions\":[{}],\"failed_attempts\":{},\"locked_until\":{},\"last_seen\":{},\"bundles\":{},\"pending\":{},\"deliveries\":{}}}",
        outcomes.join(","),
        summary.0,
        summary.1,
        summary.2,
        summary.3,
        summary.4,
        summary.5
    ))
}
