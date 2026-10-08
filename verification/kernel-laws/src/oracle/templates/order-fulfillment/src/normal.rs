#![forbid(unsafe_code)]

/// Complete original-domain policy and checked library Authority construction.
#[path = "v2_contract.rs"]
pub mod v2_contract;

pub use super::{bindings, generated, delivery, profile};

use bindings::GeneratedProject;
use delivery::Destination;
use generated::{Attempts, Order, OrderAction, OrderCommand, OrderStatus};
use generated::{Caller, CallerContext};
use std::path::Path;
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, decode_envelope};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_synthesis::finite::v2_composition as composition;
use zeno_fcis_synthesis::finite::{v2_authority::PublicationOutcome, v2_composition::Class};

/// Library authority borrowing the exact checked policy descriptor.
pub type Authority<'p> = zeno_fcis_synthesis::finite::v2_authority::Authority<'p>;
/// Persistent shell consuming genuine library publication capabilities.
pub type Shell<'a, 'p> = zeno_fcis_shell_sqlite::v2::V2SqliteShell<'a, 'p>;
pub type AppResult<T> = Result<T, String>;

fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> AppResult<T> {
    value.map_err(|error| format!("{error:?}"))
}

/// The order before any request: placed, with no payment attempt.
pub fn genesis_state() -> Order {
    Order {
        status: OrderStatus::Placed,
        payment_attempts: Attempts(0),
    }
}

/// One command. Only a payment provider's callback uses `callback_attempt`:
/// it names the payment attempt the answer is about.
pub fn command(action: OrderAction, callback_attempt: i128) -> OrderCommand {
    OrderCommand {
        action,
        callback_attempt: Attempts(callback_attempt),
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
pub fn invoke(
    shell: &mut Shell<'_, '_>,
    authority: &Authority<'_>,
    command: OrderCommand,
    caller: Caller,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default()))?;
    let context = checked(project.admit_context::<RustCryptoSha256>(
        &CallerContext { caller },
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
    let replay_id = profile::digest("example/order-fulfillment/replay", replay.as_bytes());
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
pub fn decode_state(original: &[u8]) -> AppResult<Order> {
    let envelope = checked(decode_envelope(original, DecodeLimits::default()))?;
    checked(Order::try_from_value(envelope.into_value()))
}

/// Declines one payment, and refuses a late capture of it. After a second
/// checkout, refuses a late decline of the first attempt, a capture from the
/// wrong caller, and a repeated capture, then pays, ships, and delivers the
/// order. It then interrupts request delivery, reopens the database, and
/// finishes delivery.
///
/// Returns a JSON summary.
pub fn journey(path: &Path) -> AppResult<String> {
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let destination = Destination::default();
    let mut shell = create(path, &authority)?;
    let steps = [
        (OrderAction::Checkout, 0, Caller::Customer, "Accept"),
        (
            OrderAction::PaymentDeclined,
            1,
            Caller::PaymentProvider,
            "CommittedFailure",
        ),
        (
            OrderAction::PaymentCaptured,
            1,
            Caller::PaymentProvider,
            "Reject",
        ),
        (OrderAction::Checkout, 0, Caller::Customer, "Accept"),
        (
            OrderAction::PaymentDeclined,
            1,
            Caller::PaymentProvider,
            "Reject",
        ),
        (OrderAction::PaymentCaptured, 2, Caller::Customer, "Reject"),
        (
            OrderAction::PaymentCaptured,
            2,
            Caller::PaymentProvider,
            "Accept",
        ),
        (
            OrderAction::PaymentCaptured,
            2,
            Caller::PaymentProvider,
            "Reject",
        ),
        (OrderAction::CancelOrder, 0, Caller::Customer, "Reject"),
        (OrderAction::ParcelDispatched, 0, Caller::Carrier, "Accept"),
        (OrderAction::ParcelDelivered, 0, Caller::Carrier, "Accept"),
    ];
    let mut outcomes = Vec::new();
    for (step, (action, attempt, caller, expected)) in steps.into_iter().enumerate() {
        let before = checked(shell.snapshot())?;
        let outcome = invoke(
            &mut shell,
            &authority,
            command(action, attempt),
            caller,
            &format!("message-{step}"),
        )?;
        if outcome != expected {
            return Err(format!("step {step}: expected {expected}, got {outcome}"));
        }
        if outcome == "Reject" && checked(shell.snapshot())? != before {
            return Err(format!("step {step}: a rejection published data"));
        }
        outcomes.push(format!("\"{outcome}\""));
    }
    let pending = checked(shell.next_pending())?.ok_or("missing pending request")?;
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
    let summary = (
        snapshot.version(),
        snapshot.pending(),
        destination.delivered_count(),
    );
    if state.status != OrderStatus::Delivered
        || state.payment_attempts.0 != 2
        || summary != (6, 0, 3)
    {
        return Err(format!(
            "lifecycle differs from the expected result: {state:?} {summary:?}"
        ));
    }
    Ok(format!(
        "{{\"status\":\"passed\",\"decisions\":[{}],\"order_status\":\"Delivered\",\"payment_attempts\":{},\"bundles\":{},\"pending\":{},\"deliveries\":{}}}",
        outcomes.join(","),
        state.payment_attempts.0,
        summary.0,
        summary.1,
        summary.2
    ))
}
