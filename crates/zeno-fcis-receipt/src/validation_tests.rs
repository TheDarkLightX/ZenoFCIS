//! Differential checks against the released sealing and validation bodies.
//! Reference: 3b2224d5a081e7ab3e2e282ad6704bcb177d25ac.
//! The two reference functions retain the released control flow and helpers;
//! only function names and the explicit bundle receiver differ.

use super::*;
use alloc::vec;
use zeno_fcis_crypto::{LibcruxSha256, RustCryptoSha256};
use zeno_fcis_patch::{PatchOp, PathSegment, ValuePath, hash_precondition_value, hash_value};
use zeno_fcis_plan::{Effect, OutboxEntry};
use zeno_fcis_value::Field;

// Deliberately colliding: structural field checks must not be justified solely
// by collision resistance, even though production selects an approved provider.
struct ConstantHasher;
impl CommitmentHasher for ConstantHasher {
    const ALGORITHM_ID: &'static str = "test/constant";
    fn hash(_: &[u8]) -> Hash32 {
        Hash32::new([7; 32])
    }
}

fn checked<T, E: core::fmt::Debug>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| panic!("test construction: {error:?}"))
}

fn domain() -> Domain<'static> {
    checked(Domain::new("test/validation", 1))
}

fn bindings() -> CandidateBindings {
    CandidateBindings {
        profile_hash: Hash32::new([1; 32]),
        command_hash: Hash32::new([2; 32]),
        context_hash: Hash32::new([3; 32]),
        precedence_hash: Hash32::new([4; 32]),
        algorithm_hash: Hash32::new([5; 32]),
        budget_hash: Hash32::new([6; 32]),
    }
}

fn plans(count: u32) -> (CommitPlan, OutboxPlan) {
    (
        checked(CommitPlan::try_new(
            (0..count)
                .map(|i| {
                    Effect::new(
                        i,
                        7,
                        Hash32::new([8; 32]),
                        Hash32::new([9; 32]),
                        Value::vector(vec![
                            Value::U128(u128::from(i)),
                            Value::Bytes(vec![42; 64].into()),
                        ]),
                    )
                })
                .collect(),
        )),
        checked(OutboxPlan::try_new(
            (0..count)
                .map(|i| OutboxEntry::new(i, 3, Value::U128(4), Value::U128(u128::from(i))))
                .collect(),
        )),
    )
}

fn cases<H: CommitmentHasher>() -> Vec<(Value, CanonicalPatch)> {
    let mut result = Vec::new();
    for count in [0_u16, 1, 8, 32] {
        let state = checked(Value::record_canonical(
            (0..count)
                .map(|i| Field::new(i, Value::I128(i128::from(i))))
                .collect(),
        ));
        let root = checked(hash_value::<H>(domain(), &state));
        for changed in 0..=count.min(4) {
            let mut operations = Vec::new();
            for i in 0..changed {
                let path = ValuePath::new(vec![PathSegment::Field(i)]);
                let old = checked(hash_precondition_value::<H>(&Value::I128(i128::from(i))));
                operations.push(if i % 2 == 0 {
                    PatchOp::Update {
                        path,
                        expected_old_hash: old,
                        value: Value::U128(u128::MAX),
                    }
                } else {
                    PatchOp::Delete {
                        path,
                        expected_old_hash: old,
                    }
                });
            }
            operations.push(PatchOp::Insert {
                path: ValuePath::new(vec![PathSegment::Field(count)]),
                map_key: None,
                value: Value::Tuple(vec![Value::Bool(true), Value::I128(i128::MIN)].into()),
            });
            result.push((
                state.clone(),
                checked(CanonicalPatch::try_new(1, root, operations)),
            ));
        }
    }
    for state in [
        Value::Unit,
        Value::Bool(false),
        Value::U128(u128::MAX),
        Value::I128(i128::MIN),
        Value::Bytes(vec![0, 255].into()),
        Value::Text("abc".into()),
        Value::Enum {
            type_id: 42,
            variant: 9,
        },
        Value::Tuple(vec![Value::Unit, Value::Bool(true)].into()),
        Value::Sum {
            type_id: 3,
            variant: 2,
            payload: Some(alloc::boxed::Box::new(Value::I128(-1))),
        },
        Value::vector(vec![Value::Unit]),
        Value::Map(Vec::new().into()),
    ] {
        let root = checked(hash_value::<H>(domain(), &state));
        result.push((
            state.clone(),
            checked(CanonicalPatch::try_new(1, root, Vec::new())),
        ));
        let patch = checked(CanonicalPatch::try_new(
            1,
            root,
            vec![PatchOp::Update {
                path: ValuePath::new(Vec::new()),
                expected_old_hash: checked(hash_precondition_value::<H>(&state)),
                value: Value::vector(vec![Value::I128(i128::MIN), Value::U128(u128::MAX)]),
            }],
        ));
        result.push((state, patch));
    }
    result
}

