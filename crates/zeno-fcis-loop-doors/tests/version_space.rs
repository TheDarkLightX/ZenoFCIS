//! Semantic families and budget regressions for the proposal engine. The CLI
//! tests separately establish the existing checker path; these are not judges.
#![allow(clippy::unwrap_used)]
use zeno_fcis_loop_doors::expr::{self, Allow};
use zeno_fcis_loop_doors::version_space::{
    self as V, Budget, Limits, Proposal, Requirement, Specification, Stop,
};
use zeno_fcis_synthesis::finite::Domain;

fn spec(texts: &[&str], inputs: &[Domain], outputs: &[Domain]) -> Specification {
    Specification {
        names: (0..inputs.len())
            .map(|k| ["x", "y", "z"][k].to_owned())
            .collect(),
        inputs: inputs.to_vec(),
        outputs: outputs.to_vec(),
        sketch: expr::renumber(
            &texts
                .iter()
                .map(|s| {
                    expr::parse(
                        s,
                        0,
                        Allow {
                            holes: true,
                            params: false,
                            keep: false,
                        },
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
        requirements: vec![],
    }
}
fn analyze(s: &Specification) -> (Proposal, Budget) {
    let mut b = Budget::new(Limits::default()).unwrap();
    (V::analyze(s, &mut b).unwrap(), b)
}
fn count(p: &Proposal) -> usize {
    match p {
        Proposal::Question { count, .. } => *count,
        Proposal::Singleton(_) => 1,
        Proposal::Empty { .. } => 0,
    }
}
#[test]
fn duplicate_spellings_are_one_complete_meaning() {
    let s = spec(
        &["?{x | x && true | false | true}"],
        &[Domain::Bool],
        &[Domain::Bool],
    );
    assert_eq!(count(&analyze(&s).0), 3);
}
#[test]
fn multi_output_partitions_offer_complete_tuples() {
    let s = spec(
        &["?c", "?c"],
        &[Domain::Bool],
        &[Domain::Bool, Domain::Bool],
    );
    let Proposal::Question { count, question } = analyze(&s).0 else {
        panic!("question")
    };
    assert_eq!(count, 4);
    assert_eq!(question.ordinal, 0);
    assert_eq!(
        question
            .answers
            .iter()
            .map(|a| (&a.output, a.remaining))
            .collect::<Vec<_>>(),
        vec![
            (&vec![0, 0], 1),
            (&vec![0, 1], 1),
            (&vec![1, 0], 1),
            (&vec![1, 1], 1)
        ]
    );
}
#[test]
fn threshold_questions_minimize_the_largest_bucket() {
    for n in [4, 5, 7, 9] {
        let text = format!("x < ?c[0..{n}]");
        let s = spec(
            &[&text],
            &[Domain::Int { min: 0, max: n - 1 }],
            &[Domain::Bool],
        );
        let Proposal::Question { count, question } = analyze(&s).0 else {
            panic!("question")
        };
        assert_eq!(count, usize::try_from(n + 1).unwrap());
        assert_eq!(question.worst_remaining, count.div_ceil(2));
        assert_eq!(
            question.answers.iter().map(|a| a.remaining).sum::<usize>(),
            count
        );
    }
}
#[test]
fn confirmed_answers_recover_every_boolean_owner_without_a_core() {
    let original = spec(&["x ? ?c : ?c"], &[Domain::Bool], &[Domain::Bool]);
    for table in [[0, 0], [0, 1], [1, 0], [1, 1]] {
        let mut s = original.clone();
        for round in 0..=2 {
            match analyze(&s).0 {
                Proposal::Question { question, .. } => {
                    s.requirements.push(Requirement::Example {
                        id: format!("answer{round}"),
                        input: question.input,
                        output: vec![table[question.ordinal]],
                    });
                }
                Proposal::Singleton(m) => {
                    assert_eq!(m.table, vec![vec![table[0]], vec![table[1]]]);
                    break;
                }
                Proposal::Empty { .. } => panic!("in-space owner cannot empty the grammar"),
            }
            assert!(round < 2, "must recover within two Boolean inputs");
        }
    }
}
#[test]
fn minimum_conflicts_have_lexical_ties_and_are_relative_to_the_sketch() {
    let mut s = spec(&["?c"], &[Domain::Bool], &[Domain::Bool]);
    for (id, x, y) in [("d", 0, 1), ("c", 0, 0), ("b", 1, 1), ("a", 1, 0)] {
        s.requirements.push(Requirement::Example {
            id: id.into(),
            input: vec![x],
            output: vec![y],
        });
    }
    let Proposal::Empty { conflict } = analyze(&s).0 else {
        panic!("empty")
    };
    assert_eq!(conflict, vec!["a", "b"]);
    let empty = spec(
        &["?{2 | 3}"],
        &[Domain::Bool],
        &[Domain::Int { min: 0, max: 1 }],
    );
    let Proposal::Empty { conflict } = analyze(&empty).0 else {
        panic!("empty grammar")
    };
    assert!(conflict.is_empty());
    let mut outside = spec(
        &["?c[0..1]"],
        &[Domain::Bool],
        &[Domain::Int { min: 0, max: 2 }],
    );
    outside.requirements.push(Requirement::Example {
        id: "outside".into(),
        input: vec![0],
        output: vec![2],
    });
    assert_eq!(count(&analyze(&outside).0), 0);
}
#[test]
fn laws_and_examples_filter_all_inputs_and_overflow_is_eager() {
    let mut s = spec(&["?c"], &[Domain::Bool], &[Domain::Bool]);
    s.requirements.push(Requirement::Law {
        id: "zero".into(),
        formula: expr::parse(
            "out0 == false",
            0,
            Allow {
                holes: false,
                params: false,
                keep: false,
            },
        )
        .unwrap(),
    });
    assert_eq!(count(&analyze(&s).0), 1);
    s.requirements.push(Requirement::Example {
        id: "one".into(),
        input: vec![1],
        output: vec![1],
    });
    assert_eq!(count(&analyze(&s).0), 0);
    let s = spec(
        &["?{false | true ? false : 9223372036854775807 + 1}"],
        &[Domain::Bool],
        &[Domain::Int { min: 0, max: 1 }],
    );
    assert_eq!(count(&analyze(&s).0), 1);
}
#[test]
fn every_budget_phase_is_inconclusive_and_exact_work_boundaries_finish() {
    let s = spec(&["?c"], &[Domain::Bool], &[Domain::Bool]);
    for l in [
        Limits {
            work: 0,
            ..Limits::default()
        },
        Limits {
            domain: 1,
            ..Limits::default()
        },
        Limits {
            meanings: 0,
            ..Limits::default()
        },
        Limits {
            meanings: 1,
            ..Limits::default()
        },
        Limits {
            cells: 0,
            ..Limits::default()
        },
    ] {
        let mut b = Budget::new(l).unwrap();
        assert!(matches!(V::analyze(&s, &mut b), Err(Stop::Inconclusive(_))));
    }
    let (expected, b) = analyze(&s);
    let mut exact = Budget::new(Limits {
        work: b.work(),
        ..Limits::default()
    })
    .unwrap();
    assert_eq!(
        count(&V::analyze(&s, &mut exact).unwrap()),
        count(&expected)
    );
    let mut short = Budget::new(Limits {
        work: b.work() - 1,
        ..Limits::default()
    })
    .unwrap();
    assert!(matches!(
        V::analyze(&s, &mut short),
        Err(Stop::Inconclusive(_))
    ));
    let empty = spec(&["2"], &[Domain::Bool], &[Domain::Int { min: 0, max: 1 }]);
    let mut b = Budget::new(Limits {
        conflict_subsets: 0,
        ..Limits::default()
    })
    .unwrap();
    assert!(matches!(
        V::analyze(&empty, &mut b),
        Err(Stop::Inconclusive(_))
    ));
}
#[test]
fn infinite_holes_and_implicit_core_names_do_not_claim_emptiness() {
    for text in ["?e", "?c", "x == 0 ? ?e : ?c[0..1]"] {
        let s = spec(
            &[text],
            &[Domain::Int { min: 0, max: 1 }],
            &[Domain::Int { min: 0, max: 1 }],
        );
        let mut b = Budget::new(Limits::default()).unwrap();
        assert!(matches!(V::analyze(&s, &mut b), Err(Stop::Inconclusive(_))));
    }
    let s = spec(&["out0"], &[Domain::Bool], &[Domain::Bool]);
    assert!(s.validate().is_err());
}
