//! Direct-source state lemma kernel; the production executable bodies are included.
#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;
#[cfg(test)]
extern crate std;
#[path = "../../crates/zeno-fcis-cli/src/contract/review/lemma_kernel.rs"]
pub mod lemma_kernel;
#[cfg(test)]
#[path = "lemma_kernel_tests.rs"]
mod tests;
