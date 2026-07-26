//! Project-neutral checked backend protocol.
//!
//! Private or external engines such as synthesizers, theorem provers, model
//! checkers, compilers, optimizers, and LLM-assisted design systems may propose
//! closed artifacts through this protocol. They do not choose project schemas,
//! stable identifiers, resource bounds, verification claims, or promotion
//! status. Every accepted response is independently attested and content-bound.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_project::StableName;
use zeno_fcis_synthesis::{Assignment, CandidateChecker, CheckResult, SynthesisError};
use zeno_fcis_value::{Value, ValueError, ValueLimits};

/// Maximum advertised operations for one backend identity.
pub const MAX_BACKEND_CAPABILITIES: usize = 32;
/// Maximum additional accepted claims in one backend outcome.
pub const MAX_ADDITIONAL_CLAIMS: usize = 64;
/// Maximum logical fuel authorized for one request.
pub const MAX_LOGICAL_FUEL: u64 = 1_000_000_000_000;
/// Maximum candidates a backend may inspect for one request.
pub const MAX_BACKEND_CANDIDATES: u64 = 1_000_000_000;
/// Maximum canonical output bytes authorized for one response.
pub const MAX_BACKEND_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum trace entries authorized for one request.
pub const MAX_BACKEND_TRACE_ENTRIES: u64 = 10_000_000;

/// Closed operation registry for project-neutral engines.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum BackendOperation {
    /// Search a reviewed grammar for a satisfying artifact.
    Synthesize = 0,
    /// Verify a claim or proof artifact.
    Verify = 1,
    /// Compare an implementation or runtime with a model.
    Refine = 2,
    /// Establish compositional assumptions, guarantees, frames, and coupling.
    Compose = 3,
    /// Perform a semantics-preserving transformation.
    Transform = 4,
    /// Optimize within an explicit objective and equivalence relation.
    Optimize = 5,
    /// Minimize a retained counterexample without changing its witness property.
    MinimizeCounterexample = 6,
    /// Generate a bounded design proposal for independent checking.
    GenerateDesign = 7,
}

impl CanonicalEncode for BackendOperation {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Canonical unique operation set advertised by a backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendCapabilities(Box<[BackendOperation]>);

impl BackendCapabilities {
    /// Sorts, deduplicates, and bounds an operation set.
    pub fn try_new(mut operations: Vec<BackendOperation>) -> Result<Self, BackendError> {
        if operations.is_empty() {
            return Err(BackendError::EmptyCapabilities);
        }
        if operations.len() > MAX_BACKEND_CAPABILITIES {
            return Err(BackendError::TooManyCapabilities);
        }
        operations.sort_unstable();
        operations.dedup();
        Ok(Self(operations.into_boxed_slice()))
    }

    /// Returns operations in canonical order.
    #[must_use]
    pub const fn operations(&self) -> &[BackendOperation] {
        &self.0
    }

    /// Returns whether the backend advertises an operation.
    #[must_use]
    pub fn supports(&self, operation: BackendOperation) -> bool {
        self.0.binary_search(&operation).is_ok()
    }
}

impl CanonicalEncode for BackendCapabilities {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_u16_length(output, self.0.len())?;
        for operation in &self.0 {
            operation.encode_to(output)?;
        }
        Ok(())
    }
}

/// Content-bound backend implementation identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendIdentity {
    name: StableName,
    version: StableName,
    protocol_version: u16,
    binary_hash: Hash32,
    source_hash: Hash32,
    configuration_hash: Hash32,
    capabilities: BackendCapabilities,
}

impl BackendIdentity {
    /// Creates a backend identity with nonzero implementation commitments.
    pub fn try_new(
        name: StableName,
        version: StableName,
        protocol_version: u16,
        binary_hash: Hash32,
        source_hash: Hash32,
        configuration_hash: Hash32,
        capabilities: BackendCapabilities,
    ) -> Result<Self, BackendError> {
        if protocol_version == 0 {
            return Err(BackendError::ZeroProtocolVersion);
        }
        if [binary_hash, source_hash, configuration_hash].contains(&Hash32::ZERO) {
            return Err(BackendError::ZeroIdentityHash);
        }
        Ok(Self {
            name,
            version,
            protocol_version,
            binary_hash,
            source_hash,
            configuration_hash,
            capabilities,
        })
    }

    /// Returns the stable backend family name.
    #[must_use]
    pub const fn name(&self) -> &StableName {
        &self.name
    }

    /// Returns the stable backend version label.
    #[must_use]
    pub const fn version(&self) -> &StableName {
        &self.version
    }

    /// Returns the backend protocol version.
    #[must_use]
    pub const fn protocol_version(&self) -> u16 {
        self.protocol_version
    }

    /// Returns advertised operations.
    #[must_use]
    pub const fn capabilities(&self) -> &BackendCapabilities {
        &self.capabilities
    }

    /// Computes the complete backend identity commitment.
    pub fn commitment(&self) -> Result<Hash32, BackendError> {
        hash_canonical("zeno-fcis/backend-identity", self)
    }
}

impl CanonicalEncode for BackendIdentity {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.name.encode_to(output)?;
        self.version.encode_to(output)?;
        output.extend_from_slice(&self.protocol_version.to_be_bytes());
        output.extend_from_slice(self.binary_hash.as_bytes());
        output.extend_from_slice(self.source_hash.as_bytes());
        output.extend_from_slice(self.configuration_hash.as_bytes());
        self.capabilities.encode_to(output)
    }
}

/// Exact logical resource authorization for one backend request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendLimits {
    logical_fuel: u64,
    max_candidates: u64,
    max_output_bytes: u64,
    max_trace_entries: u64,
}

