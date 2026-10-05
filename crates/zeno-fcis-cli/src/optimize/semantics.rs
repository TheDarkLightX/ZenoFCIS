//! Semantic annotations for e-classes: scalar kind, interval bounds, support,
//! an exact support-local table when the support is small enough, the values
//! at fixed sample tuples, and the eager-trap flags.
//!
//! The library evaluates every instruction, so an `Add` or `Sub` that overflows
//! on some input tuple traps the whole program there, whether or not its value
//! is used. A node is *poisoned* on a tuple when an ancestor has trapped
//! there. A class's table records its value or poison on every tuple of the
//! product of its support's domains (see [`super::signature`]), and two
//! classes merge by rule only when their tables are identical, poison
//! included. A class without a table, because its support is too large or the
//! table budget is spent, keeps conservative interval bounds and a may-poison
//! flag, and merges by rule only when neither side may be poisoned.

use std::sync::Arc;

use zeno_fcis_synthesis::finite::{Domain, Op};

use super::egraph::MergeReason;
use super::signature::{self, Budget, EXHAUSTIVE_SAMPLES, Samples, Table};
use crate::transform::{advance, domain_size, first_tuple};

/// Scalar kind of a class; the library types Bool apart from Int.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum Kind {
    Bool,
    Int,
}

/// Inclusive bounds on every defined value of a class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Interval {
    pub(crate) min: i64,
    pub(crate) max: i64,
}

impl Interval {
    pub(crate) const BOOL: Self = Self { min: 0, max: 1 };

    pub(crate) const fn point(value: i64) -> Self {
        Self {
            min: value,
            max: value,
        }
    }

    pub(crate) fn constant(self) -> Option<i64> {
        (self.min == self.max).then_some(self.min)
    }

    fn hull(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// Intersection of two bounds on the same values; `None` when they are
    /// inconsistent, which no sound merge produces.
    pub(crate) fn meet(self, other: Self) -> Option<Self> {
        let meet = Self {
            min: self.min.max(other.min),
            max: self.max.min(other.max),
        };
        (meet.min <= meet.max).then_some(meet)
    }

    fn from_domain(domain: Domain) -> Self {
        let (min, max) = domain.bounds();
        Self { min, max }
    }
}

/// Clamps an `i128` bound into `i64`, reporting whether it was out of range.
fn clamp(value: i128) -> (i64, bool) {
    match i64::try_from(value) {
        Ok(value) => (value, false),
        Err(_) if value < 0 => (i64::MIN, true),
        Err(_) => (i64::MAX, true),
    }
}

/// The declared input domain: per-input bounds and the fixed sample tuples.
#[derive(Clone, Debug)]
pub(crate) struct DomainInfo {
    inputs: Vec<Domain>,
    /// Exact number of tuples; `None` above `u64::MAX`.
    size: Option<u64>,
    minima: Vec<i64>,
    /// Number of values of each input, saturated at `u64::MAX`.
    widths: Vec<u64>,
    /// Every tuple in enumeration order when the domain has at most
    /// [`EXHAUSTIVE_SAMPLES`] tuples; otherwise the fixed selection of
    /// [`signature::sample_tuples`].
    samples: Vec<Vec<i64>>,
    exhaustive: bool,
}

impl DomainInfo {
    /// Fails only on an empty domain, which admission does not produce.
    pub(crate) fn new(inputs: &[Domain]) -> Option<Self> {
        let size = domain_size(inputs).ok()?;
        let size = size.and_then(|size| u64::try_from(size).ok());
        let bounds: Vec<(i64, i64)> = inputs.iter().map(|domain| domain.bounds()).collect();
        let (samples, exhaustive) = match size {
            Some(size) if size <= EXHAUSTIVE_SAMPLES => {
                let mut tuples = Vec::new();
                let mut tuple = first_tuple(inputs);
                loop {
                    tuples.push(tuple.clone());
                    if !advance(inputs, &mut tuple) {
                        break;
                    }
                }
                (tuples, true)
            }
            _ => (signature::sample_tuples(&bounds), false),
        };
        Some(Self {
            inputs: inputs.to_vec(),
            size,
            minima: bounds.iter().map(|(min, _)| *min).collect(),
            widths: bounds
                .iter()
                .map(|(min, max)| max.abs_diff(*min).saturating_add(1))
                .collect(),
            samples,
            exhaustive,
        })
    }

    pub(crate) fn size(&self) -> Option<u64> {
        self.size
    }

    pub(crate) fn widths(&self) -> &[u64] {
        &self.widths
    }

