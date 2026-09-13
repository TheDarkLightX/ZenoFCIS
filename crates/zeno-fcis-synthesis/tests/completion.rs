//! Independent reachability, capacity, and hostile-policy checks.
#![allow(clippy::unwrap_used)]

use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_synthesis::finite::completion::{
    CompletionError, CompletionLimits, CompletionProblem, find_completion, verify_completion,
};
use zeno_fcis_synthesis::finite::{Domain, Error, Op, Program};
use zeno_fcis_value::Value;

fn terminal(domain: Domain, value: i64) -> Program {
    Program::try_new(
        vec![domain],
        vec![Domain::Bool],
        vec![Op::Input(0), Op::Int(value), Op::Eq(0, 1)],
        vec![2],
    )
    .unwrap()
}

fn capacity_model(repaired: bool) -> (Program, Program) {
    let state = Domain::Int { min: 0, max: 3 };
    let step = Program::try_new(
        vec![state, Domain::Bool],
        vec![Domain::Bool, state],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Int(0),
            Op::Int(1),
            Op::Int(3),
            Op::Lt(2, 0),
            Op::Lt(0, 4),
            Op::Not(1),
            Op::And(7, 6),
            Op::And(5, 6),
            Op::Bool(repaired),
            Op::Select(10, 5, 9),
            Op::And(1, 11),
            Op::Not(8),
            Op::Not(12),
            Op::And(13, 14),
            Op::Not(15),
            Op::Add(0, 3),
            Op::Sub(0, 3),
            Op::Select(8, 17, 0),
            Op::Select(12, 18, 19),
        ],
        vec![16, 20],
    )
    .unwrap();
    (step, terminal(state, 0))
}

fn problem() -> CompletionProblem {
    let (step, terminal) = capacity_model(true);
    CompletionProblem::try_new(step, terminal, CompletionLimits::default()).unwrap()
}

#[test]
fn a_full_state_must_retain_an_exit_even_when_growth_is_rejected() {
    let (step, terminal) = capacity_model(false);
    let broken = CompletionProblem::try_new(step, terminal, CompletionLimits::default()).unwrap();
    assert_eq!(
        find_completion(&broken).unwrap_err(),
        CompletionError::NoExit { state: vec![3] }
    );
    let repaired = problem();
    let plan = find_completion(&repaired).unwrap();
    let checked = verify_completion(&repaired, &plan).unwrap();
    for state in 0..=3 {
        assert_eq!(
            plan.steps[usize::try_from(state).unwrap()].remaining,
            u32::try_from(state).unwrap()
        );
        assert_eq!(
            checked.next_command(&[state]).unwrap(),
            if state == 0 {
                None
            } else {
                Some([1].as_slice())
            }
        );
    }
}

fn two_state_model(edges: [Option<i64>; 2], terminals: u8) -> CompletionProblem {
    let domain = Domain::Int { min: 0, max: 1 };
    let step = Program::try_new(
        vec![domain],
        vec![Domain::Bool, domain],
        vec![
            Op::Input(0),
            Op::Int(0),
            Op::Eq(0, 1),
            Op::Bool(edges[0].is_some()),
            Op::Bool(edges[1].is_some()),
            Op::Select(2, 3, 4),
            Op::Int(edges[0].unwrap_or(0)),
            Op::Int(edges[1].unwrap_or(1)),
            Op::Select(2, 6, 7),
        ],
        vec![5, 8],
    )
    .unwrap();
    let terminal = Program::try_new(
        vec![domain],
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Int(0),
            Op::Eq(0, 1),
            Op::Bool(terminals & 1 != 0),
            Op::Bool(terminals & 2 != 0),
            Op::Select(2, 3, 4),
        ],
        vec![5],
    )
    .unwrap();
    CompletionProblem::try_new(step, terminal, CompletionLimits::default()).unwrap()
}