impl BackendLimits {
    /// Creates bounded, nonzero logical limits.
    pub const fn try_new(
        logical_fuel: u64,
        max_candidates: u64,
        max_output_bytes: u64,
        max_trace_entries: u64,
    ) -> Result<Self, BackendError> {
        if logical_fuel == 0
            || max_candidates == 0
            || max_output_bytes == 0
            || max_trace_entries == 0
        {
            return Err(BackendError::ZeroLimit);
        }
        if logical_fuel > MAX_LOGICAL_FUEL
            || max_candidates > MAX_BACKEND_CANDIDATES
            || max_output_bytes > MAX_BACKEND_OUTPUT_BYTES
            || max_trace_entries > MAX_BACKEND_TRACE_ENTRIES
        {
            return Err(BackendError::LimitTooLarge);
        }
        Ok(Self {
            logical_fuel,
            max_candidates,
            max_output_bytes,
            max_trace_entries,
        })
    }

    /// Returns authorized logical fuel.
    #[must_use]
    pub const fn logical_fuel(self) -> u64 {
        self.logical_fuel
    }

    /// Returns the candidate bound.
    #[must_use]
    pub const fn max_candidates(self) -> u64 {
        self.max_candidates
    }

    /// Returns the canonical output-byte bound.
    #[must_use]
    pub const fn max_output_bytes(self) -> u64 {
        self.max_output_bytes
    }

    /// Returns the trace-entry bound.
    #[must_use]
    pub const fn max_trace_entries(self) -> u64 {
        self.max_trace_entries
    }
}

impl CanonicalEncode for BackendLimits {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.logical_fuel.to_be_bytes());
        output.extend_from_slice(&self.max_candidates.to_be_bytes());
        output.extend_from_slice(&self.max_output_bytes.to_be_bytes());
        output.extend_from_slice(&self.max_trace_entries.to_be_bytes());
        Ok(())
    }
}

/// Exact resources reported by one backend run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendUsage {
    logical_fuel: u64,
    candidates: u64,
    output_bytes: u64,
    trace_entries: u64,
}

impl BackendUsage {
    /// Creates usage only when every dimension is within the request limit.
    pub const fn try_new(
        limits: BackendLimits,
        logical_fuel: u64,
        candidates: u64,
        output_bytes: u64,
        trace_entries: u64,
    ) -> Result<Self, BackendError> {
        if logical_fuel > limits.logical_fuel
            || candidates > limits.max_candidates
            || output_bytes > limits.max_output_bytes
            || trace_entries > limits.max_trace_entries
        {
            return Err(BackendError::UsageExceedsLimit);
        }
        Ok(Self {
            logical_fuel,
            candidates,
            output_bytes,
            trace_entries,
        })
    }

    /// Returns consumed logical fuel.
    #[must_use]
    pub const fn logical_fuel(self) -> u64 {
        self.logical_fuel
    }

    /// Returns evaluated candidates.
    #[must_use]
    pub const fn candidates(self) -> u64 {
        self.candidates
    }

    /// Returns emitted canonical bytes.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        self.output_bytes
    }

    /// Returns retained trace entries.
    #[must_use]
    pub const fn trace_entries(self) -> u64 {
        self.trace_entries
    }
}

impl CanonicalEncode for BackendUsage {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.logical_fuel.to_be_bytes());
        output.extend_from_slice(&self.candidates.to_be_bytes());
        output.extend_from_slice(&self.output_bytes.to_be_bytes());
        output.extend_from_slice(&self.trace_entries.to_be_bytes());
        Ok(())
    }
}

/// Complete immutable backend request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendRequest {
    request_id: Hash32,
    profile_hash: Hash32,
    operation: BackendOperation,
    specification_hash: Hash32,
    context_hash: Hash32,
    input: Value,
    limits: BackendLimits,
}

impl BackendRequest {
    /// Creates a content-bound request over one closed input value.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        request_id: Hash32,
        profile_hash: Hash32,
        operation: BackendOperation,
        specification_hash: Hash32,
        context_hash: Hash32,
        input: Value,
        limits: BackendLimits,
    ) -> Result<Self, BackendError> {
        if [request_id, profile_hash, specification_hash, context_hash].contains(&Hash32::ZERO) {
            return Err(BackendError::ZeroRequestBinding);
        }
        input
            .validate_limits(ValueLimits::default())
            .map_err(BackendError::Value)?;
        Ok(Self {
            request_id,
            profile_hash,
            operation,
            specification_hash,
            context_hash,
            input,
            limits,
        })
    }

    /// Returns the caller's stable request identity.
    #[must_use]
    pub const fn request_id(&self) -> Hash32 {
        self.request_id
    }

    /// Returns the project profile commitment.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the requested operation.
    #[must_use]
    pub const fn operation(&self) -> BackendOperation {
        self.operation
    }

    /// Returns the reviewed specification commitment.
    #[must_use]
    pub const fn specification_hash(&self) -> Hash32 {
        self.specification_hash
    }

    /// Returns the authenticated context commitment.
    #[must_use]
    pub const fn context_hash(&self) -> Hash32 {
        self.context_hash
    }

    /// Returns the closed input value.
    #[must_use]
    pub const fn input(&self) -> &Value {
        &self.input
    }

    /// Returns logical limits.
    #[must_use]
    pub const fn limits(&self) -> BackendLimits {
        self.limits
    }

    /// Computes the request commitment.
    pub fn commitment(&self) -> Result<Hash32, BackendError> {
        hash_canonical("zeno-fcis/backend-request", self)
    }
}

impl CanonicalEncode for BackendRequest {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.request_id.as_bytes());
        output.extend_from_slice(self.profile_hash.as_bytes());
        self.operation.encode_to(output)?;
        output.extend_from_slice(self.specification_hash.as_bytes());
        output.extend_from_slice(self.context_hash.as_bytes());
        put_blob(output, &self.input.canonical_bytes()?)?;
        self.limits.encode_to(output)
    }
}

/// Independently checked successful backend artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedOutcome {
    artifact: Value,
    reference_claim: Hash32,
    composition_claim: Hash32,
    additional_claims: Box<[Hash32]>,
    trace_hash: Hash32,
}

