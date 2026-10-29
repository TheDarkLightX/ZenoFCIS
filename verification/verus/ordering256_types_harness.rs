//! Exact production atoms/order methods, comparator and existing numeric definitions.
#![no_std]
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene, cfg_eval))]
extern crate alloc;
#[path = "../../crates/zeno-fcis-value/src/profile2/number256.rs"]
mod number256;
#[path = "../../crates/zeno-fcis-value/src/profile2/ordering256.rs"]
mod ordering256;
#[path = "../../crates/zeno-fcis-value/src/profile2/reference/legacy_wide_model.rs"]
mod wide;
