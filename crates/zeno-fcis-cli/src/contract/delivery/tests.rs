//! Actual library-law and Authority controls, not a parallel evaluator.
use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{V2Limits, V2Resource, v2_laws as l, v2_zero_limits};

use super::*;
use crate::contract::review::{
    domain,
    evaluate::{self, Framer, Outcome},
};
use crate::contract::{
    self, ContractSources, expr, model::Contract, policy, schema, schema_commitment,
};

const PROJECT: &str = include_str!("../../../tests/fixtures/escrow/project.zeno");
const RULES: &str = include_str!("../../../tests/fixtures/escrow/v2/policy.json");
const LAW: &str = include_str!("../../../tests/fixtures/delivery-laws/escrow-law.zeno");
const MAPPING: &str = include_str!("../../../tests/fixtures/delivery-laws/escrow-law.json");

fn fixture() -> (String, Value) {
    let mut rules: Value = serde_json::from_str(RULES)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    rules["delivery_laws"] = serde_json::from_str(MAPPING)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    rules["law_kinds"]["510"] = json!("AssetConservation");
    (format!("{PROJECT}\n{LAW}"), rules)
}

fn generated(
    project: &str,
    rules: &Value,
) -> Result<contract::GeneratedContract, contract::ContractError> {
    contract::generate_contract(ContractSources {
        project,
        rules: &rules.to_string(),
        schema_origin: None,
        adoptions: &[],
        replayed: &[],
        evolutions: &[],
    })
}

fn decide(project: &str, json: &Value, tuple: &[i64]) -> Outcome {
    let rules = Rules::read(&json.to_string())
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let declarations = Declarations::read(project, &rules.leaf_bindings)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let schema = schema::encode(&declarations)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let contract = Contract::build(
        &declarations,
        &rules,
        schema_commitment(&schema)
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let positions = domain::positions(&declarations)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let framer = Framer::new(&positions, &contract);
    policy::with_authority(&contract, &schema, |authority| {
        evaluate::evaluate(authority, &framer, tuple)
    })
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
}

#[test]
fn escrow_actual_payouts_match_released_holds_and_double_payout_refuses_at_two() {
    let (project, mut rules) = fixture();
    generated(&project, &rules).unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    // Replay all original AI-authored examples against the original Authority
    // and require complete decision equality with the added delivery law.
    let original: Value = serde_json::from_str(RULES)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let mut classes = std::collections::BTreeSet::new();
    let mut examples = 0;
    for line in include_str!("../../../tests/fixtures/escrow/tests/decision-examples.txt").lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let tuple: Vec<i64> = line
            .split('|')
            .next()
            .unwrap_or_else(|| panic!("required fixture value is missing"))
            .split_whitespace()
            .map(|v| {
                v.parse()
                    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
            })
            .collect();
        let expected = decide(PROJECT, &original, &tuple);
        let Outcome::Decision(ref decision) = expected else {
            panic!("original fixture refused: {expected:?}");
        };
        classes.insert(format!("{:?}", decision.class));
        examples += 1;
        assert_eq!(decide(&project, &rules, &tuple), expected);
    }
    assert_eq!(classes.len(), 3);
    assert_eq!(examples, 23);
    let split = [
        153, 5000, 5000, 0, 0, 1000000, 1100000, 174, 0, 2000, 163, 2400000,
    ];
    // Both proposed payments become the full held amount: 10,000 paid for
    // 5,000 released. The length bound remains two and both deliveries fit it.
    let case = rules["cases"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("required fixture value is missing"))
        .iter_mut()
        .find(|case| case["rule"] == "resolve: split")
        .unwrap_or_else(|| panic!("required fixture value is missing"));
    assert_eq!(
        case["outbox"]
            .as_array()
            .unwrap_or_else(|| panic!("required fixture value is missing"))
            .len(),
        2
    );
    for delivery in case["outbox"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("required fixture value is missing"))
    {
        delivery["payload"]["141"] = json!("held");
    }
    generated(&project, &rules).unwrap_or_else(|error| panic!("test operation failed: {error:?}")); // Binding is not a requirements proof.
    assert!(
        matches!(decide(&project, &rules, &split), Outcome::Refused(ref refusal) if refusal.law == Some(510) && refusal.text.contains("Violated"))
    );
    // Without the independently stated delivery law the case table's own
    // conformance law agrees with its planted mistake and allows it.
    rules
        .as_object_mut()
        .unwrap_or_else(|| panic!("required fixture value is missing"))
        .remove("delivery_laws");
    rules["law_kinds"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("required fixture value is missing"))
        .remove("510");
    assert!(matches!(
        decide(PROJECT, &rules, &split),
        Outcome::Decision(_)
    ));
}