impl AcceptedOutcome {
    /// Creates an accepted outcome with two mandatory independent claim identities.
    pub fn try_new(
        artifact: Value,
        reference_claim: Hash32,
        composition_claim: Hash32,
        mut additional_claims: Vec<Hash32>,
        trace_hash: Hash32,
    ) -> Result<Self, BackendError> {
        artifact
            .validate_limits(ValueLimits::default())
            .map_err(BackendError::Value)?;
        if reference_claim == Hash32::ZERO
            || composition_claim == Hash32::ZERO
            || trace_hash == Hash32::ZERO
        {
            return Err(BackendError::ZeroOutcomeBinding);
        }
        if additional_claims.len() > MAX_ADDITIONAL_CLAIMS
            || additional_claims.contains(&Hash32::ZERO)
        {
            return Err(BackendError::InvalidAdditionalClaims);
        }
        additional_claims.sort_unstable();
        additional_claims.dedup();
        Ok(Self {
            artifact,
            reference_claim,
            composition_claim,
            additional_claims: additional_claims.into_boxed_slice(),
            trace_hash,
        })
    }

    /// Returns the closed compiled or transformed artifact.
    #[must_use]
    pub const fn artifact(&self) -> &Value {
        &self.artifact
    }

    /// Returns the independent reference/refinement claim identity.
    #[must_use]
    pub const fn reference_claim(&self) -> Hash32 {
        self.reference_claim
    }

    /// Returns the composition/contract claim identity.
    #[must_use]
    pub const fn composition_claim(&self) -> Hash32 {
        self.composition_claim
    }

    /// Returns additional independent claims in canonical order.
    #[must_use]
    pub const fn additional_claims(&self) -> &[Hash32] {
        &self.additional_claims
    }
}

impl CanonicalEncode for AcceptedOutcome {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, &self.artifact.canonical_bytes()?)?;
        output.extend_from_slice(self.reference_claim.as_bytes());
        output.extend_from_slice(self.composition_claim.as_bytes());
        put_u16_length(output, self.additional_claims.len())?;
        for claim in &self.additional_claims {
            output.extend_from_slice(claim.as_bytes());
        }
        output.extend_from_slice(self.trace_hash.as_bytes());
        Ok(())
    }
}

/// Backend result refuted by one normalized counterexample.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectedOutcome {
    counterexample: Value,
    trace_hash: Hash32,
}

impl RejectedOutcome {
    /// Creates a rejected outcome.
    pub fn try_new(counterexample: Value, trace_hash: Hash32) -> Result<Self, BackendError> {
        counterexample
            .validate_limits(ValueLimits::default())
            .map_err(BackendError::Value)?;
        if trace_hash == Hash32::ZERO {
            return Err(BackendError::ZeroOutcomeBinding);
        }
        Ok(Self {
            counterexample,
            trace_hash,
        })
    }

    /// Returns the normalized counterexample.
    #[must_use]
    pub const fn counterexample(&self) -> &Value {
        &self.counterexample
    }
}

impl CanonicalEncode for RejectedOutcome {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, &self.counterexample.canonical_bytes()?)?;
        output.extend_from_slice(self.trace_hash.as_bytes());
        Ok(())
    }
}

/// Honest bounded result when authorized search did not cover the remaining frontier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncompleteOutcome {
    frontier: Value,
    trace_hash: Hash32,
}

impl IncompleteOutcome {
    /// Creates an incomplete outcome with a closed frontier value.
    pub fn try_new(frontier: Value, trace_hash: Hash32) -> Result<Self, BackendError> {
        frontier
            .validate_limits(ValueLimits::default())
            .map_err(BackendError::Value)?;
        if trace_hash == Hash32::ZERO {
            return Err(BackendError::ZeroOutcomeBinding);
        }
        Ok(Self {
            frontier,
            trace_hash,
        })
    }

    /// Returns the retained frontier.
    #[must_use]
    pub const fn frontier(&self) -> &Value {
        &self.frontier
    }
}

impl CanonicalEncode for IncompleteOutcome {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, &self.frontier.canonical_bytes()?)?;
        output.extend_from_slice(self.trace_hash.as_bytes());
        Ok(())
    }
}

/// Backend could not produce a determinate result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndeterminateOutcome {
    reason: StableName,
    trace_hash: Hash32,
}

impl IndeterminateOutcome {
    /// Creates an indeterminate outcome.
    pub fn try_new(reason: StableName, trace_hash: Hash32) -> Result<Self, BackendError> {
        if trace_hash == Hash32::ZERO {
            return Err(BackendError::ZeroOutcomeBinding);
        }
        Ok(Self { reason, trace_hash })
    }

    /// Returns the stable non-authoritative failure classification.
    #[must_use]
    pub const fn reason(&self) -> &StableName {
        &self.reason
    }
}

impl CanonicalEncode for IndeterminateOutcome {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.reason.encode_to(output)?;
        output.extend_from_slice(self.trace_hash.as_bytes());
        Ok(())
    }
}

/// Closed backend outcome algebra.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendOutcome {
    /// Verified candidate artifact proposed by the backend.
    Accepted(AcceptedOutcome),
    /// Candidate or claim was refuted.
    Rejected(RejectedOutcome),
    /// Search or proof was explicitly incomplete under its bound.
    Incomplete(IncompleteOutcome),
    /// Engine crashed, disagreed, timed out, or otherwise could not decide.
    Indeterminate(IndeterminateOutcome),
}

impl CanonicalEncode for BackendOutcome {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Accepted(value) => {
                output.push(0);
                value.encode_to(output)
            }
            Self::Rejected(value) => {
                output.push(1);
                value.encode_to(output)
            }
            Self::Incomplete(value) => {
                output.push(2);
                value.encode_to(output)
            }
            Self::Indeterminate(value) => {
                output.push(3);
                value.encode_to(output)
            }
        }
    }
}

