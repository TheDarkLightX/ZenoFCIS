//! The symbolic transform check on small programs, with the reference solver
//! standing in for CVC5 and Z3, and with planted solver faults.

use std::collections::BTreeMap;

use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{Domain, Op, Program, V2ExecutionFailure};

use super::{Outcome, check};
use crate::symbolic::testing::reference;
use crate::symbolic::verdict::{Answer, Answers, Judgment, Query};
use crate::transform::{self, Limits, Refusal, Replay};

const SMALL: Domain = Domain::Int { min: 0, max: 5 };

/// Every domain is above a cap of zero, so the symbolic check runs.
const SYMBOLIC: Limits = Limits {
    steps: transform::DEFAULT_STEP_LIMIT,
    input_tuples: 0,
};

fn bytes(inputs: Vec<Domain>, outputs: Vec<Domain>, nodes: Vec<Op>, roots: Vec<u16>) -> Vec<u8> {
    Program::try_new(inputs, outputs, nodes, roots)
        .unwrap_or_else(|error| panic!("test program must be admitted: {error}"))
        .value()
        .and_then(|value| value.canonical_bytes())
        .unwrap_or_else(|error| panic!("encode test program: {error}"))
}

/// Whether `x0 < x1`, then the smaller of the two.
fn smaller() -> Vec<u8> {
    bytes(
        vec![SMALL, SMALL],
        vec![Domain::Bool, SMALL],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Lt(0, 1),
            Op::Select(2, 0, 1),
        ],
        vec![2, 3],
    )
}

/// The same function written differently.
fn smaller_rewritten() -> Vec<u8> {
    bytes(
        vec![SMALL, SMALL],
        vec![Domain::Bool, SMALL],
        vec![
            Op::Input(1),
            Op::Input(0),
            Op::Lt(0, 1),
            Op::Select(2, 0, 1),
            Op::Lt(1, 0),
            Op::Not(4),
            Op::Not(5),
        ],
        vec![6, 3],
    )
}

fn answers(cvc5: Answer, z3: Answer) -> Answers {
    Answers {
        cvc5,
        z3,
        millis: [None, None],
    }
}

#[test]
fn equal_programs_hold_on_every_piece_and_refute_the_planted_control() {
    let Outcome::Equivalent(equivalence) =
        check(&smaller(), &smaller_rewritten(), SYMBOLIC, &mut reference)
    else {
        panic!("equal programs must be equivalent")
    };
    let compared = &equivalence.compared;
    let labels: Vec<&str> = compared
        .pieces
        .iter()
        .map(|piece| piece.label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "case-0",
            "case-1",
            "arithmetic-failure",
            "output-domain-failure"
        ]
    );
    assert!(
        compared
            .pieces
            .iter()
            .all(|piece| piece.judgment == Judgment::Holds { corroborated: true })
    );
    assert!(matches!(
        compared.control.judgment,
        Judgment::Refuted { .. }
    ));
    let receipt = equivalence.receipt(&serde_json::json!({"cvc5": null, "z3": null}));
    let value: serde_json::Value =
        serde_json::from_slice(&receipt).unwrap_or_else(|error| panic!("receipt json: {error}"));
    assert_eq!(value["schema"], super::RECEIPT_SCHEMA);
    assert_eq!(value["verdict"], "attested-equivalent");
    assert_eq!(value["evidence"], "attested");
    // The exhaustive replay, and with it every command that needs an
    // exhaustive receipt, does not read a symbolic one.
    assert_eq!(
        transform::replay(&receipt, &smaller(), &smaller_rewritten(), 1 << 20),
        Replay::Unreadable
    );
    assert!(matches!(
        transform::replayed(&receipt, &smaller(), &smaller_rewritten(), 1 << 20),
        Err(Replay::Unreadable)
    ));
    assert!(matches!(
        transform::refreshed(&receipt, &smaller(), &smaller_rewritten(), 1 << 20),
        Err(Replay::Unreadable)
    ));
}

#[test]
fn a_different_case_selection_is_a_replayed_counterexample() {
    // `x0 <= x1` selects differently where the inputs are equal.
    let candidate = bytes(
        vec![SMALL, SMALL],
        vec![Domain::Bool, SMALL],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Lt(1, 0),
            Op::Not(2),
            Op::Select(3, 0, 1),
        ],
        vec![3, 4],
    );
    let Outcome::Counterexample(found) = check(&smaller(), &candidate, SYMBOLIC, &mut reference)
    else {
        panic!("a different selection must be refuted")
    };
    let piece = &found.compared.pieces[found.piece];
    assert_eq!(piece.label, "case-0");
    let Judgment::Refuted { witness, .. } = &piece.judgment else {
        panic!("the refuted piece holds its witness")
    };
    assert_eq!(witness.input, [0, 0]);
    assert_eq!(witness.original, Ok(vec![0, 0]));
    assert_eq!(witness.candidate, Ok(vec![1, 0]));
}

