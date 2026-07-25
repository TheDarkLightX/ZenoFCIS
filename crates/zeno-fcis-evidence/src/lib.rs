//! Canonical formal-evidence envelopes and independent checker adapters.
//!
//! Proof and verification artifacts are first-class, independently checkable
//! ZenoFCIS promotion inputs. An evidence envelope binds the tool identity,
//! source commit, profile/schema/algorithm hashes, theorem or query identity,
//! assumptions, result, retained artifact digest, and coverage mode. The
//! envelope is fail-closed: missing, stale, inconclusive, timed-out,
//! malformed, unbound, or solver-disagreed evidence is rejected at
//! construction time.
//!
//! An external [`EvidenceChecker`] validates the retained artifact under
//! pinned tool semantics. The importer never trusts a tool's self-reported
//! result without an independent check of its artifact or replay surface.
//!
//! This crate is `no_std + alloc` and contains no `unsafe` code.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, EncodeError, Hash32};
use zeno_fcis_refine::{CoverageMode, ToolEvidence, ToolKind};

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

const MAX_TOOL_NAME_BYTES: usize = 64;
const MAX_TOOL_VERSION_BYTES: usize = 64;
const MAX_QUERY_ID_BYTES: usize = 128;
const MAX_ASSUMPTIONS: usize = 32;
const MAX_ASSUMPTION_BYTES: usize = 256;
const MAX_ENVELOPES: usize = 64;

// ---------------------------------------------------------------------------
// Tool identity
// ---------------------------------------------------------------------------

/// Pinned identity of the proof or verification tool that produced an artifact.
///
/// The binary hash binds the exact tool binary, preventing silent tool
/// substitution. The algorithm identifier distinguishes independent
/// implementations (e.g., RustCrypto vs. libcrux SHA-256).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolIdentity {
    name: Box<str>,
    version: Box<str>,
    binary_hash: Hash32,
}

impl ToolIdentity {
    /// Creates a pinned tool identity from validated fields.
    pub fn try_new(name: &str, version: &str, binary_hash: Hash32) -> Result<Self, EvidenceError> {
        validate_tool_name(name)?;
        validate_tool_version(version)?;
        if binary_hash == Hash32::ZERO {
            return Err(EvidenceError::ZeroBinaryHash);
        }
        Ok(Self {
            name: Box::from(name),
            version: Box::from(version),
            binary_hash,
        })
    }

    /// Returns the tool name (e.g., "kani", "lean", "z3").
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the pinned tool version string.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the binary commitment.
    #[must_use]
    pub const fn binary_hash(&self) -> Hash32 {
        self.binary_hash
    }
}

fn validate_tool_name(name: &str) -> Result<(), EvidenceError> {
    if name.is_empty() || !name.is_ascii() || name.len() > MAX_TOOL_NAME_BYTES {
        return Err(EvidenceError::InvalidToolName);
    }
    Ok(())
}