    #[cfg(test)]
    pub(crate) fn minima(&self) -> &[i64] {
        &self.minima
    }

    /// Whether the samples are every tuple of the domain, so that equal
    /// samples mean equal functions.
    pub(crate) fn exhaustive(&self) -> bool {
        self.exhaustive
    }

    pub(crate) fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Mask of every sample, one bit per sample.
    pub(crate) fn sample_mask(&self) -> Vec<u64> {
        signature::words(
            std::iter::repeat_n(true, self.samples.len()),
            self.samples.len(),
        )
    }

    fn input_kind(&self, index: u16) -> Option<Kind> {
        self.inputs
            .get(usize::from(index))
            .map(|domain| match domain {
                Domain::Bool => Kind::Bool,
                _ => Kind::Int,
            })
    }
}

/// Everything the e-graph knows about one class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClassData {
    pub(crate) kind: Kind,
    pub(crate) interval: Interval,
    /// Exact table over the class's minimal support; `None` when the support
    /// is too large or the table budget is spent.
    pub(crate) table: Option<Arc<Table>>,
    /// Inputs the class may depend on: the table's support when there is a
    /// table, an upper bound otherwise.
    pub(crate) support: Vec<u16>,
    /// Values at the domain's sample tuples.
    pub(crate) samples: Arc<Samples>,
    /// Whether some member may be poisoned on some tuple. Exact with a table,
    /// otherwise conservative.
    pub(crate) may_poison: bool,
}

/// Whether two classes may merge, and the merged data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MergeDecision {
    Merged(ClassData),
    /// A rule merge the guard does not admit.
    Refused,
    /// Disagreeing annotations where none can disagree.
    Inconsistent,
}

impl ClassData {
    /// The value every member takes on every tuple, when known.
    pub(crate) fn constant(&self) -> Option<i64> {
        match &self.table {
            Some(table) => table.constant_value(),
            None if self.may_poison => None,
            None => self.interval.constant(),
        }
    }

    /// Decides a merge. A rule merge needs equal kinds and either identical
    /// tables, poison included, or, when a side has no table, two classes
    /// that no trap can poison. Different sample values refuse any rule
    /// merge. A structural merge (congruence or commutativity) joins the
    /// same computation twice, so any disagreement is an inconsistency. A
    /// checked merge joins two roots the checker found equal on every tuple
    /// of a program that never fails: it needs equal kinds, samples and, when
    /// both have tables, tables, and the merged class is never poisoned.
    pub(crate) fn merge(&self, other: &Self, reason: MergeReason) -> MergeDecision {
        if reason == MergeReason::Checked {
            return self.checked(other);
        }
        let structural = reason == MergeReason::Structural;
        let disagree = if structural {
            MergeDecision::Inconsistent
        } else {
            MergeDecision::Refused
        };
        if self.kind != other.kind {
            return disagree;
        }
        let table = match (&self.table, &other.table) {
            (Some(left), Some(right)) if left != right => return disagree,
            (Some(left), Some(_)) => {
                if self.samples != other.samples {
                    return MergeDecision::Inconsistent;
                }
                Some(Arc::clone(left))
            }
            (left, right) => {
                if !structural && (self.may_poison || other.may_poison) {
                    return MergeDecision::Refused;
                }
                if self.samples != other.samples {
                    return disagree;
                }
                left.clone().or_else(|| right.clone())
            }
        };
        let Some(interval) = self.interval.meet(other.interval) else {
            return MergeDecision::Inconsistent;
        };
        let (support, may_poison) = match &table {
            Some(table) => (table.support().to_vec(), table.poisoned()),
            None => (
                self.support
                    .iter()
                    .copied()
                    .filter(|input| other.support.contains(input))
                    .collect(),
                self.may_poison || other.may_poison,
            ),
        };
        MergeDecision::Merged(Self {
            kind: self.kind,
            interval,
            table,
            support,
            samples: Arc::clone(&self.samples),
            may_poison,
        })
    }
}

impl ClassData {
    fn checked(&self, other: &Self) -> MergeDecision {
        let agree = self.kind == other.kind
            && self.samples == other.samples
            && match (&self.table, &other.table) {
                (Some(left), Some(right)) => left == right,
                _ => true,
            };
        let interval = self.interval.meet(other.interval);
        let (true, Some(interval)) = (agree, interval) else {
            return MergeDecision::Refused;
        };
        let table = self.table.clone().or_else(|| other.table.clone());
        MergeDecision::Merged(Self {
            kind: self.kind,
            interval,
            support: match &table {
                Some(table) => table.support().to_vec(),
                None => self
                    .support
                    .iter()
                    .copied()
                    .filter(|input| other.support.contains(input))
                    .collect(),
            },
            table,
            samples: Arc::clone(&self.samples),
            may_poison: false,
        })
    }
}

