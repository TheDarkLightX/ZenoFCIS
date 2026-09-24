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

/// Payment attempts an order may start.
const ATTEMPT_LIMIT: i128 = 3;
/// The only destination payment requests may go to.
const PAYMENTS_TO: &str = "payment-provider";
/// The only destination shipping requests may go to.
const SHIPPING_TO: &str = "carrier";

pub struct OrderLaws {
    project: ProjectSpec,
}

impl Default for OrderLaws {
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
            "example/order-fulfillment/verifier",
            b"External proofs are not admitted",
        )
    }
    fn verify(&self, _: &LawProofSubject, _: &EvidenceEnvelope, _: &[u8]) -> LawProofDecision {
        LawProofDecision::Indeterminate
    }
}

/// The variant ID that `command.101.125` observes.
fn action_id(action: &OrderAction) -> i128 {
    match action {
        OrderAction::Checkout => 150,
        OrderAction::PaymentCaptured => 151,
        OrderAction::PaymentDeclined => 152,
        OrderAction::ParcelDispatched => 153,
        OrderAction::ParcelDelivered => 154,
        OrderAction::CancelOrder => 155,
    }
}

/// The variant ID that `pre.100.120` and `post.100.120` observe.
fn status_id(status: &OrderStatus) -> i128 {
    match status {
        OrderStatus::Placed => 160,
        OrderStatus::AwaitingPayment => 161,
        OrderStatus::Paid => 162,
        OrderStatus::Shipped => 163,
        OrderStatus::Delivered => 164,
        OrderStatus::Cancelled => 165,
    }
}

/// The variant ID that `context.102.130` observes.
fn caller_id(caller: &Caller) -> i128 {
    match caller {
        Caller::Customer => 170,
        Caller::PaymentProvider => 171,
        Caller::Carrier => 172,
    }
}

fn observations(
    pre: &Order,
    post: &Order,
    command: &OrderCommand,
    context: &CallerContext,
) -> TraceStep {
    let observe = |root, raw: &[u32], value| {
        Observation::new(
            ProjectionPath::try_new(
                root,
                raw.iter()
                    .map(|n| StableId::new(*n).expect("static ID"))
                    .collect(),
            )
            .expect("static path"),
            value,
        )
    };
    TraceStep::try_new(vec![
        observe(ProjectionRoot::Pre, &[100, 120], status_id(&pre.status)),
        observe(ProjectionRoot::Pre, &[100, 121], pre.payment_attempts.0),
        observe(ProjectionRoot::Post, &[100, 120], status_id(&post.status)),
        observe(ProjectionRoot::Post, &[100, 121], post.payment_attempts.0),
        observe(
            ProjectionRoot::Command,
            &[101, 125],
            action_id(&command.action),
        ),
        observe(
            ProjectionRoot::Command,
            &[101, 126],
            command.callback_attempt.0,
        ),
        observe(
            ProjectionRoot::Context,
            &[102, 130],
            caller_id(&context.caller),
        ),
    ])
    .expect("distinct reviewed projections")
}

/// The caller the README assigns to each action.
fn required_caller(action: &OrderAction) -> Caller {
    match action {
        OrderAction::Checkout | OrderAction::CancelOrder => Caller::Customer,
        OrderAction::PaymentCaptured | OrderAction::PaymentDeclined => Caller::PaymentProvider,
        OrderAction::ParcelDispatched | OrderAction::ParcelDelivered => Caller::Carrier,
    }
}

/// Whether the README lets `action` start from `status`.
fn allowed_from(action: &OrderAction, status: &OrderStatus) -> bool {
    matches!(
        (action, status),
        (OrderAction::Checkout, OrderStatus::Placed)
            | (
                OrderAction::PaymentCaptured | OrderAction::PaymentDeclined,
                OrderStatus::AwaitingPayment
            )
            | (OrderAction::ParcelDispatched, OrderStatus::Paid)
            | (OrderAction::ParcelDelivered, OrderStatus::Shipped)
            | (
                OrderAction::CancelOrder,
                OrderStatus::Placed | OrderStatus::AwaitingPayment
            )
    )
}

