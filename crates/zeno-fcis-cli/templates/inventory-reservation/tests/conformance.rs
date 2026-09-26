//! Checks the executed application against its finite model and against
//! decision examples drafted from the README.
//!
//! Every decision runs through the application: schema admission, the
//! authority, the adapter in `src/program.rs`, the law checker, the committed
//! patch, and the outbox plan. Fields are read by the numeric IDs in
//! `project.zeno`, not through the generated name bindings, so a binding that
//! swapped the two stock fields fails here.
//!
//! The model independently restates the stock and authorization rules.
//! `tools/check_synthesis.py` compares the step's complete vectors with a
//! separate restatement of the rules,
//! and the examples file checks both against the README.

use inventory_reservation::{
    Authority, authority, bindings::GeneratedProject, generated::*, profile, request,
};
use std::collections::BTreeSet;
use zeno_fcis_codec::Domain;
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_value::Value;

const EXAMPLES: &str = include_str!("decision-examples.txt");
const ACTIONS: [u16; 4] = [150, 151, 152, 153];

/// Fields 110 available and 111 reserved.
type State = (i128, i128);

/// One decision, in the numeric IDs of `project.zeno`.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Outcome {
    /// `accept` or `reject`.
    kind: &'static str,
    /// The rejection reason.
    reason: Option<u32>,
    /// The stock after the decision.
    post: State,
    /// Channel and shipped units (field 140) of each queued shipment.
    shipments: Vec<(u32, i128)>,
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

fn action(id: u16) -> StockAction {
    match id {
        150 => StockAction::Reserve,
        151 => StockAction::Release,
        152 => StockAction::Ship,
        153 => StockAction::Restock,
        other => panic!("unknown action variant {other}"),
    }
}

fn stock((available, reserved): State) -> Stock {
    Stock {
        available: Units(available),
        reserved: Units(reserved),
    }
}

/// Runs one command through the application from an admitted pre-state.
fn observe(
    authority: &Authority,
    pre: State,
    action_id: u16,
    quantity: i128,
    authorized: bool,
) -> Outcome {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let root = project
        .admit_root::<RustCryptoSha256>(&stock(pre), limits)
        .unwrap();
    let pre_value = root.value().value().clone();
    // The typed bindings must place each field at its numeric ID.
    assert_eq!((int(&pre_value, 110), int(&pre_value, 111)), pre);
    let admitted_command = project
        .admit_command::<RustCryptoSha256>(&request(action(action_id), quantity), limits)
        .unwrap();
    let command_value = admitted_command.admitted().value().value();
    assert_eq!(variant(record_field(command_value, 120)), action_id);
    assert_eq!(int(command_value, 121), quantity);
    let admitted_context = project
        .admit_context::<RustCryptoSha256>(
            &StockContext {
                authorized: OperatorFlag(authorized),
            },
            limits,
        )
        .unwrap();
    assert_eq!(
        record_field(admitted_context.admitted().value().value(), 130),
        &Value::Bool(authorized)
    );
    let replay = format!("conformance {pre:?} {action_id} {quantity} {authorized}");
    let witness = authority
        .admit_invocation(
            root,
            admitted_command.admitted().clone(),
            admitted_context.admitted().clone(),
            profile::digest("example/inventory-reservation/principal", b"conformance"),
            profile::digest(
                "example/inventory-reservation/authentication",
                b"conformance",
            ),
            profile::digest("example/inventory-reservation/replay", replay.as_bytes()),
        )
        .unwrap();
    let domain = Domain::new("example/inventory-reservation/state", 1).unwrap();
    match authority.execute(witness).unwrap() {
        Decision::Reject(reject) => Outcome {
            kind: "reject",
            reason: Some(reject.reason().rejection().reason_id().get()),
            post: pre,
            shipments: Vec::new(),
        },
        Decision::Accept(accepted) => {
            let candidate = accepted.into_candidate();
            let bundle = candidate.bundle();
            assert!(bundle.commit_plan().effects().is_empty());
            let applied = bundle
                .patch()
                .apply::<RustCryptoSha256>(&pre_value, domain)
                .unwrap();
            let post = applied.state();
            Outcome {
                kind: "accept",
                reason: None,
                post: (int(post, 110), int(post, 111)),
                shipments: bundle
                    .outbox_plan()
                    .entries()
                    .iter()
                    .map(|entry| (entry.channel(), int(entry.payload(), 140)))
                    .collect(),
            }
        }
        Decision::CommittedFailure(_) => panic!("this application never commits a failure"),
    }
}

/// The finite model, independently restated from the README's precedence rules.
fn model(pre: State, action_id: u16, quantity: i128, authorized: bool) -> Outcome {
    let reject = |reason| Outcome {
        kind: "reject",
        reason: Some(reason),
        post: pre,
        shipments: Vec::new(),
    };
    if !authorized {
        return reject(200);
    }
    let (available, reserved) = pre;
    let (post, ship) = match action_id {
        150 if quantity > available => return reject(201),
        150 if reserved + quantity > 5 => return reject(203),
        150 => ((available - quantity, reserved + quantity), false),
        151 | 152 if quantity > reserved => return reject(202),
        151 if available + quantity > 5 => return reject(203),
        151 => ((available + quantity, reserved - quantity), false),
        152 => ((available, reserved - quantity), true),
        153 if available + quantity > 5 => return reject(203),
        153 => ((available + quantity, reserved), false),
        other => panic!("unknown action ID {other}"),
    };
    Outcome {
        kind: "accept",
        reason: None,
        post,
        shipments: if ship {
            vec![(300, quantity)]
        } else {
            Vec::new()
        },
    }
}

