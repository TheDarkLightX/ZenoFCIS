//! Arithmetic-lowering subject extension. Verus qualification is deferred to the 2.5.1 freeze.
#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

extern crate alloc;
extern crate self as zeno_fcis_core;
#[cfg(test)]
extern crate std;
#[path = "../../crates/zeno-fcis-core/src/resource.rs"]
pub mod resource;
pub use resource::Resource;

// The shared evaluator runs on the typed values, whose canonical variant is the
// V2 compound value; its closure is the production source, as in every V2 harness.
#[path = "../../crates/zeno-fcis-synthesis/src/finite/canonical_v2/mod.rs"]
pub mod canonical_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs"]
pub mod evaluation;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs"]
pub mod execution_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/mod.rs"]
mod value;

// Preserve the production module paths; no algorithm or specification adapter.
pub mod finite {
    #[cfg(test)]
    pub use crate::execution_v2::order_fixture_transition as v2_order_fixture_transition;
    pub use crate::{canonical_v2, evaluation, execution_v2};
}

#[cfg(verus_keep_ghost)]
#[path = "../arithmetic/unsigned_lowering.rs"]
mod unsigned_lowering;