/// Complete backend response bound to one request and implementation identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendResponse {
    request_hash: Hash32,
    identity_hash: Hash32,
    usage: BackendUsage,
    outcome: BackendOutcome,
}

impl BackendResponse {
    /// Creates a response only for an advertised operation and bounded output.
    pub fn try_new(
        request: &BackendRequest,
        identity: &BackendIdentity,
        usage: BackendUsage,
        outcome: BackendOutcome,
    ) -> Result<Self, BackendError> {
        if !identity.capabilities.supports(request.operation) {
            return Err(BackendError::UnsupportedOperation(request.operation));
        }
        let usage = BackendUsage::try_new(
            request.limits,
            usage.logical_fuel,
            usage.candidates,
            usage.output_bytes,
            usage.trace_entries,
        )?;
        let output_length = outcome
            .canonical_bytes()
            .map_err(BackendError::Encode)?
            .len();
        let output_length =
            u64::try_from(output_length).map_err(|_| BackendError::OutputTooLarge)?;
        if output_length > request.limits.max_output_bytes {
            return Err(BackendError::OutputTooLarge);
        }
        if output_length != usage.output_bytes {
            return Err(BackendError::OutputUsageMismatch {
                actual: output_length,
                reported: usage.output_bytes,
            });
        }
        Ok(Self {
            request_hash: request.commitment()?,
            identity_hash: identity.commitment()?,
            usage,
            outcome,
        })
    }

    /// Returns the exact request commitment.
    #[must_use]
    pub const fn request_hash(&self) -> Hash32 {
        self.request_hash
    }

    /// Returns the complete backend identity commitment.
    #[must_use]
    pub const fn identity_hash(&self) -> Hash32 {
        self.identity_hash
    }

    /// Returns exact reported usage.
    #[must_use]
    pub const fn usage(&self) -> BackendUsage {
        self.usage
    }

    /// Returns the closed outcome.
    #[must_use]
    pub const fn outcome(&self) -> &BackendOutcome {
        &self.outcome
    }

    /// Validates this response against an expected request and backend identity.
    pub fn validate_for(
        &self,
        request: &BackendRequest,
        identity: &BackendIdentity,
    ) -> Result<(), BackendError> {
        if self.request_hash != request.commitment()? {
            return Err(BackendError::RequestMismatch);
        }
        if self.identity_hash != identity.commitment()? {
            return Err(BackendError::IdentityMismatch);
        }
        if !identity.capabilities.supports(request.operation) {
            return Err(BackendError::UnsupportedOperation(request.operation));
        }
        BackendUsage::try_new(
            request.limits,
            self.usage.logical_fuel,
            self.usage.candidates,
            self.usage.output_bytes,
            self.usage.trace_entries,
        )?;
        let output_length = self
            .outcome
            .canonical_bytes()
            .map_err(BackendError::Encode)?
            .len();
        let output_length =
            u64::try_from(output_length).map_err(|_| BackendError::OutputTooLarge)?;
        if output_length > request.limits.max_output_bytes {
            return Err(BackendError::OutputTooLarge);
        }
        if output_length != self.usage.output_bytes {
            return Err(BackendError::OutputUsageMismatch {
                actual: output_length,
                reported: self.usage.output_bytes,
            });
        }
        Ok(())
    }

    /// Computes the complete response commitment.
    pub fn commitment(&self) -> Result<Hash32, BackendError> {
        hash_canonical("zeno-fcis/backend-response", self)
    }
}

impl CanonicalEncode for BackendResponse {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.request_hash.as_bytes());
        output.extend_from_slice(self.identity_hash.as_bytes());
        self.usage.encode_to(output)?;
        self.outcome.encode_to(output)
    }
}

/// Closed execution failures reported by a mounted backend shell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendExecutionError {
    /// Backend process or service is unavailable.
    Unavailable,
    /// Backend crashed or violated its process boundary.
    Crashed,
    /// External deadline expired; this is not a proof of semantic incompleteness.
    TimedOut,
    /// Backend exhausted a non-semantic host resource.
    HostResourceExhausted,
    /// Backend emitted malformed or contradictory protocol data.
    ProtocolViolation,
}

/// Mounted backend engine boundary.
pub trait BackendEngine {
    /// Returns the pinned implementation identity before execution.
    fn identity(&self) -> &BackendIdentity;

    /// Executes one immutable request and returns one complete response.
    fn execute(
        &mut self,
        request: &BackendRequest,
    ) -> Result<BackendResponse, BackendExecutionError>;
}

/// Independent verifier result for a complete backend response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationDecision {
    /// Response and its claims were independently attested.
    Attested {
        /// Nonzero independent verification claim identity.
        claim_hash: Hash32,
    },
    /// Response was refuted by an independently retained witness.
    Refuted {
        /// Nonzero counterexample or verifier-report identity.
        counterexample_hash: Hash32,
    },
    /// Verifier could not decide and grants no authority.
    Indeterminate,
}

/// Independent backend-response verification authority.
pub trait BackendVerifier {
    /// Returns the pinned verifier/toolchain identity.
    fn verifier_hash(&self) -> Hash32;

    /// Verifies one exact request/response pair.
    fn verify(
        &mut self,
        request: &BackendRequest,
        response: &BackendResponse,
    ) -> VerificationDecision;
}

/// Content-addressed attestation for one exact backend exchange.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendCertificate {
    request_hash: Hash32,
    response_hash: Hash32,
    identity_hash: Hash32,
    verifier_hash: Hash32,
    verification_claim: Hash32,
}

impl BackendCertificate {
    /// Returns the request commitment.
    #[must_use]
    pub const fn request_hash(&self) -> Hash32 {
        self.request_hash
    }

    /// Returns the response commitment.
    #[must_use]
    pub const fn response_hash(&self) -> Hash32 {
        self.response_hash
    }

    /// Returns the backend implementation commitment.
    #[must_use]
    pub const fn identity_hash(&self) -> Hash32 {
        self.identity_hash
    }

