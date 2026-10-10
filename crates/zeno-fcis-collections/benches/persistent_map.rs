#![allow(missing_docs)]
//! Benchmarks for persistent collection backends.
//!
//! Benchmarks small/dense and large/sparse states, lookups, batch updates,
//! freeze, canonical iteration, root generation, and retained snapshots.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_collections::{BTreeMapBackend, LogicalEntry, PersistentMap};
use zeno_fcis_value::Value;

fn make_entry(key_byte: u8, value_byte: u8) -> LogicalEntry {
    let key = Value::U128(u128::from(key_byte));
    let encoded_key = key
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode key: {e}"));
    LogicalEntry::new(encoded_key, key, Value::U128(u128::from(value_byte)))
}

fn build_dense(size: u8) -> Vec<LogicalEntry> {
    (1..=size).map(|i| make_entry(i, i * 2)).collect()
}

fn build_sparse(size: u8, stride: u8) -> Vec<LogicalEntry> {
    (1..=size)
        .filter(|i| i % stride == 0)
        .map(|i| make_entry(i, i * 3))
        .collect()
}

fn bench_insert_dense(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_dense");
    for size in [10u8, 50, 100, 200] {
        let entries = build_dense(size);
        group.bench_with_input(
            BenchmarkId::new("btreemap", size),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let mut map = BTreeMapBackend::empty();
                    for entry in entries {
                        map = map.insert(entry.clone());
                    }
                    black_box(map);
                });
            },
        );
    }
    group.finish();
}

fn bench_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup");
    for size in [10u8, 50, 100, 200] {
        let entries = build_dense(size);
        let map: BTreeMapBackend = entries
            .iter()
            .fold(BTreeMapBackend::empty(), |m, e| m.insert(e.clone()));
        let lookup_key = Value::U128(u128::from(size / 2))
            .canonical_bytes()
            .unwrap_or_else(|e| panic!("encode key: {e}"));
        group.bench_with_input(BenchmarkId::new("btreemap", size), &lookup_key, |b, key| {
            b.iter(|| map.get(black_box(key.as_slice())));
        });
    }
    group.finish();
}

fn bench_canonical_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("canonical_iteration");
    for size in [10u8, 50, 100, 200] {
        let entries = build_dense(size);
        let map: BTreeMapBackend = entries
            .iter()
            .fold(BTreeMapBackend::empty(), |m, e| m.insert(e.clone()));
        group.bench_with_input(BenchmarkId::new("btreemap", size), &map, |b, map| {
            b.iter(|| map.to_entries());
        });
    }
    group.finish();
}

fn bench_root_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("root_generation");
    for size in [10u8, 50, 100, 200] {
        let entries = build_dense(size);
        let map: BTreeMapBackend = entries
            .iter()
            .fold(BTreeMapBackend::empty(), |m, e| m.insert(e.clone()));
        group.bench_with_input(BenchmarkId::new("btreemap", size), &map, |b, map| {
            b.iter(|| map.canonical_bytes());
        });
    }
    group.finish();
}

fn bench_snapshot_retention(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_retention");
    for size in [10u8, 50, 100] {
        let entries = build_dense(size);
        group.bench_with_input(
            BenchmarkId::new("btreemap", size),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let mut snapshots = Vec::new();
                    let mut map = BTreeMapBackend::empty();
                    for entry in entries {
                        map = map.insert(entry.clone());
                        snapshots.push(map.clone());
                    }
                    black_box(snapshots);
                });
            },
        );
    }
    group.finish();
}

fn bench_sparse_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_sparse");
    for size in [50u8, 100, 200] {
        let entries = build_sparse(size, 5);
        group.bench_with_input(
            BenchmarkId::new("btreemap", size),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let mut map = BTreeMapBackend::empty();
                    for entry in entries {
                        map = map.insert(entry.clone());
                    }
                    black_box(map);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_insert_dense,
    bench_lookup,
    bench_canonical_iteration,
    bench_root_generation,
    bench_snapshot_retention,
    bench_sparse_insert,
);
criterion_main!(benches);
