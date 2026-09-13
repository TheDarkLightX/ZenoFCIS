//! Ordered-fold equivalence, resource reservation, and recovery counterexamples.
#![allow(clippy::unwrap_used)]

use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_core::{BudgetLimits, Resource};
use zeno_fcis_synthesis::finite::preparation::{
    PreparationContext, PreparationError, PreparationLimits, PreparedFold,
};
use zeno_fcis_synthesis::finite::{Domain, Error, Op, Program};
use zeno_fcis_value::Value;

fn context() -> PreparationContext {
    PreparationContext {
        state_root: Hash32::new([1; 32]),
        state_version: 0,
        invocation_hash: Hash32::new([2; 32]),
    }
}
fn budget() -> BudgetLimits {
    [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
    ]
    .into_iter()
    .fold(BudgetLimits::zero(), |budget, resource| {
        budget.with_limit(resource, u64::MAX)
    })
}
fn sum_program() -> Program {
    let scalar = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    Program::try_new(
        vec![scalar, scalar],
        vec![scalar],
        vec![Op::Input(0), Op::Input(1), Op::Add(0, 1)],
        vec![2],
    )
    .unwrap()
}
fn start(initial: i64, items: &[i64]) -> PreparedFold {
    PreparedFold::start(
        sum_program(),
        vec![initial],
        items.iter().map(|v| vec![*v]).collect(),
        context(),
        PreparationLimits::default(),
        budget(),
    )
    .unwrap()
}
fn wire(values: &[i64]) -> Vec<u8> {
    Value::Tuple(
        values
            .iter()
            .map(|v| Value::I128(i128::from(*v)))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
    .canonical_bytes()
    .unwrap()
}
fn partitions(n: u32) -> Vec<Vec<u32>> {
    if n == 0 {
        return vec![vec![]];
    }
    let mut result = Vec::new();
    for width in 1..=n.min(3) {
        for mut rest in partitions(n - width) {
            rest.insert(0, width);
            result.push(rest);
        }
    }
    result
}

#[test]
fn all_small_partitions_match_the_whole_operation_and_preserve_identity() {
    for length in 0..=3u32 {
        for encoded in 0..3u32.pow(length) {
            let mut cursor = encoded;
            let mut items = Vec::new();
            for _ in 0..length {
                items.push(i64::from(cursor % 3) - 1);
                cursor /= 3;
            }
            for initial in -1..=1 {
                let expected = initial + items.iter().sum::<i64>();
                let reference = start(initial, &items);
                for partition in partitions(length) {
                    let mut prepared = start(initial, &items);
                    for count in partition {
                        let offset = prepared.processed_items();
                        assert_eq!(
                            prepared.finish(context()),
                            Err(PreparationError::Incomplete)
                        );
                        prepared.advance(offset, count).unwrap();
                        assert_eq!(prepared.processed_items(), offset + count);
                        assert_eq!(prepared.remaining_items(), length - offset - count);
                    }
                    assert_eq!(prepared.operation_hash(), reference.operation_hash());
                    assert_eq!(prepared.reserved_budget(), reference.reserved_budget());
                    assert_eq!(prepared.finish(context()).unwrap(), [expected]);
                    assert_eq!(
                        wire(&prepared.finish(context()).unwrap()),
                        wire(&[expected])
                    );
                }
            }
        }
    }
}

#[test]
fn a_failure_after_partial_calculation_does_not_advance_the_chunk() {
    let mut prepared = start(i64::MAX - 1, &[1, 1]);
    let identity = prepared.operation_hash();
    assert_eq!(
        prepared.advance(0, 2),
        Err(PreparationError::Evaluation {
            item: 1,
            source: Error::Arithmetic
        })
    );
    assert_eq!(prepared.processed_items(), 0);
    assert_eq!(prepared.operation_hash(), identity);
    prepared.advance(0, 1).unwrap();
    assert_eq!(
        prepared.advance(1, 1),
        Err(PreparationError::Evaluation {
            item: 1,
            source: Error::Arithmetic
        })
    );
    assert_eq!(prepared.processed_items(), 1);
    assert_eq!(
        prepared.finish(context()),
        Err(PreparationError::Incomplete)
    );
}

#[test]
fn repeated_skipped_empty_and_oversized_ranges_preserve_progress() {
    let mut prepared = start(0, &[1, 2]);
    assert!(matches!(
        prepared.advance(1, 1),
        Err(PreparationError::WrongOffset { .. })
    ));
    for count in [0, 3, u32::MAX] {
        assert_eq!(
            prepared.advance(0, count),
            Err(PreparationError::InvalidChunk)
        );
    }
    assert_eq!(prepared.processed_items(), 0);
    prepared.advance(0, 1).unwrap();
    assert!(matches!(
        prepared.advance(0, 1),
        Err(PreparationError::WrongOffset { .. })
    ));
    assert_eq!(prepared.processed_items(), 1);
    prepared.advance(1, 1).unwrap();
    assert_eq!(prepared.finish(context()).unwrap(), [3]);
}

#[test]
fn full_state_version_and_invocation_must_still_match_at_finish() {
    let mut prepared = start(0, &[1]);
    prepared.advance(0, 1).unwrap();
    for current in [
        PreparationContext {
            state_root: Hash32::new([3; 32]),
            ..context()
        },
        PreparationContext {
            state_version: 1,
            ..context()
        },
        PreparationContext {
            invocation_hash: Hash32::new([4; 32]),
            ..context()
        },
    ] {
        assert_eq!(
            prepared.finish(current),
            Err(PreparationError::StaleContext)
        );
    }
    let mut output = prepared.finish(context()).unwrap();
    output[0] = 99;
    assert_eq!(prepared.finish(context()).unwrap(), [1]);
}

#[test]
fn cancellation_and_replay_at_every_prefix_match_uninterrupted_execution() {
    for prefix in 0..=3 {
        let mut abandoned = start(1, &[2, 3, 4]);
        if prefix > 0 {
            abandoned.advance(0, prefix).unwrap();
        }
        let operation = abandoned.operation_hash();
        drop(abandoned);
        let mut restarted = start(1, &[2, 3, 4]);
        assert_eq!(restarted.operation_hash(), operation);
        // Recovery trusts original inputs and executes their prefix again.
        for offset in 0..3 {
            restarted.advance(offset, 1).unwrap();
        }
        assert_eq!(restarted.finish(context()).unwrap(), [10]);
    }
}

#[test]
fn output_capacity_is_reserved_before_any_progress() {
    let required = u64::try_from(wire(&[0]).len()).unwrap();
    for (limit, accepts) in [(required - 1, false), (required, true)] {
        let result = PreparedFold::start(
            sum_program(),
            vec![0],
            vec![vec![1]],
            context(),
            PreparationLimits {
                max_output_bytes: limit,
                ..PreparationLimits::default()
            },
            budget(),
        );
        assert_eq!(result.is_ok(), accepts);
        if !accepts {
            assert_eq!(
                result.unwrap_err(),
                PreparationError::Capacity {
                    resource: "output-bytes",
                    required,
                    declared: limit
                }
            );
        }
    }
}

#[test]
fn the_reserved_input_bytes_include_both_tuple_frames_and_every_item() {
    let input = Value::Tuple(
        vec![
            Value::Tuple(vec![Value::I128(0)].into_boxed_slice()),
            Value::Tuple(
                vec![
                    Value::Tuple(vec![Value::I128(1)].into_boxed_slice()),
                    Value::Tuple(vec![Value::I128(2)].into_boxed_slice()),
                ]
                .into_boxed_slice(),
            ),
        ]
        .into_boxed_slice(),
    );
    let required = u64::try_from(input.canonical_bytes().unwrap().len()).unwrap();
    for (limit, accepts) in [(required - 1, false), (required, true)] {
        let result = PreparedFold::start(
            sum_program(),
            vec![0],
            vec![vec![1], vec![2]],
            context(),
            PreparationLimits {
                max_input_bytes: limit,
                ..PreparationLimits::default()
            },
            budget(),
        );
        assert_eq!(result.is_ok(), accepts);
        if accepts {
            assert_eq!(
                result.unwrap().reserved_budget().used(Resource::Byte),
                required + u64::try_from(wire(&[0]).len()).unwrap()
            );
        }
    }
}

#[test]
fn every_modeled_resource_must_cover_completion_at_start() {
    let required = start(0, &[1, 2]).reserved_budget();
    for resource in [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Byte,
    ] {
        let amount = required.used(resource);
        for (limit, accepts) in [(amount - 1, false), (amount, true)] {
            let result = PreparedFold::start(
                sum_program(),
                vec![0],
                vec![vec![1], vec![2]],
                context(),
                PreparationLimits::default(),
                budget().with_limit(resource, limit),
            );
            assert_eq!(result.is_ok(), accepts);
            if !accepts {
                assert!(
                    matches!(result.unwrap_err(), PreparationError::Budget(error) if error.resource() == resource)
                );
            }
        }
    }
}

#[test]
fn changed_inputs_or_context_produce_different_operation_identities() {
    let original = start(0, &[1, 2]);
    for changed in [start(1, &[1, 2]), start(0, &[2, 1]), start(0, &[1])] {
        assert_ne!(original.operation_hash(), changed.operation_hash());
    }
    let changed = PreparedFold::start(
        sum_program(),
        vec![0],
        vec![vec![1], vec![2]],
        PreparationContext {
            state_version: 1,
            ..context()
        },
        PreparationLimits::default(),
        budget(),
    )
    .unwrap();
    assert_ne!(original.operation_hash(), changed.operation_hash());
}

#[test]
fn wrong_domains_missing_context_and_invalid_limits_fail_before_preparation() {
    assert!(
        PreparedFold::start(
            sum_program(),
            vec![],
            vec![],
            context(),
            PreparationLimits::default(),
            budget()
        )
        .is_err()
    );
    assert!(
        PreparedFold::start(
            sum_program(),
            vec![0],
            vec![vec![]],
            context(),
            PreparationLimits::default(),
            budget()
        )
        .is_err()
    );
    assert!(
        PreparedFold::start(
            sum_program(),
            vec![0],
            vec![],
            PreparationContext {
                state_root: Hash32::ZERO,
                ..context()
            },
            PreparationLimits::default(),
            budget()
        )
        .is_err()
    );
    assert!(
        PreparedFold::start(
            sum_program(),
            vec![0],
            vec![],
            context(),
            PreparationLimits {
                max_chunk_items: 0,
                ..PreparationLimits::default()
            },
            budget()
        )
        .is_err()
    );
    assert!(
        PreparedFold::start(
            sum_program(),
            vec![0],
            vec![vec![1]],
            context(),
            PreparationLimits {
                max_items: 0,
                ..PreparationLimits::default()
            },
            budget()
        )
        .is_err()
    );
    let prepared = start(7, &[]);
    assert_eq!(prepared.remaining_items(), 0);
    assert_eq!(prepared.finish(context()).unwrap(), [7]);
}

#[test]
fn eager_evaluation_and_absolute_first_error_do_not_change_at_chunk_boundaries() {
    let scalar = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    let program = Program::try_new(
        vec![scalar, scalar],
        vec![scalar],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Int(i64::MAX),
            Op::Add(2, 1),
            Op::Bool(false),
            Op::Select(4, 3, 0),
        ],
        vec![5],
    )
    .unwrap();
    let mut prepared = PreparedFold::start(
        program,
        vec![0],
        vec![vec![0], vec![1]],
        context(),
        PreparationLimits::default(),
        budget(),
    )
    .unwrap();
    assert_eq!(
        prepared.advance(0, 2),
        Err(PreparationError::Evaluation {
            item: 1,
            source: Error::Arithmetic
        })
    );
    assert_eq!(prepared.processed_items(), 0);
}