/// An instruction over class data instead of node references.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Shape<'a> {
    Input(u16),
    Int(i64),
    Bool(bool),
    Add(&'a ClassData, &'a ClassData),
    Sub(&'a ClassData, &'a ClassData),
    Eq(&'a ClassData, &'a ClassData),
    Lt(&'a ClassData, &'a ClassData),
    And(&'a ClassData, &'a ClassData),
    Not(&'a ClassData),
    Select(&'a ClassData, &'a ClassData, &'a ClassData),
}

/// The annotations of one instruction, derived from its operands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Analysis {
    pub(crate) data: ClassData,
    /// Whether the instruction itself may overflow on some tuple whose
    /// operands are defined: exact with tables, else from the bounds.
    pub(crate) may_trap: bool,
}

/// Why an instruction cannot be built over its operands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TypeError {
    /// An `Input` position outside the declared ABI.
    InputReference,
    /// Operand kinds disagree with the instruction's requirements.
    Mismatch,
    /// The search's profile excludes the instruction.
    Excluded,
}

fn require(data: &ClassData, kind: Kind) -> Result<(), TypeError> {
    if data.kind == kind {
        Ok(())
    } else {
        Err(TypeError::Mismatch)
    }
}

/// Types one instruction and derives its annotations. Computing a table
/// spends from `budget`; keeping it is the e-graph's decision.
pub(crate) fn analyze(
    domain: &DomainInfo,
    shape: Shape<'_>,
    budget: &mut Budget,
) -> Result<Analysis, TypeError> {
    let constant = |kind: Kind, value: i64| Analysis {
        data: ClassData {
            kind,
            interval: Interval::point(value),
            table: Some(Arc::new(signature::constant(kind, value))),
            support: Vec::new(),
            samples: Arc::new(Samples::constant(kind, value, domain.sample_count())),
            may_poison: false,
        },
        may_trap: false,
    };
    match shape {
        Shape::Input(index) => {
            let kind = domain.input_kind(index).ok_or(TypeError::InputReference)?;
            let position = usize::from(index);
            let interval = Interval::from_domain(domain.inputs[position]);
            let table =
                signature::input(index, kind, domain.minima[position], &domain.widths, budget);
            let samples = Samples::from_values(
                kind,
                domain.samples.iter().map(|tuple| tuple[position]).collect(),
            );
            Ok(Analysis {
                data: ClassData {
                    kind,
                    interval,
                    support: table
                        .as_ref()
                        .map_or_else(|| vec![index], |table| table.support().to_vec()),
                    table: table.map(Arc::new),
                    samples: Arc::new(samples),
                    may_poison: false,
                },
                may_trap: false,
            })
        }
        Shape::Int(value) => Ok(constant(Kind::Int, value)),
        Shape::Bool(value) => Ok(constant(Kind::Bool, i64::from(value))),
        Shape::Add(a, b) | Shape::Sub(a, b) => {
            require(a, Kind::Int)?;
            require(b, Kind::Int)?;
            let subtract = matches!(shape, Shape::Sub(..));
            let (low, high) = if subtract {
                (
                    i128::from(a.interval.min) - i128::from(b.interval.max),
                    i128::from(a.interval.max) - i128::from(b.interval.min),
                )
            } else {
                (
                    i128::from(a.interval.min) + i128::from(b.interval.min),
                    i128::from(a.interval.max) + i128::from(b.interval.max),
                )
            };
            let (min, low_out) = clamp(low);
            let (max, high_out) = clamp(high);
            let op = move |arguments: &[i64]| {
                if subtract {
                    arguments[0].checked_sub(arguments[1])
                } else {
                    arguments[0].checked_add(arguments[1])
                }
            };
            Ok(derive(
                domain,
                budget,
                Kind::Int,
                Interval { min, max },
                &[a, b],
                low_out || high_out,
                op,
            ))
        }
        Shape::Eq(a, b) => {
            if a.kind != b.kind {
                return Err(TypeError::Mismatch);
            }
            let interval = if a.interval.max < b.interval.min || b.interval.max < a.interval.min {
                Interval::point(0)
            } else if let (Some(left), Some(right)) = (a.interval.constant(), b.interval.constant())
                && left == right
            {
                Interval::point(1)
            } else {
                Interval::BOOL
            };
            Ok(derive(
                domain,
                budget,
                Kind::Bool,
                interval,
                &[a, b],
                false,
                |arguments| Some(i64::from(arguments[0] == arguments[1])),
            ))
        }
        Shape::Lt(a, b) => {
            require(a, Kind::Int)?;
            require(b, Kind::Int)?;
            let interval = if a.interval.max < b.interval.min {
                Interval::point(1)
            } else if a.interval.min >= b.interval.max {
                Interval::point(0)
            } else {
                Interval::BOOL
            };
            Ok(derive(
                domain,
                budget,
                Kind::Bool,
                interval,
                &[a, b],
                false,
                |arguments| Some(i64::from(arguments[0] < arguments[1])),
            ))
        }
        Shape::And(a, b) => {
            require(a, Kind::Bool)?;
            require(b, Kind::Bool)?;
            let interval = if a.interval.min == 1 && b.interval.min == 1 {
                Interval::point(1)
            } else if a.interval.max == 0 || b.interval.max == 0 {
                Interval::point(0)
            } else {
                Interval::BOOL
            };
            Ok(derive(
                domain,
                budget,
                Kind::Bool,
                interval,
                &[a, b],
                false,
                |arguments| Some(i64::from(arguments[0] == 1 && arguments[1] == 1)),
            ))
        }
        Shape::Not(a) => {
            require(a, Kind::Bool)?;
            let interval = Interval {
                min: 1 - a.interval.max,
                max: 1 - a.interval.min,
            };
            Ok(derive(
                domain,
                budget,
                Kind::Bool,
                interval,
                &[a],
                false,
                |arguments| Some(i64::from(arguments[0] == 0)),
            ))
        }
        Shape::Select(c, a, b) => {
            require(c, Kind::Bool)?;
            if a.kind != b.kind {
                return Err(TypeError::Mismatch);
            }
            let interval = if c.interval.min == 1 {
                a.interval
            } else if c.interval.max == 0 {
                b.interval
            } else {
                a.interval.hull(b.interval)
            };
            Ok(derive(
                domain,
                budget,
                a.kind,
                interval,
                &[c, a, b],
                false,
                |arguments| {
                    Some(if arguments[0] == 1 {
                        arguments[1]
                    } else {
                        arguments[2]
                    })
                },
            ))
        }
    }
}

