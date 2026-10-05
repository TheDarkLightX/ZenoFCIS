//! Nonpublished independent reference algorithms and kernel-law tests.
#![forbid(unsafe_code)]

extern crate alloc;

// The private oracle retains complete legacy APIs for original assertions.
// No item is exported, and production crates never depend on this package.
#[cfg(test)]
#[allow(dead_code)]
mod oracle;

// Test-only compatibility resolves every unchanged historical caller to the
// private oracle while preserving the nominal normal authoring AST.
#[cfg(test)]
extern crate zeno_fcis_spec as spec_authoring;
#[cfg(test)]
extern crate self as zeno_fcis_spec;
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use spec_authoring::*;
#[cfg(test)]
#[path = "oracle/spec/ast.rs"]
mod ast;
#[cfg(test)]
mod diagnostic { pub(crate) use crate::spec_authoring::*; }
#[cfg(test)]
mod substance { pub(crate) use crate::spec_authoring::*; }
#[cfg(test)]
pub(crate) use oracle::spec::logic;
#[cfg(test)]
pub(crate) use logic::*;
#[cfg(test)]
pub(crate) use oracle::spec::evaluate_invariant;

// Test-only lexical aliases keep the exact original generated function bodies.
#[cfg(test)]
pub(crate) use oracle::generated_fixture::{bootstrap_project, generated, test_catalog};
