//! Exact typed atom operations.
use super::super::super::value::{self, ArithmeticOp, IntegerWidth};
#[cfg(verus_keep_ghost)]
use super::spec;
use super::{Atom, Failure};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (left@ == right@),))]
fn bytes_equal(left: &[u8], right: &[u8]) -> bool {
    super::super::util::bytes_equal(left, right)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (spec::atom(left) == spec::atom(right)),
))]
pub(super) fn equal(left: Atom<'_>, right: Atom<'_>) -> bool {
    match (left, right) {
        (Atom::Canonical(a), Atom::Canonical(b)) => {
            a.type_id() == b.type_id()
                && bytes_equal(a.schema(), b.schema())
                && bytes_equal(a.wire(), b.wire())
        }
        (Atom::Bool(a), Atom::Bool(b)) => a == b,
        (Atom::I128(a), Atom::I128(b)) => a == b,
        (Atom::U128(a), Atom::U128(b)) => a == b,
        (
            Atom::Enum {
                type_id: a,
                variant: b,
            },
            Atom::Enum {
                type_id: c,
                variant: d,
            },
        ) => a == c && b == d,
        (
            Atom::Sum {
                type_id: a,
                variant: b,
            },
            Atom::Sum {
                type_id: c,
                variant: d,
            },
        ) => a == c && b == d,
        (Atom::Bytes(a), Atom::Bytes(b)) | (Atom::Text(a), Atom::Text(b)) => bytes_equal(a, b),
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::same_type(spec::atom(left),spec::atom(right)),
))]
pub(super) fn same_type(left: Atom<'_>, right: Atom<'_>) -> bool {
    match (left, right) {
        (Atom::Canonical(a), Atom::Canonical(b)) => {
            a.type_id() == b.type_id() && bytes_equal(a.schema(), b.schema())
        }
        (Atom::Bool(_), Atom::Bool(_))
        | (Atom::I128(_), Atom::I128(_))
        | (Atom::U128(_), Atom::U128(_))
        | (Atom::Bytes(_), Atom::Bytes(_))
        | (Atom::Text(_), Atom::Text(_)) => true,
        (Atom::Enum { type_id: a, .. }, Atom::Enum { type_id: b, .. })
        | (Atom::Sum { type_id: a, .. }, Atom::Sum { type_id: b, .. }) => a == b,
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::value_result(result) == spec::binary(kind,spec::atom(left),spec::atom(right)),
))]
pub(super) fn binary<'a>(kind: u8, left: Atom<'a>, right: Atom<'a>) -> Result<Atom<'a>, Failure> {
    if kind == 2 {
        return Ok(Atom::Bool(equal(left, right)));
    }
    match (kind, left, right) {
        (0, Atom::I128(a), Atom::I128(b)) => match value::arithmetic(
            IntegerWidth::Signed128,
            ArithmeticOp::Add,
            Atom::I128(a),
            Atom::I128(b),
        ) {
            Ok(value) => Ok(value),
            Err(_) => Err(Failure::Undefined),
        },
        (1, Atom::I128(a), Atom::I128(b)) => match value::arithmetic(
            IntegerWidth::Signed128,
            ArithmeticOp::Sub,
            Atom::I128(a),
            Atom::I128(b),
        ) {
            Ok(value) => Ok(value),
            Err(_) => Err(Failure::Undefined),
        },
        (0, Atom::U128(a), Atom::U128(b)) => match value::arithmetic(
            IntegerWidth::Unsigned128,
            ArithmeticOp::Add,
            Atom::U128(a),
            Atom::U128(b),
        ) {
            Ok(value) => Ok(value),
            Err(_) => Err(Failure::Undefined),
        },
        (1, Atom::U128(a), Atom::U128(b)) => match value::arithmetic(
            IntegerWidth::Unsigned128,
            ArithmeticOp::Sub,
            Atom::U128(a),
            Atom::U128(b),
        ) {
            Ok(value) => Ok(value),
            Err(_) => Err(Failure::Undefined),
        },
        (5, Atom::I128(a), Atom::I128(b)) => match a.checked_mul(b) {
            Some(x) => Ok(Atom::I128(x)),
            None => Err(Failure::Undefined),
        },
        (5, Atom::U128(a), Atom::U128(b)) => match a.checked_mul(b) {
            Some(x) => Ok(Atom::U128(x)),
            None => Err(Failure::Undefined),
        },
        (3, Atom::I128(a), Atom::I128(b)) => Ok(Atom::Bool(a < b)),
        (3, Atom::U128(a), Atom::U128(b)) => Ok(Atom::Bool(a < b)),
        (4, Atom::Bool(a), Atom::Bool(b)) => Ok(Atom::Bool(a && b)),
        _ => Err(Failure::Undefined),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result as int == spec::magnitude(value as int),))]
