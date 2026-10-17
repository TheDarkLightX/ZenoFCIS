//! Durable SQLite consumer of actual library-owned V2 publication capabilities.
//!
//! SQLite transactions, the OS, the sealed SHA-256 provider and observed delivery
//! acknowledgements form the trusted I/O boundary described by [`v2`]. Historical
//! schema-v5 callback algorithms and regressions live only in the private oracle.

#![forbid(unsafe_code)]

/// Durable consumer of the mandatory library-owned V2 publication capability.
pub mod v2;

mod crash {
    /// Fault-injection point in the concrete commit protocol.
    #[cfg_attr(not(feature = "crash-injection"), allow(unreachable_pub))]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CrashPoint {
        /// Fail before opening the transaction.
        BeforeTransaction,
        /// Fail after exact bundle validation and state derivation.
        AfterValidation,
        /// Fail after writing semantic state/root/version.
        AfterStateWrite,
        /// Fail after replay, bundle, and receipt publication.
        AfterReplayWrite,
        /// Fail after every outbox row is written.
        AfterOutboxWrite,
        /// Fail immediately before SQLite commit.
        BeforeCommit,
        /// Commit, then simulate process loss before delivery.
        AfterCommit,
    }
}

#[cfg(feature = "crash-injection")]
pub use crash::CrashPoint;
#[cfg(not(feature = "crash-injection"))]
pub(crate) use crash::CrashPoint;

/// The concrete library memory destination and its collision failure.
/// These reexports preserve their original nominal shell-crate identities.
pub use zeno_fcis_shell::{DeliveryCollision, MemoryDestination};

/// Source text for internal tool fingerprints; not a stable application API.
/// Include `LIB` as well as leaf entries so fingerprints bind this mapping.
#[doc(hidden)]
pub mod source_fingerprint {
    /// Exact source bytes owned by this crate.
    pub const LIB: &str = include_str!("lib.rs");
    /// Exact source bytes owned by this crate.
    pub const V2_EQUIVALENCE_CHECKER: &str = include_str!("v2/equivalence/checker.rs");
    /// Exact source bytes owned by this crate.
    pub const V2_EQUIVALENCE_CHECKER_API: &str = include_str!("v2/equivalence/checker_api.rs");
    /// Exact source bytes owned by this crate.
    pub const V2_EQUIVALENCE_CHECKER_EQUALITY: &str =
        include_str!("v2/equivalence/checker_equality.rs");
    /// Exact source bytes owned by this crate.
    pub const V2_EQUIVALENCE_CHECKER_RANGES: &str =
        include_str!("v2/equivalence/checker_ranges.rs");
    /// Exact source bytes owned by this crate.
    pub const V2_EQUIVALENCE_CHECKER_SPEC: &str = include_str!("v2/equivalence/checker_spec.rs");
}
