#![forbid(unsafe_code)]

pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/vault.rs"));
}
pub mod bindings {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/rust/project.rs"));
}
pub mod controller;
pub mod delivery;
pub mod laws;
#[path = "../profile.rs"]
pub mod profile;
pub mod program;
#[path = "../synthesized/transition.rs"]
pub mod synthesized;

use bindings::GeneratedProject;
use delivery::Destination;
use generated::{
    Amount, Flag, Lane, LaneStatus, Money, PauseTicks, Vault, VaultAction, VaultCommand,
};
#[cfg(feature = "sqlite")]
use generated::{Caller, TickContext};
use laws::{NoExternalProofs, VaultLaws};
use program::VaultProgram;
#[cfg(feature = "sqlite")]
use std::path::Path;
#[cfg(feature = "sqlite")]
use zeno_fcis_authority::AuthorizationDecodeLimits;
use zeno_fcis_authority::{
    CatalogCommitAuthority, ExecutionBinding, GenesisPolicyBinding, StateDomainBinding,
};
#[cfg(feature = "sqlite")]
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_codec::Domain;
#[cfg(feature = "sqlite")]
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::{RustCryptoSha256, verify_approved_provider};
use zeno_fcis_laws::{LawLimits, verify_project_laws};
use zeno_fcis_patch::hash_value;
use zeno_fcis_schema::ValidationLimits;
#[cfg(feature = "sqlite")]
use zeno_fcis_shell::{CommitStatus, IdempotentDestination};
#[cfg(feature = "sqlite")]
use zeno_fcis_shell_sqlite::SqliteShell;
use zeno_fcis_transition::TransitionLimits;

pub type Authority = CatalogCommitAuthority<RustCryptoSha256, VaultProgram, VaultLaws, Destination>;
#[cfg(feature = "sqlite")]
pub type Shell = SqliteShell<VaultProgram, VaultLaws, Destination>;
pub type AppResult<T> = Result<T, String>;

fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> AppResult<T> {
    value.map_err(|error| format!("{error:?}"))
}

/// The vault before any request: no funds, both lanes empty, no pause, lane A
/// has priority. This is the checked controller's initial state.
#[must_use]
pub fn genesis_state() -> Vault {
    Vault {
        balance: Money(0),
        lane_a: LaneStatus::Empty,
        amount_a: Money(0),
        lane_b: LaneStatus::Empty,
        amount_b: Money(0),
        pause: PauseTicks(0),
        must_serve: Flag(false),
        priority: Lane::A,
    }
}

/// One command. A deposit reads only `amount`, a withdrawal request reads
/// `lane` and `amount`, and a tick reads neither.
#[must_use]
pub fn command(action: VaultAction, lane: Lane, amount: i128) -> VaultCommand {
    VaultCommand {
        action,
        lane,
        amount: Amount(amount),
    }
}

#[must_use]
pub fn deposit(amount: i128) -> VaultCommand {
    command(VaultAction::Deposit, Lane::A, amount)
}

#[must_use]
pub fn request(lane: Lane, amount: i128) -> VaultCommand {
    command(VaultAction::RequestWithdrawal, lane, amount)
}

#[must_use]
pub fn tick() -> VaultCommand {
    command(VaultAction::Tick, Lane::A, 1)
}

/// The commit authority for this vault, bound to the exact source.
///
/// # Errors
///
/// A reviewed binding, the law manifest, or the hash provider that the
/// library refuses; the message names the refusal.
pub fn authority() -> AppResult<Authority> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(
        project.admit_root::<RustCryptoSha256>(&genesis_state(), ValidationLimits::default()),
    )?;
    let domain = checked(Domain::new("example/withdrawal-queue/state", 1))?;
    let initial_root = checked(hash_value::<RustCryptoSha256>(
        domain,
        initial.value().value(),
    ))?;
    let laws = checked(verify_project_laws::<RustCryptoSha256, _, _>(
        project.catalog(),
        checked(profile::manifest())?,
        checked(profile::source_hash())?,
        vec![],
        LawLimits::default(),
        checked(profile::checker_hash())?,
        checked(VaultLaws::try_new())?,
        &checked(NoExternalProofs::try_new())?,
    ))?;
    let label =
        |name: &str| profile::digest("example/withdrawal-queue/local-policy", name.as_bytes());
    checked(CatalogCommitAuthority::try_new(
        project.catalog(),
        checked(StateDomainBinding::try_new(
            "example/withdrawal-queue/state",
            1,
        ))?,
        checked(ExecutionBinding::try_new(
            checked(profile::program_hash())?,
            checked(label(
                "RustCrypto SHA-256 with library known-answer admission; no build attestation",
            ))?,
            checked(label("MemoryDestination exact ID and entry hash"))?,
            checked(label("local tutorial deployment"))?,
            checked(label("exact invocation replay ID and complete bundle"))?,
        ))?,
        checked(GenesisPolicyBinding::try_new(
            initial_root,
            checked(profile::source_hash())?,
            checked(label(
                "balance=0; lanes empty; pause=0; must_serve=false; priority=A",
            ))?,
            checked(label("runtime-checked genesis; no external proof"))?,
            checked(label("local tutorial deployment"))?,
        ))?,
        TransitionLimits::default(),
        &checked(verify_approved_provider::<RustCryptoSha256>())?,
        laws,
        checked(VaultProgram::try_new())?,
    ))
}

/// Creates the exact reviewed genesis. An existing database is rejected by the shell.
///
/// # Errors
///
/// A database that already exists, or a genesis the authority refuses.
#[cfg(feature = "sqlite")]
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