#[test]
fn a_trap_only_one_program_has_is_a_counterexample_in_its_piece() {
    // An unused sum overflows wherever x0 is at least 1; eager evaluation
    // still traps there.
    let original = bytes(
        vec![SMALL],
        vec![SMALL],
        vec![Op::Input(0), Op::Int(i64::MAX), Op::Add(0, 1)],
        vec![0],
    );
    let candidate = bytes(vec![SMALL], vec![SMALL], vec![Op::Input(0)], vec![0]);
    let Outcome::Counterexample(found) = check(&original, &candidate, SYMBOLIC, &mut reference)
    else {
        panic!("a dropped trap must be refuted")
    };
    let piece = &found.compared.pieces[found.piece];
    assert_eq!(piece.label, "arithmetic-failure");
    let Judgment::Refuted { witness, .. } = &piece.judgment else {
        panic!("the refuted piece holds its witness")
    };
    assert_eq!(witness.original, Err(V2ExecutionFailure::Arithmetic));
    assert_eq!(witness.candidate, Ok(witness.input.clone()));
}

#[test]
fn an_output_domain_failure_is_a_replayed_difference() {
    let output = Domain::Int { min: 0, max: 2 };
    let original = bytes(vec![SMALL], vec![output], vec![Op::Input(0)], vec![0]);
    let candidate = bytes(vec![SMALL], vec![output], vec![Op::Int(0)], vec![0]);
    let Outcome::Counterexample(found) = check(&original, &candidate, SYMBOLIC, &mut reference)
    else {
        panic!("different success and refusal must be detected")
    };
    let piece = found
        .compared
        .pieces
        .iter()
        .find(|piece| piece.label == "output-domain-failure")
        .unwrap_or_else(|| panic!("output-domain difference piece is required"));
    let Judgment::Refuted { witness, .. } = &piece.judgment else {
        panic!("output-domain failure must have a replayed witness")
    };
    assert_eq!(witness.input, [3]);
    assert_eq!(witness.original, Err(V2ExecutionFailure::OutputDomain));
    assert_eq!(witness.candidate, Ok(vec![0]));
}

#[test]
fn later_outputs_and_duplicate_roots_are_compared_in_order() {
    let original = bytes(
        vec![SMALL, SMALL],
        vec![SMALL; 3],
        vec![Op::Input(0), Op::Input(1)],
        vec![0, 1, 0],
    );
    let candidate = bytes(
        vec![SMALL, SMALL],
        vec![SMALL; 3],
        vec![Op::Input(0), Op::Input(1)],
        vec![0, 0, 1],
    );
    let Outcome::Counterexample(found) = check(&original, &candidate, SYMBOLIC, &mut reference)
    else {
        panic!("equal first outputs must not hide different later outputs")
    };
    let Judgment::Refuted { witness, .. } = &found.compared.pieces[found.piece].judgment else {
        panic!("ordered outputs must have a replayed witness")
    };
    assert_eq!(witness.input, [0, 1]);
    assert_eq!(witness.original, Ok(vec![0, 1, 0]));
    assert_eq!(witness.candidate, Ok(vec![0, 0, 1]));
}

#[test]
fn inclusive_split_boundary_is_256_values_not_width_256() {
    for (max, count) in [(127, 258), (128, 3)] {
        let domain = Domain::Int { min: -128, max };
        let program = Program::try_new(vec![domain], vec![domain], vec![Op::Input(0)], vec![0])
            .unwrap_or_else(|error| panic!("boundary program: {error}"));
        let view = crate::symbolic::program::Program::of(&program);
        let queries = super::Queries::build(view, view)
            .unwrap_or_else(|error| panic!("boundary queries: {error:?}"));
        assert_eq!(queries.pieces.len(), count);
        if max == 127 {
            assert_eq!(queries.pieces[0].label, "case--128");
            assert_eq!(queries.pieces[255].label, "case-127");
            assert!(
                queries.pieces[0]
                    .source
                    .contains("(assert (= p0 (- 128)))\n")
            );
            assert!(queries.pieces[255].source.contains("(assert (= p0 127))\n"));
        } else {
            assert_eq!(queries.pieces[0].label, "outputs");
        }
        assert_eq!(queries.pieces[count - 2].label, "arithmetic-failure");
        assert_eq!(queries.pieces[count - 1].label, "output-domain-failure");
    }
}

