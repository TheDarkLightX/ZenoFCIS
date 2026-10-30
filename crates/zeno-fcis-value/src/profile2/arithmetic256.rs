//! PRIVATE arithmetic candidate. No codec, authority, usage or publication API.
//! Fixed limbs own their values; all mutable arrays are invocation-local scratch.
use core::cmp::Ordering;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "numeric256_model.rs"]
pub(crate) mod numeric_model;

#[cfg(test)]
#[allow(dead_code)]
#[path = "reference/legacy_scalar.rs"]
mod legacy_scalar;
#[allow(dead_code)]
#[path = "reference/legacy_wide.rs"]
mod wide;

/// A checked fixed-width arithmetic refusal.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// The result does not fit the requested integer width or signed range.
    Overflow,
    /// The divisor is zero.
    ZeroDivisor,
    /// Exact rounding was requested for a nonintegral result.
    Inexact,
    /// An operand is outside the operation's permitted domain.
    Domain,
    /// An internal arithmetic consistency check failed.
    Invariant,
}
/// The rounding policy for a checked arithmetic operation.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rounding {
    /// Round toward negative infinity.
    Floor,
    /// Round toward positive infinity.
    Ceil,
    /// Refuse a result that requires rounding.
    Exact,
}

#[path = "number256.rs"]
mod number256;
pub use number256::{I256, U256};
/// An owned unsigned 512-bit integer with private least significant word order.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct U512([u128; 4]);
/// An owned unsigned 768-bit integer with private least significant word order.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct U768([u128; 6]);

