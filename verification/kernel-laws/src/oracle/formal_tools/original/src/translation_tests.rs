//! Regression and operator coverage for the generated propositions.

use super::*;
use zeno_fcis_spec::{
    DivisionMode, EvalLimits, EvalOutcome, EvaluationContext, Identifier, IndeterminateReason,
    Observation, PredicateProvider, TraceStep, evaluate_relational,
};

fn atom() -> TemporalFormula {
    TemporalFormula::Atom(RelExpr::Compare(
        CompareOp::Greater,
        ValueExpr::Projection(
            ProjectionPath::try_new(
                ProjectionRoot::Pre,
                vec![zeno_fcis_spec::StableId::new(100).unwrap_or_else(|| unreachable!())],
            )
            .unwrap_or_else(|| unreachable!()),
        ),
        ValueExpr::Int(0),
    ))
}

fn rendered(formula: &TemporalFormula) -> LeanBool {
    render_temporal_lean(
        formula,
        "0",
        &BTreeMap::new(),
        &mut LeanRenderBudget::new(ExportLimits::default()),
    )
    .unwrap_or_else(|error| panic!("render: {error:?}"))
}

#[test]
fn nested_temporal_quantifiers_retain_the_outer_time_bound() {
    // G F p must ask for a witness after each outer time. Reusing the name
    // of the outer binder silently changes that lower bound to t >= t.
    let formula = TemporalFormula::Always(Box::new(TemporalFormula::Eventually(Box::new(atom()))));
    let result = rendered(&formula);
    assert!(result.term.contains("∀ time_0 : Nat, time_0 >= 0"));
    assert!(result.term.contains("∃ time_1 : Nat, time_1 >= time_0"));
    assert!(result.defined.contains("∀ time_1 : Nat, time_1 >= time_0"));
    assert!(result.term.contains("(observe \"pre_100\" time_1).val"));
}

#[test]
fn temporal_next_keeps_its_offset_inside_a_nested_quantifier() {
    let formula = TemporalFormula::Eventually(Box::new(TemporalFormula::Next(Box::new(
        TemporalFormula::Always(Box::new(atom())),
    ))));
    let result = rendered(&formula);
    assert!(
        result
            .term
            .contains("∀ time_1 : Nat, time_1 >= (time_0 + 1)")
    );
    assert!(
        result
            .defined
            .contains("∀ time_1 : Nat, time_1 >= (time_0 + 1)")
    );
}

#[test]
fn nested_until_has_distinct_witness_and_prefix_times() {
    let formula = TemporalFormula::Until(
        Box::new(TemporalFormula::Always(Box::new(atom()))),
        Box::new(TemporalFormula::Eventually(Box::new(atom()))),
    );
    let result = rendered(&formula);
    assert!(result.term.contains("∃ time_0 : Nat, time_0 >= 0"));
    assert!(
        result
            .term
            .contains("∀ time_1 : Nat, 0 <= time_1 → time_1 < time_0")
    );
    assert!(result.term.contains("∀ time_2 : Nat, time_2 >= time_1"));
    assert!(result.term.contains("∃ time_3 : Nat, time_3 >= time_0"));
    assert!(result.defined.contains("∀ time_2 : Nat, time_2 >= time_1"));
    assert!(result.defined.contains("∀ time_3 : Nat, time_3 >= time_0"));
}

struct ReferencePredicates;
impl PredicateProvider for ReferencePredicates {
    fn evaluate(&self, name: &Identifier, arguments: &[i128]) -> Option<bool> {
        (name.as_str() == "positive").then_some(arguments == [1])
    }
}

fn variable() -> Identifier {
    Identifier::try_new("index").unwrap_or_else(|| unreachable!())
}

fn equal(left: ValueExpr, right: i128) -> RelExpr {
    RelExpr::Compare(CompareOp::Eq, left, ValueExpr::Int(right))
}

fn divide(mode: DivisionMode, left: i128, right: i128) -> ValueExpr {
    ValueExpr::Div(
        mode,
        Box::new(ValueExpr::Int(left)),
        Box::new(ValueExpr::Int(right)),
    )
}

