#![forbid(unsafe_code)]

/// Complete original-domain policy and checked library Authority construction.
pub mod v2_contract;

/// Original application behavior retained for independent oracle tests.
pub mod legacy;

pub mod generated {
    #![allow(dead_code, unused_imports, clippy::all, clippy::pedantic)]
    include!(concat!(env!("OUT_DIR"), "/schema-codegen/rust/treasury.rs"));
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
    AmountOut, BaseUnits, Caller, Direction, GuardContext, HeldAmount, MinOut, ModelId,
    PendingSwap, Price, QuoteUnits, SpentUnits, Tick, TradeAmount, Treasury, TreasuryAction,
    TreasuryCommand,
};
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
    command: &TreasuryCommand,
    context: &GuardContext,
    replay: &str,
) -> AppResult<&'static str> {
    let project = checked(GeneratedProject::try_new::<RustCryptoSha256>())?;
    let snapshot = checked(shell.snapshot())?;
    let command =
        checked(project.admit_command::<RustCryptoSha256>(command, ValidationLimits::default()))?;
    let context =
        checked(project.admit_context::<RustCryptoSha256>(context, ValidationLimits::default()))?;
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
    let replay_id = profile::digest("example/agent-treasury-guard/replay", replay.as_bytes());
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
pub fn decode_state(original: &[u8]) -> AppResult<Treasury> {
    let envelope = checked(decode_envelope(original, DecodeLimits::default()))?;
    checked(Treasury::try_from_value(envelope.into_value()))
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
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = authority(&descriptor)?;
    let destination = Destination::default();
    let mut shell = create(path, &authority)?;
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
        snapshot.version(),
        snapshot.pending(),
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