#[test]
fn every_two_state_graph_matches_independent_forward_path_exploration() {
    let mut models = 0;
    for left in [None, Some(0), Some(1)] {
        for right in [None, Some(0), Some(1)] {
            for terminals in 0..4 {
                let edges = [left, right];
                let mut distances = [None; 2];
                for (start, result) in distances.iter_mut().enumerate() {
                    let mut seen = [false; 2];
                    let mut current = start;
                    let mut steps = 0;
                    loop {
                        if terminals & (1 << current) != 0 {
                            *result = Some(steps);
                            break;
                        }
                        if seen[current] {
                            break;
                        }
                        seen[current] = true;
                        let Some(next) = edges[current] else {
                            break;
                        };
                        current = usize::try_from(next).unwrap();
                        steps += 1;
                    }
                }
                let model = two_state_model(edges, terminals);
                match distances.iter().position(Option::is_none) {
                    Some(first) => assert_eq!(
                        find_completion(&model).unwrap_err(),
                        CompletionError::NoExit {
                            state: vec![i64::try_from(first).unwrap()]
                        }
                    ),
                    None => {
                        let plan = find_completion(&model).unwrap();
                        let checked = verify_completion(&model, &plan).unwrap();
                        for (index, distance) in distances.into_iter().enumerate() {
                            assert_eq!(checked.plan().steps[index].remaining, distance.unwrap());
                        }
                    }
                }
                models += 1;
            }
        }
    }
    assert_eq!(models, 36);
}

#[test]
fn a_self_loop_is_enabled_but_is_not_an_exit() {
    let model = two_state_model([Some(0), Some(1)], 1);
    assert_eq!(
        find_completion(&model).unwrap_err(),
        CompletionError::NoExit { state: vec![1] }
    );
}

#[test]
fn the_verifier_rejects_changed_bindings_rows_commands_and_ranks() {
    let model = problem();
    let original = find_completion(&model).unwrap();
    let mut changes = Vec::new();
    let mut changed = original.clone();
    changed.problem_hash = Hash32::new([7; 32]);
    changes.push(changed);
    let mut changed = original.clone();
    changed.steps.pop();
    changes.push(changed);
    let mut changed = original.clone();
    changed.steps.push(original.steps[0].clone());
    changes.push(changed);
    for (state, rank, command) in [
        (0, 1, vec![]),
        (0, 0, vec![0]),
        (1, 0, vec![1]),
        (1, 1, vec![0]),
        (1, 1, vec![2]),
        (1, 1, vec![]),
        (1, u32::MAX, vec![1]),
        (2, 1, vec![1]),
        (3, 3, vec![0]),
    ] {
        let mut changed = original.clone();
        changed.steps[state].remaining = rank;
        changed.steps[state].command = command;
        changes.push(changed);
    }
    for changed in changes {
        assert!(verify_completion(&model, &changed).is_err());
    }
    let checked = verify_completion(&model, &original).unwrap();
    let mut transport = original;
    transport.steps[1].command[0] = 0;
    assert_eq!(checked.next_command(&[1]).unwrap(), Some([1].as_slice()));
    assert!(checked.next_command(&[-1]).is_err());
    assert!(checked.next_command(&[4]).is_err());
    assert!(checked.next_command(&[]).is_err());
}

#[test]
fn unselected_and_terminal_state_commands_cannot_hide_an_arithmetic_trap() {
    let state = Domain::Int { min: 0, max: 0 };
    let step = Program::try_new(
        vec![state, Domain::Bool],
        vec![Domain::Bool, state],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Int(0),
            Op::Int(1),
            Op::Select(1, 3, 2),
            Op::Int(i64::MAX),
            Op::Add(5, 4),
            Op::Bool(true),
        ],
        vec![7, 0],
    )
    .unwrap();
    let model =
        CompletionProblem::try_new(step, terminal(state, 0), CompletionLimits::default()).unwrap();
    assert!(matches!(
        find_completion(&model),
        Err(CompletionError::Evaluation {
            source: Error::Arithmetic,
            ..
        })
    ));
    let plan = zeno_fcis_synthesis::finite::completion::CompletionPlan {
        problem_hash: model.problem_hash(),
        steps: vec![zeno_fcis_synthesis::finite::completion::CompletionStep {
            remaining: 0,
            command: vec![],
        }],
    };
    assert!(matches!(
        verify_completion(&model, &plan),
        Err(CompletionError::Evaluation {
            source: Error::Arithmetic,
            ..
        })
    ));
}

