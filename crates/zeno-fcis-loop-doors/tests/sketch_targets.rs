//! Required explicit-target seam scenarios; none may be ignored.

use std::collections::BTreeMap;
use zeno_fcis_loop_doors::expr::{self, Allow, Expr, Fill, Vocabulary};
use zeno_fcis_loop_doors::sketch::{self, Search, Settings, Status, TargetProblem};
use zeno_fcis_loop_doors::subject::Subject;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

fn integer(min: i128, max: i128) -> Domain {
    Domain::Int { min, max }
}
fn vocab() -> Vocabulary {
    Vocabulary::new(
        vec!["x".into()],
        vec![integer(0, 2)],
        vec![integer(0, 3)],
        &BTreeMap::new(),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"))
}
fn expression(text: &str) -> Expr {
    expr::parse(
        text,
        0,
        Allow {
            holes: true,
            params: true,
            keep: true,
        },
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"))
}
fn problem(text: &str, rows: &[i128], targets: &[i128], settings: Settings) -> TargetProblem {
    TargetProblem::new(
        vocab(),
        rows.iter().map(|x| vec![*x]).collect(),
        targets.to_vec(),
        expression(text),
        settings,
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"))
}

#[test]
fn explicit_targets_need_no_original_and_results_are_owned() {
    let input = problem("?{0 | x}", &[0, 1], &[0, 1], Settings::default());
    let before = format!("{input:?}");
    let mut first = sketch::propose_targets(&input);
    assert_eq!(first.status, Status::Completed);
    assert_eq!(first.expression, Some(expression("x")));
    assert_eq!(first.holes, BTreeMap::from([(0, Fill::Choice(1))]));
    first.holes.clear();
    first.expression = Some(Expr::Int(99));
    let second = sketch::propose_targets(&input);
    assert_eq!(second.holes.len(), 1);
    assert_eq!(second.expression, Some(expression("x")));
    assert_eq!(before, format!("{input:?}"));
}

#[test]
fn original_keep_parameters_and_nonfinite_holes_are_refused() {
    for text in ["keep", "$p", "?e", "?c", "x + ?c"] {
        assert!(
            TargetProblem::new(
                vocab(),
                vec![vec![0]],
                vec![0],
                expression(text),
                Settings::default()
            )
            .is_err(),
            "{text}"
        );
    }
    for sketch in [
        Expr::Hole(0, expr::Hole::Choice(vec![Expr::Int(0), Expr::Keep])),
        Expr::Hole(0, expr::Hole::Choice(vec![])),
        Expr::Hole(0, expr::Hole::Choice(vec![Expr::Int(0)])),
        Expr::Hole(0, expr::Hole::Choice(vec![Expr::Int(0); 17])),
        Expr::Hole(
            0,
            expr::Hole::Choice(vec![
                Expr::Hole(1, expr::Hole::Const(Some((0, 1)))),
                Expr::Int(0),
            ]),
        ),
    ] {
        assert!(
            TargetProblem::new(vocab(), vec![vec![0]], vec![0], sketch, Settings::default())
                .is_err()
        );
    }
}

#[test]
fn finite_choice_and_bounded_constant_agree_with_legacy_solve() {
    for search in [Search::Joint, Search::Meanings] {
        let original = Program::try_new(
            vec![integer(0, 2)],
            vec![integer(0, 3)],
            vec![Op::Input(0)],
            vec![0],
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        let subject = Subject::new(original, vocab(), BTreeMap::new(), None, 3)
            .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        for text in ["?{0 | x}", "?{0 | x} + ?{0 | 1}", "x + ?c[0..2]"] {
            let settings = Settings {
                search,
                ..Settings::default()
            };
            let legacy = sketch::solve(&subject, &[expression(text)], &settings);
            let target = sketch::propose_targets(&problem(text, &[0, 1, 2], &[0, 1, 2], settings));
            assert_eq!(legacy.status, Status::Completed, "{search:?}: {text}");
            assert_eq!(target.status, legacy.status, "{text}");
            assert_eq!(
                target.expression.as_ref(),
                legacy.expressions.first(),
                "{text}"
            );
        }
    }
}

#[test]
fn overflow_and_assignment_budget_outcomes_match_legacy_search() {
    let original = Program::try_new(
        vec![integer(0, 2)],
        vec![integer(0, 3)],
        vec![Op::Input(0)],
        vec![0],
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let subject = Subject::new(original, vocab(), BTreeMap::new(), None, 3)
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    for search in [Search::Joint, Search::Meanings] {
        for max_assignments in [0, 1, 2, 20_000] {
            let settings = Settings {
                search,
                max_assignments,
                ..Settings::default()
            };
            for text in [
                "?{0 | x}",
                "?{x + 170141183460469231731687303715884105727 | x}",
            ] {
                let legacy = sketch::solve(&subject, &[expression(text)], &settings);
                let target =
                    sketch::propose_targets(&problem(text, &[0, 1, 2], &[0, 1, 2], settings));
                assert_eq!(
                    target.status, legacy.status,
                    "{search:?}/{max_assignments}: {text}"
                );
                if target.status == Status::Completed {
                    assert_eq!(target.expression.as_ref(), legacy.expressions.first());
                }
            }
        }
    }
}

#[test]
fn first_completion_is_retained_without_cost_ranking() {
    for search in [Search::Joint, Search::Meanings] {
        let result = sketch::propose_targets(&problem(
            "?{x + 0 | x}",
            &[0, 1],
            &[0, 1],
            Settings {
                search,
                ..Settings::default()
            },
        ));
        assert_eq!(result.status, Status::Completed);
        assert_eq!(result.expression, Some(expression("x + 0")));
        assert_eq!(result.holes[&0], Fill::Choice(0));
    }
}

#[test]
fn rows_targets_domains_and_every_alternative_are_strict() {
    for (rows, targets) in [
        (vec![], vec![]),
        (vec![vec![0]], vec![]),
        (vec![vec![]], vec![0]),
        (vec![vec![0, 0]], vec![0]),
        (vec![vec![0], vec![0]], vec![0, 1]),
        (vec![vec![3]], vec![0]),
        (vec![vec![-1]], vec![0]),
        (vec![vec![0]], vec![4]),
    ] {
        assert!(
            TargetProblem::new(
                vocab(),
                rows,
                targets,
                expression("?{0 | x}"),
                Settings::default()
            )
            .is_err()
        );
    }
    for outputs in [
        vec![],
        vec![integer(0, 1), integer(0, 1)],
        vec![integer(2, 1)],
    ] {
        let v = Vocabulary::new(
            vec!["x".into()],
            vec![integer(0, 2)],
            outputs,
            &BTreeMap::new(),
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}"));
        assert!(
            TargetProblem::new(v, vec![vec![0]], vec![0], Expr::Int(0), Settings::default())
                .is_err()
        );
    }
    for text in ["?{0 | missing}", "!x", "x ? 0 : 1", "?{0 | !x}", "x * 1"] {
        assert!(
            TargetProblem::new(
                vocab(),
                vec![vec![0]],
                vec![0],
                expression(text),
                Settings::default()
            )
            .is_err(),
            "{text}"
        );
    }
}

#[test]
fn boolean_columns_and_output_kinds_are_distinct() {
    let bool_vocab = || {
        Vocabulary::new(
            vec!["b".into()],
            vec![Domain::Bool],
            vec![Domain::Bool],
            &BTreeMap::new(),
        )
        .unwrap_or_else(|error| panic!("required test result: {error:?}"))
    };
    for (row, target, text) in [
        (2, 0, "b"),
        (0, 2, "b"),
        (0, 0, "b + 1"),
        (0, 0, "?{b | 2}"),
    ] {
        assert!(
            TargetProblem::new(
                bool_vocab(),
                vec![vec![row]],
                vec![target],
                expression(text),
                Settings::default()
            )
            .is_err()
        );
    }
    let p = TargetProblem::new(
        bool_vocab(),
        vec![vec![0], vec![1]],
        vec![1, 0],
        expression("?{b | !b}"),
        Settings::default(),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(
        sketch::propose_targets(&p).expression,
        Some(expression("!b"))
    );
}

#[test]
fn correlated_features_are_not_cartesian_enumerated() {
    let names = (0..26).map(|j| format!("f{j}")).collect();
    let v = Vocabulary::new(
        names,
        vec![Domain::Bool; 26],
        vec![Domain::Bool],
        &BTreeMap::new(),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    let p = TargetProblem::new(
        v,
        vec![vec![0; 26], vec![1; 26]],
        vec![0, 1],
        expression("?{0 | f25}"),
        Settings::default(),
    )
    .unwrap_or_else(|error| panic!("required test result: {error:?}"));
    assert_eq!(
        sketch::propose_targets(&p).expression,
        Some(expression("f25"))
    );
}

#[test]
fn zero_assignment_and_work_budgets_precede_any_search() {
    for settings in [
        Settings {
            max_assignments: 0,
            ..Settings::default()
        },
        Settings {
            max_work: 0,
            ..Settings::default()
        },
    ] {
        let r = sketch::propose_targets(&problem("?{0 | x}", &[0, 1], &[0, 1], settings));
        assert_eq!(r.status, Status::Inconclusive);
        assert_eq!((r.evaluated, r.work), (0, 0));
        assert!(r.expression.is_none());
    }
}

#[test]
fn partial_budgets_preserve_measured_work_without_an_empty_space_claim() {
    for search in [Search::Joint, Search::Meanings] {
        let settings = Settings {
            search,
            max_assignments: 1,
            ..Settings::default()
        };
        let r = sketch::propose_targets(&problem("?{0 | 1} + ?{0 | 1}", &[0], &[2], settings));
        assert_eq!(r.status, Status::Inconclusive, "{search:?}: {r:?}");
        assert_eq!(r.evaluated, 1);
        assert!(r.work > 0);
        let cap = 5;
        let r = sketch::propose_targets(&problem(
            "?{0 | 1} + ?{0 | 1}",
            &[0],
            &[2],
            Settings {
                search,
                max_work: cap,
                ..Settings::default()
            },
        ));
        assert_eq!(r.status, Status::Inconclusive, "{search:?}: {r:?}");
        assert!(r.work > 0 && r.work <= cap, "{r:?}");
        assert!(r.expression.is_none());
    }
}

#[test]
fn overflow_refuses_fixed_expressions_and_skips_only_bad_choices() {
    for text in [
        "x + 170141183460469231731687303715884105727",
        "false ? x + 170141183460469231731687303715884105727 : x",
    ] {
        let r = sketch::propose_targets(&problem(text, &[1], &[1], Settings::default()));
        assert_eq!(r.status, Status::Refused);
        assert_eq!(r.reason.as_deref(), Some(expr::OVERFLOW));
        assert!(r.work > 0);
    }
    let r = sketch::propose_targets(&problem(
        "?{x + 170141183460469231731687303715884105727 | x}",
        &[1],
        &[1],
        Settings::default(),
    ));
    assert_eq!(r.status, Status::Completed);
    assert_eq!(r.holes[&0], Fill::Choice(1));
}

#[test]
fn completed_syntax_is_still_compared_with_the_explicit_targets() {
    let r = sketch::propose_targets(&problem("0", &[0, 1], &[0, 1], Settings::default()));
    assert_eq!(r.status, Status::Unrealizable);
    assert!(r.expression.is_none());
    assert!(r.work > 0);
}

#[test]
fn a_new_counterexample_rebuilds_effect_classes_cold() {
    let text = "(?{0 | x} + 0) + ?{0 | 1}";
    let initial = problem(text, &[0], &[0], Settings::default());
    let first = sketch::propose_targets(&initial);
    assert_eq!(first.status, Status::Completed);
    assert_eq!(first.holes[&0], Fill::Choice(0));
    assert_eq!(
        first
            .search
            .as_ref()
            .unwrap_or_else(|| panic!("required test value missing"))
            .searched[0]
            .values,
        vec![0]
    );
    let expanded = problem(text, &[0, 1], &[0, 1], Settings::default());
    let next = sketch::propose_targets(&expanded);
    assert_eq!(next.status, Status::Completed);
    assert_eq!(next.holes[&0], Fill::Choice(1));
    assert_eq!(
        next.search
            .as_ref()
            .unwrap_or_else(|| panic!("required test value missing"))
            .searched[0]
            .values,
        vec![0, 1]
    );
    let again = sketch::propose_targets(&initial);
    assert_eq!(again.holes, first.holes);
    assert_eq!(again.work, first.work);
}