    /// Returns the independent verifier identity.
    #[must_use]
    pub const fn verifier_hash(&self) -> Hash32 {
        self.verifier_hash
    }

    /// Returns the independent verification claim.
    #[must_use]
    pub const fn verification_claim(&self) -> Hash32 {
        self.verification_claim
    }

    /// Computes the complete certificate identity.
    pub fn commitment(&self) -> Result<Hash32, BackendError> {
        hash_canonical("zeno-fcis/backend-certificate", self)
    }
}

impl CanonicalEncode for BackendCertificate {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        for hash in [
            self.request_hash,
            self.response_hash,
            self.identity_hash,
            self.verifier_hash,
            self.verification_claim,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        Ok(())
    }
}

/// Independently verified backend response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedBackendRun {
    response: BackendResponse,
    certificate: BackendCertificate,
}

impl VerifiedBackendRun {
    /// Returns the verified response.
    #[must_use]
    pub const fn response(&self) -> &BackendResponse {
        &self.response
    }

    /// Returns the independent certificate.
    #[must_use]
    pub const fn certificate(&self) -> &BackendCertificate {
        &self.certificate
    }
}

/// Validates and independently verifies one mounted backend response.
pub fn verify_backend_response<V: BackendVerifier>(
    request: &BackendRequest,
    identity: &BackendIdentity,
    response: BackendResponse,
    verifier: &mut V,
) -> Result<VerifiedBackendRun, BackendError> {
    response.validate_for(request, identity)?;
    let verifier_hash = verifier.verifier_hash();
    if verifier_hash == Hash32::ZERO {
        return Err(BackendError::ZeroVerifierIdentity);
    }
    let verification_claim = match verifier.verify(request, &response) {
        VerificationDecision::Attested { claim_hash } if claim_hash != Hash32::ZERO => claim_hash,
        VerificationDecision::Attested { .. } => return Err(BackendError::ZeroVerificationClaim),
        VerificationDecision::Refuted {
            counterexample_hash,
        } if counterexample_hash != Hash32::ZERO => {
            return Err(BackendError::VerifierRefuted(counterexample_hash));
        }
        VerificationDecision::Refuted { .. } => return Err(BackendError::ZeroVerificationClaim),
        VerificationDecision::Indeterminate => return Err(BackendError::VerifierIndeterminate),
    };
    let certificate = BackendCertificate {
        request_hash: request.commitment()?,
        response_hash: response.commitment()?,
        identity_hash: identity.commitment()?,
        verifier_hash,
        verification_claim,
    };
    Ok(VerifiedBackendRun {
        response,
        certificate,
    })
}

/// Executes and independently verifies one mounted backend call.
pub fn execute_verified<E: BackendEngine, V: BackendVerifier>(
    engine: &mut E,
    verifier: &mut V,
    request: &BackendRequest,
) -> Result<VerifiedBackendRun, BackendError> {
    let identity = engine.identity().clone();
    if !identity.capabilities.supports(request.operation) {
        return Err(BackendError::UnsupportedOperation(request.operation));
    }
    let response = engine.execute(request).map_err(BackendError::Execution)?;
    verify_backend_response(request, &identity, response, verifier)
}

/// Stable context used to translate synthesis assignments into generic requests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendRequestTemplate {
    profile_hash: Hash32,
    specification_hash: Hash32,
    context_hash: Hash32,
    limits: BackendLimits,
}

impl BackendRequestTemplate {
    /// Creates a synthesis request template.
    pub fn try_new(
        profile_hash: Hash32,
        specification_hash: Hash32,
        context_hash: Hash32,
        limits: BackendLimits,
    ) -> Result<Self, BackendError> {
        if profile_hash == Hash32::ZERO
            || specification_hash == Hash32::ZERO
            || context_hash == Hash32::ZERO
        {
            return Err(BackendError::ZeroRequestBinding);
        }
        Ok(Self {
            profile_hash,
            specification_hash,
            context_hash,
            limits,
        })
    }

    fn request_for(&self, assignment: &Assignment) -> Result<BackendRequest, BackendError> {
        let input = Value::vector(
            assignment
                .entries()
                .iter()
                .map(|(id, value)| {
                    Value::tuple(vec![Value::U128(u128::from(id.get())), value.clone()])
                })
                .collect(),
        );
        BackendRequest::try_new(
            assignment.commitment().map_err(BackendError::Synthesis)?,
            self.profile_hash,
            BackendOperation::Synthesize,
            self.specification_hash,
            self.context_hash,
            input,
            self.limits,
        )
    }
}

impl CanonicalEncode for BackendRequestTemplate {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.profile_hash.as_bytes());
        output.extend_from_slice(self.specification_hash.as_bytes());
        output.extend_from_slice(self.context_hash.as_bytes());
        self.limits.encode_to(output)
    }
}

/// Adapts any checked generic backend into the deterministic synthesis checker API.
pub struct SynthesisBackendChecker<E, V> {
    engine: E,
    verifier: V,
    template: BackendRequestTemplate,
    checker_hash: Hash32,
}

impl<E: BackendEngine, V: BackendVerifier> SynthesisBackendChecker<E, V> {
    /// Creates an adapter only when the engine advertises synthesis and all identities are bound.
    pub fn try_new(
        engine: E,
        verifier: V,
        template: BackendRequestTemplate,
    ) -> Result<Self, BackendError> {
        if !engine
            .identity()
            .capabilities
            .supports(BackendOperation::Synthesize)
        {
            return Err(BackendError::UnsupportedOperation(
                BackendOperation::Synthesize,
            ));
        }
        let verifier_hash = verifier.verifier_hash();
        if verifier_hash == Hash32::ZERO {
            return Err(BackendError::ZeroVerifierIdentity);
        }
        let mut bytes = Vec::new();
        put_blob(&mut bytes, &engine.identity().canonical_bytes()?)?;
        put_blob(&mut bytes, &template.canonical_bytes()?)?;
        bytes.extend_from_slice(verifier_hash.as_bytes());
        let checker_hash = hash_bytes("zeno-fcis/backend-synthesis-checker", &bytes)?;
        Ok(Self {
            engine,
            verifier,
            template,
            checker_hash,
        })
    }

