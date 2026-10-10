//! Same-candidate construction for patches, plans, receipts, and commit bundles.
//!
//! Candidate sealing applies the patch to the supplied immutable pre-state,
//! derives the post-root, commits every component, and creates one candidate
//! identity carried by the receipt and bundle. Components from different
//! candidates cannot be combined through the supported API.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::DecisionKind;
use zeno_fcis_patch::{AppliedPatch, CanonicalPatch, PatchError};
use zeno_fcis_plan::{CommitPlan, OutboxPlan};
use zeno_fcis_value::{AsciiText, TextError, Value};

/// Maximum stable reason-code bytes in the initial profile.
pub const MAX_REASON_CODE_BYTES: usize = 96;

/// A stable, bounded, ASCII protocol reason code.
pub type ReasonCode = AsciiText<MAX_REASON_CODE_BYTES>;

/// The content-addressed identity of one complete committed candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CandidateId(Hash32);

impl CandidateId {
    /// Creates an identity from its commitment.
    #[must_use]
    pub const fn new(hash: Hash32) -> Self {
        Self(hash)
    }

    /// Returns the commitment.
    #[must_use]
    pub const fn hash(self) -> Hash32 {
        self.0
    }
}

impl fmt::Display for CandidateId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Inputs whose commitments must be shared by every candidate component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateBindings {
    /// Protocol/profile identity.
    pub profile_hash: Hash32,
    /// Exact authenticated command commitment.
    pub command_hash: Hash32,
    /// Exact context, policy, and evidence commitment.
    pub context_hash: Hash32,
    /// Stable rejection/failure precedence profile.
    pub precedence_hash: Hash32,
    /// Algorithms and codec versions.
    pub algorithm_hash: Hash32,
    /// Deterministic resource-limit and consumption report.
    pub budget_hash: Hash32,
}

/// The acyclic body from which the candidate identity is derived.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateBody {
    decision_kind: DecisionKind,
    reason_code: Option<ReasonCode>,
    bindings: CandidateBindings,
    pre_root: Hash32,
    post_root: Hash32,
    patch_hash: Hash32,
    commit_plan_hash: Hash32,
    outbox_plan_hash: Hash32,
}

impl CandidateBody {
    /// Returns the decision kind.
    #[must_use]
    pub const fn decision_kind(&self) -> DecisionKind {
        self.decision_kind
    }

    /// Returns the optional committed-failure reason.
    #[must_use]
    pub const fn reason_code(&self) -> Option<&ReasonCode> {
        self.reason_code.as_ref()
    }

    /// Returns shared bindings.
    #[must_use]
    pub const fn bindings(&self) -> CandidateBindings {
        self.bindings
    }

    /// Returns the pre-root.
    #[must_use]
    pub const fn pre_root(&self) -> Hash32 {
        self.pre_root
    }

    /// Returns the post-root.
    #[must_use]
    pub const fn post_root(&self) -> Hash32 {
        self.post_root
    }

    /// Returns the patch commitment.
    #[must_use]
    pub const fn patch_hash(&self) -> Hash32 {
        self.patch_hash
    }

    /// Returns the authoritative plan commitment.
    #[must_use]
    pub const fn commit_plan_hash(&self) -> Hash32 {
        self.commit_plan_hash
    }

    /// Returns the outbox plan commitment.
    #[must_use]
    pub const fn outbox_plan_hash(&self) -> Hash32 {
        self.outbox_plan_hash
    }
}

impl CanonicalEncode for CandidateBody {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(decision_tag(self.decision_kind));
        match &self.reason_code {
            None => output.push(0),
            Some(reason) => {
                output.push(1);
                put_blob(output, reason.as_str().as_bytes())?;
            }
        }
        encode_bindings(self.bindings, output);
        output.extend_from_slice(self.pre_root.as_bytes());
        output.extend_from_slice(self.post_root.as_bytes());
        output.extend_from_slice(self.patch_hash.as_bytes());
        output.extend_from_slice(self.commit_plan_hash.as_bytes());
        output.extend_from_slice(self.outbox_plan_hash.as_bytes());
        Ok(())
    }
}

/// Canonical receipt for an accepted or committed-failure candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Receipt {
    candidate_id: CandidateId,
    body: CandidateBody,
}

impl Receipt {
    /// Returns the candidate identity.
    #[must_use]
    pub const fn candidate_id(&self) -> CandidateId {
        self.candidate_id
    }

    /// Returns the candidate body.
    #[must_use]
    pub const fn body(&self) -> &CandidateBody {
        &self.body
    }
}

impl CanonicalEncode for Receipt {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.candidate_id.hash().as_bytes());
        put_blob(output, &self.body.canonical_bytes()?)
    }
}

/// An unchanged-state rejection receipt with no candidate or effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectReceipt {
    bindings: CandidateBindings,
    pre_root: Hash32,
    reason_code: ReasonCode,
}

