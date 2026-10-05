//! Canonical formal-evidence envelopes and independent checker adapters.
//!
//! Proof and verification artifacts are first-class, independently checkable
//! ZenoFCIS promotion inputs. An evidence envelope binds the tool identity,
//! profile/schema/algorithm hashes, theorem or query identity,
//! assumptions, reported result, retained artifact digest, and declared coverage.
//! Construction rejects malformed metadata, zero required bindings, blocking
//! results, and unbounded coverage. Import checks the configured subject
//! bindings and the actual artifact; construction alone does not check them.
//!
//! The importer checks exact retained bytes and their digest before calling
//! an external [`EvidenceChecker`]. Its answer remains an attestation under
//! the selected checker semantics; the library does not promote it to kernel
//! proof. [`StructuralChecker`] performs only the documented structural checks.
//!
//! This crate is `no_std + alloc` and contains no `unsafe` code.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

pub use zeno_fcis_codec::EvidenceArtifact;
use zeno_fcis_codec::{CommitmentHasher, EncodeError, Hash32};
use zeno_fcis_refine::{CoverageMode, EvidenceKind, ToolEvidence};

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
/// The binary hash is a declared commitment to the producer executable. It
/// does not by itself establish which executable actually ran.
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
        profile_hash: Hash32,
        schema_hash: Hash32,
        algorithm_hash: Hash32,
    ) -> Result<Self, EvidenceError> {
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
            profile_hash,
            schema_hash,
            algorithm_hash,
        })
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
#[non_exhaustive]
pub enum EvidenceResult {
    /// An external tool reports success. This is an attestation, not kernel proof.
    Attested = 0,
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
        matches!(self, Self::Attested)
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
/// Every declared field is structurally validated at construction. The envelope
/// alone does not establish the claim or artifact. It is immutable and
/// transitively owned. Only envelopes with `EvidenceResult::Attested` and
/// admissible coverage can be imported for promotion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceEnvelope {
    tool: ToolIdentity,
    kind: EvidenceKind,
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
        kind: EvidenceKind,
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
    pub const fn kind(&self) -> EvidenceKind {
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
}

// ---------------------------------------------------------------------------
// QueryId newtype
// ---------------------------------------------------------------------------

/// A validated theorem or model-checking query identifier.
///
/// This newtype prevents stringly-typed APIs by enforcing that query IDs
/// are non-empty, ASCII, and bounded at construction time.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct QueryId {
    inner: Box<str>,
}

impl QueryId {
    /// Creates a validated query identifier.
    pub fn try_new(value: &str) -> Result<Self, EvidenceError> {
        validate_query_id(value)?;
        Ok(Self {
            inner: Box::from(value),
        })
    }

    /// Returns the query identifier string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl core::fmt::Display for QueryId {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.inner)
    }
}

impl From<QueryId> for Box<str> {
    fn from(value: QueryId) -> Self {
        value.inner
    }
}

// ---------------------------------------------------------------------------
// EvidenceResult decoding
// ---------------------------------------------------------------------------

impl TryFrom<u8> for EvidenceResult {
    type Error = EvidenceError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Attested),
            1 => Ok(Self::Disproven),
            2 => Ok(Self::Inconclusive),
            3 => Ok(Self::Timeout),
            4 => Ok(Self::Crash),
            5 => Ok(Self::SolverDisagreement),
            _ => Err(EvidenceError::InvalidResultTag),
        }
    }
}

// ---------------------------------------------------------------------------
// Evidence envelope builder
// ---------------------------------------------------------------------------

