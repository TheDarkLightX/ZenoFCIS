//! Frozen scalar evaluator from the 2.4.0 release; results grant no publication authority.
#[path = "historical_i64/evaluation/mod.rs"]
mod evaluation;
#[path = "historical_i64/value/mod.rs"]
mod value;
#[path = "historical_i64/execution_v2/mod.rs"]
mod execution_v2;
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
