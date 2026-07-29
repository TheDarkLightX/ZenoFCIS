//! Supported umbrella for the ZenoFCIS functional core and integration layers.
//!
//! Start with [`prelude`] and enable only the feature that owns the boundary
//! being integrated. The low-level re-exports remain available for advanced,
//! reference, and adapter implementations.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

/// Curated imports for application code.
///
/// This module intentionally excludes raw candidate sealing and reference-shell
/// commit functions. Production projects should construct catalogued
/// transitions, verify project laws, and publish only through nominal
/// authorization.
pub mod prelude {
    pub use crate::{
        Accepted, AdmittedEnvelope, AdmittedValue, AsciiText, Budget, BudgetExceeded, BudgetLimits,
        BudgetUsed, BudgetedDecision, CanonicalEncode, Decision, DecisionKind,
        DecisionValidationLimits, Domain, ExhaustiveDomainManifest, Failed, Hash32, NonEmptyVec,
        OwnedBytes, ProjectProfile, PromotionEvaluationContext, RegistryEntry, RegistryKind,
        Rejected, Resource, SemanticId, StableName, StableReason, Transition, ValidatedCoverage,
        ValidatedNormalizedDecision, ValidatedPromotionEvidence, ValidatedPromotionReport,
        ValidatedRefinementCase, Value, compare_validated_exact, evaluate_validated_promotion,
        exhaustive_coverage_claim,
    };

    #[cfg(feature = "catalog")]
    pub use crate::{
        CatalogLimits, CatalogManifest, ChannelDefinition, EffectDefinition, OperationSemantics,
        ProjectCatalog, ReasonDefinition, ReasonDisposition, ValueFlow, ValueFlowKind,
    };

    #[cfg(feature = "transition")]
    pub use crate::{
        CataloguedTransitionBuilder, ExpectedInvocationBindings, TransitionDecision,
        TransitionError, TransitionLimits,
    };

    #[cfg(feature = "laws")]
    pub use crate::{
        LawDefinition, LawManifest, ProjectLawEngine, VerifiedProjectLaws, verify_project_laws,
    };

    #[cfg(feature = "authority")]
    pub use crate::{
        AuthorizedShellState, BoundInterpreter, CatalogCommitAuthority, CatalogTransitionProgram,
        ExecutionBinding, InvocationWitness, StateDomainBinding,
    };

    #[cfg(feature = "authenticated-authority")]
    pub use crate::{
        AuthenticatedCommitAuthority, CatalogAuthorizedAuthenticatedCommit,
        ProductionAuthenticatedCommitPort, ProjectionRelationDecision, ProjectionRelationEngine,
        ProjectorQualificationClaim, ProjectorQualificationVerifier,
    };

    #[cfg(feature = "authenticated-state")]
    pub use crate::{
        AuthenticatedDecodeLimits, AuthenticatedProfile, ReferenceSparseTree, SparseProofContext,
        StateProjector, decode_authenticated_plan, decode_sparse_proof,
    };

    #[cfg(feature = "rustcrypto-sha256")]
    pub use crate::RustCryptoSha256;

    #[cfg(feature = "domain-machines")]
    pub use crate::{
        DomainMachine, EnvelopeBinding, ExecutableComposition, FixedInvocationMatrix,
        FixedOutputMatrix, FixedStateMatrix, MachineInterface, TypedPathBinding,
    };

    #[cfg(feature = "composed-program")]
    pub use crate::{
        ComposedDomainProgram, ExternalOutput, ProjectionPlan, derive_semantic_program_hash,
    };

    #[cfg(feature = "backend")]
    pub use crate::{
        BackendEngine, BackendError, BackendIdentity, BackendLimits, BackendOperation,
        BackendRequest, BackendRequestTemplate, BackendResponse, BackendVerifier,
        VerificationDecision, VerifiedBackendRun, execute_verified,
    };

