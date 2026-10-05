#![forbid(unsafe_code)]

/// Complete original-domain policy and checked library Authority construction.
pub mod v2_contract;

/// Original application behavior retained for independent oracle tests.
pub mod legacy;

pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/stock.rs"));
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
use generated::{OperatorFlag, StockContext};
use generated::{Quantity, Stock, StockAction, StockCommand, Units};
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
use zeno_fcis_synthesis::finite::{
    v2_authority::{GenesisPublicationOutcome, PublicationOutcome},
    v2_composition::Class,
};

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

/// The stock before any request: nothing available, nothing reserved.
pub fn genesis_state() -> Stock {
    Stock {
        available: Units(0),
        reserved: Units(0),
    }
}

/// One stock command.
pub fn request(action: StockAction, quantity: i128) -> StockCommand {
    StockCommand {
        action,
        quantity: Quantity(quantity),
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
        GenesisPublicationOutcome::Commit(genesis) => genesis,
        GenesisPublicationOutcome::Refused { evaluation, error } => {
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
    command: StockCommand,
    authorized: bool,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default()))?;
    let context = checked(project.admit_context::<RustCryptoSha256>(
        &StockContext {
            authorized: OperatorFlag(authorized),
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
    let replay_id = profile::digest("example/inventory-reservation/replay", replay.as_bytes());
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
pub fn decode_state(original: &[u8]) -> AppResult<Stock> {
    let envelope = checked(decode_envelope(original, DecodeLimits::default()))?;
    checked(Stock::try_from_value(envelope.into_value()))
}

/// Restocks, reserves, releases, and ships, and meets every rejection reason
/// along the way. It then interrupts shipment delivery, reopens the database,
/// finishes delivery, and checks that the units in stock equal the units
/// restocked minus the units shipped.
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
        (StockAction::Restock, 3, true, "Accept"),
        (StockAction::Restock, 3, true, "Reject"),
        (StockAction::Restock, 2, true, "Accept"),
        (StockAction::Reserve, 3, true, "Accept"),
        (StockAction::Reserve, 3, true, "Reject"),
        (StockAction::Release, 1, true, "Accept"),
        (StockAction::Ship, 3, true, "Reject"),
        (StockAction::Ship, 2, false, "Reject"),
        (StockAction::Ship, 2, true, "Accept"),
        (StockAction::Reserve, 3, true, "Accept"),
        (StockAction::Ship, 1, true, "Accept"),
    ];
    let (mut restocked, mut shipped) = (0, 0);
    let mut outcomes = Vec::new();
    for (step, (action, quantity, authorized, expected)) in steps.into_iter().enumerate() {
        let before = checked(shell.snapshot())?;
        let moves = (action == StockAction::Restock, action == StockAction::Ship);
        let outcome = invoke(
            &mut shell,
            &authority,
            request(action, quantity),
            authorized,
            &format!("journey-{step}"),
        )?;
        if outcome != expected {
            return Err(format!("step {step}: expected {expected}, got {outcome}"));
        }
        if outcome == "Reject" && checked(shell.snapshot())? != before {
            return Err(format!("step {step}: a rejection published data"));
        }
        if outcome == "Accept" {
            restocked += if moves.0 { quantity } else { 0 };
            shipped += if moves.1 { quantity } else { 0 };
        }
        outcomes.push(format!("\"{outcome}\""));
    }
    let pending = checked(shell.next_pending())?.ok_or("missing pending shipment")?;
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
    if state.available.0 + state.reserved.0 != restocked - shipped {
        return Err("units in stock differ from units restocked minus units shipped".into());
    }
    let summary = (
        state.available.0,
        state.reserved.0,
        snapshot.version(),
        snapshot.pending(),
        destination.delivered_count(),
    );
    if summary != (0, 2, 7, 0, 2) || (restocked, shipped) != (5, 3) {
        return Err(format!(
            "lifecycle differs from the expected result: {summary:?}"
        ));
    }
    Ok(format!(
        "{{\"status\":\"passed\",\"decisions\":[{}],\"available\":{},\"reserved\":{},\"restocked\":{restocked},\"shipped\":{shipped},\"bundles\":{},\"pending\":{},\"deliveries\":{}}}",
        outcomes.join(","),
        summary.0,
        summary.1,
        summary.2,
        summary.3,
        summary.4
    ))
}
