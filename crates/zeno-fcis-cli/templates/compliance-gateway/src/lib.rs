#![forbid(unsafe_code)]

pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/standing.rs"));
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
pub mod rules;
#[path = "../synthesized/transition.rs"]
pub mod synthesized;

use bindings::GeneratedProject;
use delivery::Destination;
use generated::{
    AmountBand, CallerContext, CounterpartyRisk, GatewayAction, GatewayCommand, IdentityTier,
    Region, ReviewerFlag, Standing, Strikes,
};
use laws::{GatewayLaws, NoExternalProofs};
use program::GatewayProgram;
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

pub type Authority =
    CatalogCommitAuthority<RustCryptoSha256, GatewayProgram, GatewayLaws, Destination>;
#[cfg(feature = "sqlite")]
pub type Shell = SqliteShell<GatewayProgram, GatewayLaws, Destination>;
pub type AppResult<T> = Result<T, String>;

fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> AppResult<T> {
    value.map_err(|error| format!("{error:?}"))
}

/// The standing before any request: no strikes.
#[must_use]
pub fn genesis_state() -> Standing {
    Standing {
        strikes: Strikes(0),
    }
}

/// A request to screen one transfer.
#[must_use]
pub fn screen(
    region: Region,
    amount_band: i128,
    counterparty_risk: CounterpartyRisk,
) -> GatewayCommand {
    GatewayCommand {
        action: GatewayAction::Screen,
        region,
        amount_band: AmountBand(amount_band),
        counterparty_risk,
    }
}

/// A request to reinstate the account. The transfer fields are ignored.
#[must_use]
pub fn reinstate() -> GatewayCommand {
    GatewayCommand {
        action: GatewayAction::Reinstate,
        region: Region::Allowed,
        amount_band: AmountBand(0),
        counterparty_risk: CounterpartyRisk::Low,
    }
}

/// The context a request carries: the customer's identity tier and whether
/// the caller is a reviewer.
#[must_use]
pub fn context(identity_tier: i128, reviewer: bool) -> CallerContext {
    CallerContext {
        identity_tier: IdentityTier(identity_tier),
        reviewer: ReviewerFlag(reviewer),
    }
}

/// Builds the commit authority: the generated catalog, the law manifest, the
/// law checker with its rule base, and the synthesized program.
///
/// # Errors
///
/// A refused rule base, a law manifest that does not match the catalog or
/// does not enforce the scopes `project.zeno` declares, or a hash provider
/// that fails its known answers, each rendered as text.
pub fn authority() -> AppResult<Authority> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(
        project.admit_root::<RustCryptoSha256>(&genesis_state(), ValidationLimits::default()),
    )?;
    let domain = checked(Domain::new("example/compliance-gateway/state", 1))?;
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
        checked(GatewayLaws::try_new())?,
        &NoExternalProofs,
    ))?;
    let label =
        |name: &str| profile::digest("example/compliance-gateway/local-policy", name.as_bytes());
    checked(CatalogCommitAuthority::try_new(
        project.catalog(),
        checked(StateDomainBinding::try_new(
            "example/compliance-gateway/state",
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
            label("strikes=0; bounds=0..3"),
            label("runtime-checked genesis; no external proof"),
            label("local tutorial deployment"),
        ))?,
        TransitionLimits::default(),
        &checked(verify_approved_provider::<RustCryptoSha256>())?,
        laws,
        GatewayProgram,
    ))
}

/// Creates the exact reviewed genesis. An existing database is rejected by the shell.
///
/// # Errors
///
/// The shell's refusal to create the database, rendered as text.
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

/// Decides one request and, if it commits, publishes it and checks its exact replay.
///
/// Local tutorial inputs: a deployment must authenticate the principal and the
/// context, including the identity tier and the reviewer flag, before
/// admission.
///
/// # Errors
///
/// A request the schema does not admit, a decision a law refuses, or a
/// publication or replay the shell refuses, each rendered as text.
#[cfg(feature = "sqlite")]
pub fn invoke(
    shell: &mut Shell,
    authority: &Authority,
    command: GatewayCommand,
    context: CallerContext,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let pre = checked(Standing::try_from_value(snapshot.state().clone()))?;
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
            "example/compliance-gateway/principal",
            b"local tutorial customer",
        ),
        profile::digest(
            "example/compliance-gateway/authentication",
            b"trusted local input; no remote authentication",
        ),
        profile::digest("example/compliance-gateway/replay", replay.as_bytes()),
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

