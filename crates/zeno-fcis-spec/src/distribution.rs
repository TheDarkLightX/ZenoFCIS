//! Inert finite-distribution declarations and bounded exact probabilities.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::{Identifier, ProjectionPath, StableId};

/// A reduced, nonnegative rational. Components are bounded to 64 bits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Probability {
    numerator: u64,
    denominator: u64,
}

impl Probability {
    /// Zero mass.
    pub const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };
    /// Unit mass.
    pub const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };
    /// Constructs a probability in `[0, 1]`; zero denominators are refused.
    pub fn try_new(numerator: u128, denominator: u128) -> Option<Self> {
        if denominator == 0 || numerator > denominator {
            return None;
        }
        let divisor = gcd(numerator, denominator);
        Some(Self {
            numerator: u64::try_from(numerator / divisor).ok()?,
            denominator: u64::try_from(denominator / divisor).ok()?,
        })
    }

    /// Reduced numerator.
    pub const fn numerator(self) -> u64 {
        self.numerator
    }
    /// Positive reduced denominator.
    pub const fn denominator(self) -> u64 {
        self.denominator
    }

    /// Adds exact weights; overflow or mass greater than one is refused.
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let a = u128::from(self.numerator);
        let b = u128::from(self.denominator);
        let c = u128::from(other.numerator);
        let d = u128::from(other.denominator);
        Self::try_new((a * d).checked_add(c * b)?, b * d)
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// One input assumed to consist of uniform bits, independent of all other
/// declared random inputs and of the fixed non-random tuple.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomInputDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) input: ProjectionPath,
    pub(crate) bits: u16,
}

impl RandomInputDecl {
    /// Declaration identifier.
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Declaration name.
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Input projection.
    pub const fn input(&self) -> &ProjectionPath {
        &self.input
    }
    /// Bit width, including zero bits for the singleton domain.
    pub const fn bits(&self) -> u16 {
        self.bits
    }
}

/// One outcome of a distribution law. An accepted decision is its projected
/// successor value. Every committed failure is the single outcome
/// `committed-failure`, whatever successor value it writes.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DistributionOutcome {
    /// An accepted decision with this projected value.
    Value(i128),
    /// Any committed failure; its projected value is ignored.
    CommittedFailure,
}

/// A finite output-distribution law, checked separately from invocation laws.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistributionDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) outcome: ProjectionPath,
    pub(crate) target: Box<[(DistributionOutcome, Probability)]>,
}

impl DistributionDecl {
    /// Declaration identifier.
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Declaration name.
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Scalar outcome projection.
    pub const fn outcome(&self) -> &ProjectionPath {
        &self.outcome
    }
    /// Sorted distinct outcomes with positive exact mass summing to one;
    /// accepted values precede `committed-failure`.
    pub const fn target(&self) -> &[(DistributionOutcome, Probability)] {
        &self.target
    }

    pub(crate) fn normalized_target(
        mut target: Vec<(DistributionOutcome, Probability)>,
    ) -> Option<Box<[(DistributionOutcome, Probability)]>> {
        target.sort_by_key(|(value, _)| *value);
        if target.windows(2).any(|w| w[0].0 == w[1].0) {
            return None;
        }
        let total = target
            .iter()
            .try_fold(Probability::try_new(0, 1)?, |sum, (_, p)| {
                sum.checked_add(*p)
            })?;
        if total != Probability::try_new(1, 1)? {
            return None;
        }
        target.retain(|(_, p)| p.numerator() != 0);
        Some(target.into_boxed_slice())
    }
}
