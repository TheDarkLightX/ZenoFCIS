#![forbid(unsafe_code)]

pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/stock.rs"));
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
#[cfg(feature = "sqlite")]
use generated::{OperatorFlag, StockContext};
use generated::{Quantity, Stock, StockAction, StockCommand, Units};
use laws::{NoExternalProofs, StockLaws};
use program::StockProgram;
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

pub type Authority = CatalogCommitAuthority<RustCryptoSha256, StockProgram, StockLaws, Destination>;
#[cfg(feature = "sqlite")]
pub type Shell = SqliteShell<StockProgram, StockLaws, Destination>;
pub type AppResult<T> = Result<T, String>;

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

pub fn authority() -> AppResult<Authority> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(
        project.admit_root::<RustCryptoSha256>(&genesis_state(), ValidationLimits::default()),
    )?;
    let domain = checked(Domain::new("example/inventory-reservation/state", 1))?;
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
        StockLaws::default(),
        &NoExternalProofs,
    ))?;
    let label = |name: &str| {
        profile::digest(
            "example/inventory-reservation/local-policy",
            name.as_bytes(),
        )
    };
    checked(CatalogCommitAuthority::try_new(
        project.catalog(),
        checked(StateDomainBinding::try_new(
            "example/inventory-reservation/state",
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
            label("available=0; reserved=0; bounds=0..5"),
            label("runtime-checked genesis; no external proof"),
            label("local tutorial deployment"),
        ))?,
        TransitionLimits::default(),
        &checked(verify_approved_provider::<RustCryptoSha256>())?,
        laws,
        StockProgram,
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
/// Local tutorial inputs: a deployment must authenticate the principal and the
/// operator flag before admission.
#[cfg(feature = "sqlite")]
pub fn invoke(
    shell: &mut Shell,
    authority: &Authority,
    command: StockCommand,
    authorized: bool,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let pre = checked(Stock::try_from_value(snapshot.state().clone()))?;
    let pre = checked(project.admit_root::<RustCryptoSha256>(&pre, ValidationLimits::default()))?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default()))?;
    let context = checked(project.admit_context::<RustCryptoSha256>(
        &StockContext {
            authorized: OperatorFlag(authorized),
        },
        ValidationLimits::default(),
    ))?;
    let witness = checked(authority.admit_invocation(
        pre,
        command.admitted().clone(),
        context.admitted().clone(),
        profile::digest(
            "example/inventory-reservation/principal",
            b"local tutorial operator",
        ),
        profile::digest(
            "example/inventory-reservation/authentication",
            b"trusted local input; no remote authentication",
        ),
        profile::digest("example/inventory-reservation/replay", replay.as_bytes()),
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

/// Restocks, reserves, releases, and ships, and meets every rejection reason
/// along the way. It then interrupts shipment delivery, reopens the database,
/// finishes delivery, and checks that the units in stock equal the units
/// restocked minus the units shipped.
///
/// Returns a JSON summary.
#[cfg(feature = "sqlite")]
pub fn journey(path: &Path) -> AppResult<String> {
    let authority = authority()?;
    let mut destination = Destination::default();
    let mut shell = create(path, &authority, destination.clone())?;
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
    let state = checked(Stock::try_from_value(snapshot.state().clone()))?;
    if state.available.0 + state.reserved.0 != restocked - shipped {
        return Err("units in stock differ from units restocked minus units shipped".into());
    }
    let summary = (
        state.available.0,
        state.reserved.0,
        snapshot.bundle_count(),
        snapshot.pending_outbox(),
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
