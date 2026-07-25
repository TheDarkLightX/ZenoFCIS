//! Foundational functional-core types for ZenoFCIS.
//!
//! This crate contains no I/O, clocks, randomness, storage, networking, or
//! executable effect closures. It is suitable for `no_std + alloc` builds.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;
use core::cmp::Ordering;
use core::fmt;

/// The three semantic outcomes of a total FCIS transition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

/// Resource classes measured by the deterministic budget.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Resource {
    /// Logical state reads.
    Read = 0,
    /// Logical state writes.
    Write = 1,
    /// Candidate evaluations in a bounded search.
    Candidate = 2,
    /// Effect-plan operations.
    Effect = 3,
    /// Canonical bytes emitted or consumed.
    Byte = 4,
    /// Proof or witness bytes emitted.
    WitnessByte = 5,
    /// Recursion or nesting depth.
    Depth = 6,
}

impl Resource {
    const COUNT: usize = 7;

    const fn index(self) -> usize {
        self as usize
    }
}

/// Immutable deterministic resource limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetLimits {
    limits: [u64; Resource::COUNT],
}

impl BudgetLimits {
    /// Creates an all-zero budget.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            limits: [0; Resource::COUNT],
        }
    }

    /// Returns a copy with one resource limit replaced.
    #[must_use]
    pub const fn with_limit(mut self, resource: Resource, limit: u64) -> Self {
        self.limits[resource.index()] = limit;
        self
    }

    /// Returns the configured limit.
    #[must_use]
    pub const fn limit(self, resource: Resource) -> u64 {
        self.limits[resource.index()]
    }
}

impl Default for BudgetLimits {
    fn default() -> Self {
        Self::zero()
    }
}

/// Exact deterministic resource consumption.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetUsed {
    used: [u64; Resource::COUNT],
}

impl BudgetUsed {
    /// Returns zero consumption.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            used: [0; Resource::COUNT],
        }
    }

    /// Returns the consumed amount for one resource.
    #[must_use]
    pub const fn used(self, resource: Resource) -> u64 {
        self.used[resource.index()]
    }
}

impl Default for BudgetUsed {
    fn default() -> Self {
        Self::zero()
    }
}

/// A deterministic budget consumed by logical work, never by wall-clock time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Budget {
    limits: BudgetLimits,
    used: BudgetUsed,
}

impl Budget {
    /// Creates a fresh budget with zero consumption.
    #[must_use]
    pub const fn new(limits: BudgetLimits) -> Self {
        Self {
            limits,
            used: BudgetUsed::zero(),
        }
    }

    /// Charges one resource atomically.
    ///
    /// On failure, consumption remains unchanged.
    pub fn charge(&mut self, resource: Resource, amount: u64) -> Result<(), BudgetExceeded> {
        let current = self.used.used[resource.index()];
        let Some(next) = current.checked_add(amount) else {
            return Err(BudgetExceeded {
                resource,
                limit: self.limits.limit(resource),
                attempted: u64::MAX,
            });
        };
        let limit = self.limits.limit(resource);
        if next > limit {
            return Err(BudgetExceeded {
                resource,
                limit,
                attempted: next,
            });
        }
        self.used.used[resource.index()] = next;
        Ok(())
    }

    /// Returns immutable limits.
    #[must_use]
    pub const fn limits(&self) -> BudgetLimits {
        self.limits
    }

    /// Returns exact consumption.
    #[must_use]
    pub const fn used(&self) -> BudgetUsed {
        self.used
    }
}

/// A deterministic resource-bound failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetExceeded {
    resource: Resource,
    limit: u64,
    attempted: u64,
}

impl BudgetExceeded {
    /// Returns the exhausted resource.
    #[must_use]
    pub const fn resource(self) -> Resource {
        self.resource
    }

    /// Returns the configured limit.
    #[must_use]
    pub const fn limit(self) -> u64 {
        self.limit
    }

    /// Returns the attempted post-charge consumption.
    #[must_use]
    pub const fn attempted(self) -> u64 {
        self.attempted
    }
}

impl fmt::Display for BudgetExceeded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "resource {:?} would consume {} above limit {}",
            self.resource, self.attempted, self.limit
        )
    }
}

/// A pure, deterministic FCIS transition.
///
/// Implementations must treat every input as immutable and must not observe
/// ambient I/O, time, randomness, scheduling, global state, or process state.
pub trait Transition {
    /// Immutable pre-state type.
    type State;
    /// Validated command type.
    type Command;
    /// Explicit policy, evidence, and execution-context type.
    type Context;
    /// Sealed candidate returned by accepted and committed-failure outcomes.
    type Candidate;
    /// Stable unchanged-state rejection reason.
    type Reject: StableReason;
    /// Stable committed-failure reason.
    type Failure: StableReason;

    /// Computes exactly one modeled decision for admitted inputs.
    fn step(
        state: &Self::State,
        command: &Self::Command,
        context: &Self::Context,
        budget: &mut Budget,
    ) -> Decision<Self::Candidate, Self::Reject, Self::Failure>;
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Reason {
        Later,
        Earlier,
        SameOrdinalButLexicallyFirst,
    }

    impl StableReason for Reason {
        fn code(&self) -> &'static str {
            match self {
                Self::Later => "later",
                Self::Earlier => "z_earlier",
                Self::SameOrdinalButLexicallyFirst => "a_earlier",
            }
        }

        fn precedence(&self) -> u16 {
            match self {
                Self::Later => 20,
                Self::Earlier | Self::SameOrdinalButLexicallyFirst => 10,
            }
        }
    }

    #[test]
    fn reason_selection_is_independent_of_input_order() {
        let left = first_reason([
            Reason::Later,
            Reason::Earlier,
            Reason::SameOrdinalButLexicallyFirst,
        ]);
        let right = first_reason([
            Reason::SameOrdinalButLexicallyFirst,
            Reason::Later,
            Reason::Earlier,
        ]);
        assert_eq!(left, Some(Reason::SameOrdinalButLexicallyFirst));
        assert_eq!(left, right);
    }

    #[test]
    fn failed_budget_charge_is_atomic() {
        let limits = BudgetLimits::zero().with_limit(Resource::Read, 2);
        let mut budget = Budget::new(limits);
        assert_eq!(budget.charge(Resource::Read, 2), Ok(()));
        let error = budget.charge(Resource::Read, 1);
        assert!(error.is_err());
        assert_eq!(budget.used().used(Resource::Read), 2);
    }

    #[test]
    fn decision_candidate_mapping_preserves_kind() {
        let accepted: Decision<u64, Reason, Reason> = Decision::Accept(Accepted::new(3));
        assert_eq!(
            accepted.map_candidate(|value| value + 1).kind(),
            DecisionKind::Accept
        );

        let failed: Decision<u64, Reason, Reason> =
            Decision::CommittedFailure(Failed::new(3, Reason::Later));
        assert_eq!(
            failed.map_candidate(|value| value + 1).kind(),
            DecisionKind::CommittedFailure
        );
    }
}