fn limits() -> V2Limits {
    [
        V2Resource::Read,
        V2Resource::Write,
        V2Resource::Candidate,
        V2Resource::Effect,
        V2Resource::Byte,
        V2Resource::WitnessByte,
        V2Resource::Depth,
        V2Resource::Step,
    ]
    .into_iter()
    .fold(v2_zero_limits(), |limits, resource| {
        limits.with_limit(resource, u64::MAX)
    })
}

/// Mandatory metadata controls are true here, isolating the authored compiled
/// delivery law. This lower-level public library evaluator grants no authority.
fn definitions<'a>(
    truth: &'a [l::Op<'a>],
    nodes: &'a [l::Op<'a>],
    root: usize,
    scope: l::Scope,
) -> Vec<l::Law<'a>> {
    let mut laws: Vec<_> = [
        (l::Kind::StateInvariant, l::Scope::Committing, true),
        (l::Kind::RejectNoAuthority, l::Scope::Reject, false),
        (
            l::Kind::CommittedFailureEffects,
            l::Scope::CommittedFailure,
            false,
        ),
        (l::Kind::DecisionConformance, l::Scope::Always, false),
        (l::Kind::InitialCondition, l::Scope::Always, true),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (kind, scope, genesis))| l::Law {
        id: index as u32 + 1,
        kind,
        scope,
        genesis,
        program: l::Program {
            nodes: truth,
            root: 0,
        },
    })
    .collect();
    laws.push(l::Law {
        id: 510,
        kind: l::Kind::AssetConservation,
        scope,
        genesis: false,
        program: l::Program { nodes, root },
    });
    laws
}

