//! Checked signed scalar arithmetic. Unsigned magnitudes hold MIN exactly.
//! Invocation-local scalars only; the existing unsigned limb engine owns wide products.
use super::wide;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {
pub(crate) open spec fn magnitude(value: int) -> int { if value < 0 { -value } else { value } }
pub(crate) open spec fn narrow(value: int) -> Option<i128> {
    if i128::MIN <= value <= i128::MAX { Some(value as i128) } else { None }
}
pub(crate) open spec fn multiply_value(a: i128, b: i128) -> Option<i128> {
    narrow(a as int * b as int)
}
pub(crate) open spec fn division_value(up: bool, a: i128, b: i128) -> Option<i128> {
    if b == 0 { None } else {
        let negative = (a < 0) != (b < 0);
        let x = magnitude(a as int); let y = magnitude(b as int);
        let q = x / y; let r = x % y;
        let rounded = q + if r != 0 && (up != negative) { 1int } else { 0int };
        narrow(if negative { -rounded } else { rounded })
    }
}
pub(crate) open spec fn mul_div_value(up: bool, a: i128, b: i128, den: i128) -> Option<i128> {
    if a < 0 || b < 0 || den <= 0 { None } else {
        let p = a as int * b as int;
        narrow(p / den as int + if up && p % den as int != 0 { 1int } else { 0int })
    }
}
/// Total D7 relation: mathematical integer division by a positive divisor is floor.
pub(crate) open spec fn signed_mul_div_floor_value(a:i128,b:i128,den:i128)->Option<i128> {
    if den <= 0 { None } else { narrow((a as int * b as int) / den as int) }
}
/// Exact success/refusal partition, stated over every raw machine tuple.
pub(crate) open spec fn signed_floor_relation(a:i128,b:i128,d:i128,result:Option<i128>)->bool {
    result == signed_mul_div_floor_value(a,b,d)
    && match result {
        Some(q) => d>0 && q as int*d as int <= a as int*b as int
            && (a as int*b as int) < (q as int+1)*(d as int),
        None => if d<=0 {true}else{!((i128::MIN as int) <= (a as int*b as int)/d as int <= (i128::MAX as int))},
    }
}
pub(crate) open spec fn products_ge_value(a:i128,b:i128,c:i128,d:i128,e:i128,f:i128) -> Option<bool> {
    if a < 0 || b < 0 || c < 0 || d < 0 || e < 0 || f < 0 { None }
    else { Some(a as int * b as int * c as int >= d as int * e as int * f as int) }
}
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result as int == magnitude(value as int),))]
pub(crate) fn unsigned_magnitude(value: i128) -> u128 {
    if value < 0 {
        (-(value + 1)) as u128 + 1
    } else {
        value as u128
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == narrow(if negative { -(value as int) } else { value as int }),
))]
fn signed_magnitude(value: u128, negative: bool) -> Option<i128> {
    if negative && value == i128::MAX as u128 + 1 {
        Some(i128::MIN)
    } else if value > i128::MAX as u128 {
        None
    } else if negative {
        Some(-(value as i128))
    } else {
        Some(value as i128)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == multiply_value(a, b),))]
pub(crate) fn multiply(a: i128, b: i128) -> Option<i128> {
    a.checked_mul(b)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == division_value(up, a, b),))]
pub(crate) fn divide(up: bool, a: i128, b: i128) -> Option<i128> {
    if b == 0 {
        return None;
    }
    let x = unsigned_magnitude(a);
    let y = unsigned_magnitude(b);
    let negative = (a < 0) != (b < 0);
    let q = x / y;
    let r = x % y;
    let rounded = if r != 0 && (up != negative) {
        q.checked_add(1)?
    } else {
        q
    };
    signed_magnitude(rounded, negative)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == mul_div_value(up, a, b, den),))]