    /// Consumes the adapter and returns the mounted engine and verifier.
    #[must_use]
    pub fn into_parts(self) -> (E, V) {
        (self.engine, self.verifier)
    }
}

impl<E: BackendEngine, V: BackendVerifier> CandidateChecker for SynthesisBackendChecker<E, V> {
    fn checker_hash(&self) -> Hash32 {
        self.checker_hash
    }

    fn check(&mut self, assignment: &Assignment) -> CheckResult {
        let Ok(request) = self.template.request_for(assignment) else {
            return CheckResult::Indeterminate;
        };
        let Ok(run) = execute_verified(&mut self.engine, &mut self.verifier, &request) else {
            return CheckResult::Indeterminate;
        };
        let Ok(certificate_hash) = run.certificate().commitment() else {
            return CheckResult::Indeterminate;
        };
        match run.response().outcome() {
            BackendOutcome::Accepted(accepted) => {
                let Ok(reference_claim) = bind_verified_claim(
                    "zeno-fcis/backend-synthesis-reference",
                    accepted.reference_claim(),
                    certificate_hash,
                ) else {
                    return CheckResult::Indeterminate;
                };
                let Ok(composition_claim) = bind_verified_claim(
                    "zeno-fcis/backend-synthesis-composition",
                    accepted.composition_claim(),
                    certificate_hash,
                ) else {
                    return CheckResult::Indeterminate;
                };
                CheckResult::Accepted {
                    compiled: accepted.artifact().clone(),
                    reference_claim,
                    composition_claim,
                }
            }
            BackendOutcome::Rejected(rejected) => CheckResult::Rejected {
                counterexample: rejected.counterexample().clone(),
            },
            BackendOutcome::Incomplete(_) | BackendOutcome::Indeterminate(_) => {
                CheckResult::Indeterminate
            }
        }
    }
}

fn bind_verified_claim(
    domain: &'static str,
    claim: Hash32,
    certificate_hash: Hash32,
) -> Result<Hash32, BackendError> {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(claim.as_bytes());
    bytes.extend_from_slice(certificate_hash.as_bytes());
    hash_bytes(domain, &bytes)
}

