//! Concrete mounted ZenoDEX profiles built on the project-neutral runtime boundary.
//!
//! New projects should depend on `zeno-fcis-adapter`. This crate retains the
//! ZenoDEX-specific zUSD native transport, pinned source bindings, and mapping
//! from validated native results into complete ZenoFCIS decisions.

#![forbid(unsafe_code)]

/// Concrete mounted single-vault zUSD profile and native transport.
pub mod zusd;

pub use zeno_fcis_adapter::*;
