#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

#[path = "../arithmetic/spot.rs"]
pub mod spot;
#[path = "../arithmetic/spot_bounds.rs"]
pub mod spot_bounds;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/wide.rs"]
pub(crate) mod wide;
