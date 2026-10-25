#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

extern crate alloc;

#[path = "../../crates/zeno-fcis-spec/src/lexical_core.rs"]
mod lexical_core;
#[path = "../../crates/zeno-fcis-spec/src/semantic_readers.rs"]
mod semantic_readers;

#[cfg(test)]
extern crate std;
#[cfg(test)]
#[path = "spec_semantic_readers_tests.rs"]
mod tests;
