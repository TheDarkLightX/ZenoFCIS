//! One total numeric node evaluator with closed, data-only legacy adapters.
use super::super::value::{self, ArithmeticOp, Atom, IntegerWidth, NumericFailure};
use super::Op;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
pub(crate) mod admission;
#[cfg(verus_keep_ghost)]
#[path = "typed/spec.rs"]
pub(crate) mod spec;

/// Numeric graph instruction; operands name earlier nodes when admitted.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum NumericOp {
    /// Read an input in declared ABI order.
    Input(u16),
    /// Explicit U128 to I128 conversion.
    ToI128(u16),
    /// Explicit I128 to U128 conversion.
    ToU128(u16),
    /// Boolean literal with Boolean static provenance.
    Bool(bool),
    /// Full signed literal.
    I128(i128),
    /// Full unsigned literal.
    U128(u128),
    /// Checked addition at the declared width.
    Add(IntegerWidth, u16, u16),
    /// Checked subtraction at the declared width.
    Sub(IntegerWidth, u16, u16),
    /// Same-kind numeric or Boolean equality.
    Eq(u16, u16),
    /// Same-kind signed or unsigned ordering.
    Lt(u16, u16),
    /// Boolean conjunction.
    And(u16, u16),
    /// Boolean negation.
    Not(u16),
    /// Eager equal-kind selection with a Boolean condition.
    Select(u16, u16, u16),
    /// Exact unsigned floor of the first two operands' product divided by the third.
    /// A zero denominator or a quotient above u128::MAX refuses arithmetic.
    MulDivFloor(u16, u16, u16),
    /// Exact unsigned ceiling, including refusal when only rounding overflows.
    MulDivCeil(u16, u16, u16),
    /// Compare complete unsigned three-factor products without truncating intermediates.
    ProductsGe([u16; 3], [u16; 3]),
    /// Signed full-product floor with a positive signed divisor.
    SignedMulDivFloor(u16, u16, u16),
    /// Checked product at the declared signed or unsigned width.
    Mul(IntegerWidth, u16, u16),
    /// Checked floor division at the declared width.
    DivFloor(IntegerWidth, u16, u16),
    /// Checked ceiling division at the declared width.
    DivCeil(IntegerWidth, u16, u16),
    /// Nonnegative fused floor at the declared signed width.
    NonnegativeMulDivFloor(IntegerWidth, u16, u16, u16),
    /// Nonnegative fused ceiling at the declared signed width.
    NonnegativeMulDivCeil(IntegerWidth, u16, u16, u16),
    /// Exact nonnegative signed-factor comparison, returning a typed Boolean.
    NonnegativeProductsGe([u16; 3], [u16; 3]),
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Bool,
    Signed,
    Unsigned,
}

// Derived PartialEq remains available to ordinary clients/tests. Runtime
// decisions use these verified total equalities with the same cases.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (left == right),))]
pub(crate) fn kind_equals(left: Kind, right: Kind) -> bool {
    matches!(
        (left, right),
        (Kind::Bool, Kind::Bool) | (Kind::Signed, Kind::Signed) | (Kind::Unsigned, Kind::Unsigned)
    )
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (left == right),))]
pub(crate) fn option_kind_equals(left: Option<Kind>, right: Option<Kind>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(a), Some(b)) => kind_equals(a, b),
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BooleanEncoding {
    LegacyInteger01,
    TypedBool,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum Instruction {
    Input(u16),
    // Kind retains Op::Bool provenance even when its legacy wire atom is I128.
    Literal(Atom<'static>, Kind),
    Cast(bool, u16),
    Arithmetic(IntegerWidth, ArithmeticOp, u16, u16),
    Eq(BooleanEncoding, u16, u16),
    Lt(BooleanEncoding, u16, u16),
    And(BooleanEncoding, u16, u16),
    Not(BooleanEncoding, u16),
    Select(BooleanEncoding, u16, u16, u16),
    MulDiv(bool, u16, u16, u16),
    ProductsGe([u16; 3], [u16; 3]),
    ScalarMul(IntegerWidth, u16, u16),
    ScalarDiv(IntegerWidth, bool, u16, u16),
    ScalarMulDiv(IntegerWidth, bool, u16, u16, u16),
    ScalarProductsGe(BooleanEncoding, [u16; 3], [u16; 3]),
    SignedMulDivFloor(u16, u16, u16),
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Failure {
    Reference,
    Kind,
    Arithmetic,
}

/// Borrowed input/scratch adapter. Lifting reads no other position and allocates nothing.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy)]
pub(crate) enum Values<'s, 'a> {
    Legacy(&'s [i128]),
    Typed(&'s [Atom<'a>]),
}

impl<'s, 'a> Values<'s, 'a> {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result as nat == self.view().len(),))]
    pub(crate) fn len(self) -> usize {
        match self {
            Self::Legacy(v) => v.len(),
            Self::Typed(v) => v.len(),
        }
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::at(self.view(), index as nat),))]
    pub(crate) fn get(self, index: usize) -> Option<Atom<'a>> {
        match self {
            Self::Legacy(v) => {
                if index < v.len() {
                    Some(Atom::I128(v[index]))
                } else {
                    None
                }
            }
            Self::Typed(v) => {
                if index < v.len() {
                    Some(v[index])
                } else {
                    None
                }
            }
        }
    }
}

