//! Checked assembly: the composition checker's tables give the joint
//! invariant at every state of a validated aggregate's histories, and the
//! assembly validator's route specialization preserves the evaluator
//! specification's values.
#![no_std]
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;
extern crate self as zeno_fcis_core;
#[path = "../../crates/zeno-fcis-core/src/resource.rs"]
pub mod resource;
pub use resource::Resource;
#[cfg(test)]
extern crate std;

// The evaluator specification the route lemma is stated against, in the
// production source closure it compiles in, as in every V2 harness.
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/mod.rs"]
mod value;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs"]
pub mod evaluation;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/canonical_v2/mod.rs"]
pub mod canonical_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs"]
pub mod execution_v2;

// Preserve the production module paths; no algorithm or specification adapter.
pub mod finite {
    pub(crate) use crate::value;
    #[cfg(test)]
    pub use crate::execution_v2::order_fixture_transition as v2_order_fixture_transition;
    pub use crate::{canonical_v2, evaluation, execution_v2};
}

#[cfg(verus_keep_ghost)]
#[path = "checked_assembly_joint.rs"]
pub mod joint;
#[cfg(verus_keep_ghost)]
#[path = "checked_assembly_tables.rs"]
pub mod tables;
#[cfg(verus_keep_ghost)]
#[path = "checked_assembly_route.rs"]
pub mod route;