fn compare_validation<H: CommitmentHasher>(bundle: &CommitBundle, state: &Value, d: Domain<'_>) {
    let before = (bundle.clone(), state.clone());
    assert_eq!(
        bundle.validate_and_apply::<H>(state, d),
        released_validate::<H>(bundle, state, d)
    );
    assert_eq!((bundle, state), (&before.0, &before.1));
}

fn check_sealing_and_validation<H: CommitmentHasher>() {
    for (state, patch) in cases::<H>() {
        for count in [0, 1, 4] {
            let (commit, outbox) = plans(count);
            for (kind, reason) in [
                (DecisionKind::Accept, None),
                (DecisionKind::CommittedFailure, Some("failed")),
                (DecisionKind::Reject, None),
                (DecisionKind::Reject, Some("reason")),
                (DecisionKind::Accept, Some("reason")),
                (DecisionKind::CommittedFailure, None),
                (DecisionKind::CommittedFailure, Some("")),
                (DecisionKind::CommittedFailure, Some("é")),
            ] {
                let expected = released_seal::<H>(
                    &state,
                    domain(),
                    kind,
                    reason,
                    bindings(),
                    patch.clone(),
                    commit.clone(),
                    outbox.clone(),
                );
                let actual = CandidateBuilder::seal::<H>(
                    &state,
                    domain(),
                    kind,
                    reason,
                    bindings(),
                    patch.clone(),
                    commit.clone(),
                    outbox.clone(),
                );
                assert_eq!(actual, expected);
                if let Ok(bundle) = actual {
                    assert_eq!(
                        bundle.canonical_bytes(),
                        checked(expected).canonical_bytes()
                    );
                    compare_validation::<H>(&bundle, &state, domain());
                    compare_validation::<H>(&bundle, &Value::Bool(true), domain());
                    compare_validation::<H>(&bundle, &state, checked(Domain::new("test/other", 1)));
                    compare_validation::<H>(
                        &bundle,
                        &state,
                        checked(Domain::new("test/validation", 2)),
                    );
                    let bytes = checked(bundle.canonical_bytes());
                    let decoded = checked(decode_commit_bundle::<H>(
                        &bytes,
                        &state,
                        domain(),
                        BundleDecodeLimits::default(),
                    ));
                    assert_eq!(decoded, bundle);
                    compare_validation::<H>(&decoded, &state, domain());
                }
            }
        }
    }
}

fn changed_body(body: &CandidateBody, index: usize) -> CandidateBody {
    let mut changed = body.clone();
    let h = Hash32::new([99; 32]);
    match index {
        0 => changed.pre_root = h,
        1 => changed.post_root = h,
        2 => changed.patch_hash = h,
        3 => changed.commit_plan_hash = h,
        4 => changed.outbox_plan_hash = h,
        5 => changed.bindings.profile_hash = h,
        6 => changed.bindings.command_hash = h,
        7 => changed.bindings.context_hash = h,
        8 => changed.bindings.precedence_hash = h,
        9 => changed.bindings.algorithm_hash = h,
        10 => changed.bindings.budget_hash = h,
        11 => changed.reason_code = Some(checked(ReasonCode::try_from_string("different".into()))),
        12 => changed.decision_kind = DecisionKind::Reject,
        _ => panic!("unknown body field"),
    }
    changed
}

fn check_field_mutations<H: CommitmentHasher>() {
    for (state, patch) in cases::<H>() {
        let (commit, outbox) = plans(1);
        let bundle = checked(released_seal::<H>(
            &state,
            domain(),
            DecisionKind::Accept,
            None,
            bindings(),
            patch,
            commit,
            outbox,
        ));
        let mut mutations = Vec::new();
        let mut changed = bundle.clone();
        changed.candidate_id = CandidateId::new(Hash32::new([99; 32]));
        mutations.push(changed);
        let mut changed = bundle.clone();
        changed.receipt.candidate_id = CandidateId::new(Hash32::new([99; 32]));
        mutations.push(changed);
        for index in 0..13 {
            let mut changed = bundle.clone();
            changed.body = changed_body(&changed.body, index);
            mutations.push(changed);
            let mut changed = bundle.clone();
            changed.receipt.body = changed_body(&changed.receipt.body, index);
            mutations.push(changed);
        }
        let mut changed = bundle.clone();
        changed.commit_plan = CommitPlan::empty();
        mutations.push(changed);
        let mut changed = bundle.clone();
        changed.outbox_plan = OutboxPlan::empty();
        mutations.push(changed);
        let mut changed = bundle.clone();
        changed.patch = checked(CanonicalPatch::try_new(1, Hash32::ZERO, Vec::new()));
        mutations.push(changed);
        for changed in mutations {
            compare_validation::<H>(&changed, &state, domain());
            compare_validation::<H>(&changed, &Value::Text("é".into()), domain());
        }
    }
}

