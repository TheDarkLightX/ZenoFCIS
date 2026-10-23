//! Direct-source reachable probe and shared finite-machine proofs.
#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;
#[cfg(test)]
extern crate std;
#[cfg(verus_keep_ghost)]
pub mod finite_machine_proofs;
#[cfg(verus_keep_ghost)]
pub mod finite_machine_spec;
#[cfg(verus_keep_ghost)]
pub mod lemma_fixed_point;
#[path = "../../crates/zeno-fcis-cli/src/contract/review/lemma_kernel.rs"]
pub mod lemma_kernel;
#[path = "../../crates/zeno-fcis-cli/src/contract/review/probe_kernel.rs"]
pub mod probe_kernel;
#[cfg(test)]
#[path = "probe_kernel_tests.rs"]
mod tests;
