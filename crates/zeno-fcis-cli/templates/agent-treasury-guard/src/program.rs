//! Reviewed adapter around the synthesized precedence core.
//!
//! An AI agent proposes swaps. Its proposal is only a command: the guard
//! decides what the treasury commits. This adapter computes the guard's facts
//! with checked arithmetic and applies the effects of a decision;
//! `synthesized::transition` decides which rule applies first. That core was
//! selected by `zeno-fcis synth` and checked against the README's precedence
//! on all 6,144 tuples of an action and eleven facts (`synthesis.json`).
//!
//! Time, the oracle price, and the model identity arrive in the context, so
//! the decision reads no clock, no price feed, and no model.
//!
//! The core's decision code maps to the README's rules:
//!
//! | Code | Decision |
//! | --- | --- |
//! | 0 | accept |
//! | 1 to 12 | reject with the rule of that rank: `wrong_caller` (200) through `below_reserve` (211) |
//! | 13 | commit failure `swap_failed` (212) |

use crate::{bindings::*, generated::*, profile, synthesized};
use zeno_fcis_authority::{CatalogTransitionProgram, ReviewedTransitionInput};
use zeno_fcis_codec::Hash32;
use zeno_fcis_core::BudgetUsed;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_transition::TransitionDecision;

/// Ticks in one day of this scaled domain; a deployment would use 86,400.
pub const DAY_TICKS: i128 = 4;
/// The oldest oracle price a proposal may be decided on, in ticks.
pub const MAX_PRICE_AGE: i128 = 1;
/// Ticks a queued request stays valid after its proposal.
pub const INTENT_TTL: i128 = 2;
/// Quote value the treasury may commit in one day.
pub const DAILY_BUDGET: i128 = 4;
/// Quote value one swap may commit.
pub const TRADE_CAP: i128 = 3;
/// Quote the treasury always keeps.
pub const QUOTE_RESERVE: i128 = 2;
/// Base the treasury always keeps: it may sell all of it.
pub const BASE_FLOOR: i128 = 0;
/// A swap must deliver at least this fraction of the oracle value.
pub const KEEP_NUMERATOR: i128 = 3;
pub const KEEP_DENOMINATOR: i128 = 4;
/// Where swap requests are delivered.
pub const DEX: &str = "zenodex";
/// The core's codes for the two committing outcomes; the codes between them
/// are the rejection reasons in README order.
const ACCEPT: i64 = 0;
const SWAP_FAILED: i64 = 13;

type Transition<'a> = GeneratedTransition<'a, RustCryptoSha256>;

pub struct GuardProgram;

#[derive(Debug)]
pub enum GuardProgramError {
    Project(Box<GeneratedProjectError>),
    /// Checked arithmetic left the integer domain, or the core refused a
    /// fact tuple or returned a code outside its table. Neither can happen
    /// for admitted inputs; both fail closed.
    Domain,
}
impl From<GeneratedProjectError> for GuardProgramError {
    fn from(error: GeneratedProjectError) -> Self {
        Self::Project(Box::new(error))
    }
}
impl From<AdapterError> for GuardProgramError {
    fn from(error: AdapterError) -> Self {
        GeneratedProjectError::from(error).into()
    }
}

/// The only caller allowed to send each action.
#[must_use]
pub fn sender(action: &TreasuryAction) -> Caller {
    match action {
        TreasuryAction::ProposeSwap => Caller::Agent,
        TreasuryAction::SwapSettled | TreasuryAction::SwapFailed => Caller::Dex,
    }
}

/// Whether a model may propose swaps. The allowlist binds the model's
/// identity to every decision it obtains.
#[must_use]
pub fn approved(model: &ModelId) -> bool {
    matches!(model, ModelId::TreasuryAgentV2)
}

/// The action code the core takes.
#[must_use]
pub fn action_code(action: &TreasuryAction) -> i64 {
    match action {
        TreasuryAction::ProposeSwap => 0,
        TreasuryAction::SwapSettled => 1,
        TreasuryAction::SwapFailed => 2,
    }
}

/// The day a tick falls on.
#[must_use]
pub fn day(tick: i128) -> i128 {
    tick / DAY_TICKS
}

