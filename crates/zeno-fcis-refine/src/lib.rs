//! Exact runtime-to-model refinement and proof-assisted promotion evidence.
//!
//! A mounted runtime is normalized into the same data-only decision surface as
//! the model. [`NormalizedDecision`] is an untrusted transport and diagnostic
//! value. Production promotion first reconstructs it as a privately
//! constructible [`ValidatedNormalizedDecision`] against the exact invocation,
//! state, domain, and approved provider. Refinement then compares every
//! authority-bearing artifact, not merely the final state root. Promotion
//! evidence is fail-closed and content-bound; an external [`ProofVerifier`]
//! remains responsible for validating proof artifacts under pinned tool
//! semantics.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::DecisionKind;
use zeno_fcis_crypto::{ApprovedCommitmentProvider, ApprovedProviderId, VerifiedProvider};
use zeno_fcis_patch::{PatchError, hash_value};
use zeno_fcis_receipt::{
    BundleDecodeLimits, CandidateBindings, CommitBundle, ReceiptDecodeError, ReceiptDecodeLimits,
    RejectReceipt, decode_commit_bundle, decode_reject_receipt,
};
use zeno_fcis_value::Value;

const MAX_REASON_BYTES: usize = 96;
const MAX_ARTIFACT_BYTES: usize = 64 * 1024 * 1024;
const MAX_CASES: usize = 1_000_000;
const MAX_TOOL_EVIDENCE: usize = 64;
const REFINEMENT_PROTOCOL_VERSION: u16 = 1;

/// Complete untrusted transport surface for one decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionArtifacts {
    /// Semantic decision kind.
    pub kind: DecisionKind,
    /// Stable rejection or committed-failure reason.
    pub reason_code: Option<Box<str>>,
    /// Protocol/profile identity.
    pub profile_hash: Hash32,
    /// Authenticated command commitment.
    pub command_hash: Hash32,
    /// Context, policy, and evidence commitment.
    pub context_hash: Hash32,
    /// Stable precedence profile.
    pub precedence_hash: Hash32,
    /// Algorithm and codec versions.
    pub algorithm_hash: Hash32,
    /// Deterministic budget limits and consumption.
    pub budget_hash: Hash32,
    /// Pre-state root.
    pub pre_root: Hash32,
    /// Post-state root.
    pub post_root: Hash32,
    /// Candidate identity for accepted or committed-failure decisions.
    pub candidate_id: Option<Hash32>,
    /// Canonical patch bytes.
    pub patch_bytes: Option<Box<[u8]>>,
    /// Canonical authoritative plan bytes.
    pub commit_plan_bytes: Option<Box<[u8]>>,
    /// Canonical outbox plan bytes.
    pub outbox_plan_bytes: Option<Box<[u8]>>,
    /// Canonical receipt bytes.
    pub receipt_bytes: Box<[u8]>,
    /// Canonical complete bundle bytes.
    pub bundle_bytes: Option<Box<[u8]>>,
}

/// Bounded untrusted normalized decision suitable for diagnostic comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedDecision {
    artifacts: DecisionArtifacts,
}

impl NormalizedDecision {
    /// Validates one normalized decision surface.
    pub fn try_new(artifacts: DecisionArtifacts) -> Result<Self, RefineError> {
        validate_reason(artifacts.kind, artifacts.reason_code.as_deref())?;
        for bytes in [
            artifacts.patch_bytes.as_deref(),
            artifacts.commit_plan_bytes.as_deref(),
            artifacts.outbox_plan_bytes.as_deref(),
            Some(artifacts.receipt_bytes.as_ref()),
            artifacts.bundle_bytes.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if bytes.len() > MAX_ARTIFACT_BYTES {
                return Err(RefineError::ArtifactTooLarge);
            }
        }
        match artifacts.kind {
            DecisionKind::Reject => {
                if artifacts.pre_root != artifacts.post_root {
                    return Err(RefineError::RejectedStateChanged);
                }
                if artifacts.candidate_id.is_some()
                    || artifacts.patch_bytes.is_some()
                    || artifacts.commit_plan_bytes.is_some()
                    || artifacts.outbox_plan_bytes.is_some()
                    || artifacts.bundle_bytes.is_some()
                {
                    return Err(RefineError::RejectedCandidatePresent);
                }
            }
            DecisionKind::Accept | DecisionKind::CommittedFailure => {
                if artifacts.candidate_id.is_none()
                    || artifacts.patch_bytes.is_none()
                    || artifacts.commit_plan_bytes.is_none()
                    || artifacts.outbox_plan_bytes.is_none()
                    || artifacts.bundle_bytes.is_none()
                {
                    return Err(RefineError::CommittedArtifactMissing);
                }
            }
        }
        Ok(Self { artifacts })
    }

    /// Normalizes one sealed accepted or committed-failure bundle.
    pub fn from_bundle(bundle: &CommitBundle) -> Result<Self, RefineError> {
        let body = bundle.body();
        let bindings = body.bindings();
        Self::try_new(DecisionArtifacts {
            kind: body.decision_kind(),
            reason_code: body
                .reason_code()
                .map(|reason| Box::<str>::from(reason.as_str())),
            profile_hash: bindings.profile_hash,
            command_hash: bindings.command_hash,
            context_hash: bindings.context_hash,
            precedence_hash: bindings.precedence_hash,
            algorithm_hash: bindings.algorithm_hash,
            budget_hash: bindings.budget_hash,
            pre_root: body.pre_root(),
            post_root: body.post_root(),
            candidate_id: Some(bundle.candidate_id().hash()),
            patch_bytes: Some(
                bundle
                    .patch()
                    .canonical_bytes()
                    .map_err(RefineError::Encode)?
                    .into_boxed_slice(),
            ),
            commit_plan_bytes: Some(
                bundle
                    .commit_plan()
                    .canonical_bytes()
                    .map_err(RefineError::Encode)?
                    .into_boxed_slice(),
            ),
            outbox_plan_bytes: Some(
                bundle
                    .outbox_plan()
                    .canonical_bytes()
                    .map_err(RefineError::Encode)?
                    .into_boxed_slice(),
            ),
            receipt_bytes: bundle
                .receipt()
                .canonical_bytes()
                .map_err(RefineError::Encode)?
                .into_boxed_slice(),
            bundle_bytes: Some(
                bundle
                    .canonical_bytes()
                    .map_err(RefineError::Encode)?
                    .into_boxed_slice(),
            ),
        })
    }

    /// Normalizes an unchanged-state rejection receipt.
    pub fn from_reject(receipt: &RejectReceipt) -> Result<Self, RefineError> {
        let bindings = receipt.bindings();
        Self::try_new(DecisionArtifacts {
            kind: DecisionKind::Reject,
            reason_code: Some(Box::from(receipt.reason_code().as_str())),
            profile_hash: bindings.profile_hash,
            command_hash: bindings.command_hash,
            context_hash: bindings.context_hash,
            precedence_hash: bindings.precedence_hash,
            algorithm_hash: bindings.algorithm_hash,
            budget_hash: bindings.budget_hash,
            pre_root: receipt.pre_root(),
            post_root: receipt.post_root(),
            candidate_id: None,
            patch_bytes: None,
            commit_plan_bytes: None,
            outbox_plan_bytes: None,
            receipt_bytes: receipt
                .canonical_bytes()
                .map_err(RefineError::Encode)?
                .into_boxed_slice(),
            bundle_bytes: None,
        })
    }

    /// Returns normalized artifacts.
    #[must_use]
    pub const fn artifacts(&self) -> &DecisionArtifacts {
        &self.artifacts
    }
}

impl CanonicalEncode for NormalizedDecision {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(decision_tag(self.artifacts.kind));
        put_optional_text(output, self.artifacts.reason_code.as_deref())?;
        for hash in [
            self.artifacts.profile_hash,
            self.artifacts.command_hash,
            self.artifacts.context_hash,
            self.artifacts.precedence_hash,
            self.artifacts.algorithm_hash,
            self.artifacts.budget_hash,
            self.artifacts.pre_root,
            self.artifacts.post_root,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        match self.artifacts.candidate_id {
            None => output.push(0),
            Some(hash) => {
                output.push(1);
                output.extend_from_slice(hash.as_bytes());
            }
        }
        put_optional_blob(output, self.artifacts.patch_bytes.as_deref())?;
        put_optional_blob(output, self.artifacts.commit_plan_bytes.as_deref())?;
        put_optional_blob(output, self.artifacts.outbox_plan_bytes.as_deref())?;
        put_blob(output, &self.artifacts.receipt_bytes)?;
        put_optional_blob(output, self.artifacts.bundle_bytes.as_deref())
    }
}

/// Explicit nested bounds for strict decision reconstruction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DecisionValidationLimits {
    /// Strict rejection-receipt limits.
    pub receipt: ReceiptDecodeLimits,
    /// Strict complete commit-bundle limits.
    pub bundle: BundleDecodeLimits,
}

/// Exact externally supplied invocation, state, domain, and provider binding.
///
/// This value is constructed only as a consequence of successful strict
/// decision reconstruction. It cannot turn arbitrary transport bytes into a
/// validated decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionValidationBinding {
    bindings: CandidateBindings,
    pre_root: Hash32,
    state_domain_name: Box<str>,
    state_domain_version: u16,
    provider_id: ApprovedProviderId,
}

impl DecisionValidationBinding {
    /// Returns the exact candidate bindings expected by the caller.
    #[must_use]
    pub const fn bindings(&self) -> CandidateBindings {
        self.bindings
    }

    /// Returns the root of the exact supplied pre-state.
    #[must_use]
    pub const fn pre_root(&self) -> Hash32 {
        self.pre_root
    }

    /// Returns the exact state-domain name.
    #[must_use]
    pub fn state_domain_name(&self) -> &str {
        &self.state_domain_name
    }

    /// Returns the exact state-domain version.
    #[must_use]
    pub const fn state_domain_version(&self) -> u16 {
        self.state_domain_version
    }

    /// Returns the nominal approved-provider identity.
    #[must_use]
    pub const fn provider_id(&self) -> ApprovedProviderId {
        self.provider_id
    }
}

impl CanonicalEncode for DecisionValidationBinding {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&REFINEMENT_PROTOCOL_VERSION.to_be_bytes());
        encode_candidate_bindings(self.bindings, output);
        output.extend_from_slice(self.pre_root.as_bytes());
        put_blob(output, self.state_domain_name.as_bytes())?;
        output.extend_from_slice(&self.state_domain_version.to_be_bytes());
        output.extend_from_slice(&self.provider_id.code().to_be_bytes());
        Ok(())
    }
}

