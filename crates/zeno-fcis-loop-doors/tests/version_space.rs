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

// Full scaled cube is covered by disjoint fixed-x declarations (1,681 tuples
// each), within the unchanged 4,096-tuple default. These are proposal tests;
// the CLI's separate law/check/replay controls below remain necessary.
fn owner_rounding(mut s: Specification, owner: impl Fn(&[i64]) -> Vec<i64>) {
    for round in 0..=2 {
        match analyze(&s).0 {
            Proposal::Question { count, question } => {
                assert_eq!(count, 2);
                let output = owner(&question.input);
                assert!(question.answers.iter().any(|a| a.output == output));
                s.requirements.push(Requirement::Example {
                    id: format!("owner_{round}"),
                    input: question.input,
                    output,
                });
            }
            Proposal::Singleton(m) => {
                let cols = zeno_fcis_loop_doors::subject::columns(&s.inputs, 4096).unwrap();
                for (k, row) in m.table.iter().enumerate() {
                    assert_eq!(*row, owner(&cols.iter().map(|c| c[k]).collect::<Vec<_>>()));
                }
                return;
            }
            Proposal::Empty { .. } => panic!("owner formula is in the declared grammar"),
        }
    }
    panic!("two meanings must be recovered by a distinguishing answer");
}

#[test]
fn scaled_cpmm_questions_recover_fee_and_output_rounding_on_every_reserve_slice() {
    let amount = Domain::Int { min: 0, max: 40 };
    let output = Domain::Int { min: 0, max: 40 };
    for bps in [0, 30, 100] {
        let fee = format!("?{{mul_div_floor(x, {bps}, 10000) | mul_div_ceil(x, {bps}, 10000)}}");
        owner_rounding(spec(&[&fee], &[amount], &[output]), |p| {
            vec![(p[0] * bps + 9999) / 10000]
        });
        for reserve in 0..=40 {
            // Here x is y reserve, y is amount. Fee rounding was recovered in
            // the first question. The zero-denominator convention is explicit.
            let net = format!("(y - mul_div_ceil(y, {bps}, 10000))");
            let d = format!("({reserve} + {net})");
            let d = if reserve == 0 {
                format!("({d} == 0 ? 1 : {d})")
            } else {
                d
            };
            let q = format!("?{{mul_div_floor({net}, x, {d}) | mul_div_ceil({net}, x, {d})}}");
            owner_rounding(spec(&[&q], &[amount, amount], &[output]), |p| {
                let fee = (p[1] * bps + 9999) / 10000;
                let net = p[1] - fee;
                let d = reserve + net;
                vec![if d == 0 { 0 } else { net * p[0] / d }]
            });
        }
    }
}

#[test]
fn version_space_product_comparison_control_fails_when_stubbed() {
    for text in [
        "?{products_ge(mul_div_floor(x, 3, 2), 1, mul_div_ceil(x, 3, 2), 1) | true}",
        "?{products_ge(mul_div_floor(x, 3, 2), 1, 3, mul_div_ceil(x, 3, 2), 1, 3) | true}",
    ] {
        let original = spec(&[text], &[Domain::Int { min: 0, max: 4 }], &[Domain::Bool]);
        let control = |stub: bool| {
            let mut s = original.clone();
            if stub {
                s.sketch = s
                    .sketch
                    .iter()
                    .map(|n| {
                        expr::substitute(n, &mut |e| {
                            Ok(matches!(e, expr::Expr::Call(expr::Function::ProductsGe, _))
                                .then_some(expr::Expr::Int(1)))
                        })
                        .unwrap()
                    })
                    .collect();
            }
            let Proposal::Question { count: 2, question } = analyze(&s).0 else {
                return false;
            };
            assert_eq!(
                question.answers.iter().map(|a| a.remaining).sum::<usize>(),
                2
            );
            // The exact owner product comparison is false precisely on odd x.
            let output = vec![i64::from(question.input[0] % 2 == 0)];
            s.requirements.push(Requirement::Example {
                id: "owner".into(),
                input: question.input,
                output,
            });
            matches!(analyze(&s).0, Proposal::Singleton(m) if m.table == vec![vec![1], vec![0], vec![1], vec![0], vec![1]])
        };
        assert!(control(false));
        assert!(
            !control(true),
            "stubbed product comparison must fail the question control"
        );
    }
}

#[test]
fn version_space_arithmetic_refusals_exclude_the_whole_meaning() {
    for bad in [
        "mul_div_floor(x, 1, 0)",
        "mul_div_ceil(-1, x, 1)",
        "mul_div_ceil(9223372036854775807, 2, 1)",
        "true ? x : mul_div_floor(1, 1, 0)",
    ] {
        let text = format!("?{{{bad} | x}}");
        let s = spec(
            &[&text],
            &[Domain::Int { min: 0, max: 2 }],
            &[Domain::Int { min: 0, max: 2 }],
        );
        let Proposal::Singleton(m) = analyze(&s).0 else {
            panic!("undefined candidate survived: {bad}");
        };
        assert_eq!(m.table, vec![vec![0], vec![1], vec![2]]);
    }
    let s = spec(
        &["?{products_ge(x, 1, -1, 1) | products_ge(x, 1, 1, x, 1, 1)}"],
        &[Domain::Int { min: 0, max: 2 }],
        &[Domain::Bool],
    );
    let Proposal::Singleton(m) = analyze(&s).0 else {
        panic!("negative product survived");
    };
    assert_eq!(m.table, vec![vec![1]; 3]);
}
