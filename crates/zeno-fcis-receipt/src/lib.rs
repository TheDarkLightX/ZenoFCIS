//! Same-candidate construction for patches, plans, receipts, and commit bundles.
//!
//! Candidate sealing applies the patch to the supplied immutable pre-state,
//! derives the post-root, commits every component, and creates one candidate
//! identity carried by the receipt and bundle. Components from different
//! candidates cannot be combined through the supported API.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::DecisionKind;
use zeno_fcis_patch::{
    AppliedPatch, CanonicalPatch, PatchDecodeError, PatchDecodeLimits, PatchError,
    decode_canonical_patch,
};
use zeno_fcis_plan::{
    CommitPlan, OutboxPlan, PlanDecodeError, PlanDecodeLimits, decode_commit_plan,
    decode_outbox_plan,
};
use zeno_fcis_value::{AsciiText, TextError, Value};

/// Maximum stable reason-code bytes in the initial profile.
pub const MAX_REASON_CODE_BYTES: usize = 96;

/// A stable, bounded, ASCII protocol reason code.
pub type ReasonCode = AsciiText<MAX_REASON_CODE_BYTES>;

/// Explicit bounds for strict receipt decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiptDecodeLimits {
    /// Maximum bytes in one complete receipt or rejection receipt.
    pub max_input_bytes: u64,
    /// Maximum bytes in one encoded candidate body.
    pub max_body_bytes: u64,
}

impl Default for ReceiptDecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024,
            max_body_bytes: 16 * 1024,
        }
    }
}

/// Explicit bounds for strict complete commit-bundle decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleDecodeLimits {
    /// Maximum bytes in one complete encoded bundle.
    pub max_input_bytes: u64,
    /// Receipt and candidate-body limits.
    pub receipt: ReceiptDecodeLimits,
    /// Canonical patch limits.
    pub patch: PatchDecodeLimits,
    /// Canonical authoritative-plan limits.
    pub commit_plan: PlanDecodeLimits,
    /// Canonical outbox-plan limits.
    pub outbox_plan: PlanDecodeLimits,
}

impl Default for BundleDecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 128 * 1024 * 1024,
            receipt: ReceiptDecodeLimits::default(),
            patch: PatchDecodeLimits::default(),
            commit_plan: PlanDecodeLimits::default(),
            outbox_plan: PlanDecodeLimits::default(),
        }
    }
}

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

/// Strictly decodes one candidate receipt and verifies its candidate identity.
///
/// The decoder reconstructs the candidate body, recomputes the candidate
/// commitment with the selected provider, and requires exact canonical
/// re-encoding of the complete input.
pub fn decode_receipt<H: CommitmentHasher>(
    bytes: &[u8],
    limits: ReceiptDecodeLimits,
) -> Result<Receipt, ReceiptDecodeError> {
    enforce_input_limit(bytes, limits.max_input_bytes)?;
    let mut cursor = ReceiptCursor::new(bytes);
    let candidate_id = CandidateId::new(cursor.take_hash32()?);
    let encoded_body = cursor.take_blob()?;
    enforce_component_limit(
        ReceiptComponent::CandidateBody,
        encoded_body,
        limits.max_body_bytes,
    )?;
    ensure_consumed(&cursor)?;
    let body = decode_candidate_body(encoded_body)?;
    let expected = CandidateId::new(
        hash_component::<H>("zeno-fcis/candidate", &body).map_err(ReceiptDecodeError::Seal)?,
    );
    if candidate_id != expected {
        return Err(ReceiptDecodeError::CandidateMismatch);
    }
    let receipt = Receipt { candidate_id, body };
    ensure_canonical(bytes, &receipt)?;
    Ok(receipt)
}

/// Strictly decodes one unchanged-state rejection receipt.
pub fn decode_reject_receipt(
    bytes: &[u8],
    limits: ReceiptDecodeLimits,
) -> Result<RejectReceipt, ReceiptDecodeError> {
    enforce_input_limit(bytes, limits.max_input_bytes)?;
    let mut cursor = ReceiptCursor::new(bytes);
    let bindings = cursor.take_bindings()?;
    let pre_root = cursor.take_hash32()?;
    let reason = decode_reason(cursor.take_blob()?)?;
    ensure_consumed(&cursor)?;
    let receipt =
        RejectReceipt::new(bindings, pre_root, &reason).map_err(ReceiptDecodeError::Seal)?;
    ensure_canonical(bytes, &receipt)?;
    Ok(receipt)
}

