//! Spot native-to-contract refinement: the compiled Spot decision program,
//! run by the production evaluator specification, meets the Step contracts
//! that the Lean round-trip theorems take as premises, on every admitted input.
//!
//! The closure is the evaluator specification (`evaluation/spec.rs`) and the
//! scalar and wide arithmetic it calls (`value/scalar.rs`, `value/wide.rs`),
//! all by path. The evaluator's Domain, Op and Failure declarations are a
//! generated verbatim copy (`spot_evaluator_types.rs`, freshness-checked):
//! their production module also owns executable bodies outside this closure.
#![no_std]
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;

#[path = "spot_value.rs"]
mod value;
#[path = "spot_evaluation.rs"]
pub mod evaluation;

#[cfg(verus_keep_ghost)]
#[path = "spot_refinement_base.rs"]
pub mod spot_refinement_base;
#[cfg(verus_keep_ghost)]
#[path = "spot_native_program.rs"]
pub mod spot_native_program;
#[cfg(verus_keep_ghost)]
#[path = "spot_refinement_proof.rs"]
pub mod spot_refinement_proof;