    #[cfg(feature = "bootstrap")]
    pub use crate::{BootstrapLimits, BootstrapSpec, generate_project};
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
#[cfg(feature = "authenticated-authority")]
/// Candidate-bound projector qualification and authenticated-state publication.
pub use zeno_fcis_authenticated_authority as authenticated_authority;
#[cfg(feature = "authority")]
/// Nominal catalog, invocation, provider, interpreter, and deployment commit authority.
pub use zeno_fcis_authority as authority;
#[cfg(feature = "backend")]
/// Checked project-neutral backend protocol for private and external engines.
pub use zeno_fcis_backend as backend;
#[cfg(feature = "bootstrap")]
/// Deterministic project starter generation from reviewed catalogs.
pub use zeno_fcis_bootstrap as bootstrap;
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
#[cfg(feature = "composed-program")]
/// One authority-owned program for fixed composed domain machines.
pub use zeno_fcis_composed_program as composed_program;
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
/// Closed authoritative and outbox plans.
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
/// Crash-atomic SQLite interpretation and idempotent outbox delivery.
pub use zeno_fcis_shell_sqlite as shell_sqlite;
#[cfg(feature = "synthesis")]
/// Deterministic verifier-gated bounded synthesis.
pub use zeno_fcis_synthesis as synthesis;
#[cfg(feature = "transition")]
/// Catalog-aware pure transition construction and same-candidate sealing.
pub use zeno_fcis_transition as transition;
/// Transitively immutable closed values.
pub use zeno_fcis_value as value;

#[cfg(feature = "laws")]
pub use zeno_fcis_laws::{
    DecisionScope, GENESIS_LAW_EVALUATION_FORMAT_VERSION, GenesisApplicability,
    GenesisLawCheckInput, GenesisLawEvaluation, LAW_EVALUATION_FORMAT_VERSION,
    LAW_MANIFEST_FORMAT_VERSION, LAW_SET_FORMAT_VERSION, LawCheckInput, LawDecisionView,
    LawDefinition, LawEngineFailure, LawError, LawEvaluation, LawEvidenceInput,
    LawEvidenceRequirement, LawEvidenceVerifier, LawFamilyDisposition, LawFamilyPolicy, LawField,
    LawKind, LawLimits, LawManifest, LawObservation, LawProofDecision, LawProofSubject, LawStatus,
    ProjectLawEngine, VerifiedLawEvidence, VerifiedProjectLaws, verify_project_laws,
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
#[cfg(feature = "authenticated-authority")]
pub use zeno_fcis_authenticated_authority::{
    AUTHENTICATED_AUTHORITY_FORMAT_VERSION, AuthenticatedAuthorityError,
    AuthenticatedAuthorityField, AuthenticatedCommitAuthority, AuthenticatedPublication,
    CatalogAuthorizedAuthenticatedCommit, ProductionAuthenticatedCommitPort,
    ProjectionRelationDecision, ProjectionRelationEngine, ProjectionRelationEvaluation,
    ProjectionRelationSubject, ProjectorQualification, ProjectorQualificationClaim,
    ProjectorQualificationDecision, ProjectorQualificationLimits, ProjectorQualificationVerifier,
};
#[cfg(feature = "authority")]
pub use zeno_fcis_authority::{
    AUTHORIZATION_FORMAT_VERSION, AuthorizationBody, AuthorizationId, AuthorizationPolicy,
    AuthorizationRecord, AuthorizedCommitResult, AuthorizedShellError, AuthorizedShellState,
    BoundInterpreter, CatalogAuthorizationDecision, CatalogAuthorizedGenesis,
    CatalogAuthorizedReject, CatalogAuthorizedTransition, CatalogCommitAuthority,
    CatalogExecutionError, CatalogTransitionProgram, ExecutionBinding, GenesisAuthorizationBody,
    GenesisId, GenesisPolicyBinding, INVOCATION_INPUT_FORMAT_VERSION, InvocationWitness,
    ReviewedTransitionInput, StateDomainBinding,
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
    EffectDefinition, EffectHashField, HashRequirement, MAX_VALUE_FLOWS, NonZeroHash,
    OperationSemantics, ProjectCatalog, ReasonDefinition, ReasonDisposition, ValueFlow,
    ValueFlowKind, ValueRole,
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
#[cfg(feature = "composed-program")]
pub use zeno_fcis_composed_program::{
    COMPOSED_PROGRAM_FORMAT_VERSION, ComposedDomainProgram, ComposedProgramError, ExternalOutput,
    ProjectionPlan, derive_semantic_program_hash,
};
pub use zeno_fcis_core::{
    Accepted, Budget, BudgetExceeded, BudgetLimits, BudgetUsed, BudgetedDecision, Decision,
    DecisionKind, Failed, Rejected, Resource, StableReason, Transition, first_reason,
};
#[cfg(feature = "verified-sha256")]
pub use zeno_fcis_crypto::LibcruxSha256;
#[cfg(feature = "rustcrypto-sha256")]
pub use zeno_fcis_crypto::RustCryptoSha256;
#[cfg(any(feature = "rustcrypto-sha256", feature = "verified-sha256"))]
pub use zeno_fcis_crypto::{KnownAnswerReport, ProviderVerificationError, verify_known_answers};
#[cfg(feature = "sha256-parity")]
pub use zeno_fcis_crypto::{ProviderParityReport, verify_provider_parity};
#[cfg(feature = "domain-machines")]
pub use zeno_fcis_domain::{
    DOMAIN_MACHINE_FORMAT_VERSION, DomainError, DomainMachine, EnvelopeBinding,
    ExecutableComposition, FixedInvocationMatrix, FixedOutputMatrix, FixedStateMatrix,
    MAX_DOMAIN_MACHINES, MAX_PORTS_PER_MACHINE, MAX_STATE_SLOTS_PER_MACHINE, MAX_TOTAL_PORTS,
    MAX_TOTAL_STATE_SLOTS, MachineCandidate, MachineExecutionReport, MachineFailure,
    MachineInterface, MachineRejection, PortAddress, SystemCandidate, SystemExecution,
    TypedPathBinding,
};
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
    CandidateBindings, CandidateBody, CandidateBuilder, CandidateId, CommitBundle, ReasonCode,
    Receipt, RejectReceipt, SealError,
};
pub use zeno_fcis_refine::{
    CoverageMode, DecisionArtifacts, DecisionValidationBinding, DecisionValidationLimits,
    ExhaustiveDomainManifest, Mismatch, NormalizedDecision, PromotionBlocker,
    PromotionEvaluationContext, PromotionEvidence, PromotionPolicy, PromotionReport, ProofVerifier,
    RefineError, RefinementCase, RefinementReport, ToolEvidence, ToolKind, ValidatedCoverage,
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
#[cfg(feature = "transition")]
pub use zeno_fcis_transition::{
    ArtifactField, CataloguedTransitionBuilder, ExpectedInvocationBindings, LimitKind,
    MAX_TRANSITION_MAP_KEY_BYTES, MAX_TRANSITION_OBSERVED_PATHS, MAX_TRANSITION_PATCH_OPERATIONS,
    MAX_TRANSITION_REASONS, MAX_TRANSITION_STATE_DEPTH, MAX_TRANSITION_STATE_NODES,
    TRANSITION_FORMAT_VERSION, TransitionArtifacts, TransitionDecision, TransitionError,
    TransitionLimits, TransitionReject, TransitionResourceReport, canonical_access_path,
    validate_transition_decision,
};
pub use zeno_fcis_value::{
    AdmittedValue, AsciiText, BoundedVec, Field, LengthError, MapEntry, NonEmptyVec, OwnedBytes,
    TextError, Value, ValueError, ValueKind, ValueLimits, ValueMetrics,
};