#[path = "ordering256.rs"]
mod ordering256;
use ordering256::compare;
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures N == 2 ==> numeric_model::prefix2(result.0@, 2)
        + numeric_model::modulus() * (if result.1 { 1int } else { 0int })
        == numeric_model::prefix2(a@, 2) + numeric_model::prefix2(b@, 2),
        numeric_model::value(result.0@)
            + numeric_model::place(N as nat) * (if result.1 { 1int } else { 0int })
            == numeric_model::value(a@) + numeric_model::value(b@),
))]
fn add<const N: usize>(a: [u128; N], b: [u128; N]) -> ([u128; N], bool) {
    let mut out = [0; N];
    let mut carry = false;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant N == 2 ==> numeric_model::prefix2(out@, i as int)
            + numeric_model::place2(i as int) * (if carry { 1int } else { 0int })
            == numeric_model::prefix2(a@, i as int) + numeric_model::prefix2(b@, i as int),
            out@.len() == N, a@.len() == N, b@.len() == N,
            numeric_model::limbs(out@, i as nat)
                + numeric_model::place(i as nat) * (if carry { 1int } else { 0int })
                == numeric_model::limbs(a@, i as nat) + numeric_model::limbs(b@, i as nat),
    ))]
    for i in 0..N {
        let (x, p) = (a[i].wrapping_add(b[i]), a[i] > u128::MAX - b[i]);
        let (x, q) = (x.wrapping_add(carry as u128), x > u128::MAX - carry as u128);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(x as int + numeric_model::radix() * (if p || q { 1int } else { 0int })
                == a@[i as int] as int + b@[i as int] as int + (if carry { 1int } else { 0int }));
            numeric_model::carry_step(out@, a@, b@, i as nat, x, carry, p || q);
        }
        out[i] = x;
        carry = p || q;
        #[cfg(verus_keep_ghost)]
        proof! {
            if N == 2 {
                numeric_model::prefix_two(out@, i as nat + 1);
                numeric_model::prefix_two(a@, i as nat + 1);
                numeric_model::prefix_two(b@, i as nat + 1);
                assert(numeric_model::prefix2(out@, i as int + 1)
                    + numeric_model::place2(i as int + 1) * (if carry { 1int } else { 0int })
                    == numeric_model::prefix2(a@, i as int + 1)
                        + numeric_model::prefix2(b@, i as int + 1));
            }
        }
    }
    (out, carry)
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures N == 2 ==> numeric_model::prefix2(result.0@, 2)
        - numeric_model::modulus() * (if result.1 { 1int } else { 0int })
        == numeric_model::prefix2(a@, 2) - numeric_model::prefix2(b@, 2),
        numeric_model::value(result.0@)
            - numeric_model::place(N as nat) * (if result.1 { 1int } else { 0int })
            == numeric_model::value(a@) - numeric_model::value(b@),
))]
fn subtract<const N: usize>(a: [u128; N], b: [u128; N]) -> ([u128; N], bool) {
    let mut out = [0; N];
    let mut borrow = false;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant N == 2 ==> numeric_model::prefix2(out@, i as int)
            - numeric_model::place2(i as int) * (if borrow { 1int } else { 0int })
            == numeric_model::prefix2(a@, i as int) - numeric_model::prefix2(b@, i as int),
            out@.len() == N, a@.len() == N, b@.len() == N,
            numeric_model::limbs(out@, i as nat)
                - numeric_model::place(i as nat) * (if borrow { 1int } else { 0int })
                == numeric_model::limbs(a@, i as nat) - numeric_model::limbs(b@, i as nat),
    ))]
    for i in 0..N {
        let (x, p) = (a[i].wrapping_sub(b[i]), a[i] < b[i]);
        let (x, q) = (x.wrapping_sub(borrow as u128), x < borrow as u128);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(x as int - numeric_model::radix() * (if p || q { 1int } else { 0int })
                == a@[i as int] as int - b@[i as int] as int - (if borrow { 1int } else { 0int }));
            numeric_model::borrow_step(out@, a@, b@, i as nat, x, borrow, p || q);
        }
        out[i] = x;
        borrow = p || q;
        #[cfg(verus_keep_ghost)]
        proof! {
            if N == 2 {
                assert(numeric_model::prefix2(out@, i as int + 1)
                    - numeric_model::place2(i as int + 1) * (if borrow { 1int } else { 0int })
                    == numeric_model::prefix2(a@, i as int + 1)
                        - numeric_model::prefix2(b@, i as int + 1));
            }
        }
    }
    (out, borrow)
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures numeric_model::add_word_result(old(out)@, final(out)@, at, value, result),
))]
fn add_word<const N: usize>(out: &mut [u128; N], at: usize, value: u128) -> Result<(), Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {
        numeric_model::value_bounds(out@);
        numeric_model::place_positive(at as nat);
    }
    if at >= N {
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::place_monotone(N as nat, at as nat);
            if value != 0 {
                vstd::arithmetic::mul::lemma_mul_inequality(1, value as int,
                    numeric_model::place(at as nat));
            }
        }
        return if value == 0 {
            Ok(())
        } else {
            Err(Failure::Invariant)
        };
    }
    let mut carry = value;
    let mut i = at;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant at < N, at <= i <= N, out@.len() == N, old(out)@.len() == N,
            i > at ==> carry == 1,
            numeric_model::value(out@) + numeric_model::place(i as nat) * carry as int
                == numeric_model::value(old(out)@) + numeric_model::place(at as nat) * value as int,
        decreases N - i,
    ))]
    while i < N {
        let next = out[i].wrapping_add(carry);
        let overflow = out[i] > u128::MAX - carry;
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::carry_word(out@, i as nat, carry, next, overflow); }
        out[i] = next;
        carry = overflow as u128;
        if carry == 0 {
            #[cfg(verus_keep_ghost)]
            proof! { numeric_model::value_bounds(out@); }
            return Ok(());
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { numeric_model::value_bounds(out@); }
    Err(Failure::Invariant)
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires A + B <= O,
    ensures numeric_model::product_result(a@, b@, result),
))]
fn multiply<const A: usize, const B: usize, const O: usize>(
    a: [u128; A],
    b: [u128; B],
) -> Result<[u128; O], Failure> {
    let mut out = [0; O];
    #[cfg(verus_keep_ghost)]
    proof! {
        numeric_model::zero_value(out@, O as nat);
        assert(numeric_model::limbs(a@, 0) == 0);
    }
    let mut i = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i <= A, a@.len() == A, b@.len() == B, out@.len() == O, A + B <= O,
            numeric_model::value(out@) == numeric_model::limbs(a@, i as nat) * numeric_model::value(b@),
        decreases A - i,
    ))]
    while i < A {
        let mut j = 0;
        #[cfg(verus_keep_ghost)]
        proof! { assert(numeric_model::limbs(b@, 0) == 0); }
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant i < A, j <= B, a@.len() == A, b@.len() == B, out@.len() == O, A + B <= O,
                numeric_model::value(out@) == numeric_model::limbs(a@, i as nat) * numeric_model::value(b@)
                    + numeric_model::place(i as nat) * a@[i as int] as int
                        * numeric_model::limbs(b@, j as nat),
            decreases B - j,
        ))]
        while j < B {
            let product = wide::multiply(a[i], b[j]);
            #[cfg(verus_keep_ghost)]
            proof! {
                numeric_model::product_step(a@, b@, i as nat, j as nat, numeric_model::value(out@),
                    product[0], product[1], O as nat);
            }
            let at = i.checked_add(j).ok_or(Failure::Invariant)?;
            add_word(&mut out, at, product[0])?;
            add_word(
                &mut out,
                at.checked_add(1).ok_or(Failure::Invariant)?,
                product[1],
            )?;
            j += 1;
        }
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::product_row(a@, b@, i as nat); }
        i += 1;
    }
    Ok(out)
}
/// Adapted from the audited owned zUSD wide divider. The extra shifted bit is
/// preserved as carry; subtraction's borrow must match it. No quotient narrowing.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures numeric_model::divide_result(n@, d@, result),
))]
fn divide<const N: usize>(n: [u128; N], d: [u128; N]) -> Result<([u128; N], [u128; N]), Failure> {
    let mut zeros = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant zeros <= N, d@.len() == N, forall|k: int| 0 <= k < zeros ==> d@[k] == 0,
        decreases N - zeros,
    ))]
    while zeros < N && d[zeros] == 0 {
        zeros += 1;
    }
    if zeros == N {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::zero_value(d@, N as nat); }
        return Err(Failure::ZeroDivisor);
    }
    #[cfg(verus_keep_ghost)]
    proof! { numeric_model::nonzero_value(d@, zeros as int); }
    let bits = N.checked_mul(128).ok_or(Failure::Domain)?;
    let mut q = [0; N];
    let mut r = [0; N];
    #[cfg(verus_keep_ghost)]
    proof! {
        numeric_model::zero_value(q@, N as nat);
        numeric_model::zero_value(r@, N as nat);
        numeric_model::top_empty(n@);
        numeric_model::value_bounds(d@);
        vstd::arithmetic::div_mod::lemma_small_mod(0nat, numeric_model::value(d@) as nat);
        vstd::arithmetic::div_mod::lemma_basic_div(0, numeric_model::value(d@));
    }
    let mut bit = bits;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            bits == 128 * N, bit <= bits,
            n@.len() == N, d@.len() == N, q@.len() == N, r@.len() == N,
            0 < numeric_model::value(d@) < numeric_model::place(N as nat),
            numeric_model::value(r@) == numeric_model::top(n@, bit as nat) % numeric_model::value(d@),
            numeric_model::value(q@) == numeric_model::top(n@, bit as nat) / numeric_model::value(d@)
                * vstd::arithmetic::power2::pow2(bit as nat) as int,
        decreases bit,
    ))]
    while bit > 0 {
        bit -= 1;
        let mut carry = (n[bit / 128] >> (bit % 128)) & 1;
        #[cfg(verus_keep_ghost)]
        proof_decl! {
            let ghost before = r@;
            let ghost incoming = carry;
            let ghost high = numeric_model::top(n@, bit as nat + 1);
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::bit_step(n@, bit);
            numeric_model::one_bit(n@[(bit / 128) as int], bit % 128);
            assert(numeric_model::limbs(r@, 0) == 0);
            assert(numeric_model::place(0) == 1);
        }
        let mut i = 0;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant i <= N, r@.len() == N, before.len() == N, carry <= 1, incoming <= 1,
                forall|k: int| i <= k < N ==> r@[k] == before[k],
                numeric_model::limbs(r@, i as nat) + numeric_model::place(i as nat) * carry as int
                    == 2 * numeric_model::limbs(before, i as nat) + incoming as int,
            decreases N - i,
        ))]
        while i < N {
            let next = r[i] >> 127;
            #[cfg(verus_keep_ghost)]
            proof! { numeric_model::shift_step(r@, before, i as nat, carry, incoming); }
            r[i] = (r[i] << 1) | carry;
            carry = next;
            i += 1;
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            let d_value = numeric_model::value(d@);
            numeric_model::value_bounds(r@);
            numeric_model::value_bounds(n@);
            vstd::arithmetic::power2::lemma_pow2_pos(bit as nat + 1);
            vstd::arithmetic::div_mod::lemma_div_pos_is_pos(numeric_model::value(n@),
                vstd::arithmetic::power2::pow2(bit as nat + 1) as int);
            numeric_model::quotient_step(high, incoming as int, d_value,
                numeric_model::value(r@) + numeric_model::place(N as nat) * carry as int);
            numeric_model::compare_order(r@, d@);
            vstd::arithmetic::div_mod::lemma_div_pos_is_pos(high, d_value);
            numeric_model::quotient_bits(q@, bit, high / d_value);
        }
        if carry != 0 || !matches!(compare(&r, &d), Ordering::Less) {
            let (next, borrow) = subtract(r, d);
            #[cfg(verus_keep_ghost)]
            proof! { numeric_model::value_bounds(next@); }
            if borrow != (carry != 0) {
                return Err(Failure::Invariant);
            }
            r = next;
            q[bit / 128] |= 1u128 << (bit % 128);
        }
    }
    #[cfg(verus_keep_ghost)]
    proof! { numeric_model::compare_order(r@, d@); }
    if !matches!(compare(&r, &d), Ordering::Less) {
        return Err(Failure::Invariant);
    }
    #[cfg(verus_keep_ghost)]
    proof! { numeric_model::divide_finish(n@, d@, q@, r@); }
    Ok((q, r))
}

