//! V2 logical work counters. Only library execution can construct a meter.

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

#[cfg_attr(verus_keep_ghost, verus_spec)]
pub(super) const COUNT: usize = 8;

impl Resource {
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == super::spec::resource_index(self), result < COUNT,
    ))]
    pub(super) const fn index(self) -> usize {
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

/// V2 policy limits; these are configuration, not a usage report.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub(super) counters: [u64; COUNT],
}

/// Starts a V2 policy with every limit zero.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.view() == Seq::new(8, |_: int| 0u64),
))]
pub const fn zero_limits() -> Limits {
    Limits {
        counters: [0; COUNT],
    }
}

impl Limits {
    /// Sets one policy limit, preserving every other resource.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result.view() == self.view().update(
            super::spec::resource_index(resource) as int, amount),
    ))]
    pub fn with_limit(self, resource: Resource, amount: u64) -> Self {
        let mut limits = self;
        limits.counters[resource.index()] = amount;
        limits
    }

    /// Returns one configured limit.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == self.view()[super::spec::resource_index(resource) as int],
    ))]
    pub fn limit(self, resource: Resource) -> u64 {
        self.counters[resource.index()]
    }
}

/// Opaque V2 usage computed by library execution; no public constructor.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    pub(super) counters: [u64; COUNT],
}

impl Usage {
    /// Returns the work consumed for one resource.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == self.view()[super::spec::resource_index(resource) as int],
    ))]
    pub fn used(self, resource: Resource) -> u64 {
        self.counters[resource.index()]
    }
}

/// An instruction was prevented by overflow or a policy limit.
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeterFailure {
    /// Resource whose charge refused.
    pub resource: Resource,
    /// Configured limit for that resource.
    pub limit: u64,
    /// Proposed total, or u64::MAX when it cannot be represented.
    pub attempted: u64,
    /// Whether the mathematical proposed total exceeded u64::MAX.
    pub overflow: bool,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) struct Meter {
    pub(super) limits: Limits,
    pub(super) used: Usage,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.limits == limits,
        result.used.counters@ == Seq::new(8, |_: int| 0u64),
))]
pub(super) fn new(limits: Limits) -> Meter {
    Meter {
        limits,
        used: Usage {
            counters: [0; COUNT],
        },
    }
}

impl Meter {
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures final(self).limits == old(self).limits,
            (result, final(self).used.counters@) == super::spec::charge(
                old(self).limits.counters@, old(self).used.counters@, resource, amount),
    ))]
    pub(super) fn charge(&mut self, resource: Resource, amount: u64) -> Result<(), MeterFailure> {
        let index = resource.index();
        let limit = self.limits.counters[index];
        let Some(next) = self.used.counters[index].checked_add(amount) else {
            return Err(MeterFailure {
                resource,
                limit,
                attempted: u64::MAX,
                overflow: true,
            });
        };
        if next > limit {
            return Err(MeterFailure {
                resource,
                limit,
                attempted: next,
                overflow: false,
            });
        }
        self.used.counters[index] = next;
        Ok(())
    }
}
