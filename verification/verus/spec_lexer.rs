#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

#[path = "../../crates/zeno-fcis-spec/src/lexical_core.rs"]
mod lexical_core;

#[cfg(test)]
extern crate std;
#[cfg(test)]
#[path = "spec_lexer_tests.rs"]
mod tests;