/// A complete normalized decision reconstructed against external authority.
///
/// The fields are private and there is no raw constructor. Callers must supply
/// the exact pre-state, invocation bindings, state domain, approved provider,
/// and decoder limits to [`ValidatedNormalizedDecision::try_from_untrusted`].
///
/// ```compile_fail
/// use zeno_fcis_refine::{
///     DecisionValidationBinding, NormalizedDecision, ValidatedNormalizedDecision,
/// };
///
/// fn forge(
///     decision: NormalizedDecision,
///     validation: DecisionValidationBinding,
/// ) -> ValidatedNormalizedDecision {
///     ValidatedNormalizedDecision { decision, validation }
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedNormalizedDecision {
    decision: NormalizedDecision,
    validation: DecisionValidationBinding,
}

impl ValidatedNormalizedDecision {
    /// Strictly reconstructs every authority-bearing artifact.
    pub fn try_from_untrusted<H: ApprovedCommitmentProvider>(
        untrusted: NormalizedDecision,
        pre_state: &Value,
        state_domain: Domain<'_>,
        expected_bindings: CandidateBindings,
        limits: DecisionValidationLimits,
        provider: &VerifiedProvider<H>,
    ) -> Result<Self, RefineError> {
        let expected_pre_root =
            hash_value::<H>(state_domain, pre_state).map_err(RefineError::Patch)?;
        if untrusted.artifacts().pre_root != expected_pre_root {
            return Err(RefineError::UnexpectedPreRoot);
        }

        let rebuilt = match untrusted.artifacts().kind {
            DecisionKind::Reject => {
                let receipt =
                    decode_reject_receipt(&untrusted.artifacts().receipt_bytes, limits.receipt)
                        .map_err(RefineError::ReceiptDecode)?;
                if receipt.bindings() != expected_bindings {
                    return Err(RefineError::InvocationBindingMismatch);
                }
                if receipt.pre_root() != expected_pre_root {
                    return Err(RefineError::UnexpectedPreRoot);
                }
                NormalizedDecision::from_reject(&receipt)?
            }
            DecisionKind::Accept | DecisionKind::CommittedFailure => {
                let bundle_bytes = untrusted
                    .artifacts()
                    .bundle_bytes
                    .as_deref()
                    .ok_or(RefineError::CommittedArtifactMissing)?;
                let bundle =
                    decode_commit_bundle::<H>(bundle_bytes, pre_state, state_domain, limits.bundle)
                        .map_err(RefineError::ReceiptDecode)?;
                if bundle.body().bindings() != expected_bindings {
                    return Err(RefineError::InvocationBindingMismatch);
                }
                NormalizedDecision::from_bundle(&bundle)?
            }
        };
        if rebuilt != untrusted {
            return Err(RefineError::ArtifactReconstructionMismatch);
        }

        Ok(Self {
            decision: untrusted,
            validation: DecisionValidationBinding {
                bindings: expected_bindings,
                pre_root: expected_pre_root,
                state_domain_name: Box::from(state_domain.name()),
                state_domain_version: state_domain.version(),
                provider_id: provider.provider_id(),
            },
        })
    }

    /// Returns the reconstructed normalized decision.
    #[must_use]
    pub const fn decision(&self) -> &NormalizedDecision {
        &self.decision
    }

    /// Returns the exact external validation binding.
    #[must_use]
    pub const fn validation(&self) -> &DecisionValidationBinding {
        &self.validation
    }
}

impl CanonicalEncode for ValidatedNormalizedDecision {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, &self.validation.canonical_bytes()?)?;
        put_blob(output, &self.decision.canonical_bytes()?)
    }
}

/// Compares complete decisions only after both passed strict reconstruction.
#[must_use]
pub fn compare_validated_exact(
    model: &ValidatedNormalizedDecision,
    runtime: &ValidatedNormalizedDecision,
) -> RefinementReport {
    compare_exact(model.decision(), runtime.decision())
}

/// One exact runtime/model disagreement.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Mismatch {
    /// Decision kind differs.
    DecisionKind,
    /// Stable reason differs.
    ReasonCode,
    /// Profile differs.
    ProfileHash,
    /// Command binding differs.
    CommandHash,
    /// Context binding differs.
    ContextHash,
    /// Precedence profile differs.
    PrecedenceHash,
    /// Algorithm profile differs.
    AlgorithmHash,
    /// Budget binding differs.
    BudgetHash,
    /// Pre-root differs.
    PreRoot,
    /// Post-root differs.
    PostRoot,
    /// Candidate identity differs.
    CandidateId,
    /// Patch bytes differ.
    PatchBytes,
    /// Commit-plan bytes differ.
    CommitPlanBytes,
    /// Outbox-plan bytes differ.
    OutboxPlanBytes,
    /// Receipt bytes differ.
    ReceiptBytes,
    /// Complete bundle bytes differ.
    BundleBytes,
}

/// Exact refinement comparison result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefinementReport {
    mismatches: Box<[Mismatch]>,
}

impl RefinementReport {
    /// Returns whether the runtime exactly refines the compared model result.
    #[must_use]
    pub fn is_exact(&self) -> bool {
        self.mismatches.is_empty()
    }

    /// Returns every differing authority artifact.
    #[must_use]
    pub const fn mismatches(&self) -> &[Mismatch] {
        &self.mismatches
    }
}

/// Compares the complete normalized decision surface.
#[must_use]
pub fn compare_exact(model: &NormalizedDecision, runtime: &NormalizedDecision) -> RefinementReport {
    let left = model.artifacts();
    let right = runtime.artifacts();
    let mut mismatches = Vec::new();
    compare_field(
        &mut mismatches,
        left.kind,
        right.kind,
        Mismatch::DecisionKind,
    );
    compare_field(
        &mut mismatches,
        left.reason_code.as_deref(),
        right.reason_code.as_deref(),
        Mismatch::ReasonCode,
    );
    compare_field(
        &mut mismatches,
        left.profile_hash,
        right.profile_hash,
        Mismatch::ProfileHash,
    );
    compare_field(
        &mut mismatches,
        left.command_hash,
        right.command_hash,
        Mismatch::CommandHash,
    );
    compare_field(
        &mut mismatches,
        left.context_hash,
        right.context_hash,
        Mismatch::ContextHash,
    );
    compare_field(
        &mut mismatches,
        left.precedence_hash,
        right.precedence_hash,
        Mismatch::PrecedenceHash,
    );
    compare_field(
        &mut mismatches,
        left.algorithm_hash,
        right.algorithm_hash,
        Mismatch::AlgorithmHash,
    );
    compare_field(
        &mut mismatches,
        left.budget_hash,
        right.budget_hash,
        Mismatch::BudgetHash,
    );
    compare_field(
        &mut mismatches,
        left.pre_root,
        right.pre_root,
        Mismatch::PreRoot,
    );
    compare_field(
        &mut mismatches,
        left.post_root,
        right.post_root,
        Mismatch::PostRoot,
    );
    compare_field(
        &mut mismatches,
        left.candidate_id,
        right.candidate_id,
        Mismatch::CandidateId,
    );
    compare_field(
        &mut mismatches,
        left.patch_bytes.as_deref(),
        right.patch_bytes.as_deref(),
        Mismatch::PatchBytes,
    );
    compare_field(
        &mut mismatches,
        left.commit_plan_bytes.as_deref(),
        right.commit_plan_bytes.as_deref(),
        Mismatch::CommitPlanBytes,
    );
    compare_field(
        &mut mismatches,
        left.outbox_plan_bytes.as_deref(),
        right.outbox_plan_bytes.as_deref(),
        Mismatch::OutboxPlanBytes,
    );
    compare_field(
        &mut mismatches,
        left.receipt_bytes.as_ref(),
        right.receipt_bytes.as_ref(),
        Mismatch::ReceiptBytes,
    );
    compare_field(
        &mut mismatches,
        left.bundle_bytes.as_deref(),
        right.bundle_bytes.as_deref(),
        Mismatch::BundleBytes,
    );
    RefinementReport {
        mismatches: mismatches.into_boxed_slice(),
    }
}

fn compare_field<T: PartialEq>(
    mismatches: &mut Vec<Mismatch>,
    left: T,
    right: T,
    mismatch: Mismatch,
) {
    if left != right {
        mismatches.push(mismatch);
    }
}

/// One legacy mounted diagnostic refinement case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefinementCase {
    case_id: Hash32,
    input_hash: Hash32,
    model: NormalizedDecision,
    runtime: NormalizedDecision,
}

impl RefinementCase {
    /// Creates a case bound to exact input and decision artifacts.
    #[must_use]
    pub const fn new(
        case_id: Hash32,
        input_hash: Hash32,
        model: NormalizedDecision,
        runtime: NormalizedDecision,
    ) -> Self {
        Self {
            case_id,
            input_hash,
            model,
            runtime,
        }
    }

    /// Returns the case identifier.
    #[must_use]
    pub const fn case_id(&self) -> Hash32 {
        self.case_id
    }

    /// Returns the input commitment.
    #[must_use]
    pub const fn input_hash(&self) -> Hash32 {
        self.input_hash
    }

    /// Compares model and runtime results.
    #[must_use]
    pub fn report(&self) -> RefinementReport {
        compare_exact(&self.model, &self.runtime)
    }
}

/// One strictly reconstructed model/runtime comparison case.
///
/// Input and case identities are derived from the exact validation binding and
/// complete decisions. They are never accepted as unrelated caller values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRefinementCase {
    case_id: Hash32,
    input_hash: Hash32,
    model: ValidatedNormalizedDecision,
    runtime: ValidatedNormalizedDecision,
}