#[cfg_attr(verus_keep_ghost, cfg_eval)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl U256 {
    /// The unsigned zero value.
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures numeric_model::unsigned(Self::ZERO) == 0,
    ))]
    pub const ZERO: Self = {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::constant_bits(); }
        Self([0, 0])
    };
    /// The unsigned unit value.
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures numeric_model::unsigned(Self::ONE) == 1,
    ))]
    pub const ONE: Self = {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::constant_bits(); }
        Self([1, 0])
    };
    /// The largest unsigned 256-bit value.
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures numeric_model::unsigned(Self::MAX) == numeric_model::modulus() - 1,
    ))]
    pub const MAX: Self = {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::constant_bits(); }
        Self([u128::MAX, u128::MAX])
    };
    /// Zero-extends an unsigned 128-bit value.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::unsigned(result) == x as int,
    ))]
    pub const fn from_u128(x: u128) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::unsigned_word(x); }
        Self([x, 0])
    }
    /// Returns the value if its high 128-bit word is zero.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::word_result(result, numeric_model::unsigned(self)),
    ))]
    pub fn try_u128(self) -> Result<u128, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::conversion_representation(); }
        if self.0[1] == 0 {
            Ok(self.0[0])
        } else {
            Err(Failure::Overflow)
        }
    }
    /// Subtracts two unsigned values, refusing underflow.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::unsigned_result(result,
            numeric_model::unsigned(self) - numeric_model::unsigned(other)),
    ))]
    pub fn checked_sub(self, other: Self) -> Result<Self, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::conversion_representation();
            numeric_model::unsigned_bounds(self);
            numeric_model::unsigned_bounds(other);
        }
        let (value, borrow) = subtract(self.0, other.0);
        if borrow {
            Err(Failure::Overflow)
        } else {
            Ok(Self(value))
        }
    }
    /// Adds two unsigned values, refusing overflow.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::unsigned_result(result,
            numeric_model::unsigned(self) + numeric_model::unsigned(other)),
    ))]
    pub fn checked_add(self, other: Self) -> Result<Self, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::conversion_representation();
            numeric_model::unsigned_bounds(self);
            numeric_model::unsigned_bounds(other);
        }
        let (value, carry) = add(self.0, other.0);
        if carry {
            Err(Failure::Overflow)
        } else {
            Ok(Self(value))
        }
    }
    /// Returns the complete 512-bit product of two unsigned values.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::wide_product_result(result,
            numeric_model::unsigned(self) * numeric_model::unsigned(other)),
    ))]
    pub fn full_product(self, other: Self) -> Result<U512, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::limb_representations(); }
        Ok(U512(multiply(self.0, other.0)?))
    }
    /// Multiplies two unsigned values, refusing a result wider than 256 bits.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::unsigned_result(result,
            numeric_model::unsigned(self) * numeric_model::unsigned(other)),
    ))]
    pub fn checked_mul(self, other: Self) -> Result<Self, Failure> {
        self.full_product(other)?.narrow()
    }
    /// Returns the unsigned quotient and remainder, refusing a zero divisor.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::div_rem_result(self, den, result),
    ))]
    pub fn div_rem(self, den: Self) -> Result<(Self, Self), Failure> {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::limb_representations(); }
        let (q, r) = divide(self.0, den.0)?;
        Ok((Self(q), Self(r)))
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::rounded_result(q, remainder, mode, result),
    ))]
    fn rounded(q: Self, remainder: bool, mode: Rounding) -> Result<Self, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::unsigned_bounds(q); }
        match mode {
            Rounding::Exact if remainder => Err(Failure::Inexact),
            Rounding::Ceil if remainder => q.checked_add(Self::ONE),
            _ => Ok(q),
        }
    }
    /// Divides using the selected rounding mode and refuses a zero divisor.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::checked_div_result(self, den, mode, result),
    ))]
    pub fn checked_div(self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        let (q, r) = self.div_rem(den)?;
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::canonical_quotient(numeric_model::unsigned(self), numeric_model::unsigned(den),
                numeric_model::unsigned(q), numeric_model::unsigned(r));
            numeric_model::zero_words();
        }
        Self::rounded(q, r.0 != Self::ZERO.0, mode)
    }
}

