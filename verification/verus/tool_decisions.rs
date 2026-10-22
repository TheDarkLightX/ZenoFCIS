//! Two targeted tool decisions, importing their production source bodies.
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
#[path = "../../crates/zeno-fcis-contract/src/internals/decision_class.rs"]
mod decision_class;
#[path = "../../crates/zeno-fcis-contract/src/internals/decision_space.rs"]
mod decision_space;
#[path = "../../crates/zeno-fcis-cli/src/optimize/status.rs"]
mod status;
fn main() {}
