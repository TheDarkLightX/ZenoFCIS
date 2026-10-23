//! Includes the production value and decoder files directly.
#![no_std]
#![deny(unsafe_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene, allocator_api))]
extern crate alloc;
extern crate self as zeno_fcis_value;

#[path = "../../crates/zeno-fcis-value/src/lib.rs"]
pub mod native_value;
pub use native_value::*;
#[path = "../../crates/zeno-fcis-codec/src/lib.rs"]
pub mod codec;
pub use codec::decoder_spec;

#[path = "value_decoder_std.rs"]
// assume_specification expands to an unsafe wrapper for the external std call.
#[allow(unsafe_code)]
pub mod std_contracts;

#[path = "value_decoder_vectors.rs"]
pub mod vectors;