/// Strictly decodes and fully reconstructs one complete committed candidate.
///
/// Nested patches and plans pass through their own bounded canonical decoders.
/// The result is then rebuilt through [`CandidateBuilder::seal`] against the
/// supplied pre-state and state domain. No decoded wire field directly creates
/// a trusted bundle.
pub fn decode_commit_bundle<H: CommitmentHasher>(
    bytes: &[u8],
    pre_state: &Value,
    state_domain: Domain<'_>,
    limits: BundleDecodeLimits,
) -> Result<CommitBundle, ReceiptDecodeError> {
    enforce_input_limit(bytes, limits.max_input_bytes)?;
    let mut cursor = ReceiptCursor::new(bytes);
    let candidate_id = CandidateId::new(cursor.take_hash32()?);
    let encoded_body = cursor.take_blob()?;
    enforce_component_limit(
        ReceiptComponent::CandidateBody,
        encoded_body,
        limits.receipt.max_body_bytes,
    )?;
    let encoded_patch = cursor.take_blob()?;
    let encoded_commit_plan = cursor.take_blob()?;
    let encoded_outbox_plan = cursor.take_blob()?;
    let encoded_receipt = cursor.take_blob()?;
    enforce_component_limit(
        ReceiptComponent::Receipt,
        encoded_receipt,
        limits.receipt.max_input_bytes,
    )?;
    ensure_consumed(&cursor)?;

    let body = decode_candidate_body(encoded_body)?;
    let patch =
        decode_canonical_patch(encoded_patch, limits.patch).map_err(ReceiptDecodeError::Patch)?;
    let commit_plan = decode_commit_plan(encoded_commit_plan, limits.commit_plan)
        .map_err(ReceiptDecodeError::Plan)?;
    let outbox_plan = decode_outbox_plan(encoded_outbox_plan, limits.outbox_plan)
        .map_err(ReceiptDecodeError::Plan)?;
    let receipt = decode_receipt::<H>(encoded_receipt, limits.receipt)?;

    let rebuilt = CandidateBuilder::seal::<H>(
        pre_state,
        state_domain,
        body.decision_kind,
        body.reason_code.as_ref().map(AsciiText::as_str),
        body.bindings,
        patch,
        commit_plan,
        outbox_plan,
    )
    .map_err(ReceiptDecodeError::Seal)?;
    if rebuilt.candidate_id != candidate_id || rebuilt.body != body {
        return Err(ReceiptDecodeError::CandidateMismatch);
    }
    if rebuilt.receipt != receipt {
        return Err(ReceiptDecodeError::ReceiptMismatch);
    }
    ensure_canonical(bytes, &rebuilt)?;
    Ok(rebuilt)
}

fn decode_candidate_body(bytes: &[u8]) -> Result<CandidateBody, ReceiptDecodeError> {
    let mut cursor = ReceiptCursor::new(bytes);
    let decision_kind = decode_decision(cursor.take_u8()?)?;
    let reason = match cursor.take_u8()? {
        0 => None,
        1 => Some(decode_reason(cursor.take_blob()?)?),
        flag => return Err(ReceiptDecodeError::InvalidReasonFlag(flag)),
    };
    let bindings = cursor.take_bindings()?;
    let body = CandidateBody {
        decision_kind,
        reason_code: validate_decision_reason(decision_kind, reason.as_deref())
            .map_err(ReceiptDecodeError::Seal)?,
        bindings,
        pre_root: cursor.take_hash32()?,
        post_root: cursor.take_hash32()?,
        patch_hash: cursor.take_hash32()?,
        commit_plan_hash: cursor.take_hash32()?,
        outbox_plan_hash: cursor.take_hash32()?,
    };
    ensure_consumed(&cursor)?;
    ensure_canonical(bytes, &body)?;
    Ok(body)
}

fn decode_decision(tag: u8) -> Result<DecisionKind, ReceiptDecodeError> {
    match tag {
        0 => Ok(DecisionKind::Accept),
        1 => Ok(DecisionKind::Reject),
        2 => Ok(DecisionKind::CommittedFailure),
        other => Err(ReceiptDecodeError::UnknownDecisionTag(other)),
    }
}

fn decode_reason(bytes: &[u8]) -> Result<String, ReceiptDecodeError> {
    let text = core::str::from_utf8(bytes).map_err(|_| ReceiptDecodeError::InvalidReasonText)?;
    if !text.is_ascii() {
        return Err(ReceiptDecodeError::InvalidReasonText);
    }
    Ok(text.into())
}

fn enforce_input_limit(bytes: &[u8], limit: u64) -> Result<(), ReceiptDecodeError> {
    let actual = u64::try_from(bytes.len()).map_err(|_| ReceiptDecodeError::LengthOverflow)?;
    if actual > limit {
        return Err(ReceiptDecodeError::InputLimit { limit, actual });
    }
    Ok(())
}

