#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

extern crate alloc;
#[cfg(test)]
extern crate std;
extern crate self as zeno_fcis_core;
#[path = "../../crates/zeno-fcis-core/src/resource.rs"]
pub mod resource;
pub use resource::Resource;

// The shared evaluator runs on the typed values, whose canonical variant is the
// V2 compound value; its closure is the production source, as in every V2 harness.
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
    #[cfg(test)]
    pub use crate::execution_v2::order_fixture_transition as v2_order_fixture_transition;
    pub use crate::{canonical_v2, evaluation, execution_v2};
    // The native oracle (excluded under Verus) reaches the wide values by their production path.
    #[cfg(not(verus_keep_ghost))]
    pub(crate) use crate::value;
}

#[cfg(test)]
#[path = "finite_execution_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "admission_baseline.rs"]
mod admission_baseline;

#[cfg(test)]
#[path = "admission_differential_tests.rs"]
mod admission_differential_tests;