#[test]
fn exactly_nonbinding_step_limit_checks_results_without_equal_usage() {
    let original = smaller();
    let candidate = smaller_rewritten();
    let limits = Limits {
        steps: 7,
        input_tuples: 0,
    };
    let Outcome::Equivalent(equal) = check(&original, &candidate, limits, &mut reference) else {
        panic!("seven Steps admits both complete eager executions")
    };
    let receipt: serde_json::Value =
        serde_json::from_slice(&equal.receipt(&serde_json::json!({"cvc5": null, "z3": null})))
            .unwrap_or_else(|error| panic!("receipt: {error}"));
    assert_eq!(receipt["evidence"], "attested");
    let Outcome::Inconclusive(stop) = check(
        &original,
        &candidate,
        Limits {
            steps: 6,
            input_tuples: 0,
        },
        &mut reference,
    ) else {
        panic!("one fewer Step must be inconclusive")
    };
    assert!(stop.cause.starts_with("budget-boundary"));
}

#[test]
fn a_domain_within_the_cap_is_left_to_the_exhaustive_check() {
    let limits = Limits {
        steps: transform::DEFAULT_STEP_LIMIT,
        input_tuples: 36,
    };
    assert!(matches!(
        check(&smaller(), &smaller_rewritten(), limits, &mut reference),
        Outcome::ExhaustiveFits {
            size: 36,
            limit: 36
        }
    ));
}

#[test]
fn refusals_follow_the_exhaustive_checkers_order() {
    let other_inputs = bytes(
        vec![SMALL],
        vec![Domain::Bool, SMALL],
        vec![Op::Input(0), Op::Lt(0, 0)],
        vec![1, 0],
    );
    assert!(matches!(
        check(&smaller(), &other_inputs, SYMBOLIC, &mut reference),
        Outcome::Refused(Refusal::InputAbi { .. })
    ));
    assert!(matches!(
        check(b"not a program", &smaller(), SYMBOLIC, &mut reference),
        Outcome::Refused(Refusal::NotAdmitted { .. })
    ));
    let low = Limits {
        steps: 3,
        input_tuples: 0,
    };
    let Outcome::Inconclusive(stop) = check(&smaller(), &smaller_rewritten(), low, &mut reference)
    else {
        panic!("a binding Step limit must be inconclusive")
    };
    assert!(stop.cause.starts_with("budget-boundary"), "{}", stop.cause);
}

#[test]
fn a_planted_solver_disagreement_or_unknown_is_inconclusive() {
    // Z3 claims a model that does not replay while CVC5 answers unsat.
    let mut disagree = |query: &Query<'_>| {
        let reference = reference(query);
        let bogus = Answer::Sat(BTreeMap::from([("x0".to_owned(), 0), ("x1".to_owned(), 0)]));
        match reference.cvc5 {
            Answer::Unsat { .. } => answers(reference.cvc5, bogus),
            _ => reference,
        }
    };
    let Outcome::Inconclusive(stop) =
        check(&smaller(), &smaller_rewritten(), SYMBOLIC, &mut disagree)
    else {
        panic!("a disagreement must be inconclusive")
    };
    assert!(
        stop.cause.contains("the solvers disagree"),
        "{}",
        stop.cause
    );

    let mut unknown = |query: &Query<'_>| {
        let reference = reference(query);
        match reference.cvc5 {
            Answer::Unsat { .. } => answers(Answer::Unknown, reference.z3),
            _ => reference,
        }
    };
    let Outcome::Inconclusive(stop) =
        check(&smaller(), &smaller_rewritten(), SYMBOLIC, &mut unknown)
    else {
        panic!("an unknown must be inconclusive")
    };
    assert!(
        stop.cause.contains("CVC5 answered unknown"),
        "{}",
        stop.cause
    );
}

#[test]
fn a_solver_that_answers_unsat_to_everything_fails_the_planted_control() {
    let mut always_unsat = |_: &Query<'_>| {
        answers(
            Answer::Unsat { proof_output: true },
            Answer::Unsat {
                proof_output: false,
            },
        )
    };
    let Outcome::Inconclusive(stop) = check(
        &smaller(),
        &smaller_rewritten(),
        SYMBOLIC,
        &mut always_unsat,
    ) else {
        panic!("a lying solver must not establish equivalence")
    };
    assert!(
        stop.cause
            .starts_with("the planted control was not refuted"),
        "{}",
        stop.cause
    );
}