#[cfg_attr(verus_keep_ghost, verifier::external)]
impl U256 {
    /// Divides the full product plus offset, refusing a quotient wider than 256 bits.
    pub fn mul_add_div_rem(
        self,
        b: Self,
        offset: Self,
        den: Self,
    ) -> Result<(Self, Self), Failure> {
        if den == Self::ZERO {
            return Err(Failure::ZeroDivisor);
        }
        let numerator = self.full_product(b)?.checked_add(U512::from_u256(offset))?;
        let (q, r) = numerator.div_rem(U512::from_u256(den))?;
        Ok((q.narrow()?, r.narrow()?))
    }
    /// Divides the full product with one final rounding and checked narrowing.
    pub fn mul_div(self, b: Self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        let (q, r) = self.mul_add_div_rem(b, Self::ZERO, den)?;
        Self::rounded(q, r != Self::ZERO, mode)
    }
    /// Explicit nonnegative half-up operation, matching the app fee primitive.
    pub fn mul_div_half_up(self, b: Self, den: Self) -> Result<Self, Failure> {
        let half = Self([den.0[0] >> 1 | (den.0[1] << 127), den.0[1] >> 1]);
        self.mul_add_div_rem(b, half, den).map(|(q, _)| q)
    }
    /// Returns the rounded square root of the complete product.
    pub fn geometric_mean(self, b: Self, mode: Rounding) -> Result<Self, Failure> {
        self.full_product(b)?.sqrt(mode)
    }
    /// Three complete factors before a single final division/rounding.
    pub fn mul3_div(self, b: Self, c: Self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        if den == Self::ZERO {
            return Err(Failure::ZeroDivisor);
        }
        let numerator = self.full_product(b)?.multiply_u256(c)?;
        let (q, r) = numerator.div_rem(U768::from_u256(den))?;
        Self::rounded(q.narrow()?, r != U768::ZERO, mode)
    }
    /// Compares two complete products of three unsigned factors.
    pub fn products_ge(a: [Self; 3], b: [Self; 3]) -> Result<bool, Failure> {
        let left = a[0].full_product(a[1])?.multiply_u256(a[2])?;
        let right = b[0].full_product(b[1])?.multiply_u256(b[2])?;
        Ok(compare(&left.0, &right.0) != Ordering::Less)
    }
}

