//! Ordered-fold equivalence, resource reservation, and recovery counterexamples.
#![allow(clippy::unwrap_used)]

use super::super::super::evaluation::{Domain, Op};
use super::super::{Failure as Error, Limits as BudgetLimits, Resource, zero_limits};
use super::{
    Capacity, Context as PreparationContext, Failure as PreparationError, Graph as Program,
    PreparationLimits, PreparedFold, default_limits,
};
use alloc::{vec, vec::Vec};

fn context() -> PreparationContext {
    PreparationContext {
        state_root: [1; 32],
        state_version: 0,
        invocation_hash: [2; 32],
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
        Resource::Step,
    ]
    .into_iter()
    .fold(zero_limits(), |budget, resource| {
        budget.with_limit(resource, u64::MAX)
    })
}
fn sum_program() -> Program {
    let scalar = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    super::admit_graph(
        vec![scalar, scalar],
        vec![scalar],
        vec![Op::Input(0), Op::Input(1), Op::Add(0, 1)],
        vec![2],
    )
    .unwrap()
}
fn start(initial: i64, items: &[i64]) -> PreparedFold {
    super::start(
        sum_program(),
        vec![initial],
        items.iter().map(|v| vec![*v]).collect(),
        context(),
        default_limits(),
        budget(),
    )
    .unwrap()
}
fn wire(values: &[i64]) -> Vec<u8> {
    let mut out = vec![0x08];
    out.extend_from_slice(&(values.len() as u32).to_be_bytes());
    for v in values {
        out.push(0x04);
        out.extend_from_slice(&i128::from(*v).to_be_bytes());
    }
    out
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
                    assert!(prepared.same_operation(&reference));
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
    let identity = start(i64::MAX - 1, &[1, 1]);
    assert_eq!(
        prepared.advance(0, 2),
        Err(PreparationError::Evaluation {
            item: 1,
            source: Error::Arithmetic
        })
    );
    assert_eq!(prepared.processed_items(), 0);
    assert!(prepared.same_operation(&identity));
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
            state_root: [3; 32],
            ..context()
        },
        PreparationContext {
            state_version: 1,
            ..context()
        },
        PreparationContext {
            invocation_hash: [4; 32],
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
        let operation = start(1, &[2, 3, 4]);
        drop(abandoned);
        let mut restarted = start(1, &[2, 3, 4]);
        assert!(restarted.same_operation(&operation));
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
        let result = super::start(
            sum_program(),
            vec![0],
            vec![vec![1]],
            context(),
            PreparationLimits {
                max_output_bytes: limit,
                ..default_limits()
            },
            budget(),
        );
        assert_eq!(result.is_ok(), accepts);
        if !accepts {
            assert_eq!(
                result.err().unwrap(),
                PreparationError::Capacity {
                    resource: Capacity::OutputBytes,
                    required,
                    declared: limit
                }
            );
        }
    }
}