impl ValidatedRefinementCase {
    /// Creates one case from two decisions validated for the same invocation.
    pub fn try_new<H: ApprovedCommitmentProvider>(
        model: ValidatedNormalizedDecision,
        runtime: ValidatedNormalizedDecision,
        provider: &VerifiedProvider<H>,
    ) -> Result<Self, RefineError> {
        if model.validation() != runtime.validation() {
            return Err(RefineError::ValidationBindingMismatch);
        }
        if model.validation().provider_id() != provider.provider_id() {
            return Err(RefineError::ProviderBindingMismatch);
        }
        let input_hash = commitment::<H>(
            refinement_domain("zeno-fcis/refinement-input")?,
            &model.validation().canonical_bytes()?,
        )?;
        let mut case_body = Vec::new();
        case_body.extend_from_slice(input_hash.as_bytes());
        put_blob(&mut case_body, &model.canonical_bytes()?)?;
        put_blob(&mut case_body, &runtime.canonical_bytes()?)?;
        let case_id = commitment::<H>(refinement_domain("zeno-fcis/refinement-case")?, &case_body)?;
        Ok(Self {
            case_id,
            input_hash,
            model,
            runtime,
        })
    }

    /// Returns the derived case identity.
    #[must_use]
    pub const fn case_id(&self) -> Hash32 {
        self.case_id
    }

    /// Returns the derived exact-input identity.
    #[must_use]
    pub const fn input_hash(&self) -> Hash32 {
        self.input_hash
    }

    /// Returns the target profile.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.model.validation.bindings.profile_hash
    }

    /// Returns the nominal provider used for strict reconstruction.
    #[must_use]
    pub const fn provider_id(&self) -> ApprovedProviderId {
        self.model.validation.provider_id
    }

    /// Returns the strictly reconstructed model decision.
    #[must_use]
    pub const fn model(&self) -> &ValidatedNormalizedDecision {
        &self.model
    }

    /// Returns the strictly reconstructed runtime decision.
    #[must_use]
    pub const fn runtime(&self) -> &ValidatedNormalizedDecision {
        &self.runtime
    }

    /// Compares the two strictly reconstructed decisions.
    #[must_use]
    pub fn report(&self) -> RefinementReport {
        compare_validated_exact(&self.model, &self.runtime)
    }
}

impl CanonicalEncode for ValidatedRefinementCase {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.case_id.as_bytes());
        output.extend_from_slice(self.input_hash.as_bytes());
        put_blob(output, &self.model.canonical_bytes()?)?;
        put_blob(output, &self.runtime.canonical_bytes()?)
    }
}

/// Canonical finite-domain manifest for an exhaustive refinement claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExhaustiveDomainManifest {
    manifest_id: Hash32,
    provider_id: ApprovedProviderId,
    profile_hash: Hash32,
    domain_definition_hash: Hash32,
    enumeration_algorithm_hash: Hash32,
    toolchain_hash: Hash32,
    input_hashes: Box<[Hash32]>,
    empty_domain_claim: Option<Hash32>,
}

impl ExhaustiveDomainManifest {
    /// Creates a content-addressed finite-domain manifest.
    ///
    /// Inputs must already be in strictly increasing canonical order. Empty
    /// domains require an explicit nonzero profile-bound declaration; nonempty
    /// domains must not carry one.
    pub fn try_new<H: ApprovedCommitmentProvider>(
        profile_hash: Hash32,
        domain_definition_hash: Hash32,
        enumeration_algorithm_hash: Hash32,
        toolchain_hash: Hash32,
        input_hashes: Vec<Hash32>,
        empty_domain_claim: Option<Hash32>,
        provider: &VerifiedProvider<H>,
    ) -> Result<Self, RefineError> {
        if profile_hash == Hash32::ZERO
            || domain_definition_hash == Hash32::ZERO
            || enumeration_algorithm_hash == Hash32::ZERO
            || toolchain_hash == Hash32::ZERO
            || input_hashes.len() > MAX_CASES
            || input_hashes.windows(2).any(|pair| pair[0] >= pair[1])
            || (input_hashes.is_empty()
                && empty_domain_claim.is_none_or(|claim| claim == Hash32::ZERO))
            || (!input_hashes.is_empty() && empty_domain_claim.is_some())
        {
            return Err(RefineError::InvalidDomainManifest);
        }
        let mut manifest = Self {
            manifest_id: Hash32::ZERO,
            provider_id: provider.provider_id(),
            profile_hash,
            domain_definition_hash,
            enumeration_algorithm_hash,
            toolchain_hash,
            input_hashes: input_hashes.into_boxed_slice(),
            empty_domain_claim,
        };
        manifest.manifest_id = commitment::<H>(
            refinement_domain("zeno-fcis/exhaustive-domain")?,
            &manifest.body_bytes()?,
        )?;
        Ok(manifest)
    }

    fn body_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        output.extend_from_slice(&REFINEMENT_PROTOCOL_VERSION.to_be_bytes());
        output.extend_from_slice(&self.provider_id.code().to_be_bytes());
        for hash in [
            self.profile_hash,
            self.domain_definition_hash,
            self.enumeration_algorithm_hash,
            self.toolchain_hash,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        put_hashes(&mut output, &self.input_hashes)?;
        put_optional_hash(&mut output, self.empty_domain_claim);
        Ok(output)
    }

    /// Returns the derived manifest identity.
    #[must_use]
    pub const fn manifest_id(&self) -> Hash32 {
        self.manifest_id
    }

    /// Returns the exact target profile.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the reviewed finite-domain definition identity.
    #[must_use]
    pub const fn domain_definition_hash(&self) -> Hash32 {
        self.domain_definition_hash
    }

    /// Returns the exact enumeration algorithm identity.
    #[must_use]
    pub const fn enumeration_algorithm_hash(&self) -> Hash32 {
        self.enumeration_algorithm_hash
    }

    /// Returns the nominal provider that created this manifest identity.
    #[must_use]
    pub const fn provider_id(&self) -> ApprovedProviderId {
        self.provider_id
    }

    /// Returns the canonical exact input set.
    #[must_use]
    pub const fn input_hashes(&self) -> &[Hash32] {
        &self.input_hashes
    }

    /// Returns the derived exact cardinality.
    #[must_use]
    pub fn cardinality(&self) -> u64 {
        u64::try_from(self.input_hashes.len()).unwrap_or(u64::MAX)
    }

    /// Returns the optional explicit empty-domain declaration.
    #[must_use]
    pub const fn empty_domain_claim(&self) -> Option<Hash32> {
        self.empty_domain_claim
    }

    /// Returns the exact enumeration toolchain identity.
    #[must_use]
    pub const fn toolchain_hash(&self) -> Hash32 {
        self.toolchain_hash
    }
}

impl CanonicalEncode for ExhaustiveDomainManifest {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.manifest_id.as_bytes());
        output.extend_from_slice(&self.body_bytes()?);
        Ok(())
    }
}

/// Declared proof coverage for promotion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverageMode {
    /// Every input in a finite domain was replayed.
    Exhaustive {
        /// Commitment of the enumerated domain definition.
        domain_hash: Hash32,
        /// Exact domain cardinality.
        cardinality: u64,
    },
    /// A deterministic bounded case set was replayed without a completeness claim.
    Bounded {
        /// Maximum admitted case count.
        case_budget: u64,
    },
    /// A theorem covers the large or infinite domain beyond retained differential cases.
    ProofAssisted {
        /// Exact theorem statement commitment.
        theorem_claim: Hash32,
    },
}

/// Formal or differential evidence kind.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ToolKind {
    /// Z3 theorem result.
    Z3 = 0,
    /// CVC5 theorem result.
    Cvc5 = 1,
    /// Lean-checked theorem.
    Lean = 2,
    /// Kani bounded model-checking result.
    Kani = 3,
    /// Translation-validation result.
    TranslationValidation = 4,
    /// Cross-language canonical codec/root vectors.
    CodecVectors = 5,
    /// Mounted runtime-to-model differential evidence.
    RuntimeRefinement = 6,
    /// Independently checked exact finite-domain enumeration evidence.
    DomainEnumeration = 7,
}

/// One tool-bound proof artifact.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ToolEvidence {
    kind: ToolKind,
    claim: Hash32,
    artifact: Hash32,
    toolchain: Hash32,
}

impl ToolEvidence {
    /// Creates a content-bound tool result.
    #[must_use]
    pub const fn new(kind: ToolKind, claim: Hash32, artifact: Hash32, toolchain: Hash32) -> Self {
        Self {
            kind,
            claim,
            artifact,
            toolchain,
        }
    }

    /// Returns the evidence kind.
    #[must_use]
    pub const fn kind(self) -> ToolKind {
        self.kind
    }

    /// Returns the proved claim.
    #[must_use]
    pub const fn claim(self) -> Hash32 {
        self.claim
    }

    /// Returns the artifact commitment.
    #[must_use]
    pub const fn artifact(self) -> Hash32 {
        self.artifact
    }

    /// Returns the pinned toolchain commitment.
    #[must_use]
    pub const fn toolchain(self) -> Hash32 {
        self.toolchain
    }
}

impl CanonicalEncode for ToolEvidence {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(self.kind as u8);
        output.extend_from_slice(self.claim.as_bytes());
        output.extend_from_slice(self.artifact.as_bytes());
        output.extend_from_slice(self.toolchain.as_bytes());
        Ok(())
    }
}

/// External verifier for formal/differential artifacts.
pub trait ProofVerifier {
    /// Returns the exact verifier implementation/configuration identity.
    fn verifier_hash(&self) -> Hash32;

    /// Returns true only when the exact toolchain artifact establishes the claim.
    fn verify(&self, evidence: ToolEvidence) -> bool;
}

/// Fail-closed promotion requirements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromotionPolicy {
    required_tools: Box<[ToolKind]>,
    require_exact_cases: bool,
}

impl PromotionPolicy {
    /// Creates a policy with a canonical, duplicate-free required-tool set.
    pub fn try_new(
        mut required_tools: Vec<ToolKind>,
        require_exact_cases: bool,
    ) -> Result<Self, RefineError> {
        required_tools.sort();
        if required_tools.len() > MAX_TOOL_EVIDENCE
            || required_tools.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(RefineError::InvalidPromotionPolicy);
        }
        Ok(Self {
            required_tools: required_tools.into_boxed_slice(),
            require_exact_cases,
        })
    }

    /// Returns required evidence kinds.
    #[must_use]
    pub const fn required_tools(&self) -> &[ToolKind] {
        &self.required_tools
    }

    /// Returns whether every retained differential case must exactly match.
    #[must_use]
    pub const fn require_exact_cases(&self) -> bool {
        self.require_exact_cases
    }
}

impl CanonicalEncode for PromotionPolicy {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_length(output, self.required_tools.len())?;
        for kind in &self.required_tools {
            output.push(*kind as u8);
        }
        output.push(u8::from(self.require_exact_cases));
        Ok(())
    }
}

