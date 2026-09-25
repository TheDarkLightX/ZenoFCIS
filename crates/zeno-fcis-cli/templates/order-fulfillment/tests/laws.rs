//! Calls the law checker directly with decisions a faulty program could
//! produce; authorization separately validates patch consistency.
use order_fulfillment::{command, generated::*, laws::OrderLaws, profile};
use zeno_fcis_laws::*;
use zeno_fcis_patch::CanonicalPatch;
use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan};
use zeno_fcis_value::Value;

const CHECKOUT_REQUESTS_PAYMENT: u32 = 501;

fn order(status: OrderStatus, attempts: i128) -> Order {
    Order {
        status,
        payment_attempts: Attempts(attempts),
    }
}

fn payment(destination: &str, attempt: i128, action: PaymentAction) -> OutboxEntry {
    OutboxEntry::new(
        0,
        300,
        PaymentDestination(destination.into()).to_value().unwrap(),
        PaymentRequest {
            request_attempt: Attempts(attempt),
            request_action: action,
        }
        .to_value()
        .unwrap(),
    )
}

fn shipping(attempt: i128) -> OutboxEntry {
    OutboxEntry::new(
        0,
        301,
        CarrierDestination("carrier".into()).to_value().unwrap(),
        ShippingRequest {
            paid_attempt: Attempts(attempt),
        }
        .to_value()
        .unwrap(),
    )
}

/// A committing decision, as the checker sees it.
struct Decided {
    pre: Order,
    post: Order,
    command: OrderCommand,
    caller: Caller,
    requests: Vec<OutboxEntry>,
    failure: Option<u32>,
    effect: bool,
}

fn decided(
    pre: Order,
    post: Order,
    command: OrderCommand,
    caller: Caller,
    requests: Vec<OutboxEntry>,
) -> Decided {
    Decided {
        pre,
        post,
        command,
        caller,
        requests,
        failure: None,
        effect: false,
    }
}

fn holds(decided: Decided) -> bool {
    let hash = profile::digest("example/test/law", b"independent decision view");
    let pre = decided.pre.to_value().unwrap();
    let post = decided.post.to_value().unwrap();
    let command = decided.command.to_value().unwrap();
    let context = CallerContext {
        caller: decided.caller,
    }
    .to_value()
    .unwrap();
    let outbox = OutboxPlan::try_new(decided.requests).unwrap();
    let commit = CommitPlan::try_new(if decided.effect {
        vec![Effect::new(0, 999, hash, hash, Value::I128(0))]
    } else {
        vec![]
    })
    .unwrap();
    let patch = CanonicalPatch::try_new(100, hash, vec![]).unwrap();
    let decision = match decided.failure {
        None => LawDecisionView::Accept {
            post_state: &post,
            patch: &patch,
            commit_plan: &commit,
            outbox_plan: &outbox,
        },
        Some(reason_id) => LawDecisionView::CommittedFailure {
            reason_id,
            post_state: &post,
            patch: &patch,
            commit_plan: &commit,
            outbox_plan: &outbox,
        },
    };
    let input = LawCheckInput::try_new(hash, hash, &pre, &command, &context, decision).unwrap();
    OrderLaws::default()
        .evaluate(&input, LawLimits::default())
        .unwrap()
        .iter()
        .all(|observation| observation.status() == LawStatus::Satisfied)
}

#[test]
fn checkout_must_start_one_numbered_capture() {
    let checkout = |post: i128, requests| {
        decided(
            order(OrderStatus::Placed, 1),
            order(OrderStatus::AwaitingPayment, post),
            command(OrderAction::Checkout, 0),
            Caller::Customer,
            requests,
        )
    };
    let capture = |attempt| vec![payment("payment-provider", attempt, PaymentAction::Capture)];
    assert!(holds(checkout(2, capture(2))));
    assert!(!holds(checkout(1, capture(1))));
    assert!(!holds(checkout(2, capture(1))));
    assert!(!holds(checkout(2, vec![])));
    assert!(!holds(checkout(
        2,
        vec![payment("someone-else", 2, PaymentAction::Capture)]
    )));
    assert!(!holds(checkout(
        2,
        vec![payment("payment-provider", 2, PaymentAction::Void)]
    )));
    let mut effect = checkout(2, capture(2));
    effect.effect = true;
    assert!(!holds(effect));
}