/// The quote value a proposal commits: its amount for a buy, and its amount
/// at the oracle price for a sell. `None` if the product overflows.
#[must_use]
pub fn committed_value(direction: &Direction, amount: i128, price: i128) -> Option<i128> {
    match direction {
        Direction::BuyBase => Some(amount),
        Direction::SellBase => amount.checked_mul(price),
    }
}

/// The smallest `min_out` the slippage bound allows: three quarters of the
/// oracle value, rounded up so that rounding never favors the agent. `None`
/// for a negative amount, a price below 1, or an overflow.
#[must_use]
pub fn least_min_out(direction: &Direction, amount: i128, price: i128) -> Option<i128> {
    let (numerator, denominator) = match direction {
        Direction::BuyBase => (
            amount.checked_mul(KEEP_NUMERATOR)?,
            KEEP_DENOMINATOR.checked_mul(price)?,
        ),
        Direction::SellBase => (
            amount.checked_mul(price)?.checked_mul(KEEP_NUMERATOR)?,
            KEEP_DENOMINATOR,
        ),
    };
    if numerator < 0 || denominator <= 0 {
        return None;
    }
    numerator
        .checked_add(denominator)?
        .checked_sub(1)?
        .checked_div(denominator)
}

/// The treasury's fields, read through the transition so that the read
/// footprint is recorded.
struct Held {
    quote: i128,
    base: i128,
    spent_before: i128,
    last_seen: i128,
    pending: PendingSwap,
    amount: i128,
    min_out: i128,
}

fn read(transition: &mut Transition<'_>) -> Result<Held, GuardProgramError> {
    Ok(Held {
        quote: transition.read_quote()?.0,
        base: transition.read_base()?.0,
        spent_before: transition.read_spent_today()?.0,
        last_seen: transition.read_last_seen()?.0,
        pending: transition.read_pending()?,
        amount: transition.read_pending_amount()?.0,
        min_out: transition.read_pending_min_out()?.0,
    })
}

/// The facts the core decides over, in its input order after the action.
/// They are exactly the core's eleven Boolean inputs, so a struct of named
/// Booleans is their clearest form.
#[allow(clippy::struct_excessive_bools)]
struct Facts {
    caller_ok: bool,
    clock_ok: bool,
    model_ok: bool,
    price_fresh: bool,
    swap_pending: bool,
    intent_current: bool,
    settlement_sufficient: bool,
    within_cap: bool,
    within_budget: bool,
    slippage_ok: bool,
    reserve_ok: bool,
}

impl Facts {
    fn inputs(&self, action: &TreasuryAction) -> [i64; 12] {
        [
            action_code(action),
            i64::from(self.caller_ok),
            i64::from(self.clock_ok),
            i64::from(self.model_ok),
            i64::from(self.price_fresh),
            i64::from(self.swap_pending),
            i64::from(self.intent_current),
            i64::from(self.settlement_sufficient),
            i64::from(self.within_cap),
            i64::from(self.within_budget),
            i64::from(self.slippage_ok),
            i64::from(self.reserve_ok),
        ]
    }
}

/// The facts, and the sums an accepted proposal commits.
struct Assessment {
    facts: Facts,
    /// The budget already committed on the day of the request.
    spent: i128,
    /// The quote value the proposal commits.
    value: i128,
    /// The balance of the asset the proposal sells.
    sold: i128,
}

