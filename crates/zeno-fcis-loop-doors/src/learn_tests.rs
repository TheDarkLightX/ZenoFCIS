//! LLM-generated finite-language semantics and checker-bypass regressions.
use super::*;

fn base() -> Specification {
    Specification {
        names: vec!["x".into(), "y".into()],
        inputs: vec![Domain::Int { min: 0, max: 2 }; 2],
        outputs: vec![Domain::Int {
            min: -100,
            max: 100,
        }],
        constants: vec![1],
        operations: vec![Operation::Add, Operation::Sub],
        max_size: 3,
        examples: Vec::new(),
    }
}
fn analyzed(spec: &Specification) -> Analysis {
    analyze(spec, &mut Budget::new(Limits::default())).unwrap_or_else(|e| panic!("analyze: {e:?}"))
}
fn all_examples(spec: &mut Specification, f: impl Fn(i128, i128) -> Vec<i128>) {
    spec.examples = (0..=2)
        .flat_map(|x| (0..=2).map(move |y| (x, y)))
        .map(|(x, y)| Example {
            input: vec![x, y],
            output: f(x, y),
        })
        .collect();
}

#[test]
fn learn_smallest_sizes_counts_and_question_match_independent_golden() {
    let spec = base();
    let a = analyzed(&spec);
    // Syntax reference: x,y,1 at size 1; all typed sums/differences at size 3.
    assert_eq!(a.by_size, vec![[3, 0], [0, 0], [13, 0]]);
    assert_eq!(a.counts, vec![16]);
    assert_eq!(a.status, Status::Question);
    let q = a.question.unwrap_or_else(|| panic!("question"));
    assert_eq!(q.ordinal, 8);
    assert_eq!((q.worst, q.squares), (3, 44));
    assert_eq!(a.representatives[0].size, 1);
}

#[test]
fn learn_standard_macros_cost_one_and_lower_to_existing_ir() {
    let mut spec = base();
    spec.constants.clear();
    spec.operations = vec![Operation::Min, Operation::Max];
    spec.examples = vec![
        Example {
            input: vec![1, 2],
            output: vec![1],
        },
        Example {
            input: vec![2, 0],
            output: vec![0],
        },
        Example {
            input: vec![0, 2],
            output: vec![0],
        },
    ];
    let a = analyzed(&spec);
    assert_eq!(a.status, Status::Determined);
    assert_eq!(
        (
            a.representatives[0].text.as_str(),
            a.representatives[0].size
        ),
        ("min(x, y)", 3)
    );
    let candidate = a.candidate.unwrap_or_else(|| panic!("candidate"));
    assert!(
        candidate
            .nodes()
            .iter()
            .any(|n| matches!(n, Instruction::Select(..)))
    );
    for x in 0..=2 {
        for y in 0..=2 {
            assert_eq!(candidate.evaluate(&[x, y]), Ok(vec![x.min(y)]));
        }
    }
    assert_eq!(
        check_examples(&spec, &candidate, &mut Budget::new(Limits::default())),
        Ok(ExampleCheck::Reproduced { checked: 3 })
    );
}

#[test]
fn learn_two_outputs_share_one_minimax_question_and_confirm_every_example() {
    let mut spec = base();
    spec.outputs.push(spec.outputs[0]);
    all_examples(&mut spec, |x, y| vec![x + y, x - y]);
    let a = analyzed(&spec);
    assert_eq!(a.counts, vec![1, 1]);
    assert_eq!(a.status, Status::Determined);
    let p = a.candidate.unwrap_or_else(|| panic!("candidate"));
    assert_eq!(
        check_examples(&spec, &p, &mut Budget::new(Limits::default())),
        Ok(ExampleCheck::Reproduced { checked: 9 })
    );
    spec.examples.clear();
    let q = analyzed(&spec)
        .question
        .unwrap_or_else(|| panic!("question"));
    assert_eq!(q.buckets.len(), 2);
    assert_eq!(q.worst, 9);
    assert_eq!(q.squares, 1936);
}

