#![forbid(unsafe_code)]

pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/treasury.rs"));
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
use generated::{
    AmountOut, BaseUnits, Caller, Direction, GuardContext, HeldAmount, MinOut, ModelId,
    PendingSwap, Price, QuoteUnits, SpentUnits, Tick, TradeAmount, Treasury, TreasuryAction,
    TreasuryCommand,
};
use laws::{GuardLaws, NoExternalProofs};
use program::GuardProgram;
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

pub type Authority = CatalogCommitAuthority<RustCryptoSha256, GuardProgram, GuardLaws, Destination>;
#[cfg(feature = "sqlite")]
pub type Shell = SqliteShell<GuardProgram, GuardLaws, Destination>;
pub type AppResult<T> = Result<T, String>;

fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> AppResult<T> {
    value.map_err(|error| format!("{error:?}"))
}

/// The treasury before any request: 6 quote, 1 base, nothing spent, tick 0,
/// no swap outstanding.
#[must_use]
pub fn genesis_state() -> Treasury {
    treasury(6, 1, 0, 0, PendingSwap::NoSwap, 0, 0)
}

/// A state for tests and examples, in field order.
#[must_use]
pub fn treasury(
    quote: i128,
    base: i128,
    spent_today: i128,
    last_seen: i128,
    pending: PendingSwap,
    pending_amount: i128,
    pending_min_out: i128,
) -> Treasury {
    Treasury {
        quote: QuoteUnits(quote),
        base: BaseUnits(base),
        spent_today: SpentUnits(spent_today),
        last_seen: Tick(last_seen),
        pending,
        pending_amount: HeldAmount(pending_amount),
        pending_min_out: MinOut(pending_min_out),
    }
}

/// One command. Every action ignores the fields it does not use; the
/// constructors below set them to their smallest admitted values.
#[must_use]
pub fn command(
    action: TreasuryAction,
    direction: Direction,
    amount: i128,
    min_out: i128,
    intent: i128,
    amount_out: i128,
) -> TreasuryCommand {
    TreasuryCommand {
        action,
        direction,
        amount: TradeAmount(amount),
        min_out: MinOut(min_out),
        intent: Tick(intent),
        amount_out: AmountOut(amount_out),
    }
}

/// The agent's proposal: sell `amount` of the asset the direction names, for
/// at least `min_out` of the other.
#[must_use]
pub fn propose(direction: Direction, amount: i128, min_out: i128) -> TreasuryCommand {
    command(
        TreasuryAction::ProposeSwap,
        direction,
        amount,
        min_out,
        0,
        0,
    )
}

/// The report from the DEX that the intent settled and delivered
/// `amount_out`.
#[must_use]
pub fn settled(intent: i128, amount_out: i128) -> TreasuryCommand {
    command(
        TreasuryAction::SwapSettled,
        Direction::BuyBase,
        1,
        0,
        intent,
        amount_out,
    )
}

/// The report from the DEX that the intent failed, expired, or was refused.
#[must_use]
pub fn failed(intent: i128) -> TreasuryCommand {
    command(
        TreasuryAction::SwapFailed,
        Direction::BuyBase,
        1,
        0,
        intent,
        0,
    )
}

/// The context a request carries: who sent it, the current tick, the oracle
/// price and the tick it was observed at, and the model the agent runs.
#[must_use]
pub fn context(
    caller: Caller,
    now: i128,
    price: i128,
    price_time: i128,
    model: ModelId,
) -> GuardContext {
    GuardContext {
        caller,
        now: Tick(now),
        price: Price(price),
        price_time: Tick(price_time),
        model,
    }
}

/// The commit authority for this treasury: the reviewed program, the law
/// checker, and the exact genesis policy.
///
/// # Errors
///
/// If the generated project, the law manifest, or the hash provider fails
/// its own admission, or the manifest does not enforce the scopes
/// `project.zeno` declares; none does for this reviewed source.
pub fn authority() -> AppResult<Authority> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let initial = checked(
        project.admit_root::<RustCryptoSha256>(&genesis_state(), ValidationLimits::default()),
    )?;
    let domain = checked(Domain::new("example/agent-treasury-guard/state", 1))?;
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
        GuardLaws::default(),
        &NoExternalProofs,
    ))?;
    let label =
        |name: &str| profile::digest("example/agent-treasury-guard/local-policy", name.as_bytes());
    checked(CatalogCommitAuthority::try_new(
        project.catalog(),
        checked(StateDomainBinding::try_new(
            "example/agent-treasury-guard/state",
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
            label("quote=6; base=1; spent_today=0; last_seen=0; pending=NoSwap"),
            label("runtime-checked genesis; no external proof"),
            label("local tutorial deployment"),
        ))?,
        TransitionLimits::default(),
        &checked(verify_approved_provider::<RustCryptoSha256>())?,
        laws,
        GuardProgram,
    ))
}