fn magnitude(value: i128) -> u128 {
    if value < 0 {
        (-(value + 1)) as u128 + 1
    } else {
        value as u128
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::value_result(result)==spec::integer(if negative {-(value as int)} else {value as int},true),
))]
fn signed<'a>(value: u128, negative: bool) -> Result<Atom<'a>, Failure> {
    if negative && value == (i128::MAX as u128) + 1 {
        return Ok(Atom::I128(i128::MIN));
    }
    if value > i128::MAX as u128 {
        return Err(Failure::Undefined);
    }
    if negative {
        Ok(Atom::I128(-(value as i128)))
    } else {
        Ok(Atom::I128(value as i128))
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::value_result(result)==spec::division(mode,spec::atom(left),spec::atom(right)),
))]
pub(super) fn divide<'a>(
    mode: super::Division,
    left: Atom<'a>,
    right: Atom<'a>,
) -> Result<Atom<'a>, Failure> {
    match (left, right) {
        (Atom::I128(a), Atom::I128(b)) => {
            if b == 0 || (a == i128::MIN && b == -1) {
                return Err(Failure::Undefined);
            }
            let negative = (a < 0) != (b < 0);
            let x = magnitude(a);
            let y = magnitude(b);
            let q = x / y;
            let r = x % y;
            if matches!(mode, super::Division::Exact) && r != 0 {
                return Err(Failure::Undefined);
            }
            let round = r != 0
                && ((matches!(mode, super::Division::Floor) && negative)
                    || (matches!(mode, super::Division::Ceil) && !negative));
            let result = if round {
                match q.checked_add(1) {
                    Some(v) => v,
                    None => return Err(Failure::Undefined),
                }
            } else {
                q
            };
            signed(result, negative)
        }
        (Atom::U128(a), Atom::U128(b)) => {
            if b == 0 {
                return Err(Failure::Undefined);
            }
            let q = a / b;
            let r = a % b;
            if matches!(mode, super::Division::Exact) && r != 0 {
                return Err(Failure::Undefined);
            }
            if matches!(mode, super::Division::Ceil) && r != 0 {
                match q.checked_add(1) {
                    Some(v) => Ok(Atom::U128(v)),
                    None => Err(Failure::Undefined),
                }
            } else {
                Ok(Atom::U128(q))
            }
        }
        _ => Err(Failure::Undefined),
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures spec::value_result(result)==spec::to_i128(spec::atom(value)),))]
pub(super) fn to_i128<'a>(value: Atom<'a>) -> Result<Atom<'a>, Failure> {
    match value {
        Atom::I128(v) => Ok(Atom::I128(v)),
        Atom::Bool(v) => Ok(Atom::I128(if v { 1 } else { 0 })),
        Atom::Enum { variant, .. } | Atom::Sum { variant, .. } => Ok(Atom::I128(variant as i128)),
        Atom::U128(v) => {
            if v <= i128::MAX as u128 {
                Ok(Atom::I128(v as i128))
            } else {
                Err(Failure::Undefined)
            }
        }
        _ => Err(Failure::Undefined),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::value_result(result) == spec::mul_div(ceiling,
        spec::atom(a), spec::atom(b), spec::atom(denominator)),
))]
pub(super) fn mul_div<'a>(
    ceiling: bool,
    a: Atom<'a>,
    b: Atom<'a>,
    denominator: Atom<'a>,
) -> Result<Atom<'a>, Failure> {
    match (a, b, denominator) {
        (Atom::U128(x), Atom::U128(y), Atom::U128(den)) => {
            let quotient = if ceiling {
                value::wide::mul_div_ceil(x, y, den)
            } else {
                value::wide::mul_div_floor(x, y, den)
            };
            match quotient {
                Some(q) => Ok(Atom::U128(q)),
                None => Err(Failure::Undefined),
            }
        }
        _ => Err(Failure::Undefined),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::value_result(result) == spec::product_compare(
        spec::atom(a), spec::atom(b), spec::atom(c),
        spec::atom(d), spec::atom(e), spec::atom(f)),
))]
pub(super) fn product_compare<'a>(
    a: Atom<'a>,
    b: Atom<'a>,
    c: Atom<'a>,
    d: Atom<'a>,
    e: Atom<'a>,
    f: Atom<'a>,
) -> Result<Atom<'a>, Failure> {
    match (a, b, c, d, e, f) {
        (
            Atom::U128(a),
            Atom::U128(b),
            Atom::U128(c),
            Atom::U128(d),
            Atom::U128(e),
            Atom::U128(f),
        ) => Ok(Atom::Bool(value::wide::products_ge(a, b, c, d, e, f))),
        _ => Err(Failure::Undefined),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures spec::value_result(result)==spec::to_u128(spec::atom(value)),))]
pub(super) fn to_u128<'a>(value: Atom<'a>) -> Result<Atom<'a>, Failure> {
    match value {
        Atom::I128(v) if v >= 0 => Ok(Atom::U128(v as u128)),
        Atom::U128(v) => Ok(Atom::U128(v)),
        _ => Err(Failure::Undefined),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::value_result(result) == spec::signed_mul_div_floor(spec::atom(a),spec::atom(b),spec::atom(d)),
))]
pub(super) fn signed_mul_div_floor<'a>(
    a: Atom<'a>,
    b: Atom<'a>,
    d: Atom<'a>,
) -> Result<Atom<'a>, Failure> {
    use super::super::super::evaluation::typed::{self, Instruction, Values};
    let operands = [a, b, d];
    match typed::node(
        Instruction::SignedMulDivFloor(0, 1, 2),
        Values::Typed(&[]),
        Values::Typed(&operands),
    ) {
        Ok(v) => Ok(v),
        Err(_) => Err(Failure::Undefined),
    }
}
