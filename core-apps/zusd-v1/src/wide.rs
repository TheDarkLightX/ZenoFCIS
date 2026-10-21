//! Private exact 384-bit scratch. No persisted wide values or implicit downcasts.
use crate::{Failure, Work};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Wide(pub(crate) [u64; 6]);
impl From<u128> for Wide {
    fn from(x: u128) -> Self {
        Self([x as u64, (x >> 64) as u64, 0, 0, 0, 0])
    }
}
impl Wide {
    pub(crate) fn cmp(self, b: Self) -> core::cmp::Ordering {
        self.0.iter().rev().cmp(b.0.iter().rev())
    }
    pub(crate) fn add(self, b: Self, w: &mut Work) -> Result<Self, Failure> {
        w.charge(6, 0, 0)?;
        let mut out = [0; 6];
        let mut carry = 0u128;
        for (i, slot) in out.iter_mut().enumerate() {
            let sum = u128::from(self.0[i]) + u128::from(b.0[i]) + carry;
            *slot = sum as u64;
            carry = sum >> 64;
        }
        if carry != 0 {
            return Err(Failure::Arithmetic);
        }
        Ok(Self(out))
    }
    pub(crate) fn sub(self, b: Self, w: &mut Work) -> Result<Self, Failure> {
        w.charge(6, 0, 0)?;
        let mut out = [0; 6];
        let mut borrow = false;
        for (i, slot) in out.iter_mut().enumerate() {
            let (v, a) = self.0[i].overflowing_sub(b.0[i]);
            let (v, c) = v.overflowing_sub(u64::from(borrow));
            *slot = v;
            borrow = a || c;
        }
        if borrow {
            return Err(Failure::Arithmetic);
        }
        Ok(Self(out))
    }
    pub(crate) fn mul(self, b: u128, w: &mut Work) -> Result<Self, Failure> {
        w.charge(12, 0, 0)?;
        let mut out = [0u64; 6];
        for (j, factor) in [b as u64, (b >> 64) as u64].into_iter().enumerate() {
            let mut carry = 0u128;
            for i in 0..6 {
                let at = i + j;
                if at >= 6 {
                    if self.0[i] != 0 && factor != 0 {
                        return Err(Failure::Arithmetic);
                    }
                    continue;
                }
                // (2^64-1)^2 + two (2^64-1) terms fits U128 exactly.
                let value =
                    u128::from(self.0[i]) * u128::from(factor) + u128::from(out[at]) + carry;
                out[at] = value as u64;
                carry = value >> 64;
            }
            if carry != 0 {
                return Err(Failure::Arithmetic);
            }
        }
        Ok(Self(out))
    }
    pub(crate) fn narrow(self) -> Result<u128, Failure> {
        if self.0[2..].iter().any(|v| *v != 0) {
            return Err(Failure::Arithmetic);
        }
        Ok(u128::from(self.0[0]) | (u128::from(self.0[1]) << 64))
    }
    pub(crate) fn quotient(self, d: Self, w: &mut Work) -> Result<(u128, Self), Failure> {
        w.charge(1, 0, 0)?;
        if d == Self::from(0) {
            return Err(Failure::Arithmetic);
        }
        let mut q = 0u128;
        // Bounded 128-step search; overflowing trial product is necessarily too large.
        for shift in (0..128).rev() {
            let trial = q | (1u128 << shift);
            match d.mul(trial, w) {
                Ok(product) if product.cmp(self) != core::cmp::Ordering::Greater => q = trial,
                Ok(_) | Err(Failure::Arithmetic) => (),
                Err(e) => return Err(e),
            }
        }
        let remainder = self.sub(d.mul(q, w)?, w)?;
        if remainder.cmp(d) != core::cmp::Ordering::Less {
            return Err(Failure::Arithmetic);
        }
        Ok((q, remainder))
    }
}