fn check_error_precedence<H: CommitmentHasher>() {
    let state = Value::Unit;
    let patch = checked(CanonicalPatch::try_new(
        1,
        checked(hash_value::<H>(domain(), &state)),
        Vec::new(),
    ));
    let (commit, outbox) = plans(1);
    let bundle = checked(released_seal::<H>(
        &state,
        domain(),
        DecisionKind::Accept,
        None,
        bindings(),
        patch,
        commit,
        outbox,
    ));
    for bad_commit in [false, true] {
        for bad_outbox in [false, true] {
            for bad_reason in [false, true] {
                let mut changed = bundle.clone();
                changed.candidate_id = CandidateId::new(Hash32::new([99; 32]));
                if bad_commit {
                    changed.commit_plan = checked(CommitPlan::try_new(vec![Effect::new(
                        0,
                        7,
                        Hash32::ZERO,
                        Hash32::ZERO,
                        Value::Text("é".into()),
                    )]));
                }
                if bad_outbox {
                    changed.outbox_plan = checked(OutboxPlan::try_new(vec![OutboxEntry::new(
                        0,
                        3,
                        Value::Text("é".into()),
                        Value::Unit,
                    )]));
                }
                if bad_reason {
                    changed.body.reason_code =
                        Some(checked(ReasonCode::try_from_string("failed".into())));
                }
                compare_validation::<H>(&changed, &state, domain());
                compare_validation::<H>(&changed, &Value::Text("é".into()), domain());
            }
        }
    }
}

#[test]
fn sealing_and_validation_match_released_results_and_bytes() {
    check_sealing_and_validation::<RustCryptoSha256>();
    check_sealing_and_validation::<LibcruxSha256>();
    check_sealing_and_validation::<ConstantHasher>();
}

#[test]
fn every_body_identity_and_receipt_field_retains_its_check() {
    check_field_mutations::<RustCryptoSha256>();
    check_field_mutations::<LibcruxSha256>();
    check_field_mutations::<ConstantHasher>();
}

#[test]
fn simultaneous_failures_keep_the_released_first_error() {
    check_error_precedence::<RustCryptoSha256>();
    check_error_precedence::<LibcruxSha256>();
    check_error_precedence::<ConstantHasher>();
}

#[allow(clippy::too_many_arguments)]
fn released_seal<H: CommitmentHasher>(
    pre_state: &Value,
    state_domain: Domain<'_>,
    decision_kind: DecisionKind,
    reason_code: Option<&str>,
    bindings: CandidateBindings,
    patch: CanonicalPatch,
    commit_plan: CommitPlan,
    outbox_plan: OutboxPlan,
) -> Result<CommitBundle, SealError> {
    let reason_code = validate_decision_reason(decision_kind, reason_code)?;
    let applied = patch
        .apply::<H>(pre_state, state_domain)
        .map_err(SealError::Patch)?;
    let patch_hash = hash_component::<H>("zeno-fcis/patch", &patch)?;
    let commit_plan_hash = hash_component::<H>("zeno-fcis/commit-plan", &commit_plan)?;
    let outbox_plan_hash = hash_component::<H>("zeno-fcis/outbox-plan", &outbox_plan)?;
    let body = CandidateBody {
        decision_kind,
        reason_code,
        bindings,
        pre_root: patch.expected_pre_root(),
        post_root: applied.post_root(),
        patch_hash,
        commit_plan_hash,
        outbox_plan_hash,
    };
    let candidate_hash = hash_component::<H>("zeno-fcis/candidate", &body)?;
    let candidate_id = CandidateId::new(candidate_hash);
    let receipt = Receipt {
        candidate_id,
        body: body.clone(),
    };
    Ok(CommitBundle {
        candidate_id,
        body,
        patch,
        commit_plan,
        outbox_plan,
        receipt,
    })
}

fn released_validate<H: CommitmentHasher>(
    bundle: &CommitBundle,
    pre_state: &Value,
    state_domain: Domain<'_>,
) -> Result<AppliedPatch, SealError> {
    let rebuilt = released_seal::<H>(
        pre_state,
        state_domain,
        bundle.body.decision_kind,
        bundle.body.reason_code.as_ref().map(AsciiText::as_str),
        bundle.body.bindings,
        bundle.patch.clone(),
        bundle.commit_plan.clone(),
        bundle.outbox_plan.clone(),
    )?;
    if rebuilt != *bundle {
        return Err(SealError::BundleMismatch);
    }
    bundle
        .patch
        .apply::<H>(pre_state, state_domain)
        .map_err(SealError::Patch)
}