impl RejectReceipt {
    /// Creates a rejection receipt. The post-root is definitionally the pre-root.
    pub fn new(
        bindings: CandidateBindings,
        pre_root: Hash32,
        reason_code: &str,
    ) -> Result<Self, SealError> {
        Ok(Self {
            bindings,
            pre_root,
            reason_code: ReasonCode::try_from_string(reason_code.into())
                .map_err(SealError::ReasonCode)?,
        })
    }

    /// Returns the exact profile, command, context, precedence, algorithm, and budget bindings.
    #[must_use]
    pub const fn bindings(&self) -> CandidateBindings {
        self.bindings
    }

    /// Returns the unchanged root.
    #[must_use]
    pub const fn pre_root(&self) -> Hash32 {
        self.pre_root
    }

    /// Returns the unchanged post-root.
    #[must_use]
    pub const fn post_root(&self) -> Hash32 {
        self.pre_root
    }

    /// Returns the stable reason code.
    #[must_use]
    pub const fn reason_code(&self) -> &ReasonCode {
        &self.reason_code
    }
}

impl CanonicalEncode for RejectReceipt {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        encode_bindings(self.bindings, output);
        output.extend_from_slice(self.pre_root.as_bytes());
        put_blob(output, self.reason_code.as_str().as_bytes())
    }
}

/// The complete atomic publication value for one committed candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitBundle {
    candidate_id: CandidateId,
    body: CandidateBody,
    patch: CanonicalPatch,
    commit_plan: CommitPlan,
    outbox_plan: OutboxPlan,
    receipt: Receipt,
}

impl CommitBundle {
    /// Returns the candidate identity.
    #[must_use]
    pub const fn candidate_id(&self) -> CandidateId {
        self.candidate_id
    }

    /// Returns the candidate body.
    #[must_use]
    pub const fn body(&self) -> &CandidateBody {
        &self.body
    }

    /// Returns the preconditioned patch.
    #[must_use]
    pub const fn patch(&self) -> &CanonicalPatch {
        &self.patch
    }

    /// Returns authoritative operations.
    #[must_use]
    pub const fn commit_plan(&self) -> &CommitPlan {
        &self.commit_plan
    }

    /// Returns external-delivery obligations.
    #[must_use]
    pub const fn outbox_plan(&self) -> &OutboxPlan {
        &self.outbox_plan
    }

    /// Returns the candidate receipt.
    #[must_use]
    pub const fn receipt(&self) -> &Receipt {
        &self.receipt
    }

    /// Recomputes every candidate relationship against the supplied pre-state.
    pub fn validate<H: CommitmentHasher>(
        &self,
        pre_state: &Value,
        state_domain: Domain<'_>,
    ) -> Result<(), SealError> {
        self.validate_and_apply::<H>(pre_state, state_domain)
            .map(|_| ())
    }

    /// Revalidates the bundle and returns the exact pure successor state.
    pub fn validate_and_apply<H: CommitmentHasher>(
        &self,
        pre_state: &Value,
        state_domain: Domain<'_>,
    ) -> Result<AppliedPatch, SealError> {
        let rebuilt = CandidateBuilder::seal::<H>(
            pre_state,
            state_domain,
            self.body.decision_kind,
            self.body.reason_code.as_ref().map(AsciiText::as_str),
            self.body.bindings,
            self.patch.clone(),
            self.commit_plan.clone(),
            self.outbox_plan.clone(),
        )?;
        if rebuilt != *self {
            return Err(SealError::BundleMismatch);
        }
        self.patch
            .apply::<H>(pre_state, state_domain)
            .map_err(SealError::Patch)
    }
}

impl CanonicalEncode for CommitBundle {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.candidate_id.hash().as_bytes());
        put_blob(output, &self.body.canonical_bytes()?)?;
        put_blob(output, &self.patch.canonical_bytes()?)?;
        put_blob(output, &self.commit_plan.canonical_bytes()?)?;
        put_blob(output, &self.outbox_plan.canonical_bytes()?)?;
        put_blob(output, &self.receipt.canonical_bytes()?)
    }
}

/// Private-construction namespace for complete candidates.
pub struct CandidateBuilder;

