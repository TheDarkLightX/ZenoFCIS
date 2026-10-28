//! PRIVATE arithmetic candidate. No codec, authority, usage or publication API.
//! Fixed limbs own their values; all mutable arrays are invocation-local scratch.
use core::cmp::Ordering;

#[allow(dead_code)]
#[path = "reference/legacy_wide.rs"]
mod wide;
#[cfg(test)]
#[allow(dead_code)]
#[path = "reference/legacy_scalar.rs"]
mod legacy_scalar;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure { Overflow, ZeroDivisor, Inexact, Domain, Invariant }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rounding { Floor, Ceil, Exact }

/// Least-significant word first; this is an arithmetic representation, not wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct U256([u128; 2]);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct I256(U256);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct U512([u128; 4]);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct U768([u128; 6]);

fn compare<const N: usize>(a: &[u128; N], b: &[u128; N]) -> Ordering {
    for i in (0..N).rev() {
        if a[i] != b[i] { return a[i].cmp(&b[i]); }
    }
    Ordering::Equal
}
fn add<const N: usize>(a: [u128; N], b: [u128; N]) -> ([u128; N], bool) {
    let mut out = [0; N];
    let mut carry = false;
    for i in 0..N {
        let (x, p) = a[i].overflowing_add(b[i]);
        let (x, q) = x.overflowing_add(u128::from(carry));
        out[i] = x; carry = p || q;
    }
    (out, carry)
}
fn subtract<const N: usize>(a: [u128; N], b: [u128; N]) -> ([u128; N], bool) {
    let mut out = [0; N];
    let mut borrow = false;
    for i in 0..N {
        let (x, p) = a[i].overflowing_sub(b[i]);
        let (x, q) = x.overflowing_sub(u128::from(borrow));
        out[i] = x; borrow = p || q;
    }
    (out, borrow)
}
fn add_word<const N: usize>(out: &mut [u128; N], at: usize, value: u128) -> Result<(), Failure> {
    if at >= N { return if value == 0 { Ok(()) } else { Err(Failure::Invariant) }; }
    let mut carry = value;
    for limb in &mut out[at..] {
        let (next, overflow) = limb.overflowing_add(carry);
        *limb = next; carry = u128::from(overflow);
        if carry == 0 { return Ok(()); }
    }
    Err(Failure::Invariant)
}
fn multiply<const A: usize, const B: usize, const O: usize>(
    a: [u128; A], b: [u128; B],
) -> Result<[u128; O], Failure> {
    let mut out = [0; O];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            let product = wide::multiply(*x, *y);
            let at = i.checked_add(j).ok_or(Failure::Invariant)?;
            add_word(&mut out, at, product[0])?;
            add_word(&mut out, at.checked_add(1).ok_or(Failure::Invariant)?, product[1])?;
        }
    }
    Ok(out)
}
/// Adapted from the audited owned zUSD wide divider. The extra shifted bit is
/// preserved as carry; subtraction's borrow must match it. No quotient narrowing.
fn divide<const N: usize>(n: [u128; N], d: [u128; N]) -> Result<([u128; N], [u128; N]), Failure> {
    if d.iter().all(|x| *x == 0) { return Err(Failure::ZeroDivisor); }
    let bits = N.checked_mul(128).ok_or(Failure::Domain)?;
    let mut q = [0; N];
    let mut r = [0; N];
    for bit in (0..bits).rev() {
        let mut carry = (n[bit / 128] >> (bit % 128)) & 1;
        for limb in &mut r {
            let next = *limb >> 127;
            *limb = (*limb << 1) | carry; carry = next;
        }
        if carry != 0 || compare(&r, &d) != Ordering::Less {
            let (next, borrow) = subtract(r, d);
            if borrow != (carry != 0) { return Err(Failure::Invariant); }
            r = next;
            q[bit / 128] |= 1u128 << (bit % 128);
        }
    }
    if compare(&r, &d) != Ordering::Less { return Err(Failure::Invariant); }
    Ok((q, r))
}