fn enforce_component_limit(
    component: ReceiptComponent,
    bytes: &[u8],
    limit: u64,
) -> Result<(), ReceiptDecodeError> {
    let actual = u64::try_from(bytes.len()).map_err(|_| ReceiptDecodeError::LengthOverflow)?;
    if actual > limit {
        return Err(ReceiptDecodeError::ComponentLimit {
            component,
            limit,
            actual,
        });
    }
    Ok(())
}

fn ensure_consumed(cursor: &ReceiptCursor<'_>) -> Result<(), ReceiptDecodeError> {
    if cursor.remaining() != 0 {
        return Err(ReceiptDecodeError::TrailingBytes {
            offset: cursor.offset,
        });
    }
    Ok(())
}

fn ensure_canonical<T: CanonicalEncode>(bytes: &[u8], value: &T) -> Result<(), ReceiptDecodeError> {
    let encoded = value
        .canonical_bytes()
        .map_err(ReceiptDecodeError::Encode)?;
    if encoded.as_slice() != bytes {
        return Err(ReceiptDecodeError::NonCanonical);
    }
    Ok(())
}

struct ReceiptCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ReceiptCursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ReceiptDecodeError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(ReceiptDecodeError::LengthOverflow)?;
        let Some(bytes) = self.bytes.get(self.offset..end) else {
            return Err(ReceiptDecodeError::UnexpectedEnd {
                offset: self.offset,
                requested: count,
            });
        };
        self.offset = end;
        Ok(bytes)
    }

    fn take_u8(&mut self) -> Result<u8, ReceiptDecodeError> {
        Ok(self.take(1)?[0])
    }

    fn take_u32(&mut self) -> Result<u32, ReceiptDecodeError> {
        let mut bytes = [0_u8; 4];
        bytes.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(bytes))
    }

    fn take_hash32(&mut self) -> Result<Hash32, ReceiptDecodeError> {
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(self.take(32)?);
        Ok(Hash32::new(bytes))
    }

    fn take_blob(&mut self) -> Result<&'a [u8], ReceiptDecodeError> {
        let length =
            usize::try_from(self.take_u32()?).map_err(|_| ReceiptDecodeError::LengthOverflow)?;
        self.take(length)
    }

    fn take_bindings(&mut self) -> Result<CandidateBindings, ReceiptDecodeError> {
        Ok(CandidateBindings {
            profile_hash: self.take_hash32()?,
            command_hash: self.take_hash32()?,
            context_hash: self.take_hash32()?,
            precedence_hash: self.take_hash32()?,
            algorithm_hash: self.take_hash32()?,
            budget_hash: self.take_hash32()?,
        })
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

/// Bounded canonical receipt or bundle decoding failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReceiptDecodeError {
    /// Complete input exceeds the declared byte limit.
    InputLimit {
        /// Configured limit.
        limit: u64,
        /// Actual bytes.
        actual: u64,
    },
    /// One length-delimited component exceeds its declared byte limit.
    ComponentLimit {
        /// Component whose bytes exceeded the bound.
        component: ReceiptComponent,
        /// Configured limit.
        limit: u64,
        /// Actual bytes.
        actual: u64,
    },
    /// A checked length conversion or offset addition overflowed.
    LengthOverflow,
    /// Input ended before a declared field was complete.
    UnexpectedEnd {
        /// Byte offset at the failed read.
        offset: usize,
        /// Requested bytes.
        requested: usize,
    },
    /// A decision tag is outside the closed decision algebra.
    UnknownDecisionTag(u8),
    /// A reason-presence flag is not canonical.
    InvalidReasonFlag(u8),
    /// Reason bytes are not valid bounded ASCII text.
    InvalidReasonText,
    /// Bytes remain after the complete value.
    TrailingBytes {
        /// First trailing-byte offset.
        offset: usize,
    },
    /// Reconstructed canonical bytes differ from the input.
    NonCanonical,
    /// Candidate identity or body differs from the reconstructed candidate.
    CandidateMismatch,
    /// Nested receipt differs from the reconstructed candidate receipt.
    ReceiptMismatch,
    /// Nested canonical patch decoding failed.
    Patch(PatchDecodeError),
    /// Nested canonical plan decoding failed.
    Plan(PlanDecodeError),
    /// Candidate sealing or validation failed.
    Seal(SealError),
    /// Canonical encoding failed.
    Encode(EncodeError),
}

/// Length-delimited receipt component used in deterministic diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptComponent {
    /// Candidate body bytes.
    CandidateBody,
    /// Nested receipt bytes.
    Receipt,
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

