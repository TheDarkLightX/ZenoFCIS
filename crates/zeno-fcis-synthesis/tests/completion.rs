//! Independent reachability, capacity, and hostile-policy checks.
#![allow(clippy::unwrap_used)]

use zeno_fcis_codec::{CanonicalEncode, DecodeError, Hash32};
use zeno_fcis_synthesis::finite::completion::{
    CompletionError, CompletionLimits, CompletionPlan, CompletionProblem, CompletionStep,
    find_completion, verify_completion, verify_completion_bytes,
};
use zeno_fcis_synthesis::finite::{Domain, Error, Op, Program};
use zeno_fcis_value::{Value, zcve};

const PLAN_PROFILE: &str = "zeno-fcis/completion-plan/1";

fn tuple(values: Vec<Value>) -> Value {
    Value::Tuple(values.into_boxed_slice())
}

fn plan_value(plan: &CompletionPlan) -> Value {
    tuple(vec![
        Value::Text(PLAN_PROFILE.into()),
        Value::Bytes(plan.problem_hash.as_bytes().to_vec().into_boxed_slice()),
        tuple(
            plan.steps
                .iter()
                .map(|row| {
                    tuple(vec![
                        Value::U128(row.remaining.into()),
                        tuple(
                            row.command
                                .iter()
                                .map(|v| Value::I128((*v).into()))
                                .collect(),
                        ),
                    ])
                })
                .collect(),
        ),
    ])
}

fn fields(value: &mut Value) -> &mut [Value] {
    let Value::Tuple(values) = value else {
        panic!("expected a test tuple")
    };
    values
}