/// Builder for [`EvidenceEnvelope`].
///
/// Provides incremental, validated construction of evidence envelopes.
/// All required fields must be set before calling [`build`](Self::build).
/// The builder is consumed on build, preventing reuse after a failed attempt.
///
/// # Example
///
/// ```
/// use zeno_fcis_codec::Hash32;
/// use zeno_fcis_evidence::{
///     Assumption, CoverageDeclaration, EvidenceEnvelopeBuilder,
///     EvidenceResult, SourceBindings, ToolIdentity,
/// };
/// use zeno_fcis_refine::EvidenceKind;
///
/// let tool = ToolIdentity::try_new("kani", "0.62.0", Hash32::new([1; 32])).unwrap();
/// let bindings = SourceBindings::try_new(
///     Hash32::new([3; 32]),
///     Hash32::new([4; 32]),
///     Hash32::new([5; 32]),
/// ).unwrap();
/// let envelope = EvidenceEnvelopeBuilder::new(tool, EvidenceKind::Kani, bindings)
///     .query_id("theorem_001")
///     .claim_hash(Hash32::new([6; 32]))
///     .artifact_digest(Hash32::new([7; 32]))
///     .result(EvidenceResult::Attested)
///     .coverage(CoverageDeclaration::Bounded { case_budget: 100 })
///     .build()
///     .unwrap();
/// assert_eq!(envelope.kind(), EvidenceKind::Kani);
/// ```
#[derive(Clone, Debug)]
pub struct EvidenceEnvelopeBuilder {
    tool: ToolIdentity,
    kind: EvidenceKind,
    bindings: SourceBindings,
    query_id: Option<Box<str>>,
    claim_hash: Option<Hash32>,
    assumptions: Vec<Assumption>,
    result: Option<EvidenceResult>,
    artifact_digest: Option<Hash32>,
    coverage: Option<CoverageDeclaration>,
}

impl EvidenceEnvelopeBuilder {
    /// Creates a builder with the required identity fields.
    #[must_use]
    pub fn new(tool: ToolIdentity, kind: EvidenceKind, bindings: SourceBindings) -> Self {
        Self {
            tool,
            kind,
            bindings,
            query_id: None,
            claim_hash: None,
            assumptions: Vec::new(),
            result: None,
            artifact_digest: None,
            coverage: None,
        }
    }

    /// Sets the query identifier.
    #[must_use]
    pub fn query_id(mut self, value: &str) -> Self {
        self.query_id = Some(Box::from(value));
        self
    }

    /// Sets the claim hash.
    #[must_use]
    pub fn claim_hash(mut self, value: Hash32) -> Self {
        self.claim_hash = Some(value);
        self
    }

    /// Sets the artifact digest.
    #[must_use]
    pub fn artifact_digest(mut self, value: Hash32) -> Self {
        self.artifact_digest = Some(value);
        self
    }

    /// Sets the evidence result.
    #[must_use]
    pub fn result(mut self, value: EvidenceResult) -> Self {
        self.result = Some(value);
        self
    }

    /// Sets the coverage declaration.
    #[must_use]
    pub fn coverage(mut self, value: CoverageDeclaration) -> Self {
        self.coverage = Some(value);
        self
    }

    /// Adds an assumption.
    #[must_use]
    pub fn assumption(mut self, value: Assumption) -> Self {
        self.assumptions.push(value);
        self
    }

    /// Sets all assumptions, replacing any previously added.
    #[must_use]
    pub fn assumptions(mut self, values: Vec<Assumption>) -> Self {
        self.assumptions = values;
        self
    }