#[cfg_attr(verus_keep_ghost, cfg_eval)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl U512 {
    /// The unsigned 512-bit zero value.
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures numeric_model::unsigned512(Self::ZERO) == 0,
    ))]
    pub const ZERO: Self = {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::wide_representation(); }
        Self([0; 4])
    };
    /// Zero-extends an unsigned 256-bit value.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::unsigned512(result) == numeric_model::unsigned(x),
    ))]
    pub const fn from_u256(x: U256) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::conversion_representation(); }
        Self([x.0[0], x.0[1], 0, 0])
    }
    /// Returns the value if it fits in 256 bits.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::unsigned_result(result, numeric_model::unsigned512(self)),
    ))]
    pub fn narrow(self) -> Result<U256, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::conversion_representation(); }
        if self.0[2] == 0 && self.0[3] == 0 {
            Ok(U256([self.0[0], self.0[1]]))
        } else {
            Err(Failure::Overflow)
        }
    }
}

#[cfg_attr(verus_keep_ghost, verifier::external)]
impl U512 {
    /// Constructs a value from four words, least significant word first.
    pub const fn from_words_le(words: [u128; 4]) -> Self {
        Self(words)
    }
    /// Returns four words in least significant word order.
    pub const fn words_le(self) -> [u128; 4] {
        self.0
    }
    /// Adds two 512-bit values, refusing overflow.
    pub fn checked_add(self, other: Self) -> Result<Self, Failure> {
        let (value, carry) = add(self.0, other.0);
        if carry {
            Err(Failure::Overflow)
        } else {
            Ok(Self(value))
        }
    }
    /// Returns the complete 768-bit product with an unsigned 256-bit value.
    pub fn multiply_u256(self, other: U256) -> Result<U768, Failure> {
        Ok(U768(multiply(self.0, other.0)?))
    }
    /// Returns the 512-bit quotient and remainder, refusing a zero divisor.
    pub fn div_rem(self, den: Self) -> Result<(Self, Self), Failure> {
        let (q, r) = divide(self.0, den.0)?;
        Ok((Self(q), Self(r)))
    }
    /// Returns the rounded square root with checked 256-bit narrowing.
    pub fn sqrt(self, mode: Rounding) -> Result<U256, Failure> {
        let mut root = U256::ZERO;
        for bit in (0..256).rev() {
            let mut trial = root;
            trial.0[bit / 128] |= 1u128 << (bit % 128);
            let squared = trial.full_product(trial)?;
            if compare(&squared.0, &self.0) != Ordering::Greater {
                root = trial;
            }
        }
        let exact = root.full_product(root)? == self;
        U256::rounded(root, !exact, mode)
    }
    /// Full512 quotient is retained before sqrt; only the root is256.
    pub fn sqrt_ratio(self, den: Self, mode: Rounding) -> Result<U256, Failure> {
        let (q, r) = self.div_rem(den)?;
        let root = q.sqrt(Rounding::Floor)?;
        let exact = root.full_product(root)? == q && r == Self::ZERO;
        U256::rounded(root, !exact, mode)
    }
}

