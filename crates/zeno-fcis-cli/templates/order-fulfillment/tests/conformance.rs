//! Checks the executed application against the decision examples and against
//! a reference model of the README's rules on every reachable input.
//!
//! Every decision runs through the application: schema admission, the
//! authority, the synthesized decision and its reviewed adapter, the law checker,
//! the committed patch, and the outbox plan. Fields are read by the numeric IDs
//! in `project.zeno`, not through the generated name bindings, so a binding
//! that swapped two fields or variants fails here.
//!
//! A payment provider's callback names the attempt it answers. The reachable
//! states are found by exploring the executed application from genesis. The
//! reference model restates the README's rules; its author also wrote the
//! program, so it catches binding and adapter errors, not a misreading shared
//! by both. The examples file, once reviewed by the project's owner, is the
//! check on that.

use order_fulfillment::{
    Authority, authority, bindings::GeneratedProject, command, generated::*, profile,
};
use std::collections::{BTreeMap, BTreeSet};
use zeno_fcis_codec::Domain;
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_value::Value;

const EXAMPLES: &str = include_str!("decision-examples.txt");
const CHECKOUT: u16 = 150;
const CAPTURED: u16 = 151;
const DECLINED: u16 = 152;
const DISPATCHED: u16 = 153;
const DELIVERED_ACTION: u16 = 154;
const CANCEL: u16 = 155;
const PLACED: u16 = 160;
const AWAITING: u16 = 161;
const PAID: u16 = 162;
const SHIPPED: u16 = 163;
const DELIVERED: u16 = 164;
const CANCELLED: u16 = 165;
const CUSTOMER: u16 = 170;
const PROVIDER: u16 = 171;
const CARRIER: u16 = 172;
const CAPTURE: u16 = 175;
const VOID: u16 = 176;
const ACTIONS: [u16; 6] = [
    CHECKOUT,
    CAPTURED,
    DECLINED,
    DISPATCHED,
    DELIVERED_ACTION,
    CANCEL,
];
const STATUSES: [u16; 6] = [PLACED, AWAITING, PAID, SHIPPED, DELIVERED, CANCELLED];
const CALLERS: [u16; 3] = [CUSTOMER, PROVIDER, CARRIER];

/// Field 120 status (a variant ID) and field 121 payment_attempts.
type State = (u16, i128);
/// Command field 125 action (a variant ID) and field 126 callback_attempt.
type Input = (u16, i128);

/// One decision, in the numeric IDs of `project.zeno`.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Outcome {
    /// `accept`, `reject`, or `failure`.
    kind: &'static str,
    /// The rejection or committed-failure reason.
    reason: Option<u32>,
    /// The state after the decision.
    post: State,
    /// Each queued request: channel, attempt, and the payment action's variant
    /// ID (0 on the shipping channel).
    requests: Vec<(u32, i128, u16)>,
}

fn record_field(value: &Value, id: u16) -> &Value {
    let Value::Record(fields) = value else {
        panic!("record expected, got {value:?}");
    };
    fields
        .iter()
        .find(|field| field.id() == id)
        .unwrap_or_else(|| panic!("field {id} missing from {value:?}"))
        .value()
}

fn int(value: &Value, id: u16) -> i128 {
    match record_field(value, id) {
        Value::I128(value) => *value,
        other => panic!("field {id} holds {other:?}"),
    }
}

/// The variant ID of an enum or sum value.
fn variant(value: &Value) -> u16 {
    match value {
        Value::Enum { variant, .. } | Value::Sum { variant, .. } => *variant,
        other => panic!("variant expected, got {other:?}"),
    }
}

fn action(id: u16) -> OrderAction {
    match id {
        CHECKOUT => OrderAction::Checkout,
        CAPTURED => OrderAction::PaymentCaptured,
        DECLINED => OrderAction::PaymentDeclined,
        DISPATCHED => OrderAction::ParcelDispatched,
        DELIVERED_ACTION => OrderAction::ParcelDelivered,
        CANCEL => OrderAction::CancelOrder,
        other => panic!("unknown action variant {other}"),
    }
}

