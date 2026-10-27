//! Frozen scalar evaluator from the 2.4.0 release; results grant no publication authority.
#[path = "historical_i64/evaluation/mod.rs"]
mod evaluation;
#[path = "historical_i64/value/mod.rs"]
mod value;
#[path = "historical_i64/execution_v2/mod.rs"]
pub(crate) mod execution_v2;
#[path = "historical_i64/ir.rs"]
/// Released scalar data and admission; this grants no authority.
pub mod ir;
pub use crate::finite::Error;
pub use ir::{Domain, Op, Program, PROFILE, EXTENDED_PROFILE, ARITHMETIC_PROFILE, ARITHMETIC_EXTENDED_PROFILE};
pub use execution_v2::{execute, Limits, Usage, Resource, Failure, Outcome, zero_limits, MeterFailure, ScalarProgram};
pub use execution_v2::numeric::{Failure as NumericFailure, NumericProgram, Outcome as NumericOutcome, execute as execute_numeric, MAX_NUMERIC_INPUTS, MAX_NUMERIC_NODES, MAX_NUMERIC_OUTPUTS};
pub use evaluation::typed::NumericOp;
pub use value::{Atom, Domain as NumericDomain, IntegerWidth};
pub use ir::{MAX_EXTENDED_INPUTS, MAX_EXTENDED_OUTPUTS, MAX_EXTENDED_NODES, MAX_FIELDS, MAX_NODES};
pub use execution_v2::{Limits as V2Limits, Outcome as V2ExecutionOutcome, execute as execute_v2};
/// The released instruction-attempt accounting revision.
pub const V2_EXECUTION_PROFILE: &str = "zeno-fcis/finite-instruction-meter/2";

#[cfg(test)]
/// Released test fixture, retained only for upstream unit controls.
pub use execution_v2::order_fixture_transition;

/// Released original-schema data and canonical framing.
#[path = "historical_i64/canonical_v2/mod.rs"]
pub mod canonical_v2;
pub use execution_v2::composition as v2_composition;
pub use execution_v2::catalog as v2_catalog;
pub use execution_v2::laws as v2_laws;
pub(crate) use execution_v2::authority as v2_authority;
pub use execution_v2::{InputField as V2InputField, InputLeaf as V2InputLeaf,
    InputVariant as V2InputVariant, Resource as V2Resource, ScalarProgram as V2ScalarProgram,
    RecordProjection as V2RecordProjection, RecordFailure as V2RecordFailure,
    project_record as project_record_v2, zero_limits as v2_zero_limits};
pub use execution_v2::compound::Value as V2CanonicalValue;
pub use value::{Atom as V2NumericAtom, Domain as V2NumericDomain, IntegerWidth as V2IntegerWidth};
pub use execution_v2::numeric::{NumericProgram as V2NumericProgram,
    Failure as V2NumericFailure, execute as execute_numeric_v2};
pub use evaluation::typed::NumericOp as V2NumericOp;
pub use execution_v2::Failure as V2ExecutionFailure;
pub use execution_v2::{Limits as V2LimitsAlias, Usage as V2Usage};
#[cfg(test)]
pub use execution_v2::order_fixture_transition as v2_order_fixture_transition;

/// Opaque released types have no public binding constructor. They cannot be
/// obtained from HistoricalContext or passed to current publication APIs.
pub use execution_v2::authority::{Authority as RetainedEvaluator, Evaluation as RetainedEvaluation,
    Publication as RetainedPublication, PublicationOutcome as RetainedPublicationOutcome,
    WireDelivery as RetainedDelivery, Refusal as RetainedRefusal};
pub use execution_v2::continuation as v2_continuation;
pub use execution_v2::AccessAttempt as V2ReadAttempt;

/// Released data-only serializers; none produces an invocation capability.
pub use execution_v2::authority::{policy_bytes as retained_policy_bytes,
    numeric_policy_bytes as retained_numeric_policy_bytes};
pub use execution_v2::authority::candidate::candidate_bytes as retained_candidate_bytes;
