use alloc::{boxed::Box, vec};
use production::{Identifier, ProjectionPath, ProjectionRoot, RelExpr, StableId};

use crate::lexical_core::{LexemeKind as Kind, next_lexeme};
use crate::value_expr::{
    DivisionMode, ValueExpr, binary_height, division, fold_arithmetic, read_division_mode,
};

type Expr = ValueExpr;

fn operands() -> (Expr, Expr) {
    (
        Expr::Sub(
            Box::new(Expr::Var(Identifier::try_new("left").unwrap())),
            Box::new(Expr::Int(-7)),
        ),
        Expr::Select(
            Box::new(RelExpr::Bool(false)),
            Box::new(Expr::U128(u128::MAX)),
            Box::new(Expr::Projection(
                ProjectionPath::try_new(ProjectionRoot::Pre, vec![StableId::new(19).unwrap()])
                    .unwrap(),
            )),
        ),
    )
}

#[test]
fn actual_arithmetic_nodes_keep_complete_ordered_operands() {
    for (kind, additive) in [(Kind::Plus, true), (Kind::Minus, true), (Kind::Star, false)] {
        let (left, right) = operands();
        let result = fold_arithmetic(kind, additive, left.clone(), right.clone()).unwrap();
        let (actual_left, actual_right) = match (kind, result) {
            (Kind::Plus, Expr::Add(a, b))
            | (Kind::Minus, Expr::Sub(a, b))
            | (Kind::Star, Expr::Mul(a, b)) => (a, b),
            _ => panic!("wrong actual AST constructor"),
        };
        assert_eq!(*actual_left, left);
        assert_eq!(*actual_right, right);
    }
}

#[test]
fn arithmetic_operators_refuse_the_wrong_precedence_level() {
    for (kind, additive) in [
        (Kind::Plus, false),
        (Kind::Minus, false),
        (Kind::Star, true),
    ] {
        let (left, right) = operands();
        assert_eq!(fold_arithmetic(kind, additive, left, right), None);
    }
}

#[test]
fn nonarithmetic_tokens_never_construct_an_arithmetic_node() {
    for kind in [
        Kind::Slash,
        Kind::Number(1),
        Kind::Ident,
        Kind::EqEq,
        Kind::End,
        Kind::InvalidByte,
    ] {
        for additive in [false, true] {
            let (left, right) = operands();
            assert_eq!(fold_arithmetic(kind, additive, left, right), None);
        }
    }
}

#[test]
fn left_fold_steps_preserve_nonassociative_subtraction_shape() {
    let first = fold_arithmetic(Kind::Minus, true, Expr::Int(10), Expr::Int(3)).unwrap();
    let actual = fold_arithmetic(Kind::Minus, true, first, Expr::Int(2)).unwrap();
    assert_eq!(
        actual,
        Expr::Sub(
            Box::new(Expr::Sub(Box::new(Expr::Int(10)), Box::new(Expr::Int(3)))),
            Box::new(Expr::Int(2)),
        )
    );
    assert_ne!(
        actual,
        Expr::Sub(
            Box::new(Expr::Int(10)),
            Box::new(Expr::Sub(Box::new(Expr::Int(3)), Box::new(Expr::Int(2)))),
        )
    );
}

#[test]
fn lexical_operator_identity_reaches_the_actual_constructor() {
    for (spelling, additive) in [
        (b"+".as_slice(), true),
        (b"-".as_slice(), true),
        (b"*".as_slice(), false),
    ] {
        let token = next_lexeme(spelling, 0);
        assert_eq!(token.end, spelling.len());
        let result = fold_arithmetic(token.kind, additive, Expr::Int(1), Expr::Int(2)).unwrap();
        assert!(matches!(
            (spelling, result),
            (b"+", Expr::Add(_, _)) | (b"-", Expr::Sub(_, _)) | (b"*", Expr::Mul(_, _))
        ));
    }
}

#[test]
fn division_keeps_all_three_modes_and_ordered_operands() {
    for mode in [DivisionMode::Exact, DivisionMode::Floor, DivisionMode::Ceil] {
        let (left, right) = operands();
        match division(mode, left.clone(), right.clone()) {
            Expr::Div(actual_mode, actual_left, actual_right) => {
                assert_eq!(actual_mode, mode);
                assert_eq!(*actual_left, left);
                assert_eq!(*actual_right, right);
            }
            _ => panic!("not a division node"),
        }
    }
}

#[test]
fn complete_division_names_have_exact_policies() {
    assert_eq!(read_division_mode(b"div_exact"), Some(DivisionMode::Exact));
    assert_eq!(read_division_mode(b"div_floor"), Some(DivisionMode::Floor));
    assert_eq!(read_division_mode(b"div_ceil"), Some(DivisionMode::Ceil));
    for word in [
        b"".as_slice(),
        b"div",
        b"div_floo",
        b"div_floors",
        b" div_ceil",
        b"div_ceil ",
        b"DIV_EXACT",
        b"div_exact\0",
        b"mul_div_floor",
    ] {
        assert_eq!(read_division_mode(word), None, "{word:?}");
    }
}

#[test]
fn binary_height_matches_checked_arithmetic_on_small_complete_domain() {
    for left in 0usize..9 {
        for right in 0usize..9 {
            for limit in 0usize..10 {
                let expected = left
                    .max(right)
                    .checked_add(1)
                    .filter(|value| *value <= limit);
                assert_eq!(binary_height(left, right, limit), expected);
            }
        }
    }
}

#[test]
fn binary_height_refuses_overflow_and_inclusive_boundary_excess() {
    assert_eq!(binary_height(0, 0, 0), None);
    assert_eq!(binary_height(0, 0, 1), Some(1));
    assert_eq!(binary_height(256, 257, 258), Some(258));
    assert_eq!(binary_height(257, 258, 258), None);
    assert_eq!(binary_height(usize::MAX, 0, usize::MAX), None);
    assert_eq!(binary_height(0, usize::MAX, usize::MAX), None);
    assert_eq!(
        binary_height(usize::MAX - 1, 0, usize::MAX),
        Some(usize::MAX)
    );
    assert_eq!(
        binary_height(0, usize::MAX - 1, usize::MAX),
        Some(usize::MAX)
    );
}

#[test]
fn production_enum_keeps_glob_imports_and_unannotated_inference() {
    use production::ValueExpr::*;
    let literal = Int(3);
    assert!(matches!(literal, Int(3)));
    let variable = Var(Identifier::try_new("v").unwrap());
    assert!(matches!(variable, Var(_)));
}

#[test]
fn shared_enum_keeps_glob_imports_and_unannotated_inference() {
    use crate::value_expr::ValueExpr::*;
    let literal = U128(u128::MAX);
    assert!(matches!(literal, U128(u128::MAX)));
    let expression = Sub(Box::new(Int(7)), Box::new(Int(2)));
    assert!(matches!(expression, Sub(_, _)));
}