fn status(id: u16) -> OrderStatus {
    match id {
        PLACED => OrderStatus::Placed,
        AWAITING => OrderStatus::AwaitingPayment,
        PAID => OrderStatus::Paid,
        SHIPPED => OrderStatus::Shipped,
        DELIVERED => OrderStatus::Delivered,
        CANCELLED => OrderStatus::Cancelled,
        other => panic!("unknown status variant {other}"),
    }
}

fn caller(id: u16) -> Caller {
    match id {
        CUSTOMER => Caller::Customer,
        PROVIDER => Caller::PaymentProvider,
        CARRIER => Caller::Carrier,
        other => panic!("unknown caller variant {other}"),
    }
}

fn order((status_id, attempts): State) -> Order {
    Order {
        status: status(status_id),
        payment_attempts: Attempts(attempts),
    }
}

/// Every command: each action with each attempt a callback could name.
fn inputs() -> Vec<Input> {
    ACTIONS
        .into_iter()
        .flat_map(|action_id| (0..=3).map(move |attempt| (action_id, attempt)))
        .collect()
}

/// Runs one command through the application from an admitted pre-state.
fn observe(
    authority: &Authority,
    pre: State,
    (action_id, attempt): Input,
    caller_id: u16,
) -> Outcome {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let root = project
        .admit_root::<RustCryptoSha256>(&order(pre), limits)
        .unwrap();
    let pre_value = root.value().value().clone();
    // The typed bindings must place each field and variant at its numeric ID.
    assert_eq!(
        (variant(record_field(&pre_value, 120)), int(&pre_value, 121)),
        pre
    );
    let admitted_command = project
        .admit_command::<RustCryptoSha256>(&command(action(action_id), attempt), limits)
        .unwrap();
    let command_value = admitted_command.admitted().value().value();
    assert_eq!(variant(record_field(command_value, 125)), action_id);
    assert_eq!(int(command_value, 126), attempt);
    let admitted_context = project
        .admit_context::<RustCryptoSha256>(
            &CallerContext {
                caller: caller(caller_id),
            },
            limits,
        )
        .unwrap();
    assert_eq!(
        variant(record_field(
            admitted_context.admitted().value().value(),
            130
        )),
        caller_id
    );
    let replay = format!("conformance {pre:?} {action_id} {attempt} {caller_id}");
    let witness = authority
        .admit_invocation(
            root,
            admitted_command.admitted().clone(),
            admitted_context.admitted().clone(),
            profile::digest("example/order-fulfillment/principal", b"conformance"),
            profile::digest("example/order-fulfillment/authentication", b"conformance"),
            profile::digest("example/order-fulfillment/replay", replay.as_bytes()),
        )
        .unwrap();
    let domain = Domain::new("example/order-fulfillment/state", 1).unwrap();
    macro_rules! committed {
        ($kind:expr, $reason:expr, $candidate:expr) => {{
            let bundle = $candidate.bundle();
            assert!(bundle.commit_plan().effects().is_empty());
            let applied = bundle
                .patch()
                .apply::<RustCryptoSha256>(&pre_value, domain)
                .unwrap();
            let post = applied.state();
            Outcome {
                kind: $kind,
                reason: $reason,
                post: (variant(record_field(post, 120)), int(post, 121)),
                requests: bundle
                    .outbox_plan()
                    .entries()
                    .iter()
                    .map(|entry| match entry.channel() {
                        300 => (
                            300,
                            int(entry.payload(), 140),
                            variant(record_field(entry.payload(), 141)),
                        ),
                        301 => (301, int(entry.payload(), 145), 0),
                        other => panic!("unexpected channel {other}"),
                    })
                    .collect(),
            }
        }};
    }
    match authority.execute(witness).unwrap() {
        Decision::Reject(reject) => Outcome {
            kind: "reject",
            reason: Some(reject.reason().rejection().reason_id().get()),
            post: pre,
            requests: Vec::new(),
        },
        Decision::Accept(accepted) => {
            let candidate = accepted.into_candidate();
            committed!("accept", None, candidate)
        }
        Decision::CommittedFailure(failed) => {
            let (candidate, reason) = failed.into_parts();
            committed!("failure", Some(reason.get()), candidate)
        }
    }
}