#[test]
fn learn_boolean_and_conditional_vocabulary_is_typed() {
    let mut spec = base();
    spec.operations = vec![Operation::Lt, Operation::Le, Operation::Not];
    spec.outputs = vec![Domain::Bool];
    all_examples(&mut spec, |x, y| vec![i128::from(x < y)]);
    let a = analyzed(&spec);
    assert_eq!(a.counts, vec![1]);
    let p = a.candidate.unwrap_or_else(|| panic!("candidate"));
    assert_eq!(p.evaluate(&[2, 0]), Ok(vec![0]));
    assert_eq!(p.evaluate(&[0, 2]), Ok(vec![1]));
    let p = Operation::Select
        .probe()
        .unwrap_or_else(|e| panic!("probe: {e:?}"));
    assert_eq!(p.evaluate(&[0, 4, 7]), Ok(vec![7]));
    assert_eq!(p.evaluate(&[1, 4, 7]), Ok(vec![4]));
}

#[test]
fn learn_outside_vocabulary_core_is_minimum_cardinality_not_just_irreducible() {
    let mut spec = base();
    spec.max_size = 1;
    spec.examples = vec![
        Example {
            input: vec![0, 0],
            output: vec![0],
        },
        Example {
            input: vec![1, 2],
            output: vec![2],
        },
        Example {
            input: vec![2, 1],
            output: vec![2],
        },
    ];
    let a = analyzed(&spec);
    assert_eq!(a.status, Status::OutsideVocabulary);
    assert_eq!(a.core, Some(vec![1, 2]));
    assert!(a.candidate.is_none());
    for one in &spec.examples {
        let mut subset = spec.clone();
        subset.examples = vec![one.clone()];
        assert_ne!(analyzed(&subset).status, Status::OutsideVocabulary);
    }
}

#[test]
fn learn_minimum_conflict_is_not_capped_at_four_examples() {
    let spec = Specification {
        names: (0..6).map(|k| format!("b{k}")).collect(),
        inputs: vec![Domain::Bool; 6],
        outputs: vec![Domain::Bool],
        constants: Vec::new(),
        operations: Vec::new(),
        max_size: 1,
        examples: (0..6)
            .map(|k| Example {
                input: (0..6).map(|j| i128::from(j == k)).collect(),
                output: vec![0],
            })
            .collect(),
    };
    let a = analyzed(&spec);
    assert_eq!(a.status, Status::OutsideVocabulary);
    assert_eq!(a.core, Some(vec![5, 4, 3, 2, 1, 0]));
}

#[test]
fn learn_a_later_confirmed_spot_mismatch_eliminates_a_wrong_survivor() {
    let mut spec = base();
    spec.max_size = 1;
    spec.constants.clear();
    spec.examples = vec![Example {
        input: vec![1, 2],
        output: vec![1],
    }];
    assert_eq!(analyzed(&spec).status, Status::Determined);
    spec.examples.push(Example {
        input: vec![0, 0],
        output: vec![2],
    });
    assert_eq!(analyzed(&spec).status, Status::OutsideVocabulary);
}

#[test]
fn learn_domain_projection_keeps_eager_failure_separate_without_sentinels() {
    let mut spec = base();
    spec.names = vec!["x".into()];
    spec.inputs = vec![Domain::Int {
        min: i128::MIN,
        max: i128::MIN + 1,
    }];
    spec.outputs = spec.inputs.clone();
    spec.constants = vec![i128::MIN];
    spec.operations = vec![Operation::Sub, Operation::Abs];
    spec.max_size = 3;
    let a = analyzed(&spec);
    assert!(a.before[0] < a.by_size.iter().map(|row| row[0]).sum());
    let abs = Operation::Abs
        .probe()
        .unwrap_or_else(|e| panic!("abs: {e:?}"));
    assert_eq!(abs.evaluate(&[i128::MIN]), Err(Error::Arithmetic));
    let cols = columns(&spec.inputs, 2).unwrap_or_else(|e| panic!("columns: {e}"));
    assert_eq!(cols[0], vec![i128::MIN, i128::MIN + 1]);
}