impl U256 {
    pub const ZERO: Self = Self([0, 0]);
    pub const ONE: Self = Self([1, 0]);
    pub const MAX: Self = Self([u128::MAX, u128::MAX]);
    pub const fn from_u128(x: u128) -> Self { Self([x, 0]) }
    pub const fn from_words_le(words: [u128; 2]) -> Self { Self(words) }
    pub const fn words_le(self) -> [u128; 2] { self.0 }
    pub fn cmp(self, other: Self) -> Ordering { compare(&self.0, &other.0) }
    pub fn checked_add(self, other: Self) -> Result<Self, Failure> {
        let (value, carry) = add(self.0, other.0);
        if carry { Err(Failure::Overflow) } else { Ok(Self(value)) }
    }
    pub fn checked_sub(self, other: Self) -> Result<Self, Failure> {
        let (value, borrow) = subtract(self.0, other.0);
        if borrow { Err(Failure::Overflow) } else { Ok(Self(value)) }
    }
    pub fn full_product(self, other: Self) -> Result<U512, Failure> {
        Ok(U512(multiply(self.0, other.0)?))
    }
    pub fn checked_mul(self, other: Self) -> Result<Self, Failure> {
        self.full_product(other)?.narrow()
    }
    pub fn div_rem(self, den: Self) -> Result<(Self, Self), Failure> {
        let (q, r) = divide(self.0, den.0)?; Ok((Self(q), Self(r)))
    }
    fn rounded(q: Self, remainder: bool, mode: Rounding) -> Result<Self, Failure> {
        match mode {
            Rounding::Exact if remainder => Err(Failure::Inexact),
            Rounding::Ceil if remainder => q.checked_add(Self::ONE),
            _ => Ok(q),
        }
    }
    pub fn checked_div(self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        let (q, r) = self.div_rem(den)?; Self::rounded(q, r != Self::ZERO, mode)
    }
    pub fn mul_add_div_rem(self, b: Self, offset: Self, den: Self) -> Result<(Self, Self), Failure> {
        if den == Self::ZERO { return Err(Failure::ZeroDivisor); }
        let numerator = self.full_product(b)?.checked_add(U512::from_u256(offset))?;
        let (q, r) = numerator.div_rem(U512::from_u256(den))?;
        Ok((q.narrow()?, r.narrow()?))
    }
    pub fn mul_div(self, b: Self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        let (q, r) = self.mul_add_div_rem(b, Self::ZERO, den)?;
        Self::rounded(q, r != Self::ZERO, mode)
    }
    /// Explicit nonnegative half-up operation, matching the app fee primitive.
    pub fn mul_div_half_up(self, b: Self, den: Self) -> Result<Self, Failure> {
        let half = Self([den.0[0] >> 1 | (den.0[1] << 127), den.0[1] >> 1]);
        self.mul_add_div_rem(b, half, den).map(|(q, _)| q)
    }
    pub fn geometric_mean(self, b: Self, mode: Rounding) -> Result<Self, Failure> {
        self.full_product(b)?.sqrt(mode)
    }
    /// Three complete factors before a single final division/rounding.
    pub fn mul3_div(self, b: Self, c: Self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        if den == Self::ZERO { return Err(Failure::ZeroDivisor); }
        let numerator = self.full_product(b)?.multiply_u256(c)?;
        let (q, r) = numerator.div_rem(U768::from_u256(den))?;
        Self::rounded(q.narrow()?, r != U768::ZERO, mode)
    }
    pub fn products_ge(a: [Self; 3], b: [Self; 3]) -> Result<bool, Failure> {
        let left = a[0].full_product(a[1])?.multiply_u256(a[2])?;
        let right = b[0].full_product(b[1])?.multiply_u256(b[2])?;
        Ok(compare(&left.0, &right.0) != Ordering::Less)
    }
    pub fn try_u128(self) -> Result<u128, Failure> {
        if self.0[1] == 0 { Ok(self.0[0]) } else { Err(Failure::Overflow) }
    }
}

