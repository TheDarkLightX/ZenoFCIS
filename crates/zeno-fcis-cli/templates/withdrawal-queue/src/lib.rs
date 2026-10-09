#![forbid(unsafe_code)]

/// Complete original-domain policy and checked library Authority construction.
pub mod v2_contract;

pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/vault.rs"));
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
use generated::{
    Amount, Flag, Lane, LaneStatus, Money, PauseTicks, Vault, VaultAction, VaultCommand,
};
#[cfg(feature = "sqlite")]
use generated::{Caller, TickContext};
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
#[cfg(feature = "sqlite")]
use zeno_fcis_shell_sqlite::v2::relay;
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
    command: VaultCommand,
    caller: Caller,
    alarm: bool,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default()))?;
    let context = checked(project.admit_context::<RustCryptoSha256>(
        &TickContext {
            caller,
            alarm: Flag(alarm),
        },
        ValidationLimits::default(),
    ))?;
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
    let replay_id = checked(profile::digest(
        "example/withdrawal-queue/replay",
        replay.as_bytes(),
    ))?;
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
pub fn decode_state(original: &[u8]) -> AppResult<Vault> {
    let envelope = checked(decode_envelope(original, DecodeLimits::default()))?;
    checked(Vault::try_from_value(envelope.into_value()))
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
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let destination = Destination::default();
    let mut shell = create(path, &authority)?;
    let Decided {
        outcomes,
        deposited,
        payout_ticks,
    } = decide_journey(&mut shell, &authority)?;
    let pending = checked(shell.next_pending())?.ok_or("missing pending payout")?;
    // Model interruption after the destination records delivery but before SQLite acknowledges it.
    let delivered = checked(pending.deliver(&mut destination.memory()))?;
    drop(delivered);
    drop(shell);
    let mut shell = checked(Shell::open(path, &authority))?;
    while let Some(pending) = checked(shell.next_pending())? {
        let delivered = checked(pending.deliver(&mut destination.memory()))?;
        checked(delivered.acknowledge())?;
    }
    let snapshot = checked(shell.snapshot())?;
    let state = decode_state(snapshot.state())?;
    let paid = deposited - state.balance.0;
    let summary = (
        state.balance.0,
        snapshot.version(),
        snapshot.pending(),
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

/// The journey's decisions before delivery: the accepted ones, the
/// deposits they made, and the ticks that paid out.
#[cfg(feature = "sqlite")]
struct Decided {
    outcomes: Vec<String>,
    deposited: i128,
    payout_ticks: Vec<i128>,
}

/// Runs the journey's sixteen decisions on a new store, leaving its two
/// payouts pending.
#[cfg(feature = "sqlite")]
fn decide_journey(shell: &mut Shell<'_, '_>, authority: &Authority<'_>) -> AppResult<Decided> {
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
            shell,
            authority,
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
            if after.pending() > before.pending() {
                payout_ticks.push(ticks);
            }
        }
        outcomes.push(format!("\"{outcome}\""));
    }
    Ok(Decided {
        outcomes,
        deposited,
        payout_ticks,
    })
}

/// Runs the journey's sixteen decisions on a new store and stops before
/// delivery, so the store keeps both payouts pending for a relay to send.
///
/// Returns a JSON summary.
///
/// # Errors
///
/// A decision whose outcome differs from the journey's, or a refusal from
/// `invoke`, `create`, or the shell.
#[cfg(feature = "sqlite")]
pub fn payouts(path: &Path) -> AppResult<String> {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let mut shell = create(path, &authority)?;
    let Decided {
        outcomes,
        payout_ticks,
        ..
    } = decide_journey(&mut shell, &authority)?;
    let snapshot = checked(shell.snapshot())?;
    if (snapshot.version(), snapshot.pending()) != (12, 2) || payout_ticks != [4, 8] {
        return Err(format!(
            "decisions differ from the journey: {} bundles, {} pending, payout ticks {payout_ticks:?}",
            snapshot.version(),
            snapshot.pending()
        ));
    }
    Ok(format!(
        "{{\"status\":\"decided\",\"decisions\":[{}],\"bundles\":{},\"pending\":{}}}",
        outcomes.join(","),
        snapshot.version(),
        snapshot.pending()
    ))
}

/// The relay export of an existing store: one line of JSON per pending
/// delivery, in commit order (see `zeno_fcis_shell_sqlite::v2::relay`).
///
/// # Errors
///
/// A store the shell refuses to open or audit.
#[cfg(feature = "sqlite")]
pub fn relay_export(path: &Path) -> AppResult<String> {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let mut shell = checked(Shell::open(path, &authority))?;
    let lines: Vec<String> = checked(shell.export_pending())?
        .iter()
        .map(relay::export_line)
        .collect();
    Ok(lines.join("\n"))
}

/// Acknowledges one delivery of an existing store for a relay, by its ID
/// and the SHA-256 of the payload the relay sent, both in hexadecimal.
///
/// Returns a JSON summary.
///
/// # Errors
///
/// Malformed hexadecimal, or the shell's refusal by name: `UnknownDelivery`,
/// `AlreadyAcknowledged` or `PayloadMismatch`, each with no write.
#[cfg(feature = "sqlite")]
pub fn relay_acknowledge(path: &Path, id: &str, payload_sha256: &str) -> AppResult<String> {
    let parse = |text: &str, name: &str| {
        relay::parse_hex_hash(text)
            .ok_or_else(|| format!("{name}: expected 64 lowercase hexadecimal digits"))
    };
    let (id, hash) = (
        parse(id, "delivery ID")?,
        parse(payload_sha256, "payload hash")?,
    );
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let mut shell = checked(Shell::open(path, &authority))?;
    checked(relay::acknowledge(&mut shell, id, hash))?;
    let snapshot = checked(shell.snapshot())?;
    Ok(format!(
        "{{\"status\":\"acknowledged\",\"delivery_id\":\"{}\",\"pending\":{}}}",
        relay::hex(id.as_bytes()),
        snapshot.pending()
    ))
}
