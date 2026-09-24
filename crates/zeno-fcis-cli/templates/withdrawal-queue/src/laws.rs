//! Independent, bounded checks over the exact invocation and complete decision.
//!
//! The authored formulas in `project.zeno` are evaluated as written. Formulas
//! cannot see the decision's reason or its outbox, and cannot look up the
//! controller's tables, so this file checks those in Rust: the rejection
//! reason from the README's rules, the exact payout request, the absence of
//! effects, the genesis, and, for a tick, the refinement. The refinement
//! check reads the contract's transition table for the output the decision
//! actually made, never the strategy, so a strategy row that the contract
//! forbids is refused; it then requires that output and the next memory to be
//! the row of the strategy that OrbitSynthesis certified. The checker never
//! reads the synthesized step the program runs, so the two are independent.

use crate::{controller, generated::*, profile};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_evidence::EvidenceEnvelope;
use zeno_fcis_laws::*;
use zeno_fcis_spec::*;

/// The most units the vault holds.
const MAX_BALANCE: i128 = 4;
/// The only destination payout requests may go to.
const PAYOUTS_TO: &str = "settlement";

pub struct VaultLaws {
    project: ProjectSpec,
}

impl VaultLaws {
    /// # Errors
    ///
    /// An authored project the library refuses.
    pub fn try_new() -> Result<Self, profile::BindingError> {
        Ok(Self {
            project: profile::project()?,
        })
    }
}

struct NoPredicates;
impl PredicateProvider for NoPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

/// External proofs are not admitted: every law is checked at run time only.
pub struct NoExternalProofs {
    identity: Hash32,
}

impl NoExternalProofs {
    /// # Errors
    ///
    /// A verifier identity the codec refuses.
    pub fn try_new() -> Result<Self, profile::BindingError> {
        Ok(Self {
            identity: profile::digest(
                "example/withdrawal-queue/verifier",
                b"External proofs are not admitted",
            )?,
        })
    }
}

impl LawEvidenceVerifier for NoExternalProofs {
    fn verifier_identity(&self) -> Hash32 {
        self.identity
    }
    fn verify(&self, _: &LawProofSubject, _: &EvidenceEnvelope, _: &[u8]) -> LawProofDecision {
        LawProofDecision::Indeterminate
    }
}

/// The variant ID that `command.101.130` observes.
fn action_id(action: &VaultAction) -> i128 {
    match action {
        VaultAction::Deposit => 160,
        VaultAction::RequestWithdrawal => 161,
        VaultAction::Tick => 162,
    }
}

/// The variant ID that `command.101.131`, `pre.100.127`, and `post.100.127` observe.
fn lane_id(lane: &Lane) -> i128 {
    match lane {
        Lane::A => 170,
        Lane::B => 171,
    }
}

/// The variant ID that the lane status fields observe.
fn status_id(status: &LaneStatus) -> i128 {
    match status {
        LaneStatus::Empty => 180,
        LaneStatus::Arrived => 181,
        LaneStatus::Pending => 182,
    }
}