fn row_fields(value: &mut Value, row: usize) -> &mut [Value] {
    fields(&mut fields(&mut fields(value)[2])[row])
}

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
    assert!(matches!(
        verify_completion_bytes(&model, &plan_value(&plan).canonical_bytes().unwrap()),
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
    let plan = CompletionPlan {
        problem_hash: model.problem_hash(),
        steps: vec![
            CompletionStep {
                remaining: 0,
                command: vec![],
            },
            CompletionStep {
                remaining: 1,
                command: vec![],
            },
        ],
    };
    assert!(matches!(
        verify_completion_bytes(&model, &plan_value(&plan).canonical_bytes().unwrap()),
        Err(CompletionError::RejectedStateChange { input }) if input == [1]
    ));
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

#[test]
fn portable_import_preserves_the_version_one_wire_format_and_owns_its_plan() {
    let model = problem();
    let plan = find_completion(&model).unwrap();
    let checked = verify_completion(&model, &plan).unwrap();
    let mut bytes = checked.canonical_bytes().unwrap();

    // Independent wire vector: tuple(profile, binding, rows), each row being
    // tuple(U128 rank, tuple(I128 command scalars)). The binding is source-bound.
    let mut expected = vec![zcve::TAG_TUPLE];
    expected.extend_from_slice(&3u32.to_be_bytes());
    expected.push(zcve::TAG_TEXT);
    expected.extend_from_slice(&u32::try_from(PLAN_PROFILE.len()).unwrap().to_be_bytes());
    expected.extend_from_slice(PLAN_PROFILE.as_bytes());
    expected.push(zcve::TAG_BYTES);
    expected.extend_from_slice(&32u32.to_be_bytes());
    expected.extend_from_slice(model.problem_hash().as_bytes());
    expected.push(zcve::TAG_TUPLE);
    expected.extend_from_slice(&4u32.to_be_bytes());
    for rank in 0..=3u128 {
        expected.push(zcve::TAG_TUPLE);
        expected.extend_from_slice(&2u32.to_be_bytes());
        expected.push(zcve::TAG_U128);
        expected.extend_from_slice(&rank.to_be_bytes());
        expected.push(zcve::TAG_TUPLE);
        expected.extend_from_slice(&u32::from(rank != 0).to_be_bytes());
        if rank != 0 {
            expected.push(zcve::TAG_I128);
            expected.extend_from_slice(&1i128.to_be_bytes());
        }
    }
    assert_eq!(bytes, expected);
    let imported = verify_completion_bytes(&model, &bytes).unwrap();
    assert_eq!(imported.plan(), &plan);
    assert_eq!(imported.canonical_bytes().unwrap(), expected);
    bytes.fill(0);
    assert_eq!(imported.canonical_bytes().unwrap(), expected);
    assert_eq!(imported.next_command(&[3]).unwrap(), Some([1].as_slice()));
}

#[test]
fn portable_import_rejects_truncation_trailing_data_and_noncanonical_values() {
    let model = problem();
    let bytes = verify_completion(&model, &find_completion(&model).unwrap())
        .unwrap()
        .canonical_bytes()
        .unwrap();
    for length in 0..bytes.len() {
        assert!(verify_completion_bytes(&model, &bytes[..length]).is_err());
    }
    let mut trailing = bytes;
    trailing.push(zcve::TAG_UNIT);
    assert!(matches!(
        verify_completion_bytes(&model, &trailing),
        Err(CompletionError::Decoding(DecodeError::TrailingBytes { .. }))
    ));
    assert!(matches!(
        verify_completion_bytes(&model, &[255]),
        Err(CompletionError::Decoding(DecodeError::UnknownTag(255)))
    ));
    // Repeated record fields cannot be silently normalized by import.
    let mut duplicate_fields = vec![zcve::TAG_RECORD];
    duplicate_fields.extend_from_slice(&2u32.to_be_bytes());
    for _ in 0..2 {
        duplicate_fields.extend_from_slice(&1u16.to_be_bytes());
        duplicate_fields.push(zcve::TAG_UNIT);
    }
    assert!(matches!(
        verify_completion_bytes(&model, &duplicate_fields),
        Err(CompletionError::Decoding(DecodeError::NonCanonicalRecord))
    ));
}

#[test]
fn portable_import_requires_exact_shapes_tags_and_integer_ranges() {
    let model = problem();
    let original = plan_value(&find_completion(&model).unwrap());
    let mut changes = vec![Value::Unit];
    let Value::Tuple(top) = original.clone() else {
        unreachable!()
    };
    changes.push(Value::Vector(top.clone()));
    changes.push(tuple(top[..2].to_vec()));
    let mut extra = top.to_vec();
    extra.push(Value::Unit);
    changes.push(tuple(extra));
    for value in [
        Value::Unit,
        Value::Text("binding".into()),
        Value::Bytes(vec![1; 31].into_boxed_slice()),
    ] {
        let mut changed = original.clone();
        fields(&mut changed)[1] = value;
        changes.push(changed);
    }
    for value in [
        Value::Bool(true),
        Value::I128(1),
        Value::Text("1".into()),
        Value::U128(u128::from(u32::MAX) + 1),
        // Truncation would recover row 1's valid rank and silently admit it.
        Value::U128((1u128 << 32) + 1),
    ] {
        let mut changed = original.clone();
        row_fields(&mut changed, 1)[0] = value;
        changes.push(changed);
    }
    for value in [
        Value::Bool(true),
        Value::U128(1),
        Value::Text("1".into()),
        Value::I128(i128::from(i64::MAX) + 1),
        Value::I128(i128::from(i64::MIN) - 1),
        // Truncation would recover the admitted Boolean exit command 1.
        Value::I128((1i128 << 64) + 1),
    ] {
        let mut changed = original.clone();
        row_fields(&mut changed, 1)[1] = tuple(vec![value]);
        changes.push(changed);
    }
    for value in [
        Value::Vector(vec![Value::I128(1)].into_boxed_slice()),
        tuple(vec![]),
        tuple(vec![Value::I128(1), Value::I128(1)]),
    ] {
        let mut changed = original.clone();
        row_fields(&mut changed, 1)[1] = value;
        changes.push(changed);
    }
    let mut changed = original.clone();
    fields(&mut fields(&mut changed)[2])[1] = Value::Unit;
    changes.push(changed);
    let mut changed = original;
    let Value::Tuple(rows) = &mut fields(&mut changed)[2] else {
        unreachable!()
    };
    *rows = rows[..3].to_vec().into_boxed_slice();
    changes.push(changed);
    for changed in changes {
        assert!(verify_completion_bytes(&model, &changed.canonical_bytes().unwrap()).is_err());
    }
}

#[test]
fn portable_import_cannot_choose_its_profile_problem_or_progress_rules() {
    let model = problem();
    let original = find_completion(&model).unwrap();
    let mut wrong_profile = plan_value(&original);
    fields(&mut wrong_profile)[0] = Value::Text("zeno-fcis/completion-plan/0".into());
    assert!(matches!(
        verify_completion_bytes(&model, &wrong_profile.canonical_bytes().unwrap()),
        Err(CompletionError::InvalidPlan {
            reason: "plan-profile",
            ..
        })
    ));
    let (step, terminal) = capacity_model(true);
    let different_problem = CompletionProblem::try_new(
        step,
        terminal,
        CompletionLimits {
            max_command_bytes: 100,
            ..CompletionLimits::default()
        },
    )
    .unwrap();
    assert!(matches!(
        verify_completion_bytes(
            &different_problem,
            &plan_value(&original).canonical_bytes().unwrap()
        ),
        Err(CompletionError::InvalidPlan {
            reason: "problem-binding",
            ..
        })
    ));
    let mut changes = Vec::new();
    let mut changed = original.clone();
    changed.problem_hash = Hash32::new([7; 32]);
    changes.push(changed);
    for (index, rank, command) in [
        (0, 1, vec![]),
        (0, 0, vec![0]),
        (1, 0, vec![1]),
        (1, 1, vec![2]),
        (2, 1, vec![1]),
        (3, 3, vec![0]),
    ] {
        let mut changed = original.clone();
        changed.steps[index] = CompletionStep {
            remaining: rank,
            command,
        };
        changes.push(changed);
    }
    for changed in changes {
        assert!(verify_completion(&model, &changed).is_err());
        assert!(
            verify_completion_bytes(&model, &plan_value(&changed).canonical_bytes().unwrap())
                .is_err()
        );
    }
}

#[test]
fn portable_import_bounds_bytes_nodes_depth_collections_and_payloads() {
    let model = problem();
    let original = find_completion(&model).unwrap();
    let mut widest = original;
    widest.steps[0].command = vec![0];
    assert_eq!(
        model.plan_byte_limit(),
        u64::try_from(plan_value(&widest).canonical_bytes().unwrap().len()).unwrap()
    );
    let oversized = vec![zcve::TAG_UNIT; usize::try_from(model.plan_byte_limit() + 1).unwrap()];
    assert!(matches!(
        verify_completion_bytes(&model, &oversized),
        Err(CompletionError::Decoding(DecodeError::InputLimit { limit, .. }))
            if limit == model.plan_byte_limit()
    ));
    for count in [5u32, u32::MAX] {
        let mut collection = vec![zcve::TAG_TUPLE];
        collection.extend_from_slice(&count.to_be_bytes());
        assert!(matches!(
            verify_completion_bytes(&model, &collection),
            Err(CompletionError::Decoding(DecodeError::CollectionLimit { limit: 4, attempted }))
                if attempted == count
        ));
    }
    let mut deep = Value::Unit;
    for _ in 0..5 {
        deep = tuple(vec![deep]);
    }
    assert!(matches!(
        verify_completion_bytes(&model, &deep.canonical_bytes().unwrap()),
        Err(CompletionError::Decoding(DecodeError::DepthLimit {
            limit: 4,
            attempted: 5
        }))
    ));
    let too_many_nodes = tuple((0..4).map(|_| tuple(vec![Value::Unit; 4])).collect());
    assert!(matches!(
        verify_completion_bytes(&model, &too_many_nodes.canonical_bytes().unwrap()),
        Err(CompletionError::Decoding(DecodeError::NodeLimit {
            limit: 20,
            attempted: 21
        }))
    ));
    let mut blob = vec![zcve::TAG_BYTES];
    blob.extend_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        verify_completion_bytes(&model, &blob),
        Err(CompletionError::Decoding(DecodeError::BlobLimit { .. }))
    ));
    let payloads = tuple(vec![Value::Bytes(vec![0; 32].into_boxed_slice()); 2]);
    assert!(matches!(
        verify_completion_bytes(&model, &payloads.canonical_bytes().unwrap()),
        Err(CompletionError::Decoding(DecodeError::PayloadLimit { .. }))
    ));
}

