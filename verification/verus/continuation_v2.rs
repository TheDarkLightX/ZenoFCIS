//! Whole actual checked continuation and shared evaluation/authority source.
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
#[path = "../../crates/zeno-fcis-synthesis/src/finite/canonical_v2/mod.rs"]
pub mod canonical_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs"]
pub mod evaluation;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs"]
pub mod execution_v2;

// The exact same external public-API tests also compile under Cargo.
#[cfg(test)]
extern crate self as zeno_fcis_synthesis;
pub mod finite {
    pub use crate::evaluation::{Domain, Op};
    #[cfg(test)]
    pub use crate::execution_v2::order_fixture_transition as v2_order_fixture_transition;
    pub use crate::execution_v2::{
        InputField as V2InputField, InputLeaf as V2InputLeaf, Limits as V2Limits,
        Resource as V2Resource, ScalarProgram as V2ScalarProgram, authority as v2_authority,
        catalog as v2_catalog, composition as v2_composition, continuation as v2_continuation,
        laws as v2_laws, zero_limits as v2_zero_limits,
    };
    pub use crate::{canonical_v2, evaluation, execution_v2};
}
#[cfg(test)]
#[path = "../../crates/zeno-fcis-synthesis/tests/v2_authority.rs"]
mod public_authority;
