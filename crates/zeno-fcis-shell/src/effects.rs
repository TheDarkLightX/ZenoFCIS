//! Shell effects: the `shell-effects/1` port registry and pure step programs.
//!
//! A shell program requests one effect at a time and is resumed with its
//! response. [`registry`] declares every port once, as contract data;
//! [`program`] holds the two pure programs, commit with recovery and outbox
//! delivery; [`driver`] holds the single outstanding request and admits only
//! a response bound to it. Nothing here performs I/O or grants authority:
//! publication stays with the V2 Authority, which an upper adapter consults,
//! and handlers that carry effects out live outside this module.
//!
//! The programs are first-order and one-shot: no async, closures or macros,
//! and a continuation is resumed at most once. The registry binds the program
//! source by SHA-256, so the purity checker, the conformance tests and any
//! later proof target the same body.

pub mod codec;
pub mod driver;
pub mod explore;
pub mod handler;
pub mod laws;
pub mod policy;
pub mod policy_product;
pub mod policy_record;
pub mod port;
pub mod program;
pub mod record;
pub mod registry;
pub mod simulation;

#[cfg(test)]
mod explore_tests;
#[cfg(test)]
mod policy_record_tests;
#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod tests;
