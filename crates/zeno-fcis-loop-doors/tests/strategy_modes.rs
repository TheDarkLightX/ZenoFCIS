//! Scoped semantic controls for finite typed strategy proposals. LLM-generated.
use std::collections::BTreeMap;
use zeno_fcis_loop_doors::strategy_algebra::{self as A, Step};
use zeno_fcis_loop_doors::strategy_questions as Q;
use zeno_fcis_loop_doors::strategy_split as S;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};
fn limits() -> Q::Limits {
    Q::Limits {
        worlds: 4096,
        items: 128,
        cells: 1_000_000,
        work: 10_000_000,
    }
}
fn boolp(nodes: Vec<Op>, root: u16) -> Program {
    Program::try_new(vec![Domain::Bool], vec![Domain::Bool], nodes, vec![root])
        .unwrap_or_else(|e| panic!("{e:?}"))
}
fn intp(nodes: Vec<Op>, root: u16) -> Program {
    Program::try_new(
        vec![Domain::Int { min: 0, max: 3 }],
        vec![Domain::Int {
            min: i64::MIN,
            max: i64::MAX,
        }],
        nodes,
        vec![root],
    )
    .unwrap_or_else(|e| panic!("{e:?}"))
}
#[test]
fn strategy_cases_learn_pieces_and_split_then_visit_remaining_regions() {
    let original = intp(
        vec![
            Op::Input(0),
            Op::Int(2),
            Op::Lt(0, 1),
            Op::Int(7),
            Op::Select(2, 3, 0),
        ],
        4,
    );
    let predicate = Program::try_new(
        original.inputs().to_vec(),
        vec![Domain::Bool],
        vec![Op::Input(0), Op::Int(2), Op::Lt(0, 1)],
        vec![2],
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let corpus = S::LearnTermsV1 {
        integers: vec![intp(vec![Op::Int(7)], 0), intp(vec![Op::Input(0)], 0)],
        predicates: vec![predicate],
        complete: true,
    };
    let run = S::split(
        &original,
        &corpus,
        S::Config {
            initial: 2,
            spot_checks: 1,
            ..S::Config::default()
        },
        limits(),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    for x in 0..=3 {
        assert_eq!(original.evaluate(&[x]), run.candidate.evaluate(&[x]));
    }
    assert!(!run.spot_queries.is_empty());
    let mut trace = run.trace;
    // Structural plans require capture before the final check; neither step's
    // presence alone supplies semantic authority.
    let mut without_capture = trace.clone();
    without_capture.push(Step::CheckProgram);
    assert!(
        !A::conforms(
            &A::named("split-by-cases", 64).unwrap_or_else(|| panic!("plan")),
            &without_capture,
            100_000
        )
        .unwrap_or(false)
    );
    trace.push(Step::Capture);
    trace.push(Step::CheckProgram);
    assert!(
        A::conforms(
            &A::named("split-by-cases", 64).unwrap_or_else(|| panic!("plan")),
            &trace,
            100_000
        )
        .unwrap_or(false)
    );
}
#[test]
fn strategy_cases_never_promote_an_incomplete_learn_corpus() {
    let original = intp(vec![Op::Input(0)], 0);
    let corpus = S::LearnTermsV1 {
        integers: vec![original.clone()],
        predicates: vec![],
        complete: false,
    };
    let error = S::split(&original, &corpus, S::Config::default(), limits())
        .err()
        .unwrap_or_else(|| panic!("stop"));
    assert!(error.inconclusive);
    assert_eq!(error.code, "strategy-learn-incomplete");
}
#[test]
fn strategy_diagnosis_probes_split_minimal_weak_fault_explanations() {
    let c = Q::Circuit {
        inputs: 1,
        gates: vec![
            Q::Gate {
                kind: Q::GateKind::Not,
                wires: vec![0],
            },
            Q::Gate {
                kind: Q::GateKind::Not,
                wires: vec![1],
            },
        ],
        outputs: vec![2],
    };
    let first = Q::diagnose(&c, &[0], &BTreeMap::from([(2, 1)]), 2, limits())
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(first.diagnoses.len(), 2);
    assert_eq!(
        first.probe,
        Some(Q::Probe {
            wire: 1,
            zero: 1,
            one: 1
        })
    );
    for d in &first.diagnoses {
        assert_eq!(d.gates.len(), 1);
        assert!(d.worlds.iter().all(|w| w[2] == 1));
    }
    let second = Q::diagnose(&c, &[0], &BTreeMap::from([(2, 1), (1, 0)]), 2, limits())
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(second.diagnoses[0].gates, vec![0]);
    assert_eq!(second.diagnoses.len(), 1);
    assert!(
        A::conforms(
            &A::named("diagnose", 12).unwrap_or_else(|| panic!("plan")),
            &first.trace,
            100_000
        )
        .unwrap_or(false)
    );
}
#[test]
fn strategy_readings_ask_only_when_an_answer_changes_and_collapse_spellings() {
    let yes = boolp(vec![Op::Bool(true)], 0);
    let same = boolp(vec![Op::Input(0), Op::Eq(0, 0)], 1);
    let x = boolp(vec![Op::Input(0)], 0);
    let group = Q::Reading {
        id: "s1".into(),
        alternatives: vec![yes.clone(), same, x.clone()],
    };
    let run =
        Q::readings(&yes, &[group], Q::Query::Count, limits()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(run.distinct, vec![vec![0, 2]]);
    let q = run.question.unwrap_or_else(|| panic!("question"));
    assert_eq!(q.ordinal, 0);
    assert_eq!(q.holds, vec![0]);
    assert_eq!(q.does_not_hold, vec![2]);
    let notx = boolp(vec![Op::Input(0), Op::Not(0)], 1);
    let stable = Q::readings(
        &yes,
        &[Q::Reading {
            id: "s1".into(),
            alternatives: vec![x, notx],
        }],
        Q::Query::Count,
        limits(),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(stable.question.is_none());
    assert!(stable.answers.iter().all(|(_, answer)| *answer == 1));
    assert!(
        A::conforms(
            &A::named("ask-when-it-matters", 8).unwrap_or_else(|| panic!("plan")),
            &stable.trace,
            100_000
        )
        .unwrap_or(false)
    );
}
#[test]
fn strategy_missing_reading_is_not_recovered_and_budget_has_no_partial_answer() {
    let bg = boolp(vec![Op::Bool(true)], 0);
    let falsep = boolp(vec![Op::Bool(false)], 0);
    let group = Q::Reading {
        id: "s1".into(),
        alternatives: vec![falsep.clone()],
    };
    let run = Q::readings(&bg, std::slice::from_ref(&group), Q::Query::Count, limits())
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(run.answers[0].1, 0);
    assert!(run.question.is_none());
    let stopped = Q::readings(
        &bg,
        &[group],
        Q::Query::Count,
        Q::Limits {
            work: 0,
            ..limits()
        },
    )
    .err()
    .unwrap_or_else(|| panic!("stop"));
    assert!(stopped.inconclusive);
    assert!(stopped.work > 0);
}

#[test]
fn strategy_resource_stops_return_no_partial_proposals() {
    let c = Q::Circuit {
        inputs: 1,
        gates: vec![Q::Gate {
            kind: Q::GateKind::Not,
            wires: vec![0],
        }],
        outputs: vec![1],
    };
    for l in [
        Q::Limits {
            worlds: 0,
            ..limits()
        },
        Q::Limits {
            cells: 0,
            ..limits()
        },
        Q::Limits {
            items: 0,
            ..limits()
        },
        Q::Limits {
            work: 0,
            ..limits()
        },
    ] {
        let stopped = Q::diagnose(&c, &[0], &BTreeMap::from([(1, 1)]), 1, l)
            .err()
            .unwrap_or_else(|| panic!("stop"));
        assert!(stopped.inconclusive);
    }
    let original = intp(vec![Op::Input(0)], 0);
    let corpus = S::LearnTermsV1 {
        integers: vec![original.clone()],
        predicates: vec![],
        complete: true,
    };
    let stopped = S::split(
        &original,
        &corpus,
        S::Config::default(),
        Q::Limits {
            cells: 0,
            ..limits()
        },
    )
    .err()
    .unwrap_or_else(|| panic!("stop"));
    assert!(stopped.inconclusive);
    assert_eq!(stopped.work, 0);
    let stopped = S::split(
        &original,
        &corpus,
        S::Config {
            queries: 0,
            ..S::Config::default()
        },
        limits(),
    )
    .err()
    .unwrap_or_else(|| panic!("stop"));
    assert!(stopped.inconclusive);
}
#[test]
fn strategy_healthy_and_masked_faults_do_not_become_actual_fault_claims() {
    let c = Q::Circuit {
        inputs: 1,
        gates: vec![Q::Gate {
            kind: Q::GateKind::Not,
            wires: vec![0],
        }],
        outputs: vec![1],
    };
    let run = Q::diagnose(&c, &[0], &BTreeMap::from([(1, 1)]), 2, limits())
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(run.diagnoses.len(), 1);
    assert!(run.diagnoses[0].gates.is_empty());
    assert_eq!(run.diagnoses[0].worlds, vec![vec![0, 1]]);
    let inconsistent = Q::diagnose(&c, &[0], &BTreeMap::from([(0, 1)]), 2, limits())
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(inconsistent.diagnoses.is_empty());
    assert!(inconsistent.probe.is_none());
}
#[test]
fn strategy_results_and_plans_have_no_mutable_input_aliases() {
    let bg = boolp(vec![Op::Bool(true)], 0);
    let groups = vec![Q::Reading {
        id: "s1".into(),
        alternatives: vec![bg.clone(), boolp(vec![Op::Input(0)], 0)],
    }];
    let first =
        Q::readings(&bg, &groups, Q::Query::Count, limits()).unwrap_or_else(|e| panic!("{e:?}"));
    let mut changed = first.clone();
    changed.distinct.clear();
    changed.answers.clear();
    if let Some(q) = &mut changed.question {
        q.holds.clear();
    }
    let next =
        Q::readings(&bg, &groups, Q::Query::Count, limits()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(first.distinct, next.distinct);
    assert_eq!(first.answers, next.answers);
    assert_ne!(changed.distinct, next.distinct);
}

#[test]
fn strategy_cases_region_checks_find_a_nested_rare_exception() {
    let make = |nodes, root| {
        Program::try_new(
            vec![Domain::Int { min: 0, max: 7 }],
            vec![Domain::Int {
                min: i64::MIN,
                max: i64::MAX,
            }],
            nodes,
            vec![root],
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
    };
    let original = make(
        vec![
            Op::Input(0),
            Op::Int(4),
            Op::Lt(0, 1),
            Op::Int(2),
            Op::Eq(0, 3),
            Op::Int(9),
            Op::Int(5),
            Op::Select(4, 5, 6),
            Op::Int(7),
            Op::Select(2, 7, 8),
        ],
        9,
    );
    let predicate = |nodes| {
        Program::try_new(
            original.inputs().to_vec(),
            vec![Domain::Bool],
            nodes,
            vec![2],
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
    };
    let corpus = S::LearnTermsV1 {
        integers: vec![
            make(vec![Op::Int(5)], 0),
            make(vec![Op::Int(7)], 0),
            make(vec![Op::Int(9)], 0),
        ],
        predicates: vec![
            predicate(vec![Op::Input(0), Op::Int(4), Op::Lt(0, 1)]),
            predicate(vec![Op::Input(0), Op::Int(2), Op::Eq(0, 1)]),
        ],
        complete: true,
    };
    let run = S::split(
        &original,
        &corpus,
        S::Config {
            initial: 2,
            spot_checks: 1,
            ..S::Config::default()
        },
        limits(),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(run.spot_queries.first(), Some(&2));
    for x in 0..=7 {
        assert_eq!(original.evaluate(&[x]), run.candidate.evaluate(&[x]));
    }
}

#[test]
fn strategy_stops_keep_the_invoked_trace_without_partial_results() {
    let bg = boolp(vec![Op::Bool(true)], 0);
    let groups = vec![Q::Reading {
        id: "s1".into(),
        alternatives: vec![bg.clone(), boolp(vec![Op::Input(0)], 0)],
    }];
    let stop = Q::readings(
        &bg,
        &groups,
        Q::Query::Count,
        Q::Limits {
            work: 6,
            ..limits()
        },
    )
    .err()
    .unwrap_or_else(|| panic!("stop"));
    assert_eq!(stop.trace, vec![Step::Formalize, Step::Distinct]);
    assert!(
        A::conforms(
            &A::named("ask-when-it-matters", 64).unwrap_or_else(|| panic!("plan")),
            &stop.trace,
            100_000
        )
        .unwrap_or(false)
    );
    let original = intp(vec![Op::Input(0)], 0);
    let corpus = S::LearnTermsV1 {
        integers: vec![original.clone()],
        predicates: vec![],
        complete: true,
    };
    let stop = S::split(
        &original,
        &corpus,
        S::Config {
            initial: 2,
            queries: 2,
            spot_checks: 1,
            ..S::Config::default()
        },
        limits(),
    )
    .err()
    .unwrap_or_else(|| panic!("stop"));
    assert_eq!(
        stop.trace,
        vec![
            Step::Sample,
            Step::LearnTerms,
            Step::Cover,
            Step::LearnSplit,
            Step::Assemble
        ]
    );
    assert!(
        A::conforms(
            &A::named("split-by-cases", 64).unwrap_or_else(|| panic!("plan")),
            &stop.trace,
            100_000
        )
        .unwrap_or(false)
    );
    let mut changed = stop.clone();
    changed.trace.clear();
    assert!(!stop.trace.is_empty());
    assert!(changed.trace.is_empty());
}
