#![forbid(unsafe_code)]

/// Complete original-domain policy and checked library Authority construction.
pub mod v2_contract;

pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/account.rs"));
}
pub mod bindings {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/rust/project.rs"));
}
pub mod delivery;
#[path = "../profile.rs"]
pub mod profile;

#[cfg(feature = "sqlite")]
use bindings::GeneratedProject;
#[cfg(feature = "sqlite")]
use delivery::Destination;
#[cfg(feature = "sqlite")]
use generated::AccountCommand;
use generated::{Account, AdminFlag, Attempts, LockDeadline, RequestContext, UnixTime};
#[cfg(feature = "sqlite")]
use std::path::Path;
#[cfg(feature = "sqlite")]
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, decode_envelope};
#[cfg(feature = "sqlite")]
use zeno_fcis_crypto::RustCryptoSha256;
#[cfg(feature = "sqlite")]
use zeno_fcis_schema::ValidationLimits;
#[cfg(feature = "sqlite")]
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_synthesis::finite::v2_composition as composition;
#[cfg(feature = "sqlite")]
use zeno_fcis_synthesis::finite::{v2_authority::PublicationOutcome, v2_composition::Class};

/// Library authority borrowing the exact checked policy descriptor.
pub type Authority<'p> = zeno_fcis_synthesis::finite::v2_authority::Authority<'p>;
/// Persistent shell consuming genuine library publication capabilities.
#[cfg(feature = "sqlite")]
pub type Shell<'a, 'p> = zeno_fcis_shell_sqlite::v2::V2SqliteShell<'a, 'p>;
pub type AppResult<T> = Result<T, String>;

#[cfg(feature = "sqlite")]
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

/// Bind only the checked complete original schema and retained reviewed policy.
/// The caller owns Contract, then Descriptor, then this borrowing Authority.
///
/// # Errors
/// Returns the actual catalog or authority refusal.
pub fn authority<'p>(descriptor: &'p composition::Descriptor<'p>) -> AppResult<Authority<'p>> {
    v2_contract::checked_authority(descriptor).map_err(|error| match error {
        v2_contract::BindFailure::Catalog(error) => format!("catalog: {error:?}"),
        v2_contract::BindFailure::Authority(error) => format!("authority: {error:?}"),
    })
}

/// Publish the actual reviewed initial state through genuine Genesis.
///
/// # Errors
/// Returns schema/framing/genesis or persistent-shell refusal. Existing history is refused.
#[cfg(feature = "sqlite")]
pub fn create<'a, 'p>(path: &Path, authority: &'a Authority<'p>) -> AppResult<Shell<'a, 'p>> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(
        project.admit_root::<RustCryptoSha256>(&genesis_state(), ValidationLimits::default()),
    )?;
    let initial = checked(initial.envelope().canonical_bytes())?;
    let genesis = match authority.publish_genesis(&initial) {
        PublicationOutcome::Commit(genesis) => genesis,
        PublicationOutcome::Refused { evaluation, error } => {
            return Err(format!("genesis: {error:?}; evaluation: {evaluation:?}"));
        }
        other => return Err(format!("unsupported genesis outcome: {other:?}")),
    };
    checked(Shell::create(path, authority, genesis))
}

/// Decide exact original inputs, publish a genuine capability, and recompute its exact replay.
/// Local principal/context authentication and input framing are host assumptions.
///
/// # Errors
/// Returns schema admission, actual technical refusal, or shell/replay failure.
#[cfg(feature = "sqlite")]
pub fn invoke(
    shell: &mut Shell<'_, '_>,
    authority: &Authority<'_>,
    command: AccountCommand,
    context: RequestContext,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default()))?;
    let context =
        checked(project.admit_context::<RustCryptoSha256>(&context, ValidationLimits::default()))?;
    let command = checked(command.admitted().envelope().canonical_bytes())?;
    let context = checked(context.admitted().envelope().canonical_bytes())?;
    let original = composition::Raw {
        state: snapshot.state(),
        command: &command,
        context: &context,
    };
    let publication = match authority.publish(original) {
        PublicationOutcome::Commit(publication) => publication,
        PublicationOutcome::Reject(_) => return Ok("Reject"),
        PublicationOutcome::Refused { evaluation, error } => {
            return Err(format!(
                "publication: {error:?}; evaluation: {evaluation:?}"
            ));
        }
        other => return Err(format!("unsupported publication outcome: {other:?}")),
    };
    let outcome = match checked(publication.evaluation().result())?.class() {
        Class::Accept => "Accept",
        Class::CommittedFailure => "CommittedFailure",
        Class::Reject => return Err("reject yielded a publication capability".into()),
        other => return Err(format!("unsupported decision class: {other:?}")),
    };
    let subject = publication.subject().to_vec();
    let replay_id = profile::digest("example/account-lockout/replay", replay.as_bytes());
    if checked(shell.commit(replay_id, publication))?.status() != CommitStatus::Committed {
        return Err("expected first publication".into());
    }
    let replayed = match authority.replay_publication(original, &subject) {
        PublicationOutcome::Commit(publication) => publication,
        other => return Err(format!("exact publication replay refused: {other:?}")),
    };
    if checked(shell.commit(replay_id, replayed))?.status() != CommitStatus::IdempotentReplay {
        return Err("exact replay was not idempotent".into());
    }
    Ok(outcome)
}

/// Decode an already checked snapshot only for typed host presentation.
/// The decoded Value is never supplied to core evaluation.
///
/// # Errors
/// Returns malformed envelope or typed presentation failure.
#[cfg(feature = "sqlite")]
pub fn decode_state(original: &[u8]) -> AppResult<Account> {
    let envelope = checked(decode_envelope(original, DecodeLimits::default()))?;
    checked(Account::try_from_value(envelope.into_value()))
}

/// Locks the account with three failed logins, refuses logins during the lock,
/// refuses a stale clock and a non-administrator unlock, lets a login through
/// when the lock expires, and unlocks through an administrator. It then
/// interrupts alert delivery, reopens the database, and finishes delivery.
///
/// Returns a JSON summary.
#[cfg(feature = "sqlite")]
pub fn journey(path: &Path) -> AppResult<String> {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let destination = Destination::default();
    let mut shell = create(path, &authority)?;
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
    let delivered = checked(shell.deliver_next_memory_unacknowledged(&mut destination.memory()))?
        .ok_or("missing pending delivery")?;
    if delivered != (pending.delivery_id(), pending.entry_hash()) {
        return Err("destination observed a different pending entry".into());
    }
    drop(shell);
    let mut shell = checked(Shell::open(path, &authority))?;
    while checked(shell.deliver_next_memory(&mut destination.memory()))? {}
    checked(shell.acknowledge(pending.delivery_id(), pending.entry_hash()))?;
    let snapshot = checked(shell.snapshot())?;
    let state = decode_state(snapshot.state())?;
    let summary = (
        state.failed_attempts.0,
        state.locked_until.0,
        state.last_seen.0,
        snapshot.version(),
        snapshot.pending(),
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