pub(crate) fn mul_div(up: bool, a: i128, b: i128, den: i128) -> Option<i128> {
    if a < 0 || b < 0 || den <= 0 {
        return None;
    }
    let q = if up {
        wide::mul_div_ceil(a as u128, b as u128, den as u128)
    } else {
        wide::mul_div_floor(a as u128, b as u128, den as u128)
    }?;
    signed_magnitude(q, false)
}

/// Floor of the exact signed two-factor product, without narrowing the product.
/// Nonpositive divisors refuse even for zero numerators. All helper/fit failures refuse.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == signed_mul_div_floor_value(a, b, den),
        signed_floor_relation(a,b,den,result),
))]
pub(crate) fn signed_mul_div_floor(a: i128, b: i128, den: i128) -> Option<i128> {
    if den <= 0 {
        return None;
    }
    let negative = (a < 0) != (b < 0);
    let x = unsigned_magnitude(a);
    let y = unsigned_magnitude(b);
    #[cfg(verus_keep_ghost)]
    proof! {
        let p = a as int * b as int;
        let m = x as int * y as int;
        let d = den as int;
        assert(m == if negative { -p } else { p }) by(nonlinear_arith)
            requires
                p == a as int * b as int,
                m == x as int * y as int,
                negative == ((a < 0) != (b < 0)),
                x as int == if a < 0 { -(a as int) } else { a as int },
                y as int == if b < 0 { -(b as int) } else { b as int };
        vstd::arithmetic::div_mod::lemma_fundamental_div_mod(m, d);
        vstd::arithmetic::div_mod::lemma_mod_bound(m, d);
        let q = m / d;
        let r = m % d;
        let rounded = q + if negative && r != 0 { 1int } else { 0int };
        let signed = if negative { -rounded } else { rounded };
        let remainder = if negative && r != 0 { d - r } else { r };
        assert(m >= 0) by(nonlinear_arith)
            requires m == x as int * y as int, x >= 0, y >= 0;
        assert(q >= 0) by(nonlinear_arith)
            requires m >= 0, d > 0, m == d * q + r, 0 <= r < d;
        assert(p == signed * d + remainder) by(nonlinear_arith)
            requires
                m == if negative { -p } else { p },
                m == d * q + r,
                rounded == q + if negative && r != 0 { 1int } else { 0int },
                signed == if negative { -rounded } else { rounded },
                remainder == if negative && r != 0 { d - r } else { r };
        assert(0 <= remainder < d);
        vstd::arithmetic::div_mod::lemma_fundamental_div_mod_converse_div(
            p, d, signed, remainder);
        assert(signed * d <= p < (signed + 1) * d) by(nonlinear_arith)
            requires p == signed * d + remainder, 0 <= remainder < d, d > 0;
        // An unsigned helper overflow cannot hide a fitting signed quotient.
        if rounded > u128::MAX {
            assert(narrow(signed) == None::<i128>);
        }
        assert(signed_mul_div_floor_value(a, b, den) ==
            match if negative { wide::ceiling_value(x, y, den as u128) }
                else { wide::floor_value(x, y, den as u128) } {
                Some(v) => narrow(if negative { -(v as int) } else { v as int }),
                None => None::<i128>,
            });
        assert(signed_floor_relation(a, b, den, signed_mul_div_floor_value(a, b, den)));
    }
    let q = if negative {
        // D7_NEGATIVE_ROUNDING: floor(-P/d) = -ceil(P/d).
        wide::mul_div_ceil(x, y, den as u128)
    } else {
        wide::mul_div_floor(x, y, den as u128)
    }?;
    signed_magnitude(q, negative)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == products_ge_value(a,b,c,d,e,f),))]
pub(crate) fn products_ge(a: i128, b: i128, c: i128, d: i128, e: i128, f: i128) -> Option<bool> {
    if a < 0 || b < 0 || c < 0 || d < 0 || e < 0 || f < 0 {
        return None;
    }
    Some(wide::products_ge(
        a as u128, b as u128, c as u128, d as u128, e as u128, f as u128,
    ))
}