/// The rejection reason the README requires, or `None` if the input must commit.
fn expected_rejection(pre: &Order, command: &OrderCommand, context: &CallerContext) -> Option<u32> {
    let action = &command.action;
    let callback = matches!(
        action,
        OrderAction::PaymentCaptured | OrderAction::PaymentDeclined
    );
    if context.caller != required_caller(action) {
        Some(200)
    } else if !allowed_from(action, &pre.status) {
        Some(201)
    } else if callback && command.callback_attempt != pre.payment_attempts {
        Some(202)
    } else if *action == OrderAction::Checkout && pre.payment_attempts.0 == ATTEMPT_LIMIT {
        Some(203)
    } else {
        None
    }
}

/// The channel, destination, and payload of the one request a committing
/// decision must queue, if any.
fn expected_request(
    pre: &Order,
    post: &Order,
    command: &OrderCommand,
) -> Result<Option<(u32, Value, Value)>, LawEngineFailure> {
    let unsupported = |_| LawEngineFailure::Unsupported;
    let payment = |attempt: Attempts, action: PaymentAction| -> Result<_, LawEngineFailure> {
        Ok(Some((
            300,
            PaymentDestination(PAYMENTS_TO.into())
                .to_value()
                .map_err(unsupported)?,
            PaymentRequest {
                request_attempt: attempt,
                request_action: action,
            }
            .to_value()
            .map_err(unsupported)?,
        )))
    };
    match command.action {
        OrderAction::Checkout => payment(post.payment_attempts, PaymentAction::Capture),
        OrderAction::CancelOrder if pre.status == OrderStatus::AwaitingPayment => {
            payment(pre.payment_attempts, PaymentAction::Void)
        }
        OrderAction::PaymentCaptured => Ok(Some((
            301,
            CarrierDestination(SHIPPING_TO.into())
                .to_value()
                .map_err(unsupported)?,
            ShippingRequest {
                paid_attempt: post.payment_attempts,
            }
            .to_value()
            .map_err(unsupported)?,
        ))),
        _ => Ok(None),
    }
}

impl OrderLaws {
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
            profile::digest("example/order-fulfillment/observation", &witness),
        )
        .map_err(|_| LawEngineFailure::InvalidOutput)
    }
}

impl ProjectLawEngine for OrderLaws {
    fn evaluate(
        &self,
        input: &LawCheckInput<'_>,
        limits: LawLimits,
    ) -> Result<Vec<LawObservation>, LawEngineFailure> {
        if limits.max_observations < 6 {
            return Err(LawEngineFailure::Incomplete);
        }
        let pre = Order::try_from_value(input.pre_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let command = OrderCommand::try_from_value(input.command().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let context = CallerContext::try_from_value(input.context().clone())
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
                    &[501, 502, 503, 504, 505],
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
                    &[506],
                    *reason_id == 204,
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
            Order::try_from_value((*post).clone()).map_err(|_| LawEngineFailure::Unsupported)?;
        let committed_while_rejectable = expected_rejection(&pre, &command, &context).is_some();
        let delivery_ok = match (outbox.entries(), expected_request(&pre, &post, &command)?) {
            ([], None) => true,
            ([entry], Some((channel, destination, payload))) => {
                entry.ordinal() == 0
                    && entry.channel() == channel
                    && entry.destination() == &destination
                    && entry.payload() == &payload
            }
            _ => false,
        };
        let extra =
            reason_ok && !committed_while_rejectable && delivery_ok && commit.effects().is_empty();
        let step = observations(&pre, &post, &command, &context);
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
        let state = Order::try_from_value(input.initial_state().clone())
            .map_err(|_| LawEngineFailure::Unsupported)?;
        let step = observations(
            &state,
            &state,
            &OrderCommand {
                action: OrderAction::Checkout,
                callback_attempt: Attempts(0),
            },
            &CallerContext {
                caller: Caller::Customer,
            },
        );
        let bytes = input
            .canonical_bytes()
            .map_err(|_| LawEngineFailure::InvalidOutput)?;
        let placed = state.status == OrderStatus::Placed && state.payment_attempts.0 == 0;
        Ok(vec![self.check(500, &step, placed, &bytes)?])
    }
}
