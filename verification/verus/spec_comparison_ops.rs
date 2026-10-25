#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

extern crate alloc;

#[path = "../../crates/zeno-fcis-spec/src/comparison_ops.rs"]
mod comparison_ops;
#[path = "../../crates/zeno-fcis-spec/src/lexical_core.rs"]
mod lexical_core;

#[cfg(test)]
extern crate std;
#[cfg(test)]
#[path = "spec_comparison_ops_tests.rs"]
mod tests;