#[cfg_attr(verus_keep_ghost, cfg_eval)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl U768 {
    /// The unsigned 768-bit zero value.
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures numeric_model::unsigned768(Self::ZERO) == 0,
    ))]
    pub const ZERO: Self = {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::wide_representation(); }
        Self([0; 6])
    };
    /// Zero-extends an unsigned 256-bit value.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::unsigned768(result) == numeric_model::unsigned(x),
    ))]
    pub const fn from_u256(x: U256) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::conversion_representation(); }
        Self([x.0[0], x.0[1], 0, 0, 0, 0])
    }
    /// Returns the value if it fits in 256 bits.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::unsigned_result(result, numeric_model::unsigned768(self)),
    ))]
    pub fn narrow(self) -> Result<U256, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::conversion_representation(); }
        if self.0[2] == 0 && self.0[3] == 0 && self.0[4] == 0 && self.0[5] == 0 {
            Ok(U256([self.0[0], self.0[1]]))
        } else {
            Err(Failure::Overflow)
        }
    }
}

#[cfg_attr(verus_keep_ghost, verifier::external)]
impl U768 {
    /// Returns the 768-bit quotient and remainder, refusing a zero divisor.
    pub fn div_rem(self, den: Self) -> Result<(Self, Self), Failure> {
        let (q, r) = divide(self.0, den.0)?;
        Ok((Self(q), Self(r)))
    }
}

