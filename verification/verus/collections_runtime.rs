//! Production framing and write-back proof closure.
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[path = "../../crates/zeno-fcis-contract/src/contract_error.rs"]
mod contract_error;
pub use contract_error::ContractError;
#[path = "../../crates/zeno-fcis-contract/src/internals/review/domain/leaf.rs"]
pub mod leaf;
#[path = "../../crates/zeno-fcis-contract/src/internals/declarations/source.rs"]
pub mod source;
pub mod internals {
    pub mod declarations {
        pub use crate::source::Source;
    }
    pub mod review {
        pub mod domain {
            pub use crate::leaf::*;
        }
    }
}
#[path = "../../crates/zeno-fcis-contract/src/collections/absent.rs"]
mod absent;
use absent::canonical_absent_view;
#[path = "../../crates/zeno-fcis-contract/src/collections/runtime.rs"]
mod runtime;
#[path = "../../crates/zeno-fcis-contract/src/collections/runtime_codec.rs"]
mod runtime_codec;
