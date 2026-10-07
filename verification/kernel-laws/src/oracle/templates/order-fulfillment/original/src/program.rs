//! Reviewed adapter around the synthesized finite order decision and successor state.
//!
//! Each command's context names who sent it. A command is decided from the
//! order's current status, and a payment provider's callback must name the
//! current attempt. A repeated or late callback therefore finds the order
//! moved on, or names an older attempt, and is rejected instead of acting
//! twice or on the wrong attempt.

use crate::legacy::synthesized;
use crate::{bindings::*, generated::*, profile};
use zeno_fcis_authority::{CatalogTransitionProgram, ReviewedTransitionInput};
use zeno_fcis_codec::Hash32;
use zeno_fcis_core::BudgetUsed;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_transition::TransitionDecision;

/// Where payment requests are delivered.
pub const PAYMENT_PROVIDER: &str = "payment-provider";
/// Where shipping requests are delivered.
pub const CARRIER: &str = "carrier";

fn action_code(action: &OrderAction) -> i64 {
    match action {
        OrderAction::Checkout => 0,
        OrderAction::PaymentCaptured => 1,
        OrderAction::PaymentDeclined => 2,
        OrderAction::ParcelDispatched => 3,
        OrderAction::ParcelDelivered => 4,
        OrderAction::CancelOrder => 5,
    }
}

fn status_code(status: &OrderStatus) -> i64 {
    match status {
        OrderStatus::Placed => 0,
        OrderStatus::AwaitingPayment => 1,
        OrderStatus::Paid => 2,
        OrderStatus::Shipped => 3,
        OrderStatus::Delivered => 4,
        OrderStatus::Cancelled => 5,
    }
}

fn caller_code(caller: &Caller) -> i64 {
    match caller {
        Caller::Customer => 0,
        Caller::PaymentProvider => 1,
        Caller::Carrier => 2,
    }
}

fn status_from_code(code: i64) -> Result<OrderStatus, OrderProgramError> {
    match code {
        0 => Ok(OrderStatus::Placed),
        1 => Ok(OrderStatus::AwaitingPayment),
        2 => Ok(OrderStatus::Paid),
        3 => Ok(OrderStatus::Shipped),
        4 => Ok(OrderStatus::Delivered),
        5 => Ok(OrderStatus::Cancelled),
        _ => Err(OrderProgramError::SynthesisDomain),
    }
}

pub struct OrderProgram;

#[derive(Debug)]
pub enum OrderProgramError {
    Project(Box<GeneratedProjectError>),
    SynthesisDomain,
}

impl From<GeneratedProjectError> for OrderProgramError {
    fn from(error: GeneratedProjectError) -> Self {
        Self::Project(Box::new(error))
    }
}

impl From<AdapterError> for OrderProgramError {
    fn from(error: AdapterError) -> Self {
        GeneratedProjectError::from(error).into()
    }
}

impl CatalogTransitionProgram<RustCryptoSha256> for OrderProgram {
    type Error = OrderProgramError;

    fn transition_build_hash(&self) -> Hash32 {
        profile::program_hash()
    }

    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error> {
        let project = GeneratedProject::try_new::<RustCryptoSha256>()?;
        let command = OrderCommand::try_from_value(input.command().value().value().clone())?;
        let context = CallerContext::try_from_value(input.context().value().value().clone())?;
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
        let status = transition.read_status()?;
        let attempts = transition.read_payment_attempts()?.0;
        let attempts_i64 =
            i64::try_from(attempts).map_err(|_| OrderProgramError::SynthesisDomain)?;
        let output = synthesized::transition(&[
            action_code(&command.action),
            status_code(&status),
            attempts_i64,
            i64::try_from(command.callback_attempt.0)
                .map_err(|_| OrderProgramError::SynthesisDomain)?,
            caller_code(&context.caller),
        ])
        .ok_or(OrderProgramError::SynthesisDomain)?;
        let post_status = status_from_code(output[1])?;
        let post_attempts = Attempts(i128::from(output[2]));
        match output[0] {
            0 => {
                transition.require(false, RejectReasonId::Reason200)?;
            }
            1 => {
                transition.require(false, RejectReasonId::Reason201)?;
            }
            2 => {
                transition.require(false, RejectReasonId::Reason202)?;
            }
            3 => {
                transition.require(false, RejectReasonId::Reason203)?;
            }
            4..=10 => {
                transition.update_status(&post_status)?;
                if output[2] != attempts_i64 {
                    transition.update_payment_attempts(&post_attempts)?;
                }
                match output[0] {
                    4 => {
                        // The synthesized successor count is this request's
                        // idempotency number at the provider.
                        transition.enqueue_channel_300(
                            0,
                            &PaymentDestination(PAYMENT_PROVIDER.into()),
                            &PaymentRequest {
                                request_attempt: post_attempts,
                                request_action: PaymentAction::Capture,
                            },
                        )?;
                    }
                    5 => {
                        transition.enqueue_channel_301(
                            0,
                            &CarrierDestination(CARRIER.into()),
                            &ShippingRequest {
                                paid_attempt: post_attempts,
                            },
                        )?;
                    }
                    6 => {
                        // The decline is a fact worth keeping, so it commits
                        // as a failure and reopens checkout.
                        transition.fail_if(true, CommittedFailureReasonId::Reason204)?;
                    }
                    10 => {
                        // Void the exact attempt that was pending.
                        transition.enqueue_channel_300(
                            0,
                            &PaymentDestination(PAYMENT_PROVIDER.into()),
                            &PaymentRequest {
                                request_attempt: post_attempts,
                                request_action: PaymentAction::Void,
                            },
                        )?;
                    }
                    7..=9 => {}
                    _ => return Err(OrderProgramError::SynthesisDomain),
                }
            }
            _ => return Err(OrderProgramError::SynthesisDomain),
        }
        Ok(transition.seal()?)
    }
}