/// The variant ID that `context.102.140` observes.
fn caller_id(caller: &Caller) -> i128 {
    match caller {
        Caller::Operator => 190,
        Caller::OwnerA => 191,
        Caller::OwnerB => 192,
        Caller::Keeper => 193,
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

/// Observes one vault under `root`, at the numeric IDs of `project.zeno`.
///
/// The law checker observes the vault before and after every decision
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
    state: &Vault,
) -> Result<Vec<Observation>, LawEngineFailure> {
    Ok(vec![
        observe(root, &[100, 120], state.balance.0)?,
        observe(root, &[100, 121], status_id(&state.lane_a))?,
        observe(root, &[100, 122], state.amount_a.0)?,
        observe(root, &[100, 123], status_id(&state.lane_b))?,
        observe(root, &[100, 124], state.amount_b.0)?,
        observe(root, &[100, 125], state.pause.0)?,
        observe(root, &[100, 126], i128::from(state.must_serve.0))?,
        observe(root, &[100, 127], lane_id(&state.priority))?,
    ])
}

/// The observations the formulas read from one decision.
fn trace_step(
    pre: &Vault,
    post: &Vault,
    command: &VaultCommand,
    context: &TickContext,
) -> Result<TraceStep, LawEngineFailure> {
    let mut step = state_observations(ProjectionRoot::Pre, pre)?;
    step.extend(state_observations(ProjectionRoot::Post, post)?);
    step.extend([
        observe(
            ProjectionRoot::Command,
            &[101, 130],
            action_id(&command.action),
        )?,
        observe(ProjectionRoot::Command, &[101, 131], lane_id(&command.lane))?,
        observe(ProjectionRoot::Command, &[101, 132], command.amount.0)?,
        observe(
            ProjectionRoot::Context,
            &[102, 140],
            caller_id(&context.caller),
        )?,
        observe(
            ProjectionRoot::Context,
            &[102, 141],
            i128::from(context.alarm.0),
        )?,
    ]);
    TraceStep::try_new(step).ok_or(LawEngineFailure::Unsupported)
}

/// The caller the README assigns to each command.
fn required_caller(command: &VaultCommand) -> Caller {
    match (&command.action, &command.lane) {
        (VaultAction::Deposit, _) => Caller::Operator,
        (VaultAction::RequestWithdrawal, Lane::A) => Caller::OwnerA,
        (VaultAction::RequestWithdrawal, Lane::B) => Caller::OwnerB,
        (VaultAction::Tick, _) => Caller::Keeper,
    }
}

/// The rejection reason the README requires, or `None` if the input must commit.
fn expected_rejection(pre: &Vault, command: &VaultCommand, context: &TickContext) -> Option<u32> {
    let amount = command.amount.0;
    if context.caller != required_caller(command) {
        return Some(200);
    }
    match command.action {
        VaultAction::RequestWithdrawal => {
            let status = match command.lane {
                Lane::A => &pre.lane_a,
                Lane::B => &pre.lane_b,
            };
            if *status != LaneStatus::Empty {
                Some(201)
            } else if amount > pre.balance.0 - pre.amount_a.0 - pre.amount_b.0 {
                Some(202)
            } else {
                None
            }
        }
        VaultAction::Deposit if pre.balance.0 + amount > MAX_BALANCE => Some(203),
        _ => None,
    }
}

/// The state a deposit or a withdrawal request must leave.
fn expected_after_command(pre: &Vault, command: &VaultCommand) -> Vault {
    let mut post = pre.clone();
    match (&command.action, &command.lane) {
        (VaultAction::Deposit, _) => post.balance = Money(pre.balance.0 + command.amount.0),
        (VaultAction::RequestWithdrawal, Lane::A) => {
            post.lane_a = LaneStatus::Arrived;
            post.amount_a = Money(command.amount.0);
        }
        (VaultAction::RequestWithdrawal, Lane::B) => {
            post.lane_b = LaneStatus::Arrived;
            post.amount_b = Money(command.amount.0);
        }
        (VaultAction::Tick, _) => {}
    }
    post
}

/// The lane a tick paid, from its payout requests: `WAIT` for none. `None`
/// means the outbox is not one well-formed payout.
fn paid_lane(entries: &[zeno_fcis_plan::OutboxEntry]) -> Result<Option<u8>, LawEngineFailure> {
    let unsupported = |_| LawEngineFailure::Unsupported;
    let destination = PayoutDestination(PAYOUTS_TO.into())
        .to_value()
        .map_err(unsupported)?;
    match entries {
        [] => Ok(Some(controller::WAIT)),
        [entry] => {
            if entry.ordinal() != 0 || entry.channel() != 300 || entry.destination() != &destination
            {
                return Ok(None);
            }
            let request =
                PayoutRequest::try_from_value(entry.payload().clone()).map_err(unsupported)?;
            Ok(Some(match request.paid_lane {
                Lane::A => controller::PAY_A,
                Lane::B => controller::PAY_B,
            }))
        }
        _ => Ok(None),
    }
}

/// Whether a tick's decision is the checked controller's: its output is the
/// one the contract allows and the strategy row selects, the plant part of
/// the state follows the contract's transition, the next memory is the row's,
/// and the payout is exactly the paid lane's amount.
fn tick_refines_the_controller(
    pre: &Vault,
    post: &Vault,
    alarm: bool,
    entries: &[zeno_fcis_plan::OutboxEntry],
) -> Result<bool, LawEngineFailure> {
    let Some(output) = paid_lane(entries)? else {
        return Ok(false);
    };
    let pause = |state: &Vault| u8::try_from(state.pause.0).ok();
    let (Some(pause_pre), Some(pause_post)) = (pause(pre), pause(post)) else {
        return Ok(false);
    };
    let plant = controller::plant_index(
        pre.lane_a == LaneStatus::Pending,
        pre.lane_b == LaneStatus::Pending,
        pause_pre,
        pre.must_serve.0,
    );
    let symbol = controller::input_index(
        pre.lane_a == LaneStatus::Arrived,
        pre.lane_b == LaneStatus::Arrived,
        alarm,
    );
    // The contract alone decides whether this output was allowed here and
    // where the plant goes; a forbidden output has no successor.
    let Some(next_plant) = controller::successor(plant, symbol, output) else {
        return Ok(false);
    };
    let plant_post = controller::plant_index(
        post.lane_a == LaneStatus::Pending,
        post.lane_b == LaneStatus::Pending,
        pause_post,
        post.must_serve.0,
    );
    if plant_post != next_plant {
        return Ok(false);
    }
    // The certified strategy's row for the tracked memory must select this
    // output and lead to the memory the decision left.
    let memory = controller::memory_index(plant, pre.priority == Lane::B);
    let memory_post = controller::memory_index(plant_post, post.priority == Lane::B);
    if controller::row(memory, symbol) != Some([output, memory_post]) {
        return Ok(false);
    }
    // Money: the paid lane's whole amount leaves the balance and the lane,
    // and the payout request carries exactly that amount.
    let (paid, amount_a, amount_b) = match output {
        controller::PAY_A => (pre.amount_a.0, 0, pre.amount_b.0),
        controller::PAY_B => (pre.amount_b.0, pre.amount_a.0, 0),
        _ => (0, pre.amount_a.0, pre.amount_b.0),
    };
    if post.balance.0 != pre.balance.0 - paid
        || post.amount_a.0 != amount_a
        || post.amount_b.0 != amount_b
    {
        return Ok(false);
    }
    if let [entry] = entries {
        let request = PayoutRequest::try_from_value(entry.payload().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        if request.paid_amount.0 != paid {
            return Ok(false);
        }
    }
    Ok(true)
}

impl VaultLaws {
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
            profile::id(law_id).map_err(invalid)?,
            status,
            profile::digest("example/withdrawal-queue/observation", &witness).map_err(invalid)?,
        )
        .map_err(invalid)
    }
}

