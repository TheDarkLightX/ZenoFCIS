//! Exact production SMT construction, deep copy, script updates and syntax printers.
//! This is not the scalar program-to-query semantic refinement.
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

#[path = "../../crates/zeno-fcis-cli/src/symbolic/smt/term.rs"]
mod term;

#[path = "../../crates/zeno-fcis-cli/src/symbolic/smt/script.rs"]
mod script;