/// Legacy diagnostic evidence submitted for bounded or theorem-assisted checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromotionEvidence {
    profile_hash: Hash32,
    coverage: CoverageMode,
    cases: Box<[RefinementCase]>,
    tools: Box<[ToolEvidence]>,
}

impl PromotionEvidence {
    /// Creates evidence and rejects duplicate case or tool keys.
    pub fn try_new(
        profile_hash: Hash32,
        coverage: CoverageMode,
        mut cases: Vec<RefinementCase>,
        mut tools: Vec<ToolEvidence>,
    ) -> Result<Self, RefineError> {
        if cases.len() > MAX_CASES || tools.len() > MAX_TOOL_EVIDENCE {
            return Err(RefineError::EvidenceTooLarge);
        }
        cases.sort_by_key(RefinementCase::case_id);
        if cases
            .windows(2)
            .any(|pair| pair[0].case_id == pair[1].case_id)
        {
            return Err(RefineError::DuplicateCase);
        }
        tools.sort();
        if tools.windows(2).any(|pair| pair[0].kind == pair[1].kind) {
            return Err(RefineError::DuplicateToolEvidence);
        }
        Ok(Self {
            profile_hash,
            coverage,
            cases: cases.into_boxed_slice(),
            tools: tools.into_boxed_slice(),
        })
    }

    /// Returns the target profile.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the coverage mode.
    #[must_use]
    pub const fn coverage(&self) -> CoverageMode {
        self.coverage
    }

    /// Returns mounted refinement cases.
    #[must_use]
    pub const fn cases(&self) -> &[RefinementCase] {
        &self.cases
    }

    /// Returns tool evidence.
    #[must_use]
    pub const fn tools(&self) -> &[ToolEvidence] {
        &self.tools
    }
}

/// Coverage admitted by the strict promotion path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidatedCoverage {
    /// Every canonical manifest input was replayed exactly once and the
    /// enumeration claim is independently checked.
    Exhaustive {
        /// Exact canonical finite-domain manifest.
        manifest: Box<ExhaustiveDomainManifest>,
        /// Evidence for the exact manifest/case enumeration claim.
        coverage_evidence: ToolEvidence,
    },
    /// A nonempty bounded case set without a completeness claim.
    Bounded {
        /// Maximum admitted case count.
        case_budget: u64,
    },
    /// A theorem covers behavior outside any retained differential cases.
    ProofAssisted {
        /// Exact theorem statement commitment.
        theorem_claim: Hash32,
    },
}

impl CanonicalEncode for ValidatedCoverage {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Exhaustive {
                manifest,
                coverage_evidence,
            } => {
                output.push(0);
                put_blob(output, &manifest.canonical_bytes()?)?;
                coverage_evidence.encode_to(output)
            }
            Self::Bounded { case_budget } => {
                output.push(1);
                output.extend_from_slice(&case_budget.to_be_bytes());
                Ok(())
            }
            Self::ProofAssisted { theorem_claim } => {
                output.push(2);
                output.extend_from_slice(theorem_claim.as_bytes());
                Ok(())
            }
        }
    }
}

/// Strictly reconstructed, canonically ordered promotion evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPromotionEvidence {
    profile_hash: Hash32,
    coverage: ValidatedCoverage,
    cases: Box<[ValidatedRefinementCase]>,
    tools: Box<[ToolEvidence]>,
}

impl ValidatedPromotionEvidence {
    /// Creates evidence and enforces exact profile, input, and manifest binding.
    pub fn try_new(
        profile_hash: Hash32,
        coverage: ValidatedCoverage,
        mut cases: Vec<ValidatedRefinementCase>,
        mut tools: Vec<ToolEvidence>,
    ) -> Result<Self, RefineError> {
        if profile_hash == Hash32::ZERO {
            return Err(RefineError::InvalidPromotionProfile);
        }
        if cases.len() > MAX_CASES || tools.len() > MAX_TOOL_EVIDENCE {
            return Err(RefineError::EvidenceTooLarge);
        }
        cases.sort_by_key(ValidatedRefinementCase::input_hash);
        if cases
            .windows(2)
            .any(|pair| pair[0].input_hash() == pair[1].input_hash())
        {
            return Err(RefineError::DuplicateCaseInput);
        }
        if cases.iter().any(|case| case.profile_hash() != profile_hash) {
            return Err(RefineError::CaseProfileMismatch);
        }
        tools.sort();
        if tools.windows(2).any(|pair| pair[0].kind == pair[1].kind) {
            return Err(RefineError::DuplicateToolEvidence);
        }

        match &coverage {
            ValidatedCoverage::Exhaustive {
                manifest,
                coverage_evidence,
            } => {
                if manifest.profile_hash() != profile_hash
                    || cases
                        .iter()
                        .any(|case| case.provider_id() != manifest.provider_id())
                    || coverage_evidence.kind() != ToolKind::DomainEnumeration
                    || !manifest
                        .input_hashes()
                        .iter()
                        .copied()
                        .eq(cases.iter().map(ValidatedRefinementCase::input_hash))
                {
                    return Err(RefineError::DomainCaseMismatch);
                }
            }
            ValidatedCoverage::Bounded { case_budget } => {
                let actual = u64::try_from(cases.len()).unwrap_or(u64::MAX);
                if actual == 0 || actual > *case_budget {
                    return Err(RefineError::InvalidBoundedCoverage);
                }
            }
            ValidatedCoverage::ProofAssisted { theorem_claim } => {
                if *theorem_claim == Hash32::ZERO {
                    return Err(RefineError::InvalidProofClaim);
                }
            }
        }

        Ok(Self {
            profile_hash,
            coverage,
            cases: cases.into_boxed_slice(),
            tools: tools.into_boxed_slice(),
        })
    }

    /// Returns the target profile.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the exact coverage declaration.
    #[must_use]
    pub const fn coverage(&self) -> &ValidatedCoverage {
        &self.coverage
    }

    /// Returns canonical validated cases.
    #[must_use]
    pub const fn cases(&self) -> &[ValidatedRefinementCase] {
        &self.cases
    }

    /// Returns canonical additional tool evidence.
    #[must_use]
    pub const fn tools(&self) -> &[ToolEvidence] {
        &self.tools
    }
}

impl CanonicalEncode for ValidatedPromotionEvidence {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&REFINEMENT_PROTOCOL_VERSION.to_be_bytes());
        output.extend_from_slice(self.profile_hash.as_bytes());
        put_blob(output, &self.coverage.canonical_bytes()?)?;
        put_length(output, self.cases.len())?;
        for case in &self.cases {
            put_blob(output, &case.canonical_bytes()?)?;
        }
        put_length(output, self.tools.len())?;
        for evidence in &self.tools {
            evidence.encode_to(output)?;
        }
        Ok(())
    }
}

/// Exact source/importer context for one production promotion evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PromotionEvaluationContext {
    source_revision: Hash32,
    importer_hash: Hash32,
}

impl PromotionEvaluationContext {
    /// Creates a nonzero exact evaluation context.
    pub fn try_new(source_revision: Hash32, importer_hash: Hash32) -> Result<Self, RefineError> {
        if source_revision == Hash32::ZERO || importer_hash == Hash32::ZERO {
            return Err(RefineError::InvalidPromotionContext);
        }
        Ok(Self {
            source_revision,
            importer_hash,
        })
    }

    /// Returns the exact evaluated source revision.
    #[must_use]
    pub const fn source_revision(self) -> Hash32 {
        self.source_revision
    }

    /// Returns the exact evidence-importer identity.
    #[must_use]
    pub const fn importer_hash(self) -> Hash32 {
        self.importer_hash
    }
}

impl CanonicalEncode for PromotionEvaluationContext {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.source_revision.as_bytes());
        output.extend_from_slice(self.importer_hash.as_bytes());
        Ok(())
    }
}

/// One fail-closed promotion blocker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PromotionBlocker {
    /// Legacy cases contain structurally normalized but unvalidated artifacts.
    UnvalidatedDecisionArtifacts,
    /// A case is bound to another profile.
    CaseProfileMismatch {
        /// Case identifier.
        case_id: Hash32,
    },
    /// Model and runtime differ for one case.
    RefinementMismatch {
        /// Case identifier.
        case_id: Hash32,
        /// Exact mismatches.
        mismatches: Box<[Mismatch]>,
    },
    /// Exhaustive domain cardinality does not equal retained case count.
    ExhaustiveCardinalityMismatch,
    /// Legacy exhaustive evidence lacks a canonical verified domain manifest.
    UnverifiedLegacyExhaustiveCoverage,
    /// Bounded evidence exceeds its declared budget or contains no cases.
    InvalidBoundedCoverage,
    /// Proof-assisted coverage omits evidence for its theorem claim.
    MissingTheoremEvidence,
    /// Exact manifest enumeration evidence is absent, mismatched, or invalid.
    MissingCoverageEvidence,
    /// Policy-required tool evidence is absent or invalid.
    MissingToolEvidence {
        /// Required evidence kind.
        kind: ToolKind,
    },
}

/// Promotion decision report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromotionReport {
    blockers: Box<[PromotionBlocker]>,
}

/// Content-addressed result of the strict promotion path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPromotionReport {
    report_id: Hash32,
    policy_hash: Hash32,
    evidence_hash: Hash32,
    source_revision: Hash32,
    importer_hash: Hash32,
    verifier_hash: Hash32,
    provider_id: ApprovedProviderId,
    blockers: Box<[PromotionBlocker]>,
}

impl ValidatedPromotionReport {
    /// Returns whether exact validated evidence satisfies the policy.
    #[must_use]
    pub fn is_promotable(&self) -> bool {
        self.blockers.is_empty()
    }

    /// Returns the content-addressed report identity.
    #[must_use]
    pub const fn report_id(&self) -> Hash32 {
        self.report_id
    }

    /// Returns the exact promotion-policy identity.
    #[must_use]
    pub const fn policy_hash(&self) -> Hash32 {
        self.policy_hash
    }

    /// Returns the exact validated-evidence identity.
    #[must_use]
    pub const fn evidence_hash(&self) -> Hash32 {
        self.evidence_hash
    }

    /// Returns the evaluated source revision.
    #[must_use]
    pub const fn source_revision(&self) -> Hash32 {
        self.source_revision
    }