#[inline(always)]
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::legacy_instruction(*op),))]
pub(crate) fn legacy_instruction(op: &Op) -> Instruction {
    use BooleanEncoding::LegacyInteger01 as L;
    match *op {
        Op::Input(i) => Instruction::Input(i),
        Op::Int(v) => Instruction::Literal(Atom::I128(v), Kind::Signed),
        Op::Bool(v) => Instruction::Literal(Atom::I128(if v { 1 } else { 0 }), Kind::Bool),
        Op::Add(a, b) => Instruction::Arithmetic(IntegerWidth::Signed128, ArithmeticOp::Add, a, b),
        Op::Sub(a, b) => Instruction::Arithmetic(IntegerWidth::Signed128, ArithmeticOp::Sub, a, b),
        Op::Eq(a, b) => Instruction::Eq(L, a, b),
        Op::Lt(a, b) => Instruction::Lt(L, a, b),
        Op::And(a, b) => Instruction::And(L, a, b),
        Op::Not(a) => Instruction::Not(L, a),
        Op::Select(c, a, b) => Instruction::Select(L, c, a, b),
        Op::Mul(a, b) => Instruction::ScalarMul(IntegerWidth::Signed128, a, b),
        Op::DivFloor(a, b) => Instruction::ScalarDiv(IntegerWidth::Signed128, false, a, b),
        Op::DivCeil(a, b) => Instruction::ScalarDiv(IntegerWidth::Signed128, true, a, b),
        Op::MulDivFloor(a, b, d) => {
            Instruction::ScalarMulDiv(IntegerWidth::Signed128, false, a, b, d)
        }
        Op::MulDivCeil(a, b, d) => {
            Instruction::ScalarMulDiv(IntegerWidth::Signed128, true, a, b, d)
        }
        Op::ProductsGe(a, b) => Instruction::ScalarProductsGe(L, a, b),
        Op::SignedMulDivFloor(a, b, d) => Instruction::SignedMulDivFloor(a, b, d),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::typed_instruction(op),))]
