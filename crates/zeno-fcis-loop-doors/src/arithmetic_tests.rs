//! Main-route arithmetic regressions. These checks produce no acceptance receipts.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

use crate::expr::{Allow, Expr, Vocabulary, evaluate, parse, print_symbolic};
use crate::lower::{from_program, lower_outputs};
use crate::sketch::{self, Status};
use crate::subject::Subject;

fn checked<T, E: core::fmt::Debug>(result: core::result::Result<T, E>) -> T {
    result.unwrap_or_else(|e| panic!("fixture refused: {e:?}"))
}

fn expression(text: &str) -> Expr {
    checked(parse(text, 0, Allow::ALL))
}

fn seed(inputs: Vec<Domain>, outputs: Vec<Domain>, names: &[&str]) -> (Program, Vocabulary) {
    let roots = outputs
        .iter()
        .map(|d| u16::from(*d == Domain::Bool))
        .collect();
    let program = checked(Program::try_new(
        inputs.clone(),
        outputs.clone(),
        vec![Op::Int(0), Op::Bool(true)],
        roots,
    ));
    let vocab = checked(Vocabulary::new(
        names.iter().map(|n| n.to_string()).collect(),
        inputs,
        outputs,
        &BTreeMap::new(),
    ));
    (program, vocab)
}

fn subject(text: &str, boolean: bool) -> Subject {
    let integer = Domain::Int { min: 0, max: 40 };
    let output = if boolean {
        Domain::Bool
    } else {
        Domain::Int {
            min: 0,
            max: i64::MAX,
        }
    };
    let (original, vocab) = seed(vec![integer], vec![output], &["x"]);
    let program = checked(lower_outputs(&[expression(text)], &vocab, &original));
    checked(Subject::new(program, vocab, BTreeMap::new(), None, 41))
}

fn rejects_stub<T: Eq>(right: &T, wrong: &T) {
    let control =
        |compare: &dyn Fn(&T, &T) -> bool| compare(right, right) && !compare(right, wrong);
    assert!(control(&|a, b| a == b));
    assert!(
        !control(&|_, _| true),
        "a stubbed comparison must fail the control"
    );
}

#[test]
fn main_route_calls_round_trip_lower_lift_and_reject_stubs() {
    for (text, wrong, boolean) in [
        ("mul_div_floor(x, 3, 2)", "mul_div_ceil(x, 3, 2)", false),
        ("mul_div_ceil(x, 3, 2)", "mul_div_floor(x, 3, 2)", false),
        ("products_ge(x, 2, 3, 1)", "products_ge(x, 1, 3, 1)", true),
        (
            "products_ge(x, 2, 1, 3, 1, 1)",
            "products_ge(x, 1, 1, 3, 1, 1)",
            true,
        ),
    ] {
        let s = subject(text, boolean);
        let ast = expression(text);
        assert_eq!(expression(&print_symbolic(&ast)), ast);
        let lifted = checked(from_program(&s.original, &s.vocab, 512));
        let lowered = checked(lower_outputs(&lifted, &s.vocab, &s.original));
        let kept = checked(lower_outputs(&[Expr::Keep], &s.vocab, &s.original));
        let got = checked(evaluate(&ast, &s.env(None), &BTreeMap::new()));
        let mutant = checked(evaluate(&expression(wrong), &s.env(None), &BTreeMap::new()));
        assert_eq!(got, s.targets[0]);
        rejects_stub(&got, &mutant);
        for x in 0..=40 {
            assert_eq!(lowered.evaluate(&[x]), s.original.evaluate(&[x]));
            assert_eq!(kept.evaluate(&[x]), s.original.evaluate(&[x]));
        }
    }
}

#[test]
fn new_calls_keep_full_products_and_refuse_undefined_values() {
    let s = subject("x", false);
    let max = i64::MAX;
    for (text, expected) in [
        (format!("mul_div_floor({max}, {max}, {max})"), max),
        (format!("mul_div_ceil({max}, {max}, {max})"), max),
        (
            format!(
                "products_ge({max}, {max}, {max}, {max}, {max}, {})",
                max - 1
            ),
            1,
        ),
        (
            format!(
                "products_ge({max}, {max}, {}, {max}, {max}, {max})",
                max - 1
            ),
            0,
        ),
    ] {
        assert_eq!(
            checked(evaluate(&expression(&text), &s.env(None), &BTreeMap::new())),
            vec![expected; 41]
        );
    }
    for text in [
        "mul_div_floor(x, 2, 0)".to_string(),
        "mul_div_ceil(x, 2, 0)".to_string(),
        "mul_div_floor(-1, 1, 1)".to_string(),
        "mul_div_ceil(1, -1, 1)".to_string(),
        "products_ge(1, 1, -1, 1)".to_string(),
        "products_ge(1, 1, 1, 1, 1, -1)".to_string(),
        format!("mul_div_floor({max}, {max}, 1)"),
        format!("mul_div_ceil({max}, 2, 1)"),
        "true ? 1 : mul_div_floor(1, 1, 0)".to_string(),
    ] {
        assert!(
            evaluate(&expression(&text), &s.env(None), &BTreeMap::new()).is_err(),
            "{text}"
        );
    }
}