/// Computes every fact with checked arithmetic. Facts an action does not
/// use are computed all the same; the core ignores them.
fn assess(
    held: &Held,
    command: &TreasuryCommand,
    context: &GuardContext,
) -> Result<Assessment, GuardProgramError> {
    let domain = || GuardProgramError::Domain;
    let now = context.now.0;
    let price = context.price.0;
    let amount = command.amount.0;
    // The budget belongs to the day of the last commit; a later day starts
    // from zero.
    let spent = if day(now) > day(held.last_seen) {
        0
    } else {
        held.spent_before
    };
    let value = committed_value(&command.direction, amount, price).ok_or_else(domain)?;
    let least = least_min_out(&command.direction, amount, price).ok_or_else(domain)?;
    let (sold, floor) = match command.direction {
        Direction::BuyBase => (held.quote, QUOTE_RESERVE),
        Direction::SellBase => (held.base, BASE_FLOOR),
    };
    let facts = Facts {
        caller_ok: context.caller == sender(&command.action),
        // Every commit takes its own tick: the tick numbers the intent, so a
        // late answer about an earlier swap can never name the current one.
        clock_ok: now > held.last_seen,
        model_ok: approved(&context.model),
        price_fresh: context.price_time.0 <= now
            && now.checked_sub(context.price_time.0).ok_or_else(domain)? <= MAX_PRICE_AGE,
        swap_pending: held.pending != PendingSwap::NoSwap,
        intent_current: command.intent.0 == held.last_seen,
        settlement_sufficient: command.amount_out.0 >= held.min_out,
        within_cap: value <= TRADE_CAP,
        within_budget: spent.checked_add(value).ok_or_else(domain)? <= DAILY_BUDGET,
        slippage_ok: command.min_out.0 >= least,
        reserve_ok: sold.checked_sub(amount).ok_or_else(domain)? >= floor,
    };
    Ok(Assessment {
        facts,
        spent,
        value,
        sold,
    })
}

/// What the core decided.
enum Decision {
    Accept,
    Reject(RejectReasonId),
    Fail(CommittedFailureReasonId),
}

/// Reads the core's code through the table in the module documentation.
fn decision(code: i64) -> Result<Decision, GuardProgramError> {
    Ok(match code {
        ACCEPT => Decision::Accept,
        1 => Decision::Reject(RejectReasonId::Reason200),
        2 => Decision::Reject(RejectReasonId::Reason201),
        3 => Decision::Reject(RejectReasonId::Reason202),
        4 => Decision::Reject(RejectReasonId::Reason203),
        5 => Decision::Reject(RejectReasonId::Reason204),
        6 => Decision::Reject(RejectReasonId::Reason205),
        7 => Decision::Reject(RejectReasonId::Reason206),
        8 => Decision::Reject(RejectReasonId::Reason207),
        9 => Decision::Reject(RejectReasonId::Reason208),
        10 => Decision::Reject(RejectReasonId::Reason209),
        11 => Decision::Reject(RejectReasonId::Reason210),
        12 => Decision::Reject(RejectReasonId::Reason211),
        SWAP_FAILED => Decision::Fail(CommittedFailureReasonId::Reason212),
        _ => return Err(GuardProgramError::Domain),
    })
}

/// Debits the sold asset, records the swap, counts its value, and queues the
/// request to the DEX.
fn propose(
    transition: &mut Transition<'_>,
    held: &Held,
    assessed: &Assessment,
    command: &TreasuryCommand,
    now: i128,
) -> Result<(), GuardProgramError> {
    let domain = || GuardProgramError::Domain;
    let amount = command.amount.0;
    let debited = assessed.sold.checked_sub(amount).ok_or_else(domain)?;
    let (asset_in, asset_out) = match command.direction {
        Direction::BuyBase => {
            transition.update_quote(&QuoteUnits(debited))?;
            transition.update_pending(&PendingSwap::PendingBuy)?;
            (Asset::Quote, Asset::Base)
        }
        Direction::SellBase => {
            transition.update_base(&BaseUnits(debited))?;
            transition.update_pending(&PendingSwap::PendingSell)?;
            (Asset::Base, Asset::Quote)
        }
    };
    let committed = assessed
        .spent
        .checked_add(assessed.value)
        .ok_or_else(domain)?;
    if committed != held.spent_before {
        transition.update_spent_today(&SpentUnits(committed))?;
    }
    transition.update_pending_amount(&HeldAmount(amount))?;
    transition.update_pending_min_out(&command.min_out)?;
    // The request is shaped after ZenoDEX's SwapIntent. Its number is this
    // proposal's tick; the adapter that submits it derives ZenoDEX's own
    // intent ID and signs it.
    transition.enqueue_channel_300(
        0,
        &DexDestination(DEX.into()),
        &SwapRequest {
            intent_number: Tick(now),
            asset_in,
            asset_out,
            amount_in: command.amount,
            min_amount_out: command.min_out,
            deadline: Deadline(now.checked_add(INTENT_TTL).ok_or_else(domain)?),
        },
    )?;
    Ok(())
}

