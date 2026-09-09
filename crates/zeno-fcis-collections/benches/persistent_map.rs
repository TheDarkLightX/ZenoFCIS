#![allow(missing_docs)]
//! Benchmarks for persistent collection backends.
//!
//! Benchmarks small/dense and large/sparse states, lookups, batch updates,
//! canonical iteration/encoding, and retained snapshots. No hash is timed here.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_collections::{BTreeMapBackend, LogicalEntry, PersistentMap};
use zeno_fcis_value::Value;

fn make_entry(key_byte: u16, value_byte: u32) -> LogicalEntry {
    let key = Value::U128(u128::from(key_byte));
    let encoded_key = key
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode key: {e}"));
    LogicalEntry::try_new(encoded_key, key, Value::U128(u128::from(value_byte)))
        .unwrap_or_else(|error| panic!("logical entry: {error}"))
}

fn build_dense(size: u16) -> Vec<LogicalEntry> {
    (1..=size)
        .map(|i| make_entry(i, u32::from(i) * 2))
        .collect()
}

fn build_sparse(size: u16, stride: u16) -> Vec<LogicalEntry> {
    (1..=size)
        .filter(|i| i % stride == 0)
        .map(|i| make_entry(i, u32::from(i) * 3))
        .collect()
}

fn bench_insert_dense<M: PersistentMap>(c: &mut Criterion, name: &str) {
    let mut group = c.benchmark_group("insert_dense");
    for size in [10u16, 50, 100, 200, 1024] {
        let entries = build_dense(size);
        group.bench_with_input(BenchmarkId::new(name, size), &entries, |b, entries| {
            b.iter(|| {
                let mut map = M::empty();
                for entry in entries {
                    map = map.insert(entry.clone());
                }
                black_box(map);
            });
        });
    }
    group.finish();
}

fn bench_lookup<M: PersistentMap>(c: &mut Criterion, name: &str) {
    let mut group = c.benchmark_group("lookup");
    for size in [10u16, 50, 100, 200, 1024] {
        let entries = build_dense(size);
        let map: M = entries.iter().fold(M::empty(), |m, e| m.insert(e.clone()));
        let lookup_key = Value::U128(u128::from(size / 2))
            .canonical_bytes()
            .unwrap_or_else(|e| panic!("encode key: {e}"));
        group.bench_with_input(BenchmarkId::new(name, size), &lookup_key, |b, key| {
            b.iter(|| map.get(black_box(key.as_slice())));
        });
    }
    group.finish();
}

fn bench_canonical_iteration<M: PersistentMap>(c: &mut Criterion, name: &str) {
    let mut group = c.benchmark_group("canonical_iteration");
    for size in [10u16, 50, 100, 200, 1024] {
        let entries = build_dense(size);
        let map: M = entries.iter().fold(M::empty(), |m, e| m.insert(e.clone()));
        group.bench_with_input(BenchmarkId::new(name, size), &map, |b, map| {
            b.iter(|| map.to_entries());
        });
    }
    group.finish();
}

fn bench_canonical_encoding<M: PersistentMap>(c: &mut Criterion, name: &str) {
    let mut group = c.benchmark_group("canonical_encoding");
    for size in [10u16, 50, 100, 200, 1024] {
        let entries = build_dense(size);
        let map: M = entries.iter().fold(M::empty(), |m, e| m.insert(e.clone()));
        group.bench_with_input(BenchmarkId::new(name, size), &map, |b, map| {
            b.iter(|| {
                map.try_canonical_bytes()
                    .unwrap_or_else(|error| panic!("canonical bytes: {error}"))
            });
        });
    }
    group.finish();
}

fn bench_snapshot_retention<M: PersistentMap>(c: &mut Criterion, name: &str) {
    let mut group = c.benchmark_group("snapshot_retention");
    for size in [10u16, 50, 100, 200] {
        let entries = build_dense(size);
        group.bench_with_input(BenchmarkId::new(name, size), &entries, |b, entries| {
            b.iter(|| {
                let mut snapshots = Vec::new();
                let mut map = M::empty();
                for entry in entries {
                    map = map.insert(entry.clone());
                    snapshots.push(map.clone());
                }
                black_box(snapshots);
            });
        });
    }
    group.finish();
}

fn bench_sparse_insert<M: PersistentMap>(c: &mut Criterion, name: &str) {
    let mut group = c.benchmark_group("insert_sparse");
    for size in [50u16, 100, 200, 1024] {
        let entries = build_sparse(size, 5);
        group.bench_with_input(BenchmarkId::new(name, size), &entries, |b, entries| {
            b.iter(|| {
                let mut map = M::empty();
                for entry in entries {
                    map = map.insert(entry.clone());
                }
                black_box(map);
            });
        });
    }
    group.finish();
}

// Validate the identical benchmark fixtures against the allocating reference
// before any timings. In particular, exercise widths beyond the former u8 limit.
fn check_fixtures<M: PersistentMap>() {
    for size in [10_u16, 50, 100, 200, 1024] {
        for (entries, cardinality, multiplier) in [
            (build_dense(size), usize::from(size), 2_u128),
            (build_sparse(size, 5), usize::from(size / 5), 3_u128),
        ] {
            assert_eq!(entries.len(), cardinality);
            for entry in &entries {
                let Value::U128(key) = entry.key() else {
                    panic!("fixture key must be unsigned");
                };
                assert_eq!(entry.value(), &Value::U128(key * multiplier));
            }
            let reference = entries
                .iter()
                .fold(BTreeMapBackend::empty(), |m, e| m.insert(e.clone()));
            let candidate = entries.iter().fold(M::empty(), |m, e| m.insert(e.clone()));
            assert_eq!(candidate.len(), cardinality);
            let expected = reference
                .try_canonical_bytes()
                .unwrap_or_else(|error| panic!("reference encoding: {error}"));
            assert_eq!(candidate.try_canonical_bytes(), Ok(expected.clone()));
            // Exercise a branching update and removal while retaining the input.
            let updated = candidate.insert(make_entry(1, 999));
            let updated_reference = reference.insert(make_entry(1, 999));
            assert_eq!(
                updated.try_canonical_bytes(),
                updated_reference.try_canonical_bytes()
            );
            let key = Value::U128(1)
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("removal key: {error}"));
            assert_eq!(
                updated.remove(&key).try_canonical_bytes(),
                updated_reference.remove(&key).try_canonical_bytes()
            );
            assert_eq!(candidate.try_canonical_bytes(), Ok(expected));
        }
    }
}

fn bench_backend<M: PersistentMap>(c: &mut Criterion, name: &str) {
    check_fixtures::<M>();
    bench_insert_dense::<M>(c, name);
    bench_lookup::<M>(c, name);
    bench_canonical_iteration::<M>(c, name);
    bench_canonical_encoding::<M>(c, name);
    bench_snapshot_retention::<M>(c, name);
    bench_sparse_insert::<M>(c, name);
}

fn bench_maps(c: &mut Criterion) {
    bench_backend::<BTreeMapBackend>(c, "btreemap");
    #[cfg(feature = "rpds-backend")]
    bench_backend::<zeno_fcis_collections::RpdsBackend>(c, "rpds");
    #[cfg(feature = "ordered-map")]
    bench_backend::<zeno_fcis_collections::OrderedMap>(c, "ordered_map");
}

criterion_group!(benches, bench_maps);
criterion_main!(benches);
