//! Inert compatibility data and standalone evidence utilities.
//!
//! Native authoring, meter construction and candidate sealing are retired.
//! The complete historical reference algorithms and original assertions live
//! only in the private nonpublished kernel-law oracle. Use [`crate::program`]
//! for checked declarations and genuine publication.

/// Compatibility imports for inert data and standalone evidence utilities.
///
/// This module intentionally excludes raw candidate sealing and reference-shell
/// commit functions. Production projects use complete checked declarations
/// and genuine publications through `crate::program`.
pub mod prelude {
    pub use super::{
        Accepted, AdmittedEnvelope, AdmittedValue, AsciiText, CanonicalEncode, Decision,
        DecisionKind, DecisionValidationLimits, Domain, ExhaustiveDomainManifest, Failed, Hash32,
        NonEmptyVec, OwnedBytes, ProjectProfile, PromotionEvaluationContext, RegistryEntry,
        RegistryKind, Rejected, Resource, SemanticId, StableName, StableReason, ValidatedCoverage,
        ValidatedNormalizedDecision, ValidatedPromotionEvidence, ValidatedPromotionReport,
        ValidatedRefinementCase, Value, compare_validated_exact, evaluate_validated_promotion,
        exhaustive_coverage_claim,
    };

    #[cfg(feature = "catalog")]
    pub use super::{
        CatalogLimits, CatalogManifest, ChannelDefinition, CommitEffectSemantics, EffectDefinition,
        OperationSemantics, ProjectCatalog, ReasonDefinition, ReasonDisposition, ValueFlow,
        ValueFlowKind,
    };

    #[cfg(feature = "laws")]
    pub use super::{LawDefinition, LawManifest};

    #[cfg(feature = "authenticated-state")]
    pub use super::{
        AuthenticatedDecodeLimits, AuthenticatedProfile, ReferenceSparseTree, SparseProofContext,
        StateProjector, decode_authenticated_plan, decode_sparse_proof,
    };

    #[cfg(feature = "rustcrypto-sha256")]
    pub use super::RustCryptoSha256;

    #[cfg(feature = "backend")]
    pub use super::{
        BackendEngine, BackendError, BackendIdentity, BackendLimits, BackendOperation,
        BackendRequest, BackendRequestTemplate, BackendResponse, BackendVerifier,
        VerificationDecision, VerifiedBackendRun, execute_verified,
    };

    #[cfg(feature = "authoring")]
    pub use zeno_fcis_spec::{
        ProjectLimits, ProjectSpec, SourceLimits, elaborate_project, parse_project,
    };

    #[cfg(feature = "synthesis")]
    pub use zeno_fcis_synthesis::finite::completion::{
        CompletionError, CompletionLimits, CompletionProblem, VerifiedCompletion, find_completion,
        verify_completion, verify_completion_bytes,
    };

    #[cfg(feature = "bootstrap")]
    pub use super::{BootstrapLimits, BootstrapSpec, generate_project};
}

#[cfg(feature = "mounted-runtime")]
/// Strict project-neutral mounted-runtime adapters and replay fixtures.
pub use zeno_fcis_adapter as adapter;
#[cfg(feature = "mounted-zenodex")]
/// Concrete ZenoDEX runtime profiles built on the generic mounted boundary.
pub use zeno_fcis_adapter_zenodex as adapter_zenodex;
#[cfg(feature = "authenticated-state")]
/// Versioned sparse authenticated-state planning and proof verification.
pub use zeno_fcis_authenticated as authenticated;
#[cfg(feature = "backend")]
/// Checked project-neutral backend protocol for private and external engines.
pub use zeno_fcis_backend as backend;
#[cfg(feature = "bootstrap")]
/// Deterministic project starter generation from reviewed catalogs.
pub use zeno_fcis_bootstrap as bootstrap;
#[cfg(feature = "authoring")]
/// Bounded `.zeno` parser, typed project AST, builders, logic, and derived views.
pub use zeno_fcis_spec as spec;

