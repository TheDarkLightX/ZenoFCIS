//! Actual schema-wire checker and all original-byte reader dependencies.
#![no_std]
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;
#[cfg(test)]
extern crate std;
extern crate self as zeno_fcis_core;
#[path = "../../crates/zeno-fcis-core/src/resource.rs"]
pub mod resource;
pub use resource::Resource;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs"]
pub mod evaluation;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs"]
pub mod execution_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/canonical_v2/mod.rs"]
pub mod canonical_v2;

// Preserve the production module paths; no algorithm or specification adapter.
pub mod finite {
    #[cfg(test)]
    pub use crate::execution_v2::order_fixture_transition as v2_order_fixture_transition; pub use crate::{canonical_v2, evaluation, execution_v2}; }
