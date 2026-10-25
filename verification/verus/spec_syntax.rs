//! Verifies the same syntax-certificate checker used by the runtime crate.
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

#[path = "../../crates/zeno-fcis-spec/src/syntax_certificate.rs"]
mod syntax_certificate;

#[cfg(test)]
#[path = "spec_syntax_tests.rs"]
mod tests;
