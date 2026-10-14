//! Serialization sublemma for the actual CLI SMT numeral printer.
//! This alone is not the scalar encoder or query refinement theorem.
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

#[path = "../../crates/zeno-fcis-cli/src/symbolic/smt/numeral.rs"]
mod numeral;
