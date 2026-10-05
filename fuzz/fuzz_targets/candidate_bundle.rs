#![no_main]

//! Sealing is library-private, so the harness encodes one complete bundle from
//! independently derived component commitments and admits it through the
//! public decoder, which re-seals it against the pre-state. The admitted bundle
//! must validate, commit once, and replay idempotently.

use libfuzzer_sys::fuzz_target;
use zeno_fcis_codec::{Domain, Hash32, commitment, domains};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_patch::{CanonicalPatch, PatchOp, ValuePath, hash_value};
use zeno_fcis_plan::{CommitPlan, OutboxEntry, OutboxPlan};
use zeno_fcis_receipt::{BundleDecodeLimits, ReceiptDecodeError, decode_commit_bundle};
use zeno_fcis_shell::{CommitStatus, ShellState, apply_reference_bundle};
use zeno_fcis_value::Value;

const ACCEPT_TAG: u8 = 0;
const NO_FAILURE_REASON: u8 = 0;

fn repeated_hash(seed: u8) -> Hash32 {
    Hash32::new([seed; 32])
}

fn component_hash(domain: Domain<'_>, bytes: &[u8]) -> Hash32 {
    let Ok(hash) = commitment::<RustCryptoSha256>(domain, bytes) else {
        panic!("bounded component must commit");
    };
    hash
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) {
    let Ok(length) = u32::try_from(bytes.len()) else {
        panic!("bounded component length must fit the wire prefix");
    };
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
}

fuzz_target!(|input: &[u8]| {
    let Some(seed) = input.first().copied() else {
        return;
    };
    let field_id = match input.get(1..3) {
        Some(bytes) => u16::from_be_bytes([bytes[0], bytes[1]]),
        None => u16::from(seed),
    };
    let payload_end = input.len().min(131);
    let payload_start = input.len().min(3);
    let Ok(payload) = Value::bytes(input[payload_start..payload_end].to_vec()) else {
        panic!("at most 128 payload bytes must form a value");
    };
    let Ok(pre_state) = Value::record_canonical(Vec::new()) else {
        panic!("the empty record must be canonical");
    };
    let Ok(domain) = Domain::new("fuzz/candidate-state", 1) else {
        panic!("fixed fuzz domain must be valid");
    };
    let Ok(pre_root) = hash_value::<RustCryptoSha256>(domain, &pre_state) else {
        panic!("bounded pre-state must hash");
    };
    let Ok(patch) = CanonicalPatch::try_new(
        1,
        pre_root,
        vec![PatchOp::Insert {
            path: ValuePath::new(vec![zeno_fcis_patch::PathSegment::Field(field_id)]),
            map_key: None,
            value: payload.clone(),
        }],
    ) else {
        panic!("single-field insert must form a canonical patch");
    };
    let Ok(outbox) = OutboxPlan::try_new(vec![OutboxEntry::new(
        0,
        u32::from(seed),
        Value::unsigned(u128::from(field_id)),
        payload,
    )]) else {
        panic!("single-entry outbox must be canonical");
    };
    let commit_plan = CommitPlan::empty();
    let Ok(applied) = patch.apply::<RustCryptoSha256>(&pre_state, domain) else {
        panic!("single-field insert must apply to the empty record");
    };
    let (Ok(patch_bytes), Ok(commit_plan_bytes), Ok(outbox_bytes)) = (
        patch.canonical_bytes(),
        commit_plan.canonical_bytes(),
        outbox.canonical_bytes(),
    ) else {
        panic!("bounded components must encode");
    };

    // Accepted candidate body: decision, no failure reason, the six bindings,
    // then the pre-root, post-root and the three component commitments.
    let mut body = vec![ACCEPT_TAG, NO_FAILURE_REASON];
    for offset in 0..6 {
        body.extend_from_slice(repeated_hash(seed.wrapping_add(offset)).as_bytes());
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
    let candidate_id = component_hash(domains::CANDIDATE, &body);
    let mut receipt = candidate_id.as_bytes().to_vec();
    put_blob(&mut receipt, &body);
    let mut encoded = candidate_id.as_bytes().to_vec();
    for component in [
        &body,
        &patch_bytes,
        &commit_plan_bytes,
        &outbox_bytes,
        &receipt,
    ] {
        put_blob(&mut encoded, component);
    }

    let limits = BundleDecodeLimits::default();
    let Ok(bundle) = decode_commit_bundle::<RustCryptoSha256>(&encoded, &pre_state, domain, limits)
    else {
        panic!("valid bounded components must seal");
    };
    assert_eq!(bundle.candidate_id().hash(), candidate_id);
    assert_eq!(bundle.canonical_bytes().ok(), Some(encoded.clone()));
    assert!(
        bundle
            .validate::<RustCryptoSha256>(&pre_state, domain)
            .is_ok()
    );

    // Any other candidate identity is refused before the shell sees it.
    let mut forged = encoded.clone();
    forged[usize::from(seed) % 32] ^= 1;
    assert_eq!(
        decode_commit_bundle::<RustCryptoSha256>(&forged, &pre_state, domain, limits).err(),
        Some(ReceiptDecodeError::CandidateMismatch)
    );

    let Ok(shell) = ShellState::new::<RustCryptoSha256>(pre_state, domain) else {
        panic!("bounded pre-state must initialize a shell");
    };
    let replay_id = repeated_hash(seed.wrapping_add(6));
    let Ok(first) = apply_reference_bundle::<RustCryptoSha256>(&shell, domain, replay_id, &bundle)
    else {
        panic!("valid bundle must commit");
    };
    assert_eq!(first.status(), CommitStatus::Committed);
    let Ok(replay) =
        apply_reference_bundle::<RustCryptoSha256>(first.state(), domain, replay_id, &bundle)
    else {
        panic!("exact replay must succeed");
    };
    assert_eq!(replay.status(), CommitStatus::IdempotentReplay);
    assert_eq!(replay.state(), first.state());
});