#[cfg(feature = "catalog")]
/// Schema-bound reason, effect, channel, and plan-validation catalogs.
pub use zeno_fcis_catalog as catalog;
/// Canonical encoding and commitment-provider interfaces.
pub use zeno_fcis_codec as codec;
#[cfg(feature = "codegen")]
/// Deterministic, inspectable source generation from closed schemas.
pub use zeno_fcis_codegen as codegen;
#[cfg(feature = "collections")]
/// Backend-independent persistent collection interfaces and implementations.
pub use zeno_fcis_collections as collections;
/// Proof-carrying assume-guarantee contracts and deterministic composition evidence.
pub use zeno_fcis_compose as compose;
/// Decision algebra, budgets, reasons, and transition traits.
pub use zeno_fcis_core as core;
#[cfg(any(
    feature = "rustcrypto-sha256",
    feature = "verified-sha256",
    feature = "sha256-parity"
))]
/// Vetted SHA-256 providers and independent provider-parity evidence.
pub use zeno_fcis_crypto as crypto;
#[cfg(feature = "evidence")]
/// Canonical evidence envelopes and independent checker adapters.
pub use zeno_fcis_evidence as evidence;
#[cfg(feature = "laws")]
/// Tool-neutral project invariants, conservation laws, and checked proof subjects.
pub use zeno_fcis_laws as laws;
/// Preconditioned canonical state patches.
pub use zeno_fcis_patch as patch;
/// Closed non-executable commit evidence and durable outbox plans.
pub use zeno_fcis_plan as plan;
#[cfg(feature = "zenodex-profile")]
/// Optional ZenoDEX profile values and the first zUSD schema registry.
pub use zeno_fcis_profile_zenodex as profile_zenodex;
/// Project-neutral profiles, stable registries, and migration compatibility.
pub use zeno_fcis_project as project;
/// Candidate sealing, receipts, and atomic bundles.
pub use zeno_fcis_receipt as receipt;
/// Exact runtime-to-model refinement and proof-assisted promotion.
pub use zeno_fcis_refine as refine;
#[cfg(feature = "schema")]
/// Closed, acyclic protocol schemas and schema-bound value admission.
pub use zeno_fcis_schema as schema;
#[cfg(feature = "secret")]
/// Zeroizing and constant-time secret-handling boundaries.
pub use zeno_fcis_secret as secret;
#[cfg(feature = "security")]
/// Information-flow, side-channel, and covert-channel assurance values.
pub use zeno_fcis_security as security;
/// Pure reference semantics for atomic commit, replay, and outbox acknowledgement.
pub use zeno_fcis_shell as shell;
#[cfg(feature = "sqlite-shell")]
/// Crash-atomic SQLite publication and idempotent outbox delivery.
pub use zeno_fcis_shell_sqlite as shell_sqlite;
#[cfg(feature = "synthesis")]
/// Deterministic verifier-gated bounded synthesis.
pub use zeno_fcis_synthesis as synthesis;
/// Transitively immutable closed values.
pub use zeno_fcis_value as value;

#[cfg(feature = "laws")]
pub use zeno_fcis_laws::{
    DecisionScope, GenesisApplicability, LAW_MANIFEST_FORMAT_VERSION, LawDefinition,
    LawEngineFailure, LawError, LawEvidenceRequirement, LawFamilyDisposition, LawFamilyPolicy,
    LawField, LawKind, LawLimits, LawManifest, LawStatus,
};

