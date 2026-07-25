//! Umbrella exports for the ZenoFCIS semantic and atomic-candidate kernel.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

/// Canonical encoding and commitment-provider interfaces.
pub use zeno_fcis_codec as codec;
/// Decision algebra, budgets, reasons, and transition traits.
pub use zeno_fcis_core as core;
/// Preconditioned canonical state patches.
pub use zeno_fcis_patch as patch;
/// Closed authoritative and outbox plans.
pub use zeno_fcis_plan as plan;
/// Candidate sealing, receipts, and atomic bundles.
pub use zeno_fcis_receipt as receipt;
/// Pure reference semantics for atomic commit, replay, and outbox acknowledgement.
pub use zeno_fcis_shell as shell;
/// Transitively immutable closed values.
pub use zeno_fcis_value as value;

pub use zeno_fcis_codec::{
    CanonicalEncode, CommitmentHasher, DecodeError, DecodeLimits, Domain, EncodeError, Envelope,
    Hash32, commitment, decode_envelope, decode_value, domain_preimage,
};
pub use zeno_fcis_core::{
    Accepted, Budget, BudgetExceeded, BudgetLimits, BudgetUsed, Decision, DecisionKind, Failed,
    Rejected, Resource, StableReason, Transition, first_reason,
};
pub use zeno_fcis_patch::{
    AppliedPatch, CanonicalPatch, PatchError, PatchOp, PathSegment, ValuePath, hash_value,
};
pub use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan, PlanError};
pub use zeno_fcis_receipt::{
    CandidateBindings, CandidateBody, CandidateBuilder, CandidateId, CommitBundle, ReasonCode,
    Receipt, RejectReceipt, SealError,
};
pub use zeno_fcis_shell::{
    CommitResult, CommitStatus, OutboxRecord, ReplayRecord, ShellError, ShellState, acknowledge,
    commit,
};
pub use zeno_fcis_value::{
    AsciiText, BoundedVec, Field, LengthError, MapEntry, NonEmptyVec, OwnedBytes, TextError, Value,
    ValueError, ValueKind, ValueLimits, ValueMetrics,
};