/// The annotations of an instruction computing `op` over its operands with
/// eager poison. `bound_trap` is the interval verdict on overflow, used when
/// no table can be computed.
fn derive(
    domain: &DomainInfo,
    budget: &mut Budget,
    kind: Kind,
    interval: Interval,
    operands: &[&ClassData],
    bound_trap: bool,
    op: impl Fn(&[i64]) -> Option<i64> + Copy,
) -> Analysis {
    let samples: Vec<&Samples> = operands.iter().map(|data| &*data.samples).collect();
    let (samples, _) = Samples::combine(kind, &samples, op);
    let combined = operands
        .iter()
        .map(|data| data.table.as_deref())
        .collect::<Option<Vec<&Table>>>()
        .and_then(|tables| signature::combine(kind, &tables, domain.widths(), budget, op));
    match combined {
        Some(combined) => {
            let table = combined.table;
            let interval = match table.range() {
                Some((min, max)) => interval.meet(Interval { min, max }).unwrap_or(interval),
                None => interval,
            };
            Analysis {
                data: ClassData {
                    kind,
                    interval,
                    support: table.support().to_vec(),
                    may_poison: table.poisoned(),
                    table: Some(Arc::new(table)),
                    samples: Arc::new(samples),
                },
                may_trap: combined.trapped,
            }
        }
        None => {
            let mut support: Vec<u16> = operands
                .iter()
                .flat_map(|data| data.support.iter().copied())
                .collect();
            support.sort_unstable();
            support.dedup();
            Analysis {
                data: ClassData {
                    kind,
                    interval,
                    table: None,
                    support,
                    samples: Arc::new(samples),
                    may_poison: operands.iter().any(|data| data.may_poison) || bound_trap,
                },
                may_trap: bound_trap,
            }
        }
    }
}

/// Canonical byte length of one encoded instruction: a tuple header of five
/// bytes plus seventeen bytes per signed integer (tag and arguments).
pub(crate) fn op_bytes(op: &Op) -> u64 {
    let arguments: u64 = match op {
        Op::Input(_) | Op::Int(_) | Op::Bool(_) | Op::Not(_) => 1,
        Op::Add(..) | Op::Sub(..) | Op::Eq(..) | Op::Lt(..) | Op::And(..) => 2,
        Op::Select(..) => 3,
        _ => 3,
    };
    5 + 17 * (1 + arguments)
}
