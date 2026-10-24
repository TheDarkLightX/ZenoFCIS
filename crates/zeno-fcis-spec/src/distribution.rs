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

/// A reduced signed rational with an i64 numerator and u64 positive denominator.
/// Arithmetic uses checked wide intermediates; an unrepresentable result is refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rational {
    numerator: i64,
    denominator: u64,
}

impl Rational {
    /// Additive identity.
    pub const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };
    /// Constructs and reduces a rational within the component bounds.
    pub fn try_new(numerator: i128, denominator: u128) -> Option<Self> {
        if denominator == 0 {
            return None;
        }
        let divisor = gcd(numerator.unsigned_abs(), denominator);
        let magnitude = u64::try_from(numerator.unsigned_abs() / divisor).ok()?;
        let signed = if numerator < 0 {
            -i128::from(magnitude)
        } else {
            i128::from(magnitude)
        };
        Some(Self {
            numerator: i64::try_from(signed).ok()?,
            denominator: u64::try_from(denominator / divisor).ok()?,
        })
    }
    /// Reduced signed numerator.
    pub const fn numerator(self) -> i64 {
        self.numerator
    }
    /// Reduced positive denominator.
    pub const fn denominator(self) -> u64 {
        self.denominator
    }
    /// Exact bounded addition.
    pub fn checked_add(self, other: Self) -> Option<Self> {
        Self::try_new(
            (i128::from(self.numerator) * i128::from(other.denominator))
                .checked_add(i128::from(other.numerator) * i128::from(self.denominator))?,
            u128::from(self.denominator) * u128::from(other.denominator),
        )
    }
    /// Exact bounded subtraction.
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        Self::try_new(
            (i128::from(self.numerator) * i128::from(other.denominator))
                .checked_sub(i128::from(other.numerator) * i128::from(self.denominator))?,
            u128::from(self.denominator) * u128::from(other.denominator),
        )
    }
    /// Exact bounded multiplication.
    pub fn checked_mul(self, other: Self) -> Option<Self> {
        Self::try_new(
            i128::from(self.numerator) * i128::from(other.numerator),
            u128::from(self.denominator) * u128::from(other.denominator),
        )
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (i128::from(self.numerator) * i128::from(other.denominator))
            .cmp(&(i128::from(other.numerator) * i128::from(self.denominator)))
    }
}
impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// One finite reward functional.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RewardKind {
    /// `E[R]`.
    Expectation,
    /// `E[R^2] - E[R]^2`.
    Variance,
    /// P(R >= t), including equality at the threshold.
    Threshold(i128),
    /// E[max(0, R - b)].
    Excess(i128),
}

/// An exact comparison with a declared rational bound.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RewardComparison {
    /// At most the bound.
    AtMost,
    /// At least the bound.
    AtLeast,
    /// Exactly the bound.
    Equal,
}
impl RewardComparison {
    /// Whether this comparison holds exactly.
    pub fn holds(self, actual: Rational, bound: Rational) -> bool {
        match self {
            Self::AtMost => actual <= bound,
            Self::AtLeast => actual >= bound,
            Self::Equal => actual == bound,
        }
    }
}

/// One measure and its exact bound, in canonical order within a declaration.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RewardMeasure {
    pub(crate) kind: RewardKind,
    pub(crate) comparison: RewardComparison,
    pub(crate) bound: Rational,
}
impl RewardMeasure {
    /// Functional to evaluate.
    pub const fn kind(self) -> RewardKind {
        self.kind
    }
    /// Comparison to apply.
    pub const fn comparison(self) -> RewardComparison {
        self.comparison
    }
    /// Exact rational bound.
    pub const fn bound(self) -> Rational {
        self.bound
    }
}

/// Inert reward constraints, checked per fiber using the distribution fold.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) random: Box<[StableId]>,
    pub(crate) outcome: ProjectionPath,
    pub(crate) failure: Option<i128>,
    pub(crate) measures: Box<[RewardMeasure]>,
}
impl RewardDecl {
    /// Declaration identifier.
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Declaration name.
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// All random premise IDs, sorted and distinct.
    pub const fn random(&self) -> &[StableId] {
        &self.random
    }
    /// Integer successor projection.
    pub const fn outcome(&self) -> &ProjectionPath {
        &self.outcome
    }
    /// Explicit constant for committed failures, or none (which refutes).
    pub const fn failure(&self) -> Option<i128> {
        self.failure
    }
    /// Nonempty, sorted and distinct measure constraints.
    pub const fn measures(&self) -> &[RewardMeasure] {
        &self.measures
    }
}
