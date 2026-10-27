//! Differential evidence against the original ordered step, retained only in tests.
use super::super::domain_tests::{fail, fixtures};
use super::super::obligations_tests::committed;
use super::*;

fn original_step(
    composition: &Composition,
    tables: &Tables,
    pre: &[Vec<i128>],
    request: &[i128],
) -> (Ended, Vec<Vec<i128>>) {
    let mut after = pre.to_vec();
    for at in 0..composition.components.len() {
        match local(composition, tables, at, pre, request) {
            Local::Skip => {}
            Local::Reject(reason) => return (Ended::Reject(at, reason), pre.to_vec()),
            Local::Refused(class) => return (Ended::Refused(at, class), pre.to_vec()),
            Local::Commit {
                failure,
                reason,
                post,
            } => {
                if composition.components[at].owns {
                    after[at] = post;
                }
                if failure {
                    return (Ended::CommittedFailure(at, reason), after);
                }
            }
        }
    }
    (Ended::Accept, after)
}

#[test]
fn shared_step_matches_original_on_every_tuple_of_small_fixture_domains() {
    // The huge/medium limit fixtures cannot be enumerated; their public API
    // refusal/streaming tests remain unchanged. Every listed finite fixture
    // below is enumerated over ALL states and requests, including unreachable
    // states and states outside the declared invariants.
    let names = [
        "guarded/composition.json",
        "documented/lockstep-failure.json",
        "documented/procurement.json",
        "faults/read-only-changes.json",
        "faults/binding-out-of-domain.json",
        "faults/invariant-not-preserved.json",
        "faults/coupling-broken-by-step.json",
        "faults/coupling-false-at-genesis.json",
        "faults/ticker-tripwire.json",
    ];
    let mut total = 0usize;
    for name in names {
        assert!(fixtures().join(name).is_file(), "{name}");
        let (text, sources) = committed(name);
        let joint = JointComposition::from_sources(&DomainSources {
            manifest: &text,
            components: &sources,
        })
        .unwrap_or_else(fail);
        let state_values: Vec<Vec<i128>> = joint.layout.iter().map(|s| s.domain.values()).collect();
        let request_values: Vec<Vec<i128>> =
            joint.request.iter().map(|s| s.domain.values()).collect();
        super::super::leaf::for_each_tuple(&state_values, |state| {
            super::super::leaf::for_each_tuple(&request_values, |request| {
                let pre = joint.split(state, request).unwrap_or_else(fail);
                assert_eq!(
                    step(&joint.composition, &joint.tables, &pre, request),
                    original_step(&joint.composition, &joint.tables, &pre, request),
                    "{name}: {state:?} {request:?}"
                );
                total += 1;
                Ok::<(), ()>(())
            })
        })
        .unwrap_or_else(fail);
    }
    assert!(total > 100);
}

#[test]
fn documented_committed_failure_reaches_the_shared_step() {
    let (text, sources) = committed("documented/lockstep-failure.json");
    let joint = JointComposition::from_sources(&DomainSources {
        manifest: &text,
        components: &sources,
    })
    .unwrap_or_else(fail);
    let outcome = joint
        .joint_step(joint.genesis(), &[0, 1])
        .unwrap_or_else(fail);
    assert!(matches!(outcome.kind, OutcomeKind::CommittedFailure { .. }));
    assert_eq!(&outcome.successor[..2], &[1, 1]);
}

#[test]
fn resolver_never_calls_after_any_terminating_kind() {
    use super::super::resolve::{self, Local as Kind};
    for kind in [Kind::Reject, Kind::Refused, Kind::CommitFailure] {
        let mut calls = Vec::new();
        let result = resolve::resolve(4, |at| {
            calls.push(at);
            assert!(at <= 1, "callback ran after the terminal component");
            if at == 1 { kind } else { Kind::Commit }
        });
        assert_eq!(calls, vec![0, 1]);
        assert_eq!(
            result.committed,
            if kind == Kind::CommitFailure { 2 } else { 0 }
        );
    }
}

#[test]
fn outside_failure_cut_has_a_realizable_coupling_counterexample() {
    use super::super::domain_tests::{Built, budget, counter, part};
    use serde_json::json;
    let wire = part(
        "wire",
        "faults/tripwire",
        &["tripped", "touches"],
        "true",
        json!({"fire": "qty == 2", "authorized": "authorized"}),
        "true",
        "true",
    );
    let built = Built::new(
        vec![counter(), wire, budget()],
        json!([{"name":"balance", "members":["counter", "budget"],
            "invariant":"counter.value + budget.remaining == 2"}]),
    );
    let text = built.text();
    let sources = DomainSources {
        manifest: &text,
        components: &built.sources,
    };
    let verdict = super::super::check_domain(&sources).unwrap_or_else(fail);
    assert_eq!(verdict.verdict(), super::super::Verdict::Violated);
    let joint = JointComposition::from_sources(&sources).unwrap_or_else(fail);
    let outcome = joint
        .joint_step(joint.genesis(), &[0, 2, 1])
        .unwrap_or_else(fail);
    assert!(matches!(outcome.kind, OutcomeKind::CommittedFailure { .. }));
    assert_eq!(outcome.successor[0] + outcome.successor[3], 3);
}