/// Screens transfers that are allowed, held, and blocked under every blocking
/// rule, freezes the account with three strikes, refuses a reinstatement
/// without a reviewer and one with nothing to reinstate, and reinstates as a
/// reviewer. It then interrupts notice delivery, reopens the database, and
/// finishes delivery.
///
/// Returns a JSON summary.
///
/// # Errors
///
/// Any step whose outcome, standing, outbox, or delivery count differs from
/// the story above, rendered as text.
#[cfg(feature = "sqlite")]
pub fn journey(path: &Path) -> AppResult<String> {
    let authority = authority()?;
    let mut destination = Destination::default();
    let mut shell = create(path, &authority, destination.clone())?;
    let steps = [
        (
            screen(Region::Allowed, 1, CounterpartyRisk::Low),
            2,
            false,
            "Accept",
        ),
        (
            screen(Region::Restricted, 2, CounterpartyRisk::Low),
            2,
            false,
            "Accept",
        ),
        (
            screen(Region::Sanctioned, 0, CounterpartyRisk::Low),
            3,
            false,
            "CommittedFailure",
        ),
        (reinstate(), 3, false, "Reject"),
        (
            screen(Region::Allowed, 3, CounterpartyRisk::High),
            0,
            false,
            "CommittedFailure",
        ),
        (
            screen(Region::Allowed, 2, CounterpartyRisk::Low),
            2,
            false,
            "Accept",
        ),
        (
            screen(Region::Restricted, 4, CounterpartyRisk::Medium),
            3,
            false,
            "CommittedFailure",
        ),
        (
            screen(Region::Allowed, 0, CounterpartyRisk::Low),
            3,
            false,
            "CommittedFailure",
        ),
        (reinstate(), 3, true, "Accept"),
        (reinstate(), 3, true, "Reject"),
        (
            screen(Region::Allowed, 4, CounterpartyRisk::Low),
            0,
            false,
            "CommittedFailure",
        ),
        (
            screen(Region::Allowed, 4, CounterpartyRisk::Medium),
            3,
            false,
            "Accept",
        ),
    ];
    let (mut allowed, mut held, mut blocked) = (0, 0, 0);
    let mut outcomes = Vec::new();
    for (step, (command, identity_tier, reviewer, expected)) in steps.into_iter().enumerate() {
        let before = checked(shell.snapshot())?;
        let screening = command.action == GatewayAction::Screen;
        let outcome = invoke(
            &mut shell,
            &authority,
            command,
            context(identity_tier, reviewer),
            &format!("journey-{step}"),
        )?;
        if outcome != expected {
            return Err(format!("step {step}: expected {expected}, got {outcome}"));
        }
        let after = checked(shell.snapshot())?;
        if outcome == "Reject" && after != before {
            return Err(format!("step {step}: a rejection published data"));
        }
        // A held transfer is the accepted screening that queues a ticket.
        let queued = after.pending_outbox() > before.pending_outbox();
        match (outcome, screening, queued) {
            ("Accept", true, false) => allowed += 1,
            ("Accept", true, true) => held += 1,
            ("CommittedFailure", true, true) => blocked += 1,
            ("CommittedFailure", _, _) => {
                return Err(format!(
                    "step {step}: a committed failure outside a screening, or without an alert"
                ));
            }
            _ => {}
        }
        outcomes.push(format!("\"{outcome}\""));
    }
    let pending = checked(shell.next_pending())?.ok_or("missing pending notice")?;
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
    let state = checked(Standing::try_from_value(snapshot.state().clone()))?;
    let summary = (
        state.strikes.0,
        snapshot.bundle_count(),
        snapshot.pending_outbox(),
        destination.delivered_count(),
    );
    if summary != (1, 10, 0, 8) || (allowed, held, blocked) != (1, 3, 5) {
        return Err(format!(
            "lifecycle differs from the expected result: {summary:?} {:?}",
            (allowed, held, blocked)
        ));
    }
    Ok(format!(
        "{{\"status\":\"passed\",\"decisions\":[{}],\"strikes\":{},\"allowed\":{allowed},\"held\":{held},\"blocked\":{blocked},\"bundles\":{},\"pending\":{},\"deliveries\":{}}}",
        outcomes.join(","),
        summary.0,
        summary.1,
        summary.2,
        summary.3
    ))
}
