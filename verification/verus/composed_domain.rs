//! Shared composed-domain resolver, state application and conditional theorem.
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
#[path = "../../crates/zeno-fcis-contract/src/checked_composition/resolve.rs"]
pub(crate) mod resolve;
#[cfg(verus_keep_ghost)]
#[path = "composed_domain_theorem.rs"]
pub(crate) mod theorem;
