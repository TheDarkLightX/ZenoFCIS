//! Exact original preparation bodies exercised against both retained and checked APIs.
#![allow(clippy::unwrap_used)]
use crate::oracle::{core as old_core, finite::preparation as old};
use zeno_fcis_synthesis::finite::{self, v2_continuation as checked};
use zeno_fcis_synthesis::{SynthesisError};
use zeno_fcis_codec::{EncodeError,Hash32};
use zeno_fcis_value::ValueError;
use std::fmt;

struct PreparedFold { legacy:old::PreparedFold, checked:checked::PreparedFold }
fn context(c:old::PreparationContext)->checked::Context {
    checked::Context { state_root:*c.state_root.as_bytes(),state_version:c.state_version,
        invocation_hash:*c.invocation_hash.as_bytes() }
}
fn limits(l:old::PreparationLimits)->checked::PreparationLimits {
    checked::PreparationLimits { max_items:l.max_items,max_chunk_items:l.max_chunk_items,
        max_input_bytes:l.max_input_bytes,max_output_bytes:l.max_output_bytes }
}
fn resource(r:old_core::Resource)->finite::V2Resource {
    match r { old_core::Resource::Read=>finite::V2Resource::Read,
        old_core::Resource::Write=>finite::V2Resource::Write,
        old_core::Resource::Candidate=>finite::V2Resource::Candidate,
        old_core::Resource::Effect=>finite::V2Resource::Effect,
        old_core::Resource::Byte=>finite::V2Resource::Byte,
        old_core::Resource::WitnessByte=>finite::V2Resource::WitnessByte,
        old_core::Resource::Depth=>finite::V2Resource::Depth }
}
fn old_resource(r:finite::V2Resource)->old_core::Resource {
    for original in resources() { if resource(original)==r { return original; } }
    panic!("Step has no historical seven-resource counter")
}
fn resources()->[old_core::Resource;7] {
    [old_core::Resource::Read,old_core::Resource::Write,old_core::Resource::Candidate,
        old_core::Resource::Effect,old_core::Resource::Byte,old_core::Resource::WitnessByte,old_core::Resource::Depth]
}
fn budget(l:old_core::BudgetLimits)->finite::V2Limits {
    resources().into_iter().fold(finite::v2_zero_limits(),|b,r|b.with_limit(resource(r),l.limit(r)))
        .with_limit(finite::V2Resource::Step,u64::MAX)
}
fn failure(e:checked::Failure)->old::PreparationError {
    use checked::{Capacity as C,Failure as F,Invalid as I};
    match e {
        F::Graph(e)=>panic!("previously admitted original graph refused: {e:?}"),
        F::Invalid(e)=>old::PreparationError::Invalid(match e { I::Limits=>"limits",I::Context=>"context",I::Accumulator=>"accumulator",_=>panic!("unexpected future invalid preparation kind") }),
        F::Capacity{resource,required,declared}=>old::PreparationError::Capacity {
            resource:match resource { C::Items=>"items",C::Steps=>"steps",C::OutputBytes=>"output-bytes",C::InputBytes=>"input-bytes",_=>panic!("unexpected future preparation capacity") },required,declared },
        F::InvalidItem{item}=>old::PreparationError::InvalidItem{item},
        F::Budget(e)=>{
            assert!(!e.overflow,"original bounded logical reservation never overflows");
            let r=old_resource(e.resource);
            let mut b=old_core::Budget::new(old_core::BudgetLimits::zero().with_limit(r,e.limit));
            old::PreparationError::Budget(b.charge(r,e.attempted).unwrap_err())
        },
        F::EncodingNodes{limit,attempted}=>old::PreparationError::Encoding(SynthesisError::Encode(
            EncodeError::InvalidValue(ValueError::NodeLimit{limit,attempted}))),
        F::WrongOffset{expected,supplied}=>old::PreparationError::WrongOffset{expected,supplied},
        F::InvalidChunk=>old::PreparationError::InvalidChunk,
        F::Evaluation{item,source}=>old::PreparationError::Evaluation { item,source:match source {
            finite::V2ExecutionFailure::InputDomain=>finite::Error::Invalid("input-domain"),
            finite::V2ExecutionFailure::Reference=>finite::Error::Invalid("node-reference"),
            finite::V2ExecutionFailure::Arithmetic=>finite::Error::Arithmetic,
            finite::V2ExecutionFailure::OutputDomain=>finite::Error::Invalid("output-domain"),
            finite::V2ExecutionFailure::Budget(e)=>panic!("unlimited original Step refused: {e:?}"),
            _=>panic!("unexpected future evaluator failure"),
        } },
        F::Incomplete=>old::PreparationError::Incomplete,F::StaleContext=>old::PreparationError::StaleContext,
        _=>panic!("unexpected future preparation failure"),
    }
}
impl fmt::Debug for PreparedFold {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result { self.legacy.fmt(f) }
}
impl PreparedFold {
    fn start(p:finite::Program,a:Vec<i64>,items:Vec<Vec<i64>>,c:old::PreparationContext,
        l:old::PreparationLimits,b:old_core::BudgetLimits)->Result<Self,old::PreparationError> {
        let g=checked::admit_graph(p.inputs().to_vec(),p.outputs().to_vec(),p.nodes().to_vec(),p.roots().to_vec()).unwrap();
        let new=checked::start(g,a.clone(),items.clone(),context(c),limits(l),budget(b));
        let original=old::PreparedFold::start(p,a,items,c,l,b);
        match (original,new) {
            (Ok(legacy),Ok(checked))=>{ let fold=Self{legacy,checked};fold.compare();Ok(fold) },
            (Err(a),Err(b))=>{assert_eq!(a,failure(b));Err(a)},
            (a,b)=>panic!("original and actual checked admission differ: {} / {}",a.is_ok(),b.is_ok()),
        }
    }
    fn compare(&self) {
        assert_eq!(self.legacy.processed_items(),self.checked.processed_items());
        assert_eq!(self.legacy.remaining_items(),self.checked.remaining_items());
        for r in resources() { assert_eq!(self.legacy.reserved_budget().used(r),self.checked.reserved_budget().used(resource(r))); }
    }
    fn advance(&mut self,offset:u32,count:u32)->Result<(),old::PreparationError> {
        let a=self.legacy.advance(offset,count);let b=self.checked.advance(offset,count).map_err(failure);
        assert_eq!(a,b);self.compare();a
    }
    fn finish(&self,c:old::PreparationContext)->Result<Vec<i64>,old::PreparationError> {
        let a=self.legacy.finish(c);let b=self.checked.finish(context(c)).map_err(failure);assert_eq!(a,b);a
    }
    fn processed_items(&self)->u32 { self.compare();self.checked.processed_items() }
    fn remaining_items(&self)->u32 { self.compare();self.checked.remaining_items() }
    fn operation_hash(&self)->Hash32 { self.legacy.operation_hash() }
    fn reserved_budget(&self)->old_core::BudgetUsed { self.compare();self.legacy.reserved_budget() }
}