#[cfg_attr(verus_keep_ghost, cfg_eval)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl I256 {
    /// The signed zero value.
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures numeric_model::signed(Self::ZERO) == 0,
    ))]
    pub const ZERO: Self = {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::constant_bits(); }
        Self(U256::ZERO)
    };
    /// The smallest signed 256-bit value.
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures numeric_model::signed(Self::MIN) == -numeric_model::half(),
    ))]
    pub const MIN: Self = {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::constant_bits();
        assert(1u128 << 127 == 170141183460469231731687303715884105728u128) by(bit_vector); }
        Self(U256([0, 1u128 << 127]))
    };
    /// The largest signed 256-bit value.
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures numeric_model::signed(Self::MAX) == numeric_model::half() - 1,
    ))]
    pub const MAX: Self = {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::constant_bits();
        assert(1u128 << 127 == 170141183460469231731687303715884105728u128) by(bit_vector); }
        Self(U256([u128::MAX, (1u128 << 127) - 1]))
    };
    /// Sign-extends a signed 128-bit value.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::signed(result) == x as int,
    ))]
    pub const fn from_i128(x: i128) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::conversion_representation();
            numeric_model::signed_word_cast(x);
        }
        Self(U256([x as u128, if x < 0 { u128::MAX } else { 0 }]))
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::unsigned(result) ==
            if numeric_model::unsigned(x) == 0 { 0int }
            else { numeric_model::modulus() - numeric_model::unsigned(x) },
    ))]
    fn negative_bits(x: U256) -> U256 {
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::conversion_representation();
            numeric_model::unsigned_bounds(x);
            numeric_model::complement(x.0@[0]);
            numeric_model::complement(x.0@[1]);
        }
        let (words, _) = add([!x.0[0], !x.0[1]], [1, 0]);
        U256(words)
    }
    /// Returns the unsigned absolute magnitude, including that of the minimum value.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::unsigned(result) ==
            if numeric_model::signed(self) < 0 { -numeric_model::signed(self) }
            else { numeric_model::signed(self) },
    ))]
    pub fn magnitude(self) -> U256 {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::conversion_representation(); }
        if self.is_negative() {
            Self::negative_bits(self.0)
        } else {
            self.0
        }
    }
    /// Applies the sign to a magnitude, refusing values outside the signed range.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::signed_result(result,
            if negative { -numeric_model::unsigned(x) } else { numeric_model::unsigned(x) }),
    ))]
    pub fn from_magnitude(x: U256, negative: bool) -> Result<Self, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::conversion_representation();
            numeric_model::unsigned_bounds(x);
        }
        let bound = if negative { Self::MIN.0 } else { Self::MAX.0 };
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::unsigned_bounds(bound);
            assert(numeric_model::unsigned(bound) ==
                if negative { numeric_model::half() } else { numeric_model::half() - 1 });
        }
        if matches!(x.cmp(bound), Ordering::Greater) {
            return Err(Failure::Overflow);
        }
        Ok(Self(if negative { Self::negative_bits(x) } else { x }))
    }
    /// Returns the unsigned value if this signed value is nonnegative.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::unsigned_result(result, numeric_model::signed(self)),
    ))]
    pub fn try_unsigned(self) -> Result<U256, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::conversion_representation();
            numeric_model::signed_bounds(self);
        }
        if self.is_negative() {
            Err(Failure::Overflow)
        } else {
            Ok(self.0)
        }
    }
    /// A fitting signed delta need not require either unsigned price/amount to
    /// fit I256 separately. Form the exact magnitude before signed narrowing.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::signed_result(result,
            numeric_model::unsigned(a) - numeric_model::unsigned(b)),
    ))]
    pub fn difference_unsigned(a: U256, b: U256) -> Result<Self, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! { numeric_model::unsigned_bounds(a); numeric_model::unsigned_bounds(b); }
        if matches!(a.cmp(b), Ordering::Less) {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(numeric_model::unsigned(a) < numeric_model::unsigned(b));
                assert(0 <= numeric_model::unsigned(b) - numeric_model::unsigned(a) < numeric_model::modulus());
            }
            Self::from_magnitude(b.checked_sub(a)?, true)
        } else {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(numeric_model::unsigned(a) >= numeric_model::unsigned(b));
                assert(0 <= numeric_model::unsigned(a) - numeric_model::unsigned(b) < numeric_model::modulus());
            }
            Self::from_magnitude(a.checked_sub(b)?, false)
        }
    }
    /// Negates the value, refusing the minimum signed value.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::signed_result(result, -numeric_model::signed(self)),
    ))]
    pub fn checked_neg(self) -> Result<Self, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::conversion_representation();
            numeric_model::signed_bounds(self);
            numeric_model::unsigned_bounds(self.0);
            numeric_model::minimum_bits();
        }
        if self.0.0 == Self::MIN.0.0 {
            #[cfg(verus_keep_ghost)]
            proof! { assert(numeric_model::signed(self) == -numeric_model::half()); }
            Err(Failure::Overflow)
        } else {
            #[cfg(verus_keep_ghost)]
            proof! { assert(numeric_model::signed(self) != -numeric_model::half()); }
            Ok(Self(Self::negative_bits(self.0)))
        }
    }
    /// Adds two signed values, refusing signed overflow.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::signed_result(result,
            numeric_model::signed(self) + numeric_model::signed(other)),
    ))]
    pub fn checked_add(self, other: Self) -> Result<Self, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::conversion_representation();
            numeric_model::signed_bounds(self);
            numeric_model::signed_bounds(other);
            numeric_model::unsigned_bounds(self.0);
            numeric_model::unsigned_bounds(other.0);
        }
        let (bits, _) = add(self.0.0, other.0.0);
        let result = Self(U256(bits));
        if self.is_negative() == other.is_negative() && result.is_negative() != self.is_negative() {
            Err(Failure::Overflow)
        } else {
            Ok(result)
        }
    }
    /// Subtracts two signed values, refusing signed overflow.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures numeric_model::signed_result(result,
            numeric_model::signed(self) - numeric_model::signed(other)),
    ))]
    pub fn checked_sub(self, other: Self) -> Result<Self, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! {
            numeric_model::conversion_representation();
            numeric_model::signed_bounds(self);
            numeric_model::signed_bounds(other);
            numeric_model::unsigned_bounds(self.0);
            numeric_model::unsigned_bounds(other.0);
        }
        let (bits, _) = subtract(self.0.0, other.0.0);
        let result = Self(U256(bits));
        if self.is_negative() != other.is_negative() && result.is_negative() != self.is_negative() {
            Err(Failure::Overflow)
        } else {
            Ok(result)
        }
    }
}

