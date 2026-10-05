use super::*;
use alloc::vec;
use core::cell::RefCell;

#[derive(Default)]
struct Predicates(RefCell<Vec<(Identifier, Vec<i128>)>>);

impl PredicateProvider for Predicates {
    fn evaluate(&self, name: &Identifier, arguments: &[i128]) -> Option<bool> {
        self.0.borrow_mut().push((name.clone(), arguments.to_vec()));
        match name.as_str() {
            "yes" => Some(true),
            "no" => Some(false),
            _ => None,
        }
    }
}

fn name(value: &str) -> Identifier {
    Identifier::try_new(value).unwrap_or_else(|| unreachable!())
}

fn var(value: &str) -> ValueExpr {
    ValueExpr::Var(name(value))
}

fn sum(variable: &str, start: i128, end: i128, body: ValueExpr) -> ValueExpr {
    ValueExpr::Sum {
        variable: name(variable),
        start,
        end,
        body: Box::new(body),
    }
}

fn predicate(value: &str, arguments: Vec<ValueExpr>) -> RelExpr {
    RelExpr::Predicate {
        name: name(value),
        arguments: arguments.into_boxed_slice(),
    }
}

fn equal(left: ValueExpr, right: i128) -> RelExpr {
    RelExpr::Compare(CompareOp::Eq, left, ValueExpr::Int(right))
}

fn empty_step() -> TraceStep {
    TraceStep::try_new(Vec::new()).unwrap_or_else(|| unreachable!())
}

fn limits(operations: u64, quantifiers: u64, predicates: u64) -> EvalLimits {
    EvalLimits::try_new(operations, quantifiers, predicates).unwrap_or_else(|| unreachable!())
}

#[test]
fn nested_sums_restore_each_outer_binding_and_argument_order() {
    let formula = RelExpr::ForAll {
        variable: name("i"),
        start: 1,
        end: 4,
        body: Box::new(predicate(
            "yes",
            vec![
                var("i"),
                sum("i", 0, 3, var("i")),
                var("i"),
                sum(
                    "j",
                    0,
                    2,
                    ValueExpr::Add(Box::new(var("i")), Box::new(var("j"))),
                ),
                var("i"),
                sum("i", 0, 2, sum("i", 0, 3, var("i"))),
                var("i"),
            ],
        )),
    };
    let predicates = Predicates::default();
    let mut variables = Vec::new();
    assert_eq!(
        eval_rel(
            &formula,
            &empty_step(),
            &predicates,
            &mut variables,
            &mut Fuel::new(EvalLimits::default()),
        ),
        Ok(true)
    );
    assert!(variables.is_empty());
    assert_eq!(
        *predicates.0.borrow(),
        vec![
            (name("yes"), vec![1, 3, 1, 3, 1, 6, 1]),
            (name("yes"), vec![2, 3, 2, 5, 2, 6, 2]),
            (name("yes"), vec![3, 3, 3, 7, 3, 6, 3]),
        ]
    );
}

#[test]
fn nested_quantifiers_restore_bindings_after_early_success() {
    let formula = RelExpr::ForAll {
        variable: name("i"),
        start: 4,
        end: 7,
        body: Box::new(RelExpr::And(
            Box::new(RelExpr::Exists {
                variable: name("i"),
                start: 0,
                end: 10,
                body: Box::new(equal(var("i"), 2)),
            }),
            Box::new(predicate("yes", vec![var("i")])),
        )),
    };
    let predicates = Predicates::default();
    let mut variables = Vec::new();
    let mut fuel = Fuel::new(EvalLimits::default());
    assert_eq!(
        eval_rel(
            &formula,
            &empty_step(),
            &predicates,
            &mut variables,
            &mut fuel
        ),
        Ok(true)
    );
    assert!(variables.is_empty());
    assert_eq!(fuel.quantifiers, 12);
    assert_eq!(
        *predicates.0.borrow(),
        vec![
            (name("yes"), vec![4]),
            (name("yes"), vec![5]),
            (name("yes"), vec![6])
        ]
    );
}

