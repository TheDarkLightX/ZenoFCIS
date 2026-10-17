//! Checked scalar arithmetic and explicit nonnegative full-product operations.
//! All temporaries are owned by one invocation; only scalar values escape.
use super::wide;
#[cfg(verus_keep_ghost)]
use vstd::arithmetic::div_mod::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {
pub(crate) open spec fn magnitude(value: int) -> int { if value < 0 { -value } else { value } }
pub(crate) open spec fn narrow(value: int) -> Option<i64> {
    if i64::MIN <= value <= i64::MAX { Some(value as i64) } else { None }
}
pub(crate) open spec fn multiply_value(a: i64, b: i64) -> Option<i64> {
    narrow(a as int * b as int)
}
/// Magnitude/sign rational rounding, matching the existing law definition.
pub(crate) open spec fn division_value(up: bool, a: i64, b: i64) -> Option<i64> {
    if b == 0 { None } else {
        let negative = (a < 0) != (b < 0);
        let x = magnitude(a as int); let y = magnitude(b as int);
        let q = x / y; let r = x % y;
        let rounded = q + if r != 0 && (up != negative) { 1int } else { 0int };
        narrow(if negative { -rounded } else { rounded })
    }
}
pub(crate) open spec fn mul_div_value(up: bool, a: i64, b: i64, den: i64) -> Option<i64> {
    if a < 0 || b < 0 || den <= 0 { None } else {
        let p = a as int * b as int;
        narrow(p / den as int + if up && p % den as int != 0 { 1int } else { 0int })
    }
}
pub(crate) open spec fn products_ge_value(a:i64,b:i64,c:i64,d:i64,e:i64,f:i64) -> Option<bool> {
    if a < 0 || b < 0 || c < 0 || d < 0 || e < 0 || f < 0 { None }
    else { Some(a as int * b as int * c as int >= d as int * e as int * f as int) }
}
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == multiply_value(a, b),))]
pub(crate) fn multiply(a: i64, b: i64) -> Option<i64> {
    a.checked_mul(b)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == division_value(up, a, b),))]
pub(crate) fn divide(up: bool, a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        return None;
    }
    // i128 holds both i64 magnitudes, including MIN, without abs overflow.
    let x = if a < 0 { -(a as i128) } else { a as i128 };
    let y = if b < 0 { -(b as i128) } else { b as i128 };
    let negative = (a < 0) != (b < 0);
    let q = x / y;
    let r = x % y;
    #[cfg(verus_keep_ghost)]
    proof! {
        lemma_fundamental_div_mod(x as int, y as int);
        lemma_mod_bound(x as int, y as int);
        assert(0 <= q <= x) by(nonlinear_arith)
            requires 0<=x, y>0, 0<=r<y, x as int == q as int*y as int+r as int;
        assert(x <= 9_223_372_036_854_775_808int);
    }
    let rounded = if r != 0 && (up != negative) { q + 1 } else { q };
    let result = if negative { -rounded } else { rounded };
    if result < i64::MIN as i128 || result > i64::MAX as i128 {
        None
    } else {
        Some(result as i64)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == mul_div_value(up, a, b, den),))]
pub(crate) fn mul_div(up: bool, a: i64, b: i64, den: i64) -> Option<i64> {
    if a < 0 || b < 0 || den <= 0 {
        return None;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        let p = a as int * b as int;
        let upper = (i64::MAX as int) * (i64::MAX as int);
        vstd::arithmetic::mul::lemma_mul_upper_bound(a as int, i64::MAX as int,
            b as int, i64::MAX as int);
        assert(0 <= p) by(nonlinear_arith)
            requires 0 <= a as int, 0 <= b as int, p == (a as int) * (b as int);
        assert(p <= upper);
        assert(upper + 1 <= u128::MAX as int);
        let divisor = den as int;
        let q = p / divisor;
        let r = p % divisor;
        lemma_fundamental_div_mod(p, divisor);
        lemma_mod_bound(p, divisor);
        assert(p == q * divisor + r);
        assert(0 <= q <= p) by(nonlinear_arith)
            requires divisor > 0, 0 <= p, p == q * divisor + r, 0 <= r < divisor;
        assert(q + 1 <= u128::MAX as int);
    }
    let result = if up {
        wide::mul_div_ceil(a as u128, b as u128, den as u128)
    } else {
        wide::mul_div_floor(a as u128, b as u128, den as u128)
    };
    match result {
        Some(q) if q <= i64::MAX as u128 => Some(q as i64),
        _ => None,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == products_ge_value(a,b,c,d,e,f),))]
pub(crate) fn products_ge(a: i64, b: i64, c: i64, d: i64, e: i64, f: i64) -> Option<bool> {
    if a < 0 || b < 0 || c < 0 || d < 0 || e < 0 || f < 0 {
        return None;
    }
    Some(wide::products_ge(
        a as u128, b as u128, c as u128, d as u128, e as u128, f as u128,
    ))
}
