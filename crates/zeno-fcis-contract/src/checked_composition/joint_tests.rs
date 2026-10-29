//! The joint step through its public API: one test for each rule of the
//! composed-domain step, the layout, the in-memory constructor and inputs
//! outside the layout.

use serde_json::{Value, json};

use super::domain_tests::{Built, Part, budget, counter, fail, part, pins};
use super::obligations_tests::committed;
use super::{
    DomainSources, FieldDomain, JointComposition, JointOutcome, OutcomeKind, RefusedClass,
};

fn joint(parts: Vec<Part>) -> JointComposition {
    let built = Built::new(parts, json!([]));
    let text = built.text();
    JointComposition::from_sources(&DomainSources {
        manifest: &text,
        components: &built.sources,
    })
    .unwrap_or_else(fail)
}

fn step(joint: &JointComposition, state: &[i128], request: &[i128]) -> JointOutcome {
    joint.joint_step(state, request).unwrap_or_else(fail)
}

fn reject(component: &str) -> OutcomeKind {
    OutcomeKind::Reject {
        component: component.to_owned(),
        reason: Some(201),
    }
}

fn refused(component: &str, class: RefusedClass) -> OutcomeKind {
    OutcomeKind::Refused {
        component: component.to_owned(),
        class,
    }
}

/// op, qty, authorized.
const SPEND_ONE: [i128; 3] = [0, 1, 1];

#[test]
fn the_in_memory_constructor_lays_out_state_request_and_genesis() {
    let joint = joint(vec![counter(), budget()]);
    assert_eq!(joint.components(), ["counter", "budget"]);
    let layout: Vec<(String, String, FieldDomain)> = joint
        .state_layout()
        .iter()
        .map(|slot| {
            (
                slot.component.clone(),
                slot.field.clone(),
                slot.domain.clone(),
            )
        })
        .collect();
    assert_eq!(
        layout,
        [
            (
                "counter".to_owned(),
                "value".to_owned(),
                FieldDomain::Int { min: 0, max: 2 }
            ),
            (
                "budget".to_owned(),
                "remaining".to_owned(),
                FieldDomain::Int { min: 0, max: 2 }
            ),
        ]
    );
    let request: Vec<&str> = joint
        .request_layout()
        .iter()
        .map(|slot| slot.name.as_str())
        .collect();
    assert_eq!(request, ["op", "qty", "authorized"]);
    assert_eq!(joint.request_layout()[2].domain, FieldDomain::Bool);
    assert_eq!(joint.genesis(), [0, 2]);
}

/// Every participant accepts: every successor commits.
#[test]
fn when_every_participant_accepts_every_successor_commits() {
    let joint = joint(vec![counter(), budget()]);
    let outcome = step(&joint, &[0, 2], &SPEND_ONE);
    assert_eq!(outcome.kind, OutcomeKind::Accept);
    assert_eq!(outcome.successor, [1, 1]);
}

/// Components decide on the same pre-state: a binding reads an earlier
/// component's pre-state, not its provisional successor.
#[test]
fn bindings_read_the_pre_state_of_earlier_components() {
    let mut reader = budget();
    reader.entry["bind"]["amount"] = json!("choose(counter.value == 0, 1, 2)");
    let joint = joint(vec![counter(), reader]);
    // The counter moves 0 -> 1 in the same step; the budget still sees 0
    // and spends 1, not 2.
    let outcome = step(&joint, &[0, 2], &SPEND_ONE);
    assert_eq!(outcome.kind, OutcomeKind::Accept);
    assert_eq!(outcome.successor, [1, 1]);
}

/// A component whose participation formula does not hold is skipped.
#[test]
fn a_non_participating_component_is_skipped() {
    let joint = joint(vec![counter(), budget()]);
    // op == 1: the counter does not participate.
    let outcome = step(&joint, &[0, 2], &[1, 1, 1]);
    assert_eq!(outcome.kind, OutcomeKind::Accept);
    assert_eq!(outcome.successor, [0, 1]);
}

/// An Accept stays provisional: a later Reject discards it, and nothing
/// changes.
#[test]
fn an_accept_stays_provisional_and_a_later_reject_changes_nothing() {
    let joint = joint(vec![counter(), budget()]);
    // The counter accepts; the budget has 1 and is asked for 2.
    let outcome = step(&joint, &[0, 1], &[0, 2, 1]);
    assert_eq!(outcome.kind, reject("budget"));
    assert_eq!(outcome.successor, [0, 1]);
}

