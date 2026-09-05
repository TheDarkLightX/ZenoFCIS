#![allow(missing_docs)]
//! Compare exact domain commitments against the allocating reference path.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use zeno_fcis_codec::{CommitmentHasher, Domain, commitment, domain_preimage};
use zeno_fcis_crypto::{LibcruxSha256, RustCryptoSha256};

fn bench_provider<H: CommitmentHasher>(c: &mut Criterion, name: &str) {
    let domain = Domain::new("zeno-fcis/bench", 1)
        .unwrap_or_else(|error| panic!("benchmark domain: {error}"));
    let mut group = c.benchmark_group(name);
    for size in [0_usize, 64, 1024, 65_536, 1_048_576] {
        let payload: Vec<u8> = (0_u8..=255).cycle().take(size).collect();
        let reference = domain_preimage(domain, &payload)
            .unwrap_or_else(|error| panic!("reference preimage: {error}"));
        assert_eq!(commitment::<H>(domain, &payload), Ok(H::hash(&reference)));
        group.throughput(Throughput::Bytes(
            u64::try_from(size).unwrap_or_else(|error| panic!("benchmark size: {error}")),
        ));
        group.bench_with_input(BenchmarkId::new("materialized", size), &payload, |b, p| {
            b.iter(|| {
                let preimage = domain_preimage(black_box(domain), black_box(p))
                    .unwrap_or_else(|error| panic!("preimage: {error}"));
                black_box(H::hash(&preimage))
            });
        });
        group.bench_with_input(BenchmarkId::new("streamed", size), &payload, |b, p| {
            b.iter(|| {
                black_box(
                    commitment::<H>(black_box(domain), black_box(p))
                        .unwrap_or_else(|error| panic!("commitment: {error}")),
                )
            });
        });
    }
    group.finish();
}

fn bench_commitments(c: &mut Criterion) {
    bench_provider::<RustCryptoSha256>(c, "commitment/rustcrypto");
    bench_provider::<LibcruxSha256>(c, "commitment/libcrux");
}

criterion_group!(benches, bench_commitments);
criterion_main!(benches);
