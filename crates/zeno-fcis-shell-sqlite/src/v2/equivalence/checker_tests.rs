use super::*;
use alloc::vec;
use zeno_fcis_synthesis::finite::Op;

fn ok<T>(result: Result<T, Failure>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected checker refusal: {error:?}"),
    }
}

fn refusal<T>(result: Result<T, Failure>) -> Failure {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected a checker refusal"),
    }
}

fn program<'a>(
    inputs: &'a [Domain],
    outputs: &'a [Domain],
    nodes: &'a [Op],
    roots: &'a [u16],
) -> V2ScalarProgram<'a> {
    V2ScalarProgram {
        inputs,
        outputs,
        nodes,
        roots,
    }
}

#[test]
fn late_empty_domain_wins_over_product_overflow_and_cap() {
    let full = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    let empty = Domain::Int { min: 1, max: 0 };
    assert_eq!(domain_size(&[full]), Ok(Some(1u128 << 64)));
    assert_eq!(domain_size(&[full, full]), Ok(None));
    assert_eq!(domain_size(&[full, full, empty]), Err(2));
    let inputs = [full, full, empty];
    let p = program(&inputs, &[], &[], &[]);
    assert_eq!(
        validate_pair(&p, &p, 0),
        Err(Failure::EmptyInputDomain { position: 2 })
    );
}

#[test]
fn empty_product_is_one_evaluated_tuple_and_identity_still_checks_cap() {
    let p = program(&[], &[], &[], &[]);
    let done = ok(compare_with_usage(&p, &p, 1, 0));
    assert_eq!(done.tuples(), 1);
    assert_eq!(
        done.usage(),
        Usage {
            max_steps: [0, 0],
            candidate_uses_more: 0,
            usage_preserved: true
        }
    );
    assert_eq!(ok(compare_equal(&p, &p, 1)).tuples(), 1);
    assert!(!ok(compare_equal(&p, &p, 1)).enumerated());
    assert!(matches!(
        compare_equal(&p, &p, 0),
        Err(Failure::DomainTooLarge {
            size: Some(1),
            limit: 0
        })
    ));
}

#[test]
fn late_mismatch_beats_earlier_step_overruns_and_uses_last_input_fastest_order() {
    let inputs = [Domain::Bool, Domain::Int { min: -1, max: 2 }];
    let outputs = [Domain::Int { min: -1, max: 2 }];
    let plain = [Op::Input(1)];
    let changed = [
        Op::Input(0),
        Op::Input(1),
        Op::Int(2),
        Op::Eq(1, 2),
        Op::And(0, 3),
        Op::Int(0),
        Op::Select(4, 5, 1),
    ];
    let left = program(&inputs, &outputs, &plain, &[0]);
    let right = program(&inputs, &outputs, &changed, &[6]);
    assert_eq!(
        refusal(compare_with_usage(&left, &right, 8, 0)),
        Failure::Counterexample {
            ordinal: 7,
            input: vec![1, 2],
            original: Observation {
                result: Ok(vec![2]),
                steps: 1
            },
            candidate: Observation {
                result: Ok(vec![0]),
                steps: 7
            },
        }
    );
    assert_eq!(inputs, [Domain::Bool, Domain::Int { min: -1, max: 2 }]);
    assert_eq!(plain, [Op::Input(1)]);
}

#[test]
fn trapped_attempts_and_strict_step_boundaries_are_counted() {
    let inputs = [Domain::Bool];
    let outputs = [Domain::Bool];
    let left_nodes = [Op::Input(0), Op::Int(i64::MAX), Op::Add(1, 1)];
    let right_nodes = [
        Op::Input(0),
        Op::Bool(true),
        Op::Int(i64::MAX),
        Op::Add(2, 2),
    ];
    let left = program(&inputs, &outputs, &left_nodes, &[0]);
    let right = program(&inputs, &outputs, &right_nodes, &[0]);
    let usage = Usage {
        max_steps: [3, 4],
        candidate_uses_more: 2,
        usage_preserved: false,
    };
    assert_eq!(
        refusal(compare_with_usage(&left, &right, 2, 3)),
        Failure::BudgetBoundary {
            over_limit: [0, 2],
            minimum_limit: 4,
            usage,
        }
    );
    assert_eq!(ok(compare_with_usage(&left, &right, 2, 4)).usage(), usage);
    assert!(ok(compare_equal(&left, &right, 2)).enumerated());
}

#[test]
fn full_outputs_and_exact_failure_classes_decide_equality() {
    let outputs = [Domain::Bool, Domain::Bool];
    let nodes = [Op::Bool(false), Op::Bool(true)];
    let left = program(&[], &outputs, &nodes, &[0, 0]);
    let right = program(&[], &outputs, &nodes, &[0, 1]);
    assert!(matches!(
        compare_equal(&left, &right, 1),
        Err(Failure::Counterexample { ordinal: 0, .. })
    ));
    let trap = [Op::Int(i64::MAX), Op::Add(0, 0)];
    let invalid = [Op::Input(0)];
    let left = program(&[], &[], &trap, &[]);
    let right = program(&[], &[], &invalid, &[]);
    assert_eq!(
        refusal(compare_with_usage(&left, &right, 1, 0)),
        Failure::Counterexample {
            ordinal: 0,
            input: vec![],
            original: Observation {
                result: Err(V2ExecutionFailure::Arithmetic),
                steps: 2
            },
            candidate: Observation {
                result: Err(V2ExecutionFailure::Reference),
                steps: 1
            },
        }
    );
}

#[test]
fn raw_program_full_budget_failure_is_preserved_without_claiming_admission() {
    let nodes = vec![Op::Int(0); MAX_NODES + 1];
    let extra = vec![Op::Int(0); MAX_NODES + 2];
    let left = program(&[], &[], &nodes, &[]);
    let right = program(&[], &[], &extra, &[]);
    let observed = observe(&left, &[]);
    assert!(matches!(
        observed.result,
        Err(V2ExecutionFailure::Budget(_))
    ));
    assert_eq!(observed.steps, FULL_BUDGET);
    assert_eq!(
        ok(compare_with_usage(&left, &right, 1, FULL_BUDGET)).usage(),
        Usage {
            max_steps: [FULL_BUDGET, FULL_BUDGET],
            candidate_uses_more: 0,
            usage_preserved: true,
        }
    );
}
