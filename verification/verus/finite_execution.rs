#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

extern crate alloc;

#[path = "../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs"]
mod evaluation;

#[cfg(test)]
#[path = "finite_execution_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "admission_baseline.rs"]
mod admission_baseline;

#[cfg(test)]
#[path = "admission_differential_tests.rs"]
mod admission_differential_tests;
