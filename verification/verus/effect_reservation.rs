//! Proof harness: model, specification validation, and executable model bridge.
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
#![forbid(unsafe_code)]

#[cfg(verus_keep_ghost)]
#[path = "effect_protocol.rs"]
pub mod model;

#[cfg(verus_keep_ghost)]
#[path = "effect_protocol_replay.rs"]
pub mod replay;

#[cfg(verus_keep_ghost)]
#[path = "effect_protocol_examples.rs"]
pub mod examples;

#[path = "effect_protocol_runtime.rs"]
pub mod runtime;