    /// Returns the exact importer identity.
    #[must_use]
    pub const fn importer_hash(&self) -> Hash32 {
        self.importer_hash
    }

    /// Returns the exact external verifier identity.
    #[must_use]
    pub const fn verifier_hash(&self) -> Hash32 {
        self.verifier_hash
    }

    /// Returns the nominal approved provider identity.
    #[must_use]
    pub const fn provider_id(&self) -> ApprovedProviderId {
        self.provider_id
    }

    /// Returns every fail-closed blocker.
    #[must_use]
    pub const fn blockers(&self) -> &[PromotionBlocker] {
        &self.blockers
    }
}

impl CanonicalEncode for ValidatedPromotionReport {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.report_id.as_bytes());
        output.extend_from_slice(&validated_report_body(self)?);
        Ok(())
    }
}

impl PromotionReport {
    /// Returns whether the evidence satisfies the declared promotion policy.
    #[must_use]
    pub fn is_promotable(&self) -> bool {
        self.blockers.is_empty()
    }

    /// Returns fail-closed blockers.
    #[must_use]
    pub const fn blockers(&self) -> &[PromotionBlocker] {
        &self.blockers
    }
}

/// Evaluates legacy diagnostic refinement and proof evidence.
///
/// Cardinality-only exhaustive claims always fail closed. Use
/// [`evaluate_validated_promotion`] for production promotion evidence.
#[must_use]
pub fn evaluate_promotion<V: ProofVerifier>(
    policy: &PromotionPolicy,
    evidence: &PromotionEvidence,
    verifier: &V,
) -> PromotionReport {
    let mut blockers = Vec::new();
    blockers.push(PromotionBlocker::UnvalidatedDecisionArtifacts);
    for case in evidence.cases() {
        let model_profile = case.model.artifacts().profile_hash;
        let runtime_profile = case.runtime.artifacts().profile_hash;
        if model_profile != evidence.profile_hash() || runtime_profile != evidence.profile_hash() {
            blockers.push(PromotionBlocker::CaseProfileMismatch {
                case_id: case.case_id(),
            });
        }
        if policy.require_exact_cases() {
            let report = case.report();
            if !report.is_exact() {
                blockers.push(PromotionBlocker::RefinementMismatch {
                    case_id: case.case_id(),
                    mismatches: report.mismatches,
                });
            }
        }
    }

    match evidence.coverage() {
        CoverageMode::Exhaustive { cardinality, .. } => {
            if usize::try_from(cardinality).ok() != Some(evidence.cases().len()) {
                blockers.push(PromotionBlocker::ExhaustiveCardinalityMismatch);
            }
            blockers.push(PromotionBlocker::UnverifiedLegacyExhaustiveCoverage);
        }
        CoverageMode::Bounded { case_budget } => {
            let actual = u64::try_from(evidence.cases().len()).unwrap_or(u64::MAX);
            if actual == 0 || actual > case_budget {
                blockers.push(PromotionBlocker::InvalidBoundedCoverage);
            }
        }
        CoverageMode::ProofAssisted { theorem_claim } => {
            if !evidence
                .tools()
                .iter()
                .copied()
                .any(|item| item.claim() == theorem_claim && verifier.verify(item))
            {
                blockers.push(PromotionBlocker::MissingTheoremEvidence);
            }
        }
    }

    for kind in policy.required_tools() {
        if !evidence
            .tools()
            .iter()
            .copied()
            .any(|item| item.kind() == *kind && verifier.verify(item))
        {
            blockers.push(PromotionBlocker::MissingToolEvidence { kind: *kind });
        }
    }
    PromotionReport {
        blockers: blockers.into_boxed_slice(),
    }
}

/// Evaluates strictly reconstructed cases and independently checked coverage.
pub fn evaluate_validated_promotion<H, V>(
    policy: &PromotionPolicy,
    evidence: &ValidatedPromotionEvidence,
    context: PromotionEvaluationContext,
    provider: &VerifiedProvider<H>,
    verifier: &V,
) -> Result<ValidatedPromotionReport, RefineError>
where
    H: ApprovedCommitmentProvider,
    V: ProofVerifier,
{
    if verifier.verifier_hash() == Hash32::ZERO {
        return Err(RefineError::InvalidVerifierIdentity);
    }
    if evidence
        .cases()
        .iter()
        .any(|case| case.provider_id() != provider.provider_id())
    {
        return Err(RefineError::ProviderBindingMismatch);
    }
    let mut blockers = Vec::new();
    for case in evidence.cases() {
        if policy.require_exact_cases() {
            let report = case.report();
            if !report.is_exact() {
                blockers.push(PromotionBlocker::RefinementMismatch {
                    case_id: case.case_id(),
                    mismatches: report.mismatches,
                });
            }
        }
    }

    match evidence.coverage() {
        ValidatedCoverage::Exhaustive {
            manifest,
            coverage_evidence,
        } => {
            let claim = exhaustive_coverage_claim::<H>(
                manifest,
                evidence.cases(),
                context,
                provider,
                verifier.verifier_hash(),
            )?;
            if coverage_evidence.kind() != ToolKind::DomainEnumeration
                || coverage_evidence.claim() != claim
                || coverage_evidence.toolchain() != manifest.toolchain_hash()
                || coverage_evidence.artifact() == Hash32::ZERO
                || !verifier.verify(*coverage_evidence)
            {
                blockers.push(PromotionBlocker::MissingCoverageEvidence);
            }
        }
        ValidatedCoverage::Bounded { .. } => {}
        ValidatedCoverage::ProofAssisted { theorem_claim } => {
            if !evidence
                .tools()
                .iter()
                .copied()
                .any(|item| item.claim() == *theorem_claim && verifier.verify(item))
            {
                blockers.push(PromotionBlocker::MissingTheoremEvidence);
            }
        }
    }

    for kind in policy.required_tools() {
        let coverage_matches = matches!(
            evidence.coverage(),
            ValidatedCoverage::Exhaustive {
                coverage_evidence,
                ..
            } if coverage_evidence.kind() == *kind && verifier.verify(*coverage_evidence)
        );
        if !coverage_matches
            && !evidence
                .tools()
                .iter()
                .copied()
                .any(|item| item.kind() == *kind && verifier.verify(item))
        {
            blockers.push(PromotionBlocker::MissingToolEvidence { kind: *kind });
        }
    }

    let policy_hash = commitment::<H>(
        refinement_domain("zeno-fcis/promotion-policy")?,
        &policy.canonical_bytes()?,
    )?;
    let evidence_hash = commitment::<H>(
        refinement_domain("zeno-fcis/promotion-evidence")?,
        &evidence.canonical_bytes()?,
    )?;
    let mut report = ValidatedPromotionReport {
        report_id: Hash32::ZERO,
        policy_hash,
        evidence_hash,
        source_revision: context.source_revision(),
        importer_hash: context.importer_hash(),
        verifier_hash: verifier.verifier_hash(),
        provider_id: provider.provider_id(),
        blockers: blockers.into_boxed_slice(),
    };
    report.report_id = commitment::<H>(
        refinement_domain("zeno-fcis/promotion-report")?,
        &validated_report_body(&report)?,
    )?;
    Ok(report)
}

/// Derives the exact theorem/query claim an enumeration checker must establish.
pub fn exhaustive_coverage_claim<H: ApprovedCommitmentProvider>(
    manifest: &ExhaustiveDomainManifest,
    cases: &[ValidatedRefinementCase],
    context: PromotionEvaluationContext,
    provider: &VerifiedProvider<H>,
    verifier_hash: Hash32,
) -> Result<Hash32, RefineError> {
    if verifier_hash == Hash32::ZERO {
        return Err(RefineError::InvalidVerifierIdentity);
    }
    if manifest.provider_id() != provider.provider_id()
        || cases
            .iter()
            .any(|case| case.provider_id() != provider.provider_id())
    {
        return Err(RefineError::ProviderBindingMismatch);
    }
    if cases
        .iter()
        .any(|case| case.profile_hash() != manifest.profile_hash())
        || !manifest
            .input_hashes()
            .iter()
            .copied()
            .eq(cases.iter().map(ValidatedRefinementCase::input_hash))
    {
        return Err(RefineError::DomainCaseMismatch);
    }
    let mut body = Vec::new();
    body.extend_from_slice(&REFINEMENT_PROTOCOL_VERSION.to_be_bytes());
    put_blob(&mut body, &manifest.canonical_bytes()?)?;
    put_length(&mut body, cases.len())?;
    for case in cases {
        body.extend_from_slice(case.input_hash().as_bytes());
        body.extend_from_slice(case.case_id().as_bytes());
    }
    context.encode_to(&mut body)?;
    body.extend_from_slice(&provider.provider_id().code().to_be_bytes());
    body.extend_from_slice(verifier_hash.as_bytes());
    commitment::<H>(
        refinement_domain("zeno-fcis/exhaustive-coverage-claim")?,
        &body,
    )
    .map_err(RefineError::Encode)
}

fn validated_report_body(report: &ValidatedPromotionReport) -> Result<Vec<u8>, EncodeError> {
    let mut output = Vec::new();
    output.extend_from_slice(&REFINEMENT_PROTOCOL_VERSION.to_be_bytes());
    for hash in [
        report.policy_hash,
        report.evidence_hash,
        report.source_revision,
        report.importer_hash,
        report.verifier_hash,
    ] {
        output.extend_from_slice(hash.as_bytes());
    }
    output.extend_from_slice(&report.provider_id.code().to_be_bytes());
    put_length(&mut output, report.blockers.len())?;
    for blocker in &report.blockers {
        encode_promotion_blocker(blocker, &mut output)?;
    }
    Ok(output)
}

fn encode_promotion_blocker(
    blocker: &PromotionBlocker,
    output: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    match blocker {
        PromotionBlocker::CaseProfileMismatch { case_id } => {
            output.push(0);
            output.extend_from_slice(case_id.as_bytes());
        }
        PromotionBlocker::RefinementMismatch {
            case_id,
            mismatches,
        } => {
            output.push(1);
            output.extend_from_slice(case_id.as_bytes());
            put_length(output, mismatches.len())?;
            for mismatch in mismatches {
                output.push(mismatch_tag(*mismatch));
            }
        }
        PromotionBlocker::ExhaustiveCardinalityMismatch => output.push(2),
        PromotionBlocker::UnverifiedLegacyExhaustiveCoverage => output.push(3),
        PromotionBlocker::InvalidBoundedCoverage => output.push(4),
        PromotionBlocker::MissingTheoremEvidence => output.push(5),
        PromotionBlocker::MissingCoverageEvidence => output.push(6),
        PromotionBlocker::MissingToolEvidence { kind } => {
            output.push(7);
            output.push(*kind as u8);
        }
        PromotionBlocker::UnvalidatedDecisionArtifacts => output.push(8),
    }
    Ok(())
}