fn put_u16_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u16::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    let length = u32::try_from(bytes.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn hash_canonical<T: CanonicalEncode>(
    domain: &'static str,
    value: &T,
) -> Result<Hash32, BackendError> {
    let bytes = value.canonical_bytes().map_err(BackendError::Encode)?;
    hash_bytes(domain, &bytes)
}

fn hash_bytes(domain: &'static str, bytes: &[u8]) -> Result<Hash32, BackendError> {
    let domain = Domain::new(domain, 1).map_err(BackendError::Encode)?;
    commitment::<RustCryptoSha256>(domain, bytes).map_err(BackendError::Encode)
}

/// Generic backend construction, validation, or verification failure.
#[derive(Debug)]
pub enum BackendError {
    /// A backend identity advertised no operations.
    EmptyCapabilities,
    /// Capability count exceeds the protocol bound.
    TooManyCapabilities,
    /// Backend protocol version is zero.
    ZeroProtocolVersion,
    /// Backend binary, source, or configuration identity is zero.
    ZeroIdentityHash,
    /// A request profile, specification, context, or request identity is zero.
    ZeroRequestBinding,
    /// A logical limit is zero.
    ZeroLimit,
    /// A logical limit exceeds the protocol maximum.
    LimitTooLarge,
    /// Reported usage exceeds the authorized request limit.
    UsageExceedsLimit,
    /// Backend does not advertise the requested operation.
    UnsupportedOperation(BackendOperation),
    /// Accepted claims or trace identity are zero.
    ZeroOutcomeBinding,
    /// Additional claim list is oversized or contains a zero identity.
    InvalidAdditionalClaims,
    /// Canonical output exceeds the declared output budget.
    OutputTooLarge,
    /// Reported output-byte usage differs from the exactly encoded outcome.
    OutputUsageMismatch {
        /// Exact canonical outcome byte count.
        actual: u64,
        /// Backend-reported output byte count.
        reported: u64,
    },
    /// Response does not bind the expected request.
    RequestMismatch,
    /// Response does not bind the expected backend identity.
    IdentityMismatch,
    /// Independent verifier identity is zero.
    ZeroVerifierIdentity,
    /// Independent verification claim or counterexample identity is zero.
    ZeroVerificationClaim,
    /// Independent verifier refuted the response.
    VerifierRefuted(Hash32),
    /// Independent verifier could not decide.
    VerifierIndeterminate,
    /// Mounted backend execution failed.
    Execution(BackendExecutionError),
    /// Closed value admission failed.
    Value(ValueError),
    /// Synthesis assignment conversion failed.
    Synthesis(SynthesisError),
    /// Canonical encoding or commitment failed.
    Encode(EncodeError),
}

impl From<EncodeError> for BackendError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCapabilities => formatter.write_str("backend capabilities are empty"),
            Self::TooManyCapabilities => {
                formatter.write_str("backend capability count exceeds bound")
            }
            Self::ZeroProtocolVersion => formatter.write_str("backend protocol version is zero"),
            Self::ZeroIdentityHash => formatter.write_str("backend identity contains a zero hash"),
            Self::ZeroRequestBinding => {
                formatter.write_str("backend request contains a zero binding")
            }
            Self::ZeroLimit => formatter.write_str("backend request limit is zero"),
            Self::LimitTooLarge => {
                formatter.write_str("backend request limit exceeds protocol maximum")
            }
            Self::UsageExceedsLimit => formatter.write_str("backend usage exceeds request limit"),
            Self::UnsupportedOperation(operation) => {
                write!(formatter, "backend does not support {operation:?}")
            }
            Self::ZeroOutcomeBinding => {
                formatter.write_str("backend outcome contains a zero claim or trace")
            }
            Self::InvalidAdditionalClaims => {
                formatter.write_str("backend additional claims are invalid")
            }
            Self::OutputTooLarge => formatter.write_str("backend output exceeds declared bound"),
            Self::OutputUsageMismatch { actual, reported } => write!(
                formatter,
                "backend reported {reported} output bytes but encoded {actual}"
            ),
            Self::RequestMismatch => {
                formatter.write_str("backend response request binding mismatch")
            }
            Self::IdentityMismatch => formatter.write_str("backend response identity mismatch"),
            Self::ZeroVerifierIdentity => formatter.write_str("backend verifier identity is zero"),
            Self::ZeroVerificationClaim => {
                formatter.write_str("backend verification claim is zero")
            }
            Self::VerifierRefuted(_) => {
                formatter.write_str("backend response was independently refuted")
            }
            Self::VerifierIndeterminate => {
                formatter.write_str("backend verifier was indeterminate")
            }
            Self::Execution(error) => write!(formatter, "backend execution failed: {error:?}"),
            Self::Value(error) => write!(formatter, "backend value rejected: {error}"),
            Self::Synthesis(error) => write!(formatter, "synthesis conversion failed: {error}"),
            Self::Encode(error) => write!(formatter, "backend encoding failed: {error}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for BackendError {}

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_synthesis::{
        Hole, HoleId, SearchBudget, SearchResult, SynthesisBindings, SynthesisProblem, search,
    };

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn name(value: &str) -> StableName {
        StableName::try_new(value).unwrap_or_else(|error| panic!("name: {error}"))
    }

    fn limits() -> BackendLimits {
        BackendLimits::try_new(10_000, 100, 1_000_000, 1_000)
            .unwrap_or_else(|error| panic!("limits: {error}"))
    }

    fn identity(operations: Vec<BackendOperation>) -> BackendIdentity {
        BackendIdentity::try_new(
            name("reference-backend"),
            name("1.0.0"),
            1,
            hash(1),
            hash(2),
            hash(3),
            BackendCapabilities::try_new(operations)
                .unwrap_or_else(|error| panic!("capabilities: {error}")),
        )
        .unwrap_or_else(|error| panic!("identity: {error}"))
    }

    fn request(operation: BackendOperation) -> BackendRequest {
        BackendRequest::try_new(
            hash(10),
            hash(11),
            operation,
            hash(12),
            hash(13),
            Value::U128(7),
            limits(),
        )
        .unwrap_or_else(|error| panic!("request: {error}"))
    }

    #[test]
    fn capabilities_are_history_independent() {
        let left = BackendCapabilities::try_new(vec![
            BackendOperation::Verify,
            BackendOperation::Synthesize,
            BackendOperation::Verify,
        ])
        .unwrap_or_else(|error| panic!("left: {error}"));
        let right = BackendCapabilities::try_new(vec![
            BackendOperation::Synthesize,
            BackendOperation::Verify,
        ])
        .unwrap_or_else(|error| panic!("right: {error}"));
        assert_eq!(left, right);
        assert_eq!(left.canonical_bytes(), right.canonical_bytes());
    }

    #[test]
    fn limits_and_usage_fail_closed() {
        assert!(matches!(
            BackendLimits::try_new(0, 1, 1, 1),
            Err(BackendError::ZeroLimit)
        ));
        let limits = limits();
        assert!(matches!(
            BackendUsage::try_new(limits, 10_001, 0, 0, 0),
            Err(BackendError::UsageExceedsLimit)
        ));
    }

    #[test]
    fn response_revalidates_usage_against_request_limits() {
        let request = request(BackendOperation::Verify);
        let identity = identity(vec![BackendOperation::Verify]);
        let outcome = BackendOutcome::Rejected(
            RejectedOutcome::try_new(Value::Unit, hash(30))
                .unwrap_or_else(|error| panic!("outcome: {error}")),
        );
        let output_bytes = u64::try_from(
            outcome
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("outcome bytes: {error}"))
                .len(),
        )
        .unwrap_or_else(|error| panic!("output length: {error}"));
        let permissive = BackendLimits::try_new(20_000, 200, 1_000_000, 2_000)
            .unwrap_or_else(|error| panic!("permissive limits: {error}"));
        let usage = BackendUsage::try_new(permissive, 10_001, 1, output_bytes, 1)
            .unwrap_or_else(|error| panic!("usage: {error}"));
        assert!(matches!(
            BackendResponse::try_new(&request, &identity, usage, outcome),
            Err(BackendError::UsageExceedsLimit)
        ));
    }

    #[test]
    fn response_requires_exact_output_byte_accounting() {
        let request = request(BackendOperation::Verify);
        let identity = identity(vec![BackendOperation::Verify]);
        let outcome = BackendOutcome::Rejected(
            RejectedOutcome::try_new(Value::Unit, hash(31))
                .unwrap_or_else(|error| panic!("outcome: {error}")),
        );
        let actual = u64::try_from(
            outcome
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("outcome bytes: {error}"))
                .len(),
        )
        .unwrap_or_else(|error| panic!("output length: {error}"));
        let reported = actual + 1;
        let usage = BackendUsage::try_new(request.limits(), 1, 1, reported, 1)
            .unwrap_or_else(|error| panic!("usage: {error}"));
        assert!(matches!(
            BackendResponse::try_new(&request, &identity, usage, outcome),
            Err(BackendError::OutputUsageMismatch {
                actual: observed,
                reported: declared,
            }) if observed == actual && declared == reported
        ));
    }

    #[test]
    fn response_requires_advertised_operation() {
        let request = request(BackendOperation::Verify);
        let identity = identity(vec![BackendOperation::Synthesize]);
        let usage = BackendUsage::try_new(request.limits(), 1, 1, 1024, 1)
            .unwrap_or_else(|error| panic!("usage: {error}"));
        let outcome = BackendOutcome::Rejected(
            RejectedOutcome::try_new(Value::Unit, hash(30))
                .unwrap_or_else(|error| panic!("outcome: {error}")),
        );
        assert!(matches!(
            BackendResponse::try_new(&request, &identity, usage, outcome),
            Err(BackendError::UnsupportedOperation(BackendOperation::Verify))
        ));
    }

    struct MockVerifier {
        claim_hash: Hash32,
    }

    impl BackendVerifier for MockVerifier {
        fn verifier_hash(&self) -> Hash32 {
            hash(40)
        }

        fn verify(
            &mut self,
            _request: &BackendRequest,
            _response: &BackendResponse,
        ) -> VerificationDecision {
            VerificationDecision::Attested {
                claim_hash: self.claim_hash,
            }
        }
    }

    struct MockEngine {
        identity: BackendIdentity,
    }

    impl BackendEngine for MockEngine {
        fn identity(&self) -> &BackendIdentity {
            &self.identity
        }

        fn execute(
            &mut self,
            request: &BackendRequest,
        ) -> Result<BackendResponse, BackendExecutionError> {
            let selected = match request.input() {
                Value::Vector(entries) => match entries.first() {
                    Some(Value::Tuple(pair)) => pair.get(1).cloned().unwrap_or(Value::Unit),
                    _ => Value::Unit,
                },
                _ => Value::Unit,
            };
            let outcome = if selected == Value::U128(2) {
                BackendOutcome::Accepted(
                    AcceptedOutcome::try_new(
                        Value::U128(99),
                        hash(50),
                        hash(51),
                        Vec::new(),
                        hash(52),
                    )
                    .unwrap_or_else(|error| panic!("accepted: {error}")),
                )
            } else {
                BackendOutcome::Rejected(
                    RejectedOutcome::try_new(selected, hash(53))
                        .unwrap_or_else(|error| panic!("rejected: {error}")),
                )
            };
            let output_bytes = u64::try_from(
                outcome
                    .canonical_bytes()
                    .unwrap_or_else(|error| panic!("outcome bytes: {error}"))
                    .len(),
            )
            .unwrap_or_else(|error| panic!("output length: {error}"));
            let usage = BackendUsage::try_new(request.limits(), 10, 1, output_bytes, 1)
                .unwrap_or_else(|error| panic!("usage: {error}"));
            BackendResponse::try_new(request, &self.identity, usage, outcome)
                .map_err(|_| BackendExecutionError::ProtocolViolation)
        }
    }

    #[test]
    fn synthesis_result_binds_independent_verifier_attestation() {
        let make_checker = |claim_hash| {
            let engine = MockEngine {
                identity: identity(vec![BackendOperation::Synthesize]),
            };
            let template = BackendRequestTemplate::try_new(hash(60), hash(61), hash(62), limits())
                .unwrap_or_else(|error| panic!("template: {error}"));
            SynthesisBackendChecker::try_new(engine, MockVerifier { claim_hash }, template)
                .unwrap_or_else(|error| panic!("checker: {error}"))
        };
        let hole = Hole::try_new(
            HoleId::try_new(1).unwrap_or_else(|error| panic!("hole id: {error}")),
            vec![Value::U128(2), Value::U128(1)],
        )
        .unwrap_or_else(|error| panic!("hole: {error}"));
        let problem = SynthesisProblem::try_new(
            SynthesisBindings {
                schema_hash: hash(70),
                contract_hash: hash(71),
                grammar_hash: hash(72),
                algorithm_hash: hash(73),
            },
            vec![hole],
            SearchBudget { max_assignments: 2 },
        )
        .unwrap_or_else(|error| panic!("problem: {error}"));
        let mut first_checker = make_checker(hash(41));
        let mut second_checker = make_checker(hash(42));
        let first = search(&problem, &mut first_checker)
            .unwrap_or_else(|error| panic!("first search failed: {error}"));
        let second = search(&problem, &mut second_checker)
            .unwrap_or_else(|error| panic!("second search failed: {error}"));
        assert_ne!(first, second);
    }

    #[test]
    fn generic_backend_drives_canonical_synthesis() {
        let engine = MockEngine {
            identity: identity(vec![BackendOperation::Synthesize]),
        };
        let template = BackendRequestTemplate::try_new(hash(60), hash(61), hash(62), limits())
            .unwrap_or_else(|error| panic!("template: {error}"));
        let mut checker = SynthesisBackendChecker::try_new(
            engine,
            MockVerifier {
                claim_hash: hash(41),
            },
            template,
        )
        .unwrap_or_else(|error| panic!("checker: {error}"));
        let hole = Hole::try_new(
            HoleId::try_new(1).unwrap_or_else(|error| panic!("hole id: {error}")),
            vec![Value::U128(2), Value::U128(1)],
        )
        .unwrap_or_else(|error| panic!("hole: {error}"));
        let problem = SynthesisProblem::try_new(
            SynthesisBindings {
                schema_hash: hash(70),
                contract_hash: hash(71),
                grammar_hash: hash(72),
                algorithm_hash: hash(73),
            },
            vec![hole],
            SearchBudget { max_assignments: 2 },
        )
        .unwrap_or_else(|error| panic!("problem: {error}"));
        let result =
            search(&problem, &mut checker).unwrap_or_else(|error| panic!("search failed: {error}"));
        match result {
            SearchResult::Selected {
                assignment,
                compiled,
                ..
            } => {
                assert_eq!(
                    assignment
                        .get(HoleId::try_new(1).unwrap_or_else(|error| panic!("id: {error}"))),
                    Some(&Value::U128(2))
                );
                assert_eq!(compiled, Value::U128(99));
            }
            SearchResult::NoSolution { .. } => panic!("expected selected candidate"),
        }
    }
}
