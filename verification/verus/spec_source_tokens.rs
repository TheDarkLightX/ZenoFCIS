#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;
#[path = "../../crates/zeno-fcis-spec/src/lexical_core.rs"]
mod lexical_core;
#[path = "../../crates/zeno-fcis-spec/src/source_span.rs"]
mod source_span;
#[path = "../../crates/zeno-fcis-spec/src/source_token.rs"]
mod source_token;

#[cfg(all(test, not(verus_keep_ghost)))]
#[path = "spec_source_tokens_tests.rs"]
mod native_tests;
