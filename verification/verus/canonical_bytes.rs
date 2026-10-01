//! Checks the production V2 integer byte-reader source, not a transcription.
#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

#[path = "../../crates/zeno-fcis-synthesis/src/finite/canonical_v2/mod.rs"]
pub mod canonical_v2;

#[cfg(test)]
#[path = "canonical_bytes_tests.rs"]
mod tests;