#[test]
fn only_the_provider_pays_the_current_attempt_and_payment_requests_shipping() {
    let pay = |attempt, caller, requests| {
        decided(
            order(OrderStatus::AwaitingPayment, 2),
            order(OrderStatus::Paid, 2),
            command(OrderAction::PaymentCaptured, attempt),
            caller,
            requests,
        )
    };
    assert!(holds(pay(2, Caller::PaymentProvider, vec![shipping(2)])));
    assert!(!holds(pay(2, Caller::Customer, vec![shipping(2)])));
    // A capture that names an earlier attempt must not pay this one.
    assert!(!holds(pay(1, Caller::PaymentProvider, vec![shipping(2)])));
    assert!(!holds(pay(2, Caller::PaymentProvider, vec![])));
    assert!(!holds(pay(2, Caller::PaymentProvider, vec![shipping(1)])));
    // A capture that arrives after payment must not ship twice.
    assert!(!holds(decided(
        order(OrderStatus::Paid, 2),
        order(OrderStatus::Paid, 2),
        command(OrderAction::PaymentCaptured, 2),
        Caller::PaymentProvider,
        vec![shipping(2)],
    )));
}

#[test]
fn declines_cancellations_and_carrier_steps_are_checked() {
    let decline = |attempt, reason| {
        let mut decision = decided(
            order(OrderStatus::AwaitingPayment, 2),
            order(OrderStatus::Placed, 2),
            command(OrderAction::PaymentDeclined, attempt),
            Caller::PaymentProvider,
            vec![],
        );
        decision.failure = reason;
        decision
    };
    assert!(holds(decline(2, Some(204))));
    assert!(!holds(decline(2, Some(203))));
    // A decline of an earlier attempt must not reopen the order.
    assert!(!holds(decline(1, Some(204))));
    // A decline is never accepted.
    assert!(!holds(decline(2, None)));
    let cancel = |requests| {
        decided(
            order(OrderStatus::AwaitingPayment, 2),
            order(OrderStatus::Cancelled, 2),
            command(OrderAction::CancelOrder, 0),
            Caller::Customer,
            requests,
        )
    };
    assert!(holds(cancel(vec![payment(
        "payment-provider",
        2,
        PaymentAction::Void
    )])));
    assert!(!holds(cancel(vec![])));
    assert!(!holds(cancel(vec![payment(
        "payment-provider",
        1,
        PaymentAction::Void
    )])));
    assert!(!holds(decided(
        order(OrderStatus::Paid, 1),
        order(OrderStatus::Cancelled, 1),
        command(OrderAction::CancelOrder, 0),
        Caller::Customer,
        vec![],
    )));
    assert!(!holds(decided(
        order(OrderStatus::Placed, 1),
        order(OrderStatus::Shipped, 1),
        command(OrderAction::ParcelDispatched, 0),
        Caller::Carrier,
        vec![],
    )));
    assert!(holds(decided(
        order(OrderStatus::Shipped, 1),
        order(OrderStatus::Delivered, 1),
        command(OrderAction::ParcelDelivered, 0),
        Caller::Carrier,
        vec![],
    )));
}

