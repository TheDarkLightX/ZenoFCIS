//! Independent, bounded checks over the exact invocation and complete decision.
//!
//! The authored formulas in `project.zeno` are evaluated as written. Formulas
//! cannot see the decision's reason, its outbox, or its effects, so this file
//! checks those in Rust, from the README's rules rather than from the program.

use crate::{generated::*, profile};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_evidence::EvidenceEnvelope;
use zeno_fcis_laws::*;
use zeno_fcis_spec::*;
use zeno_fcis_value::Value;

/// Ticks in one day.
const DAY: i128 = 4;
/// The oldest price a proposal may use.
const PRICE_AGE: i128 = 1;
/// Ticks a request stays valid.
const TTL: i128 = 2;
/// Quote value one day may commit.
const BUDGET: i128 = 4;
/// Quote value one swap may commit.
const CAP: i128 = 3;
/// Quote the treasury keeps.
const RESERVE: i128 = 2;
/// The only destination swap requests may go to.
const REQUESTS_TO: &str = "zenodex";

pub struct GuardLaws {
    project: ProjectSpec,
}

impl Default for GuardLaws {
    fn default() -> Self {
        Self {
            project: profile::project(),
        }
    }
}

struct NoPredicates;
impl PredicateProvider for NoPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

pub struct NoExternalProofs;
impl LawEvidenceVerifier for NoExternalProofs {
    fn verifier_identity(&self) -> Hash32 {
        profile::digest(
            "example/agent-treasury-guard/verifier",
            b"External proofs are not admitted",
        )
    }
    fn verify(&self, _: &LawProofSubject, _: &EvidenceEnvelope, _: &[u8]) -> LawProofDecision {
        LawProofDecision::Indeterminate
    }
}

/// The variant ID that `pre.100.134` and `post.100.134` observe.
fn pending_id(pending: &PendingSwap) -> i128 {
    match pending {
        PendingSwap::NoSwap => 170,
        PendingSwap::PendingBuy => 171,
        PendingSwap::PendingSell => 172,
    }
}

/// The variant ID that `command.101.141` observes.
fn direction_id(direction: &Direction) -> i128 {
    match direction {
        Direction::BuyBase => 173,
        Direction::SellBase => 174,
    }
}

/// The variant ID that `command.101.140` observes.
fn action_id(action: &TreasuryAction) -> i128 {
    match action {
        TreasuryAction::ProposeSwap => 175,
        TreasuryAction::SwapSettled => 176,
        TreasuryAction::SwapFailed => 177,
    }
}

/// The variant ID that `context.102.150` observes.
fn caller_id(caller: &Caller) -> i128 {
    match caller {
        Caller::Agent => 178,
        Caller::Dex => 179,
    }
}

/// The variant ID that `context.102.154` observes.
fn model_id(model: &ModelId) -> i128 {
    match model {
        ModelId::TreasuryAgentV2 => 180,
        ModelId::TreasuryAgentV1 => 181,
        ModelId::UnlistedModel => 182,
    }
}

/// One observation at a reviewed path. Only a path the library refuses can
/// fail, which the static IDs here never cause.
fn observe(
    root: ProjectionRoot,
    raw: &[u32],
    value: i128,
) -> Result<Observation, LawEngineFailure> {
    let segments = raw
        .iter()
        .map(|id| StableId::new(*id).ok_or(LawEngineFailure::Unsupported))
        .collect::<Result<Vec<_>, _>>()?;
    let path = ProjectionPath::try_new(root, segments).ok_or(LawEngineFailure::Unsupported)?;
    Ok(Observation::new(path, value))
}

/// Observes one treasury under `root`, at the numeric IDs of `project.zeno`.
///
/// The law checker observes the treasury before and after every decision
/// through this one mapping, so an invariant over `pre.` paths, such as
/// claims 600 and 601, reads the same fields at genesis, before a step, and
/// after it, which the induction in `tests/claims.rs` relies on.
///
/// # Errors
///
/// `LawEngineFailure::Unsupported` if a projection cannot be formed, which
/// the static IDs here never cause.
pub fn state_observations(
    root: ProjectionRoot,
    state: &Treasury,
) -> Result<Vec<Observation>, LawEngineFailure> {
    Ok(vec![
        observe(root, &[100, 130], state.quote.0)?,
        observe(root, &[100, 131], state.base.0)?,
        observe(root, &[100, 132], state.spent_today.0)?,
        observe(root, &[100, 133], state.last_seen.0)?,
        observe(root, &[100, 134], pending_id(&state.pending))?,
        observe(root, &[100, 135], state.pending_amount.0)?,
        observe(root, &[100, 136], state.pending_min_out.0)?,
    ])
}