#[test]
fn debug_output_does_not_expose_the_unfinished_accumulator_or_owned_inputs() {
    let mut prepared = start(100, &[2, 3]);
    prepared.advance(0, 1).unwrap();
    let output = format!("{prepared:?}");
    assert!(
        !output.contains("accumulator"),
        "unfinished result is exposed"
    );
    assert!(!output.contains("program:"), "owned program is exposed");
    assert!(!output.contains("items:"), "owned input is exposed");
}

#[test]
fn an_order_dependent_fold_keeps_item_order_across_every_partition() {
    let scalar = Domain::Int {
        min: -100,
        max: 100,
    };
    let program = Program::try_new(
        vec![scalar, scalar],
        vec![scalar],
        vec![Op::Input(0), Op::Input(1), Op::Add(0, 0), Op::Add(2, 1)],
        vec![3],
    )
    .unwrap();
    for partition in partitions(3) {
        let mut prepared = PreparedFold::start(
            program.clone(),
            vec![0],
            vec![vec![1], vec![2], vec![3]],
            context(),
            PreparationLimits::default(),
            budget(),
        )
        .unwrap();
        for count in partition {
            prepared.advance(prepared.processed_items(), count).unwrap();
        }
        // Independent sequential recurrence: ((0 * 2 + 1) * 2 + 2) * 2 + 3.
        assert_eq!(prepared.finish(context()).unwrap(), [11]);
    }
}

#[test]
fn invalid_item_diagnostics_report_the_position_without_printing_its_contents() {
    let error = PreparedFold::start(
        sum_program(),
        vec![0],
        vec![vec![1], vec![]],
        context(),
        PreparationLimits::default(),
        budget(),
    )
    .unwrap_err();
    assert_eq!(error, PreparationError::InvalidItem { item: 1 });
}
