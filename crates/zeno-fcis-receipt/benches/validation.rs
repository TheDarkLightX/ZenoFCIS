#![allow(missing_docs)]
//! Measure public validation; use Criterion baselines for released/candidate runs.

use std::{hint::black_box, time::Duration};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use zeno_fcis_codec::{Domain, Hash32};
use zeno_fcis_core::DecisionKind;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_patch::{
    CanonicalPatch, PatchOp, PathSegment, ValuePath, hash_precondition_value, hash_value,
};
use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan};
use zeno_fcis_receipt::{CandidateBindings, CandidateBuilder, CommitBundle};
use zeno_fcis_value::{Field, Value};

fn checked<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
    value.unwrap_or_else(|error| panic!("benchmark construction: {error:?}"))
}

fn domain() -> Domain<'static> {
    checked(Domain::new("bench/validation", 1))
}

fn case(fields: u16, edits: u16, entries: u32) -> (Value, CommitBundle) {
    let state = checked(Value::record_canonical(
        (0..fields)
            .map(|i| Field::new(i, Value::U128(u128::from(i))))
            .collect(),
    ));
    let patch = checked(CanonicalPatch::try_new(
        1,
        checked(hash_value::<RustCryptoSha256>(domain(), &state)),
        (0..edits)
            .map(|i| PatchOp::Update {
                path: ValuePath::new(vec![PathSegment::Field(i)]),
                expected_old_hash: checked(hash_precondition_value::<RustCryptoSha256>(
                    &Value::U128(u128::from(i)),
                )),
                value: Value::U128(u128::from(i) + 1),
            })
            .collect(),
    ));
    let commit = checked(CommitPlan::try_new(
        (0..entries)
            .map(|i| {
                Effect::new(
                    i,
                    7,
                    Hash32::new([7; 32]),
                    Hash32::new([8; 32]),
                    Value::Bytes(vec![42; 64].into()),
                )
            })
            .collect(),
    ));
    let outbox = checked(OutboxPlan::try_new(
        (0..entries)
            .map(|i| OutboxEntry::new(i, 3, Value::U128(4), Value::Bytes(vec![42; 64].into())))
            .collect(),
    ));
    let bundle = checked(CandidateBuilder::seal::<RustCryptoSha256>(
        &state,
        domain(),
        DecisionKind::Accept,
        None,
        CandidateBindings {
            profile_hash: Hash32::new([1; 32]),
            command_hash: Hash32::new([2; 32]),
            context_hash: Hash32::new([3; 32]),
            precedence_hash: Hash32::new([4; 32]),
            algorithm_hash: Hash32::new([5; 32]),
            budget_hash: Hash32::new([6; 32]),
        },
        patch,
        commit,
        outbox,
    ));
    (state, bundle)
}

fn validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("bundle-validation");
    for (name, fields, edits, entries) in [
        ("empty", 0, 0, 0),
        ("counter", 1, 1, 1),
        ("record-64", 64, 8, 4),
        ("record-1024", 1024, 16, 16),
        ("record-4096-one-edit", 4096, 1, 1),
    ] {
        let (state, bundle) = case(fields, edits, entries);
        let expected = checked(bundle.patch().apply::<RustCryptoSha256>(&state, domain()));
        assert_eq!(
            bundle.validate_and_apply::<RustCryptoSha256>(&state, domain()),
            Ok(expected)
        );
        group.bench_with_input(
            BenchmarkId::new("accepted", name),
            &(state, bundle),
            |b, (state, bundle)| {
                b.iter(|| {
                    checked(black_box(bundle).validate_and_apply::<RustCryptoSha256>(
                        black_box(state),
                        black_box(domain()),
                    ))
                });
            },
        );
    }
    let (_, bundle) = case(64, 8, 4);
    group.bench_function("stale-pre-state", |b| {
        b.iter(|| {
            let result = black_box(&bundle)
                .validate_and_apply::<RustCryptoSha256>(black_box(&Value::Unit), domain());
            assert!(result.is_err());
            black_box(result)
        })
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(40).warm_up_time(Duration::from_millis(400)).measurement_time(Duration::from_secs(2));
    targets = validation
}
criterion_main!(benches);