/// Credits the bought asset with what the DEX delivered.
fn settle(
    transition: &mut Transition<'_>,
    held: &Held,
    amount_out: i128,
) -> Result<(), GuardProgramError> {
    let domain = || GuardProgramError::Domain;
    match held.pending {
        PendingSwap::PendingBuy => {
            transition.update_base(&BaseUnits(
                held.base.checked_add(amount_out).ok_or_else(domain)?,
            ))?;
        }
        PendingSwap::PendingSell => {
            transition.update_quote(&QuoteUnits(
                held.quote.checked_add(amount_out).ok_or_else(domain)?,
            ))?;
        }
        PendingSwap::NoSwap => return Err(domain()),
    }
    Ok(())
}

/// Returns the held amount to the asset it was taken from. The budget stays
/// committed: the failure is a fact worth keeping, so it commits as a
/// failure instead of being rejected.
fn refund(transition: &mut Transition<'_>, held: &Held) -> Result<(), GuardProgramError> {
    let domain = || GuardProgramError::Domain;
    match held.pending {
        PendingSwap::PendingBuy => {
            transition.update_quote(&QuoteUnits(
                held.quote.checked_add(held.amount).ok_or_else(domain)?,
            ))?;
        }
        PendingSwap::PendingSell => {
            transition.update_base(&BaseUnits(
                held.base.checked_add(held.amount).ok_or_else(domain)?,
            ))?;
        }
        PendingSwap::NoSwap => return Err(domain()),
    }
    Ok(())
}

/// Clears the outstanding swap and restarts the budget on a new day.
fn clear_swap(
    transition: &mut Transition<'_>,
    held: &Held,
    spent: i128,
) -> Result<(), GuardProgramError> {
    transition.update_pending(&PendingSwap::NoSwap)?;
    transition.update_pending_amount(&HeldAmount(0))?;
    transition.update_pending_min_out(&MinOut(0))?;
    if spent != held.spent_before {
        transition.update_spent_today(&SpentUnits(spent))?;
    }
    Ok(())
}

impl CatalogTransitionProgram<RustCryptoSha256> for GuardProgram {
    type Error = GuardProgramError;

    fn transition_build_hash(&self) -> Hash32 {
        profile::program_hash()
    }

    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error> {
        let project = GeneratedProject::try_new::<RustCryptoSha256>()?;
        let command = TreasuryCommand::try_from_value(input.command().value().value().clone())?;
        let context = GuardContext::try_from_value(input.context().value().value().clone())?;
        let admitted_command =
            project.admit_command::<RustCryptoSha256>(&command, ValidationLimits::default())?;
        let admitted_context =
            project.admit_context::<RustCryptoSha256>(&context, ValidationLimits::default())?;
        let mut transition = project.begin_bound_transition::<RustCryptoSha256>(
            input.pre_state(),
            input.state_domain(),
            &admitted_command,
            &admitted_context,
            input.expected_bindings(),
            BudgetUsed::default(),
            input.limits(),
        )?;
        transition
            .observe_context_caller()?
            .observe_context_now()?
            .observe_context_price()?
            .observe_context_price_time()?
            .observe_context_model()?;
        let held = read(&mut transition)?;
        let assessed = assess(&held, &command, &context)?;
        let inputs = assessed.facts.inputs(&command.action);
        let [code] = synthesized::transition(&inputs).ok_or(GuardProgramError::Domain)?;
        let now = context.now.0;
        match decision(code)? {
            Decision::Reject(reason) => {
                transition.require(false, reason)?;
                return Ok(transition.seal()?);
            }
            Decision::Accept if command.action == TreasuryAction::ProposeSwap => {
                propose(&mut transition, &held, &assessed, &command, now)?;
            }
            Decision::Accept => {
                settle(&mut transition, &held, command.amount_out.0)?;
                clear_swap(&mut transition, &held, assessed.spent)?;
            }
            Decision::Fail(reason) => {
                refund(&mut transition, &held)?;
                clear_swap(&mut transition, &held, assessed.spent)?;
                transition.fail_if(true, reason)?;
            }
        }
        transition.update_last_seen(&Tick(now))?;
        Ok(transition.seal()?)
    }
}