fn mismatch_tag(mismatch: Mismatch) -> u8 {
    match mismatch {
        Mismatch::DecisionKind => 0,
        Mismatch::ReasonCode => 1,
        Mismatch::ProfileHash => 2,
        Mismatch::CommandHash => 3,
        Mismatch::ContextHash => 4,
        Mismatch::PrecedenceHash => 5,
        Mismatch::AlgorithmHash => 6,
        Mismatch::BudgetHash => 7,
        Mismatch::PreRoot => 8,
        Mismatch::PostRoot => 9,
        Mismatch::CandidateId => 10,
        Mismatch::PatchBytes => 11,
        Mismatch::CommitPlanBytes => 12,
        Mismatch::OutboxPlanBytes => 13,
        Mismatch::ReceiptBytes => 14,
        Mismatch::BundleBytes => 15,
    }
}

fn refinement_domain(name: &str) -> Result<Domain<'_>, EncodeError> {
    Domain::new(name, REFINEMENT_PROTOCOL_VERSION)
}

fn encode_candidate_bindings(bindings: CandidateBindings, output: &mut Vec<u8>) {
    for hash in [
        bindings.profile_hash,
        bindings.command_hash,
        bindings.context_hash,
        bindings.precedence_hash,
        bindings.algorithm_hash,
        bindings.budget_hash,
    ] {
        output.extend_from_slice(hash.as_bytes());
    }
}

fn put_hashes(output: &mut Vec<u8>, hashes: &[Hash32]) -> Result<(), EncodeError> {
    put_length(output, hashes.len())?;
    for hash in hashes {
        output.extend_from_slice(hash.as_bytes());
    }
    Ok(())
}

fn put_optional_hash(output: &mut Vec<u8>, hash: Option<Hash32>) {
    match hash {
        None => output.push(0),
        Some(hash) => {
            output.push(1);
            output.extend_from_slice(hash.as_bytes());
        }
    }
}

fn validate_reason(kind: DecisionKind, reason: Option<&str>) -> Result<(), RefineError> {
    if reason.is_some_and(|value| {
        !value.is_ascii() || value.is_empty() || value.len() > MAX_REASON_BYTES
    }) {
        return Err(RefineError::InvalidReasonCode);
    }
    match (kind, reason) {
        (DecisionKind::Accept, None)
        | (DecisionKind::Reject, Some(_))
        | (DecisionKind::CommittedFailure, Some(_)) => Ok(()),
        _ => Err(RefineError::InvalidReasonCode),
    }
}

fn decision_tag(kind: DecisionKind) -> u8 {
    match kind {
        DecisionKind::Accept => 0,
        DecisionKind::Reject => 1,
        DecisionKind::CommittedFailure => 2,
    }
}

fn put_optional_text(output: &mut Vec<u8>, value: Option<&str>) -> Result<(), EncodeError> {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            put_blob(output, value.as_bytes())?;
        }
    }
    Ok(())
}

fn put_optional_blob(output: &mut Vec<u8>, value: Option<&[u8]>) -> Result<(), EncodeError> {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            put_blob(output, value)?;
        }
    }
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    let length = u32::try_from(bytes.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn put_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

/// Decision normalization or promotion-evidence construction failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefineError {
    /// Reason is absent, present in the wrong decision, non-ASCII, empty, or too long.
    InvalidReasonCode,
    /// Rejection changed state.
    RejectedStateChanged,
    /// Rejection carried candidate artifacts.
    RejectedCandidatePresent,
    /// Accepted or committed-failure decision omitted an authority artifact.
    CommittedArtifactMissing,
    /// One canonical artifact exceeds its bound.
    ArtifactTooLarge,
    /// Canonical encoding failed.
    Encode(EncodeError),
    /// Strict receipt or complete-bundle reconstruction failed.
    ReceiptDecode(ReceiptDecodeError),
    /// Exact pre-state root derivation failed.
    Patch(PatchError),
    /// The submitted artifact is not rooted in the supplied exact pre-state.
    UnexpectedPreRoot,
    /// Reconstructed candidate bindings differ from the external invocation.
    InvocationBindingMismatch,
    /// Strictly reconstructed artifacts do not equal the complete transport value.
    ArtifactReconstructionMismatch,
    /// Model and runtime decisions were validated for different invocations.
    ValidationBindingMismatch,
    /// A validated artifact was reused under another approved provider.
    ProviderBindingMismatch,
    /// Promotion policy has duplicate or excessive tool requirements.
    InvalidPromotionPolicy,
    /// Evidence exceeds its case or tool bound.
    EvidenceTooLarge,
    /// Case identifier is duplicated.
    DuplicateCase,
    /// More than one validated case represents the same exact input.
    DuplicateCaseInput,
    /// A validated case belongs to another profile.
    CaseProfileMismatch,
    /// The exact validated case set differs from the canonical domain manifest.
    DomainCaseMismatch,
    /// The finite-domain manifest is zero, excessive, duplicate, or noncanonical.
    InvalidDomainManifest,
    /// Bounded validated coverage is empty or exceeds its declared budget.
    InvalidBoundedCoverage,
    /// A proof-assisted claim is zero or otherwise inadmissible.
    InvalidProofClaim,
    /// Source revision or importer identity is absent.
    InvalidPromotionContext,
    /// The strict promotion profile identity is absent.
    InvalidPromotionProfile,
    /// The external proof verifier omitted its implementation identity.
    InvalidVerifierIdentity,
    /// More than one artifact claims the same evidence kind.
    DuplicateToolEvidence,
}

impl fmt::Display for RefineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidReasonCode => formatter.write_str("invalid decision reason code"),
            Self::RejectedStateChanged => formatter.write_str("rejected decision changed state"),
            Self::RejectedCandidatePresent => {
                formatter.write_str("rejected decision carried candidate artifacts")
            }
            Self::CommittedArtifactMissing => {
                formatter.write_str("committed decision omitted an authority artifact")
            }
            Self::ArtifactTooLarge => {
                formatter.write_str("canonical authority artifact exceeds its bound")
            }
            Self::Encode(error) => error.fmt(formatter),
            Self::ReceiptDecode(error) => error.fmt(formatter),
            Self::Patch(error) => error.fmt(formatter),
            Self::UnexpectedPreRoot => {
                formatter.write_str("decision is not rooted in the exact supplied pre-state")
            }
            Self::InvocationBindingMismatch => {
                formatter.write_str("decision differs from the exact external invocation binding")
            }
            Self::ArtifactReconstructionMismatch => formatter
                .write_str("strictly reconstructed artifacts differ from the transport decision"),
            Self::ValidationBindingMismatch => {
                formatter.write_str("model and runtime validation bindings differ")
            }
            Self::ProviderBindingMismatch => {
                formatter.write_str("validated artifact provider binding differs")
            }
            Self::InvalidPromotionPolicy => formatter.write_str("invalid promotion policy"),
            Self::EvidenceTooLarge => formatter.write_str("promotion evidence exceeds its bound"),
            Self::DuplicateCase => formatter.write_str("refinement case is duplicated"),
            Self::DuplicateCaseInput => formatter.write_str("refinement input is duplicated"),
            Self::CaseProfileMismatch => {
                formatter.write_str("validated refinement case belongs to another profile")
            }
            Self::DomainCaseMismatch => {
                formatter.write_str("validated case set differs from the domain manifest")
            }
            Self::InvalidDomainManifest => {
                formatter.write_str("invalid exhaustive-domain manifest")
            }
            Self::InvalidBoundedCoverage => {
                formatter.write_str("invalid bounded validated coverage")
            }
            Self::InvalidProofClaim => formatter.write_str("invalid proof-assisted claim"),
            Self::InvalidPromotionContext => {
                formatter.write_str("invalid promotion evaluation context")
            }
            Self::InvalidPromotionProfile => {
                formatter.write_str("invalid promotion profile identity")
            }
            Self::InvalidVerifierIdentity => formatter.write_str("invalid proof-verifier identity"),
            Self::DuplicateToolEvidence => formatter.write_str("tool evidence kind is duplicated"),
        }
    }
}

