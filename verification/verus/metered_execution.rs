//! Checks exactly the production V2 meter and shared eager evaluator bodies.
#![no_std]
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;

#[path = "../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs"]
pub mod evaluation;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs"]
pub mod execution_v2;
