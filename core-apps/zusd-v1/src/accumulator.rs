//! App-local V1 uint256 accumulators. Amounts and final payouts remain U128.
use crate::{Evaluation, Failure, Limits, Work, run, wide::Wide};
use alloc::vec;
use zeno_fcis_value::{Value, ValueRef};

/// Canonical high/low U128 limbs; no mutable interior or escaping scratch.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Accumulator256 {
    high: u128,
    low: u128,
}
impl From<u128> for Accumulator256 {
    fn from(low: u128) -> Self {
        Self { high: 0, low }
    }
}
impl Accumulator256 {
    pub const ZERO: Self = Self::from_limbs(0, 0);
    pub const MAX: Self = Self::from_limbs(u128::MAX, u128::MAX);
    pub const fn from_limbs(high: u128, low: u128) -> Self {
        Self { high, low }
    }
    pub const fn limbs(self) -> (u128, u128) {
        (self.high, self.low)
    }
    pub fn to_be_bytes(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[..16].copy_from_slice(&self.high.to_be_bytes());
        bytes[16..].copy_from_slice(&self.low.to_be_bytes());
        bytes
    }
    pub fn from_be_bytes(bytes: [u8; 32]) -> Self {
        let mut high = [0; 16];
        let mut low = [0; 16];
        high.copy_from_slice(&bytes[..16]);
        low.copy_from_slice(&bytes[16..]);
        Self::from_limbs(u128::from_be_bytes(high), u128::from_be_bytes(low))
    }
    /// Schema representation is the ordered tuple (high, low), both exact U128.
    pub fn to_value(self) -> Result<Value, Failure> {
        Value::tuple(vec![Value::unsigned(self.high), Value::unsigned(self.low)])
            .map_err(|_| Failure::Canonical)
    }
    pub fn from_value(value: &Value) -> Result<Self, Failure> {
        match value.view() {
            ValueRef::Tuple(items) if items.len() == 2 => {
                match (items[0].view(), items[1].view()) {
                    (ValueRef::U128(high), ValueRef::U128(low)) => Ok(Self::from_limbs(high, low)),
                    _ => Err(Failure::Canonical),
                }
            }
            _ => Err(Failure::Canonical),
        }
    }
    pub fn checked_add(self, other: Self, limits: Limits) -> Evaluation<Self> {
        run(limits, |w| self.add(other, w))
    }
    /// Exact 384-bit product; only the final quotient is narrowed to U128.
    pub fn multiply_divide(self, amount: u128, divisor: u128, limits: Limits) -> Evaluation<u128> {
        run(limits, |w| {
            Wide::from(self)
                .mul(amount, w)?
                .quotient(Wide::from(divisor), w)
                .map(|x| x.0)
        })
    }
    pub(crate) fn add(self, other: Self, w: &mut Work) -> Result<Self, Failure> {
        Wide::from(self).add(Wide::from(other), w)?.accumulator()
    }
    pub(crate) fn sub(self, other: Self, w: &mut Work) -> Result<Self, Failure> {
        Wide::from(self).sub(Wide::from(other), w)?.accumulator()
    }
}
