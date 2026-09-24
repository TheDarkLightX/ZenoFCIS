//! Reviewed, hand-written state machine for one order.
//!
//! Each command's context names who sent it. A command is decided from the
//! order's current status, and a payment provider's callback must name the
//! current attempt. A repeated or late callback therefore finds the order
//! moved on, or names an older attempt, and is rejected instead of acting
//! twice or on the wrong attempt.

use crate::{bindings::*, generated::*, profile};
use zeno_fcis_authority::{CatalogTransitionProgram, ReviewedTransitionInput};
use zeno_fcis_codec::Hash32;
use zeno_fcis_core::BudgetUsed;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_transition::TransitionDecision;

/// Payment attempts an order may start.
pub const MAX_PAYMENT_ATTEMPTS: i128 = 3;
/// Where payment requests are delivered.
pub const PAYMENT_PROVIDER: &str = "payment-provider";
/// Where shipping requests are delivered.
pub const CARRIER: &str = "carrier";

/// The only caller allowed to send each action.
pub fn sender(action: &OrderAction) -> Caller {
    match action {
        OrderAction::Checkout | OrderAction::CancelOrder => Caller::Customer,
        OrderAction::PaymentCaptured | OrderAction::PaymentDeclined => Caller::PaymentProvider,
        OrderAction::ParcelDispatched | OrderAction::ParcelDelivered => Caller::Carrier,
    }
}

pub struct OrderProgram;

impl CatalogTransitionProgram<RustCryptoSha256> for OrderProgram {
    type Error = GeneratedProjectError;

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
        if context.caller != sender(&command.action) {
            transition.require(false, RejectReasonId::Reason200)?;
            return transition.seal();
        }
        let status = transition.read_status()?;
        let attempts = transition.read_payment_attempts()?.0;
        match (&command.action, &status) {
            (
                OrderAction::PaymentCaptured | OrderAction::PaymentDeclined,
                OrderStatus::AwaitingPayment,
            ) if command.callback_attempt.0 != attempts => {
                // An answer about an earlier attempt must not settle this one.
                transition.require(false, RejectReasonId::Reason202)?;
            }
            (OrderAction::Checkout, OrderStatus::Placed) if attempts == MAX_PAYMENT_ATTEMPTS => {
                transition.require(false, RejectReasonId::Reason203)?;
            }
            (OrderAction::Checkout, OrderStatus::Placed) => {
                // Each attempt has its own number, which the provider uses
                // as the idempotency key for this capture.
                let attempt = Attempts(attempts + 1);
                transition.update_status(&OrderStatus::AwaitingPayment)?;
                transition.update_payment_attempts(&attempt)?;
                transition.enqueue_channel_300(
                    0,
                    &PaymentDestination(PAYMENT_PROVIDER.into()),
                    &PaymentRequest {
                        request_attempt: attempt,
                        request_action: PaymentAction::Capture,
                    },
                )?;
            }
            (OrderAction::PaymentCaptured, OrderStatus::AwaitingPayment) => {
                transition.update_status(&OrderStatus::Paid)?;
                transition.enqueue_channel_301(
                    0,
                    &CarrierDestination(CARRIER.into()),
                    &ShippingRequest {
                        paid_attempt: Attempts(attempts),
                    },
                )?;
            }
            (OrderAction::PaymentDeclined, OrderStatus::AwaitingPayment) => {
                // The decline is a fact worth keeping, so it commits as a
                // failure: the order returns to placed for another attempt.
                transition.update_status(&OrderStatus::Placed)?;
                transition.fail_if(true, CommittedFailureReasonId::Reason204)?;
            }
            (OrderAction::ParcelDispatched, OrderStatus::Paid) => {
                transition.update_status(&OrderStatus::Shipped)?;
            }
            (OrderAction::ParcelDelivered, OrderStatus::Shipped) => {
                transition.update_status(&OrderStatus::Delivered)?;
            }
            (OrderAction::CancelOrder, OrderStatus::Placed) => {
                transition.update_status(&OrderStatus::Cancelled)?;
            }
            (OrderAction::CancelOrder, OrderStatus::AwaitingPayment) => {
                // The capture may still be pending at the provider, so the
                // cancellation voids that exact attempt.
                transition.update_status(&OrderStatus::Cancelled)?;
                transition.enqueue_channel_300(
                    0,
                    &PaymentDestination(PAYMENT_PROVIDER.into()),
                    &PaymentRequest {
                        request_attempt: Attempts(attempts),
                        request_action: PaymentAction::Void,
                    },
                )?;
            }
            _ => {
                transition.require(false, RejectReasonId::Reason201)?;
            }
        }
        transition.seal()
    }
}