impl U512 {
    pub const ZERO: Self = Self([0; 4]);
    pub const fn from_words_le(words: [u128; 4]) -> Self { Self(words) }
    pub const fn words_le(self) -> [u128; 4] { self.0 }
    pub const fn from_u256(x: U256) -> Self { Self([x.0[0], x.0[1], 0, 0]) }
    pub fn narrow(self) -> Result<U256, Failure> {
        if self.0[2] == 0 && self.0[3] == 0 { Ok(U256([self.0[0], self.0[1]])) }
        else { Err(Failure::Overflow) }
    }
    pub fn checked_add(self, other: Self) -> Result<Self, Failure> {
        let (value, carry) = add(self.0, other.0);
        if carry { Err(Failure::Overflow) } else { Ok(Self(value)) }
    }
    pub fn multiply_u256(self, other: U256) -> Result<U768, Failure> {
        Ok(U768(multiply(self.0, other.0)?))
    }
    pub fn div_rem(self, den: Self) -> Result<(Self, Self), Failure> {
        let (q, r) = divide(self.0, den.0)?; Ok((Self(q), Self(r)))
    }
    pub fn sqrt(self, mode: Rounding) -> Result<U256, Failure> {
        let mut root = U256::ZERO;
        for bit in (0..256).rev() {
            let mut trial = root;
            trial.0[bit / 128] |= 1u128 << (bit % 128);
            let squared = trial.full_product(trial)?;
            if compare(&squared.0, &self.0) != Ordering::Greater { root = trial; }
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

impl U768 {
    pub const ZERO: Self = Self([0; 6]);
    pub const fn from_u256(x: U256) -> Self { Self([x.0[0], x.0[1], 0, 0, 0, 0]) }
    pub fn div_rem(self, den: Self) -> Result<(Self, Self), Failure> {
        let (q, r) = divide(self.0, den.0)?; Ok((Self(q), Self(r)))
    }
    pub fn narrow(self) -> Result<U256, Failure> {
        if self.0[2..].iter().all(|x| *x == 0) { Ok(U256([self.0[0], self.0[1]])) }
        else { Err(Failure::Overflow) }
    }
}

impl I256 {
    pub const ZERO: Self = Self(U256::ZERO);
    pub const MIN: Self = Self(U256([0, 1u128 << 127]));
    pub const MAX: Self = Self(U256([u128::MAX, (1u128 << 127) - 1]));
    pub const fn from_bits(bits: U256) -> Self { Self(bits) }
    pub const fn bits(self) -> U256 { self.0 }
    pub const fn from_i128(x: i128) -> Self { Self(U256([x as u128, if x < 0 { u128::MAX } else { 0 }])) }
    pub fn is_negative(self) -> bool { self.0.0[1] >> 127 != 0 }
    fn negative_bits(x: U256) -> U256 {
        let (words, _) = add([!x.0[0], !x.0[1]], [1, 0]); U256(words)
    }
    pub fn magnitude(self) -> U256 {
        if self.is_negative() { Self::negative_bits(self.0) } else { self.0 }
    }
    pub fn from_magnitude(x: U256, negative: bool) -> Result<Self, Failure> {
        let bound = if negative { Self::MIN.0 } else { Self::MAX.0 };
        if x.cmp(bound) == Ordering::Greater { return Err(Failure::Overflow); }
        Ok(Self(if negative { Self::negative_bits(x) } else { x }))
    }
    pub fn try_unsigned(self) -> Result<U256, Failure> {
        if self.is_negative() { Err(Failure::Overflow) } else { Ok(self.0) }
    }
    /// A fitting signed delta need not require either unsigned price/amount to
    /// fit I256 separately. Form the exact magnitude before signed narrowing.
    pub fn difference_unsigned(a: U256, b: U256) -> Result<Self, Failure> {
        if a.cmp(b) == Ordering::Less { Self::from_magnitude(b.checked_sub(a)?, true) }
        else { Self::from_magnitude(a.checked_sub(b)?, false) }
    }
    pub fn cmp(self, other: Self) -> Ordering {
        match (self.is_negative(), other.is_negative()) {
            (true, false) => Ordering::Less, (false, true) => Ordering::Greater,
            _ => self.0.cmp(other.0),
        }
    }
    pub fn checked_neg(self) -> Result<Self, Failure> {
        if self == Self::MIN { Err(Failure::Overflow) } else { Ok(Self(Self::negative_bits(self.0))) }
    }
    pub fn checked_add(self, other: Self) -> Result<Self, Failure> {
        let (bits, _) = add(self.0.0, other.0.0); let result = Self(U256(bits));
        if self.is_negative() == other.is_negative() && result.is_negative() != self.is_negative() {
            Err(Failure::Overflow)
        } else { Ok(result) }
    }
    pub fn checked_sub(self, other: Self) -> Result<Self, Failure> {
        let (bits, _) = subtract(self.0.0, other.0.0); let result = Self(U256(bits));
        if self.is_negative() != other.is_negative() && result.is_negative() != self.is_negative() {
            Err(Failure::Overflow)
        } else { Ok(result) }
    }
    pub fn checked_mul(self, other: Self) -> Result<Self, Failure> {
        Self::from_magnitude(self.magnitude().checked_mul(other.magnitude())?, self.is_negative() != other.is_negative())
    }
    fn magnitude_round(mode: Rounding, negative: bool) -> Rounding {
        match (mode, negative) {
            (Rounding::Floor, true) | (Rounding::Ceil, false) => Rounding::Ceil,
            (Rounding::Exact, _) => Rounding::Exact, _ => Rounding::Floor,
        }
    }
    pub fn checked_div(self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        let negative = self.is_negative() != den.is_negative();
        let q = self.magnitude().checked_div(den.magnitude(), Self::magnitude_round(mode, negative))?;
        Self::from_magnitude(q, negative)
    }
    pub fn mul_div_positive(self, b: Self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        if den.is_negative() || den == Self::ZERO { return Err(Failure::Domain); }
        let negative = self.is_negative() != b.is_negative();
        let q = self.magnitude().mul_div(b.magnitude(), den.magnitude(), Self::magnitude_round(mode, negative))?;
        Self::from_magnitude(q, negative)
    }
    pub fn nonnegative_mul_div(self, b: Self, den: Self, mode: Rounding) -> Result<Self, Failure> {
        if self.is_negative() || b.is_negative() { return Err(Failure::Domain); }
        self.mul_div_positive(b, den, mode)
    }
    /// Raw signed*unsigned*signed ratio; scale/unit and payer/payee policies
    /// belong to a separately bound consumer, not this arithmetic helper.
    pub fn mul_unsigned_signed_div(self, b: U256, c: Self, den: U256, mode: Rounding) -> Result<Self, Failure> {
        let negative = self.is_negative() != c.is_negative();
        let q = self.magnitude().mul3_div(b, c.magnitude(), den, Self::magnitude_round(mode, negative))?;
        Self::from_magnitude(q, negative)
    }
    pub fn nonnegative_products_ge(a: [Self; 3], b: [Self; 3]) -> Result<bool, Failure> {
        for x in a.iter().chain(b.iter()) { if x.is_negative() { return Err(Failure::Domain); } }
        U256::products_ge(a.map(|x| x.0), b.map(|x| x.0))
    }
    pub fn try_i128(self) -> Result<i128, Failure> {
        let low = self.0.0[0] as i128;
        if self.0.0[1] == if low < 0 { u128::MAX } else { 0 } { Ok(low) }
        else { Err(Failure::Overflow) }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
