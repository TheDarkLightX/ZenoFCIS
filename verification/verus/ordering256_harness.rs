//! Actual comparison module used by arithmetic256; no copied implementation.
#![no_std]
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;
#[path = "../../crates/zeno-fcis-value/src/profile2/ordering256.rs"]
mod arithmetic256;
