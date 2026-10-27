//! Generated-input robustness tests for the commit-bundle decoder and the
//! reference shell, in pure Rust with no dependency. A seeded generator seals
//! complete bundles from independently derived components (as the fuzz target
//! `candidate_bundle` does) and mutates their encodings at the byte and
//! length-prefix level. Every input must decode without a panic; a bundle
//! decodes only if it is byte-for-byte one that was sealed; a decoded bundle
//! commits once, replays idempotently and is refused against any other root.
//! Set `ZENO_FCIS_ROBUSTNESS_ITERATIONS` for a longer run.

use std::panic::{AssertUnwindSafe, catch_unwind};

use zeno_fcis_codec::{Domain, Hash32, RustCryptoSha256, commitment, domains};
use zeno_fcis_patch::{CanonicalPatch, PatchOp, PathSegment, ValuePath, hash_value};
use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan};
use zeno_fcis_receipt::{BundleDecodeLimits, ReceiptDecodeError, decode_commit_bundle};
use zeno_fcis_shell::{CommitStatus, ShellError, ShellState, apply_reference_bundle};
use zeno_fcis_value::Value;

type H = RustCryptoSha256;

const ACCEPT_TAG: u8 = 0;
const NO_FAILURE_REASON: u8 = 0;

/// SplitMix64: small, fast and reproducible from its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn hash(&mut self) -> Hash32 {
        let mut bytes = [0u8; 32];
        for chunk in bytes.chunks_mut(8) {
            chunk.copy_from_slice(&self.next().to_be_bytes());
        }
        Hash32::new(bytes)
    }
}

fn iterations() -> u64 {
    std::env::var("ZENO_FCIS_ROBUSTNESS_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500)
}

fn domain() -> Domain<'static> {
    Domain::new("robustness/candidate-state", 1)
        .unwrap_or_else(|error| panic!("fixed domain is valid: {error:?}"))
}

fn pre_state() -> Value {
    Value::record_canonical(Vec::new())
        .unwrap_or_else(|error| panic!("the empty record is canonical: {error:?}"))
}

fn payload(rng: &mut Rng) -> Value {
    match rng.below(3) {
        0 => Value::unsigned(u128::from(rng.next())),
        1 => Value::boolean(rng.below(2) == 0),
        _ => {
            let len = rng.below(64) as usize;
            Value::bytes((0..len).map(|_| rng.next() as u8).collect())
                .unwrap_or_else(|error| panic!("short bytes: {error:?}"))
        }
    }
}

fn component_hash(domain: Domain<'_>, bytes: &[u8]) -> Hash32 {
    commitment::<H>(domain, bytes)
        .unwrap_or_else(|error| panic!("bounded component commits: {error:?}"))
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) {
    let length = u32::try_from(bytes.len())
        .unwrap_or_else(|error| panic!("bounded component length: {error:?}"));
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
}

/// One sealed bundle: its encoding, its candidate identity, and the offsets
/// of its five length prefixes.
struct Sealed {
    encoded: Vec<u8>,
    candidate: Hash32,
    prefixes: Vec<usize>,
}