#[test]
fn rejections_must_carry_the_rule_that_applies() {
    let hash = profile::digest("example/test/reject", b"independent decision view");
    let cases = [
        (
            OrderStatus::Placed,
            0,
            OrderAction::PaymentCaptured,
            0,
            Caller::Customer,
            200,
            true,
        ),
        (
            OrderStatus::Placed,
            0,
            OrderAction::PaymentCaptured,
            0,
            Caller::Customer,
            201,
            false,
        ),
        (
            OrderStatus::Paid,
            1,
            OrderAction::PaymentCaptured,
            1,
            Caller::PaymentProvider,
            201,
            true,
        ),
        (
            OrderStatus::AwaitingPayment,
            2,
            OrderAction::PaymentDeclined,
            1,
            Caller::PaymentProvider,
            202,
            true,
        ),
        (
            OrderStatus::AwaitingPayment,
            2,
            OrderAction::PaymentDeclined,
            2,
            Caller::PaymentProvider,
            202,
            false,
        ),
        (
            OrderStatus::Placed,
            3,
            OrderAction::Checkout,
            0,
            Caller::Customer,
            203,
            true,
        ),
        (
            OrderStatus::Placed,
            2,
            OrderAction::Checkout,
            0,
            Caller::Customer,
            203,
            false,
        ),
    ];
    for (status, attempts, action, callback_attempt, caller, reason_id, right) in cases {
        let pre = order(status, attempts).to_value().unwrap();
        let command = command(action, callback_attempt).to_value().unwrap();
        let context = CallerContext { caller }.to_value().unwrap();
        let input = LawCheckInput::try_new(
            hash,
            hash,
            &pre,
            &command,
            &context,
            LawDecisionView::Reject { reason_id },
        )
        .unwrap();
        let result = OrderLaws::default().evaluate(&input, LawLimits::default());
        assert_eq!(result.is_ok(), right, "reason {reason_id}: {result:?}");
    }
}

#[test]
fn genesis_and_checker_limits_fail_closed() {
    let hash = profile::digest("example/test/genesis", b"exact initial state");
    for (state, expected) in [
        (order(OrderStatus::Placed, 0), true),
        (order(OrderStatus::Placed, 1), false),
        (order(OrderStatus::Paid, 1), false),
    ] {
        let value = state.to_value().unwrap();
        let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
        let observations = OrderLaws::default()
            .evaluate_genesis(&input, LawLimits::default())
            .unwrap();
        assert_eq!(
            observations
                .iter()
                .all(|observation| observation.status() == LawStatus::Satisfied),
            expected
        );
    }
    let value = order(OrderStatus::Placed, 0).to_value().unwrap();
    let input = GenesisLawCheckInput::try_new(hash, hash, hash, &value).unwrap();
    let limits = LawLimits {
        max_observations: 0,
        ..LawLimits::default()
    };
    assert_eq!(
        OrderLaws::default().evaluate_genesis(&input, limits),
        Err(LawEngineFailure::Incomplete)
    );
}

/// `profile::manifest()` with law 501 bound as `rebind` says, instead of as
/// `profile.rs` binds it.
fn rebound(
    rebind: impl Fn(&LawDefinition) -> (DecisionScope, GenesisApplicability),
) -> LawManifest {
    let manifest = profile::manifest();
    let law = profile::id(CHECKOUT_REQUESTS_PAYMENT);
    let definitions = manifest
        .definitions()
        .iter()
        .map(|definition| {
            if definition.id() != law {
                return definition.clone();
            }
            let (scope, genesis) = rebind(definition);
            LawDefinition::try_new(
                definition.id(),
                definition.name().clone(),
                definition.kind(),
                scope,
                genesis,
                definition.claim_hash(),
                definition.checker_profile_hash(),
                definition.evidence_requirement(),
            )
            .unwrap()
        })
        .collect();
    LawManifest::try_new(manifest.families().to_vec(), definitions).unwrap()
}

#[test]
fn the_manifest_enforces_the_scopes_the_project_declares() {
    let project = profile::project();
    // Every law with a formula declares its scope, so each one is compared.
    assert!(
        project
            .laws()
            .iter()
            .all(|law| law.applicability().is_some())
    );
    assert_eq!(profile::manifest().check_declared_scopes(&project), Ok(()));
    // Law 501 is declared `on accept`, without `, genesis`: a manifest that
    // enforced it on every commit, or applied it at genesis, is reported.
    let law = profile::id(CHECKOUT_REQUESTS_PAYMENT);
    let elsewhere = rebound(|shipped| (DecisionScope::Committing, shipped.genesis_applicability()));
    assert_eq!(
        elsewhere.check_declared_scopes(&project),
        Err(vec![ScopeMismatch::Scope {
            law,
            declared: DecisionScope::Accept,
            enforced: DecisionScope::Committing,
        }])
    );
    let at_genesis = rebound(|shipped| (shipped.scope(), GenesisApplicability::Required));
    assert_eq!(
        at_genesis.check_declared_scopes(&project),
        Err(vec![ScopeMismatch::Genesis {
            law,
            declared: false
        }])
    );
}