#[test]
fn multiple_signed_and_boolean_fields_keep_lexicographic_state_and_command_order() {
    let signed = Domain::Int { min: -1, max: 1 };
    let step = Program::try_new(
        vec![signed, Domain::Bool, Domain::Bool, Domain::Bool],
        vec![Domain::Bool, signed, Domain::Bool],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Input(2),
            Op::Int(-1),
            Op::Int(1),
            Op::Lt(3, 0),
            Op::Not(2),
            Op::And(5, 6),
            Op::And(2, 1),
            Op::Not(7),
            Op::Not(8),
            Op::And(9, 10),
            Op::Not(11),
            Op::Sub(0, 4),
            Op::Select(7, 13, 0),
            Op::Bool(false),
            Op::Select(8, 15, 1),
        ],
        vec![12, 14, 16],
    )
    .unwrap();
    let terminal = Program::try_new(
        vec![signed, Domain::Bool],
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Int(-1),
            Op::Eq(0, 2),
            Op::Not(1),
            Op::And(3, 4),
        ],
        vec![5],
    )
    .unwrap();
    let model = CompletionProblem::try_new(step, terminal, CompletionLimits::default()).unwrap();
    assert_eq!(model.state_count(), 6);
    assert_eq!(model.command_count(), 4);
    let proposed = find_completion(&model).unwrap();
    let checked =
        verify_completion_bytes(&model, &plan_value(&proposed).canonical_bytes().unwrap()).unwrap();
    let mut index = 0;
    for signed in -1..=1 {
        for boolean in 0..=1 {
            assert_eq!(
                checked.plan().steps[index].remaining,
                u32::try_from(signed + 1 + boolean).unwrap()
            );
            let expected = if signed > -1 {
                Some([0, 0].as_slice())
            } else if boolean == 1 {
                Some([1, 0].as_slice())
            } else {
                None
            };
            assert_eq!(checked.next_command(&[signed, boolean]).unwrap(), expected);
            index += 1;
        }
    }
    for state in [
        vec![-2, 0],
        vec![2, 0],
        vec![-1, -1],
        vec![-1, 2],
        vec![-1],
        vec![-1, 0, 0],
    ] {
        assert!(checked.next_command(&state).is_err());
    }
    let mut invalid = proposed;
    invalid.steps[1].command = vec![1, 2];
    assert!(
        verify_completion_bytes(&model, &plan_value(&invalid).canonical_bytes().unwrap()).is_err()
    );
}

