//! Declarative programs bound to checked original schemas and library execution.
//!
//! Start with [`prelude`]. Declare the complete program through [`program`], bind
//! its original schema and policy with [`bind_catalog`], then obtain a [`Program`]
//! using [`bind_program`]. Only that program produces genuine [`Publication`]
//! capabilities from original state, command and context envelopes.
//!
//! Limits are declarations; usage comes from the private library meter. Application
//! traits, claimed usage and caller-built candidates cannot replace this route.
//! [`legacy`] retains inert compatibility data and standalone evidence tools.
//! The historical authoring algorithms live in a private verification oracle.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

pub mod legacy;
pub mod program;

pub use program::{
    CatalogLimits, CatalogRefusal, CheckedCatalog, Evaluation, FrameBinding, Framing, Invocation,
    Limits, MeterFailure, Program, ProgramDefinition, Publication, PublicationOutcome, Refusal,
    Resource, Usage, WireDelivery, bind_catalog, bind_program, policy_bytes, zero_limits,
};

/// Normal application imports for the checked declarative execution family.
pub mod prelude {
    pub use crate::{
        CatalogLimits, CatalogRefusal, CheckedCatalog, Evaluation, FrameBinding, Framing,
        Invocation, Limits, MeterFailure, Program, ProgramDefinition, Publication,
        PublicationOutcome, Refusal, Resource, Usage, WireDelivery, bind_catalog, bind_program,
        policy_bytes, program, zero_limits,
    };
}

/// Original-source authoring tools. Parsed declarations still require program admission.
#[cfg(feature = "authoring")]
pub use zeno_fcis_spec as spec;

/// The capability-consuming V2 persistence adapter; SQLite is an imperative-shell boundary.
#[cfg(feature = "sqlite-shell")]
pub use zeno_fcis_shell_sqlite::v2 as sqlite;
