//! Foundational functional-core types for ZenoFCIS.
//!
//! This crate contains no I/O, clocks, randomness, storage, networking, or
//! executable effect closures. It is suitable for `no_std + alloc` builds.
//!
//! Native meter construction, caller-authored transition traits, and usage-bound
//! decision factories are retired. Their complete historical algorithms and tests
//! live only in the nonpublished private kernel-law oracle. These remaining
//! decision data types and resource identities confer no V2 publication authority.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;
use core::cmp::Ordering;

/// The three semantic outcomes of a total FCIS transition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum DecisionKind {
    /// The requested command was accepted and produced an authoritative candidate.
    Accept,
    /// The command was rejected and produced no authoritative candidate.
    Reject,
    /// The requested operation failed, but an intentional authoritative transition occurred.
    CommittedFailure,
}

/// A successful transition carrying one sealed candidate value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Accepted<A> {
    candidate: A,
}

impl<A> Accepted<A> {
    /// Creates an accepted outcome from a sealed candidate.
    #[must_use]
    pub const fn new(candidate: A) -> Self {
        Self { candidate }
    }

    /// Returns a shared reference to the candidate.
    #[must_use]
    pub const fn candidate(&self) -> &A {
        &self.candidate
    }

    /// Consumes the wrapper and returns the candidate.
    #[must_use]
    pub fn into_candidate(self) -> A {
        self.candidate
    }
}

/// An unchanged-state semantic rejection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rejected<R> {
    reason: R,
}

impl<R> Rejected<R> {
    /// Creates an unchanged-state rejection.
    #[must_use]
    pub const fn new(reason: R) -> Self {
        Self { reason }
    }

    /// Returns the stable rejection reason.
    #[must_use]
    pub const fn reason(&self) -> &R {
        &self.reason
    }

    /// Consumes the wrapper and returns the reason.
    #[must_use]
    pub fn into_reason(self) -> R {
        self.reason
    }
}

/// A failed requested operation that intentionally committed a candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Failed<A, F> {
    candidate: A,
    reason: F,
}

impl<A, F> Failed<A, F> {
    /// Creates a committed-failure outcome.
    #[must_use]
    pub const fn new(candidate: A, reason: F) -> Self {
        Self { candidate, reason }
    }

    /// Returns the committed candidate.
    #[must_use]
    pub const fn candidate(&self) -> &A {
        &self.candidate
    }

    /// Returns the stable committed-failure reason.
    #[must_use]
    pub const fn reason(&self) -> &F {
        &self.reason
    }

    /// Consumes the value and returns its parts.
    #[must_use]
    pub fn into_parts(self) -> (A, F) {
        (self.candidate, self.reason)
    }
}

/// The total three-way FCIS decision algebra.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Decision<A, R, F> {
    /// Accepted command with one sealed candidate.
    Accept(Accepted<A>),
    /// Unchanged-state rejection with no candidate.
    Reject(Rejected<R>),
    /// Failed requested operation with one intentional committed candidate.
    CommittedFailure(Failed<A, F>),
}

impl<A, R, F> Decision<A, R, F> {
    /// Returns the semantic decision kind.
    #[must_use]
    pub const fn kind(&self) -> DecisionKind {
        match self {
            Self::Accept(_) => DecisionKind::Accept,
            Self::Reject(_) => DecisionKind::Reject,
            Self::CommittedFailure(_) => DecisionKind::CommittedFailure,
        }
    }

    /// Maps the candidate while preserving rejection and failure semantics.
    pub fn map_candidate<B>(self, map: impl FnOnce(A) -> B) -> Decision<B, R, F> {
        match self {
            Self::Accept(value) => Decision::Accept(Accepted::new(map(value.into_candidate()))),
            Self::Reject(value) => Decision::Reject(value),
            Self::CommittedFailure(value) => {
                let (candidate, reason) = value.into_parts();
                Decision::CommittedFailure(Failed::new(map(candidate), reason))
            }
        }
    }
}

/// A stable protocol-visible reason code with an explicit precedence ordinal.
///
/// Implementations must keep `code` and `precedence` stable for a protocol
/// version. Source-code branch order must not be used as implicit precedence.
pub trait StableReason: Clone + Eq {
    /// Returns the stable protocol code.
    fn code(&self) -> &'static str;

    /// Returns the total precedence ordinal; lower values win.
    fn precedence(&self) -> u16;
}

/// Chooses one stable reason from all applicable reasons.
///
/// The total order is `(precedence, code)`, so equal precedence ordinals remain
/// deterministic. Profiles should normally reject duplicate ordinals during
/// construction rather than relying on the code tie-break.
pub fn first_reason<R, I>(reasons: I) -> Option<R>
where
    R: StableReason,
    I: IntoIterator<Item = R>,
{
    reasons.into_iter().min_by(compare_reasons)
}

fn compare_reasons<R: StableReason>(left: &R, right: &R) -> Ordering {
    left.precedence()
        .cmp(&right.precedence())
        .then_with(|| left.code().as_bytes().cmp(right.code().as_bytes()))
}

/// Shared identities for the checked V2 logical-work profile.
pub mod resource;
pub use resource::Resource;

/// Collects applicable reasons without allowing iterator order to become policy.
#[must_use]
pub fn collect_and_choose<R, I>(reasons: I) -> (Vec<R>, Option<R>)
where
    R: StableReason,
    I: IntoIterator<Item = R>,
{
    let collected: Vec<R> = reasons.into_iter().collect();
    let selected = first_reason(collected.iter().cloned());
    (collected, selected)
}