#[cfg(feature = "authenticated-state")]
pub use zeno_fcis_authenticated::{
    AUTHENTICATED_PLAN_ENCODING_VERSION, AuthDecodeError, AuthError, AuthenticatedDecodeLimits,
    AuthenticatedProfile, AuthenticatedStatePlanner, ContextVerifiedSparseProof,
    DecodedAuthenticatedPlan, LeafWrite, NodeBatch, PlannedAuthenticatedCommit, PlannedState,
    ProofLeaf, ReferenceSparseTree, SPARSE_PROOF_ENCODING_VERSION, SparseProof, SparseProofContext,
    StaleNodeCandidate, StateProjector, TreeReader, TreeWriter, decode_authenticated_plan,
    decode_sparse_proof,
};
#[cfg(feature = "backend")]
pub use zeno_fcis_backend::{
    AcceptedOutcome, BackendCapabilities, BackendCertificate, BackendEngine, BackendError,
    BackendExecutionError, BackendIdentity, BackendLimits, BackendOperation, BackendOutcome,
    BackendRequest, BackendRequestTemplate, BackendResponse, BackendUsage, BackendVerifier,
    IncompleteOutcome, IndeterminateOutcome, RejectedOutcome, SynthesisBackendChecker,
    VerificationDecision, VerifiedBackendRun, execute_verified, verify_backend_response,
};
#[cfg(feature = "bootstrap")]
pub use zeno_fcis_bootstrap::{
    BOOTSTRAP_FORMAT_VERSION, BOOTSTRAP_GENERATOR_ID, BootstrapBundle, BootstrapError,
    BootstrapFile, BootstrapLimits, BootstrapSpec, MAX_BOOTSTRAP_FILE_BYTES, MAX_BOOTSTRAP_FILES,
    MAX_BOOTSTRAP_TOTAL_BYTES, generate_project,
};
#[cfg(feature = "catalog")]
pub use zeno_fcis_catalog::{
    CatalogError, CatalogLimits, CatalogManifest, CatalogMetrics, ChannelDefinition,
    CommitEffectSemantics, EffectDefinition, EffectHashField, HashRequirement, MAX_VALUE_FLOWS,
    NonZeroHash, OperationSemantics, ProjectCatalog, ReasonDefinition, ReasonDisposition,
    ValueFlow, ValueFlowKind, ValueRole,
};
pub use zeno_fcis_codec::{
    AdmittedEnvelope, CanonicalEncode, CommitmentHasher, DecodeError, DecodeLimits, Domain,
    EncodeError, Envelope, Hash32, commitment, decode_envelope, decode_value, domain_preimage,
};
#[cfg(feature = "codegen")]
pub use zeno_fcis_codegen::{
    CodegenError, GENERATOR_ID, GeneratedBundle, GeneratedFile, GenerationSpec, generate,
};
pub use zeno_fcis_compose::{
    AccessPath, Assumption, AssumptionDischarge, ClaimEvidence, ComponentContract, ComponentId,
    CompositionBlocker, CompositionClaim, CompositionEvidence, CompositionReport, CompositionSpec,
    Conflict, ConflictKind, ContractError, EvidenceVerifier, Footprint, FrameRule, Guarantee,
    ParallelConflictLaw, ParallelParityEvidence, ParallelVerificationContext, PathAtom, PathSet,
    ProviderGuarantee, Wiring, conflicts, verify_assume_guarantee, verify_deterministic_parallel,
};
pub use zeno_fcis_core::{
    Accepted, Decision, DecisionKind, Failed, Rejected, Resource, StableReason, first_reason,
};
#[cfg(feature = "verified-sha256")]
pub use zeno_fcis_crypto::LibcruxSha256;
#[cfg(feature = "rustcrypto-sha256")]
pub use zeno_fcis_crypto::RustCryptoSha256;
#[cfg(any(feature = "rustcrypto-sha256", feature = "verified-sha256"))]
pub use zeno_fcis_crypto::{KnownAnswerReport, ProviderVerificationError, verify_known_answers};
#[cfg(feature = "sha256-parity")]
pub use zeno_fcis_crypto::{ProviderParityReport, verify_provider_parity};
pub use zeno_fcis_patch::{
    AppliedPatch, CanonicalPatch, PatchDecodeError, PatchDecodeLimits, PatchError, PatchOp,
    PathSegment, ValuePath, decode_canonical_patch, hash_precondition_value, hash_value, value_at,
};
pub use zeno_fcis_plan::{
    CommitPlan, Effect, OutboxEntry, OutboxPlan, PlanDecodeError, PlanDecodeLimits, PlanError,
    decode_commit_plan, decode_outbox_plan,
};
#[cfg(feature = "zenodex-profile")]
pub use zeno_fcis_profile_zenodex::{
    BPS_SCALE, E8, MAX_AMOUNT_E8, ProfileError, ZUSD_COMMAND_TYPE_V1, ZUSD_STATE_TYPE_V1,
    ZenoDexLane, ZenoDexProfileV1, ZusdCommandTagV1, ZusdRejectV1, ZusdStateError,
    ZusdStateFieldV1, ZusdStateV1, zusd_precedence_bytes_v1, zusd_precedence_hash_v1,
    zusd_promotion_policy_v1,
};
pub use zeno_fcis_project::{
    AdditiveExtensionEvidence, CompatibilityBlocker, CompatibilityReport, DomainPrefix,
    EvolutionError, EvolutionMode, ProfileBindings, ProfileEvolution, ProjectProfile,
    RegistryEntry, RegistryKind, SemanticId, StableName, compare_successor,
};
pub use zeno_fcis_receipt::{
    CandidateBindings, CandidateBody, CandidateId, CommitBundle, ReasonCode, Receipt,
    RejectReceipt, SealError,
};
pub use zeno_fcis_refine::{
    CoverageMode, DecisionArtifacts, DecisionValidationBinding, DecisionValidationLimits,
    EvidenceKind, ExhaustiveDomainManifest, Mismatch, NormalizedDecision, PromotionBlocker,
    PromotionEvaluationContext, PromotionEvidence, PromotionPolicy, PromotionReport, ProofVerifier,
    RefineError, RefinementCase, RefinementReport, ToolEvidence, ValidatedCoverage,
    ValidatedNormalizedDecision, ValidatedPromotionEvidence, ValidatedPromotionReport,
    ValidatedRefinementCase, compare_exact, compare_validated_exact, evaluate_promotion,
    evaluate_validated_promotion, exhaustive_coverage_claim,
};
#[cfg(feature = "schema")]
pub use zeno_fcis_schema::{
    EnumVariantDef, FieldDef, FieldId, Schema, SchemaAdmittedEnvelope, SchemaAdmittedTypeEnvelope,
    SchemaEnvelopeError, SchemaError, SchemaLimits, SchemaMetrics, SchemaName, SumVariantDef,
    TypeDef, TypeId, TypeKind, ValidationLimits, ValidationReport, ValueValidationError, VariantId,
};
#[cfg(feature = "secret")]
pub use zeno_fcis_secret::{
    Exposed, ExposureEvent, ExposurePermit, HardenedExecution, SecretBox, SecretBytes,
    SecretChoice, SecretError,
};
#[cfg(feature = "security")]
pub use zeno_fcis_security::{
    CapacityEvidence, ChannelClass, CompartmentId, Declassification, DeploymentContract,
    LeakageBlocker, LeakagePolicy, LeakageReport, LeakageRule, Mitigation, Observation,
    ObservationKind, ObservationTrace, ObserverClearance, RuleMode, SecurityDomainId,
    SecurityError, SecurityEvidence, SecurityEvidenceKind, SecurityLabel, SecurityPromotionBlocker,
    SecurityPromotionPolicy, SecurityPromotionReport, compare_traces, evaluate_security_promotion,
};
pub use zeno_fcis_shell::{
    CommitResult, CommitStatus, OutboxRecord, ReplayRecord, ShellError, ShellState, acknowledge,
    apply_reference_bundle,
};
pub use zeno_fcis_value::{
    AdmittedValue, AsciiText, BoundedVec, Field, LengthError, MapEntry, NonEmptyVec, OwnedBytes,
    TextError, Value, ValueError, ValueKind, ValueLimits, ValueMetrics,
};