pub(crate) fn typed_instruction(op: NumericOp) -> Instruction {
    use BooleanEncoding::TypedBool as T;
    match op {
        NumericOp::Input(i) => Instruction::Input(i),
        NumericOp::ToI128(i) => Instruction::Cast(true, i),
        NumericOp::ToU128(i) => Instruction::Cast(false, i),
        NumericOp::I128(v) => Instruction::Literal(Atom::I128(v), Kind::Signed),
        NumericOp::U128(v) => Instruction::Literal(Atom::U128(v), Kind::Unsigned),
        NumericOp::Bool(v) => Instruction::Literal(Atom::Bool(v), Kind::Bool),
        NumericOp::Add(w, a, b) => Instruction::Arithmetic(w, ArithmeticOp::Add, a, b),
        NumericOp::Sub(w, a, b) => Instruction::Arithmetic(w, ArithmeticOp::Sub, a, b),
        NumericOp::Eq(a, b) => Instruction::Eq(T, a, b),
        NumericOp::Lt(a, b) => Instruction::Lt(T, a, b),
        NumericOp::And(a, b) => Instruction::And(T, a, b),
        NumericOp::Not(a) => Instruction::Not(T, a),
        NumericOp::Select(c, a, b) => Instruction::Select(T, c, a, b),
        NumericOp::MulDivFloor(a, b, den) => Instruction::MulDiv(false, a, b, den),
        NumericOp::MulDivCeil(a, b, den) => Instruction::MulDiv(true, a, b, den),
        NumericOp::ProductsGe(left, right) => Instruction::ProductsGe(left, right),
        NumericOp::SignedMulDivFloor(a, b, d) => Instruction::SignedMulDivFloor(a, b, d),
        NumericOp::Mul(w, a, b) => Instruction::ScalarMul(w, a, b),
        NumericOp::DivFloor(w, a, b) => Instruction::ScalarDiv(w, false, a, b),
        NumericOp::DivCeil(w, a, b) => Instruction::ScalarDiv(w, true, a, b),
        NumericOp::NonnegativeMulDivFloor(w, a, b, d) => {
            Instruction::ScalarMulDiv(w, false, a, b, d)
        }
        NumericOp::NonnegativeMulDivCeil(w, a, b, d) => Instruction::ScalarMulDiv(w, true, a, b, d),
        NumericOp::NonnegativeProductsGe(a, b) => Instruction::ScalarProductsGe(T, a, b),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::kind(atom),))]
pub(crate) fn kind(atom: Atom<'_>) -> Option<Kind> {
    match atom {
        Atom::Bool(_) => Some(Kind::Bool),
        Atom::I128(_) => Some(Kind::Signed),
        Atom::U128(_) => Some(Kind::Unsigned),
        _ => None,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::boolean(encoding, value),))]
fn boolean(encoding: BooleanEncoding, value: bool) -> Atom<'static> {
    match encoding {
        BooleanEncoding::LegacyInteger01 => Atom::I128(if value { 1 } else { 0 }),
        BooleanEncoding::TypedBool => Atom::Bool(value),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::project(value),))]
pub(crate) fn project(value: Atom<'_>) -> Option<i128> {
    match value {
        Atom::I128(v) => Some(v),
        _ => None,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::project_width(width, value),))]
fn project_width(width: IntegerWidth, value: Atom<'_>) -> Option<i128> {
    match value {
        Atom::I128(v)
            if matches!(width, IntegerWidth::Signed128)
                || (matches!(width, IntegerWidth::Signed64)
                    && v >= i64::MIN as i128
                    && v <= i64::MAX as i128) =>
        {
            Some(v)
        }
        _ => None,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::width_result(width, value),))]
fn width_result(width: IntegerWidth, value: Option<i128>) -> Result<Atom<'static>, Failure> {
    match value {
        Some(v) => match project_width(width, Atom::I128(v)) {
            Some(v) => Ok(Atom::I128(v)),
            None => Err(Failure::Arithmetic),
        },
        None => Err(Failure::Arithmetic),
    }
}