fn validate_tool_version(version: &str) -> Result<(), EvidenceError> {
    if version.is_empty() || !version.is_ascii() || version.len() > MAX_TOOL_VERSION_BYTES {
        return Err(EvidenceError::InvalidToolVersion);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Source bindings
// ---------------------------------------------------------------------------

/// Content-addressed bindings that anchor evidence to exact protocol artifacts.
///
/// Every field is non-zero, enforced at construction. A zero hash means the
/// evidence is unbound and must be rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceBindings {
    /// Source commit hash of the code under verification.
    source_commit: Hash32,
    /// Profile commitment being promoted.
    profile_hash: Hash32,
    /// Schema commitment for the profile.
    schema_hash: Hash32,
    /// Algorithm and codec version commitment.
    algorithm_hash: Hash32,
}

impl SourceBindings {
    /// Creates validated source bindings. Rejects any zero hash.
    pub fn try_new(
        source_commit: Hash32,
        profile_hash: Hash32,
        schema_hash: Hash32,
        algorithm_hash: Hash32,
    ) -> Result<Self, EvidenceError> {
        if source_commit == Hash32::ZERO {
            return Err(EvidenceError::UnboundSourceCommit);
        }
        if profile_hash == Hash32::ZERO {
            return Err(EvidenceError::UnboundProfile);
        }
        if schema_hash == Hash32::ZERO {
            return Err(EvidenceError::UnboundSchema);
        }
        if algorithm_hash == Hash32::ZERO {
            return Err(EvidenceError::UnboundAlgorithm);
        }
        Ok(Self {
            source_commit,
            profile_hash,
            schema_hash,
            algorithm_hash,
        })
    }

    /// Returns the source commit hash.
    #[must_use]
    pub const fn source_commit(&self) -> Hash32 {
        self.source_commit
    }

    /// Returns the profile commitment.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the schema commitment.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.schema_hash
    }

    /// Returns the algorithm commitment.
    #[must_use]
    pub const fn algorithm_hash(&self) -> Hash32 {
        self.algorithm_hash
    }

    /// Validates that every binding is non-zero.
    pub fn validate(&self) -> Result<(), EvidenceError> {
        if self.source_commit == Hash32::ZERO {
            return Err(EvidenceError::UnboundSourceCommit);
        }
        if self.profile_hash == Hash32::ZERO {
            return Err(EvidenceError::UnboundProfile);
        }
        if self.schema_hash == Hash32::ZERO {
            return Err(EvidenceError::UnboundSchema);
        }
        if self.algorithm_hash == Hash32::ZERO {
            return Err(EvidenceError::UnboundAlgorithm);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Evidence result
// ---------------------------------------------------------------------------

/// The outcome reported by a proof or verification tool.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum EvidenceResult {
    /// The tool established the claim (proof, verification, or model-check success).
    Proven = 0,
    /// The tool disproved the claim (counterexample found).
    Disproven = 1,
    /// The tool could not reach a conclusion within its bounds.
    Inconclusive = 2,
    /// The tool exceeded its time or resource budget.
    Timeout = 3,
    /// The tool crashed or produced malformed output.
    Crash = 4,
    /// Two independent solvers disagreed on the same query.
    SolverDisagreement = 5,
}

impl EvidenceResult {
    /// Returns true only when the result can support promotion.
    #[must_use]
    pub const fn is_conclusive_success(self) -> bool {
        matches!(self, Self::Proven)
    }

    /// Returns true when the result is a failure that blocks promotion.
    #[must_use]
    pub const fn is_blocking(self) -> bool {
        !self.is_conclusive_success()
    }
}

// ---------------------------------------------------------------------------
// Assumption
// ---------------------------------------------------------------------------

/// One named assumption under which the evidence was produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Assumption {
    label: Box<str>,
    statement_hash: Hash32,
}

impl Assumption {
    /// Creates a validated assumption.
    pub fn try_new(label: &str, statement_hash: Hash32) -> Result<Self, EvidenceError> {
        if label.is_empty() || !label.is_ascii() || label.len() > MAX_ASSUMPTION_BYTES {
            return Err(EvidenceError::InvalidAssumptionLabel);
        }
        if statement_hash == Hash32::ZERO {
            return Err(EvidenceError::ZeroAssumptionHash);
        }
        Ok(Self {
            label: Box::from(label),
            statement_hash,
        })
    }

    /// Returns the assumption label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the assumption statement commitment.
    #[must_use]
    pub const fn statement_hash(&self) -> Hash32 {
        self.statement_hash
    }
}

// ---------------------------------------------------------------------------
// Coverage declaration
// ---------------------------------------------------------------------------

/// Declared coverage scope for one piece of evidence.
///
/// This extends `zeno_fcis_refine::CoverageMode` with an explicit `Unbounded`
/// variant that is always rejected for promotion. Exhaustive finite coverage
/// requires an exact domain cardinality. Bounded coverage disclaims
/// completeness. Proof-assisted coverage defers to a theorem claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverageDeclaration {
    /// Every input in a finite domain was checked.
    ExhaustiveFinite {
        /// Commitment of the enumerated domain definition.
        domain_hash: Hash32,
        /// Exact domain cardinality.
        cardinality: u64,
    },
    /// A deterministic bounded case set was checked without a completeness claim.
    Bounded {
        /// Maximum admitted case count.
        case_budget: u64,
    },
    /// A theorem covers the large or infinite domain.
    ProofAssisted {
        /// Exact theorem statement commitment.
        theorem_claim: Hash32,
    },
    /// Coverage is unbounded — always rejected for promotion.
    Unbounded,
}

