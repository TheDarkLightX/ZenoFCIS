//! Semantic annotations for e-classes: scalar kind, interval bounds, exact
//! signatures over small domains, and the eager-trap flags.
//!
//! The library evaluates every instruction, so an `Add` or `Sub` that overflows
//! on some input tuple traps the whole program there, whether or not its value
//! is used. A node is *poisoned* on a tuple when an ancestor has trapped
//! there. Signatures record the value on every tuple of the declared domain,
//! with `None` at poisoned positions, and two classes merge only when their
//! signatures are identical, poison included. Without exact signatures the
//! annotations are conservative interval bounds and a may-poison flag.

use zeno_fcis_synthesis::finite::{Domain, Op};

use crate::transform::{advance, domain_size, first_tuple};

/// Largest domain, in tuples, for which signatures are exact. Six Boolean
/// inputs give exactly this many tuples.
pub(crate) const EXACT_SIGNATURE_TUPLES: u64 = 64;

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

/// Value of a class on every tuple of the declared domain, in the checker's
/// enumeration order; `None` where eager evaluation has already trapped.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct Signature {
    values: Vec<Option<i64>>,
}

impl Signature {
    pub(crate) fn constant(value: i64, tuples: usize) -> Self {
        Self {
            values: vec![Some(value); tuples],
        }
    }

    pub(crate) fn from_values(values: Vec<Option<i64>>) -> Self {
        Self { values }
    }

    pub(crate) fn values(&self) -> &[Option<i64>] {
        &self.values
    }

    /// Whether some tuple is poisoned.
    pub(crate) fn poisoned(&self) -> bool {
        self.values.iter().any(Option::is_none)
    }

    /// The single value taken on every tuple, when there is one and no
    /// tuple is poisoned.
    pub(crate) fn constant_value(&self) -> Option<i64> {
        let first = (*self.values.first()?)?;
        self.values
            .iter()
            .all(|value| *value == Some(first))
            .then_some(first)
    }

    /// Bit `i` set when the value on tuple `i` is `1`. Only meaningful for
    /// unpoisoned Boolean signatures over at most 64 tuples.
    pub(crate) fn bits(&self) -> Option<u64> {
        if self.values.len() > 64 {
            return None;
        }
        let mut bits = 0_u64;
        for (index, value) in self.values.iter().enumerate() {
            match *value {
                Some(1) => bits |= 1_u64 << index,
                Some(0) => {}
                _ => return None,
            }
        }
        Some(bits)
    }

    /// Whether both signatures agree, poison included, on every tuple whose
    /// bit is set in `mask`.
    pub(crate) fn agrees_on(&self, other: &Self, mask: u64) -> bool {
        self.values.len() == other.values.len()
            && (0..self.values.len())
                .filter(|index| mask & (1_u64 << index) != 0)
                .all(|index| self.values[index] == other.values[index])
    }
}

/// The declared input domain, with its tuples listed when it is small enough
/// for exact signatures.
#[derive(Clone, Debug)]
pub(crate) struct DomainInfo {
    inputs: Vec<Domain>,
    /// Exact number of tuples; `None` above `u64::MAX`.
    size: Option<u64>,
    /// Every tuple in enumeration order when `size <= EXACT_SIGNATURE_TUPLES`.
    tuples: Option<Vec<Vec<i64>>>,
}

impl DomainInfo {
    /// Fails only on an empty domain, which admission does not produce.
    pub(crate) fn new(inputs: &[Domain]) -> Option<Self> {
        let size = domain_size(inputs).ok()?;
        let size = size.and_then(|size| u64::try_from(size).ok());
        let tuples = match size {
            Some(size) if size <= EXACT_SIGNATURE_TUPLES => {
                let mut tuples = Vec::new();
                let mut tuple = first_tuple(inputs);
                loop {
                    tuples.push(tuple.clone());
                    if !advance(inputs, &mut tuple) {
                        break;
                    }
                }
                Some(tuples)
            }
            _ => None,
        };
        Some(Self {
            inputs: inputs.to_vec(),
            size,
            tuples,
        })
    }

    pub(crate) fn size(&self) -> Option<u64> {
        self.size
    }

    /// Whether signatures are exact for this domain.
    pub(crate) fn exact(&self) -> bool {
        self.tuples.is_some()
    }

    pub(crate) fn tuple_count(&self) -> usize {
        self.tuples.as_ref().map_or(0, Vec::len)
    }

    /// Mask of every tuple of an exact domain.
    pub(crate) fn full_mask(&self) -> u64 {
        match self.tuple_count() {
            64 => u64::MAX,
            count => (1_u64 << count) - 1,
        }
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
    /// Exact signature; `None` when the domain is too large.
    pub(crate) signature: Option<Signature>,
    /// Whether some member may be poisoned on some tuple. Exact when
    /// signatures are exact, otherwise conservative.
    pub(crate) may_poison: bool,
}

impl ClassData {
    /// The value every member takes on every tuple, when known.
    pub(crate) fn constant(&self) -> Option<i64> {
        match &self.signature {
            Some(signature) => signature.constant_value(),
            None if self.may_poison => None,
            None => self.interval.constant(),
        }
    }