/// The README's rules, restated independently of `src/program.rs`.
fn model(pre: State, (action_id, attempt): Input, caller_id: u16) -> Outcome {
    let (status_id, attempts) = pre;
    let reject = |reason| Outcome {
        kind: "reject",
        reason: Some(reason),
        post: pre,
        requests: Vec::new(),
    };
    let sender = match action_id {
        CHECKOUT | CANCEL => CUSTOMER,
        CAPTURED | DECLINED => PROVIDER,
        _ => CARRIER,
    };
    if caller_id != sender {
        return reject(200);
    }
    let accept = |post, requests| Outcome {
        kind: "accept",
        reason: None,
        post,
        requests,
    };
    match (action_id, status_id) {
        (CAPTURED | DECLINED, AWAITING) if attempt != attempts => reject(202),
        (CHECKOUT, PLACED) if attempts == 3 => reject(203),
        (CHECKOUT, PLACED) => accept((AWAITING, attempts + 1), vec![(300, attempts + 1, CAPTURE)]),
        (CAPTURED, AWAITING) => accept((PAID, attempts), vec![(301, attempts, 0)]),
        (DECLINED, AWAITING) => Outcome {
            kind: "failure",
            reason: Some(204),
            post: (PLACED, attempts),
            requests: Vec::new(),
        },
        (DISPATCHED, PAID) => accept((SHIPPED, attempts), Vec::new()),
        (DELIVERED_ACTION, SHIPPED) => accept((DELIVERED, attempts), Vec::new()),
        (CANCEL, PLACED) => accept((CANCELLED, attempts), Vec::new()),
        (CANCEL, AWAITING) => accept((CANCELLED, attempts), vec![(300, attempts, VOID)]),
        _ => reject(201),
    }
}

/// Explores the executed application from genesis and returns every state it
/// reaches, with the states each one can move to.
fn reachable(authority: &Authority) -> BTreeMap<State, BTreeSet<State>> {
    let mut graph = BTreeMap::new();
    let mut pending = vec![(PLACED, 0)];
    while let Some(state) = pending.pop() {
        if graph.contains_key(&state) {
            continue;
        }
        let mut next = BTreeSet::new();
        for input in inputs() {
            for caller_id in CALLERS {
                let outcome = observe(authority, state, input, caller_id);
                if outcome.kind != "reject" {
                    next.insert(outcome.post);
                    pending.push(outcome.post);
                }
            }
        }
        graph.insert(state, next);
    }
    graph
}

