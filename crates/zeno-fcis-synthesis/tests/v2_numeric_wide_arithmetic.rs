//! Full-width math, refusal precedence and eager logical accounting at the public API.
use std::{vec, vec::Vec};
use zeno_fcis_synthesis::finite::{
    V2NumericAtom as Atom, V2NumericDomain as Domain, V2NumericFailure as Failure,
    V2NumericOp as Op, V2NumericProgram as Program, V2Resource as Resource, execute_numeric_v2,
    v2_zero_limits,
};

const UNSIGNED: Domain<'static> = Domain::U128 {
    min: 0,
    max: u128::MAX,
};

fn evaluate(
    nodes: &[Op],
    result: Domain<'_>,
    root: u16,
    steps: u64,
) -> (Result<Vec<Atom<'static>>, Failure>, u64) {
    let program = Program {
        inputs: &[],
        outputs: &[result],
        nodes,
        roots: &[root],
    };
    let (outcome, usage) = execute_numeric_v2(
        program,
        &[],
        v2_zero_limits().with_limit(Resource::Step, steps),
    )
    .into_parts();
    (outcome, usage.used(Resource::Step))
}

#[test]
fn exact_products_can_overflow_u128_while_their_quotient_fits() {
    let nodes = [Op::U128(u128::MAX), Op::MulDivFloor(0, 0, 0)];
    let p = Program {
        inputs: &[],
        outputs: &[UNSIGNED],
        nodes: &nodes,
        roots: &[1],
    };
    assert_eq!(p.admit(), Ok(()));
    assert_eq!(
        evaluate(&nodes, UNSIGNED, 1, 2),
        (Ok(vec![Atom::U128(u128::MAX)]), 2)
    );
    let nodes = [
        Op::U128(10u128.pow(30)),
        Op::U128(1_000),
        Op::U128(10_000),
        Op::MulDivCeil(0, 1, 2),
    ];
    assert_eq!(
        evaluate(&nodes, UNSIGNED, 3, 4),
        (Ok(vec![Atom::U128(10u128.pow(29))]), 4)
    );
}

#[test]
fn ceiling_overflow_is_distinct_from_a_fitting_floor() {
    // (MAX-1)^2 = MAX*(MAX-2) + 1. Floor fits; rounding upward does not.
    let mut nodes = [
        Op::U128(u128::MAX - 1),
        Op::U128(u128::MAX - 2),
        Op::MulDivFloor(0, 0, 1),
    ];
    assert_eq!(
        evaluate(&nodes, UNSIGNED, 2, 3),
        (Ok(vec![Atom::U128(u128::MAX)]), 3)
    );
    nodes[2] = Op::MulDivCeil(0, 0, 1);
    assert_eq!(
        evaluate(&nodes, UNSIGNED, 2, 3),
        (Err(Failure::Arithmetic), 3)
    );
}

#[test]
fn complete_three_factor_comparison_handles_the_384_bit_boundary() {
    let nodes = [
        Op::U128(u128::MAX),
        Op::U128(u128::MAX - 1),
        Op::ProductsGe([0, 0, 0], [1, 0, 0]),
    ];
    assert_eq!(
        evaluate(&nodes, Domain::Bool, 2, 3),
        (Ok(vec![Atom::Bool(true)]), 3)
    );
    let nodes = [
        Op::U128(u128::MAX),
        Op::U128(u128::MAX - 1),
        Op::ProductsGe([1, 0, 0], [0, 0, 0]),
    ];
    assert_eq!(
        evaluate(&nodes, Domain::Bool, 2, 3),
        (Ok(vec![Atom::Bool(false)]), 3)
    );
    let nodes = [Op::U128(u128::MAX), Op::ProductsGe([0, 0, 0], [0, 0, 0])];
    assert_eq!(
        evaluate(&nodes, Domain::Bool, 1, 2),
        (Ok(vec![Atom::Bool(true)]), 2)
    );
}

#[test]
fn all_references_precede_kind_and_arithmetic_checks() {
    let nodes = [Op::Bool(false), Op::MulDivFloor(0, 0, 9)];
    assert_eq!(
        evaluate(&nodes, UNSIGNED, 1, 2),
        (Err(Failure::Reference), 2)
    );
    let nodes = [Op::Bool(false), Op::MulDivCeil(0, 0, 0)];
    assert_eq!(evaluate(&nodes, UNSIGNED, 1, 2), (Err(Failure::Kind), 2));
    let nodes = [Op::Bool(false), Op::ProductsGe([0, 0, 0], [0, 0, 9])];
    assert_eq!(
        evaluate(&nodes, Domain::Bool, 1, 2),
        (Err(Failure::Reference), 2)
    );
    let nodes = [Op::Bool(false), Op::ProductsGe([0, 0, 0], [0, 0, 0])];
    assert_eq!(
        evaluate(&nodes, Domain::Bool, 1, 2),
        (Err(Failure::Kind), 2)
    );
}

#[test]
fn an_unused_zero_denominator_is_eagerly_refused_after_its_fetch_charge() {
    let nodes = [Op::U128(0), Op::U128(1), Op::MulDivFloor(1, 1, 0)];
    let p = Program {
        inputs: &[],
        outputs: &[UNSIGNED],
        nodes: &nodes,
        roots: &[0],
    };
    assert_eq!(p.admit(), Ok(()));
    assert_eq!(
        evaluate(&nodes, UNSIGNED, 0, 3),
        (Err(Failure::Arithmetic), 3)
    );
    let (result, usage) =
        execute_numeric_v2(p, &[], v2_zero_limits().with_limit(Resource::Step, 2)).into_parts();
    assert!(
        matches!(result, Err(Failure::Budget(e)) if e.resource == Resource::Step && e.attempted == 3)
    );
    assert_eq!(usage.used(Resource::Step), 2);
}
