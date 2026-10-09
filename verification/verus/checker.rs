//! Checks exactly the production V2 meter and shared eager evaluator bodies.
#![no_std]
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;
extern crate self as zeno_fcis_core;
extern crate self as zeno_fcis_synthesis;
#[path = "../../crates/zeno-fcis-core/src/resource.rs"]
pub mod resource;
pub use resource::Resource;
#[cfg(test)]
extern crate std;

#[path = "../../crates/zeno-fcis-synthesis/src/finite/canonical_v2/mod.rs"]
pub mod canonical_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs"]
pub mod evaluation;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs"]
pub mod execution_v2;

// Preserve the production module paths; no algorithm or specification adapter.
pub mod finite {
    pub use crate::evaluation::{Domain, MAX_NODES, Op};
    #[cfg(test)]
    pub use crate::execution_v2::order_fixture_transition as v2_order_fixture_transition;
    pub use crate::execution_v2::{
        Failure as V2ExecutionFailure, Resource as V2Resource, ScalarProgram as V2ScalarProgram,
        execute as execute_v2, zero_limits as v2_zero_limits,
    };
    pub use crate::{canonical_v2, evaluation, execution_v2};
}
#[cfg(verus_keep_ghost)]
pub use execution_v2::execution as checker_execution;
#[path = "../../crates/zeno-fcis-shell-sqlite/src/v2/equivalence/checker.rs"]
mod checker;
#[path = "../../crates/zeno-fcis-shell-sqlite/src/v2/equivalence/checker_api.rs"]
pub mod checker_api;