/// A refusal by a component's Authority ends the step with no change: the
/// ticker's successor 3 breaks its law, which bounds the count at 2.
#[test]
fn an_authority_refusal_ends_the_step_with_no_change() {
    let ticker = part(
        "ticker",
        "faults/ticker",
        &["count"],
        "true",
        json!({"step": "qty - 1", "authorized": "authorized"}),
        "true",
        "true",
    );
    let joint = joint(vec![counter(), ticker, budget()]);
    let outcome = step(&joint, &[0, 2, 2], &[0, 2, 1]);
    assert_eq!(outcome.kind, refused("ticker", RefusedClass::Law));
    assert_eq!(outcome.successor, [0, 2, 2]);
    // From 1 the same step is accepted by everyone.
    let outcome = step(&joint, &[0, 1, 2], &[0, 1, 1]);
    assert_eq!(outcome.kind, OutcomeKind::Accept);
    assert_eq!(outcome.successor, [1, 1, 1]);
}

/// A binding with no value, or outside its field's domain, is a refusal:
/// the step ends with no change.
#[test]
fn a_binding_without_a_value_or_outside_its_domain_ends_the_step() {
    // Bindings use checked 64-bit arithmetic: an overflow has no value.
    let mut undefined = budget();
    undefined.entry["bind"]["amount"] = json!("170141183460469231731687303715884105727 + qty");
    let joint_undefined = joint(vec![counter(), undefined]);
    let outcome = step(&joint_undefined, &[0, 2], &SPEND_ONE);
    assert_eq!(
        outcome.kind,
        refused("budget", RefusedClass::BindingUndefined)
    );
    assert_eq!(outcome.successor, [0, 2]);
    let mut outside = budget();
    outside.entry["bind"]["amount"] = json!("qty + 1");
    let joint_outside = joint(vec![counter(), outside]);
    let outcome = step(&joint_outside, &[0, 2], &[0, 2, 1]);
    assert_eq!(
        outcome.kind,
        refused("budget", RefusedClass::BindingOutsideDomain)
    );
    assert_eq!(outcome.successor, [0, 2]);
}

/// The first CommittedFailure commits the provisional successors up to and
/// including its own and ends the step: later components do not run.
#[test]
fn the_first_committed_failure_commits_its_prefix_and_ends_the_step() {
    let wire = part(
        "wire",
        "faults/tripwire",
        &["tripped", "touches"],
        "true",
        json!({"fire": "qty == 2", "authorized": "authorized"}),
        "true",
        "true",
    );
    let joint = joint(vec![counter(), wire, budget()]);
    // qty == 2 fires the wire: the counter's increment and the trip commit;
    // the budget, after the wire, does not run.
    let outcome = step(&joint, &[0, 0, 0, 2], &[0, 2, 1]);
    assert_eq!(
        outcome.kind,
        OutcomeKind::CommittedFailure {
            component: "wire".to_owned(),
            reason: Some(201)
        }
    );
    assert_eq!(outcome.successor, [1, 1, 0, 2]);
    // qty == 1 does not: everyone accepts.
    let outcome = step(&joint, &[0, 0, 0, 2], &SPEND_ONE);
    assert_eq!(outcome.kind, OutcomeKind::Accept);
    assert_eq!(outcome.successor, [1, 0, 1, 1]);
}

/// A read-only component's committing decision that changes its state is a
/// refusal; one that keeps it lets the step go on.
#[test]
fn a_read_only_component_that_changes_its_state_is_refused() {
    let mut read_only = counter();
    read_only.entry["owns"] = json!([]);
    read_only.entry["bind"]["value"] = json!("0");
    let changing = joint(vec![budget(), read_only]);
    assert_eq!(changing.state_layout().len(), 1);
    let outcome = step(&changing, &[2], &SPEND_ONE);
    assert_eq!(
        outcome.kind,
        refused("counter", RefusedClass::ReadOnlyStateChanged)
    );
    assert_eq!(outcome.successor, [2]);
    let guard = part(
        "guard",
        "faults/guard",
        &[],
        "true",
        json!({"mark": "0", "request": "0", "authorized": "authorized"}),
        "true",
        "true",
    );
    let guarded = joint(vec![guard, budget()]);
    let outcome = step(&guarded, &[2], &SPEND_ONE);
    assert_eq!(outcome.kind, OutcomeKind::Accept);
    assert_eq!(outcome.successor, [1]);
    let outcome = step(&guarded, &[2], &[0, 1, 0]);
    assert_eq!(
        outcome.kind,
        OutcomeKind::Reject {
            component: "guard".to_owned(),
            reason: Some(200)
        }
    );
}