impl CandidateBuilder {
    /// Applies the patch, derives every component hash, and seals one bundle.
    #[allow(clippy::too_many_arguments)]
    pub fn seal<H: CommitmentHasher>(
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
}

fn validate_decision_reason(
    decision_kind: DecisionKind,
    reason_code: Option<&str>,
) -> Result<Option<ReasonCode>, SealError> {
    match (decision_kind, reason_code) {
        (DecisionKind::Accept, None) => Ok(None),
        (DecisionKind::CommittedFailure, Some(reason)) => Ok(Some(
            ReasonCode::try_from_string(reason.into()).map_err(SealError::ReasonCode)?,
        )),
        (DecisionKind::Reject, _) => Err(SealError::RejectCannotBeCandidate),
        (DecisionKind::Accept, Some(_)) => Err(SealError::AcceptHasFailureReason),
        (DecisionKind::CommittedFailure, None) => Err(SealError::MissingFailureReason),
    }
}

fn hash_component<H: CommitmentHasher>(
    domain_name: &str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, SealError> {
    let domain = Domain::new(domain_name, 1).map_err(SealError::Encode)?;
    let bytes = value.canonical_bytes().map_err(SealError::Encode)?;
    commitment::<H>(domain, &bytes).map_err(SealError::Encode)
}

const fn decision_tag(kind: DecisionKind) -> u8 {
    match kind {
        DecisionKind::Accept => 0,
        DecisionKind::Reject => 1,
        DecisionKind::CommittedFailure => 2,
    }
}

fn encode_bindings(bindings: CandidateBindings, output: &mut Vec<u8>) {
    output.extend_from_slice(bindings.profile_hash.as_bytes());
    output.extend_from_slice(bindings.command_hash.as_bytes());
    output.extend_from_slice(bindings.context_hash.as_bytes());
    output.extend_from_slice(bindings.precedence_hash.as_bytes());
    output.extend_from_slice(bindings.algorithm_hash.as_bytes());
    output.extend_from_slice(bindings.budget_hash.as_bytes());
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    let length = u32::try_from(bytes.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

/// Candidate construction or validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SealError {
    /// Canonical encoding failed.
    Encode(EncodeError),
    /// Patch application failed.
    Patch(PatchError),
    /// Stable reason-code construction failed.
    ReasonCode(TextError),
    /// Rejection cannot carry a committed candidate.
    RejectCannotBeCandidate,
    /// Acceptance cannot carry a committed-failure reason.
    AcceptHasFailureReason,
    /// Committed failure requires a reason.
    MissingFailureReason,
    /// Reconstructed bundle does not match the supplied bundle.
    BundleMismatch,
}

impl fmt::Display for SealError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(error) => error.fmt(formatter),
            Self::Patch(error) => error.fmt(formatter),
            Self::ReasonCode(error) => error.fmt(formatter),
            Self::RejectCannotBeCandidate => {
                formatter.write_str("rejection cannot carry a candidate")
            }
            Self::AcceptHasFailureReason => {
                formatter.write_str("acceptance cannot carry a failure reason")
            }
            Self::MissingFailureReason => {
                formatter.write_str("committed failure requires a reason")
            }
            Self::BundleMismatch => {
                formatter.write_str("commit bundle relationships do not validate")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use zeno_fcis_patch::{PatchOp, ValuePath};
    use zeno_fcis_value::Field;

    struct TestHasher;

    impl CommitmentHasher for TestHasher {
        const ALGORITHM_ID: &'static str = "test/fold/v1";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().copied().enumerate() {
                let slot = index % 32;
                output[slot] = output[slot].wrapping_add(byte);
            }
            Hash32::new(output)
        }
    }

    fn domain() -> Domain<'static> {
        Domain::new("test/state", 1).unwrap_or_else(|error| panic!("domain: {error}"))
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

    #[test]
    fn candidate_seals_patch_plan_receipt_and_bundle_together() {
        let state = Value::Record(Vec::<Field>::new().into_boxed_slice());
        let pre_root =
            zeno_fcis_patch::hash_value::<TestHasher>(domain(), &state).unwrap_or(Hash32::ZERO);
        let patch = CanonicalPatch::try_new(
            1,
            pre_root,
            vec![PatchOp::Insert {
                path: ValuePath::new(vec![zeno_fcis_patch::PathSegment::Field(1)]),
                map_key: None,
                value: Value::U128(9),
            }],
        )
        .unwrap_or_else(|error| panic!("patch: {error}"));
        let bundle = CandidateBuilder::seal::<TestHasher>(
            &state,
            domain(),
            DecisionKind::Accept,
            None,
            bindings(),
            patch,
            CommitPlan::empty(),
            OutboxPlan::empty(),
        );
        assert!(bundle.is_ok());
        let bundle = bundle.unwrap_or_else(|error| panic!("seal: {error}"));
        assert_eq!(bundle.receipt().candidate_id(), bundle.candidate_id());
        assert_eq!(bundle.validate::<TestHasher>(&state, domain()), Ok(()));
    }

    #[test]
    fn committed_failure_requires_a_reason() {
        let state = Value::Unit;
        let root =
            zeno_fcis_patch::hash_value::<TestHasher>(domain(), &state).unwrap_or(Hash32::ZERO);
        let patch = CanonicalPatch::try_new(1, root, Vec::new())
            .unwrap_or_else(|error| panic!("patch: {error}"));
        let result = CandidateBuilder::seal::<TestHasher>(
            &state,
            domain(),
            DecisionKind::CommittedFailure,
            None,
            bindings(),
            patch,
            CommitPlan::empty(),
            OutboxPlan::empty(),
        );
        assert_eq!(result, Err(SealError::MissingFailureReason));
    }

    #[test]
    fn reject_receipt_is_unchanged_state_only() {
        let receipt = RejectReceipt::new(bindings(), Hash32::new([9; 32]), "invalid_command");
        assert!(receipt.is_ok());
        let receipt = receipt.unwrap_or_else(|error| panic!("receipt: {error}"));
        assert_eq!(receipt.pre_root(), receipt.post_root());
    }
}