#[test]
fn arithmetic_sketch_candidates_are_checked_without_nonlinear_inverse_claims() {
    for (target, sketch, wrong, boolean) in [
        (
            "mul_div_floor(x, 3, 2)",
            "mul_div_floor(x, ?c[0..4], 2)",
            "mul_div_ceil(x, 3, 2)",
            false,
        ),
        (
            "mul_div_ceil(x, 3, 2)",
            "mul_div_ceil(x, ?c[0..4], 2)",
            "mul_div_floor(x, 3, 2)",
            false,
        ),
        (
            "products_ge(x, 2, 3, 1)",
            "products_ge(x, ?c[1..3], 3, 1)",
            "products_ge(x, 1, 3, 1)",
            true,
        ),
        (
            "products_ge(x, 2, 1, 3, 1, 1)",
            "products_ge(x, ?{1 | 2}, 1, 3, 1, 1)",
            "products_ge(x, 1, 1, 3, 1, 1)",
            true,
        ),
    ] {
        let s = subject(target, boolean);
        let result = sketch::solve(&s, &[expression(sketch)], &sketch::Settings::default());
        assert_eq!(
            result.status,
            Status::Completed,
            "{target}: {:?}",
            result.reason
        );
        let program = result
            .program
            .unwrap_or_else(|| panic!("completion missing"));
        for x in 0..=40 {
            assert_eq!(program.evaluate(&[x]), s.original.evaluate(&[x]));
        }
        // Reach solve's final lowered-program comparison, not a detached
        // equality helper. Forcing that production `agrees` to true must
        // turn this rejection into Completed and fail the control.
        let rejected = sketch::solve(&s, &[expression(wrong)], &sketch::Settings::default());
        assert_eq!(
            rejected.status,
            Status::NotFound,
            "{target} accepted {wrong}"
        );
        assert!(
            rejected.program.is_none(),
            "wrong completion escaped: {wrong}"
        );
    }
    let s = subject("mul_div_floor(x, 3, 2)", false);
    let result = sketch::solve(
        &s,
        &[expression("mul_div_floor(x, ?e, 2)")],
        &sketch::Settings::default(),
    );
    assert_eq!(result.status, Status::NotFound);
}

#[test]
fn review_wrong_arithmetic_completion_is_not_accepted() {
    let s = subject("mul_div_floor(x, 3, 2)", false);
    let result = sketch::solve(
        &s,
        &[expression("mul_div_floor(x, 1, 2)")],
        &sketch::Settings::default(),
    );
    assert_eq!(
        result.status,
        Status::NotFound,
        "wrong result at x=1: target=1, candidate=0"
    );
    assert!(result.program.is_none());
}

fn reject_wrong_sketch(target: &str, wrong: &str, boolean: bool) {
    let s = subject(target, boolean);
    let result = sketch::solve(&s, &[expression(wrong)], &sketch::Settings::default());
    assert_eq!(result.status, Status::NotFound, "{target} accepted {wrong}");
    assert!(result.program.is_none());
}

#[test]
fn arithmetic_sketch_rejects_wrong_floor() {
    reject_wrong_sketch("mul_div_floor(x, 3, 2)", "mul_div_ceil(x, 3, 2)", false);
}

#[test]
fn arithmetic_sketch_rejects_wrong_ceil() {
    reject_wrong_sketch("mul_div_ceil(x, 3, 2)", "mul_div_floor(x, 3, 2)", false);
}

#[test]
fn arithmetic_sketch_rejects_wrong_two_factor_product() {
    reject_wrong_sketch("products_ge(x, 2, 3, 1)", "products_ge(x, 1, 3, 1)", true);
}

#[test]
fn arithmetic_sketch_rejects_wrong_three_factor_product() {
    reject_wrong_sketch(
        "products_ge(x, 2, 1, 3, 1, 1)",
        "products_ge(x, 1, 1, 3, 1, 1)",
        true,
    );
}

#[test]
fn scaled_cpmm_matches_the_formula_for_each_declared_fee() {
    for bps in [0, 30, 100] {
        let fee = format!("mul_div_ceil(amount, {bps}, 10000)");
        let net = format!("(amount - {fee})");
        let denominator = format!("(x + {net} == 0 ? 1 : x + {net})");
        let output = format!("mul_div_floor({net}, y, {denominator})");
        let reserve = Domain::Int { min: 0, max: 40 };
        let wide = Domain::Int { min: 0, max: 80 };
        let (original, vocab) = seed(
            vec![reserve; 3],
            vec![reserve, reserve, wide, reserve, Domain::Bool],
            &["x", "y", "amount"],
        );
        let nodes: Vec<Expr> = [
            fee,
            output.clone(),
            "x + amount".to_string(),
            format!("y - {output}"),
            format!("products_ge(x + amount, y - {output}, x, y)"),
        ]
        .iter()
        .map(|t| expression(t))
        .collect();
        let program = checked(lower_outputs(&nodes, &vocab, &original));
        let mut count = 0;
        let mut rounding_witness = false;
        for x in 0..=40 {
            for y in 0..=40 {
                for amount in 0..=40 {
                    // Independent small-integer reference; not a production evaluator.
                    let fee = (amount * bps + 9999) / 10000;
                    let net = amount - fee;
                    let output = if x + net == 0 { 0 } else { net * y / (x + net) };
                    let expected = vec![fee, output, x + amount, y - output, 1];
                    let got = checked(program.evaluate(&[x, y, amount]));
                    assert_eq!(got, expected, "fee {bps}, input {x},{y},{amount}");
                    let mut mutant = got.clone();
                    mutant[4] = 0;
                    rejects_stub(&got, &mutant);
                    rounding_witness |= bps != 0 && fee != amount * bps / 10000;
                    count += 1;
                }
            }
        }
        assert_eq!(count, 68_921);
        assert!(bps == 0 || rounding_witness);
    }
}
