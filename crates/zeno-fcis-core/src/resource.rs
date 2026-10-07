//! Shared resource identities for the checked V2 logical-work profile.
//!
//! This representation contains no meter or usage construction. Synthesis owns
//! the private counters and every checked charge against these identities.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Resource classes in the V2 logical-work profile.
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Resource {
    /// Protected data access attempts.
    Read,
    /// Staged state-write attempts.
    Write,
    /// Candidate construction attempts.
    Candidate,
    /// Staged effect attempts.
    Effect,
    /// Payload bytes, under the consuming operation's profile.
    Byte,
    /// Witness bytes, under the consuming operation's profile.
    WitnessByte,
    /// Structural depth, under the consuming operation's profile.
    Depth,
    /// Eager IR instruction attempts, including a trapping instruction.
    Step,
}

/// The number of counters in the checked V2 logical-work profile.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const COUNT: usize = 8;

impl Resource {
    /// Returns this resource's exact counter position, always below `COUNT`.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == resource_index(self), result < COUNT,
    ))]
    pub const fn index(self) -> usize {
        match self {
            Self::Read => 0,
            Self::Write => 1,
            Self::Candidate => 2,
            Self::Effect => 3,
            Self::Byte => 4,
            Self::WitnessByte => 5,
            Self::Depth => 6,
            Self::Step => 7,
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {
    /// Exact mathematical counter position of the shared resource identity.
    pub open spec fn resource_index(resource: Resource) -> usize {
        match resource {
            Resource::Read => 0, Resource::Write => 1, Resource::Candidate => 2,
            Resource::Effect => 3, Resource::Byte => 4, Resource::WitnessByte => 5,
            Resource::Depth => 6, Resource::Step => 7,
        }
    }
}
