//! Production evaluator closure, preserving all production module paths.
#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;
extern crate self as zeno_fcis_core;
#[path = "../../crates/zeno-fcis-core/src/resource.rs"]
pub mod resource;
pub use resource::Resource;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/canonical_v2/mod.rs"]
pub mod canonical_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs"]
pub mod evaluation;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs"]
pub mod execution_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/mod.rs"]
pub(crate) mod value;
pub mod finite {
    pub(crate) use crate::value;
    pub use crate::{canonical_v2, evaluation, execution_v2};
}
#[cfg(verus_keep_ghost)]
use value::{scalar, wide};
#[cfg(verus_keep_ghost)]
#[path = "arithmetic_spec.rs"]
mod arithmetic_spec;
#[cfg(verus_keep_ghost)]
mod arithmetic_typed_spec;
#[cfg(verus_keep_ghost)]
mod arithmetic_vectors;