#[test]
fn inputs_outside_the_layout_are_refused() {
    let joint = joint(vec![counter(), budget()]);
    assert!(joint.joint_step(&[0], &SPEND_ONE).is_err());
    assert!(joint.joint_step(&[0, 2], &[0, 1]).is_err());
    assert!(joint.joint_step(&[3, 2], &SPEND_ONE).is_err());
    assert!(joint.joint_step(&[0, 2], &[0, 3, 1]).is_err());
}

#[test]
fn the_reachable_table_is_complete_or_refused() {
    let joint = joint(vec![counter(), budget()]);
    let rows = joint.reachable(1 << 16).unwrap_or_else(fail);
    // 12 requests on each reachable state, genesis first.
    assert_eq!(rows.len() % 12, 0);
    assert_eq!(rows[0].state, joint.genesis());
    assert!(joint.reachable(5).is_err());
}

#[test]
fn the_joint_step_is_deterministic() {
    let first = joint(vec![counter(), budget()]);
    let second = joint(vec![counter(), budget()]);
    for state in [[0, 2], [1, 1], [2, 0]] {
        for request in [[0, 1, 1], [1, 2, 1], [2, 1, 0]] {
            assert_eq!(
                step(&first, &state, &request),
                step(&second, &state, &request)
            );
        }
    }
}

fn joint_from(manifest: &str) -> Result<JointComposition, super::Refusal> {
    let (text, sources) = committed(manifest);
    JointComposition::from_sources(&DomainSources {
        manifest: &text,
        components: &sources,
    })
}

/// Request fields far too large to enumerate are streamed: the reachable
/// table is refused at its limit, never collected.
#[test]
fn the_reachable_table_streams_requests_of_any_size() {
    for manifest in [
        "limits/huge/composition.json",
        "limits/medium/composition.json",
    ] {
        let joint = joint_from(manifest).unwrap_or_else(fail);
        assert_eq!(joint.reachable(10), Err(10), "{manifest}");
    }
    // Here a participation formula reads three full 64-bit request fields:
    // its table is refused before it is built.
    let refusal = joint_from("limits/ovf/composition.json")
        .err()
        .unwrap_or_else(|| fail("refused"));
    assert_eq!(refusal.place(), "participation of `budget`");
}

/// A component whose local domain is over the ceiling is refused before
/// any table is built.
#[test]
fn the_constructor_refuses_tables_over_the_ceiling_before_building() {
    let refusal = joint_from("limits/widehuge.json")
        .err()
        .unwrap_or_else(|| fail("refused"));
    assert!(
        refusal
            .reason()
            .contains("refused before anything is built"),
        "{refusal}"
    );
}

/// Derive current program identities through the ordinary native producer while
/// retaining the historical fixture's external source pins and all other data.
fn current_guarded_fixture() -> (String, Vec<super::LeafSources>) {
    let (historical, sources) = committed("guarded/composition.json");
    let mut manifest: Value = serde_json::from_str(&historical).unwrap_or_else(fail);
    let components = manifest["components"]
        .as_array_mut()
        .unwrap_or_else(|| fail("components"));
    assert_eq!(components.len(), sources.len());
    assert_eq!(components.len(), 2);
    for (component, source) in components.iter_mut().zip(&sources) {
        let [project, policy, program] = pins(source);
        assert_eq!(component["project_sha256"], json!(project));
        assert_eq!(component["policy_sha256"], json!(policy));
        component["program_sha256"] = json!(program);
    }
    (
        serde_json::to_string(&manifest).unwrap_or_else(fail),
        sources,
    )
}

#[test]
fn native_bridge_27_historical_program_bindings_refuse_before_current_derivation() {
    let (historical, sources) = committed("guarded/composition.json");
    let refusal = JointComposition::from_sources(&DomainSources {
        manifest: &historical,
        components: &sources,
    })
    .err()
    .unwrap_or_else(|| fail("historical program binding must refuse"));
    assert_eq!(
        refusal.obligation(),
        super::refusal::Obligation::ProgramBinding
    );

    let (current, current_sources) = current_guarded_fixture();
    let mut old: Value = serde_json::from_str(&historical).unwrap_or_else(fail);
    let mut new: Value = serde_json::from_str(&current).unwrap_or_else(fail);
    assert_eq!(sources.len(), current_sources.len());
    for (old, new) in old["components"]
        .as_array_mut()
        .unwrap_or_else(|| fail("historical components"))
        .iter_mut()
        .zip(
            new["components"]
                .as_array_mut()
                .unwrap_or_else(|| fail("current components")),
        )
    {
        old["program_sha256"] = Value::Null;
        new["program_sha256"] = Value::Null;
    }
    assert_eq!(
        old, new,
        "only native-derived program identities may change"
    );
}