/// Seals a bundle that inserts one to three distinct fields into the empty
/// record, with a random commit plan and outbox.
fn sealed(rng: &mut Rng) -> Sealed {
    let pre = pre_state();
    let pre_root = hash_value::<H>(domain(), &pre)
        .unwrap_or_else(|error| panic!("bounded pre-state hashes: {error:?}"));
    let mut fields: Vec<u16> = (0..=rng.below(3)).map(|_| rng.below(64) as u16).collect();
    fields.sort_unstable();
    fields.dedup();
    let ops = fields
        .iter()
        .map(|&field| PatchOp::Insert {
            path: ValuePath::new(vec![PathSegment::Field(field)]),
            map_key: None,
            value: payload(rng),
        })
        .collect();
    let patch = CanonicalPatch::try_new(1, pre_root, ops)
        .unwrap_or_else(|error| panic!("distinct inserts form a patch: {error:?}"));
    let commit_plan = CommitPlan::try_new(
        (0..rng.below(3) as u32)
            .map(|ordinal| {
                Effect::new(
                    ordinal,
                    rng.next() as u32,
                    rng.hash(),
                    rng.hash(),
                    payload(rng),
                )
            })
            .collect(),
    )
    .unwrap_or_else(|error| panic!("distinct ordinals form a plan: {error:?}"));
    let outbox = OutboxPlan::try_new(
        (0..rng.below(3) as u32)
            .map(|ordinal| OutboxEntry::new(ordinal, rng.next() as u32, payload(rng), payload(rng)))
            .collect(),
    )
    .unwrap_or_else(|error| panic!("distinct ordinals form an outbox: {error:?}"));
    let applied = patch
        .apply::<H>(&pre, domain())
        .unwrap_or_else(|error| panic!("inserts apply to the empty record: {error:?}"));
    let patch_bytes = patch
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("patch encodes: {error:?}"));
    let commit_plan_bytes = commit_plan
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("plan encodes: {error:?}"));
    let outbox_bytes = outbox
        .canonical_bytes()
        .unwrap_or_else(|error| panic!("outbox encodes: {error:?}"));

    // Accepted candidate body: decision, no failure reason, the six bindings,
    // then the pre-root, post-root and the three component commitments.
    let mut body = vec![ACCEPT_TAG, NO_FAILURE_REASON];
    for _ in 0..6 {
        body.extend_from_slice(rng.hash().as_bytes());
    }
    for hash in [
        pre_root,
        applied.post_root(),
        component_hash(domains::PATCH, &patch_bytes),
        component_hash(domains::COMMIT_PLAN, &commit_plan_bytes),
        component_hash(domains::OUTBOX_PLAN, &outbox_bytes),
    ] {
        body.extend_from_slice(hash.as_bytes());
    }
    let candidate = component_hash(domains::CANDIDATE, &body);
    let mut receipt = candidate.as_bytes().to_vec();
    put_blob(&mut receipt, &body);
    let mut encoded = candidate.as_bytes().to_vec();
    let mut prefixes = Vec::new();
    for component in [
        &body,
        &patch_bytes,
        &commit_plan_bytes,
        &outbox_bytes,
        &receipt,
    ] {
        prefixes.push(encoded.len());
        put_blob(&mut encoded, component);
    }
    Sealed {
        encoded,
        candidate,
        prefixes,
    }
}

fn decode(bytes: &[u8]) -> Result<zeno_fcis_receipt::CommitBundle, ReceiptDecodeError> {
    decode_commit_bundle::<H>(bytes, &pre_state(), domain(), BundleDecodeLimits::default())
}

/// One mutation; prefix mutations rewrite a length prefix to an edge value.
fn mutate(rng: &mut Rng, mut bytes: Vec<u8>, prefixes: &[usize], other: &[u8]) -> Vec<u8> {
    match rng.below(8) {
        0 if !bytes.is_empty() => {
            let i = rng.below(bytes.len() as u64) as usize;
            bytes[i] ^= 1 << rng.below(8);
        }
        1 => {
            let i = rng.below(bytes.len() as u64 + 1) as usize;
            bytes.insert(i, rng.next() as u8);
        }
        2 if !bytes.is_empty() => {
            let i = rng.below(bytes.len() as u64) as usize;
            bytes.remove(i);
        }
        3 => {
            let keep = rng.below(bytes.len() as u64 + 1) as usize;
            bytes.truncate(keep);
        }
        4 if !other.is_empty() => {
            let i = rng.below(bytes.len() as u64 + 1) as usize;
            let j = rng.below(other.len() as u64) as usize;
            bytes.truncate(i);
            bytes.extend_from_slice(&other[j..]);
        }
        5 | 6 if !prefixes.is_empty() => {
            let at = prefixes[rng.below(prefixes.len() as u64) as usize];
            if at + 4 <= bytes.len() {
                let old =
                    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
                let rest = (bytes.len() - at - 4) as u32;
                let new = [
                    0,
                    1,
                    old.wrapping_sub(1),
                    old.wrapping_add(1),
                    rest,
                    rest + 1,
                    u32::MAX,
                    1 << 31,
                ][rng.below(8) as usize];
                bytes[at..at + 4].copy_from_slice(&new.to_be_bytes());
            }
        }
        _ if !bytes.is_empty() => {
            let i = rng.below(bytes.len() as u64) as usize;
            bytes[i] = [0x00, 0xFF, 0x7F, 0x80][rng.below(4) as usize];
        }
        _ => bytes.push(rng.next() as u8),
    }
    bytes
}

/// The decoder's contract on any input: no panic, and an input decodes only if
/// it is exactly one of the `sealed` encodings, which then re-encodes to itself.
fn check(input: &[u8], sealed: &[&[u8]], seed: u64) {
    let outcome = catch_unwind(AssertUnwindSafe(|| decode(input)));
    let decoded = outcome.unwrap_or_else(|_| {
        panic!("decode_commit_bundle panicked; seed {seed}, input {input:02x?}")
    });
    if let Ok(bundle) = decoded {
        assert!(
            sealed.contains(&input),
            "a changed bundle decoded; seed {seed}, input {input:02x?}"
        );
        assert_eq!(
            bundle.canonical_bytes().ok().as_deref(),
            Some(input),
            "seed {seed}"
        );
    }
}