impl CoverageDeclaration {
    /// Converts to the refine crate's `CoverageMode`, returning `None` for
    /// `Unbounded` which has no refine-crate equivalent.
    #[must_use]
    pub fn to_coverage_mode(self) -> Option<CoverageMode> {
        match self {
            Self::ExhaustiveFinite {
                domain_hash,
                cardinality,
            } => Some(CoverageMode::Exhaustive {
                domain_hash,
                cardinality,
            }),
            Self::Bounded { case_budget } => Some(CoverageMode::Bounded { case_budget }),
            Self::ProofAssisted { theorem_claim } => {
                Some(CoverageMode::ProofAssisted { theorem_claim })
            }
            Self::Unbounded => None,
        }
    }

    /// Returns true if this coverage is admissible for promotion.
    #[must_use]
    pub const fn is_admissible(self) -> bool {
        !matches!(self, Self::Unbounded)
    }

    /// Validates that all hashes are non-zero and cardinalities are positive.
    pub fn validate(&self) -> Result<(), EvidenceError> {
        match self {
            Self::ExhaustiveFinite {
                domain_hash,
                cardinality,
            } => {
                if *domain_hash == Hash32::ZERO {
                    return Err(EvidenceError::ZeroDomainHash);
                }
                if *cardinality == 0 {
                    return Err(EvidenceError::ZeroCardinality);
                }
            }
            Self::Bounded { case_budget } => {
                if *case_budget == 0 {
                    return Err(EvidenceError::ZeroCaseBudget);
                }
            }
            Self::ProofAssisted { theorem_claim } => {
                if *theorem_claim == Hash32::ZERO {
                    return Err(EvidenceError::ZeroTheoremClaim);
                }
            }
            Self::Unbounded => {}
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Evidence envelope
// ---------------------------------------------------------------------------

/// A canonical, content-addressed evidence envelope.
///
/// Every field is bound at construction time. The envelope is immutable and
/// transitively owned. Only envelopes with `EvidenceResult::Proven` and
/// admissible coverage can be imported for promotion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceEnvelope {
    tool: ToolIdentity,
    kind: ToolKind,
    bindings: SourceBindings,
    query_id: Box<str>,
    claim_hash: Hash32,
    assumptions: Box<[Assumption]>,
    result: EvidenceResult,
    artifact_digest: Hash32,
    coverage: CoverageDeclaration,
}

impl EvidenceEnvelope {
    /// Creates a validated evidence envelope.
    ///
    /// Rejects:
    /// - missing or unbound source bindings
    /// - zero artifact digest
    /// - inconclusive, timed-out, crashed, or solver-disagreed results
    /// - unbounded coverage
    /// - excessive assumptions
    /// - invalid query identifiers
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        tool: ToolIdentity,
        kind: ToolKind,
        bindings: SourceBindings,
        query_id: &str,
        claim_hash: Hash32,
        assumptions: Vec<Assumption>,
        result: EvidenceResult,
        artifact_digest: Hash32,
        coverage: CoverageDeclaration,
    ) -> Result<Self, EvidenceError> {
        bindings.validate()?;
        validate_query_id(query_id)?;
        if claim_hash == Hash32::ZERO {
            return Err(EvidenceError::ZeroClaimHash);
        }
        if assumptions.len() > MAX_ASSUMPTIONS {
            return Err(EvidenceError::TooManyAssumptions);
        }
        if artifact_digest == Hash32::ZERO {
            return Err(EvidenceError::ZeroArtifactDigest);
        }
        if result.is_blocking() {
            return Err(EvidenceError::BlockingResult { result });
        }
        if !coverage.is_admissible() {
            return Err(EvidenceError::UnboundedCoverage);
        }
        coverage.validate()?;
        Ok(Self {
            tool,
            kind,
            bindings,
            query_id: Box::from(query_id),
            claim_hash,
            assumptions: assumptions.into_boxed_slice(),
            result,
            artifact_digest,
            coverage,
        })
    }

    /// Returns the tool identity.
    #[must_use]
    pub fn tool(&self) -> &ToolIdentity {
        &self.tool
    }

    /// Returns the evidence kind.
    #[must_use]
    pub const fn kind(&self) -> ToolKind {
        self.kind
    }

    /// Returns the source bindings.
    #[must_use]
    pub const fn bindings(&self) -> SourceBindings {
        self.bindings
    }

    /// Returns the theorem or query identifier.
    #[must_use]
    pub fn query_id(&self) -> &str {
        &self.query_id
    }

    /// Returns the claim commitment (hash of the theorem/query statement).
    #[must_use]
    pub const fn claim_hash(&self) -> Hash32 {
        self.claim_hash
    }

    /// Returns the declared assumptions.
    #[must_use]
    pub fn assumptions(&self) -> &[Assumption] {
        &self.assumptions
    }

    /// Returns the evidence result.
    #[must_use]
    pub const fn result(&self) -> EvidenceResult {
        self.result
    }

    /// Returns the retained artifact digest.
    #[must_use]
    pub const fn artifact_digest(&self) -> Hash32 {
        self.artifact_digest
    }

    /// Returns the coverage declaration.
    #[must_use]
    pub const fn coverage(&self) -> CoverageDeclaration {
        self.coverage
    }

    /// Converts to a `ToolEvidence` for the refine crate's promotion pipeline.
    ///
    /// The `claim` is the provided claim hash, the `artifact` is the retained
    /// artifact digest, and the `toolchain` is the tool binary hash.
    #[must_use]
    pub fn to_tool_evidence(&self) -> ToolEvidence {
        ToolEvidence::new(
            self.kind,
            self.claim_hash,
            self.artifact_digest,
            self.tool.binary_hash,
        )
    }
}

fn validate_query_id(query_id: &str) -> Result<(), EvidenceError> {
    if query_id.is_empty() || !query_id.is_ascii() || query_id.len() > MAX_QUERY_ID_BYTES {
        return Err(EvidenceError::InvalidQueryId);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Canonical encoding
// ---------------------------------------------------------------------------

impl CanonicalEncode for EvidenceEnvelope {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_text(output, self.tool.name())?;
        put_text(output, self.tool.version())?;
        output.extend_from_slice(self.tool.binary_hash.as_bytes());
        output.push(self.kind as u8);
        output.extend_from_slice(self.bindings.source_commit().as_bytes());
        output.extend_from_slice(self.bindings.profile_hash().as_bytes());
        output.extend_from_slice(self.bindings.schema_hash().as_bytes());
        output.extend_from_slice(self.bindings.algorithm_hash().as_bytes());
        put_text(output, &self.query_id)?;
        output.extend_from_slice(self.claim_hash.as_bytes());
        let assumption_count =
            u16::try_from(self.assumptions.len()).map_err(|_| EncodeError::LengthOverflow)?;
        output.extend_from_slice(&assumption_count.to_be_bytes());
        for assumption in self.assumptions.iter() {
            put_text(output, assumption.label())?;
            output.extend_from_slice(assumption.statement_hash().as_bytes());
        }
        output.push(self.result as u8);
        output.extend_from_slice(self.artifact_digest.as_bytes());
        encode_coverage(output, self.coverage)?;
        Ok(())
    }
}

fn encode_coverage(output: &mut Vec<u8>, coverage: CoverageDeclaration) -> Result<(), EncodeError> {
    match coverage {
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash,
            cardinality,
        } => {
            output.push(0);
            output.extend_from_slice(domain_hash.as_bytes());
            output.extend_from_slice(&cardinality.to_be_bytes());
        }
        CoverageDeclaration::Bounded { case_budget } => {
            output.push(1);
            output.extend_from_slice(&case_budget.to_be_bytes());
        }
        CoverageDeclaration::ProofAssisted { theorem_claim } => {
            output.push(2);
            output.extend_from_slice(theorem_claim.as_bytes());
        }
        CoverageDeclaration::Unbounded => output.push(3),
    }
    Ok(())
}

fn put_text(output: &mut Vec<u8>, text: &str) -> Result<(), EncodeError> {
    let length = u32::try_from(text.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(text.as_bytes());
    Ok(())
}

// ---------------------------------------------------------------------------
// Independent checker
// ---------------------------------------------------------------------------

/// Independent verifier for retained evidence artifacts.
///
/// An importer must not trust a tool's self-reported result without checking
/// its retained artifact or replay surface. The checker receives the complete
/// envelope and returns `true` only when the artifact establishes the claim
/// under the pinned tool semantics.
pub trait EvidenceChecker {
    /// Returns true only when the retained artifact establishes the claim.
    fn check(&self, envelope: &EvidenceEnvelope) -> bool;
}

/// A checker that always rejects. Used as a fail-closed default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RejectAllChecker;

impl EvidenceChecker for RejectAllChecker {
    fn check(&self, _envelope: &EvidenceEnvelope) -> bool {
        false
    }
}

/// A checker that accepts envelopes with non-zero claim hash, non-zero
/// artifact digest, non-zero binary hash, and a conclusive success result.
/// This is a minimal structural check, not a proof verification.
/// Production code must supply a real checker.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StructuralChecker;

impl EvidenceChecker for StructuralChecker {
    fn check(&self, envelope: &EvidenceEnvelope) -> bool {
        envelope.claim_hash() != Hash32::ZERO
            && envelope.artifact_digest() != Hash32::ZERO
            && envelope.tool().binary_hash() != Hash32::ZERO
            && envelope.result().is_conclusive_success()
    }
}

// ---------------------------------------------------------------------------
// Evidence importer
// ---------------------------------------------------------------------------

/// Validates and imports evidence envelopes for promotion.
///
/// The importer is fail-closed: any envelope that fails validation or whose
/// retained artifact is not independently checked is rejected. The importer
/// also requires mounted runtime refinement evidence for any production
/// promotion report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceImporter {
    bindings: SourceBindings,
    envelopes: Box<[EvidenceEnvelope]>,
    has_runtime_refinement: bool,
}

impl EvidenceImporter {
    /// Creates an importer bound to exact source bindings.
    pub fn try_new(bindings: SourceBindings) -> Result<Self, EvidenceError> {
        bindings.validate()?;
        Ok(Self {
            bindings,
            envelopes: Box::from([]),
            has_runtime_refinement: false,
        })
    }