    /// Builds the validated evidence envelope.
    ///
    /// Returns an error if any required field is missing or invalid.
    pub fn build(self) -> Result<EvidenceEnvelope, EvidenceError> {
        let query_id = self.query_id.ok_or(EvidenceError::MissingQueryId)?;
        let claim_hash = self.claim_hash.ok_or(EvidenceError::MissingClaimHash)?;
        let result = self.result.ok_or(EvidenceError::MissingResult)?;
        let artifact_digest = self
            .artifact_digest
            .ok_or(EvidenceError::MissingArtifactDigest)?;
        let coverage = self.coverage.ok_or(EvidenceError::MissingCoverage)?;
        EvidenceEnvelope::try_new(
            self.tool,
            self.kind,
            self.bindings,
            &query_id,
            claim_hash,
            self.assumptions,
            result,
            artifact_digest,
            coverage,
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

impl EvidenceEnvelope {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-EVIDENCE\0");
        output.extend_from_slice(&2_u16.to_be_bytes());
        put_text(output, self.tool.name())?;
        put_text(output, self.tool.version())?;
        output.extend_from_slice(self.tool.binary_hash.as_bytes());
        output.push(self.kind as u8);
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

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
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
/// envelope and exact bytes. Its result is an external attestation under
/// the selected semantics, not a kernel theorem.
pub trait EvidenceChecker {
    /// Returns true only when the retained artifact establishes the claim.
    fn check(&self, envelope: &EvidenceEnvelope, artifact: &[u8]) -> bool;
}

/// A checker that always rejects. Used as a fail-closed default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RejectAllChecker;

impl EvidenceChecker for RejectAllChecker {
    fn check(&self, _envelope: &EvidenceEnvelope, _artifact: &[u8]) -> bool {
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
    fn check(&self, envelope: &EvidenceEnvelope, _artifact: &[u8]) -> bool {
        envelope.claim_hash() != Hash32::ZERO
            && envelope.artifact_digest() != Hash32::ZERO
            && envelope.tool().binary_hash() != Hash32::ZERO
            && envelope.result().is_conclusive_success()
    }
}

// ---------------------------------------------------------------------------
// Evidence importer
// ---------------------------------------------------------------------------

/// An untrusted envelope paired with the exact retained artifact bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceInput {
    envelope: EvidenceEnvelope,
    artifact: EvidenceArtifact,
}

impl EvidenceInput {
    /// Owns a proposed pair; import independently checks all bindings and bytes.
    #[must_use]
    pub const fn new(envelope: EvidenceEnvelope, artifact: EvidenceArtifact) -> Self {
        Self { envelope, artifact }
    }

    /// Borrows the claimed envelope.
    #[must_use]
    pub const fn envelope(&self) -> &EvidenceEnvelope {
        &self.envelope
    }

    /// Borrows the exact retained artifact and computed digest.
    #[must_use]
    pub const fn artifact(&self) -> &EvidenceArtifact {
        &self.artifact
    }

    /// Creates an untrusted refinement proposal retaining these same bytes.
    /// The consuming refinement check independently rechecks the digest.
    #[must_use]
    pub fn to_tool_evidence(&self) -> ToolEvidence {
        ToolEvidence::new(
            self.envelope.kind,
            self.envelope.claim_hash,
            self.artifact.clone(),
            self.envelope.tool.binary_hash,
        )
    }
}

/// Validates and imports evidence envelopes for promotion.
///
/// The importer is fail-closed: any envelope that fails validation or whose
/// retained artifact is not independently checked is rejected. The importer
/// also requires mounted runtime refinement evidence for any production
/// promotion report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceImporter {
    bindings: SourceBindings,
    inputs: Box<[EvidenceInput]>,
    has_runtime_refinement: bool,
}

impl EvidenceImporter {
    /// Creates an importer bound to exact source bindings.
    pub fn try_new(bindings: SourceBindings) -> Result<Self, EvidenceError> {
        bindings.validate()?;
        Ok(Self {
            bindings,
            inputs: Box::from([]),
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
    pub fn envelopes(&self) -> impl ExactSizeIterator<Item = &EvidenceEnvelope> {
        self.inputs.iter().map(EvidenceInput::envelope)
    }

    /// Borrows all imported envelopes with their retained immutable bytes.
    #[must_use]
    pub fn inputs(&self) -> &[EvidenceInput] {
        &self.inputs
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
    /// - have a matching artifact digest under the selected hash provider
    /// - pass the independent checker over those exact bytes
    /// - not duplicate an existing evidence kind
    pub fn import<H: CommitmentHasher, C: EvidenceChecker>(
        &mut self,
        inputs: Vec<EvidenceInput>,
        checker: &C,
    ) -> Result<(), EvidenceError> {
        if self.inputs.len() + inputs.len() > MAX_ENVELOPES {
            return Err(EvidenceError::TooManyEnvelopes);
        }
        let mut merged = self.inputs.to_vec();
        for input in &inputs {
            let envelope = input.envelope();
            verify_bindings_match(&self.bindings, envelope)?;
            if !input.artifact.matches::<H>()
                || input.artifact.digest() != envelope.artifact_digest()
            {
                return Err(EvidenceError::ArtifactDigestMismatch {
                    kind: envelope.kind(),
                });
            }
            if !checker.check(envelope, input.artifact.bytes()) {
                return Err(EvidenceError::ArtifactCheckFailed {
                    kind: envelope.kind(),
                });
            }
            if merged
                .iter()
                .any(|existing| existing.envelope.kind == envelope.kind)
            {
                return Err(EvidenceError::DuplicateEvidenceKind {
                    kind: envelope.kind(),
                });
            }
            merged.push(input.clone());
        }
        merged.sort_by_key(|input| input.envelope.kind);
        self.has_runtime_refinement = merged
            .iter()
            .any(|input| input.envelope.kind == EvidenceKind::RuntimeRefinement);
        self.inputs = merged.into_boxed_slice();
        Ok(())
    }

    /// Converts imported envelopes to `ToolEvidence` for the refine crate.
    #[must_use]
    pub fn to_tool_evidence(&self) -> Vec<ToolEvidence> {
        self.inputs
            .iter()
            .map(EvidenceInput::to_tool_evidence)
            .collect()
    }

    /// Returns the coverage mode from the strongest available evidence.
    ///
    /// Priority: ExhaustiveFinite > ProofAssisted > Bounded.
    /// Returns `None` if no admissible coverage is available.
    #[must_use]
    pub fn best_coverage(&self) -> Option<CoverageMode> {
        let mut best: Option<CoverageDeclaration> = None;
        for envelope in self.envelopes() {
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
    required_tools: Box<[EvidenceKind]>,
    require_runtime_refinement: bool,
}

impl PromotionGate {
    /// Creates a promotion gate with required tool kinds.
    pub fn try_new(
        mut required_tools: Vec<EvidenceKind>,
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
    pub fn required_tools(&self) -> &[EvidenceKind] {
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
            if !importer.envelopes().any(|e| e.kind == kind) {
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
#[non_exhaustive]
pub enum PromotionBlocker {
    /// A required tool kind is absent from the imported evidence.
    MissingToolEvidence {
        /// Required evidence kind.
        kind: EvidenceKind,
    },
    /// Mounted runtime refinement evidence is absent.
    MissingRuntimeRefinement,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Evidence construction, validation, or import failure.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EvidenceError {
    /// Tool name was empty, non-ASCII, or too long.
    InvalidToolName,
    /// Tool version was empty, non-ASCII, or too long.
    InvalidToolVersion,
    /// Tool binary hash was zero.
    ZeroBinaryHash,
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
    /// Envelope profile does not match importer bindings.
    ProfileMismatch,
    /// Envelope schema does not match importer bindings.
    SchemaMismatch,
    /// Envelope algorithm does not match importer bindings.
    AlgorithmMismatch,
    /// Independent artifact check failed.
    ArtifactCheckFailed {
        /// Evidence kind that failed.
        kind: EvidenceKind,
    },
    /// The retained bytes do not match the envelope digest or selected provider.
    ArtifactDigestMismatch {
        /// Evidence kind whose artifact mismatched.
        kind: EvidenceKind,
    },
    /// Duplicate evidence kind in imported inputs.
    DuplicateEvidenceKind {
        /// Duplicated evidence kind.
        kind: EvidenceKind,
    },
    /// Promotion gate has duplicate or excessive tool requirements.
    InvalidPromotionGate,
    /// Builder was missing the query identifier.
    MissingQueryId,
    /// Builder was missing the claim hash.
    MissingClaimHash,
    /// Builder was missing the evidence result.
    MissingResult,
    /// Builder was missing the artifact digest.
    MissingArtifactDigest,
    /// Builder was missing the coverage declaration.
    MissingCoverage,
    /// Evidence result tag was not a valid variant.
    InvalidResultTag,
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToolName => formatter.write_str("invalid tool name"),
            Self::InvalidToolVersion => formatter.write_str("invalid tool version"),
            Self::ZeroBinaryHash => formatter.write_str("tool binary hash is zero"),
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
            Self::ProfileMismatch => formatter.write_str("envelope profile does not match"),
            Self::SchemaMismatch => formatter.write_str("envelope schema does not match"),
            Self::AlgorithmMismatch => formatter.write_str("envelope algorithm does not match"),
            Self::ArtifactCheckFailed { kind } => {
                write!(formatter, "independent artifact check failed for {kind:?}")
            }
            Self::ArtifactDigestMismatch { kind } => {
                write!(formatter, "retained artifact digest mismatch for {kind:?}")
            }
            Self::DuplicateEvidenceKind { kind } => {
                write!(formatter, "duplicate tool evidence kind {kind:?}")
            }
            Self::InvalidPromotionGate => formatter.write_str("invalid promotion gate"),
            Self::MissingQueryId => formatter.write_str("missing query identifier"),
            Self::MissingClaimHash => formatter.write_str("missing claim hash"),
            Self::MissingResult => formatter.write_str("missing evidence result"),
            Self::MissingArtifactDigest => formatter.write_str("missing artifact digest"),
            Self::MissingCoverage => formatter.write_str("missing coverage declaration"),
            Self::InvalidResultTag => formatter.write_str("invalid evidence result tag"),
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
