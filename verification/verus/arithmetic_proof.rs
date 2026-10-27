//! Source-closed arithmetic correspondence. Linux x86-64 and pinned toolchains.
#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

#[cfg(verus_keep_ghost)]
#[path = "arithmetic_spec.rs"]
mod arithmetic_spec;
#[cfg(verus_keep_ghost)]
mod arithmetic_vectors;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/scalar.rs"]
pub(crate) mod scalar;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/wide.rs"]
pub(crate) mod wide;
