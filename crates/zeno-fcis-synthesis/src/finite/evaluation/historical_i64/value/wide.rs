//! Exact fixed-width intermediates for generic unsigned numeric instructions.
//! No allocation, callbacks or observable state; all input operands remain full u128.
#[cfg(verus_keep_ghost)]
use vstd::arithmetic::{div_mod::*, mul::*, power2::*};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec)]
const RADIX: u128 = 18_446_744_073_709_551_616;

#[cfg(verus_keep_ghost)]
verus! {
pub(crate) open spec fn scale() -> int { u128::MAX as int + 1 }
pub(crate) open spec fn number2(v: [u128; 2]) -> int {
    v[1] as int * scale() + v[0] as int
}
pub(crate) open spec fn number3(v: [u128; 3]) -> int {
    v[2] as int * scale() * scale() + v[1] as int * scale() + v[0] as int
}
pub(crate) open spec fn floor_value(a: u128, b: u128, den: u128) -> Option<u128> {
    if den == 0 { None }
    else {
        let q = (a as int * b as int) / den as int;
        if q > u128::MAX { None } else { Some(q as u128) }
    }
}
pub(crate) open spec fn ceiling_value(a: u128, b: u128, den: u128) -> Option<u128> {
    if den == 0 { None }
    else {
        let p = a as int * b as int;
        let q = p / den as int + if p % den as int == 0 { 0int } else { 1int };
        if q > u128::MAX { None } else { Some(q as u128) }
    }
}
proof fn split64(v: u128)
    ensures
        v as int == (v / RADIX) as int * RADIX as int + (v % RADIX) as int,
        v / RADIX < RADIX, v % RADIX < RADIX,
{
    lemma_fundamental_div_mod(v as int, RADIX as int);
    lemma_mod_bound(v as int, RADIX as int);
}
proof fn quotient_from_interval(p: int, den: int, q: int)
    requires den > 0, 0 <= q, q * den <= p < (q + 1) * den,
    ensures p / den == q,
{
    assert(0 <= p - q * den < den) by(nonlinear_arith)
        requires den > 0, 0 <= q, q * den <= p, p < (q + 1) * den;
    lemma_fundamental_div_mod_converse_div(p, den, q, p - q * den);
}
}