    /// Whether this class may merge with `other` without changing any
    /// result: equal kinds and, with exact signatures, identical signatures
    /// including poison. Without signatures only unpoisoned classes merge,
    /// since the rules preserve values but not necessarily poison.
    pub(crate) fn compatible(&self, other: &Self) -> bool {
        if self.kind != other.kind {
            return false;
        }
        match (&self.signature, &other.signature) {
            (Some(left), Some(right)) => left == right,
            (None, None) => !self.may_poison && !other.may_poison,
            _ => false,
        }
    }

    /// Data of a merged class. Both bounds are sound, so their meet is too.
    pub(crate) fn merged(&self, other: &Self) -> Option<Self> {
        Some(Self {
            kind: self.kind,
            interval: self.interval.meet(other.interval)?,
            signature: self.signature.clone(),
            may_poison: self.may_poison || other.may_poison,
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
    /// operands are defined: exact with signatures, else from the bounds.
    pub(crate) may_trap: bool,
}

/// Why an instruction cannot be built over its operands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TypeError {
    /// An `Input` position outside the declared ABI.
    InputReference,
    /// Operand kinds disagree with the instruction's requirements.
    Mismatch,
}

/// Types one instruction and derives its annotations.
pub(crate) fn analyze(domain: &DomainInfo, shape: Shape<'_>) -> Result<Analysis, TypeError> {
    let tuples = domain.tuple_count();
    let exact = domain.exact();
    let require = |data: &ClassData, kind: Kind| {
        if data.kind == kind {
            Ok(())
        } else {
            Err(TypeError::Mismatch)
        }
    };
    let constant = |kind: Kind, value: i64| Analysis {
        data: ClassData {
            kind,
            interval: Interval::point(value),
            signature: exact.then(|| Signature::constant(value, tuples)),
            may_poison: false,
        },
        may_trap: false,
    };
    match shape {
        Shape::Input(index) => {
            let kind = domain.input_kind(index).ok_or(TypeError::InputReference)?;
            let interval = Interval::from_domain(domain.inputs[usize::from(index)]);
            let signature = domain.tuples.as_ref().map(|tuples| Signature {
                values: tuples
                    .iter()
                    .map(|tuple| tuple.get(usize::from(index)).copied())
                    .collect(),
            });
            Ok(Analysis {
                data: ClassData {
                    kind,
                    interval,
                    signature,
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
            let interval = Interval { min, max };
            let mut bound_trap = low_out || high_out;
            let mut exact_trap = false;
            let signature = combine(a, b, |left, right| {
                let result = if subtract {
                    left.checked_sub(right)
                } else {
                    left.checked_add(right)
                };
                if result.is_none() {
                    exact_trap = true;
                }
                result
            });
            if signature.is_some() {
                bound_trap = exact_trap;
            }
            Ok(finish(Kind::Int, interval, signature, a, b, bound_trap))
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
            let signature = combine(a, b, |left, right| Some(i64::from(left == right)));
            Ok(finish(Kind::Bool, interval, signature, a, b, false))
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
            let signature = combine(a, b, |left, right| Some(i64::from(left < right)));
            Ok(finish(Kind::Bool, interval, signature, a, b, false))
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
            let signature = combine(a, b, |left, right| Some(i64::from(left == 1 && right == 1)));
            Ok(finish(Kind::Bool, interval, signature, a, b, false))
        }
        Shape::Not(a) => {
            require(a, Kind::Bool)?;
            let interval = Interval {
                min: 1 - a.interval.max,
                max: 1 - a.interval.min,
            };
            let signature = a.signature.as_ref().map(|signature| Signature {
                values: signature
                    .values
                    .iter()
                    .map(|value| value.map(|value| i64::from(value == 0)))
                    .collect(),
            });
            Ok(finish(Kind::Bool, interval, signature, a, a, false))
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
            let signature = match (&c.signature, &a.signature, &b.signature) {
                (Some(c), Some(a), Some(b)) => Some(Signature {
                    values: c
                        .values
                        .iter()
                        .zip(&a.values)
                        .zip(&b.values)
                        .map(|((c, a), b)| match (*c, *a, *b) {
                            (Some(c), Some(a), Some(b)) => Some(if c == 1 { a } else { b }),
                            _ => None,
                        })
                        .collect(),
                }),
                _ => None,
            };
            let may_poison = c.may_poison || a.may_poison || b.may_poison;
            Ok(Analysis {
                data: ClassData {
                    kind: a.kind,
                    interval,
                    may_poison: signature.as_ref().map_or(may_poison, Signature::poisoned),
                    signature,
                },
                may_trap: false,
            })
        }
    }
}

/// Pointwise binary combination with poison propagation.
fn combine(
    a: &ClassData,
    b: &ClassData,
    mut op: impl FnMut(i64, i64) -> Option<i64>,
) -> Option<Signature> {
    let (left, right) = (a.signature.as_ref()?, b.signature.as_ref()?);
    Some(Signature {
        values: left
            .values
            .iter()
            .zip(&right.values)
            .map(|(left, right)| match (*left, *right) {
                (Some(left), Some(right)) => op(left, right),
                _ => None,
            })
            .collect(),
    })
}

fn finish(
    kind: Kind,
    interval: Interval,
    signature: Option<Signature>,
    a: &ClassData,
    b: &ClassData,
    may_trap: bool,
) -> Analysis {
    let may_poison = match &signature {
        Some(signature) => signature.poisoned(),
        None => a.may_poison || b.may_poison || may_trap,
    };
    Analysis {
        data: ClassData {
            kind,
            interval,
            signature,
            may_poison,
        },
        may_trap,
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