fn raw_law(
    project: &str,
    mapping: Value,
    formula: &str,
    deliveries: &[l::Delivery<'_>],
    class: l::Class,
    scope: l::Scope,
    cap: V2Limits,
) -> l::Outcome {
    let mut json: Value = serde_json::from_str(RULES)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    json["delivery_laws"] = json!({"510": mapping});
    let mut rules = Rules::read(&json.to_string())
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    // No predicted deliveries are consulted by the graph compiler except to
    // check the static maximum; raw-frame probes also exercise M=0.
    for case in &mut rules.cases {
        case.outbox.clear();
    }
    let declarations = Declarations::read(project, &rules.leaf_bindings)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let mut graph = LawGraph::new(&declarations);
    let bounded = bind(
        &mut graph,
        &declarations,
        &rules,
        &rules.delivery_laws[&510],
    )
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let value = graph
        .compile(
            &expr::parse(formula)
                .unwrap_or_else(|error| panic!("test operation failed: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let predicate = graph
        .boolean(value)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    graph
        .require_bindings_used()
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let root = graph
        .op(Op::And(bounded.index, predicate.index), Kind::Bool)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let nodes: Vec<_> = graph.table.nodes.iter().map(policy::law_op).collect();
    let truth = [l::Op::Literal(l::Atom::Bool(true))];
    let laws = definitions(&truth, &nodes, root.index, scope);
    let genesis = l::Frame::Genesis {
        initial: l::RootView::Record(&[]),
    };
    let (result, usage, _, _) = l::evaluate(&laws, &[510], &genesis, limits()).into_parts();
    assert_eq!(result, Ok(()));
    let candidate = l::Candidate {
        class,
        reason: if class == l::Class::Accept {
            None
        } else {
            Some(200)
        },
        post: l::RootView::Record(&[]),
        patch: &[],
        effects: &[],
        outbox: deliveries,
        reads: &[],
        attempts: &[],
        usage,
    };
    let frame = l::Frame::Transition {
        pre: l::RootView::Record(&[]),
        command: l::RootView::Record(&[]),
        context: l::RootView::Record(&[]),
        candidate: &candidate,
    };
    l::evaluate(&laws, &[510], &frame, cap)
}

fn mapping(bound: u32) -> Value {
    json!({"max_deliveries":bound,"observations":{
        "outbox.104":{"channel":300},"outbox.104.141":{"channel":300,"field":141}}})
}
fn delivery(ordinal: u32, channel: u32, amount: Option<l::Atom<'static>>) -> l::Delivery<'static> {
    l::Delivery {
        ordinal,
        channel,
        destination: l::Atom::Text(b"payments-desk"),
        payload: amount
            .into_iter()
            .map(|value| l::Field { id: 141, value })
            .collect(),
        idempotency: l::Atom::U128(u128::from(ordinal)),
    }
}
fn checked(deliveries: &[l::Delivery<'_>], count: usize, sum: i128) -> l::Outcome {
    raw_law(
        PROJECT,
        mapping(2),
        &format!("outbox.104 == {count} && outbox.104.141 == {sum}"),
        deliveries,
        l::Class::Accept,
        l::Scope::Committing,
        limits(),
    )
}

#[test]
fn zero_single_multiple_nonmatching_and_bound_use_actual_candidate() {
    assert_eq!(checked(&[], 0, 0).into_parts().0, Ok(()));
    assert_eq!(
        checked(&[delivery(0, 300, Some(l::Atom::I128(5)))], 1, 5)
            .into_parts()
            .0,
        Ok(())
    );
    assert_eq!(
        checked(
            &[
                delivery(0, 300, Some(l::Atom::I128(2))),
                delivery(1, 300, Some(l::Atom::I128(3)))
            ],
            2,
            5
        )
        .into_parts()
        .0,
        Ok(())
    );
    // A nonmatching channel has no field 141 and must not be read as a payout.
    assert_eq!(
        checked(&[delivery(0, 301, None)], 0, 0).into_parts().0,
        Ok(())
    );
    let three = [
        delivery(0, 300, Some(l::Atom::I128(0))),
        delivery(1, 300, Some(l::Atom::I128(0))),
        delivery(2, 300, Some(l::Atom::I128(0))),
    ];
    assert_eq!(
        checked(&three, 2, 0).into_parts().0,
        Err(l::Failure::Violated)
    );
    assert_eq!(
        raw_law(
            PROJECT,
            mapping(0),
            "outbox.104 == 0 && outbox.104.141 == 0",
            &[],
            l::Class::Accept,
            l::Scope::Committing,
            limits()
        )
        .into_parts()
        .0,
        Ok(())
    );
}

#[test]
fn matching_missing_wrong_type_and_sum_overflow_refuse_in_library() {
    for amount in [
        None,
        Some(l::Atom::Bool(false)),
        Some(l::Atom::U128(0)),
        Some(l::Atom::Text(b"0")),
        Some(l::Atom::Sum {
            type_id: 106,
            variant: 160,
        }),
    ] {
        assert_eq!(
            checked(&[delivery(0, 300, amount)], 1, 0).into_parts().0,
            Err(l::Failure::Undefined)
        );
    }
    let overflow = [
        delivery(0, 300, Some(l::Atom::I128(i128::MAX))),
        delivery(1, 300, Some(l::Atom::I128(1))),
    ];
    assert_eq!(
        checked(&overflow, 2, 0).into_parts().0,
        Err(l::Failure::Undefined)
    );
    // Valid atoms, but count/conservation themselves disagree.
    assert_eq!(
        checked(
            &[
                delivery(0, 300, Some(l::Atom::I128(5))),
                delivery(1, 300, Some(l::Atom::I128(5)))
            ],
            2,
            5
        )
        .into_parts()
        .0,
        Err(l::Failure::Violated)
    );
}

#[test]
fn multiple_declared_channels_are_counted_and_summed_separately() {
    let project = format!(
        "{PROJECT}\ntype 117 payload AuditAmount;\nfield 142 117 audit_amount 108;\nchannel 301 audit destination 103 payload 117;\n"
    );
    let mapping = json!({"max_deliveries":2,"observations":{
        "outbox.104":{"channel":300},"outbox.104.141":{"channel":300,"field":141},
        "outbox.117":{"channel":301},"outbox.117.142":{"channel":301,"field":142}}});
    let mut second = delivery(1, 301, None);
    second.payload.push(l::Field {
        id: 142,
        value: l::Atom::I128(7),
    });
    let deliveries = [delivery(0, 300, Some(l::Atom::I128(5))), second];
    let formula =
        "outbox.104 == 1 && outbox.104.141 == 5 && outbox.117 == 1 && outbox.117.142 == 7";
    assert_eq!(
        raw_law(
            &project,
            mapping,
            formula,
            &deliveries,
            l::Class::Accept,
            l::Scope::Committing,
            limits()
        )
        .into_parts()
        .0,
        Ok(())
    );
}

#[test]
fn scope_and_actual_read_meter_are_preserved() {
    let deliveries = [delivery(0, 300, Some(l::Atom::I128(5)))];
    let (result, usage, _, reads) = checked(&deliveries, 1, 5).into_parts();
    assert_eq!(result, Ok(()));
    assert_eq!(usage.used(V2Resource::Read), reads.len() as u64);
    assert!(
        reads
            .iter()
            .any(|r| r.observation == l::Observation::OutboxPayload(0, 141))
    );
    assert!(!reads.iter().any(|r| matches!(
        r.observation,
        l::Observation::OutboxPayload(1, _) | l::Observation::OutboxChannel(1)
    )));
    let low = limits().with_limit(V2Resource::Read, usage.used(V2Resource::Read) - 1);
    assert!(matches!(
        raw_law(
            PROJECT,
            mapping(2),
            "outbox.104 == 1 && outbox.104.141 == 5",
            &deliveries,
            l::Class::Accept,
            l::Scope::Committing,
            low
        )
        .into_parts()
        .0,
        Err(l::Failure::Budget(_))
    ));
    assert_eq!(
        raw_law(
            PROJECT,
            mapping(2),
            "outbox.104 == 1 && outbox.104.141 == 5",
            &deliveries,
            l::Class::CommittedFailure,
            l::Scope::Committing,
            limits()
        )
        .into_parts()
        .0,
        Ok(())
    );
    let (result, _, diagnostics, reads) = raw_law(
        PROJECT,
        mapping(2),
        "outbox.104 == 99 && outbox.104.141 == 0",
        &[],
        l::Class::Reject,
        l::Scope::Committing,
        limits(),
    )
    .into_parts();
    assert_eq!(result, Ok(()));
    assert!(
        diagnostics
            .iter()
            .any(|d| d.id == 510 && d.verdict == l::Verdict::Skipped)
    );
    assert!(reads.is_empty());
    assert_eq!(
        raw_law(
            PROJECT,
            mapping(2),
            "outbox.104 == 0 && outbox.104.141 == 0",
            &[],
            l::Class::Reject,
            l::Scope::Reject,
            limits()
        )
        .into_parts()
        .0,
        Ok(())
    );
}

#[test]
fn malformed_unused_ambiguous_and_genesis_delivery_laws_are_refused() {
    let (project, original) = fixture();
    for (label, mutate) in [
        (
            "too small",
            Box::new(|r: &mut Value| r["delivery_laws"]["510"]["max_deliveries"] = json!(1))
                as Box<dyn Fn(&mut Value)>,
        ),
        (
            "too large",
            Box::new(|r: &mut Value| r["delivery_laws"]["510"]["max_deliveries"] = json!(65)),
        ),
        (
            "negative",
            Box::new(|r: &mut Value| r["delivery_laws"]["510"]["max_deliveries"] = json!(-1)),
        ),
        (
            "fraction",
            Box::new(|r: &mut Value| r["delivery_laws"]["510"]["max_deliveries"] = json!(2.5)),
        ),
        (
            "text bound",
            Box::new(|r: &mut Value| r["delivery_laws"]["510"]["max_deliveries"] = json!("2")),
        ),
        (
            "missing bound",
            Box::new(|r: &mut Value| {
                r["delivery_laws"]["510"]
                    .as_object_mut()
                    .unwrap_or_else(|| panic!("required fixture value is missing"))
                    .remove("max_deliveries");
            }),
        ),
        (
            "too many observations",
            Box::new(|r: &mut Value| {
                r["delivery_laws"]["510"]["observations"] = Value::Object(
                    (0..17)
                        .map(|i| (format!("outbox.104.{i}"), json!({"channel":300,"field":i})))
                        .collect(),
                );
            }),
        ),
        (
            "noncanonical projection",
            Box::new(|r: &mut Value| {
                let value = r["delivery_laws"]["510"]["observations"]
                    .as_object_mut()
                    .unwrap_or_else(|| panic!("required fixture value is missing"))
                    .remove("outbox.104.141")
                    .unwrap_or_else(|| panic!("required fixture value is missing"));
                r["delivery_laws"]["510"]["observations"]["outbox.0104.141"] = value;
            }),
        ),
        (
            "unknown channel",
            Box::new(|r: &mut Value| {
                r["delivery_laws"]["510"]["observations"]["outbox.104"]["channel"] = json!(999)
            }),
        ),
        (
            "missing field",
            Box::new(|r: &mut Value| {
                r["delivery_laws"]["510"]["observations"]["outbox.104.141"]["field"] = json!(999)
            }),
        ),
        (
            "wrong field type",
            Box::new(|r: &mut Value| {
                r["delivery_laws"]["510"]["observations"]["outbox.104.141"]["field"] = json!(140)
            }),
        ),
        (
            "unknown law",
            Box::new(|r: &mut Value| {
                r["delivery_laws"]["511"] = r["delivery_laws"]["510"].clone();
            }),
        ),
        (
            "missing mapping",
            Box::new(|r: &mut Value| {
                r.as_object_mut()
                    .unwrap_or_else(|| panic!("required fixture value is missing"))
                    .remove("delivery_laws");
            }),
        ),
        (
            "unused mapping",
            Box::new(|r: &mut Value| {
                r["delivery_laws"]["502"] = r["delivery_laws"]["510"].clone();
            }),
        ),
        (
            "empty observations",
            Box::new(|r: &mut Value| r["delivery_laws"]["510"]["observations"] = json!({})),
        ),
        (
            "extra key",
            Box::new(|r: &mut Value| r["delivery_laws"]["510"]["fallback"] = json!(0)),
        ),
        (
            "shadow root",
            Box::new(|r: &mut Value| r["variables"]["outbox.104.141"] = json!(0)),
        ),
        (
            "wrong mandatory scope",
            Box::new(|r: &mut Value| r["law_kinds"]["510"] = json!("RejectNoAuthority")),
        ),
    ] {
        let mut rules = original.clone();
        mutate(&mut rules);
        assert!(generated(&project, &rules).is_err(), "must refuse {label}");
    }
    assert!(
        generated(
            &project.replace(
                "payouts_equal_released on commit",
                "payouts_equal_released on commit, genesis"
            ),
            &original
        )
        .err()
        .unwrap_or_else(|| panic!("expected operation to be refused"))
        .to_string()
        .contains("genesis")
    );
    // Both channels are declared, but a single payload-root name cannot mean
    // two channels in the same law, even when their field selectors differ.
    let alias_project = format!("{project}\nchannel 301 alternate destination 103 payload 104;\n");
    let mut ambiguous = original;
    ambiguous["delivery_laws"]["510"]["observations"]["outbox.104.141"]["channel"] = json!(301);
    assert!(
        generated(&alias_project, &ambiguous)
            .err()
            .unwrap_or_else(|| panic!("expected operation to be refused"))
            .to_string()
            .contains("multiple channels")
    );
    let repeated = RULES.replacen(
        "\"schema\":",
        "\"delivery_laws\": {}, \"delivery_laws\": {}, \"schema\":",
        1,
    );
    assert!(Rules::read(&repeated).is_err());
}

#[test]
fn old_contract_bytes_stay_equal_and_new_bound_channel_field_are_policy_bound() {
    let original: Value = serde_json::from_str(RULES)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let baseline = generated(PROJECT, &original)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    let mut empty = original.clone();
    empty["delivery_laws"] = json!({});
    assert_eq!(
        generated(PROJECT, &empty)
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}")),
        baseline
    );
    let (project, mut rules) = fixture();
    let first = generated(&project, &rules)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    rules["delivery_laws"]["510"]["max_deliveries"] = json!(3);
    assert_ne!(
        generated(&project, &rules)
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
            .policy(),
        first.policy()
    );
    let project = format!("{project}\nchannel 301 alternate destination 103 payload 104;\n");
    let first = generated(&project, &rules)
        .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    for value in rules["delivery_laws"]["510"]["observations"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("required fixture value is missing"))
        .values_mut()
    {
        value["channel"] = json!(301);
    }
    assert_ne!(
        generated(&project, &rules)
            .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
            .policy(),
        first.policy()
    );

    // At M=0 the sum is zero, but its selected field still enters the policy
    // as an inactive observation. Changing field identity cannot disappear.
    let base = include_str!("../../../templates/durable-counter/project.zeno");
    let mut rules: Value = serde_json::from_str(include_str!(
        "../../../templates/durable-counter/v2/policy.json"
    ))
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    for case in rules["cases"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("required fixture value is missing"))
    {
        case["outbox"] = json!([]);
    }
    rules["law_kinds"]["510"] = json!("AssetConservation");
    let mut policies = Vec::new();
    for field in [112, 113] {
        let name = format!("outbox.104.{field}");
        rules["delivery_laws"] = json!({"510":{"max_deliveries":0,"observations":{(name.clone()):{"channel":300,"field":field}}}});
        let project = format!("{base}\nlaw 510 no_deliveries on commit = {name} == 0;\n");
        policies.push(
            generated(&project, &rules)
                .unwrap_or_else(|error| panic!("test operation failed: {error:?}"))
                .policy()
                .to_vec(),
        );
    }
    assert_ne!(policies[0], policies[1]);
}

#[test]
fn symbolic_delivery_observations_are_explicitly_inconclusive() {
    use crate::contract::symbolic::{Status, SymbolicSources, check};
    let project = format!(
        "{}\nlaw 510 notification_count on commit = outbox.104 == 1;\n",
        include_str!("../../../templates/durable-counter/project.zeno")
    );
    let mut rules: Value = serde_json::from_str(include_str!(
        "../../../templates/durable-counter/v2/policy.json"
    ))
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    rules["law_kinds"]["510"] = json!("AssetConservation");
    rules["delivery_laws"] =
        json!({"510":{"max_deliveries":1,"observations":{"outbox.104":{"channel":300}}}});
    let checked = check(
        SymbolicSources {
            project: &project,
            rules: &rules.to_string(),
            strengthening: None,
        },
        64,
        &mut crate::symbolic::testing::reference,
    )
    .unwrap_or_else(|error| panic!("test operation failed: {error:?}"));
    assert_eq!(checked.status, Status::Inconclusive);
    assert!(
        checked.report["causes"]
            .as_array()
            .unwrap_or_else(|| panic!("required fixture value is missing"))
            .iter()
            .any(|cause| cause
                .as_str()
                .is_some_and(|text| text.contains("unsupported delivery observations")
                    && text.contains("510")))
    );
}