#[test]
fn both_quantifiers_restore_bindings_on_early_and_complete_exits() {
    for body in [false, true] {
        for (formula, iterations) in [
            (
                RelExpr::ForAll {
                    variable: name("i"),
                    start: 0,
                    end: 4,
                    body: Box::new(RelExpr::Bool(body)),
                },
                if body { 4 } else { 1 },
            ),
            (
                RelExpr::Exists {
                    variable: name("i"),
                    start: 0,
                    end: 4,
                    body: Box::new(RelExpr::Bool(body)),
                },
                if body { 1 } else { 4 },
            ),
        ] {
            let mut fuel = Fuel::new(EvalLimits::default());
            let mut variables = Vec::new();
            assert_eq!(
                eval_rel(
                    &formula,
                    &empty_step(),
                    &Predicates::default(),
                    &mut variables,
                    &mut fuel
                ),
                Ok(body)
            );
            assert!(variables.is_empty());
            assert_eq!(
                (fuel.operations, fuel.quantifiers, fuel.predicates),
                (iterations + 1, iterations, 0)
            );
        }
    }
}

#[test]
fn quantifier_and_sum_bindings_are_removed_on_every_failure_path() {
    let bad_values = [
        (var("missing"), IndeterminateReason::MissingProjection),
        (
            ValueExpr::Div(
                DivisionMode::Exact,
                Box::new(ValueExpr::Int(1)),
                Box::new(ValueExpr::Int(0)),
            ),
            IndeterminateReason::DivisionByZero,
        ),
        (
            sum("i", 0, 2, ValueExpr::Int(i128::MAX)),
            IndeterminateReason::Overflow,
        ),
        (
            sum("i", 2, 1, var("missing")),
            IndeterminateReason::InvalidRange,
        ),
    ];
    for (body, reason) in bad_values {
        let formula = RelExpr::Exists {
            variable: name("i"),
            start: 0,
            end: 2,
            body: Box::new(equal(sum("i", 0, 2, body), 0)),
        };
        let mut variables = Vec::new();
        assert_eq!(
            eval_rel(
                &formula,
                &empty_step(),
                &Predicates::default(),
                &mut variables,
                &mut Fuel::new(EvalLimits::default())
            ),
            Err(reason)
        );
        assert!(variables.is_empty(), "{reason:?}");
    }
    let formula = RelExpr::ForAll {
        variable: name("i"),
        start: 0,
        end: 2,
        body: Box::new(equal(sum("i", 0, 2, var("i")), 1)),
    };
    for (bounds, reason, counts) in [
        (
            limits(100, 1, 100),
            IndeterminateReason::QuantifierLimit,
            (3, 2, 0),
        ),
        (
            limits(3, 100, 100),
            IndeterminateReason::OperationLimit,
            (4, 2, 0),
        ),
    ] {
        let mut variables = Vec::new();
        let mut fuel = Fuel::new(bounds);
        assert_eq!(
            eval_rel(
                &formula,
                &empty_step(),
                &Predicates::default(),
                &mut variables,
                &mut fuel
            ),
            Err(reason)
        );
        assert!(variables.is_empty());
        assert_eq!((fuel.operations, fuel.quantifiers, fuel.predicates), counts);
    }
}