fn operator_cases() -> Vec<(&'static str, RelExpr, EvalOutcome)> {
    use EvalOutcome::{False, Indeterminate, True};
    use IndeterminateReason::{DivisionByZero, NonExactDivision, Overflow};
    let int = |value| Box::new(ValueExpr::Int(value));
    let sum = |start, end, body| ValueExpr::Sum {
        variable: variable(),
        start,
        end,
        body: Box::new(body),
    };
    let mut cases = vec![
        ("true", RelExpr::Bool(true), True),
        ("false", RelExpr::Bool(false), False),
        ("not", RelExpr::Not(Box::new(RelExpr::Bool(false))), True),
        (
            "and",
            RelExpr::And(
                Box::new(RelExpr::Bool(true)),
                Box::new(RelExpr::Bool(false)),
            ),
            False,
        ),
        (
            "or",
            RelExpr::Or(
                Box::new(RelExpr::Bool(false)),
                Box::new(RelExpr::Bool(true)),
            ),
            True,
        ),
        (
            "implies",
            RelExpr::Implies(
                Box::new(RelExpr::Bool(false)),
                Box::new(RelExpr::Bool(false)),
            ),
            True,
        ),
        ("add", equal(ValueExpr::Add(int(-3), int(7)), 4), True),
        (
            "subtract",
            equal(ValueExpr::Sub(int(-3), int(7)), -10),
            True,
        ),
        (
            "multiply",
            equal(ValueExpr::Mul(int(-3), int(7)), -21),
            True,
        ),
        (
            "add_overflow",
            equal(ValueExpr::Add(int(i128::MAX), int(1)), 0),
            Indeterminate(Overflow),
        ),
        (
            "subtract_overflow",
            equal(ValueExpr::Sub(int(i128::MIN), int(1)), 0),
            Indeterminate(Overflow),
        ),
        (
            "multiply_overflow",
            equal(ValueExpr::Mul(int(i128::MAX), int(2)), 0),
            Indeterminate(Overflow),
        ),
        (
            "exact_negative",
            equal(divide(DivisionMode::Exact, 6, -3), -2),
            True,
        ),
        (
            "exact_remainder",
            equal(divide(DivisionMode::Exact, -7, 3), -2),
            Indeterminate(NonExactDivision),
        ),
        (
            "zero_divisor",
            equal(divide(DivisionMode::Floor, 7, 0), 0),
            Indeterminate(DivisionByZero),
        ),
        (
            "division_overflow",
            equal(divide(DivisionMode::Exact, i128::MIN, -1), 0),
            Indeterminate(Overflow),
        ),
        (
            "sum_signed",
            equal(sum(-2, 3, ValueExpr::Var(variable())), 0),
            True,
        ),
        ("sum_empty", equal(sum(0, 0, ValueExpr::Int(7)), 0), True),
        (
            "sum_overflow",
            equal(sum(0, 2, ValueExpr::Int(i128::MAX)), 0),
            Indeterminate(Overflow),
        ),
        (
            "forall_empty",
            RelExpr::ForAll {
                variable: variable(),
                start: 0,
                end: 0,
                body: Box::new(RelExpr::Bool(false)),
            },
            True,
        ),
        (
            "exists_empty",
            RelExpr::Exists {
                variable: variable(),
                start: 0,
                end: 0,
                body: Box::new(RelExpr::Bool(true)),
            },
            False,
        ),
        (
            "forall_values",
            RelExpr::ForAll {
                variable: variable(),
                start: -2,
                end: 3,
                body: Box::new(RelExpr::Compare(
                    CompareOp::Less,
                    ValueExpr::Var(variable()),
                    ValueExpr::Int(3),
                )),
            },
            True,
        ),
        (
            "exists_value",
            RelExpr::Exists {
                variable: variable(),
                start: -2,
                end: 3,
                body: Box::new(equal(ValueExpr::Var(variable()), 1)),
            },
            True,
        ),
        (
            "predicate",
            RelExpr::Predicate {
                name: Identifier::try_new("positive").unwrap_or_else(|| unreachable!()),
                arguments: vec![ValueExpr::Int(1)].into_boxed_slice(),
            },
            True,
        ),
    ];
    for (label, operation, expected) in [
        ("eq", CompareOp::Eq, False),
        ("ne", CompareOp::NotEq, True),
        ("lt", CompareOp::Less, False),
        ("le", CompareOp::LessEq, False),
        ("gt", CompareOp::Greater, True),
        ("ge", CompareOp::GreaterEq, True),
    ] {
        cases.push((
            label,
            RelExpr::Compare(operation, ValueExpr::Int(7), ValueExpr::Int(3)),
            expected,
        ));
    }
    for (label, left, right, floor, ceil) in [
        ("positive_division", 7, 3, 2, 3),
        ("negative_numerator", -7, 3, -3, -2),
        ("negative_divisor", 7, -3, -3, -2),
        ("both_negative", -7, -3, 2, 3),
    ] {
        cases.push((
            label,
            equal(divide(DivisionMode::Floor, left, right), floor),
            True,
        ));
        cases.push((
            label,
            equal(divide(DivisionMode::Ceil, left, right), ceil),
            True,
        ));
    }
    let undefined = equal(divide(DivisionMode::Exact, 1, 0), 0);
    cases.push((
        "strict_and",
        RelExpr::And(Box::new(RelExpr::Bool(false)), Box::new(undefined.clone())),
        Indeterminate(DivisionByZero),
    ));
    cases.push((
        "strict_or",
        RelExpr::Or(Box::new(RelExpr::Bool(true)), Box::new(undefined.clone())),
        Indeterminate(DivisionByZero),
    ));
    cases.push((
        "strict_implication",
        RelExpr::Implies(Box::new(RelExpr::Bool(false)), Box::new(undefined)),
        Indeterminate(DivisionByZero),
    ));
    let first_one_then_undefined = ValueExpr::Div(
        DivisionMode::Exact,
        int(1),
        Box::new(ValueExpr::Sub(int(1), Box::new(ValueExpr::Var(variable())))),
    );
    cases.push((
        "forall_stops_at_false",
        RelExpr::ForAll {
            variable: variable(),
            start: 0,
            end: 2,
            body: Box::new(equal(first_one_then_undefined.clone(), 0)),
        },
        False,
    ));
    cases.push((
        "exists_stops_at_true",
        RelExpr::Exists {
            variable: variable(),
            start: 0,
            end: 2,
            body: Box::new(equal(first_one_then_undefined, 1)),
        },
        True,
    ));
    let nested_sum = sum(
        1,
        3,
        ValueExpr::Add(
            Box::new(ValueExpr::Var(variable())),
            Box::new(sum(4, 6, ValueExpr::Var(variable()))),
        ),
    );
    cases.push(("nested_shadowed_sum", equal(nested_sum, 21), True));
    if let TemporalFormula::Atom(RelExpr::Compare(_, projection, _)) = atom() {
        cases.push(("projection", equal(projection, 7), True));
    }
    cases
}