#[test]
fn sealed_bundles_decode_commit_once_and_replay() {
    for seed in 0..iterations() {
        let mut rng = Rng(seed);
        let a = sealed(&mut rng);
        let b = sealed(&mut rng);
        let bundle = decode(&a.encoded)
            .unwrap_or_else(|e| panic!("sealed bundle refused: {e:?}; seed {seed}"));
        assert_eq!(bundle.candidate_id().hash(), a.candidate, "seed {seed}");
        assert_eq!(
            bundle.canonical_bytes().ok(),
            Some(a.encoded.clone()),
            "seed {seed}"
        );
        assert!(
            bundle.validate::<H>(&pre_state(), domain()).is_ok(),
            "seed {seed}"
        );

        let shell = ShellState::new::<H>(pre_state(), domain())
            .unwrap_or_else(|error| panic!("shell starts: {error:?}"));
        let replay_a = rng.hash();
        let first = apply_reference_bundle::<H>(&shell, domain(), replay_a, &bundle)
            .unwrap_or_else(|error| panic!("commits: {error:?}"));
        assert_eq!(first.status(), CommitStatus::Committed, "seed {seed}");
        let again = apply_reference_bundle::<H>(first.state(), domain(), replay_a, &bundle)
            .unwrap_or_else(|error| panic!("replays: {error:?}"));
        assert_eq!(
            again.status(),
            CommitStatus::IdempotentReplay,
            "seed {seed}"
        );
        assert_eq!(again.state(), first.state(), "seed {seed}");

        // A second bundle sealed against the same pre-state is refused once
        // the root has moved, and cannot reuse the first replay identity.
        let other = decode(&b.encoded)
            .unwrap_or_else(|error| panic!("second sealed bundle decodes: {error:?}"));
        if other.candidate_id() != bundle.candidate_id() {
            let moved = apply_reference_bundle::<H>(first.state(), domain(), rng.hash(), &other);
            assert!(
                matches!(moved, Err(ShellError::RootConflict { .. })),
                "seed {seed}: {moved:?}"
            );
            let reused = apply_reference_bundle::<H>(first.state(), domain(), replay_a, &other);
            assert!(
                matches!(reused, Err(ShellError::ReplayConflict { .. })),
                "seed {seed}: {reused:?}"
            );
        }
    }
}

#[test]
fn mutated_bundles_never_panic_and_never_decode() {
    for seed in 0..iterations() {
        let mut rng = Rng(seed ^ 0xB0B0_B0B0);
        let a = sealed(&mut rng);
        let b = sealed(&mut rng);
        let mut input = a.encoded.clone();
        for _ in 0..=rng.below(3) {
            input = mutate(&mut rng, input, &a.prefixes, &b.encoded);
        }
        check(&input, &[&a.encoded, &b.encoded], seed);
    }
}

#[test]
fn every_length_prefix_edge_is_refused_without_panic() {
    let mut rng = Rng(7);
    let a = sealed(&mut rng);
    for &at in &a.prefixes {
        let old = u32::from_be_bytes([
            a.encoded[at],
            a.encoded[at + 1],
            a.encoded[at + 2],
            a.encoded[at + 3],
        ]);
        let rest = (a.encoded.len() - at - 4) as u32;
        for new in [
            0,
            1,
            old.wrapping_sub(1),
            old.wrapping_add(1),
            rest + 1,
            u32::MAX,
            1 << 31,
        ] {
            if new == old {
                continue;
            }
            let mut input = a.encoded.clone();
            input[at..at + 4].copy_from_slice(&new.to_be_bytes());
            check(&input, &[&a.encoded], u64::from(new));
            assert!(
                decode(&input).is_err(),
                "prefix at {at} set to {new} decoded"
            );
        }
    }
}

#[test]
fn random_bytes_never_panic() {
    for seed in 0..iterations() * 4 {
        let mut rng = Rng(seed.wrapping_mul(31) ^ 0x5EED);
        let len = rng.below(400) as usize;
        let mut input: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        // Half the inputs get a plausible first length prefix after the identity.
        if seed % 2 == 0 && input.len() >= 36 {
            let claimed = rng.below(input.len() as u64) as u32;
            input[32..36].copy_from_slice(&claimed.to_be_bytes());
        }
        check(&input, &[], seed);
    }
}
