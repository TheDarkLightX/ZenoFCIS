//! Durable SQLite consumer of actual library-owned V2 publication capabilities.
//!
//! SQLite transactions, the OS, the sealed SHA-256 provider and observed delivery
//! acknowledgements form the trusted I/O boundary described by [`v2`]. Historical
//! schema-v5 callback algorithms and regressions live only in the private oracle.

#![forbid(unsafe_code)]

/// Durable consumer of the mandatory library-owned V2 publication capability.
pub mod v2;

/// Binds shell-effects publication data to the V2 Authority.
pub mod effects;

/// How an explicitly selected simulation crash point interrupts a commit.
#[cfg(feature = "crash-injection")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CrashMode {
    /// Return an injected error and allow transaction rollback.
    #[default]
    Error,
    /// Abort the process without running destructors.
    Abort,
}

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
        /// Interrupt after the first outbox INSERT, for per-statement abort tests.
        #[cfg(feature = "crash-injection")]
        AfterFirstOutboxWrite,
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