mod original_regressions {
// Ordered-fold equivalence, resource reservation, and recovery counterexamples.


use super::PreparedFold;
use crate::oracle::core::{BudgetLimits, Resource};
use crate::oracle::finite::preparation::{
    PreparationContext, PreparationError, PreparationLimits,
};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
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
    Value::tuple(
        values
            .iter()
            .map(|v| Value::signed(i128::from(*v)))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|error| panic!("original admitted fixture: {error}"))
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
    let input = Value::tuple(vec![
        Value::tuple(vec![Value::signed(0)])
            .unwrap_or_else(|error| panic!("original admitted fixture: {error}")),
        Value::tuple(vec![
            Value::tuple(vec![Value::signed(1)])
                .unwrap_or_else(|error| panic!("original admitted fixture: {error}")),
            Value::tuple(vec![Value::signed(2)])
                .unwrap_or_else(|error| panic!("original admitted fixture: {error}")),
        ])
        .unwrap_or_else(|error| panic!("original admitted fixture: {error}")),
    ])
    .unwrap_or_else(|error| panic!("original admitted fixture: {error}"));
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
fn capacity_diagnostics_report_the_complete_input_requirement() {
    let required =
        u64::try_from(wire(&[0]).len() + 2 * wire(&[]).len() + 3 * wire(&[1]).len()).unwrap();
    for limit in 0..required {
        let error = PreparedFold::start(
            sum_program(),
            vec![0],
            vec![vec![1], vec![2], vec![3]],
            context(),
            PreparationLimits {
                max_input_bytes: limit,
                ..PreparationLimits::default()
            },
            budget(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            PreparationError::Capacity {
                resource: "input-bytes",
                required,
                declared: limit,
            }
        );
    }
    assert!(
        PreparedFold::start(
            sum_program(),
            vec![0],
            vec![vec![1], vec![2], vec![3]],
            context(),
            PreparationLimits {
                max_input_bytes: required,
                ..PreparationLimits::default()
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
        let error = PreparedFold::start(
            sum_program(),
            vec![0],
            items,
            context(),
            PreparationLimits {
                max_input_bytes: limit,
                ..PreparationLimits::default()
            },
            budget(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            invalid.map_or(
                PreparationError::Capacity {
                    resource: "input-bytes",
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

}

#[test]
fn original_canonical_node_ceiling_and_reservation_precedence_match() {
    let scalar=finite::Domain::Int{min:-1,max:1};
    for count in [62499usize,62500] {
        let p=finite::Program::try_new(vec![scalar;16],vec![scalar],vec![finite::Op::Input(0)],vec![0]).unwrap();
        let c=old::PreparationContext{state_root:Hash32::new([1;32]),state_version:0,invocation_hash:Hash32::new([2;32])};
        let l=old::PreparationLimits{max_items:65536,max_chunk_items:64,max_input_bytes:16777216,max_output_bytes:4096};
        let b=resources().into_iter().fold(old_core::BudgetLimits::zero(),|b,r|b.with_limit(r,u64::MAX));
        let result=PreparedFold::start(p.clone(),vec![0],vec![vec![0;15];count],c,l,b);
        assert_eq!(result.is_ok(),count==62499);
        if count==62500 {
            assert_eq!(result.unwrap_err(),old::PreparationError::Encoding(SynthesisError::Encode(
                EncodeError::InvalidValue(ValueError::NodeLimit{limit:1000000,attempted:1000001}))));
            let result=PreparedFold::start(p,vec![0],vec![vec![0;15];count],c,l,
                b.with_limit(old_core::Resource::Read,0));
            assert!(matches!(result,Err(old::PreparationError::Budget(e)) if e.resource()==old_core::Resource::Read));
        }
    }
}
