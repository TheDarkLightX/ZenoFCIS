//! Counterexample screening through the same evaluator used for replay.
use zeno_fcis_loop_doors::fuzz::{self, Limits, Observation, State};
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

fn program(inputs: Vec<Domain>, outputs: Vec<Domain>, nodes: Vec<Op>, roots: Vec<u16>) -> Program {
    Program::try_new(inputs, outputs, nodes, roots).unwrap_or_else(|e| panic!("fixture: {e:?}"))
}

fn needle() -> (Program, Program) {
    let d = Domain::Int {
        min: 0,
        max: 1_000_000_000,
    };
    (
        program(
            vec![d],
            vec![Domain::Bool],
            vec![Op::Input(0), Op::Int(777_777_777), Op::Eq(0, 1)],
            vec![2],
        ),
        program(vec![d], vec![Domain::Bool], vec![Op::Bool(false)], vec![0]),
    )
}

fn reservation() -> (Program, Program, Vec<State>) {
    let inputs = vec![Domain::Bool; 3];
    let outputs = vec![Domain::Bool; 3];
    let nodes = vec![
        Op::Input(0),
        Op::Input(1),
        Op::Input(2),
        Op::Not(1),
        Op::Select(2, 3, 0),
        Op::Bool(true),
        Op::Bool(false),
        Op::Select(3, 5, 0),
        Op::Select(2, 7, 6),
        Op::Select(2, 5, 1),
    ];
    (
        program(
            inputs.clone(),
            outputs.clone(),
            nodes.clone(),
            vec![4, 8, 9],
        ),
        program(inputs, outputs, nodes, vec![4, 8, 8]),
        vec![
            State {
                input: 0,
                output: 1,
                initial: 0,
            },
            State {
                input: 1,
                output: 2,
                initial: 0,
            },
        ],
    )
}

#[test]
fn a_scalar_witness_replays_without_enumerating_the_domain() {
    let (reference, candidate) = needle();
    let report = fuzz::screen(&reference, &candidate, &[], &[], Limits::default())
        .unwrap_or_else(|e| panic!("screen: {e}"));
    let w = report.witness.unwrap_or_else(|| panic!("expected witness"));
    assert_eq!(w.input, vec![777_777_777]);
    assert_eq!(reference.evaluate(&w.input), Ok(vec![1]));
    assert_eq!(candidate.evaluate(&w.input), Ok(vec![0]));
    assert!(report.pairs <= Limits::default().pairs);
}

#[test]
fn retained_traces_find_stateful_history_loss_and_are_reduced() {
    let (reference, candidate, state) = reservation();
    let seeds = vec![vec![vec![0], vec![1], vec![0], vec![1]]];
    let report = fuzz::screen(&reference, &candidate, &state, &seeds, Limits::default())
        .unwrap_or_else(|e| panic!("screen: {e}"));
    let w = report.witness.unwrap_or_else(|| panic!("expected witness"));
    assert_eq!(w.trace, vec![vec![1], vec![0]]);
    assert_eq!(w.step, 1);
    assert_eq!(w.input, vec![1, 1, 0]);
    assert_eq!(w.reference, Observation::Values(vec![1, 0, 1]));
    assert_eq!(w.candidate, Observation::Values(vec![1, 0, 0]));
    // Seeds, never old verdicts, are portable to another candidate.
    let next = fuzz::screen(
        &reference,
        &reference,
        &state,
        &report.corpus,
        Limits::default(),
    )
    .unwrap_or_else(|e| panic!("replay: {e}"));
    assert!(next.witness.is_none());
}

#[test]
fn identical_programs_are_only_inconclusive_and_repeat_deterministically() {
    let (reference, _, state) = reservation();
    let limits = Limits {
        trials: 40,
        pairs: 320,
        ..Limits::default()
    };
    let a = fuzz::screen(&reference, &reference, &state, &[], limits)
        .unwrap_or_else(|e| panic!("screen: {e}"));
    let b = fuzz::screen(&reference, &reference, &state, &[], limits)
        .unwrap_or_else(|e| panic!("screen: {e}"));
    assert_eq!(a, b);
    assert!(a.witness.is_none());
}

#[test]
fn every_seed_is_checked_before_a_failing_first_seed_runs() {
    let (r, c) = needle();
    let seeds = vec![vec![vec![777_777_777]], vec![vec![-1]]];
    let result = fuzz::screen(&r, &c, &[], &seeds, Limits::default());
    assert_eq!(
        result.map_err(|e| e.0),
        Err("fuzz-seed-domain-or-shape".into())
    );
}