    /// Returns the source bindings.
    #[must_use]
    pub const fn bindings(&self) -> SourceBindings {
        self.bindings
    }

    /// Returns the imported envelopes.
    #[must_use]
    pub fn envelopes(&self) -> &[EvidenceEnvelope] {
        &self.envelopes
    }

    /// Returns whether mounted runtime refinement evidence is present.
    #[must_use]
    pub const fn has_runtime_refinement(&self) -> bool {
        self.has_runtime_refinement
    }

    /// Imports a batch of envelopes after validation and independent checking.
    ///
    /// Every envelope must:
    /// - bind the same source bindings as the importer
    /// - pass the independent checker
    /// - not duplicate an existing tool kind
    pub fn import<C: EvidenceChecker>(
        &mut self,
        envelopes: Vec<EvidenceEnvelope>,
        checker: &C,
    ) -> Result<(), EvidenceError> {
        if self.envelopes.len() + envelopes.len() > MAX_ENVELOPES {
            return Err(EvidenceError::TooManyEnvelopes);
        }
        let mut merged: Vec<EvidenceEnvelope> = self.envelopes.to_vec();
        for envelope in &envelopes {
            verify_bindings_match(&self.bindings, envelope)?;
            if !checker.check(envelope) {
                return Err(EvidenceError::ArtifactCheckFailed {
                    kind: envelope.kind(),
                });
            }
            if merged.iter().any(|existing| existing.kind == envelope.kind) {
                return Err(EvidenceError::DuplicateToolKind {
                    kind: envelope.kind(),
                });
            }
            merged.push(envelope.clone());
        }
        merged.sort_by_key(|e| e.kind);
        self.has_runtime_refinement |= envelopes
            .iter()
            .any(|e| e.kind == ToolKind::RuntimeRefinement);
        self.envelopes = merged.into_boxed_slice();
        Ok(())
    }