#[test]
fn a_rejected_transition_cannot_secretly_change_the_state() {
    let state = Domain::Int { min: 0, max: 1 };
    let step = Program::try_new(
        vec![state],
        vec![Domain::Bool, state],
        vec![Op::Bool(false), Op::Int(0)],
        vec![0, 1],
    )
    .unwrap();
    let model =
        CompletionProblem::try_new(step, terminal(state, 0), CompletionLimits::default()).unwrap();
    assert!(
        matches!(find_completion(&model), Err(CompletionError::RejectedStateChange { input }) if input == [1])
    );
}

#[test]
fn complete_resource_reservations_reject_one_below_each_required_bound() {
    for resource in [
        "transitions",
        "steps",
        "state-bytes",
        "command-bytes",
        "policy-bytes",
    ] {
        let mut limits = CompletionLimits::default();
        let set = |limits: &mut CompletionLimits, value| match resource {
            "transitions" => limits.max_transitions = value,
            "steps" => limits.max_steps = value,
            "state-bytes" => limits.max_state_bytes = value,
            "command-bytes" => limits.max_command_bytes = value,
            "policy-bytes" => limits.max_policy_bytes = value,
            _ => unreachable!(),
        };
        set(&mut limits, 0);
        let (step, terminal) = capacity_model(true);
        let error = CompletionProblem::try_new(step, terminal, limits).unwrap_err();
        let CompletionError::Resource(Error::Budget {
            resource: actual,
            required,
            ..
        }) = error
        else {
            panic!("{error:?}")
        };
        assert_eq!(actual, resource);
        set(&mut limits, required - 1);
        let (step, terminal) = capacity_model(true);
        assert!(CompletionProblem::try_new(step, terminal, limits).is_err());
        set(&mut limits, required);
        let (step, terminal) = capacity_model(true);
        let model = CompletionProblem::try_new(step, terminal, limits).unwrap();
        let checked = verify_completion(&model, &find_completion(&model).unwrap()).unwrap();
        assert!(
            u64::try_from(checked.canonical_bytes().unwrap().len()).unwrap()
                <= limits.max_policy_bytes
        );
    }
}

#[test]
fn bindings_include_resource_policy_and_program_semantics() {
    let original = problem();
    let plan = find_completion(&original).unwrap();
    let (step, terminal) = capacity_model(true);
    let different = CompletionProblem::try_new(
        step,
        terminal,
        CompletionLimits {
            max_command_bytes: 100,
            ..CompletionLimits::default()
        },
    )
    .unwrap();
    assert_ne!(original.problem_hash(), different.problem_hash());
    assert!(verify_completion(&different, &plan).is_err());
    let (step, terminal) = capacity_model(false);
    let different =
        CompletionProblem::try_new(step, terminal, CompletionLimits::default()).unwrap();
    assert_ne!(original.problem_hash(), different.problem_hash());
    assert!(verify_completion(&different, &plan).is_err());
}

#[test]
fn canonical_state_size_is_the_actual_codec_size() {
    let wire = Value::Tuple(vec![Value::I128(3)].into_boxed_slice())
        .canonical_bytes()
        .unwrap();
    let (step, terminal) = capacity_model(true);
    let model = CompletionProblem::try_new(
        step,
        terminal,
        CompletionLimits {
            max_state_bytes: u64::try_from(wire.len()).unwrap(),
            ..CompletionLimits::default()
        },
    )
    .unwrap();
    assert!(find_completion(&model).is_ok());
}

#[test]
fn search_uses_lexicographic_ties_but_verification_accepts_any_valid_decrease() {
    let state = Domain::Int { min: 0, max: 1 };
    let command = Domain::Int { min: -1, max: 1 };
    let step = Program::try_new(
        vec![state, command],
        vec![Domain::Bool, state],
        vec![Op::Bool(true), Op::Int(0)],
        vec![0, 1],
    )
    .unwrap();
    let model =
        CompletionProblem::try_new(step, terminal(state, 0), CompletionLimits::default()).unwrap();
    let mut plan = find_completion(&model).unwrap();
    assert_eq!(plan.steps[1].command, [-1]);
    plan.steps[1].command = vec![1];
    assert!(verify_completion(&model, &plan).is_ok());
}