/// Exact two-limb product, in least-significant-first order.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures number2(result) == a as int * b as int,
))]
pub(crate) fn multiply(a: u128, b: u128) -> [u128; 2] {
    let a0 = a % RADIX;
    let a1 = a / RADIX;
    let b0 = b % RADIX;
    let b1 = b / RADIX;
    #[cfg(verus_keep_ghost)]
    proof! {
        split64(a); split64(b);
        lemma_mul_upper_bound(a0 as int, RADIX as int - 1, b0 as int, RADIX as int - 1);
        lemma_mul_upper_bound(a0 as int, RADIX as int - 1, b1 as int, RADIX as int - 1);
        lemma_mul_upper_bound(a1 as int, RADIX as int - 1, b0 as int, RADIX as int - 1);
        lemma_mul_upper_bound(a1 as int, RADIX as int - 1, b1 as int, RADIX as int - 1);
    }
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    #[cfg(verus_keep_ghost)]
    proof! {
        lemma_div_is_ordered(p00 as int, (RADIX as int - 1) * (RADIX as int - 1), RADIX as int);
        lemma_div_is_ordered(p01 as int, (RADIX as int - 1) * (RADIX as int - 1), RADIX as int);
        lemma_div_is_ordered(p10 as int, (RADIX as int - 1) * (RADIX as int - 1), RADIX as int);
        assert(p00 / RADIX <= RADIX - 2);
        assert(p01 / RADIX <= RADIX - 2);
        assert(p10 / RADIX <= RADIX - 2);
    }
    let middle = p00 / RADIX + p01 % RADIX + p10 % RADIX;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(middle <= 3 * RADIX - 4);
        lemma_div_is_ordered(middle as int, 3 * RADIX as int - 4, RADIX as int);
        assert(middle / RADIX <= 2);
        assert(p11 as int + (p01 / RADIX) as int + (p10 / RADIX) as int
            + (middle / RADIX) as int <= u128::MAX as int);
        assert((p00 % RADIX) as int + (middle % RADIX) as int * RADIX as int <= u128::MAX as int) by(nonlinear_arith);
    }
    let high = p11 + p01 / RADIX + p10 / RADIX + middle / RADIX;
    let low = p00 % RADIX + (middle % RADIX) * RADIX;
    #[cfg(verus_keep_ghost)]
    proof! {
        lemma_fundamental_div_mod(p00 as int, RADIX as int);
        lemma_fundamental_div_mod(p01 as int, RADIX as int);
        lemma_fundamental_div_mod(p10 as int, RADIX as int);
        lemma_fundamental_div_mod(middle as int, RADIX as int);
        assert(number2([low, high]) == a as int * b as int) by(nonlinear_arith)
            requires
                a as int == a1 as int * RADIX as int + a0 as int,
                b as int == b1 as int * RADIX as int + b0 as int,
                p00 as int == a0 as int * b0 as int,
                p01 as int == a0 as int * b1 as int,
                p10 as int == a1 as int * b0 as int,
                p11 as int == a1 as int * b1 as int,
                p00 as int == (p00 / RADIX) as int * RADIX as int + (p00 % RADIX) as int,
                p01 as int == (p01 / RADIX) as int * RADIX as int + (p01 % RADIX) as int,
                p10 as int == (p10 / RADIX) as int * RADIX as int + (p10 % RADIX) as int,
                middle as int == (p00 / RADIX) as int + (p01 % RADIX) as int + (p10 % RADIX) as int,
                middle as int == (middle / RADIX) as int * RADIX as int + (middle % RADIX) as int,
                high as int == p11 as int + (p01 / RADIX) as int + (p10 / RADIX) as int + (middle / RADIX) as int,
                low as int == (p00 % RADIX) as int + (middle % RADIX) as int * RADIX as int,
                RADIX as int * RADIX as int == scale(),
                number2([low, high]) == high as int * scale() + low as int;
    }
    [low, high]
}

/// Exact three-factor product, preserving every bit of a possible 384-bit value.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures number3(result) == a as int * b as int * c as int,
))]
pub(crate) fn multiply_three(a: u128, b: u128, c: u128) -> [u128; 3] {
    let first = multiply(a, b);
    let low = multiply(first[0], c);
    let high = multiply(first[1], c);
    let room = u128::MAX - high[0];
    let (middle, carry) = if low[1] > room {
        (low[1] - room - 1, 1u128)
    } else {
        (low[1] + high[0], 0u128)
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        lemma_mul_upper_bound(first[1] as int, u128::MAX as int, c as int, u128::MAX as int);
        assert(high[1] <= u128::MAX - 1) by(nonlinear_arith)
            requires
                number2(high) == first[1] as int * c as int,
                number2(high) == high[1] as int * scale() + high[0] as int,
                first[1] as int * c as int <= u128::MAX as int * u128::MAX as int,
                scale() == u128::MAX as int + 1,
                0 <= high[0] as int, 0 <= high[1] as int;
        assert(middle as int + carry as int * scale() == low[1] as int + high[0] as int);
    }
    let top = high[1] + carry;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(number3([low[0], middle, top]) == a as int * b as int * c as int) by(nonlinear_arith)
            requires
                number2(first) == a as int * b as int,
                number2(first) == first[1] as int * scale() + first[0] as int,
                number2(low) == first[0] as int * c as int,
                number2(low) == low[1] as int * scale() + low[0] as int,
                number2(high) == first[1] as int * c as int,
                number2(high) == high[1] as int * scale() + high[0] as int,
                middle as int + carry as int * scale() == low[1] as int + high[0] as int,
                top as int == high[1] as int + carry as int,
                number3([low[0], middle, top]) == top as int * scale() * scale() + middle as int * scale() + low[0] as int;
    }
    [low[0], middle, top]
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (number2(left) <= number2(right)),
))]
fn less_equal(left: [u128; 2], right: [u128; 2]) -> bool {
    if left[1] == right[1] {
        left[0] <= right[0]
    } else {
        left[1] < right[1]
    }
}

