//! The production evaluator specification, by path, over a generated verbatim
//! copy of the evaluator's Domain, Op and Failure declarations.
#[path = "spot_evaluator_types.rs"]
mod types;
pub use types::{Domain, Op};
pub(crate) use types::Failure;
#[cfg(verus_keep_ghost)]
#[path = "../../crates/zeno-fcis-synthesis/src/finite/evaluation/spec.rs"]
pub(super) mod spec;