#[test]
fn every_reachable_input_matches_the_reference_model() {
    let authority = authority().unwrap();
    let graph = reachable(&authority);
    // Law 500: an order awaiting payment, paid, shipped, or delivered has made
    // a payment attempt. The reachable states are exactly the admitted states
    // that satisfy it.
    let lawful: BTreeSet<State> = STATUSES
        .into_iter()
        .flat_map(|status_id| (0..=3).map(move |attempts| (status_id, attempts)))
        .filter(|(status_id, attempts)| {
            !matches!(*status_id, AWAITING | PAID | SHIPPED | DELIVERED) || *attempts >= 1
        })
        .collect();
    assert_eq!(graph.keys().copied().collect::<BTreeSet<_>>(), lawful);
    assert_eq!(lawful.len(), 20);
    let mut checked = 0;
    for state in graph.keys() {
        for input in inputs() {
            for caller_id in CALLERS {
                assert_eq!(
                    observe(&authority, *state, input, caller_id),
                    model(*state, input, caller_id),
                    "state {state:?} command {input:?} caller {caller_id}"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 1440);
}

#[test]
fn every_reachable_order_can_still_finish() {
    let authority = authority().unwrap();
    let graph = reachable(&authority);
    for start in graph.keys() {
        let mut seen = BTreeSet::from([*start]);
        let mut pending = vec![*start];
        let mut finished = false;
        while let Some(state) = pending.pop() {
            if matches!(state.0, DELIVERED | CANCELLED) {
                finished = true;
                break;
            }
            for next in &graph[&state] {
                if seen.insert(*next) {
                    pending.push(*next);
                }
            }
        }
        assert!(finished, "order stuck from {start:?}");
    }
}

/// Parses `pre.120 pre.121 action callback_attempt caller | outcome reason
/// post.120 post.121 | request`, where the request is `-`,
/// `300 attempt action`, or `301 attempt`.
fn parse_example(line: &str) -> ((State, Input, u16), Outcome) {
    let parts: Vec<&str> = line.split('|').map(str::trim).collect();
    let [input, decision, request] = parts.as_slice() else {
        panic!("three columns expected: {line}");
    };
    let input: Vec<i128> = input
        .split_whitespace()
        .map(|word| word.parse().unwrap())
        .collect();
    let [status_id, attempts, action_id, attempt, caller_id] = input.as_slice() else {
        panic!("five inputs expected: {line}");
    };
    let decision: Vec<&str> = decision.split_whitespace().collect();
    let [kind, reason, p120, p121] = decision.as_slice() else {
        panic!("four decision fields expected: {line}");
    };
    let kind = match *kind {
        "accept" => "accept",
        "reject" => "reject",
        "failure" => "failure",
        other => panic!("unknown outcome {other}"),
    };
    let reason = (*reason != "-").then(|| reason.parse().unwrap());
    let request: Vec<i128> = if *request == "-" {
        Vec::new()
    } else {
        request
            .split_whitespace()
            .map(|word| word.parse().unwrap())
            .collect()
    };
    let requests = match request.as_slice() {
        [] => Vec::new(),
        [300, attempt, request_action] => {
            vec![(300, *attempt, u16::try_from(*request_action).unwrap())]
        }
        [301, attempt] => vec![(301, *attempt, 0)],
        other => panic!("unknown request {other:?}: {line}"),
    };
    let variant_id = |raw: &i128| u16::try_from(*raw).unwrap();
    (
        (
            (variant_id(status_id), *attempts),
            (variant_id(action_id), *attempt),
            variant_id(caller_id),
        ),
        Outcome {
            kind,
            reason,
            post: (p120.parse().unwrap(), p121.parse().unwrap()),
            requests,
        },
    )
}

#[test]
fn decision_examples_match_the_executed_application() {
    let authority = authority().unwrap();
    let mut covered = BTreeSet::new();
    let mut count = 0;
    for line in EXAMPLES
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let ((pre, input, caller_id), expected) = parse_example(line);
        assert_eq!(
            observe(&authority, pre, input, caller_id),
            expected,
            "example: {line}"
        );
        covered.insert((expected.kind, expected.reason));
        count += 1;
    }
    assert_eq!(count, 23);
    assert_eq!(
        covered,
        BTreeSet::from([
            ("accept", None),
            ("failure", Some(204)),
            ("reject", Some(200)),
            ("reject", Some(201)),
            ("reject", Some(202)),
            ("reject", Some(203)),
        ])
    );
}

#[test]
fn schema_admission_and_genesis_are_exact() {
    let authority = authority().unwrap();
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    for attempts in [-1, 4] {
        assert!(
            project
                .admit_root::<RustCryptoSha256>(&order((PLACED, attempts)), limits)
                .is_err(),
            "attempts {attempts} must be refused"
        );
        assert!(
            project
                .admit_command::<RustCryptoSha256>(
                    &command(OrderAction::PaymentCaptured, attempts),
                    limits
                )
                .is_err(),
            "callback attempt {attempts} must be refused"
        );
    }
    for status_id in STATUSES {
        for attempts in 0..=3 {
            let root = project
                .admit_root::<RustCryptoSha256>(&order((status_id, attempts)), limits)
                .unwrap();
            assert_eq!(
                authority.authorize_genesis(root).is_ok(),
                (status_id, attempts) == (PLACED, 0),
                "genesis {status_id} {attempts}"
            );
        }
    }
}
