//! Contract diagnostic value.

#[cfg(not(verus_keep_ghost))]
use std::fmt;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Why no contract was generated: the file and entry, and the reason.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct ContractError {
    place: String,
    reason: String,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl ContractError {
    /// The error about `place`, a file and an entry within it.
    pub fn new<P: Into<String>, R: Into<String>>(place: P, reason: R) -> Self {
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

#[cfg(not(verus_keep_ghost))]
impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.place, self.reason)
    }
}

#[cfg(not(verus_keep_ghost))]
impl std::error::Error for ContractError {}