fn every_input() -> Vec<(State, u16, i128, bool)> {
    let mut inputs = Vec::new();
    for available in 0..=5 {
        for reserved in 0..=5 {
            for action_id in ACTIONS {
                for quantity in 1..=3 {
                    for authorized in [false, true] {
                        inputs.push(((available, reserved), action_id, quantity, authorized));
                    }
                }
            }
        }
    }
    inputs
}

#[test]
fn every_admitted_input_matches_the_finite_model_and_conserves_units() {
    let authority = authority().unwrap();
    let inputs = every_input();
    assert_eq!(inputs.len(), 864);
    for (pre, action_id, quantity, authorized) in inputs {
        let observed = observe(&authority, pre, action_id, quantity, authorized);
        assert_eq!(
            observed,
            model(pre, action_id, quantity, authorized),
            "input {pre:?} action {action_id} quantity {quantity} authorized {authorized}"
        );
        if observed.kind == "accept" {
            let added = if action_id == 153 { quantity } else { 0 };
            let shipped = if action_id == 152 { quantity } else { 0 };
            assert_eq!(
                observed.post.0 + observed.post.1,
                pre.0 + pre.1 + added - shipped,
                "units not conserved for {pre:?} {action_id} {quantity}"
            );
        }
    }
}

#[test]
fn schema_admission_matches_the_finite_domain() {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    for available in -1..=6 {
        for reserved in -1..=6 {
            let admitted = project
                .admit_root::<RustCryptoSha256>(&stock((available, reserved)), limits)
                .is_ok();
            let in_domain = (0..=5).contains(&available) && (0..=5).contains(&reserved);
            assert_eq!(admitted, in_domain, "stock ({available}, {reserved})");
        }
    }
    for quantity in -1..=4 {
        let admitted = project
            .admit_command::<RustCryptoSha256>(&request(StockAction::Reserve, quantity), limits)
            .is_ok();
        assert_eq!(admitted, (1..=3).contains(&quantity), "quantity {quantity}");
    }
}

#[test]
fn genesis_is_empty_and_reaches_every_admitted_state() {
    let authority = authority().unwrap();
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let other = project
        .admit_root::<RustCryptoSha256>(&stock((1, 0)), limits)
        .unwrap();
    assert!(authority.authorize_genesis(other).is_err());
    let mut seen = BTreeSet::from([(0, 0)]);
    let mut pending = vec![(0, 0)];
    while let Some(state) = pending.pop() {
        for action_id in ACTIONS {
            for quantity in 1..=3 {
                let outcome = observe(&authority, state, action_id, quantity, true);
                if outcome.kind == "accept" && seen.insert(outcome.post) {
                    pending.push(outcome.post);
                }
            }
        }
    }
    assert_eq!(seen.len(), 36);
}

/// Parses `pre.110 pre.111 action quantity authorized | outcome reason
/// post.110 post.111 | shipment`, where the shipment is `-` or `channel units`.
fn parse_example(line: &str) -> ((State, u16, i128, bool), Outcome) {
    let parts: Vec<&str> = line.split('|').map(str::trim).collect();
    let [input, decision, shipment] = parts.as_slice() else {
        panic!("three columns expected: {line}");
    };
    let input: Vec<i128> = input
        .split_whitespace()
        .map(|word| word.parse().unwrap())
        .collect();
    let [available, reserved, action_id, quantity, authorized] = input.as_slice() else {
        panic!("five inputs expected: {line}");
    };
    let decision: Vec<&str> = decision.split_whitespace().collect();
    let [kind, reason, p110, p111] = decision.as_slice() else {
        panic!("four decision fields expected: {line}");
    };
    let kind = match *kind {
        "accept" => "accept",
        "reject" => "reject",
        other => panic!("unknown outcome {other}"),
    };
    let reason = (*reason != "-").then(|| reason.parse().unwrap());
    let shipments = if *shipment == "-" {
        Vec::new()
    } else {
        let shipment: Vec<i128> = shipment
            .split_whitespace()
            .map(|word| word.parse().unwrap())
            .collect();
        let [channel, units] = shipment.as_slice() else {
            panic!("two shipment fields expected: {line}");
        };
        vec![(u32::try_from(*channel).unwrap(), *units)]
    };
    let authorized = match authorized {
        0 => false,
        1 => true,
        other => panic!("authorized must be 0 or 1, got {other}"),
    };
    (
        (
            (*available, *reserved),
            u16::try_from(*action_id).unwrap(),
            *quantity,
            authorized,
        ),
        Outcome {
            kind,
            reason,
            post: (p110.parse().unwrap(), p111.parse().unwrap()),
            shipments,
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
        let ((pre, action_id, quantity, authorized), expected) = parse_example(line);
        assert_eq!(
            observe(&authority, pre, action_id, quantity, authorized),
            expected,
            "example: {line}"
        );
        covered.insert((expected.kind, expected.reason));
        count += 1;
    }
    assert_eq!(count, 20);
    assert_eq!(
        covered,
        BTreeSet::from([
            ("accept", None),
            ("reject", Some(200)),
            ("reject", Some(201)),
            ("reject", Some(202)),
            ("reject", Some(203)),
        ])
    );
}
