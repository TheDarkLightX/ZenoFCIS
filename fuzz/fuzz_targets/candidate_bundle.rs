#![no_main]

use libfuzzer_sys::fuzz_target;
use zeno_fcis_codec::{Domain, Hash32};
use zeno_fcis_core::DecisionKind;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_patch::{CanonicalPatch, PatchOp, ValuePath, hash_value};
use zeno_fcis_plan::{CommitPlan, OutboxEntry, OutboxPlan};
use zeno_fcis_receipt::{CandidateBindings, CandidateBuilder};
use zeno_fcis_shell::{CommitStatus, ShellState, commit};
use zeno_fcis_value::Value;

fn repeated_hash(seed: u8) -> Hash32 {
    Hash32::new([seed; 32])
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
    let payload = Value::Bytes(
        input[payload_start..payload_end]
            .to_vec()
            .into_boxed_slice(),
    );
    let pre_state = Value::Record(Vec::new().into_boxed_slice());
    let Ok(domain) = Domain::new("zeno-fcis/fuzz-state", 1) else {
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
        Value::U128(u128::from(field_id)),
        payload,
    )]) else {
        panic!("single-entry outbox must be canonical");
    };
    let bindings = CandidateBindings {
        profile_hash: repeated_hash(seed),
        command_hash: repeated_hash(seed.wrapping_add(1)),
        context_hash: repeated_hash(seed.wrapping_add(2)),
        precedence_hash: repeated_hash(seed.wrapping_add(3)),
        algorithm_hash: repeated_hash(seed.wrapping_add(4)),
        budget_hash: repeated_hash(seed.wrapping_add(5)),
    };
    let Ok(bundle) = CandidateBuilder::seal::<RustCryptoSha256>(
        &pre_state,
        domain,
        DecisionKind::Accept,
        None,
        bindings,
        patch,
        CommitPlan::empty(),
        outbox,
    ) else {
        panic!("valid bounded components must seal");
    };
    assert!(
        bundle
            .validate::<RustCryptoSha256>(&pre_state, domain)
            .is_ok()
    );
    let Ok(shell) = ShellState::new::<RustCryptoSha256>(pre_state, domain) else {
        panic!("bounded pre-state must initialize a shell");
    };
    let replay_id = repeated_hash(seed.wrapping_add(6));
    let Ok(first) = commit::<RustCryptoSha256>(&shell, domain, replay_id, &bundle) else {
        panic!("valid bundle must commit");
    };
    assert_eq!(first.status(), CommitStatus::Committed);
    let Ok(replay) = commit::<RustCryptoSha256>(first.state(), domain, replay_id, &bundle) else {
        panic!("exact replay must succeed");
    };
    assert_eq!(replay.status(), CommitStatus::IdempotentReplay);
    assert_eq!(replay.state(), first.state());
});