/// A reviewed binding that the library refused makes the observation invalid.
fn invalid<E>(_: E) -> LawEngineFailure {
    LawEngineFailure::InvalidOutput
}

impl ProjectLawEngine for VaultLaws {
    fn evaluate(
        &self,
        input: &LawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        if limits.max_observations < 4 {
            return Err(LawEngineFailure::Incomplete);
        }
        let pre = Vault::try_from_value(input.pre_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let command = VaultCommand::try_from_value(input.command().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let context = TickContext::try_from_value(input.context().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let (post, commit, outbox) = match input.decision() {
            LawDecisionView::Accept {
                post_state,
                commit_plan,
                outbox_plan,
                ..
            } => (post_state, commit_plan, outbox_plan),
            // This application never commits a failure (law 508).
            LawDecisionView::CommittedFailure { .. } => {
                return Err(LawEngineFailure::InvalidOutput);
            }
            LawDecisionView::Reject { reason_id } => {
                return if expected_rejection(&pre, &command, &context) == Some(*reason_id) {
                    Ok(vec![])
                } else {
                    Err(LawEngineFailure::InvalidOutput)
                };
            }
        };
        let post =
            Vault::try_from_value((*post).clone()).map_err(|_| LawEngineFailure::Unsupported)?;
        let accepted_while_rejectable = expected_rejection(&pre, &command, &context).is_some();
        let conforms = match command.action {
            VaultAction::Tick => {
                tick_refines_the_controller(&pre, &post, context.alarm.0, outbox.entries())?
            }
            _ => post == expected_after_command(&pre, &command) && outbox.entries().is_empty(),
        };
        let extra = !accepted_while_rejectable && conforms && commit.effects().is_empty();
        let step = trace_step(&pre, &post, &command, &context)?;
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        Ok(vec![
            self.check(500, &step, true, &bytes)?,
            self.check(501, &step, extra, &bytes)?,
            self.check(502, &step, extra, &bytes)?,
            self.check(503, &step, extra, &bytes)?,
        ])
    }

    fn evaluate_genesis(
        &self,
        input: &GenesisLawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        if limits.max_observations == 0 {
            return Err(LawEngineFailure::Incomplete);
        }
        let state = Vault::try_from_value(input.initial_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let step = trace_step(
            &state,
            &state,
            &VaultCommand {
                action: VaultAction::Tick,
                lane: Lane::A,
                amount: Amount(1),
            },
            &TickContext {
                caller: Caller::Keeper,
                alarm: Flag(false),
            },
        )?;
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        // The controller's initial state: nothing pending, no pause, lane A first.
        let initial = state.balance.0 == 0
            && state.lane_a == LaneStatus::Empty
            && state.amount_a.0 == 0
            && state.lane_b == LaneStatus::Empty
            && state.amount_b.0 == 0
            && state.pause.0 == 0
            && !state.must_serve.0
            && state.priority == Lane::A;
        Ok(vec![self.check(500, &step, initial, &bytes)?])
    }
}
