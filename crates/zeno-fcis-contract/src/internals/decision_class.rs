//! Decision tags shared by the compiler and the finite decision space.
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Decision class of a case.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Class {
    Accept,
    Reject,
    CommittedFailure,
}

impl Class {
    pub const ALL: [Self; 3] = [Self::Accept, Self::Reject, Self::CommittedFailure];

    pub fn name(self) -> &'static str {
        match self {
            Self::Accept => "Accept",
            Self::Reject => "Reject",
            Self::CommittedFailure => "CommittedFailure",
        }
    }

    /// The value the `Class` law observation reads.
    pub fn code(self) -> i128 {
        match self {
            Self::Accept => 0,
            Self::Reject => 1,
            Self::CommittedFailure => 2,
        }
    }
}
