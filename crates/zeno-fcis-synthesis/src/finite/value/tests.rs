use super::{ArithmeticOp, Atom, IntegerWidth, NumericFailure, arithmetic, legacy_arithmetic};

#[test]
fn legacy_boundary_pairs_match_independent_wider_arithmetic() {
    let values = [
        i64::MIN,
        i64::MIN + 1,
        -2,
        -1,
        0,
        1,
        2,
        i64::MAX - 1,
        i64::MAX,
    ];
    for left in values {
        for right in values {
            for op in [ArithmeticOp::Add, ArithmeticOp::Sub] {
                let exact = match op {
                    ArithmeticOp::Add => i128::from(left) + i128::from(right),
                    ArithmeticOp::Sub => i128::from(left) - i128::from(right),
                };
                assert_eq!(
                    legacy_arithmetic(op, left, right),
                    i64::try_from(exact).ok()
                );
            }
        }
    }
}

#[test]
fn width_is_semantic_and_checks_operands_before_result() {
    let above = i128::from(i64::MAX) + 1;
    assert_eq!(
        arithmetic(
            IntegerWidth::Signed64,
            ArithmeticOp::Add,
            Atom::I128(i128::from(i64::MAX)),
            Atom::I128(1)
        ),
        Err(NumericFailure::Range)
    );
    assert_eq!(
        arithmetic(
            IntegerWidth::Signed128,
            ArithmeticOp::Add,
            Atom::I128(i128::from(i64::MAX)),
            Atom::I128(1)
        ),
        Ok(Atom::I128(above))
    );
    // The mathematical result fits, but neither original operand is signed64.
    assert_eq!(
        arithmetic(
            IntegerWidth::Signed64,
            ArithmeticOp::Sub,
            Atom::I128(above),
            Atom::I128(above)
        ),
        Err(NumericFailure::Range)
    );
    assert_eq!(
        arithmetic(
            IntegerWidth::Signed64,
            ArithmeticOp::Add,
            Atom::I128(i128::from(i64::MIN) - 1),
            Atom::I128(1)
        ),
        Err(NumericFailure::Range)
    );
}

#[test]
fn full_width_endpoints_and_kinds_have_exact_failures() {
    for (width, op, left, right, expected) in [
        (
            IntegerWidth::Signed128,
            ArithmeticOp::Add,
            Atom::I128(i128::MAX),
            Atom::I128(1),
            Err(NumericFailure::Range),
        ),
        (
            IntegerWidth::Signed128,
            ArithmeticOp::Sub,
            Atom::I128(i128::MIN),
            Atom::I128(1),
            Err(NumericFailure::Range),
        ),
        (
            IntegerWidth::Signed128,
            ArithmeticOp::Sub,
            Atom::I128(i128::MIN),
            Atom::I128(i128::MIN),
            Ok(Atom::I128(0)),
        ),
        (
            IntegerWidth::Signed128,
            ArithmeticOp::Add,
            Atom::I128(i128::MAX),
            Atom::I128(i128::MIN),
            Ok(Atom::I128(-1)),
        ),
        (
            IntegerWidth::Unsigned128,
            ArithmeticOp::Add,
            Atom::U128(u128::MAX),
            Atom::U128(1),
            Err(NumericFailure::Range),
        ),
        (
            IntegerWidth::Unsigned128,
            ArithmeticOp::Sub,
            Atom::U128(0),
            Atom::U128(1),
            Err(NumericFailure::Range),
        ),
        (
            IntegerWidth::Unsigned128,
            ArithmeticOp::Sub,
            Atom::U128(u128::MAX),
            Atom::U128(u128::MAX),
            Ok(Atom::U128(0)),
        ),
        (
            IntegerWidth::Unsigned128,
            ArithmeticOp::Add,
            Atom::U128(u128::MAX),
            Atom::U128(0),
            Ok(Atom::U128(u128::MAX)),
        ),
    ] {
        assert_eq!(arithmetic(width, op, left, right), expected);
    }
    for width in [
        IntegerWidth::Signed64,
        IntegerWidth::Signed128,
        IntegerWidth::Unsigned128,
    ] {
        for (left, right) in [
            (Atom::Bool(true), Atom::I128(1)),
            (Atom::I128(1), Atom::U128(1)),
            (Atom::Bytes(b""), Atom::Text(b"")),
        ] {
            assert_eq!(
                arithmetic(width, ArithmeticOp::Add, left, right),
                Err(NumericFailure::Kind)
            );
        }
    }
}

#[test]
fn raw_main_node_semantics_and_eager_traps_remain_visible() {
    use super::super::evaluation::{Domain, Failure, Op, evaluate_into, evaluate_node};
    use alloc::vec;
    use alloc::vec::Vec;
    assert_eq!(evaluate_node(&Op::Not(0), &[], &[2]), Ok(0));
    assert_eq!(evaluate_node(&Op::And(0, 1), &[], &[2, 1]), Ok(0));
    assert_eq!(evaluate_node(&Op::Select(0, 1, 2), &[], &[2, 5, 9]), Ok(9));
    assert_eq!(
        evaluate_node(&Op::Add(0, 9), &[], &[i128::MAX]),
        Err(Failure::Reference)
    );
    let graph = [
        Op::Bool(false),
        Op::Int(i128::MAX),
        Op::Int(1),
        Op::Add(1, 2),
        Op::Select(0, 3, 2),
    ];
    for roots in [&[2_u16][..], &[4_u16][..]] {
        let mut scratch = vec![42];
        let mut output = Vec::new();
        assert_eq!(
            evaluate_into(
                &[],
                &[Domain::Int {
                    min: i128::MIN,
                    max: i128::MAX
                }],
                &graph,
                roots,
                &[],
                &mut scratch,
                &mut output
            ),
            Err(Failure::Arithmetic)
        );
        assert!(scratch.is_empty() && output.is_empty());
    }
}