impl From<EncodeError> for RefineError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    #[cfg(feature = "libcrux")]
    use zeno_fcis_crypto::LibcruxSha256;
    use zeno_fcis_crypto::{RustCryptoSha256, verify_approved_provider};
    use zeno_fcis_patch::{CanonicalPatch, PatchOp, PathSegment, ValuePath};
    use zeno_fcis_plan::{CommitPlan, OutboxPlan};
    use zeno_fcis_receipt::{CandidateBuilder, RejectReceipt};

    struct ExactVerifier(Hash32);

    impl ProofVerifier for ExactVerifier {
        fn verifier_hash(&self) -> Hash32 {
            self.0
        }

        fn verify(&self, evidence: ToolEvidence) -> bool {
            evidence.claim() == evidence.artifact() && evidence.toolchain() != Hash32::ZERO
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn provider() -> VerifiedProvider<RustCryptoSha256> {
        verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("provider: {error}"))
    }

    fn domain() -> Domain<'static> {
        Domain::new("test/refine-state", 1).unwrap_or_else(|error| panic!("domain: {error}"))
    }

    fn state() -> Value {
        Value::record_canonical(Vec::new()).unwrap_or_else(|error| panic!("state: {error}"))
    }

    fn bindings(command: u8) -> CandidateBindings {
        CandidateBindings {
            profile_hash: hash(1),
            command_hash: hash(command),
            context_hash: hash(3),
            precedence_hash: hash(4),
            algorithm_hash: hash(5),
            budget_hash: hash(6),
        }
    }

    fn rejection(profile: Hash32, reason: &str) -> NormalizedDecision {
        NormalizedDecision::try_new(DecisionArtifacts {
            kind: DecisionKind::Reject,
            reason_code: Some(Box::from(reason)),
            profile_hash: profile,
            command_hash: hash(2),
            context_hash: hash(3),
            precedence_hash: hash(4),
            algorithm_hash: hash(5),
            budget_hash: hash(6),
            pre_root: hash(7),
            post_root: hash(7),
            candidate_id: None,
            patch_bytes: None,
            commit_plan_bytes: None,
            outbox_plan_bytes: None,
            receipt_bytes: vec![1].into_boxed_slice(),
            bundle_bytes: None,
        })
        .unwrap_or_else(|error| panic!("decision: {error}"))
    }

    fn admitted_rejection(command: u8, reason: &str) -> NormalizedDecision {
        let pre_state = state();
        let pre_root = hash_value::<RustCryptoSha256>(domain(), &pre_state)
            .unwrap_or_else(|error| panic!("root: {error}"));
        let receipt = RejectReceipt::new(bindings(command), pre_root, reason)
            .unwrap_or_else(|error| panic!("receipt: {error}"));
        NormalizedDecision::from_reject(&receipt)
            .unwrap_or_else(|error| panic!("normalize: {error}"))
    }

    fn validate_rejection(command: u8, reason: &str) -> ValidatedNormalizedDecision {
        ValidatedNormalizedDecision::try_from_untrusted(
            admitted_rejection(command, reason),
            &state(),
            domain(),
            bindings(command),
            DecisionValidationLimits::default(),
            &provider(),
        )
        .unwrap_or_else(|error| panic!("validate: {error}"))
    }

    fn validated_case(command: u8, runtime_reason: &str) -> ValidatedRefinementCase {
        ValidatedRefinementCase::try_new(
            validate_rejection(command, "model_reason"),
            validate_rejection(command, runtime_reason),
            &provider(),
        )
        .unwrap_or_else(|error| panic!("case: {error}"))
    }

    fn admitted_bundle() -> (NormalizedDecision, CandidateBindings, Value) {
        let pre_state = state();
        let exact_bindings = bindings(2);
        let pre_root = hash_value::<RustCryptoSha256>(domain(), &pre_state)
            .unwrap_or_else(|error| panic!("root: {error}"));
        let patch = CanonicalPatch::try_new(
            1,
            pre_root,
            vec![PatchOp::Insert {
                path: ValuePath::new(vec![PathSegment::Field(1)]),
                map_key: None,
                value: Value::U128(9),
            }],
        )
        .unwrap_or_else(|error| panic!("patch: {error}"));
        let bundle = CandidateBuilder::seal::<RustCryptoSha256>(
            &pre_state,
            domain(),
            DecisionKind::Accept,
            None,
            exact_bindings,
            patch,
            CommitPlan::empty(),
            OutboxPlan::empty(),
        )
        .unwrap_or_else(|error| panic!("bundle: {error}"));
        (
            NormalizedDecision::from_bundle(&bundle)
                .unwrap_or_else(|error| panic!("normalize: {error}")),
            exact_bindings,
            pre_state,
        )
    }

    fn exact_policy() -> PromotionPolicy {
        PromotionPolicy::try_new(vec![ToolKind::DomainEnumeration], true)
            .unwrap_or_else(|error| panic!("policy: {error}"))
    }

    fn exhaustive_report(
        source_revision: Hash32,
        importer_hash: Hash32,
        verifier: &ExactVerifier,
    ) -> ValidatedPromotionReport {
        let exact_provider = provider();
        let cases = vec![validated_case(2, "model_reason")];
        let manifest = ExhaustiveDomainManifest::try_new::<RustCryptoSha256>(
            hash(1),
            hash(80),
            hash(81),
            hash(82),
            vec![cases[0].input_hash()],
            None,
            &exact_provider,
        )
        .unwrap_or_else(|error| panic!("manifest: {error}"));
        let context = PromotionEvaluationContext::try_new(source_revision, importer_hash)
            .unwrap_or_else(|error| panic!("context: {error}"));
        let claim = exhaustive_coverage_claim::<RustCryptoSha256>(
            &manifest,
            &cases,
            context,
            &exact_provider,
            verifier.verifier_hash(),
        )
        .unwrap_or_else(|error| panic!("claim: {error}"));
        let evidence = ValidatedPromotionEvidence::try_new(
            hash(1),
            ValidatedCoverage::Exhaustive {
                manifest: Box::new(manifest),
                coverage_evidence: ToolEvidence::new(
                    ToolKind::DomainEnumeration,
                    claim,
                    claim,
                    hash(82),
                ),
            },
            cases,
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        evaluate_validated_promotion::<RustCryptoSha256, _>(
            &exact_policy(),
            &evidence,
            context,
            &exact_provider,
            verifier,
        )
        .unwrap_or_else(|error| panic!("promotion: {error}"))
    }

    #[test]
    fn rejected_decisions_cannot_carry_candidates() {
        let error = NormalizedDecision::try_new(DecisionArtifacts {
            kind: DecisionKind::Reject,
            reason_code: Some(Box::from("bad")),
            profile_hash: hash(1),
            command_hash: hash(2),
            context_hash: hash(3),
            precedence_hash: hash(4),
            algorithm_hash: hash(5),
            budget_hash: hash(6),
            pre_root: hash(7),
            post_root: hash(7),
            candidate_id: Some(hash(8)),
            patch_bytes: None,
            commit_plan_bytes: None,
            outbox_plan_bytes: None,
            receipt_bytes: vec![1].into_boxed_slice(),
            bundle_bytes: None,
        });
        assert_eq!(error, Err(RefineError::RejectedCandidatePresent));
    }

    #[test]
    fn exact_comparison_reports_artifact_differences() {
        let left = rejection(hash(1), "reason_a");
        let right = rejection(hash(1), "reason_b");
        assert_eq!(
            compare_exact(&left, &right).mismatches(),
            [Mismatch::ReasonCode]
        );
    }

    #[test]
    fn strict_reconstruction_rejects_fabricated_equal_decisions() {
        let fabricated = rejection(hash(1), "fabricated");
        assert!(compare_exact(&fabricated, &fabricated).is_exact());
        let validated = ValidatedNormalizedDecision::try_from_untrusted(
            fabricated,
            &state(),
            domain(),
            bindings(2),
            DecisionValidationLimits::default(),
            &provider(),
        );
        assert!(matches!(validated, Err(RefineError::UnexpectedPreRoot)));
    }

    #[test]
    fn strict_reconstruction_binds_exact_invocation_and_domain() {
        let decision = admitted_rejection(2, "denied");
        assert!(
            ValidatedNormalizedDecision::try_from_untrusted(
                decision.clone(),
                &state(),
                domain(),
                bindings(2),
                DecisionValidationLimits::default(),
                &provider(),
            )
            .is_ok()
        );
        assert!(matches!(
            ValidatedNormalizedDecision::try_from_untrusted(
                decision.clone(),
                &state(),
                domain(),
                bindings(9),
                DecisionValidationLimits::default(),
                &provider(),
            ),
            Err(RefineError::InvocationBindingMismatch)
        ));
        let wrong_domain =
            Domain::new("test/other-state", 1).unwrap_or_else(|error| panic!("domain: {error}"));
        assert!(matches!(
            ValidatedNormalizedDecision::try_from_untrusted(
                decision,
                &state(),
                wrong_domain,
                bindings(2),
                DecisionValidationLimits::default(),
                &provider(),
            ),
            Err(RefineError::UnexpectedPreRoot)
        ));
    }

    #[test]
    fn every_committed_artifact_substitution_fails_reconstruction() {
        let (decision, exact_bindings, pre_state) = admitted_bundle();
        assert!(
            ValidatedNormalizedDecision::try_from_untrusted(
                decision.clone(),
                &pre_state,
                domain(),
                exact_bindings,
                DecisionValidationLimits::default(),
                &provider(),
            )
            .is_ok()
        );

        let mut mutations = Vec::new();
        let mut candidate = decision.artifacts().clone();
        candidate.candidate_id = Some(hash(99));
        mutations.push(candidate);
        let mut pre_root = decision.artifacts().clone();
        pre_root.pre_root = hash(99);
        mutations.push(pre_root);
        let mut post_root = decision.artifacts().clone();
        post_root.post_root = hash(99);
        mutations.push(post_root);
        for select in 0..5 {
            let mut artifact = decision.artifacts().clone();
            let bytes = match select {
                0 => artifact.patch_bytes.as_mut(),
                1 => artifact.commit_plan_bytes.as_mut(),
                2 => artifact.outbox_plan_bytes.as_mut(),
                3 => Some(&mut artifact.receipt_bytes),
                _ => artifact.bundle_bytes.as_mut(),
            }
            .unwrap_or_else(|| panic!("artifact {select}"));
            let last = bytes.len().saturating_sub(1);
            bytes[last] ^= 1;
            mutations.push(artifact);
        }

        for mutation in mutations {
            let untrusted = NormalizedDecision::try_new(mutation)
                .unwrap_or_else(|error| panic!("untrusted shape: {error}"));
            assert!(
                ValidatedNormalizedDecision::try_from_untrusted(
                    untrusted,
                    &pre_state,
                    domain(),
                    exact_bindings,
                    DecisionValidationLimits::default(),
                    &provider(),
                )
                .is_err()
            );
        }
    }

    #[test]
    fn legacy_bounded_evidence_remains_diagnostic_only() {
        let profile = hash(1);
        let model = rejection(profile, "reason");
        let runtime = model.clone();
        let case = RefinementCase::new(hash(2), hash(3), model, runtime);
        let tools = vec![ToolEvidence::new(
            ToolKind::RuntimeRefinement,
            hash(4),
            hash(4),
            hash(5),
        )];
        let evidence = PromotionEvidence::try_new(
            profile,
            CoverageMode::Bounded { case_budget: 1 },
            vec![case],
            tools,
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        let policy = PromotionPolicy::try_new(vec![ToolKind::RuntimeRefinement], true)
            .unwrap_or_else(|error| panic!("policy: {error}"));
        assert_eq!(
            evaluate_promotion(&policy, &evidence, &ExactVerifier(hash(90))).blockers(),
            [PromotionBlocker::UnvalidatedDecisionArtifacts]
        );
    }

    #[test]
    fn legacy_cardinality_only_exhaustive_coverage_fails_closed() {
        let model = rejection(hash(1), "reason");
        let case = RefinementCase::new(hash(2), hash(3), model.clone(), model);
        let evidence = PromotionEvidence::try_new(
            hash(1),
            CoverageMode::Exhaustive {
                domain_hash: hash(44),
                cardinality: 1,
            },
            vec![case],
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        assert!(matches!(
            evaluate_promotion(
                &PromotionPolicy::try_new(Vec::new(), true)
                    .unwrap_or_else(|error| panic!("policy: {error}")),
                &evidence,
                &ExactVerifier(hash(90)),
            )
            .blockers(),
            [
                PromotionBlocker::UnvalidatedDecisionArtifacts,
                PromotionBlocker::UnverifiedLegacyExhaustiveCoverage,
            ]
        ));
    }

    #[test]
    fn duplicate_inputs_fail_even_when_derived_case_ids_differ() {
        let exact = validated_case(2, "model_reason");
        let mismatch = validated_case(2, "different_runtime_reason");
        assert_ne!(exact.case_id(), mismatch.case_id());
        assert_eq!(exact.input_hash(), mismatch.input_hash());
        assert!(matches!(
            ValidatedPromotionEvidence::try_new(
                hash(1),
                ValidatedCoverage::Bounded { case_budget: 2 },
                vec![exact, mismatch],
                Vec::new(),
            ),
            Err(RefineError::DuplicateCaseInput)
        ));
    }

    #[test]
    #[cfg(feature = "libcrux")]
    fn validated_cases_cannot_cross_approved_provider_bindings() {
        let model = validate_rejection(2, "model_reason");
        let runtime = model.clone();
        let other_provider = verify_approved_provider::<LibcruxSha256>()
            .unwrap_or_else(|error| panic!("provider: {error}"));
        assert!(matches!(
            ValidatedRefinementCase::try_new(model, runtime, &other_provider),
            Err(RefineError::ProviderBindingMismatch)
        ));
    }

    #[test]
    fn exhaustive_manifest_rejects_duplicate_and_reordered_members() {
        let exact_provider = provider();
        let duplicate = ExhaustiveDomainManifest::try_new::<RustCryptoSha256>(
            hash(1),
            hash(80),
            hash(81),
            hash(82),
            vec![hash(10), hash(10)],
            None,
            &exact_provider,
        );
        assert!(matches!(duplicate, Err(RefineError::InvalidDomainManifest)));
        let reordered = ExhaustiveDomainManifest::try_new::<RustCryptoSha256>(
            hash(1),
            hash(80),
            hash(81),
            hash(82),
            vec![hash(11), hash(10)],
            None,
            &exact_provider,
        );
        assert!(matches!(reordered, Err(RefineError::InvalidDomainManifest)));
    }

    #[test]
    fn exhaustive_evidence_rejects_missing_and_extra_members() {
        let exact_provider = provider();
        let cases = vec![
            validated_case(2, "model_reason"),
            validated_case(7, "model_reason"),
        ];
        let mut exact_inputs = cases
            .iter()
            .map(ValidatedRefinementCase::input_hash)
            .collect::<Vec<_>>();
        exact_inputs.sort();
        let missing = ExhaustiveDomainManifest::try_new::<RustCryptoSha256>(
            hash(1),
            hash(80),
            hash(81),
            hash(82),
            vec![exact_inputs[0]],
            None,
            &exact_provider,
        )
        .unwrap_or_else(|error| panic!("manifest: {error}"));
        assert!(matches!(
            ValidatedPromotionEvidence::try_new(
                hash(1),
                ValidatedCoverage::Exhaustive {
                    manifest: Box::new(missing),
                    coverage_evidence: ToolEvidence::new(
                        ToolKind::DomainEnumeration,
                        hash(1),
                        hash(1),
                        hash(82),
                    ),
                },
                cases.clone(),
                Vec::new(),
            ),
            Err(RefineError::DomainCaseMismatch)
        ));

        let mut extra_inputs = exact_inputs;
        extra_inputs.push(hash(200));
        extra_inputs.sort();
        let extra = ExhaustiveDomainManifest::try_new::<RustCryptoSha256>(
            hash(1),
            hash(80),
            hash(81),
            hash(82),
            extra_inputs,
            None,
            &exact_provider,
        )
        .unwrap_or_else(|error| panic!("manifest: {error}"));
        assert!(matches!(
            ValidatedPromotionEvidence::try_new(
                hash(1),
                ValidatedCoverage::Exhaustive {
                    manifest: Box::new(extra),
                    coverage_evidence: ToolEvidence::new(
                        ToolKind::DomainEnumeration,
                        hash(1),
                        hash(1),
                        hash(82),
                    ),
                },
                cases,
                Vec::new(),
            ),
            Err(RefineError::DomainCaseMismatch)
        ));
    }

    #[test]
    fn exhaustive_promotion_requires_exact_independent_coverage_evidence() {
        let verifier = ExactVerifier(hash(90));
        let report = exhaustive_report(hash(91), hash(92), &verifier);
        assert!(report.is_promotable());

        let exact_provider = provider();
        let cases = vec![validated_case(2, "model_reason")];
        let manifest = ExhaustiveDomainManifest::try_new::<RustCryptoSha256>(
            hash(1),
            hash(80),
            hash(81),
            hash(82),
            vec![cases[0].input_hash()],
            None,
            &exact_provider,
        )
        .unwrap_or_else(|error| panic!("manifest: {error}"));
        let context = PromotionEvaluationContext::try_new(hash(91), hash(92))
            .unwrap_or_else(|error| panic!("context: {error}"));
        let evidence = ValidatedPromotionEvidence::try_new(
            hash(1),
            ValidatedCoverage::Exhaustive {
                manifest: Box::new(manifest),
                coverage_evidence: ToolEvidence::new(
                    ToolKind::DomainEnumeration,
                    hash(93),
                    hash(93),
                    hash(82),
                ),
            },
            cases,
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        let rejected = evaluate_validated_promotion::<RustCryptoSha256, _>(
            &exact_policy(),
            &evidence,
            context,
            &exact_provider,
            &verifier,
        )
        .unwrap_or_else(|error| panic!("promotion: {error}"));
        assert!(
            rejected
                .blockers()
                .contains(&PromotionBlocker::MissingCoverageEvidence)
        );
    }

    #[test]
    fn empty_domain_requires_explicit_verified_declaration() {
        let exact_provider = provider();
        assert!(matches!(
            ExhaustiveDomainManifest::try_new::<RustCryptoSha256>(
                hash(1),
                hash(80),
                hash(81),
                hash(82),
                Vec::new(),
                None,
                &exact_provider,
            ),
            Err(RefineError::InvalidDomainManifest)
        ));
        let manifest = ExhaustiveDomainManifest::try_new::<RustCryptoSha256>(
            hash(1),
            hash(80),
            hash(81),
            hash(82),
            Vec::new(),
            Some(hash(83)),
            &exact_provider,
        )
        .unwrap_or_else(|error| panic!("manifest: {error}"));
        let verifier = ExactVerifier(hash(90));
        let context = PromotionEvaluationContext::try_new(hash(91), hash(92))
            .unwrap_or_else(|error| panic!("context: {error}"));
        let claim = exhaustive_coverage_claim::<RustCryptoSha256>(
            &manifest,
            &[],
            context,
            &exact_provider,
            verifier.verifier_hash(),
        )
        .unwrap_or_else(|error| panic!("claim: {error}"));
        let evidence = ValidatedPromotionEvidence::try_new(
            hash(1),
            ValidatedCoverage::Exhaustive {
                manifest: Box::new(manifest),
                coverage_evidence: ToolEvidence::new(
                    ToolKind::DomainEnumeration,
                    claim,
                    claim,
                    hash(82),
                ),
            },
            Vec::new(),
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        let report = evaluate_validated_promotion::<RustCryptoSha256, _>(
            &exact_policy(),
            &evidence,
            context,
            &exact_provider,
            &verifier,
        )
        .unwrap_or_else(|error| panic!("promotion: {error}"));
        assert!(report.is_promotable());
    }

    #[test]
    fn report_identity_binds_source_importer_and_verifier() {
        let first = exhaustive_report(hash(91), hash(92), &ExactVerifier(hash(90)));
        let changed_source = exhaustive_report(hash(93), hash(92), &ExactVerifier(hash(90)));
        let changed_importer = exhaustive_report(hash(91), hash(94), &ExactVerifier(hash(90)));
        let changed_verifier = exhaustive_report(hash(91), hash(92), &ExactVerifier(hash(95)));
        assert_ne!(first.report_id(), changed_source.report_id());
        assert_ne!(first.report_id(), changed_importer.report_id());
        assert_ne!(first.report_id(), changed_verifier.report_id());
    }

    #[test]
    fn unidentified_verifier_cannot_issue_a_promotion_report() {
        let exact_provider = provider();
        let manifest = ExhaustiveDomainManifest::try_new::<RustCryptoSha256>(
            hash(1),
            hash(80),
            hash(81),
            hash(82),
            Vec::new(),
            Some(hash(83)),
            &exact_provider,
        )
        .unwrap_or_else(|error| panic!("manifest: {error}"));
        let evidence = ValidatedPromotionEvidence::try_new(
            hash(1),
            ValidatedCoverage::Exhaustive {
                manifest: Box::new(manifest),
                coverage_evidence: ToolEvidence::new(
                    ToolKind::DomainEnumeration,
                    hash(84),
                    hash(84),
                    hash(82),
                ),
            },
            Vec::new(),
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        assert!(matches!(
            evaluate_validated_promotion::<RustCryptoSha256, _>(
                &exact_policy(),
                &evidence,
                PromotionEvaluationContext::try_new(hash(91), hash(92))
                    .unwrap_or_else(|error| panic!("context: {error}")),
                &exact_provider,
                &ExactVerifier(Hash32::ZERO),
            ),
            Err(RefineError::InvalidVerifierIdentity)
        ));
    }

    #[test]
    fn proof_assisted_promotion_requires_theorem_evidence() {
        let profile = hash(1);
        let theorem = hash(9);
        let evidence = PromotionEvidence::try_new(
            profile,
            CoverageMode::ProofAssisted {
                theorem_claim: theorem,
            },
            Vec::new(),
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        let policy = PromotionPolicy::try_new(Vec::new(), false)
            .unwrap_or_else(|error| panic!("policy: {error}"));
        assert!(matches!(
            evaluate_promotion(&policy, &evidence, &ExactVerifier(hash(90))).blockers(),
            [
                PromotionBlocker::UnvalidatedDecisionArtifacts,
                PromotionBlocker::MissingTheoremEvidence,
            ]
        ));
    }
}