#[test]
fn learn_overflow_in_an_unselected_branch_is_eager() {
    let mut spec = base();
    spec.names = vec!["x".into(), "c".into()];
    spec.inputs = vec![
        Domain::Int {
            min: i128::MAX,
            max: i128::MAX,
        },
        Domain::Bool,
    ];
    spec.operations = vec![Operation::Add, Operation::Select];
    spec.max_size = 6;
    let mut budget = Budget::new(Limits::default());
    let a = analyze(&spec, &mut budget).unwrap_or_else(|e| panic!("analyze: {e:?}"));
    assert!(a.by_size.iter().any(|counts| counts[0] > 0));
    // The enumerator's operation probes and final program use the same existing evaluator.
    let vocab = spec
        .vocabulary()
        .unwrap_or_else(|e| panic!("vocabulary: {e:?}"));
    let seed = Program::try_new(
        spec.inputs.clone(),
        spec.outputs.clone(),
        vec![Instruction::Int(0)],
        vec![0],
    )
    .unwrap_or_else(|e| panic!("seed: {e}"));
    let node = Expr::select(
        Expr::Var("c".into()),
        Expr::Var("x".into()),
        Expr::bin(Op::Add, Expr::Var("x".into()), Expr::Int(1)),
    );
    let p = lower_outputs(&[node], &vocab, &seed).unwrap_or_else(|e| panic!("lower: {e}"));
    assert_eq!(p.evaluate(&[i128::MAX, 1]), Err(Error::Arithmetic));
    assert_eq!(p.evaluate(&[i128::MAX, 0]), Err(Error::Arithmetic));
}

#[test]
fn learn_budget_stops_have_no_partial_result_and_retain_work() {
    let spec = base();
    let caps = [
        Limits {
            work: 0,
            ..Limits::default()
        },
        Limits {
            meanings: 1,
            ..Limits::default()
        },
        Limits {
            cells: 1,
            ..Limits::default()
        },
        Limits {
            tuples: 1,
            ..Limits::default()
        },
    ];
    for limits in caps {
        let mut b = Budget::new(limits);
        assert!(matches!(analyze(&spec, &mut b), Err(Stop::Inconclusive(_))));
        assert!(b.usage().work <= limits.work);
    }
    let mut b = Budget::new(Limits {
        meanings: 1,
        ..Limits::default()
    });
    assert!(analyze(&spec, &mut b).is_err());
    assert!(b.usage().work > 0);
    let mut impossible = spec;
    impossible.max_size = 1;
    impossible.examples = vec![Example {
        input: vec![0, 0],
        output: vec![3],
    }];
    let mut b = Budget::new(Limits {
        conflict_subsets: 0,
        ..Limits::default()
    });
    assert!(matches!(
        analyze(&impossible, &mut b),
        Err(Stop::Inconclusive(_))
    ));
}

#[test]
fn learn_exact_lowered_check_is_load_bearing_and_retains_refusal_usage() {
    let mut spec = base();
    spec.max_size = 1;
    spec.examples = vec![Example {
        input: vec![1, 2],
        output: vec![1],
    }];
    let wrong = Program::try_new(
        spec.inputs.clone(),
        spec.outputs.clone(),
        vec![Instruction::Int(0)],
        vec![0],
    )
    .unwrap_or_else(|e| panic!("wrong: {e}"));
    let mut b = Budget::new(Limits::default());
    // Stubbing the evaluator comparison to reproduce every example MUST fail this assertion.
    assert_eq!(
        check_examples(&spec, &wrong, &mut b),
        Ok(ExampleCheck::Mismatch {
            example: 0,
            checked: 1,
            actual: Some(vec![0])
        })
    );
    assert!(b.usage().work > 0);
    let mut b = Budget::new(Limits {
        work: 0,
        ..Limits::default()
    });
    assert!(matches!(
        check_examples(&spec, &wrong, &mut b),
        Err(Stop::Inconclusive(_))
    ));
}

#[test]
fn learn_repeated_runs_are_identical_and_results_own_their_data() {
    let mut spec = base();
    let a = analyzed(&spec);
    let b = analyzed(&spec);
    assert_eq!(
        (a.counts.clone(), a.question.clone(), a.by_size.clone()),
        (b.counts, b.question, b.by_size)
    );
    let candidate = a.candidate.unwrap_or_else(|| panic!("candidate"));
    let original_bytes = format!("{candidate:?}");
    spec.constants.clear();
    spec.inputs[0] = Domain::Bool;
    spec.names[0] = "changed".into();
    assert_eq!(format!("{candidate:?}"), original_bytes);
    let mut getter = candidate.nodes().to_vec();
    getter.clear();
    assert!(!candidate.nodes().is_empty());
}

#[test]
fn learn_refuses_duplicate_examples_and_out_of_domain_answers() {
    let mut spec = base();
    let e = Example {
        input: vec![0, 0],
        output: vec![0],
    };
    spec.examples = vec![e.clone(), e];
    assert!(spec.vocabulary().is_err());
    spec.examples = vec![Example {
        input: vec![3, 0],
        output: vec![0],
    }];
    assert!(spec.vocabulary().is_err());
    spec.examples = vec![Example {
        input: vec![1, 0],
        output: vec![101],
    }];
    assert!(spec.vocabulary().is_err());
}