/// Creates the exact reviewed genesis. An existing database is rejected by the shell.
///
/// # Errors
///
/// If the database exists, or the genesis is refused.
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
/// `replay` identifies the incoming message. Publishing the same authorized
/// decision again is an idempotent replay; a message that arrives again is
/// decided against the treasury's current state. The context is a trusted
/// tutorial input: a deployment must authenticate the caller, read the time
/// and the oracle price itself, and bind the model identity from its own
/// records before admission. An agent's proposal never carries its context.
///
/// # Errors
///
/// If the command or context is not admitted, the authority fails, or the
/// shell cannot publish or replay the decision.
#[cfg(feature = "sqlite")]
pub fn invoke(
    shell: &mut Shell,
    authority: &Authority,
    command: &TreasuryCommand,
    context: &GuardContext,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let pre = checked(Treasury::try_from_value(snapshot.state().clone()))?;
    let pre = checked(project.admit_root::<RustCryptoSha256>(&pre, ValidationLimits::default()))?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(command, ValidationLimits::default()))?;
    let context =
        checked(project.admit_context::<RustCryptoSha256>(context, ValidationLimits::default()))?;
    let witness = checked(authority.admit_invocation(
        pre,
        command.admitted().clone(),
        context.admitted().clone(),
        profile::digest(
            "example/agent-treasury-guard/principal",
            b"local tutorial caller",
        ),
        profile::digest(
            "example/agent-treasury-guard/authentication",
            b"trusted local input; no remote authentication",
        ),
        profile::digest("example/agent-treasury-guard/replay", replay.as_bytes()),
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

/// One step of the scripted agent: the tick, the caller, the model, the
/// oracle price and its tick, the command, what happens in words, and the
/// outcome the guard must reach.
#[cfg(feature = "sqlite")]
struct Step {
    now: i128,
    caller: Caller,
    model: ModelId,
    price: i128,
    price_time: i128,
    command: TreasuryCommand,
    story: &'static str,
    expected: &'static str,
}

/// A scripted stand-in for the agent over three days. No model is called and
/// no network is used: a real agent's proposal becomes the same command, and
/// the shell supplies the context. The script is a table, one line per step.
#[cfg(feature = "sqlite")]
#[allow(clippy::too_many_lines)]
fn script() -> Vec<Step> {
    use Caller::{Agent, Dex};
    use Direction::{BuyBase, SellBase};
    use ModelId::{TreasuryAgentV2 as V2, UnlistedModel};
    let step = |now, caller, model, price, price_time, command, story, expected| Step {
        now,
        caller,
        model,
        price,
        price_time,
        command,
        story,
        expected,
    };
    vec![
        // Day 0: a good buy, then answers and proposals the guard refuses.
        step(
            1,
            Agent,
            V2,
            1,
            1,
            propose(BuyBase, 2, 2),
            "agent buys 2 base with 2 quote at price 1, for at least 2 base",
            "Accept",
        ),
        step(
            2,
            Agent,
            V2,
            1,
            2,
            propose(BuyBase, 1, 1),
            "agent proposes again while the buy is outstanding",
            "Reject",
        ),
        step(
            2,
            Agent,
            V2,
            1,
            2,
            settled(1, 2),
            "agent, not the DEX, reports the buy settled",
            "Reject",
        ),
        step(
            2,
            Dex,
            V2,
            1,
            2,
            settled(1, 1),
            "DEX settles the buy with 1 base, below the minimum of 2",
            "Reject",
        ),
        step(
            2,
            Dex,
            V2,
            1,
            2,
            settled(1, 2),
            "DEX settles the buy with 2 base",
            "Accept",
        ),
        step(
            3,
            Dex,
            V2,
            1,
            3,
            settled(1, 2),
            "DEX repeats the settlement; nothing is outstanding",
            "Reject",
        ),
        step(
            3,
            Agent,
            V2,
            2,
            3,
            propose(SellBase, 2, 3),
            "agent sells 2 base at price 2: worth 4, over the cap of 3",
            "Reject",
        ),
        step(
            3,
            Agent,
            V2,
            1,
            1,
            propose(BuyBase, 1, 1),
            "agent buys on a price observed 2 ticks ago",
            "Reject",
        ),
        step(
            3,
            Agent,
            V2,
            1,
            3,
            propose(BuyBase, 3, 3),
            "agent buys 3: with 2 already spent today, over the budget of 4",
            "Reject",
        ),
        step(
            3,
            Agent,
            V2,
            1,
            3,
            propose(BuyBase, 2, 1),
            "agent buys 2 for at least 1 base: below three quarters of the oracle value",
            "Reject",
        ),
        step(
            3,
            Agent,
            UnlistedModel,
            1,
            3,
            propose(BuyBase, 1, 1),
            "an unlisted model proposes a buy",
            "Reject",
        ),
        step(
            3,
            Agent,
            V2,
            2,
            3,
            propose(SellBase, 1, 2),
            "agent sells 1 base at price 2 for at least 2 quote, filling the budget",
            "Accept",
        ),
        // Day 1: a late answer about the first swap, a failure, the reserve,
        // and a budget that a failure does not restore.
        step(
            4,
            Dex,
            V2,
            2,
            4,
            settled(1, 2),
            "DEX settles intent 1 again, late: the outstanding intent is 3",
            "Reject",
        ),
        step(
            4,
            Dex,
            V2,
            2,
            4,
            failed(3),
            "DEX reports the sell failed: the base is refunded, and the new day restarts the budget",
            "CommittedFailure",
        ),
        step(
            5,
            Agent,
            V2,
            1,
            5,
            propose(BuyBase, 3, 3),
            "agent buys 3 with 4 quote: that would leave 1, below the reserve of 2",
            "Reject",
        ),
        step(
            5,
            Agent,
            V2,
            1,
            5,
            propose(BuyBase, 2, 2),
            "agent buys 2 base with 2 quote",
            "Accept",
        ),
        step(
            6,
            Dex,
            V2,
            1,
            6,
            failed(5),
            "DEX reports the buy failed: the quote is refunded, the 2 stay committed",
            "CommittedFailure",
        ),
        step(
            7,
            Agent,
            V2,
            1,
            7,
            propose(BuyBase, 3, 3),
            "agent buys 3: with 2 still committed today, over the budget",
            "Reject",
        ),
        step(
            7,
            Agent,
            V2,
            1,
            7,
            propose(BuyBase, 2, 2),
            "agent buys 2 base with 2 quote",
            "Accept",
        ),
        // Day 2: the budget restarts, the clock must advance, and a sell settles.
        step(
            8,
            Dex,
            V2,
            1,
            8,
            settled(7, 3),
            "DEX settles the buy with 3 base; the new day restarts the budget",
            "Accept",
        ),
        step(
            8,
            Agent,
            V2,
            1,
            8,
            propose(BuyBase, 1, 1),
            "agent proposes at the tick of the last commit",
            "Reject",
        ),
        step(
            9,
            Agent,
            V2,
            1,
            9,
            propose(SellBase, 3, 3),
            "agent sells 3 base at price 1 for at least 3 quote",
            "Accept",
        ),
        step(
            10,
            Dex,
            V2,
            1,
            10,
            settled(9, 3),
            "DEX settles the sell with 3 quote",
            "Accept",
        ),
    ]
}

/// Runs the scripted agent through three days: good and bad proposals, the
/// DEX's settlements and failures, and every rule's refusal. It then
/// interrupts request delivery, reopens the database, and finishes delivery.
///
/// Returns a JSON summary that tells the story step by step.
///
/// # Errors
///
/// If any step ends other than as scripted, or the delivery does not finish.
#[cfg(feature = "sqlite")]
pub fn journey(path: &Path) -> AppResult<String> {
    let authority = authority()?;
    let mut destination = Destination::default();
    let mut shell = create(path, &authority, destination.clone())?;
    let mut outcomes = Vec::new();
    let mut story = Vec::new();
    for (index, step) in script().into_iter().enumerate() {
        let before = checked(shell.snapshot())?;
        let outcome = invoke(
            &mut shell,
            &authority,
            &step.command,
            &context(
                step.caller,
                step.now,
                step.price,
                step.price_time,
                step.model,
            ),
            &format!("message-{index}"),
        )?;
        if outcome != step.expected {
            return Err(format!(
                "step {index}: expected {}, got {outcome}",
                step.expected
            ));
        }
        if outcome == "Reject" && checked(shell.snapshot())? != before {
            return Err(format!("step {index}: a rejection published data"));
        }
        outcomes.push(format!("\"{outcome}\""));
        story.push(format!("\"tick {}: {}: {outcome}\"", step.now, step.story));
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
    let state = checked(Treasury::try_from_value(snapshot.state().clone()))?;
    let summary = (
        snapshot.bundle_count(),
        snapshot.pending_outbox(),
        destination.delivered_count(),
    );
    if state != treasury(5, 3, 3, 10, PendingSwap::NoSwap, 0, 0) || summary != (10, 0, 5) {
        return Err(format!(
            "lifecycle differs from the expected result: {state:?} {summary:?}"
        ));
    }
    Ok(format!(
        "{{\"status\":\"passed\",\"decisions\":[{}],\"story\":[{}],\"quote\":{},\"base\":{},\"spent_today\":{},\"last_seen\":{},\"swap\":\"NoSwap\",\"bundles\":{},\"pending\":{},\"deliveries\":{}}}",
        outcomes.join(","),
        story.join(","),
        state.quote.0,
        state.base.0,
        state.spent_today.0,
        state.last_seen.0,
        summary.0,
        summary.1,
        summary.2
    ))
}