/// The observations the formulas read for one decision, at the paths
/// `project.zeno` declares.
///
/// # Errors
///
/// As `state_observations`.
pub fn trace_step(
    pre: &Treasury,
    post: &Treasury,
    command: &TreasuryCommand,
    context: &GuardContext,
) -> Result<TraceStep, LawEngineFailure> {
    let mut step = state_observations(ProjectionRoot::Pre, pre)?;
    step.extend(state_observations(ProjectionRoot::Post, post)?);
    step.extend([
        observe(
            ProjectionRoot::Command,
            &[101, 140],
            action_id(&command.action),
        )?,
        observe(
            ProjectionRoot::Command,
            &[101, 141],
            direction_id(&command.direction),
        )?,
        observe(ProjectionRoot::Command, &[101, 142], command.amount.0)?,
        observe(ProjectionRoot::Command, &[101, 143], command.min_out.0)?,
        observe(ProjectionRoot::Command, &[101, 144], command.intent.0)?,
        observe(ProjectionRoot::Command, &[101, 145], command.amount_out.0)?,
        observe(
            ProjectionRoot::Context,
            &[102, 150],
            caller_id(&context.caller),
        )?,
        observe(ProjectionRoot::Context, &[102, 151], context.now.0)?,
        observe(ProjectionRoot::Context, &[102, 152], context.price.0)?,
        observe(ProjectionRoot::Context, &[102, 153], context.price_time.0)?,
        observe(
            ProjectionRoot::Context,
            &[102, 154],
            model_id(&context.model),
        )?,
    ]);
    TraceStep::try_new(step).ok_or(LawEngineFailure::Unsupported)
}

/// The caller the README assigns to each action.
fn required_caller(action: &TreasuryAction) -> Caller {
    match action {
        TreasuryAction::ProposeSwap => Caller::Agent,
        TreasuryAction::SwapSettled | TreasuryAction::SwapFailed => Caller::Dex,
    }
}

/// The quote value a proposal commits, as the README values it.
fn value(direction: &Direction, amount: i128, price: i128) -> i128 {
    match direction {
        Direction::BuyBase => amount,
        Direction::SellBase => amount * price,
    }
}

/// The smallest `min_out` the README's slippage bound allows.
fn least_min_out(direction: &Direction, amount: i128, price: i128) -> i128 {
    let (numerator, denominator) = match direction {
        Direction::BuyBase => (amount * 3, 4 * price),
        Direction::SellBase => (amount * price * 3, 4),
    };
    (numerator + denominator - 1) / denominator
}

/// The budget already committed on the day of the request.
fn spent_on_the_day(pre: &Treasury, now: i128) -> i128 {
    if now / DAY > pre.last_seen.0 / DAY {
        0
    } else {
        pre.spent_today.0
    }
}

/// The rejection reason the README requires, or `None` if the input must commit.
fn expected_rejection(
    pre: &Treasury,
    command: &TreasuryCommand,
    context: &GuardContext,
) -> Option<u32> {
    let now = context.now.0;
    if context.caller != required_caller(&command.action) {
        return Some(200);
    }
    if now <= pre.last_seen.0 {
        return Some(201);
    }
    match command.action {
        TreasuryAction::ProposeSwap => {
            let price = context.price.0;
            let amount = command.amount.0;
            let committed = value(&command.direction, amount, price);
            let held = match command.direction {
                Direction::BuyBase => pre.quote.0 - RESERVE,
                Direction::SellBase => pre.base.0,
            };
            if context.model != ModelId::TreasuryAgentV2 {
                Some(202)
            } else if context.price_time.0 > now || now - context.price_time.0 > PRICE_AGE {
                Some(203)
            } else if pre.pending != PendingSwap::NoSwap {
                Some(204)
            } else if committed > CAP {
                Some(208)
            } else if spent_on_the_day(pre, now) + committed > BUDGET {
                Some(209)
            } else if command.min_out.0 < least_min_out(&command.direction, amount, price) {
                Some(210)
            } else if held < amount {
                Some(211)
            } else {
                None
            }
        }
        TreasuryAction::SwapSettled | TreasuryAction::SwapFailed => {
            if pre.pending == PendingSwap::NoSwap {
                Some(205)
            } else if command.intent != pre.last_seen {
                Some(206)
            } else if command.action == TreasuryAction::SwapSettled
                && command.amount_out.0 < pre.pending_min_out.0
            {
                Some(207)
            } else {
                None
            }
        }
    }
}

/// The destination and payload of the one request a committing decision must
/// queue, if any: an accepted proposal queues exactly the swap it proposed.
fn expected_request(
    command: &TreasuryCommand,
    context: &GuardContext,
) -> Result<Option<(Value, Value)>, LawEngineFailure> {
    if command.action != TreasuryAction::ProposeSwap {
        return Ok(None);
    }
    let unsupported = |_| LawEngineFailure::Unsupported;
    let (asset_in, asset_out) = match command.direction {
        Direction::BuyBase => (Asset::Quote, Asset::Base),
        Direction::SellBase => (Asset::Base, Asset::Quote),
    };
    Ok(Some((
        DexDestination(REQUESTS_TO.into())
            .to_value()
            .map_err(unsupported)?,
        SwapRequest {
            intent_number: context.now,
            asset_in,
            asset_out,
            amount_in: command.amount,
            min_amount_out: command.min_out,
            deadline: Deadline(context.now.0 + TTL),
        }
        .to_value()
        .map_err(unsupported)?,
    )))
}