#[test]
fn portable_plans_cover_signed_endpoints_and_zero_command_fields() {
    let low = Domain::Int {
        min: i64::MIN,
        max: i64::MIN + 1,
    };
    let high = Domain::Int {
        min: i64::MAX - 1,
        max: i64::MAX,
    };
    for command in [
        None,
        Some(Domain::Int {
            min: i64::MIN,
            max: i64::MIN,
        }),
    ] {
        let mut inputs = vec![low, high];
        inputs.extend(command);
        let step = Program::try_new(
            inputs,
            vec![Domain::Bool, low, high],
            vec![Op::Bool(true), Op::Int(i64::MIN), Op::Int(i64::MAX - 1)],
            vec![0, 1, 2],
        )
        .unwrap();
        let terminal = Program::try_new(
            vec![low, high],
            vec![Domain::Bool],
            vec![
                Op::Input(0),
                Op::Input(1),
                Op::Int(i64::MIN),
                Op::Int(i64::MAX - 1),
                Op::Eq(0, 2),
                Op::Eq(1, 3),
                Op::And(4, 5),
            ],
            vec![6],
        )
        .unwrap();
        let model =
            CompletionProblem::try_new(step, terminal, CompletionLimits::default()).unwrap();
        let proposed = find_completion(&model).unwrap();
        let checked =
            verify_completion_bytes(&model, &plan_value(&proposed).canonical_bytes().unwrap())
                .unwrap();
        assert_eq!(checked.plan(), &proposed);
        assert_eq!(
            checked.next_command(&[i64::MIN, i64::MAX - 1]).unwrap(),
            None
        );
        let expected = if command.is_some() {
            vec![i64::MIN]
        } else {
            vec![]
        };
        for state in [
            [i64::MIN, i64::MAX],
            [i64::MIN + 1, i64::MAX - 1],
            [i64::MIN + 1, i64::MAX],
        ] {
            assert_eq!(
                checked.next_command(&state).unwrap(),
                Some(expected.as_slice())
            );
        }
    }
}

fn push_op(nodes: &mut Vec<Op>, op: Op) -> u16 {
    let index = u16::try_from(nodes.len()).unwrap();
    nodes.push(op);
    index
}

fn graph_problem(edges: &[[Option<usize>; 2]], terminals: &[bool]) -> CompletionProblem {
    let state = Domain::Int {
        min: 0,
        max: i64::try_from(edges.len() - 1).unwrap(),
    };
    let mut nodes = vec![Op::Input(0), Op::Input(1), Op::Bool(false)];
    let mut accepted = 2;
    let mut successor = 0;
    for (source, commands) in edges.iter().enumerate() {
        let number = push_op(&mut nodes, Op::Int(i64::try_from(source).unwrap()));
        let matches_state = push_op(&mut nodes, Op::Eq(0, number));
        for (command, target) in commands.iter().enumerate() {
            let matches_command = if command == 0 {
                push_op(&mut nodes, Op::Not(1))
            } else {
                1
            };
            let matches = push_op(&mut nodes, Op::And(matches_state, matches_command));
            let enabled = push_op(&mut nodes, Op::Bool(target.is_some()));
            let next = push_op(
                &mut nodes,
                Op::Int(i64::try_from(target.unwrap_or(source)).unwrap()),
            );
            accepted = push_op(&mut nodes, Op::Select(matches, enabled, accepted));
            successor = push_op(&mut nodes, Op::Select(matches, next, successor));
        }
    }
    let step = Program::try_new(
        vec![state, Domain::Bool],
        vec![Domain::Bool, state],
        nodes,
        vec![accepted, successor],
    )
    .unwrap();
    let mut nodes = vec![Op::Input(0), Op::Bool(false)];
    let mut result = 1;
    for (index, terminal) in terminals.iter().enumerate() {
        let number = push_op(&mut nodes, Op::Int(i64::try_from(index).unwrap()));
        let matches = push_op(&mut nodes, Op::Eq(0, number));
        let terminal = push_op(&mut nodes, Op::Bool(*terminal));
        result = push_op(&mut nodes, Op::Select(matches, terminal, result));
    }
    let terminal = Program::try_new(vec![state], vec![Domain::Bool], nodes, vec![result]).unwrap();
    CompletionProblem::try_new(step, terminal, CompletionLimits::default()).unwrap()
}

