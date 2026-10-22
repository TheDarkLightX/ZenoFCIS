//! Frozen-order completion regressions.
#![allow(clippy::unwrap_used)]
use std::collections::BTreeMap;
use zeno_fcis_loop_doors::expr::{self, Allow, Expr, Vocabulary};
use zeno_fcis_loop_doors::repair::first_difference;
use zeno_fcis_loop_doors::sketch::{self, Search, Settings, Status};
use zeno_fcis_loop_doors::subject::{DEFAULT_MAX_TUPLES, Subject};
use zeno_fcis_synthesis::finite_runtime::import_program;

fn subject() -> Subject {
    let p = import_program(include_bytes!("data/i128/bounded-counter-c5.original.zcve")).unwrap();
    let variants = BTreeMap::from([(
        "action".to_owned(),
        BTreeMap::from([("Inc".to_owned(), 150), ("Dec".to_owned(), 151)]),
    )]);
    let vocab = Vocabulary::new(
        vec!["value".into(), "action".into(), "authorized".into()],
        p.inputs().to_vec(),
        p.outputs().to_vec(),
        &variants,
    )
    .unwrap();
    Subject::new(p, vocab, BTreeMap::new(), Some(5), DEFAULT_MAX_TUPLES).unwrap()
}
fn outputs(text: &str) -> Vec<Expr> {
    vec![
        expr::parse(
            text,
            0,
            Allow {
                holes: true,
                params: false,
                keep: false,
            },
        )
        .unwrap(),
        Expr::Keep,
        Expr::Keep,
    ]
}
const C5: &str = "authorized ? ((action == 151 ? ?c : ?c) < value - ?c ? 1 : action == 151 && value == 0 ? ?c : action == ?c ? ?c : 3) : 0";
#[test]
fn c5_completes_at_twenty_thousand_terminal_assignments() {
    let s = subject();
    for search in [Search::Joint, Search::Meanings] {
        let r = sketch::solve(
            &s,
            &outputs(C5),
            &Settings {
                search,
                max_assignments: 20_000,
                ..Settings::default()
            },
        );
        assert_eq!(r.status, Status::Completed, "{search:?}: {:?}", r.outputs);
        assert!(r.outputs[0].tried <= 20_000);
        assert!(first_difference(&s, r.program.as_ref().unwrap()).is_none());
    }
}
#[test]
fn zero_budget_precedes_any_empty_space_proof() {
    let s = subject();
    for search in [Search::Joint, Search::Meanings] {
        let r = sketch::solve(
            &s,
            &outputs(C5),
            &Settings {
                search,
                max_assignments: 0,
                ..Settings::default()
            },
        );
        assert_eq!(r.status, Status::Inconclusive);
        assert_eq!(r.evaluated, 0);
    }
}
#[test]
fn a_joint_assignment_prefix_is_not_displaced_by_internal_visits() {
    let s = subject();
    for n in [1, 2, 8, 32, 128, 20_000] {
        let joint = sketch::solve(
            &s,
            &outputs(C5),
            &Settings {
                search: Search::Joint,
                max_assignments: n,
                ..Settings::default()
            },
        );
        let meaning = sketch::solve(
            &s,
            &outputs(C5),
            &Settings {
                search: Search::Meanings,
                max_assignments: n,
                ..Settings::default()
            },
        );
        if joint.status == Status::Completed {
            assert_eq!(meaning.status, Status::Completed);
        }
        assert!(meaning.outputs[0].tried <= n);
    }
}
