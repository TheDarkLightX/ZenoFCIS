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
))]
fn add<const N: usize>(a: [u128; N], b: [u128; N]) -> ([u128; N], bool) {
    let mut out = [0; N];
    let mut carry = false;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant N == 2 ==> numeric_model::prefix2(out@, i as int)
            + numeric_model::place2(i as int) * (if carry { 1int } else { 0int })
            == numeric_model::prefix2(a@, i as int) + numeric_model::prefix2(b@, i as int),
    ))]
    for i in 0..N {
        let (x, p) = (a[i].wrapping_add(b[i]), a[i] > u128::MAX - b[i]);
        let (x, q) = (x.wrapping_add(carry as u128), x > u128::MAX - carry as u128);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(x as int + numeric_model::radix() * (if p || q { 1int } else { 0int })
                == a@[i as int] as int + b@[i as int] as int + (if carry { 1int } else { 0int }));
        }
        out[i] = x;
        carry = p || q;
        #[cfg(verus_keep_ghost)]
        proof! {
            if N == 2 {
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
))]
fn subtract<const N: usize>(a: [u128; N], b: [u128; N]) -> ([u128; N], bool) {
    let mut out = [0; N];
    let mut borrow = false;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant N == 2 ==> numeric_model::prefix2(out@, i as int)
            - numeric_model::place2(i as int) * (if borrow { 1int } else { 0int })
            == numeric_model::prefix2(a@, i as int) - numeric_model::prefix2(b@, i as int),
    ))]
    for i in 0..N {
        let (x, p) = (a[i].wrapping_sub(b[i]), a[i] < b[i]);
        let (x, q) = (x.wrapping_sub(borrow as u128), x < borrow as u128);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(x as int - numeric_model::radix() * (if p || q { 1int } else { 0int })
                == a@[i as int] as int - b@[i as int] as int - (if borrow { 1int } else { 0int }));
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
#[cfg_attr(verus_keep_ghost, verifier::external)]
fn add_word<const N: usize>(out: &mut [u128; N], at: usize, value: u128) -> Result<(), Failure> {
    if at >= N {
        return if value == 0 {
            Ok(())
        } else {
            Err(Failure::Invariant)
        };
    }
    let mut carry = value;
    for limb in &mut out[at..] {
        let (next, overflow) = limb.overflowing_add(carry);
        *limb = next;
        carry = u128::from(overflow);
        if carry == 0 {
            return Ok(());
        }
    }
    Err(Failure::Invariant)
}
#[cfg_attr(verus_keep_ghost, verifier::external)]
fn multiply<const A: usize, const B: usize, const O: usize>(
    a: [u128; A],
    b: [u128; B],
) -> Result<[u128; O], Failure> {
    let mut out = [0; O];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            let product = wide::multiply(*x, *y);
            let at = i.checked_add(j).ok_or(Failure::Invariant)?;
            add_word(&mut out, at, product[0])?;
            add_word(
                &mut out,
                at.checked_add(1).ok_or(Failure::Invariant)?,
                product[1],
            )?;
        }
    }
    Ok(out)
}
/// Adapted from the audited owned zUSD wide divider. The extra shifted bit is
/// preserved as carry; subtraction's borrow must match it. No quotient narrowing.
#[cfg_attr(verus_keep_ghost, verifier::external)]
fn divide<const N: usize>(n: [u128; N], d: [u128; N]) -> Result<([u128; N], [u128; N]), Failure> {
    if d.iter().all(|x| *x == 0) {
        return Err(Failure::ZeroDivisor);
    }
    let bits = N.checked_mul(128).ok_or(Failure::Domain)?;
    let mut q = [0; N];
    let mut r = [0; N];
    for bit in (0..bits).rev() {
        let mut carry = (n[bit / 128] >> (bit % 128)) & 1;
        for limb in &mut r {
            let next = *limb >> 127;
            *limb = (*limb << 1) | carry;
            carry = next;
        }
        if carry != 0 || compare(&r, &d) != Ordering::Less {
            let (next, borrow) = subtract(r, d);
            if borrow != (carry != 0) {
                return Err(Failure::Invariant);
            }
            r = next;
            q[bit / 128] |= 1u128 << (bit % 128);
        }
    }
    if compare(&r, &d) != Ordering::Less {
        return Err(Failure::Invariant);
    }
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
}

#[cfg_attr(verus_keep_ghost, verifier::external)]
impl U256 {
    /// Adds two unsigned values, refusing overflow.
    pub fn checked_add(self, other: Self) -> Result<Self, Failure> {
        let (value, carry) = add(self.0, other.0);
        if carry {
            Err(Failure::Overflow)
        } else {
            Ok(Self(value))
        }
    }
    /// Returns the complete 512-bit product of two unsigned values.
    pub fn full_product(self, other: Self) -> Result<U512, Failure> {
        Ok(U512(multiply(self.0, other.0)?))
    }
    /// Multiplies two unsigned values, refusing a result wider than 256 bits.
    pub fn checked_mul(self, other: Self) -> Result<Self, Failure> {
        self.full_product(other)?.narrow()
    }
    /// Returns the unsigned quotient and remainder, refusing a zero divisor.
    pub fn div_rem(self, den: Self) -> Result<(Self, Self), Failure> {
        let (q, r) = divide(self.0, den.0)?;
        Ok((Self(q), Self(r)))
    }
    fn rounded(q: Self, remainder: bool, mode: Rounding) -> Result<Self, Failure> {
        match mode {
            Rounding::Exact if remainder => Err(Failure::Inexact),
            Rounding::Ceil if remainder => q.checked_add(Self::ONE),
            _ => Ok(q),
        }
    }
    /// Divides using the selected rounding mode and refuses a zero divisor.
    pub fn checked_div(self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        let (q, r) = self.div_rem(den)?;
        Self::rounded(q, r != Self::ZERO, mode)
    }
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
}

#[cfg_attr(verus_keep_ghost, verifier::external)]
impl I256 {
    /// Negates the value, refusing the minimum signed value.
    pub fn checked_neg(self) -> Result<Self, Failure> {
        if self == Self::MIN {
            Err(Failure::Overflow)
        } else {
            Ok(Self(Self::negative_bits(self.0)))
        }
    }
    /// Adds two signed values, refusing signed overflow.
    pub fn checked_add(self, other: Self) -> Result<Self, Failure> {
        let (bits, _) = add(self.0.0, other.0.0);
        let result = Self(U256(bits));
        if self.is_negative() == other.is_negative() && result.is_negative() != self.is_negative() {
            Err(Failure::Overflow)
        } else {
            Ok(result)
        }
    }
    /// Subtracts two signed values, refusing signed overflow.
    pub fn checked_sub(self, other: Self) -> Result<Self, Failure> {
        let (bits, _) = subtract(self.0.0, other.0.0);
        let result = Self(U256(bits));
        if self.is_negative() != other.is_negative() && result.is_negative() != self.is_negative() {
            Err(Failure::Overflow)
        } else {
            Ok(result)
        }
    }
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