/// Experimental 2.7 native bridge: check a source-pinned Zeno composition
/// through BOTH the production composed-domain checker and its bound Authority
/// joint-step API, against an independently written complete scalar reference.
///
/// The tested profile derives current program pins for the otherwise unchanged
/// committed guarded-budget fixture. Matching
/// its finite transitions is differential evidence, NOT a machine-checked
/// parser/Authority-to-Verus refinement theorem, nor deployment authority.
#[test]
fn native_bridge_27_guarded_budget_matches_independent_complete_reference() {
    let (manifest, sources) = current_guarded_fixture();
    let bundle = DomainSources {
        manifest: &manifest,
        components: &sources,
    };
    let checked = super::check_domain(&bundle).unwrap_or_else(fail);
    assert_eq!(
        checked.verdict(),
        super::Verdict::Proved,
        "{}",
        checked.json()
    );
    let joint = JointComposition::from_sources(&bundle).unwrap_or_else(fail);

    assert_eq!(joint.components(), ["guard", "budget"]);
    assert_eq!(joint.genesis(), [3]);
    assert_eq!(joint.state_layout().len(), 1);
    assert_eq!(joint.state_layout()[0].component, "budget");
    assert_eq!(joint.state_layout()[0].field, "remaining");
    assert_eq!(
        joint.state_layout()[0].domain,
        FieldDomain::Int { min: 0, max: 3 }
    );
    assert_eq!(joint.request_layout().len(), 2);
    assert_eq!(joint.request_layout()[0].name, "amount");
    assert_eq!(joint.request_layout()[1].name, "authorized");
    assert_eq!(joint.request_layout()[1].domain, FieldDomain::Bool);

    let mut compared = 0usize;
    let mut accepted = 0usize;
    let mut rejected_auth = 0usize;
    let mut rejected_funds = 0usize;
    // This reference does not call, wrap or import the Authority decision
    // table interpreter. It is independently stated owner intent.
    // This tests the entire declared finite product, never samples.
    for remaining in 0i128..=3 {
        for amount in 0i128..=3 {
            for authorized in 0i128..=1 {
                let got = step(&joint, &[remaining], &[amount, authorized]);
                let want = if authorized == 0 {
                    rejected_auth += 1;
                    JointOutcome {
                        kind: OutcomeKind::Reject {
                            component: "guard".to_owned(),
                            reason: Some(200),
                        },
                        successor: vec![remaining],
                    }
                } else if amount > remaining {
                    rejected_funds += 1;
                    JointOutcome {
                        kind: OutcomeKind::Reject {
                            component: "budget".to_owned(),
                            reason: Some(201),
                        },
                        successor: vec![remaining],
                    }
                } else {
                    accepted += 1;
                    JointOutcome {
                        kind: OutcomeKind::Accept,
                        successor: vec![remaining - amount],
                    }
                };
                assert_eq!(
                    got, want,
                    "native composed-domain disagreement at remaining={remaining}, amount={amount}, authorized={authorized}"
                );
                assert!(got.successor[0] <= remaining);
                assert!((0..=3).contains(&got.successor[0]));
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 32);
    assert_eq!(rejected_auth, 16);
    assert_eq!(rejected_funds, 6);
    assert_eq!(accepted, 10);

    // Every reachable state x every request must appear. A smaller limit
    // MUST refuse, never silently truncate and then grant coverage.
    let reachable = joint.reachable(32).unwrap_or_else(fail);
    assert_eq!(reachable.len(), 32);
    assert!(joint.reachable(31).is_err());
    for row in reachable {
        let expected = step(&joint, &row.state, &row.request);
        assert_eq!(row.outcome, expected);
        assert!((0..=3).contains(&row.outcome.successor[0]));
    }
}

/// External source-byte commitments are checked BEFORE runtime binding and
/// proof-like checking; changing project text invalidates both.
#[test]
fn native_bridge_27_tampering_one_component_source_fails_closed() {
    let (manifest, mut sources) = current_guarded_fixture();
    sources[0]
        .project
        .push_str("\n// deliberately changed after manifest pin\n");
    let bundle = DomainSources {
        manifest: &manifest,
        components: &sources,
    };
    assert!(JointComposition::from_sources(&bundle).is_err());
    assert!(super::check_domain(&bundle).is_err());
}