#[test]
fn learn_arithmetic_vocabulary_lowers_and_checks_every_example() {
    for op in [
        Operation::Mul,
        Operation::DivFloor,
        Operation::DivCeil,
        Operation::MulDivFloor,
        Operation::MulDivCeil,
        Operation::ProductsGe2,
        Operation::ProductsGe3,
    ] {
        let mut spec = base();
        spec.constants = vec![2];
        spec.operations = vec![op];
        spec.max_size = 1 + op.arguments().len();
        if op.kind() == Kind::Bool {
            spec.outputs = vec![Domain::Bool];
        }
        all_examples(&mut spec, |x, y| {
            vec![match op {
                Operation::Mul => x * y,
                Operation::DivFloor => x / 2,
                Operation::DivCeil => (x + 1) / 2,
                Operation::MulDivFloor => x * y / 2,
                Operation::MulDivCeil => (x * y + 1) / 2,
                Operation::ProductsGe2 => i128::from(x * y >= 4),
                Operation::ProductsGe3 => i128::from(x * y * 2 >= 8),
                _ => unreachable!(),
            }]
        });
        let analysis = analyzed(&spec);
        assert_eq!(analysis.status, Status::Determined, "{op:?}");
        let candidate = analysis.candidate.unwrap_or_else(|| panic!("candidate"));
        assert_eq!(
            check_examples(&spec, &candidate, &mut Budget::new(Limits::default())),
            Ok(ExampleCheck::Reproduced { checked: 9 }),
            "{op:?}"
        );
        let expanded = &analysis.representatives[0].expression;
        let text = expr::print_symbolic(expanded);
        assert_eq!(
            expr::parse(&text, 0, expr::Allow::PLAIN),
            Ok(expanded.clone())
        );
    }
}

#[test]
fn learn_product_comparison_control_is_load_bearing() {
    for op in [Operation::ProductsGe2, Operation::ProductsGe3] {
        let mut spec = base();
        spec.outputs = vec![Domain::Bool];
        spec.constants = vec![2];
        spec.operations = vec![op];
        spec.max_size = 1 + op.arguments().len();
        all_examples(&mut spec, |x, y| vec![i128::from(x * y >= 4)]);
        let candidate = analyzed(&spec)
            .candidate
            .unwrap_or_else(|| panic!("candidate"));
        assert!(
            candidate
                .nodes()
                .iter()
                .any(|n| matches!(n, Instruction::ProductsGe(..)))
        );
        // Mutate only the product-comparison instruction to an always-true stub.
        // Both runs still use the production lowered-example checker and D1.
        let control = |stub: bool| {
            let nodes = candidate
                .nodes()
                .iter()
                .map(|n| {
                    if stub && matches!(n, Instruction::ProductsGe(..)) {
                        Instruction::Bool(true)
                    } else {
                        n.clone()
                    }
                })
                .collect();
            let program = Program::try_new(
                candidate.inputs().to_vec(),
                candidate.outputs().to_vec(),
                nodes,
                candidate.roots().to_vec(),
            )
            .unwrap_or_else(|e| panic!("{e:?}"));
            matches!(
                check_examples(&spec, &program, &mut Budget::new(Limits::default())),
                Ok(ExampleCheck::Reproduced { checked: 9 })
            )
        };
        assert!(control(false));
        assert!(
            !control(true),
            "stubbed product comparison must fail learn control"
        );
    }
}

#[test]
fn learn_fused_probes_preserve_full_products_and_refusals() {
    for op in [Operation::MulDivFloor, Operation::MulDivCeil] {
        let p = op.probe().unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            p.evaluate(&[i128::MAX, i128::MAX, i128::MAX]),
            Ok(vec![i128::MAX])
        );
        for input in [
            [1, 2, 0],
            [-1, 2, 1],
            [1, -2, 1],
            [1, 2, -1],
            [i128::MAX, i128::MAX, 1],
        ] {
            assert_eq!(p.evaluate(&input), Err(Error::Arithmetic));
        }
    }
    let p = Operation::ProductsGe3
        .probe()
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(p.evaluate(&[i128::MAX; 6]), Ok(vec![1]));
    assert_eq!(p.evaluate(&[1, 1, 1, 1, 1, -1]), Err(Error::Arithmetic));
}