/// Decides one command and, if it commits, publishes it and checks its exact replay.
///
/// Local tutorial inputs: a deployment must authenticate the caller, and must
/// take the alarm from a source it trusts, before admission.
///
/// # Errors
///
/// A command or state the schema refuses, a decision the law checker refuses,
/// or a publication the shell refuses; the message names the refusal.
#[cfg(feature = "sqlite")]
pub fn invoke(
    shell: &mut Shell,
    authority: &Authority,
    command: VaultCommand,
    caller: Caller,
    alarm: bool,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let pre = checked(Vault::try_from_value(snapshot.state().clone()))?;
    let pre = checked(project.admit_root::<RustCryptoSha256>(&pre, ValidationLimits::default()))?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default()))?;
    let context = checked(project.admit_context::<RustCryptoSha256>(
        &TickContext {
            caller,
            alarm: Flag(alarm),
        },
        ValidationLimits::default(),
    ))?;
    let witness = checked(authority.admit_invocation(
        pre,
        command.admitted().clone(),
        context.admitted().clone(),
        checked(profile::digest(
            "example/withdrawal-queue/principal",
            b"local tutorial caller",
        ))?,
        checked(profile::digest(
            "example/withdrawal-queue/authentication",
            b"trusted local input; no remote authentication",
        ))?,
        checked(profile::digest(
            "example/withdrawal-queue/replay",
            replay.as_bytes(),
        ))?,
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

/// Deposits four units, meets every rejection reason, and requests a
/// withdrawal on each lane. Then the keeper ticks eight times while the alarm
/// stays raised for most of them: the alarm pauses payouts, the pause ends,
/// must-serve pays lane A, a second alarm pauses again, and must-serve pays
/// lane B on the eighth tick, the checked bound. It then interrupts payout
/// delivery, reopens the database, finishes delivery, and checks that the
/// balance equals the deposits minus the payouts.
///
/// Returns a JSON summary.
///
/// # Errors
///
/// Any step whose outcome differs from the story above, or a refusal from
/// `invoke`, `create`, or the shell.
#[cfg(feature = "sqlite")]
pub fn journey(path: &Path) -> AppResult<String> {
    let authority = authority()?;
    let mut destination = Destination::default();
    let mut shell = create(path, &authority, destination.clone())?;
    let steps = [
        (deposit(2), Caller::Operator, false, "Accept"),
        (deposit(2), Caller::Operator, false, "Accept"),
        (deposit(1), Caller::Operator, false, "Reject"),
        (request(Lane::A, 2), Caller::OwnerB, false, "Reject"),
        (request(Lane::A, 2), Caller::OwnerA, false, "Accept"),
        (request(Lane::A, 1), Caller::OwnerA, false, "Reject"),
        (request(Lane::B, 2), Caller::OwnerB, false, "Accept"),
        (tick(), Caller::Keeper, true, "Accept"),
        (tick(), Caller::Keeper, true, "Accept"),
        (tick(), Caller::Keeper, true, "Accept"),
        (tick(), Caller::Keeper, true, "Accept"),
        (tick(), Caller::Keeper, true, "Accept"),
        (tick(), Caller::Keeper, false, "Accept"),
        (tick(), Caller::Keeper, false, "Accept"),
        (tick(), Caller::Keeper, true, "Accept"),
        (request(Lane::A, 1), Caller::OwnerA, false, "Reject"),
    ];
    let (mut deposited, mut ticks) = (0, 0);
    let mut outcomes = Vec::new();
    let mut payout_ticks = Vec::new();
    for (step, (command, caller, alarm, expected)) in steps.into_iter().enumerate() {
        let before = checked(shell.snapshot())?;
        let is_deposit = command.action == VaultAction::Deposit;
        let is_tick = command.action == VaultAction::Tick;
        let amount = command.amount.0;
        let outcome = invoke(
            &mut shell,
            &authority,
            command,
            caller,
            alarm,
            &format!("journey-{step}"),
        )?;
        if outcome != expected {
            return Err(format!("step {step}: expected {expected}, got {outcome}"));
        }
        let after = checked(shell.snapshot())?;
        if outcome == "Reject" && after != before {
            return Err(format!("step {step}: a rejection published data"));
        }
        if outcome == "Accept" && is_deposit {
            deposited += amount;
        }
        if outcome == "Accept" && is_tick {
            ticks += 1;
            if after.pending_outbox() > before.pending_outbox() {
                payout_ticks.push(ticks);
            }
        }
        outcomes.push(format!("\"{outcome}\""));
    }
    let pending = checked(shell.next_pending())?.ok_or("missing pending payout")?;
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
    let state = checked(Vault::try_from_value(snapshot.state().clone()))?;
    let paid = deposited - state.balance.0;
    let summary = (
        state.balance.0,
        snapshot.bundle_count(),
        snapshot.pending_outbox(),
        destination.delivered_count(),
    );
    if state != genesis_state()
        || summary != (0, 12, 0, 2)
        || (deposited, paid) != (4, 4)
        || payout_ticks != [4, 8]
    {
        return Err(format!(
            "lifecycle differs from the expected result: {state:?} {summary:?} {payout_ticks:?}"
        ));
    }
    Ok(format!(
        "{{\"status\":\"passed\",\"decisions\":[{}],\"balance\":{},\"deposited\":{deposited},\"paid\":{paid},\"payout_ticks\":[{}],\"bundles\":{},\"pending\":{},\"deliveries\":{}}}",
        outcomes.join(","),
        summary.0,
        payout_ticks
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(","),
        summary.1,
        summary.2,
        summary.3
    ))
}