#[test]
fn empty_extreme_and_invalid_ranges_keep_their_results_and_fuel() {
    for bound in [i128::MIN, 0, i128::MAX] {
        for (formula, expected) in [
            (
                RelExpr::ForAll {
                    variable: name("i"),
                    start: bound,
                    end: bound,
                    body: Box::new(predicate("missing", vec![])),
                },
                true,
            ),
            (
                RelExpr::Exists {
                    variable: name("i"),
                    start: bound,
                    end: bound,
                    body: Box::new(predicate("missing", vec![])),
                },
                false,
            ),
            (equal(sum("i", bound, bound, var("missing")), 0), true),
        ] {
            let mut fuel = Fuel::new(limits(100, 1, 1));
            assert_eq!(
                eval_rel(
                    &formula,
                    &empty_step(),
                    &Predicates::default(),
                    &mut Vec::new(),
                    &mut fuel
                ),
                Ok(expected)
            );
            assert_eq!((fuel.quantifiers, fuel.predicates), (0, 0));
        }
    }
    for (start, end) in [(i128::MIN, i128::MIN + 1), (i128::MAX - 1, i128::MAX)] {
        let formula = equal(sum("i", start, end, var("i")), start);
        assert_eq!(
            evaluate_relational(
                &formula,
                EvaluationContext::new(
                    &empty_step(),
                    &Predicates::default(),
                    EvalLimits::default()
                )
            ),
            EvalOutcome::True
        );
    }
}

#[test]
fn eager_boolean_evaluation_and_predicate_failure_order_are_preserved() {
    for formula in [
        RelExpr::And(
            Box::new(RelExpr::Bool(false)),
            Box::new(predicate("missing", vec![])),
        ),
        RelExpr::Or(
            Box::new(RelExpr::Bool(true)),
            Box::new(predicate("missing", vec![])),
        ),
        RelExpr::Implies(
            Box::new(RelExpr::Bool(false)),
            Box::new(predicate("missing", vec![])),
        ),
    ] {
        let predicates = Predicates::default();
        assert_eq!(
            evaluate_relational(
                &formula,
                EvaluationContext::new(&empty_step(), &predicates, EvalLimits::default())
            ),
            EvalOutcome::Indeterminate(IndeterminateReason::MissingPredicate)
        );
        assert_eq!(*predicates.0.borrow(), vec![(name("missing"), vec![])]);
    }
    let formula = RelExpr::And(
        Box::new(predicate("yes", vec![])),
        Box::new(predicate("missing", vec![sum("i", 0, 2, var("missing"))])),
    );
    for (bounds, reason, counts) in [
        (
            limits(100, 100, 1),
            IndeterminateReason::PredicateLimit,
            (3, 0, 2),
        ),
        (
            limits(100, 100, 2),
            IndeterminateReason::MissingProjection,
            (5, 1, 2),
        ),
    ] {
        let predicates = Predicates::default();
        let mut fuel = Fuel::new(bounds);
        let mut variables = Vec::new();
        assert_eq!(
            eval_rel(
                &formula,
                &empty_step(),
                &predicates,
                &mut variables,
                &mut fuel
            ),
            Err(reason)
        );
        assert!(variables.is_empty());
        assert_eq!((fuel.operations, fuel.quantifiers, fuel.predicates), counts);
        assert_eq!(*predicates.0.borrow(), vec![(name("yes"), vec![])]);
    }
}

#[test]
fn scalar_operands_keep_the_first_error_inside_nested_sums() {
    let left = sum(
        "i",
        0,
        2,
        ValueExpr::Div(
            DivisionMode::Exact,
            Box::new(ValueExpr::Int(1)),
            Box::new(ValueExpr::Int(0)),
        ),
    );
    let right = sum("i", 0, 2, var("missing"));
    for value in [
        ValueExpr::Add(Box::new(left.clone()), Box::new(right.clone())),
        ValueExpr::Sub(Box::new(left.clone()), Box::new(right.clone())),
        ValueExpr::Mul(Box::new(left.clone()), Box::new(right.clone())),
        ValueExpr::Div(DivisionMode::Exact, Box::new(left), Box::new(right)),
    ] {
        let formula = equal(value, 0);
        let mut fuel = Fuel::new(EvalLimits::default());
        let mut variables = Vec::new();
        assert_eq!(
            eval_rel(
                &formula,
                &empty_step(),
                &Predicates::default(),
                &mut variables,
                &mut fuel
            ),
            Err(IndeterminateReason::DivisionByZero)
        );
        assert!(variables.is_empty());
        assert_eq!(
            (fuel.operations, fuel.quantifiers, fuel.predicates),
            (6, 1, 0)
        );
    }
}
