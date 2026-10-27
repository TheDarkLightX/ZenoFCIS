//! Shared immutable typed leaves and width-explicit checked arithmetic.
pub(crate) mod scalar;
pub(crate) mod wide;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(crate) mod spec;

/// Complete typed value used by decision expressions and candidates.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Atom<'a> {
    /// Complete original value validated by the library parser.
    Canonical(super::execution_v2::compound::Value<'a>),
    /// Boolean value.
    Bool(bool),
    /// Signed 128-bit value.
    I128(i128),
    /// Unsigned 128-bit value.
    U128(u128),
    /// Original declared Enum type and variant.
    Enum {
        /// Original type identifier.
        type_id: u32,
        /// Original variant identifier.
        variant: u16,
    },
    /// Original declared payload-free Sum type and variant.
    Sum {
        /// Original type identifier.
        type_id: u32,
        /// Original variant identifier.
        variant: u16,
    },
    /// Complete uninterpreted bytes.
    Bytes(&'a [u8]),
    /// Complete text bytes, validated by the consuming schema profile.
    Text(&'a [u8]),
}
/// Complete allowed value domain for a decision slot.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Domain<'a> {
    /// A complete checked original type; values require private decoder custody.
    Canonical {
        /// Complete original schema bytes, compared without a digest assumption.
        schema: &'a [u8],
        /// Original type identifier.
        type_id: u32,
    },
    /// Both Boolean values.
    Bool,
    /// Inclusive signed range.
    I128 {
        /// Inclusive lower bound.
        min: i128,
        /// Inclusive upper bound.
        max: i128,
    },
    /// Inclusive unsigned range.
    U128 {
        /// Inclusive lower bound.
        min: u128,
        /// Inclusive upper bound.
        max: u128,
    },
    /// Original Enum type and complete allowed variant identifier set.
    Enum {
        /// Original type identifier.
        type_id: u32,
        /// Complete allowed variants; ordering has no semantic significance.
        variants: &'a [u16],
    },
    /// Original payload-free Sum type and complete variant identifier set.
    Sum {
        /// Original type identifier.
        type_id: u32,
        /// Complete allowed variants; ordering has no semantic significance.
        variants: &'a [u16],
    },
    /// Complete uninterpreted byte values.
    Bytes,
    /// Complete text values, subject to the consuming schema profile.
    Text,
}
/// Arithmetic width is independent of the original wire atom.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum IntegerWidth {
    /// Checked signed 64-bit arithmetic, represented by I128 atoms.
    Signed64,
    /// Checked signed 128-bit arithmetic.
    Signed128,
    /// Checked unsigned 128-bit arithmetic.
    Unsigned128,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArithmeticOp {
    Add,
    Sub,
}

/// Nonauthorizing data: wrong kinds precede operand/result range failures.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NumericFailure {
    Kind,
    Range,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::arithmetic(width, op, left, right),
))]
pub(crate) fn arithmetic<'a>(
    width: IntegerWidth,
    op: ArithmeticOp,
    left: Atom<'a>,
    right: Atom<'a>,
) -> Result<Atom<'a>, NumericFailure> {
    match (width, left, right) {
        (IntegerWidth::Signed64 | IntegerWidth::Signed128, Atom::I128(a), Atom::I128(b)) => {
            if matches!(width, IntegerWidth::Signed64)
                && (a < i64::MIN as i128
                    || a > i64::MAX as i128
                    || b < i64::MIN as i128
                    || b > i64::MAX as i128)
            {
                return Err(NumericFailure::Range);
            }
            let computed = match op {
                ArithmeticOp::Add => a.checked_add(b),
                ArithmeticOp::Sub => a.checked_sub(b),
            };
            match computed {
                Some(value) => {
                    if matches!(width, IntegerWidth::Signed64)
                        && (value < i64::MIN as i128 || value > i64::MAX as i128)
                    {
                        Err(NumericFailure::Range)
                    } else {
                        Ok(Atom::I128(value))
                    }
                }
                None => Err(NumericFailure::Range),
            }
        }
        (IntegerWidth::Unsigned128, Atom::U128(a), Atom::U128(b)) => {
            let computed = match op {
                ArithmeticOp::Add => a.checked_add(b),
                ArithmeticOp::Sub => a.checked_sub(b),
            };
            match computed {
                Some(value) => Ok(Atom::U128(value)),
                None => Err(NumericFailure::Range),
            }
        }
        _ => Err(NumericFailure::Kind),
    }
}

/// Exact embedding and checked projection for every legacy i64 operand pair.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::legacy(op, left, right),
))]
#[cfg(any(test, verus_keep_ghost))]
pub(crate) fn legacy_arithmetic(op: ArithmeticOp, left: i64, right: i64) -> Option<i64> {
    #[cfg(verus_keep_ghost)]
    proof! { spec::legacy_embedding(op, left, right); }
    match arithmetic(
        IntegerWidth::Signed64,
        op,
        Atom::I128(left as i128),
        Atom::I128(right as i128),
    ) {
        Ok(Atom::I128(value)) => {
            if value < i64::MIN as i128 || value > i64::MAX as i128 {
                None
            } else {
                Some(value as i64)
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::admitted(domain, value),))]
pub(crate) fn admitted(domain: Domain<'_>, value: Atom<'_>) -> bool {
    match (domain, value) {
        (Domain::Canonical { schema, type_id }, Atom::Canonical(v)) => {
            type_id == v.type_id() && super::execution_v2::util::bytes_equal(schema, v.schema())
        }
        (Domain::Bool, Atom::Bool(_))
        | (Domain::Bytes, Atom::Bytes(_))
        | (Domain::Text, Atom::Text(_)) => true,
        (Domain::I128 { min, max }, Atom::I128(v)) => min <= v && v <= max,
        (Domain::U128 { min, max }, Atom::U128(v)) => min <= v && v <= max,
        (
            Domain::Enum { type_id, variants },
            Atom::Enum {
                type_id: t,
                variant,
            },
        )
        | (
            Domain::Sum { type_id, variants },
            Atom::Sum {
                type_id: t,
                variant,
            },
        ) => type_id == t && contains(variants, variant),
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == values@.contains(id),))]
pub(crate) fn contains(values: &[u16], id: u16) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= values.len(),
        forall|j:int| 0 <= j < i ==> values@[j] != id, decreases values.len() - i,))]
    while i < values.len() {
        if values[i] == id {
            return true;
        }
        i += 1;
    }
    false
}

/// Admit a nonempty, strictly increasing table of declared variant codes.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (variants@.len() > 0 &&
        forall|j:int| 0 < j < variants@.len() ==> variants@[j-1] < #[trigger] variants@[j]),
))]
pub(crate) fn validate_variant_codes(variants: &[u16]) -> bool {
    if variants.is_empty() {
        return false;
    }
    let mut index = 1usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant 1 <= index <= variants.len(),
            forall|j:int| 0 < j < index ==> variants@[j-1] < #[trigger] variants@[j],
        decreases variants.len() - index,
    ))]
    while index < variants.len() {
        if variants[index - 1] >= variants[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// Test membership without changing the declared variant-code domain.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (exists|j:int| 0 <= j < variants@.len() && variants@[j] == variant),
))]
pub(crate) fn contains_variant_code(variants: &[u16], variant: u16) -> bool {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= variants.len(),
            forall|j:int| 0 <= j < index ==> variants@[j] != variant,
        decreases variants.len() - index,
    ))]
    while index < variants.len() {
        if variants[index] == variant {
            return true;
        }
        index += 1;
    }
    false
}