#[test]
fn larger_branching_graphs_match_independent_all_pairs_shortest_paths() {
    const STATES: usize = 8;
    let mut complete = 0;
    let mut dead_ends = 0;
    for seed in 0..32u64 {
        let mut random = seed + 1;
        let mut edges = [[None; 2]; STATES];
        let mut terminals = [false; STATES];
        for (index, commands) in edges.iter_mut().enumerate() {
            for target in commands {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let choice = usize::try_from(random >> 32).unwrap() % (STATES + 1);
                *target = (choice < STATES).then_some(choice);
            }
            terminals[index] = random & 4 != 0;
        }
        match seed {
            0 => {
                edges.fill([None; 2]);
                terminals.fill(false);
            }
            1 => terminals.fill(true),
            2 => {
                terminals.fill(false);
                terminals[STATES - 1] = true;
                for (source, commands) in edges.iter_mut().enumerate() {
                    *commands = [
                        Some((source + 1).min(STATES - 1)),
                        Some(source.saturating_sub(1)),
                    ];
                }
            }
            3 => {
                terminals.fill(false);
                terminals[0] = true;
                for (source, commands) in edges.iter_mut().enumerate() {
                    *commands = [Some((source + 1) % STATES), Some(source)];
                }
            }
            _ => {}
        }
        // Floyd-Warshall examines all intermediate vertices; it neither calls
        // the finite interpreter nor copies the production reverse-BFS search.
        let unreachable = STATES + 1;
        let mut distances = [[unreachable; STATES]; STATES];
        for (source, commands) in edges.iter().enumerate() {
            distances[source][source] = 0;
            for target in commands.iter().flatten() {
                distances[source][*target] = distances[source][*target].min(1);
            }
        }
        for via in 0..STATES {
            for source in 0..STATES {
                for target in 0..STATES {
                    distances[source][target] = distances[source][target]
                        .min(distances[source][via] + distances[via][target]);
                }
            }
        }
        let ranks: Vec<_> = distances
            .iter()
            .map(|row| {
                row.iter()
                    .zip(terminals)
                    .filter_map(|(distance, terminal)| terminal.then_some(*distance))
                    .min()
                    .filter(|distance| *distance < unreachable)
            })
            .collect();
        let model = graph_problem(&edges, &terminals);
        if let Some(first) = ranks.iter().position(Option::is_none) {
            assert_eq!(
                find_completion(&model).unwrap_err(),
                CompletionError::NoExit {
                    state: vec![i64::try_from(first).unwrap()]
                }
            );
            dead_ends += 1;
            continue;
        }
        let proposed = find_completion(&model).unwrap();
        let checked =
            verify_completion_bytes(&model, &plan_value(&proposed).canonical_bytes().unwrap())
                .unwrap();
        for (source, rank) in ranks.iter().enumerate() {
            let rank = rank.unwrap();
            assert_eq!(
                checked.plan().steps[source].remaining,
                u32::try_from(rank).unwrap()
            );
            let selected = if rank == 0 {
                None
            } else {
                edges[source]
                    .iter()
                    .position(|target| target.is_some_and(|target| ranks[target] == Some(rank - 1)))
                    .map(|command| vec![i64::try_from(command).unwrap()])
            };
            assert_eq!(
                checked
                    .next_command(&[i64::try_from(source).unwrap()])
                    .unwrap(),
                selected.as_deref()
            );
        }
        complete += 1;
    }
    assert!(complete > 0);
    assert!(dead_ends > 0);
    assert_eq!(complete + dead_ends, 32);
}
