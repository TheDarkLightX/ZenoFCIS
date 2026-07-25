//! Umbrella exports for the initial ZenoFCIS semantic kernel.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

/// Decision algebra, budgets, reasons, and transition traits.
pub use zeno_fcis_core as core;
/// Canonical encoding and commitment-provider interfaces.
pub use zeno_fcis_codec as codec;
/// Transitively immutable closed values.
pub use zeno_fcis_value as value;

pub use zeno_fcis_core::{
    Accepted, Budget, BudgetExceeded, BudgetLimits, BudgetUsed, Decision, DecisionKind, Failed,
    Rejected, Resource, StableReason, Transition, first_reason,
};
pub use zeno_fcis_codec::{
    CanonicalEncode, CommitmentHasher, DecodeError, DecodeLimits, Domain, EncodeError, Envelope,
    Hash32, commitment, decode_envelope, decode_value, domain_preimage,
};
pub use zeno_fcis_value::{
    AsciiText, BoundedVec, Field, LengthError, MapEntry, NonEmptyVec, OwnedBytes, TextError, Value,
    ValueError, ValueKind, ValueLimits, ValueMetrics,
};