fn reference_step() -> TraceStep {
    let TemporalFormula::Atom(RelExpr::Compare(_, ValueExpr::Projection(path), _)) = atom() else {
        unreachable!()
    };
    TraceStep::try_new(vec![Observation::new(path, 7)]).unwrap_or_else(|| unreachable!())
}

#[test]
fn relational_operator_corpus_matches_independent_expected_outcomes() {
    let step = reference_step();
    for (label, expression, expected) in operator_cases() {
        assert_eq!(
            evaluate_relational(
                &expression,
                EvaluationContext::new(&step, &ReferencePredicates, EvalLimits::default())
            ),
            expected,
            "{label}"
        );
        let rendered = rendered(&TemporalFormula::Atom(expression));
        assert!(
            !rendered.term.is_empty() && !rendered.defined.is_empty(),
            "{label}"
        );
    }
}

// Uses the production prelude and translated formulas. `decide` checks each
// closed expected result, including undefined arithmetic and quantifier order.
// This corpus is translation evidence; it creates no production proof receipt.
fn lean_operator_corpus() -> String {
    let claim = ClaimDecl::new(
        zeno_fcis_spec::StableId::new(700).unwrap_or_else(|| unreachable!()),
        Identifier::try_new("translation_corpus").unwrap_or_else(|| unreachable!()),
        vec![BackendId::Lean],
        ClaimMode::UnboundedProof,
        ClaimFormula::Temporal(TemporalFormula::Atom(RelExpr::Bool(true))),
    );
    let exported = export_lean(&claim).unwrap_or_else(|error| panic!("export prelude: {error:?}"));
    let source = std::str::from_utf8(exported.source()).unwrap_or_else(|_| unreachable!());
    let (prelude, _) = source
        .split_once("variable (observe")
        .unwrap_or_else(|| unreachable!());
    let mut source = prelude.to_owned();
    source.push_str("instance (value : Int) : Decidable (inI128 value) := inferInstanceAs (Decidable (i128Min <= value ∧ value <= i128Max))\n");
    source.push_str("def observe (_ : String) (_ : Nat) : I128 := ⟨7, by decide⟩\n");
    source.push_str("def predicate (name : String) (arguments : List Int) : Prop := name = \"positive\" ∧ arguments = [1]\n");
    source.push_str("instance (name : String) (arguments : List Int) : Decidable (predicate name arguments) := inferInstanceAs (Decidable (name = \"positive\" ∧ arguments = [1]))\n");
    for (index, (label, expression, expected)) in operator_cases().into_iter().enumerate() {
        let result = rendered(&TemporalFormula::Atom(expression));
        let proposition = match expected {
            EvalOutcome::True => format!("({}) ∧ ({})", result.defined, result.term),
            EvalOutcome::False => format!("({}) ∧ ¬ ({})", result.defined, result.term),
            EvalOutcome::Indeterminate(_) => format!("¬ ({})", result.defined),
        };
        source.push_str(&format!(
            "-- {label}\ntheorem operator_{index} : {proposition} := by decide\n"
        ));
    }
    // Abstract observations make time binding part of the checked proposition.
    source.push_str("section Temporal\nvariable (obs : String → Nat → I128)\n");
    let p = "(obs \"pre_100\" t).val > (0 : Int)";
    let cases = [
        (atom(), "(obs \"pre_100\" 0).val > (0 : Int)".to_owned()),
        (TemporalFormula::Not(Box::new(atom())), "¬ ((obs \"pre_100\" 0).val > (0 : Int))".to_owned()),
        (TemporalFormula::And(Box::new(atom()), Box::new(atom())), "((obs \"pre_100\" 0).val > (0 : Int)) ∧ ((obs \"pre_100\" 0).val > (0 : Int))".to_owned()),
        (TemporalFormula::Or(Box::new(atom()), Box::new(atom())), "((obs \"pre_100\" 0).val > (0 : Int)) ∨ ((obs \"pre_100\" 0).val > (0 : Int))".to_owned()),
        (TemporalFormula::Next(Box::new(atom())), "(obs \"pre_100\" (0 + 1)).val > (0 : Int)".to_owned()),
        (TemporalFormula::Always(Box::new(atom())), format!("∀ t : Nat, t >= 0 → ({p})")),
        (TemporalFormula::Eventually(Box::new(atom())), format!("∃ t : Nat, t >= 0 ∧ ({p})")),
        (TemporalFormula::Always(Box::new(TemporalFormula::Eventually(Box::new(atom())))), format!("∀ s : Nat, s >= 0 → (∃ t : Nat, t >= s ∧ ({p}))")),
        (TemporalFormula::Eventually(Box::new(TemporalFormula::Always(Box::new(atom())))), format!("∃ s : Nat, s >= 0 ∧ (∀ t : Nat, t >= s → ({p}))")),
        (TemporalFormula::Until(Box::new(atom()), Box::new(atom())), "∃ s : Nat, s >= 0 ∧ ((obs \"pre_100\" s).val > (0 : Int)) ∧ (∀ t : Nat, 0 <= t → t < s → ((obs \"pre_100\" t).val > (0 : Int)))".to_owned()),
    ];
    for (index, (formula, reference)) in cases.into_iter().enumerate() {
        let result = rendered(&formula);
        source.push_str(&format!(
            "theorem temporal_{index} : ({}) ↔ ({reference}) := by rfl\n",
            result.term.replace("observe ", "obs ")
        ));
        source.push_str(&format!(
            "theorem temporal_defined_{index} : {} := by simp\n",
            result.defined.replace("observe ", "obs ")
        ));
    }
    source.push_str("end Temporal\nend ZenoFCIS\n");
    source
}

#[test]
#[ignore = "requires an explicitly supplied existing Lean executable; installs and copies no runtime"]
fn operator_corpus_checks_with_existing_lean() {
    let lean = std::env::var_os("ZENO_FCIS_TRANSLATION_LEAN")
        .unwrap_or_else(|| panic!("set ZENO_FCIS_TRANSLATION_LEAN to an existing executable"));
    let version = std::process::Command::new(&lean)
        .arg("--version")
        .output()
        .unwrap_or_else(|error| panic!("existing Lean: {error}"));
    assert!(version.status.success());
    let sequence = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| unreachable!())
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "zeno-fcis-translation-corpus-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap_or_else(|error| panic!("create corpus directory: {error}"));
    let path = directory.join("Translation.lean");
    fs::write(&path, lean_operator_corpus())
        .unwrap_or_else(|error| panic!("write corpus: {error}"));
    let output = std::process::Command::new("timeout")
        .arg("45s")
        .arg(lean)
        .arg(&path)
        .output()
        .unwrap_or_else(|error| panic!("run existing Lean: {error}"));
    println!(
        "{}translation corpus: {}",
        String::from_utf8_lossy(&version.stdout),
        path.display()
    );
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