#[cfg_attr(verus_keep_ghost, verifier::external)]
impl I256 {
    /// Multiplies two signed values, refusing signed overflow.
    pub fn checked_mul(self, other: Self) -> Result<Self, Failure> {
        Self::from_magnitude(
            self.magnitude().checked_mul(other.magnitude())?,
            self.is_negative() != other.is_negative(),
        )
    }
    fn magnitude_round(mode: Rounding, negative: bool) -> Rounding {
        match (mode, negative) {
            (Rounding::Floor, true) | (Rounding::Ceil, false) => Rounding::Ceil,
            (Rounding::Exact, _) => Rounding::Exact,
            _ => Rounding::Floor,
        }
    }
    /// Divides with signed rounding, refusing zero, inexactness or signed overflow.
    pub fn checked_div(self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        let negative = self.is_negative() != den.is_negative();
        let q = self
            .magnitude()
            .checked_div(den.magnitude(), Self::magnitude_round(mode, negative))?;
        Self::from_magnitude(q, negative)
    }
    /// Divides the full signed product by a positive denominator with checked rounding.
    pub fn mul_div_positive(self, b: Self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        if den.is_negative() || den == Self::ZERO {
            return Err(Failure::Domain);
        }
        let negative = self.is_negative() != b.is_negative();
        let q = self.magnitude().mul_div(
            b.magnitude(),
            den.magnitude(),
            Self::magnitude_round(mode, negative),
        )?;
        Self::from_magnitude(q, negative)
    }
    /// Requires nonnegative factors and a positive denominator before rounded division.
    pub fn nonnegative_mul_div(self, b: Self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        if self.is_negative() || b.is_negative() {
            return Err(Failure::Domain);
        }
        self.mul_div_positive(b, den, mode)
    }
    /// Raw signed*unsigned*signed ratio; scale/unit and payer/payee policies
    /// belong to a separately bound consumer, not this arithmetic helper.
    pub fn mul_unsigned_signed_div(
        self,
        b: U256,
        c: Self,
        den: U256,
        mode: Rounding,
    ) -> Result<Self, Failure> {
        let negative = self.is_negative() != c.is_negative();
        let q = self.magnitude().mul3_div(
            b,
            c.magnitude(),
            den,
            Self::magnitude_round(mode, negative),
        )?;
        Self::from_magnitude(q, negative)
    }
    /// Compares two triple products after refusing any negative factor.
    pub fn nonnegative_products_ge(a: [Self; 3], b: [Self; 3]) -> Result<bool, Failure> {
        for x in a.iter().chain(b.iter()) {
            if x.is_negative() {
                return Err(Failure::Domain);
            }
        }
        U256::products_ge(a.map(|x| x.0), b.map(|x| x.0))
    }
    /// Returns the value if its upper word is a valid 128-bit sign extension.
    pub fn try_i128(self) -> Result<i128, Failure> {
        let low = self.0.0[0] as i128;
        if self.0.0[1] == if low < 0 { u128::MAX } else { 0 } {
            Ok(low)
        } else {
            Err(Failure::Overflow)
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