#[test]
fn the_reserved_input_bytes_include_both_tuple_frames_and_every_item() {
    let required = (wire(&[0]).len() + 2 * wire(&[]).len() + 2 * wire(&[1]).len()) as u64;
    for (limit, accepts) in [(required - 1, false), (required, true)] {
        let result = super::start(
            sum_program(),
            vec![0],
            vec![vec![1], vec![2]],
            context(),
            PreparationLimits {
                max_input_bytes: limit,
                ..default_limits()
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
fn capacity_diagnostics_report_the_complete_input_requirement() {
    let required =
        u64::try_from(wire(&[0]).len() + 2 * wire(&[]).len() + 3 * wire(&[1]).len()).unwrap();
    for limit in 0..required {
        let error = super::start(
            sum_program(),
            vec![0],
            vec![vec![1], vec![2], vec![3]],
            context(),
            PreparationLimits {
                max_input_bytes: limit,
                ..default_limits()
            },
            budget(),
        )
        .err()
        .unwrap();
        assert_eq!(
            error,
            PreparationError::Capacity {
                resource: Capacity::InputBytes,
                required,
                declared: limit,
            }
        );
    }
    assert!(
        super::start(
            sum_program(),
            vec![0],
            vec![vec![1], vec![2], vec![3]],
            context(),
            PreparationLimits {
                max_input_bytes: required,
                ..default_limits()
            },
            budget(),
        )
        .is_ok()
    );
}

#[test]
fn input_capacity_and_invalid_items_keep_the_first_error_precedence() {
    let frame = u64::try_from(wire(&[]).len()).unwrap();
    let scalar = u64::try_from(wire(&[0]).len()).unwrap();
    let initial = scalar + 2 * frame;
    let required = initial + 3 * scalar;
    for (items, limit, invalid) in [
        (vec![vec![], vec![2], vec![3]], initial - 1, None),
        (vec![vec![], vec![2], vec![3]], initial, Some(0)),
        (vec![vec![1], vec![], vec![3]], initial, None),
        (vec![vec![1], vec![], vec![3]], initial + scalar, Some(1)),
    ] {
        let error = super::start(
            sum_program(),
            vec![0],
            items,
            context(),
            PreparationLimits {
                max_input_bytes: limit,
                ..default_limits()
            },
            budget(),
        )
        .err()
        .unwrap();
        assert_eq!(
            error,
            invalid.map_or(
                PreparationError::Capacity {
                    resource: Capacity::InputBytes,
                    required,
                    declared: limit,
                },
                |item| PreparationError::InvalidItem { item }
            )
        );
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
            let result = super::start(
                sum_program(),
                vec![0],
                vec![vec![1], vec![2]],
                context(),
                default_limits(),
                budget().with_limit(resource, limit),
            );
            assert_eq!(result.is_ok(), accepts);
            if !accepts {
                assert!(
                    matches!(result.err().unwrap(), PreparationError::Budget(error) if error.resource == resource)
                );
            }
        }
    }
}

#[test]
fn changed_inputs_or_context_produce_different_operation_identities() {
    let original = start(0, &[1, 2]);
    for changed in [start(1, &[1, 2]), start(0, &[2, 1]), start(0, &[1])] {
        assert!(!original.same_operation(&changed));
    }
    let changed = super::start(
        sum_program(),
        vec![0],
        vec![vec![1], vec![2]],
        PreparationContext {
            state_version: 1,
            ..context()
        },
        default_limits(),
        budget(),
    )
    .unwrap();
    assert!(!original.same_operation(&changed));
}

#[test]
fn wrong_domains_missing_context_and_invalid_limits_fail_before_preparation() {
    assert!(
        super::start(
            sum_program(),
            vec![],
            vec![],
            context(),
            default_limits(),
            budget()
        )
        .is_err()
    );
    assert!(
        super::start(
            sum_program(),
            vec![0],
            vec![vec![]],
            context(),
            default_limits(),
            budget()
        )
        .is_err()
    );
    assert!(
        super::start(
            sum_program(),
            vec![0],
            vec![],
            PreparationContext {
                state_root: [0; 32],
                ..context()
            },
            default_limits(),
            budget()
        )
        .is_err()
    );
    assert!(
        super::start(
            sum_program(),
            vec![0],
            vec![],
            context(),
            PreparationLimits {
                max_chunk_items: 0,
                ..default_limits()
            },
            budget()
        )
        .is_err()
    );
    assert!(
        super::start(
            sum_program(),
            vec![0],
            vec![vec![1]],
            context(),
            PreparationLimits {
                max_items: 0,
                ..default_limits()
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
    let program = super::admit_graph(
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
    let mut prepared = super::start(
        program,
        vec![0],
        vec![vec![0], vec![1]],
        context(),
        default_limits(),
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
fn unfinished_results_and_failed_contexts_never_expose_a_prefix() {
    let mut prepared = start(100, &[2, 3]);
    prepared.advance(0, 1).unwrap();
    assert_eq!(
        prepared.finish(context()),
        Err(PreparationError::Incomplete)
    );
    let stale = PreparationContext {
        state_version: 1,
        ..context()
    };
    assert_eq!(prepared.finish(stale), Err(PreparationError::Incomplete));
}

#[test]
fn an_order_dependent_fold_keeps_item_order_across_every_partition() {
    let scalar = Domain::Int {
        min: -100,
        max: 100,
    };
    for partition in partitions(3) {
        let program = super::admit_graph(
            vec![scalar, scalar],
            vec![scalar],
            vec![Op::Input(0), Op::Input(1), Op::Add(0, 0), Op::Add(2, 1)],
            vec![3],
        )
        .unwrap();
        let mut prepared = super::start(
            program,
            vec![0],
            vec![vec![1], vec![2], vec![3]],
            context(),
            default_limits(),
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
    let error = super::start(
        sum_program(),
        vec![0],
        vec![vec![1], vec![]],
        context(),
        default_limits(),
        budget(),
    )
    .err()
    .unwrap();
    assert_eq!(error, PreparationError::InvalidItem { item: 1 });
}
#[test]
fn original_complete_bounded_example_uses_the_normal_checked_cursor() {
    let accumulator = Domain::Int {
        min: -100,
        max: 100,
    };
    let item = Domain::Int { min: -3, max: 3 };
    let graph = super::admit_graph(
        vec![accumulator, item],
        vec![accumulator],
        vec![Op::Input(0), Op::Input(1), Op::Add(0, 1)],
        vec![2],
    )
    .unwrap();
    let limits = zero_limits()
        .with_limit(Resource::Read, 6)
        .with_limit(Resource::Write, 3)
        .with_limit(Resource::Candidate, 3)
        .with_limit(Resource::Byte, 1000)
        .with_limit(Resource::Step, 9);
    let mut fold = super::start(
        graph,
        vec![0],
        vec![vec![1], vec![2], vec![3]],
        context(),
        default_limits(),
        limits,
    )
    .unwrap();
    assert_eq!(fold.reserved_budget().used(Resource::Byte), 120);
    assert_eq!(fold.reserved_budget().used(Resource::Step), 0);
    fold.advance(0, 2).unwrap();
    assert_eq!(fold.usage().used(Resource::Step), 6);
    assert_eq!(fold.finish(context()), Err(PreparationError::Incomplete));
    fold.advance(2, 1).unwrap();
    assert_eq!(fold.usage().used(Resource::Step), 9);
    assert_eq!(fold.finish(context()), Ok(vec![6]));
}
#[test]
fn failed_chunks_and_retries_carry_the_actual_step_meter() {
    let mut fold = super::start(
        sum_program(),
        vec![i64::MAX - 1],
        vec![vec![1], vec![1]],
        context(),
        default_limits(),
        budget().with_limit(Resource::Step, 8),
    )
    .unwrap();
    assert_eq!(
        fold.advance(0, 2),
        Err(PreparationError::Evaluation {
            item: 1,
            source: Error::Arithmetic
        })
    );
    assert_eq!(fold.processed_items(), 0);
    assert_eq!(fold.usage().used(Resource::Step), 6);
    let fresh = super::start(
        sum_program(),
        vec![i64::MAX - 1],
        vec![vec![1], vec![1]],
        context(),
        default_limits(),
        budget().with_limit(Resource::Step, 8),
    )
    .unwrap();
    assert!(fold.same_operation(&fresh));
    assert_eq!(
        fold.advance(0, 1),
        Err(PreparationError::Evaluation {
            item: 0,
            source: Error::Budget(super::super::MeterFailure {
                resource: Resource::Step,
                limit: 8,
                attempted: 9,
                overflow: false
            })
        })
    );
    assert_eq!(fold.processed_items(), 0);
    assert_eq!(fold.usage().used(Resource::Step), 8);
    assert_eq!(fold.finish(context()), Err(PreparationError::Incomplete));
    assert_eq!(
        fold.advance(0, 1),
        Err(PreparationError::Evaluation {
            item: 0,
            source: Error::Budget(super::super::MeterFailure {
                resource: Resource::Step,
                limit: 8,
                attempted: 9,
                overflow: false
            })
        })
    );
    assert_eq!(fold.usage().used(Resource::Step), 8);
    for (offset, count) in [(1, 1), (0, 0), (0, 3), (0, u32::MAX)] {
        assert!(fold.advance(offset, count).is_err());
        assert_eq!(fold.usage().used(Resource::Step), 8);
    }
    for r in [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Byte,
    ] {
        assert_eq!(fold.usage().used(r), fold.reserved_budget().used(r));
    }
}
#[test]
fn identity_compares_all_eight_limits_and_all_context_bytes() {
    let original = start(0, &[1, 2]);
    for r in [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ] {
        let changed = super::start(
            sum_program(),
            vec![0],
            vec![vec![1], vec![2]],
            context(),
            default_limits(),
            budget().with_limit(r, u64::MAX - 1),
        )
        .unwrap();
        assert!(!original.same_operation(&changed));
    }
    for index in 0..32 {
        for root in [true, false] {
            let mut c = context();
            if root {
                c.state_root[index] ^= 1;
            } else {
                c.invocation_hash[index] ^= 1;
            }
            let changed = super::start(
                sum_program(),
                vec![0],
                vec![vec![1], vec![2]],
                c,
                default_limits(),
                budget(),
            )
            .unwrap();
            assert!(!original.same_operation(&changed));
        }
    }
}
#[test]
fn graph_admission_keeps_original_shape_and_left_operand_precedence() {
    use super::GraphFailure as G;
    let scalar = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    let cases = [
        (
            vec![scalar; 33],
            vec![scalar],
            vec![Op::Input(99)],
            vec![0],
            G::Shape,
        ),
        (vec![scalar], vec![], vec![Op::Input(99)], vec![], G::Shape),
        (vec![scalar], vec![scalar], vec![], vec![0], G::Shape),
        (
            vec![Domain::Int { min: 1, max: 0 }],
            vec![scalar],
            vec![Op::Input(99)],
            vec![0],
            G::Shape,
        ),
        (
            vec![scalar],
            vec![scalar],
            vec![Op::Input(1)],
            vec![0],
            G::InputReference,
        ),
        (
            vec![scalar],
            vec![scalar],
            vec![Op::Add(0, 0)],
            vec![0],
            G::NodeReference,
        ),
        (
            vec![Domain::Bool],
            vec![scalar],
            vec![Op::Input(0), Op::Add(0, 99)],
            vec![1],
            G::TypeMismatch,
        ),
        (
            vec![scalar],
            vec![scalar],
            vec![Op::Input(0), Op::Add(0, 99)],
            vec![1],
            G::NodeReference,
        ),
        (
            vec![scalar],
            vec![Domain::Bool],
            vec![Op::Input(0)],
            vec![0],
            G::OutputType,
        ),
    ];
    for (inputs, outputs, nodes, roots, e) in cases {
        assert_eq!(
            super::admit_graph(inputs, outputs, nodes, roots).err(),
            Some(e)
        );
    }
}
