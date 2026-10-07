//! Durable SQLite consumer of actual library-owned V2 publication capabilities.
//!
//! SQLite transactions, the OS, the sealed SHA-256 provider and observed delivery
//! acknowledgements form the trusted I/O boundary described by [`v2`]. Historical
//! schema-v5 callback algorithms and regressions live only in the private oracle.

#![forbid(unsafe_code)]

/// Durable consumer of the mandatory library-owned V2 publication capability.
pub mod v2;

/// Fault-injection point in the concrete commit protocol.
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

/// The concrete library memory destination and its collision failure.
/// These reexports preserve their original nominal shell-crate identities.
pub use zeno_fcis_shell::{DeliveryCollision, MemoryDestination};