/// All references are resolved before kind/range checks, including both Select arms.
// Keep the closed adapters visible to the caller's optimizer. In particular,
// the metered driver always supplies Typed scratch; no runtime mode test is needed.
#[inline(always)]
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::node(op,input.view(),values.view()),))]
pub(crate) fn node<'a>(
    op: Instruction,
    input: Values<'_, 'a>,
    values: Values<'_, 'a>,
) -> Result<Atom<'a>, Failure> {
    match op {
        Instruction::Input(i) => match input.get(i as usize) {
            None => Err(Failure::Reference),
            Some(v) => {
                if kind(v).is_some() {
                    Ok(v)
                } else {
                    Err(Failure::Kind)
                }
            }
        },
        Instruction::Literal(v, _) => Ok(v),
        Instruction::Cast(signed, a) => {
            let Some(v) = values.get(a as usize) else {
                return Err(Failure::Reference);
            };
            match (signed, v) {
                (true, Atom::U128(v)) => {
                    if v <= i128::MAX as u128 {
                        Ok(Atom::I128(v as i128))
                    } else {
                        Err(Failure::Arithmetic)
                    }
                }
                (false, Atom::I128(v)) => {
                    if v >= 0 {
                        Ok(Atom::U128(v as u128))
                    } else {
                        Err(Failure::Arithmetic)
                    }
                }
                _ => Err(Failure::Kind),
            }
        }
        Instruction::ScalarMul(w, a, b) | Instruction::ScalarDiv(w, _, a, b) => {
            let (Some(a), Some(b)) = (values.get(a as usize), values.get(b as usize)) else {
                return Err(Failure::Reference);
            };
            if matches!(w, IntegerWidth::Unsigned128)
                && matches!((a, b), (Atom::U128(_), Atom::U128(_)))
            {
                return match (a, b) {
                    (Atom::U128(a), Atom::U128(b)) => {
                        let result = match op {
                            Instruction::ScalarMul(..) => a.checked_mul(b),
                            Instruction::ScalarDiv(_, up, _, _) => {
                                if let Some(q) = a.checked_div(b) {
                                    if up && a % b != 0 {
                                        q.checked_add(1)
                                    } else {
                                        Some(q)
                                    }
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };
                        match result {
                            Some(value) => Ok(Atom::U128(value)),
                            None => Err(Failure::Arithmetic),
                        }
                    }
                    _ => Err(Failure::Kind),
                };
            }
            if !matches!((a, b), (Atom::I128(_), Atom::I128(_))) {
                return Err(Failure::Kind);
            }
            let (Some(a), Some(b)) = (project_width(w, a), project_width(w, b)) else {
                return Err(Failure::Arithmetic);
            };
            let result = match op {
                Instruction::ScalarMul(..) => value::scalar::multiply(a, b),
                Instruction::ScalarDiv(_, up, _, _) => value::scalar::divide(up, a, b),
                _ => None,
            };
            width_result(w, result)
        }
        Instruction::ScalarMulDiv(w, up, a, b, d) => {
            let (Some(a), Some(b), Some(d)) = (
                values.get(a as usize),
                values.get(b as usize),
                values.get(d as usize),
            ) else {
                return Err(Failure::Reference);
            };
            if !matches!((a, b, d), (Atom::I128(_), Atom::I128(_), Atom::I128(_))) {
                return Err(Failure::Kind);
            }
            let (Some(a), Some(b), Some(d)) = (
                project_width(w, a),
                project_width(w, b),
                project_width(w, d),
            ) else {
                return Err(Failure::Arithmetic);
            };
            width_result(w, value::scalar::mul_div(up, a, b, d))
        }
        Instruction::ScalarProductsGe(encoding, a, b) => {
            let (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f)) = (
                values.get(a[0] as usize),
                values.get(a[1] as usize),
                values.get(a[2] as usize),
                values.get(b[0] as usize),
                values.get(b[1] as usize),
                values.get(b[2] as usize),
            ) else {
                return Err(Failure::Reference);
            };
            let (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f)) = (
                project(a),
                project(b),
                project(c),
                project(d),
                project(e),
                project(f),
            ) else {
                return Err(Failure::Kind);
            };
            match value::scalar::products_ge(a, b, c, d, e, f) {
                Some(v) => Ok(boolean(encoding, v)),
                None => Err(Failure::Arithmetic),
            }
        }
        Instruction::SignedMulDivFloor(a, b, d) => {
            let (Some(a), Some(b), Some(d)) = (
                values.get(a as usize),
                values.get(b as usize),
                values.get(d as usize),
            ) else {
                return Err(Failure::Reference);
            };
            match (a, b, d) {
                (Atom::I128(a), Atom::I128(b), Atom::I128(d)) => {
                    match value::scalar::signed_mul_div_floor(a, b, d) {
                        Some(v) => Ok(Atom::I128(v)),
                        None => Err(Failure::Arithmetic),
                    }
                }
                _ => Err(Failure::Kind),
            }
        }
        Instruction::MulDiv(up, a, b, den) => {
            let (Some(a), Some(b), Some(den)) = (
                values.get(a as usize),
                values.get(b as usize),
                values.get(den as usize),
            ) else {
                return Err(Failure::Reference);
            };
            match (a, b, den) {
                (Atom::U128(a), Atom::U128(b), Atom::U128(den)) => {
                    let result = if up {
                        value::wide::mul_div_ceil(a, b, den)
                    } else {
                        value::wide::mul_div_floor(a, b, den)
                    };
                    match result {
                        Some(v) => Ok(Atom::U128(v)),
                        None => Err(Failure::Arithmetic),
                    }
                }
                _ => Err(Failure::Kind),
            }
        }
        Instruction::ProductsGe(left, right) => {
            let (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f)) = (
                values.get(left[0] as usize),
                values.get(left[1] as usize),
                values.get(left[2] as usize),
                values.get(right[0] as usize),
                values.get(right[1] as usize),
                values.get(right[2] as usize),
            ) else {
                return Err(Failure::Reference);
            };
            match (a, b, c, d, e, f) {
                (
                    Atom::U128(a),
                    Atom::U128(b),
                    Atom::U128(c),
                    Atom::U128(d),
                    Atom::U128(e),
                    Atom::U128(f),
                ) => Ok(Atom::Bool(value::wide::products_ge(a, b, c, d, e, f))),
                _ => Err(Failure::Kind),
            }
        }
        Instruction::Not(encoding, a) => {
            let Some(v) = values.get(a as usize) else {
                return Err(Failure::Reference);
            };
            match (encoding, v) {
                (BooleanEncoding::LegacyInteger01, Atom::I128(v)) => Ok(boolean(encoding, v == 0)),
                (BooleanEncoding::TypedBool, Atom::Bool(v)) => Ok(boolean(encoding, !v)),
                _ => Err(Failure::Kind),
            }
        }
        Instruction::Select(encoding, c, a, b) => {
            let (Some(c), Some(a), Some(b)) = (
                values.get(c as usize),
                values.get(a as usize),
                values.get(b as usize),
            ) else {
                return Err(Failure::Reference);
            };
            match (encoding, c, a, b) {
                (BooleanEncoding::LegacyInteger01, Atom::I128(c), Atom::I128(a), Atom::I128(b)) => {
                    Ok(Atom::I128(if c == 1 { a } else { b }))
                }
                (BooleanEncoding::TypedBool, Atom::Bool(c), a, b) => {
                    if kind(a).is_some() && option_kind_equals(kind(a), kind(b)) {
                        Ok(if c { a } else { b })
                    } else {
                        Err(Failure::Kind)
                    }
                }
                _ => Err(Failure::Kind),
            }
        }
        Instruction::Arithmetic(_, _, a, b)
        | Instruction::Eq(_, a, b)
        | Instruction::Lt(_, a, b)
        | Instruction::And(_, a, b) => {
            let (Some(left), Some(right)) = (values.get(a as usize), values.get(b as usize)) else {
                return Err(Failure::Reference);
            };
            match op {
                Instruction::Arithmetic(width, operation, _, _) => {
                    match value::arithmetic(width, operation, left, right) {
                        Ok(v) => Ok(v),
                        Err(NumericFailure::Kind) => Err(Failure::Kind),
                        Err(NumericFailure::Range) => Err(Failure::Arithmetic),
                    }
                }
                Instruction::Eq(encoding, _, _) => match (left, right) {
                    (Atom::I128(a), Atom::I128(b)) => Ok(boolean(encoding, a == b)),
                    (Atom::U128(a), Atom::U128(b)) => Ok(boolean(encoding, a == b)),
                    (Atom::Bool(a), Atom::Bool(b)) => Ok(boolean(encoding, a == b)),
                    _ => Err(Failure::Kind),
                },
                Instruction::Lt(encoding, _, _) => match (left, right) {
                    (Atom::I128(a), Atom::I128(b)) => Ok(boolean(encoding, a < b)),
                    (Atom::U128(a), Atom::U128(b)) => Ok(boolean(encoding, a < b)),
                    _ => Err(Failure::Kind),
                },
                Instruction::And(encoding, _, _) => match (encoding, left, right) {
                    (BooleanEncoding::LegacyInteger01, Atom::I128(a), Atom::I128(b)) => {
                        Ok(boolean(encoding, a == 1 && b == 1))
                    }
                    (BooleanEncoding::TypedBool, Atom::Bool(a), Atom::Bool(b)) => {
                        Ok(boolean(encoding, a && b))
                    }
                    _ => Err(Failure::Kind),
                },
                _ => Err(Failure::Kind),
            }
        }
    }
}
