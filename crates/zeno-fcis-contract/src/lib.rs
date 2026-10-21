//! The ZenoFCIS contract compiler core and the checked assume-guarantee
//! composition checker.
//!
//! An application contract is its `project.zeno` declarations and its
//! `v2/policy.json` rules. This crate compiles them into the library's
//! program definition, encodes the policy bytes the library binds, and binds
//! the program through the library's ordinary route, `bind_catalog` then
//! `bind_program`. The `zeno-fcis` command line tool is a shell over this
//! core: it reads and writes files, renders application source, replays
//! program adoptions and runs the reviews.
//!
//! The supported API is [`checked_composition`], [`refinement`] and [`ContractError`].
//! [`checked_composition::check`] takes the source bytes of a composition of
//! separately bound contracts, its manifest and each leaf's two files, and
//! compiles and checks them itself. Its result, a
//! [`checked_composition::CheckedComposition`], has private fields and only
//! the checker builds it. It records the checker's source-bound identity and
//! the SHA-256 of every table the check ran, and its canonical receipt
//! replays with [`checked_composition::replay`].
//! [`checked_composition::require_checked`] gives the checked class of the
//! claims of a `zeno-fcis-compose` specification, which no compose evidence
//! verifier can produce, for a composition whose claim is unconditional; a
//! composition whose root group leaves conditions is refused, and
//! [`checked_composition::CheckedComposition::claim_kind`] and
//! [`checked_composition::CheckedComposition::conditions`] state them. At run time this crate reads no file, environment
//! variable, clock or network and starts no process or thread.
//!
//! The `internals` module, hidden from this documentation, holds the
//! compiler itself. It is public only so that the command line tool can
//! share it, and carries no stability promise.

#![forbid(unsafe_code)]

pub mod checked_composition;
pub mod collections;
pub mod distribution;
#[doc(hidden)]
pub mod internals;
pub mod refinement;

#[cfg(test)]
mod priority_construction_controls;

use std::fmt;

/// Why no contract was generated: the file and entry, and the reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractError {
    place: String,
    reason: String,
}

impl ContractError {
    /// The error about `place`, a file and an entry within it.
    pub fn new(place: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            place: place.into(),
            reason: reason.into(),
        }
    }

    /// The file, and entry within it, that the reason is about.
    pub fn place(&self) -> &str {
        &self.place
    }

    /// What is wrong there.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.place, self.reason)
    }
}

impl std::error::Error for ContractError {}