    /// Converts imported envelopes to `ToolEvidence` for the refine crate.
    #[must_use]
    pub fn to_tool_evidence(&self) -> Vec<ToolEvidence> {
        self.envelopes
            .iter()
            .map(EvidenceEnvelope::to_tool_evidence)
            .collect()
    }

    /// Returns the coverage mode from the strongest available evidence.
    ///
    /// Priority: ExhaustiveFinite > ProofAssisted > Bounded.
    /// Returns `None` if no admissible coverage is available.
    #[must_use]
    pub fn best_coverage(&self) -> Option<CoverageMode> {
        let mut best: Option<CoverageDeclaration> = None;
        for envelope in self.envelopes.iter() {
            best = match (best, envelope.coverage()) {
                (None, cov) => Some(cov),
                (
                    Some(CoverageDeclaration::Bounded { .. }),
                    cov @ (CoverageDeclaration::ExhaustiveFinite { .. }
                    | CoverageDeclaration::ProofAssisted { .. }),
                ) => Some(cov),
                (
                    Some(CoverageDeclaration::ProofAssisted { .. }),
                    cov @ CoverageDeclaration::ExhaustiveFinite { .. },
                ) => Some(cov),
                (current, _) => current,
            };
        }
        best.and_then(CoverageDeclaration::to_coverage_mode)
    }
}

fn verify_bindings_match(
    expected: &SourceBindings,
    envelope: &EvidenceEnvelope,
) -> Result<(), EvidenceError> {
    let actual = envelope.bindings();
    if actual.source_commit() != expected.source_commit() {
        return Err(EvidenceError::StaleSourceCommit);
    }
    if actual.profile_hash() != expected.profile_hash() {
        return Err(EvidenceError::ProfileMismatch);
    }
    if actual.schema_hash() != expected.schema_hash() {
        return Err(EvidenceError::SchemaMismatch);
    }
    if actual.algorithm_hash() != expected.algorithm_hash() {
        return Err(EvidenceError::AlgorithmMismatch);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Promotion gate
// ---------------------------------------------------------------------------

/// Fail-closed promotion gate requiring mounted runtime refinement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromotionGate {
    required_tools: Box<[ToolKind]>,
    require_runtime_refinement: bool,
}

impl PromotionGate {
    /// Creates a promotion gate with required tool kinds.
    pub fn try_new(
        mut required_tools: Vec<ToolKind>,
        require_runtime_refinement: bool,
    ) -> Result<Self, EvidenceError> {
        required_tools.sort();
        if required_tools.len() > MAX_ENVELOPES
            || required_tools.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(EvidenceError::InvalidPromotionGate);
        }
        Ok(Self {
            required_tools: required_tools.into_boxed_slice(),
            require_runtime_refinement,
        })
    }

    /// Returns required tool kinds.
    #[must_use]
    pub fn required_tools(&self) -> &[ToolKind] {
        &self.required_tools
    }

    /// Returns whether mounted runtime refinement is required.
    #[must_use]
    pub const fn require_runtime_refinement(&self) -> bool {
        self.require_runtime_refinement
    }

    /// Evaluates whether the importer satisfies the promotion gate.
    ///
    /// Returns a list of blockers. An empty list means the gate is satisfied.
    #[must_use]
    pub fn evaluate(&self, importer: &EvidenceImporter) -> Vec<PromotionBlocker> {
        let mut blockers = Vec::new();
        for &kind in self.required_tools.iter() {
            if !importer.envelopes().iter().any(|e| e.kind == kind) {
                blockers.push(PromotionBlocker::MissingToolEvidence { kind });
            }
        }
        if self.require_runtime_refinement && !importer.has_runtime_refinement() {
            blockers.push(PromotionBlocker::MissingRuntimeRefinement);
        }
        blockers
    }

    /// Returns true only when the gate is satisfied.
    #[must_use]
    pub fn is_satisfied(&self, importer: &EvidenceImporter) -> bool {
        self.evaluate(importer).is_empty()
    }
}

/// One fail-closed promotion blocker from the evidence gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromotionBlocker {
    /// A required tool kind is absent from the imported evidence.
    MissingToolEvidence {
        /// Required evidence kind.
        kind: ToolKind,
    },
    /// Mounted runtime refinement evidence is absent.
    MissingRuntimeRefinement,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Evidence construction, validation, or import failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvidenceError {
    /// Tool name was empty, non-ASCII, or too long.
    InvalidToolName,
    /// Tool version was empty, non-ASCII, or too long.
    InvalidToolVersion,
    /// Tool binary hash was zero.
    ZeroBinaryHash,
    /// Source commit binding was zero.
    UnboundSourceCommit,
    /// Profile binding was zero.
    UnboundProfile,
    /// Schema binding was zero.
    UnboundSchema,
    /// Algorithm binding was zero.
    UnboundAlgorithm,
    /// Query identifier was empty, non-ASCII, or too long.
    InvalidQueryId,
    /// Claim hash was zero.
    ZeroClaimHash,
    /// Assumption label was empty, non-ASCII, or too long.
    InvalidAssumptionLabel,
    /// Assumption statement hash was zero.
    ZeroAssumptionHash,
    /// Too many assumptions.
    TooManyAssumptions,
    /// Artifact digest was zero.
    ZeroArtifactDigest,
    /// Evidence result blocks promotion.
    BlockingResult {
        /// The blocking result.
        result: EvidenceResult,
    },
    /// Coverage was declared as unbounded.
    UnboundedCoverage,
    /// Exhaustive finite coverage had a zero domain hash.
    ZeroDomainHash,
    /// Exhaustive finite coverage had zero cardinality.
    ZeroCardinality,
    /// Bounded coverage had a zero case budget.
    ZeroCaseBudget,
    /// Proof-assisted coverage had a zero theorem claim hash.
    ZeroTheoremClaim,
    /// Too many envelopes for one importer.
    TooManyEnvelopes,
    /// Envelope source commit does not match importer bindings.
    StaleSourceCommit,
    /// Envelope profile does not match importer bindings.
    ProfileMismatch,
    /// Envelope schema does not match importer bindings.
    SchemaMismatch,
    /// Envelope algorithm does not match importer bindings.
    AlgorithmMismatch,
    /// Independent artifact check failed.
    ArtifactCheckFailed {
        /// Evidence kind that failed.
        kind: ToolKind,
    },
    /// Duplicate tool kind in imported evidence.
    DuplicateToolKind {
        /// Duplicated evidence kind.
        kind: ToolKind,
    },
    /// Promotion gate has duplicate or excessive tool requirements.
    InvalidPromotionGate,
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToolName => formatter.write_str("invalid tool name"),
            Self::InvalidToolVersion => formatter.write_str("invalid tool version"),
            Self::ZeroBinaryHash => formatter.write_str("tool binary hash is zero"),
            Self::UnboundSourceCommit => formatter.write_str("source commit binding is zero"),
            Self::UnboundProfile => formatter.write_str("profile binding is zero"),
            Self::UnboundSchema => formatter.write_str("schema binding is zero"),
            Self::UnboundAlgorithm => formatter.write_str("algorithm binding is zero"),
            Self::InvalidQueryId => formatter.write_str("invalid query identifier"),
            Self::ZeroClaimHash => formatter.write_str("claim hash is zero"),
            Self::InvalidAssumptionLabel => formatter.write_str("invalid assumption label"),
            Self::ZeroAssumptionHash => formatter.write_str("assumption statement hash is zero"),
            Self::TooManyAssumptions => formatter.write_str("too many assumptions"),
            Self::ZeroArtifactDigest => formatter.write_str("artifact digest is zero"),
            Self::BlockingResult { result } => {
                write!(formatter, "evidence result {result:?} blocks promotion")
            }
            Self::UnboundedCoverage => formatter.write_str("coverage is unbounded"),
            Self::ZeroDomainHash => formatter.write_str("exhaustive finite domain hash is zero"),
            Self::ZeroCardinality => formatter.write_str("exhaustive finite cardinality is zero"),
            Self::ZeroCaseBudget => formatter.write_str("bounded case budget is zero"),
            Self::ZeroTheoremClaim => {
                formatter.write_str("proof-assisted theorem claim hash is zero")
            }
            Self::TooManyEnvelopes => formatter.write_str("too many evidence envelopes"),
            Self::StaleSourceCommit => formatter.write_str("envelope source commit is stale"),
            Self::ProfileMismatch => formatter.write_str("envelope profile does not match"),
            Self::SchemaMismatch => formatter.write_str("envelope schema does not match"),
            Self::AlgorithmMismatch => formatter.write_str("envelope algorithm does not match"),
            Self::ArtifactCheckFailed { kind } => {
                write!(formatter, "independent artifact check failed for {kind:?}")
            }
            Self::DuplicateToolKind { kind } => {
                write!(formatter, "duplicate tool evidence kind {kind:?}")
            }
            Self::InvalidPromotionGate => formatter.write_str("invalid promotion gate"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EvidenceError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