impl GuardLaws {
    fn check(
        &self,
        law_id: u32,
        step: &TraceStep,
        extra: bool,
        input: &[u8],
    ) -> Result<LawObservation, LawEngineFailure> {
        let law = self
            .project
            .laws()
            .iter()
            .find(|law| law.id().get() == law_id)
            .ok_or(LawEngineFailure::Unsupported)?;
        let result = evaluate_relational(
            law.formula(),
            EvaluationContext::new(step, &NoPredicates, EvalLimits::default()),
        );
        let status = match (result, extra) {
            (EvalOutcome::True, true) => LawStatus::Satisfied,
            (EvalOutcome::Indeterminate(_), _) => LawStatus::Indeterminate,
            _ => LawStatus::Violated,
        };
        let mut witness = input.to_vec();
        witness.extend_from_slice(&law_id.to_be_bytes());
        witness.push(status as u8);
        LawObservation::try_new(
            profile::id(law_id),
            status,
            profile::digest("example/agent-treasury-guard/observation", &witness),
        )
        .map_err(|_| LawEngineFailure::InvalidOutput)
    }
}

impl ProjectLawEngine for GuardLaws {
    fn evaluate(
        &self,
        input: &LawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        if limits.max_observations < 8 {
            return Err(LawEngineFailure::Incomplete);
        }
        let pre = Treasury::try_from_value(input.pre_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let command = TreasuryCommand::try_from_value(input.command().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let context = GuardContext::try_from_value(input.context().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let (post, commit, outbox, relations, reason_ok): (_, _, _, &[u32], _) =
            match input.decision() {
                LawDecisionView::Accept {
                    post_state,
                    commit_plan,
                    outbox_plan,
                    ..
                } => (
                    post_state,
                    commit_plan,
                    outbox_plan,
                    &[501, 502, 503, 504, 505, 506, 507],
                    true,
                ),
                LawDecisionView::CommittedFailure {
                    reason_id,
                    post_state,
                    commit_plan,
                    outbox_plan,
                    ..
                } => (
                    post_state,
                    commit_plan,
                    outbox_plan,
                    &[501, 502, 508],
                    *reason_id == 212,
                ),
                LawDecisionView::Reject { reason_id } => {
                    return if expected_rejection(&pre, &command, &context) == Some(*reason_id) {
                        Ok(vec![])
                    } else {
                        Err(LawEngineFailure::InvalidOutput)
                    };
                }
            };
        let post =
            Treasury::try_from_value((*post).clone()).map_err(|_| LawEngineFailure::Unsupported)?;
        let committed_while_rejectable = expected_rejection(&pre, &command, &context).is_some();
        let delivery_ok = match (outbox.entries(), expected_request(&command, &context)?) {
            ([], None) => true,
            ([entry], Some((destination, payload))) => {
                entry.ordinal() == 0
                    && entry.channel() == 300
                    && entry.destination() == &destination
                    && entry.payload() == &payload
            }
            _ => false,
        };
        let extra =
            reason_ok && !committed_while_rejectable && delivery_ok && commit.effects().is_empty();
        let step = trace_step(&pre, &post, &command, &context)?;
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        let mut checked = vec![self.check(500, &step, true, &bytes)?];
        for relation in relations {
            checked.push(self.check(*relation, &step, extra, &bytes)?);
        }
        Ok(checked)
    }

    fn evaluate_genesis(
        &self,
        input: &GenesisLawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        if limits.max_observations == 0 {
            return Err(LawEngineFailure::Incomplete);
        }
        let state = Treasury::try_from_value(input.initial_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let step = trace_step(
            &state,
            &state,
            &TreasuryCommand {
                action: TreasuryAction::ProposeSwap,
                direction: Direction::BuyBase,
                amount: TradeAmount(1),
                min_out: MinOut(0),
                intent: Tick(0),
                amount_out: AmountOut(0),
            },
            &GuardContext {
                caller: Caller::Agent,
                now: Tick(0),
                price: Price(1),
                price_time: Tick(0),
                model: ModelId::TreasuryAgentV2,
            },
        )?;
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        // The reviewed genesis: 6 quote, 1 base, nothing spent, tick 0, no swap.
        let reviewed = state.quote.0 == 6
            && state.base.0 == 1
            && state.spent_today.0 == 0
            && state.last_seen.0 == 0
            && state.pending == PendingSwap::NoSwap
            && state.pending_amount.0 == 0
            && state.pending_min_out.0 == 0;
        Ok(vec![self.check(500, &step, reviewed, &bytes)?])
    }
}
