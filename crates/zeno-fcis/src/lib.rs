//! Umbrella exports for the ZenoFCIS semantic, candidate, composition, and refinement kernel.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

/// Canonical encoding and commitment-provider interfaces.
pub use zeno_fcis_codec as codec;
/// Assume-guarantee contracts and deterministic composition evidence.
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
#[cfg(feature = "codegen")]
/// Deterministic, inspectable source generation from closed schemas.
pub use zeno_fcis_codegen as codegen;
/// Preconditioned canonical state patches.
pub use zeno_fcis_patch as patch;
/// Closed authoritative and outbox plans.
pub use zeno_fcis_plan as plan;
/// ZenoDEX profile values and the first zUSD schema registry.
pub use zeno_fcis_profile_zenodex as profile_zenodex;
/// Candidate sealing, receipts, and atomic bundles.
pub use zeno_fcis_receipt as receipt;
/// Exact runtime-to-model refinement and proof-assisted promotion.
pub use zeno_fcis_refine as refine;
#[cfg(feature = "schema")]
/// Closed, acyclic protocol schemas and schema-bound value admission.
pub use zeno_fcis_schema as schema;
/// Pure reference semantics for atomic commit, replay, and outbox acknowledgement.
pub use zeno_fcis_shell as shell;
/// Transitively immutable closed values.
pub use zeno_fcis_value as value;

pub use zeno_fcis_codec::{
    CanonicalEncode, CommitmentHasher, DecodeError, DecodeLimits, Domain, EncodeError, Envelope,
    Hash32, commitment, decode_envelope, decode_value, domain_preimage,
};
pub use zeno_fcis_compose::{
    AccessPath, Assumption, AssumptionDischarge, ClaimEvidence, ComponentContract, ComponentId,
    CompositionBlocker, CompositionEvidence, CompositionReport, CompositionSpec, Conflict,
    ConflictKind, ContractError, EvidenceVerifier, Footprint, FrameRule, Guarantee, PathAtom,
    PathSet, Wiring, conflicts, verify_assume_guarantee, verify_deterministic_parallel,
};
pub use zeno_fcis_core::{
    Accepted, Budget, BudgetExceeded, BudgetLimits, BudgetUsed, Decision, DecisionKind, Failed,
    Rejected, Resource, StableReason, Transition, first_reason,
};
#[cfg(feature = "rustcrypto-sha256")]
pub use zeno_fcis_crypto::RustCryptoSha256;
#[cfg(feature = "verified-sha256")]
pub use zeno_fcis_crypto::LibcruxSha256;
#[cfg(any(feature = "rustcrypto-sha256", feature = "verified-sha256"))]
pub use zeno_fcis_crypto::{KnownAnswerReport, ProviderVerificationError, verify_known_answers};
#[cfg(feature = "sha256-parity")]
pub use zeno_fcis_crypto::{ProviderParityReport, verify_provider_parity};
#[cfg(feature = "codegen")]
pub use zeno_fcis_codegen::{
    CodegenError, GeneratedBundle, GeneratedFile, GenerationSpec, GENERATOR_ID, generate,
};
pub use zeno_fcis_patch::{
    AppliedPatch, CanonicalPatch, PatchError, PatchOp, PathSegment, ValuePath, hash_value,
};
pub use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan, PlanError};
pub use zeno_fcis_profile_zenodex::{
    BPS_SCALE, E8, MAX_AMOUNT_E8, ProfileError, ZUSD_COMMAND_TYPE_V1, ZUSD_STATE_TYPE_V1,
    ZenoDexLane, ZenoDexProfileV1, ZusdCommandTagV1, ZusdRejectV1, ZusdStateError,
    ZusdStateFieldV1, ZusdStateV1, zusd_precedence_bytes_v1, zusd_precedence_hash_v1,
    zusd_promotion_policy_v1,
};
pub use zeno_fcis_receipt::{
    CandidateBindings, CandidateBody, CandidateBuilder, CandidateId, CommitBundle, ReasonCode,
    Receipt, RejectReceipt, SealError,
};
pub use zeno_fcis_refine::{
    CoverageMode, DecisionArtifacts, Mismatch, NormalizedDecision, PromotionBlocker,
    PromotionEvidence, PromotionPolicy, PromotionReport, ProofVerifier, RefineError,
    RefinementCase, RefinementReport, ToolEvidence, ToolKind, compare_exact, evaluate_promotion,
};
#[cfg(feature = "schema")]
pub use zeno_fcis_schema::{
    EnumVariantDef, FieldDef, FieldId, Schema, SchemaError, SchemaLimits, SchemaMetrics, SchemaName,
    SumVariantDef, TypeDef, TypeId, TypeKind, ValidationLimits, ValidationReport,
    ValueValidationError, VariantId,
};
pub use zeno_fcis_shell::{
    CommitResult, CommitStatus, OutboxRecord, ReplayRecord, ShellError, ShellState, acknowledge,
    commit,
};
pub use zeno_fcis_value::{
    AsciiText, BoundedVec, Field, LengthError, MapEntry, NonEmptyVec, OwnedBytes, TextError, Value,
    ValueError, ValueKind, ValueLimits, ValueMetrics,
};