impl fmt::Display for ReceiptDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputLimit { limit, actual } => {
                write!(formatter, "receipt input exceeds {limit} bytes: {actual}")
            }
            Self::ComponentLimit {
                component,
                limit,
                actual,
            } => write!(
                formatter,
                "{component:?} exceeds {limit} encoded bytes: {actual}"
            ),
            Self::LengthOverflow => formatter.write_str("receipt length arithmetic overflowed"),
            Self::UnexpectedEnd { offset, requested } => write!(
                formatter,
                "receipt ended at byte {offset} while reading {requested} bytes"
            ),
            Self::UnknownDecisionTag(tag) => write!(formatter, "unknown decision tag {tag}"),
            Self::InvalidReasonFlag(flag) => write!(formatter, "invalid reason flag {flag}"),
            Self::InvalidReasonText => {
                formatter.write_str("reason is not valid bounded ASCII text")
            }
            Self::TrailingBytes { offset } => {
                write!(formatter, "trailing receipt bytes at offset {offset}")
            }
            Self::NonCanonical => formatter.write_str("receipt bytes are not canonical"),
            Self::CandidateMismatch => {
                formatter.write_str("candidate identity or body does not reconstruct")
            }
            Self::ReceiptMismatch => {
                formatter.write_str("nested receipt does not match the candidate")
            }
            Self::Patch(error) => error.fmt(formatter),
            Self::Plan(error) => error.fmt(formatter),
            Self::Seal(error) => error.fmt(formatter),
            Self::Encode(error) => error.fmt(formatter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use zeno_fcis_patch::{PatchOp, ValuePath};
    use zeno_fcis_plan::{Effect, OutboxEntry};
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

    fn candidate_bundle() -> CommitBundle {
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
        let commit_plan = CommitPlan::try_new(vec![Effect::new(
            0,
            7,
            Hash32::new([7; 32]),
            Hash32::new([8; 32]),
            Value::U128(9),
        )])
        .unwrap_or_else(|error| panic!("commit plan: {error}"));
        let outbox_plan =
            OutboxPlan::try_new(vec![OutboxEntry::new(0, 3, Value::U128(4), Value::U128(5))])
                .unwrap_or_else(|error| panic!("outbox plan: {error}"));
        CandidateBuilder::seal::<TestHasher>(
            &state,
            domain(),
            DecisionKind::Accept,
            None,
            bindings(),
            patch,
            commit_plan,
            outbox_plan,
        )
        .unwrap_or_else(|error| panic!("seal: {error}"))
    }

    fn empty_state() -> Value {
        Value::Record(Vec::<Field>::new().into_boxed_slice())
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

    #[test]
    fn strict_decoders_round_trip_complete_artifacts_at_exact_limits() {
        let bundle = candidate_bundle();
        let bundle_bytes = bundle
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("bundle bytes: {error}"));
        let receipt_bytes = bundle
            .receipt()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("receipt bytes: {error}"));
        let receipt_limits = ReceiptDecodeLimits {
            max_input_bytes: u64::try_from(receipt_bytes.len()).unwrap_or(u64::MAX),
            max_body_bytes: u64::try_from(
                bundle.body().canonical_bytes().unwrap_or_default().len(),
            )
            .unwrap_or(u64::MAX),
        };
        let decoded_receipt = decode_receipt::<TestHasher>(&receipt_bytes, receipt_limits);
        assert_eq!(decoded_receipt, Ok(bundle.receipt().clone()));

        let limits = BundleDecodeLimits {
            max_input_bytes: u64::try_from(bundle_bytes.len()).unwrap_or(u64::MAX),
            receipt: receipt_limits,
            patch: PatchDecodeLimits {
                max_input_bytes: u64::try_from(
                    bundle.patch().canonical_bytes().unwrap_or_default().len(),
                )
                .unwrap_or(u64::MAX),
                ..PatchDecodeLimits::default()
            },
            commit_plan: PlanDecodeLimits {
                max_input_bytes: u64::try_from(
                    bundle
                        .commit_plan()
                        .canonical_bytes()
                        .unwrap_or_default()
                        .len(),
                )
                .unwrap_or(u64::MAX),
                ..PlanDecodeLimits::default()
            },
            outbox_plan: PlanDecodeLimits {
                max_input_bytes: u64::try_from(
                    bundle
                        .outbox_plan()
                        .canonical_bytes()
                        .unwrap_or_default()
                        .len(),
                )
                .unwrap_or(u64::MAX),
                ..PlanDecodeLimits::default()
            },
        };
        let decoded =
            decode_commit_bundle::<TestHasher>(&bundle_bytes, &empty_state(), domain(), limits);
        assert_eq!(decoded, Ok(bundle));
    }

    #[test]
    fn strict_bundle_decoder_rejects_candidate_receipt_and_state_substitution() {
        let bundle = candidate_bundle();
        let bytes = bundle
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("bundle bytes: {error}"));
        let mut changed_candidate = bytes.clone();
        changed_candidate[0] ^= 1;
        assert_eq!(
            decode_commit_bundle::<TestHasher>(
                &changed_candidate,
                &empty_state(),
                domain(),
                BundleDecodeLimits::default(),
            ),
            Err(ReceiptDecodeError::CandidateMismatch)
        );

        let mut changed_receipt = bytes.clone();
        let last = changed_receipt.len().saturating_sub(1);
        changed_receipt[last] ^= 1;
        assert!(matches!(
            decode_commit_bundle::<TestHasher>(
                &changed_receipt,
                &empty_state(),
                domain(),
                BundleDecodeLimits::default(),
            ),
            Err(ReceiptDecodeError::CandidateMismatch)
                | Err(ReceiptDecodeError::ReceiptMismatch)
                | Err(ReceiptDecodeError::NonCanonical)
        ));

        assert!(matches!(
            decode_commit_bundle::<TestHasher>(
                &bytes,
                &Value::Unit,
                domain(),
                BundleDecodeLimits::default(),
            ),
            Err(ReceiptDecodeError::Seal(SealError::Patch(_)))
        ));
    }

    #[test]
    fn strict_bundle_decoder_enforces_nested_and_outer_bounds() {
        let bundle = candidate_bundle();
        let bytes = bundle
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("bundle bytes: {error}"));
        let actual = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        assert_eq!(
            decode_commit_bundle::<TestHasher>(
                &bytes,
                &empty_state(),
                domain(),
                BundleDecodeLimits {
                    max_input_bytes: actual.saturating_sub(1),
                    ..BundleDecodeLimits::default()
                },
            ),
            Err(ReceiptDecodeError::InputLimit {
                limit: actual.saturating_sub(1),
                actual,
            })
        );
        assert!(matches!(
            decode_commit_bundle::<TestHasher>(
                &bytes,
                &empty_state(),
                domain(),
                BundleDecodeLimits {
                    patch: PatchDecodeLimits {
                        max_operations: 0,
                        ..PatchDecodeLimits::default()
                    },
                    ..BundleDecodeLimits::default()
                },
            ),
            Err(ReceiptDecodeError::Patch(
                PatchDecodeError::OperationLimit { .. }
            ))
        ));
    }

    #[test]
    fn strict_decoders_reject_malformed_flags_trailing_bytes_and_truncation() {
        let bundle = candidate_bundle();
        let mut receipt = bundle
            .receipt()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("receipt bytes: {error}"));
        let body_offset = 32 + 4;
        receipt[body_offset + 1] = 2;
        assert_eq!(
            decode_receipt::<TestHasher>(&receipt, ReceiptDecodeLimits::default()),
            Err(ReceiptDecodeError::InvalidReasonFlag(2))
        );

        let mut trailing = bundle
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("bundle bytes: {error}"));
        trailing.push(0);
        assert!(matches!(
            decode_commit_bundle::<TestHasher>(
                &trailing,
                &empty_state(),
                domain(),
                BundleDecodeLimits::default(),
            ),
            Err(ReceiptDecodeError::TrailingBytes { .. })
        ));
        trailing.truncate(31);
        assert!(matches!(
            decode_commit_bundle::<TestHasher>(
                &trailing,
                &empty_state(),
                domain(),
                BundleDecodeLimits::default(),
            ),
            Err(ReceiptDecodeError::UnexpectedEnd { .. })
        ));
    }

    #[test]
    fn strict_reject_receipt_decoder_round_trips_and_rejects_non_ascii() {
        let receipt = RejectReceipt::new(bindings(), Hash32::new([9; 32]), "invalid_command")
            .unwrap_or_else(|error| panic!("receipt: {error}"));
        let bytes = receipt
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("receipt bytes: {error}"));
        assert_eq!(
            decode_reject_receipt(&bytes, ReceiptDecodeLimits::default()),
            Ok(receipt)
        );
        let mut invalid = bytes;
        let last = invalid.len().saturating_sub(1);
        invalid[last] = 0xff;
        assert_eq!(
            decode_reject_receipt(&invalid, ReceiptDecodeLimits::default()),
            Err(ReceiptDecodeError::InvalidReasonText)
        );
    }
}
