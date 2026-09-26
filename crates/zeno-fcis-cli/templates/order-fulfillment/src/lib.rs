#![forbid(unsafe_code)]

pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/order.rs"));
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
#[path = "../synthesized/transition.rs"]
pub mod synthesized;

use bindings::GeneratedProject;
use delivery::Destination;
use generated::{Attempts, Order, OrderAction, OrderCommand, OrderStatus};
#[cfg(feature = "sqlite")]
use generated::{Caller, CallerContext};
use laws::{NoExternalProofs, OrderLaws};
use program::OrderProgram;
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

pub type Authority = CatalogCommitAuthority<RustCryptoSha256, OrderProgram, OrderLaws, Destination>;
#[cfg(feature = "sqlite")]
pub type Shell = SqliteShell<OrderProgram, OrderLaws, Destination>;
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

pub fn authority() -> AppResult<Authority> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(
        project.admit_root::<RustCryptoSha256>(&genesis_state(), ValidationLimits::default()),
    )?;
    let domain = checked(Domain::new("example/order-fulfillment/state", 1))?;
    let initial_root = checked(hash_value::<RustCryptoSha256>(
        domain,
        initial.value().value(),
    ))?;
    // The scopes `project.zeno` declares, which `check` and `prove` read,
    // must be the scopes this authority enforces.
    let manifest = profile::manifest();
    checked(manifest.check_declared_scopes(&profile::project()))?;
    let laws = checked(verify_project_laws::<RustCryptoSha256, _, _>(
        project.catalog(),
        manifest,
        profile::source_hash(),
        vec![],
        LawLimits::default(),
        profile::checker_hash(),
        OrderLaws::default(),
        &NoExternalProofs,
    ))?;
    let label =
        |name: &str| profile::digest("example/order-fulfillment/local-policy", name.as_bytes());
    checked(CatalogCommitAuthority::try_new(
        project.catalog(),
        checked(StateDomainBinding::try_new(
            "example/order-fulfillment/state",
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
            label("status=Placed; payment_attempts=0"),
            label("runtime-checked genesis; no external proof"),
            label("local tutorial deployment"),
        ))?,
        TransitionLimits::default(),
        &checked(verify_approved_provider::<RustCryptoSha256>())?,
        laws,
        OrderProgram,
    ))
}

/// Creates the exact reviewed genesis. An existing database is rejected by the shell.
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
/// `replay` identifies the incoming message, such as a webhook's delivery ID.
/// Publishing the same authorized decision again is an idempotent replay; a
/// message that arrives again is decided against the order's current status.
/// Local tutorial inputs: a deployment must authenticate the caller before
/// admission.
#[cfg(feature = "sqlite")]
pub fn invoke(
    shell: &mut Shell,
    authority: &Authority,
    command: OrderCommand,
    caller: Caller,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let pre = checked(Order::try_from_value(snapshot.state().clone()))?;
    let pre = checked(project.admit_root::<RustCryptoSha256>(&pre, ValidationLimits::default()))?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default()))?;
    let context = checked(project.admit_context::<RustCryptoSha256>(
        &CallerContext { caller },
        ValidationLimits::default(),
    ))?;
    let witness = checked(authority.admit_invocation(
        pre,
        command.admitted().clone(),
        context.admitted().clone(),
        profile::digest(
            "example/order-fulfillment/principal",
            b"local tutorial caller",
        ),
        profile::digest(
            "example/order-fulfillment/authentication",
            b"trusted local input; no remote authentication",
        ),
        profile::digest("example/order-fulfillment/replay", replay.as_bytes()),
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

/// Declines one payment, and refuses a late capture of it. After a second
/// checkout, refuses a late decline of the first attempt, a capture from the
/// wrong caller, and a repeated capture, then pays, ships, and delivers the
/// order. It then interrupts request delivery, reopens the database, and
/// finishes delivery.
///
/// Returns a JSON summary.
#[cfg(feature = "sqlite")]
pub fn journey(path: &Path) -> AppResult<String> {
    let authority = authority()?;
    let mut destination = Destination::default();
    let mut shell = create(path, &authority, destination.clone())?;
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
    let state = checked(Order::try_from_value(snapshot.state().clone()))?;
    let summary = (
        snapshot.bundle_count(),
        snapshot.pending_outbox(),
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
