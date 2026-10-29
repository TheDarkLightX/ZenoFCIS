//! Source-bound numeric constants and methods with their integer refinements.
#![no_std]
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene, cfg_eval))]
extern crate alloc;
#[path = "../../crates/zeno-fcis-value/src/profile2/arithmetic256.rs"]
mod arithmetic256;