/// Exact unsigned three-factor comparison; no intermediate overflow refusal.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (a as int * b as int * c as int >= d as int * e as int * f as int),
))]
pub(crate) fn products_ge(a: u128, b: u128, c: u128, d: u128, e: u128, f: u128) -> bool {
    let left = multiply_three(a, b, c);
    let right = multiply_three(d, e, f);
    if left[2] != right[2] {
        left[2] > right[2]
    } else if left[1] != right[1] {
        left[1] > right[1]
    } else {
        left[0] >= right[0]
    }
}

/// Floor(a*b/den), with a fixed128-round search and exact quotient overflow refusal.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == floor_value(a, b, den),
))]
pub(crate) fn mul_div_floor(a: u128, b: u128, den: u128) -> Option<u128> {
    if den == 0 {
        return None;
    }
    let product = multiply(a, b);
    if product[1] >= den {
        #[cfg(verus_keep_ghost)]
        proof! {
            lemma_div_is_ordered(den as int * scale(), number2(product), den as int);
            lemma_div_multiples_vanish(scale(), den as int);
        }
        return None;
    }
    let mut quotient = 0u128;
    let mut bit = 170_141_183_460_469_231_731_687_303_715_884_105_728u128;
    let mut remaining = 128u16;
    #[cfg(verus_keep_ghost)]
    proof! {
        lemma2_to64(); lemma2_to64_rest();
        lemma_pow2_adds(63, 64); lemma_pow2_adds(64, 64);
        assert(pow2(127) == bit);
        assert(pow2(128) == scale());
        assert(number2(product) < den as int * scale());
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            den > 0, remaining <= 128,
            number2(product) == a as int * b as int,
            quotient as int * den as int <= number2(product),
            remaining > 0 ==> bit as int == pow2((remaining - 1) as nat),
            remaining > 0 ==> quotient as int + 2 * bit as int <= scale(),
            remaining > 0 ==> number2(product) < (quotient as int + 2 * bit as int) * den as int,
            remaining == 0 ==> number2(product) < (quotient as int + 1) * den as int,
        decreases remaining,
    ))]
    while remaining > 0 {
        #[cfg(verus_keep_ghost)]
        proof! { lemma_pow2_pos((remaining - 1) as nat); }
        let trial = quotient + bit;
        if less_equal(multiply(trial, den), product) {
            quotient = trial;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(number2(product) < (quotient as int + bit as int) * den as int);
            assert(quotient as int + bit as int <= scale());
            if remaining > 1 { lemma_pow2_unfold((remaining - 1) as nat); }
            else { lemma2_to64(); assert(bit == 1); }
        }
        remaining -= 1;
        if remaining > 0 {
            bit /= 2;
        }
    }
    #[cfg(verus_keep_ghost)]
    proof! { quotient_from_interval(number2(product), den as int, quotient as int); }
    Some(quotient)
}

/// Ceiling(a*b/den), including the distinct final-rounding overflow boundary.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == ceiling_value(a, b, den),
))]
pub(crate) fn mul_div_ceil(a: u128, b: u128, den: u128) -> Option<u128> {
    let quotient = mul_div_floor(a, b, den)?;
    let product = multiply(a, b);
    let floor_product = multiply(quotient, den);
    #[cfg(verus_keep_ghost)]
    proof! {
        lemma_fundamental_div_mod(number2(product), den as int);
        lemma_mod_bound(number2(product), den as int);
        assert(number2(floor_product) == number2(product) / den as int * den as int);
    }
    if product[0] == floor_product[0] && product[1] == floor_product[1] {
        Some(quotient)
    } else {
        quotient.checked_add(1)
    }
}
