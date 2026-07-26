//! Exact runtime-to-model refinement and proof-assisted promotion evidence.
//!
//! A mounted runtime is normalized into the same data-only decision surface as
//! the model. Refinement compares every authority-bearing artifact, not merely
//! the final state root. Promotion evidence is fail-closed and content-bound;
//! an external [`ProofVerifier`] remains responsible for validating proof
//! artifacts under pinned tool semantics.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, EncodeError, Hash32};
use zeno_fcis_core::DecisionKind;
use zeno_fcis_receipt::{CommitBundle, RejectReceipt};

const MAX_REASON_BYTES: usize = 96;
const MAX_ARTIFACT_BYTES: usize = 64 * 1024 * 1024;
const MAX_CASES: usize = 1_000_000;
const MAX_TOOL_EVIDENCE: usize = 64;

/// Complete normalized authority surface for one decision.
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

/// Validated normalized decision suitable for exact comparison.
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

/// One mounted refinement case.
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

/// External verifier for formal/differential artifacts.
pub trait ProofVerifier {
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

/// Content-addressed evidence submitted for promotion.
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

/// One fail-closed promotion blocker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PromotionBlocker {
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
    /// Bounded evidence exceeds its declared budget or contains no cases.
    InvalidBoundedCoverage,
    /// Proof-assisted coverage omits evidence for its theorem claim.
    MissingTheoremEvidence,
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

/// Evaluates exact refinement and proof evidence under one promotion policy.
#[must_use]
pub fn evaluate_promotion<V: ProofVerifier>(
    policy: &PromotionPolicy,
    evidence: &PromotionEvidence,
    verifier: &V,
) -> PromotionReport {
    let mut blockers = Vec::new();
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
    /// Promotion policy has duplicate or excessive tool requirements.
    InvalidPromotionPolicy,
    /// Evidence exceeds its case or tool bound.
    EvidenceTooLarge,
    /// Case identifier is duplicated.
    DuplicateCase,
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
            Self::InvalidPromotionPolicy => formatter.write_str("invalid promotion policy"),
            Self::EvidenceTooLarge => formatter.write_str("promotion evidence exceeds its bound"),
            Self::DuplicateCase => formatter.write_str("refinement case is duplicated"),
            Self::DuplicateToolEvidence => formatter.write_str("tool evidence kind is duplicated"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    struct ExactVerifier;

    impl ProofVerifier for ExactVerifier {
        fn verify(&self, evidence: ToolEvidence) -> bool {
            evidence.claim() == evidence.artifact() && evidence.toolchain() != Hash32::ZERO
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
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
    fn bounded_promotion_requires_nonempty_exact_cases() {
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
        assert!(evaluate_promotion(&policy, &evidence, &ExactVerifier).is_promotable());
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
            evaluate_promotion(&policy, &evidence, &ExactVerifier).blockers(),
            [PromotionBlocker::MissingTheoremEvidence]
        ));
    }
}
