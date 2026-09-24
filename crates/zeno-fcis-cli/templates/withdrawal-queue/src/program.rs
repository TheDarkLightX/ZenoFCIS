//! Reviewed decision for one vault. Deposits and withdrawal requests follow
//! hand-written rules; a tick runs the synthesized controller step.
//!
//! `zeno-fcis synth run` selected the step in `synthesized/transition.rs`
//! from the sketch in `synthesis.json` by checking every hole assignment
//! against the tick rules on all 384 inputs. This adapter maps the vault's
//! fields into the step's inputs, maps the step's outputs back into typed
//! staging, and queues exactly one payout request when the step pays a lane.
//! The law checker in `src/laws.rs` then requires the tick to match the
//! controller table that OrbitSynthesis certified and the contract's
//! transition, so an adapter error cannot publish.

use crate::{bindings::*, controller, generated::*, profile, synthesized};
use zeno_fcis_authority::{CatalogTransitionProgram, ReviewedTransitionInput};
use zeno_fcis_codec::Hash32;
use zeno_fcis_core::BudgetUsed;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_transition::TransitionDecision;

/// The most units the vault holds; `build.rs` gives the schema the same bound.
pub const MAX_BALANCE: i128 = 4;
/// Where payout requests are delivered.
pub const SETTLEMENT: &str = "settlement";

/// The only caller allowed to send each command.
#[must_use]
pub fn sender(action: &VaultAction, lane: &Lane) -> Caller {
    match (action, lane) {
        (VaultAction::Deposit, _) => Caller::Operator,
        (VaultAction::RequestWithdrawal, Lane::A) => Caller::OwnerA,
        (VaultAction::RequestWithdrawal, Lane::B) => Caller::OwnerB,
        (VaultAction::Tick, _) => Caller::Keeper,
    }
}

/// The decision, bound to the exact source it was built from.
pub struct VaultProgram {
    build_hash: Hash32,
}

impl VaultProgram {
    /// # Errors
    ///
    /// A source binding the library refuses.
    pub fn try_new() -> Result<Self, profile::BindingError> {
        Ok(Self {
            build_hash: profile::program_hash()?,
        })
    }
}

#[derive(Debug)]
pub enum VaultProgramError {
    Project(Box<GeneratedProjectError>),
    /// The vault's fields or the step's answer lie outside the checked domain.
    ControllerDomain,
}
impl From<GeneratedProjectError> for VaultProgramError {
    fn from(error: GeneratedProjectError) -> Self {
        Self::Project(Box::new(error))
    }
}
impl From<AdapterError> for VaultProgramError {
    fn from(error: AdapterError) -> Self {
        GeneratedProjectError::from(error).into()
    }
}

/// A lane's status after a tick, from the step's pending flag.
fn lane_status(pending: i64) -> LaneStatus {
    if pending == 1 {
        LaneStatus::Pending
    } else {
        LaneStatus::Empty
    }
}

impl CatalogTransitionProgram<RustCryptoSha256> for VaultProgram {
    type Error = VaultProgramError;

    fn transition_build_hash(&self) -> Hash32 {
        self.build_hash
    }

    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error> {
        let project = GeneratedProject::try_new::<RustCryptoSha256>()?;
        let command = VaultCommand::try_from_value(input.command().value().value().clone())?;
        let context = TickContext::try_from_value(input.context().value().value().clone())?;
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
        transition.observe_context_caller()?;
        if context.caller != sender(&command.action, &command.lane) {
            transition.require(false, RejectReasonId::Reason200)?;
            return Ok(transition.seal()?);
        }
        let balance = transition.read_balance()?.0;
        let lane_a = transition.read_lane_a()?;
        let amount_a = transition.read_amount_a()?.0;
        let lane_b = transition.read_lane_b()?;
        let amount_b = transition.read_amount_b()?.0;
        let amount = command.amount.0;
        match command.action {
            VaultAction::Deposit => {
                if balance + amount > MAX_BALANCE {
                    transition.require(false, RejectReasonId::Reason203)?;
                } else {
                    transition.update_balance(&Money(balance + amount))?;
                }
            }
            VaultAction::RequestWithdrawal => {
                let status = match command.lane {
                    Lane::A => &lane_a,
                    Lane::B => &lane_b,
                };
                if *status != LaneStatus::Empty {
                    transition.require(false, RejectReasonId::Reason201)?;
                } else if amount > balance - amount_a - amount_b {
                    // Every requested amount stays reserved inside the
                    // balance, so a payout never lacks funds.
                    transition.require(false, RejectReasonId::Reason202)?;
                } else {
                    match command.lane {
                        Lane::A => {
                            transition.update_lane_a(&LaneStatus::Arrived)?;
                            transition.update_amount_a(&Money(amount))?;
                        }
                        Lane::B => {
                            transition.update_lane_b(&LaneStatus::Arrived)?;
                            transition.update_amount_b(&Money(amount))?;
                        }
                    }
                }
            }
            VaultAction::Tick => {
                transition.observe_context_alarm()?;
                let pause = i64::try_from(transition.read_pause()?.0)
                    .map_err(|_| VaultProgramError::ControllerDomain)?;
                let must_serve = transition.read_must_serve()?.0;
                let priority = transition.read_priority()?;
                // The step's inputs, in the order `synthesis.json` declares:
                // the controller's memory, then what the tick brings.
                let flag = i64::from;
                let step = synthesized::transition(&[
                    flag(lane_a == LaneStatus::Pending),
                    flag(lane_b == LaneStatus::Pending),
                    pause,
                    flag(must_serve),
                    flag(priority == Lane::B),
                    flag(lane_a == LaneStatus::Arrived),
                    flag(lane_b == LaneStatus::Arrived),
                    flag(context.alarm.0),
                ])
                .ok_or(VaultProgramError::ControllerDomain)?;
                let [
                    decision,
                    pending_a,
                    pending_b,
                    next_pause,
                    next_must_serve,
                    priority_b,
                ] = step;
                let paid = if decision == i64::from(controller::PAY_A) {
                    Some((Lane::A, amount_a))
                } else if decision == i64::from(controller::PAY_B) {
                    Some((Lane::B, amount_b))
                } else {
                    None
                };
                if let Some((lane, paid_amount)) = paid {
                    transition.update_balance(&Money(balance - paid_amount))?;
                    match lane {
                        Lane::A => transition.update_amount_a(&Money(0))?,
                        Lane::B => transition.update_amount_b(&Money(0))?,
                    };
                    transition.enqueue_channel_300(
                        0,
                        &PayoutDestination(SETTLEMENT.into()),
                        &PayoutRequest {
                            paid_lane: lane,
                            paid_amount: Amount(paid_amount),
                        },
                    )?;
                }
                transition.update_lane_a(&lane_status(pending_a))?;
                transition.update_lane_b(&lane_status(pending_b))?;
                transition.update_pause(&PauseTicks(i128::from(next_pause)))?;
                transition.update_must_serve(&Flag(next_must_serve == 1))?;
                transition.update_priority(&if priority_b == 1 { Lane::B } else { Lane::A })?;
            }
        }
        Ok(transition.seal()?)
    }
}