#[test]
fn invalid_abi_bindings_and_limits_are_refused() {
    let (r, c, state) = reservation();
    let mut bad = state.clone();
    bad[1].input = bad[0].input;
    assert!(fuzz::screen(&r, &c, &bad, &[], Limits::default()).is_err());
    bad = state.clone();
    bad[0].output = 99;
    assert!(fuzz::screen(&r, &c, &bad, &[], Limits::default()).is_err());
    bad = state;
    bad[0].initial = 2;
    assert!(fuzz::screen(&r, &c, &bad, &[], Limits::default()).is_err());
    let limits = Limits {
        steps: 65,
        ..Limits::default()
    };
    assert!(fuzz::screen(&r, &c, &[], &[], limits).is_err());
    let (other, _) = needle();
    assert!(fuzz::screen(&r, &other, &[], &[], Limits::default()).is_err());
}

#[test]
fn reduction_and_final_replay_never_exceed_small_budgets() {
    let (r, c, state) = reservation();
    for trials in 2..12 {
        for pairs in 2..32 {
            let limits = Limits {
                trials,
                pairs,
                steps: 4,
                ..Limits::default()
            };
            let report = fuzz::screen(&r, &c, &state, &[vec![vec![1], vec![0]]], limits)
                .unwrap_or_else(|e| panic!("screen: {e}"));
            assert!(report.trials <= trials);
            assert!(report.pairs <= pairs);
            if let Some(w) = report.witness {
                assert_ne!(w.reference, w.candidate);
            }
        }
    }
}

#[test]
fn full_width_i64_sampling_does_not_overflow_or_materialize_a_table() {
    let d = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    let r = program(vec![d], vec![d], vec![Op::Input(0)], vec![0]);
    let limits = Limits {
        uniform: true,
        trials: 16,
        pairs: 32,
        ..Limits::default()
    };
    assert!(
        fuzz::screen(&r, &r, &[], &[], limits)
            .unwrap_or_else(|e| panic!("{e}"))
            .witness
            .is_none()
    );
}

#[test]
fn output_domain_and_eager_arithmetic_failures_are_observations() {
    let d = Domain::Int { min: 0, max: 3 };
    let r = program(
        vec![d],
        vec![d],
        vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)],
        vec![2],
    );
    let c = program(vec![d], vec![d], vec![Op::Input(0)], vec![0]);
    let report = fuzz::screen(
        &r,
        &c,
        &[],
        &[vec![vec![3]]],
        Limits {
            trials: 2,
            ..Limits::default()
        },
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        report.witness.map(|w| w.reference),
        Some(Observation::OutputDomain)
    );
    let r = program(
        vec![Domain::Bool],
        vec![Domain::Bool],
        vec![Op::Input(0), Op::Int(i64::MAX), Op::Int(1), Op::Add(1, 2)],
        vec![0],
    );
    let c = program(
        vec![Domain::Bool],
        vec![Domain::Bool],
        vec![Op::Input(0)],
        vec![0],
    );
    let report = fuzz::screen(&r, &c, &[], &[vec![vec![0]]], Limits::default())
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        report.witness.map(|w| w.reference),
        Some(Observation::Arithmetic)
    );
    assert!(
        fuzz::screen(&r, &r, &[], &[], Limits::default())
            .unwrap_or_else(|e| panic!("{e}"))
            .witness
            .is_none()
    );
}

#[test]
fn equal_budget_guidance_comparison_has_a_live_uniform_baseline() {
    let (r, c) = needle();
    let mut found = [0; 2];
    for (mode, uniform) in [false, true].into_iter().enumerate() {
        for seed in 0..32 {
            let limits = Limits {
                seed,
                uniform,
                trials: 32,
                pairs: 64,
                ..Limits::default()
            };
            let report = fuzz::screen(&r, &c, &[], &[], limits).unwrap_or_else(|e| panic!("{e}"));
            found[mode] += usize::from(report.witness.is_some());
            assert!(report.pairs <= 64);
        }
    }
    // A deliberately boundary-sensitive task, not evidence of general superiority.
    assert!(found[0] > found[1], "directed/uniform: {found:?}");
    println!("needle case, 32 fixed seeds, directed/uniform findings: {found:?}");
}
