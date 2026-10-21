//! Private exact 384-bit scratch. Persisting a value requires checked narrowing.
use crate::{Accumulator256, Failure, Work};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Wide(pub(crate) [u64; 6]);
impl From<u128> for Wide {
    fn from(x: u128) -> Self {
        Self([x as u64, (x >> 64) as u64, 0, 0, 0, 0])
    }
}
impl From<Accumulator256> for Wide {
    fn from(x: Accumulator256) -> Self {
        let (high, low) = x.limbs();
        Self([
            low as u64,
            (low >> 64) as u64,
            high as u64,
            (high >> 64) as u64,
            0,
            0,
        ])
    }
}
impl Wide {
    pub(crate) fn accumulator(self) -> Result<Accumulator256, Failure> {
        if self.0[4..].iter().any(|v| *v != 0) {
            return Err(Failure::Arithmetic);
        }
        Ok(Accumulator256::from_limbs(
            u128::from(self.0[2]) | (u128::from(self.0[3]) << 64),
            u128::from(self.0[0]) | (u128::from(self.0[1]) << 64),
        ))
    }
    /// Binary long division. r<d before each iteration. The shifted remainder
    /// may use one carry bit; subtracting d then yields r<d within 384 bits.
    pub(crate) fn quotient_wide(self, d: Self, w: &mut Work) -> Result<(Self, Self), Failure> {
        w.charge(1, 0, 0)?;
        if d == Self::from(0) {
            return Err(Failure::Arithmetic);
        }
        let mut q = Self::from(0);
        let mut r = Self::from(0);
        for bit in (0..384).rev() {
            w.charge(19, 0, 0)?;
            let mut carry = (self.0[bit / 64] >> (bit % 64)) & 1;
            for limb in &mut r.0 {
                let next = *limb >> 63;
                *limb = (*limb << 1) | carry;
                carry = next;
            }
            if carry != 0 || r.cmp(d).is_ge() {
                let mut borrow = false;
                for (i, limb) in r.0.iter_mut().enumerate() {
                    let (a, b) = limb.overflowing_sub(d.0[i]);
                    let (a, c) = a.overflowing_sub(u64::from(borrow));
                    *limb = a;
                    borrow = b || c;
                }
                if borrow != (carry != 0) {
                    return Err(Failure::Arithmetic);
                }
                q.0[bit / 64] |= 1u64 << (bit % 64);
            }
        }
        Ok((q, r))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Limits;
    fn parse(text: &str) -> Wide {
        let mut out = [0; 6];
        for (i, limb) in out.iter_mut().enumerate() {
            *limb = u64::from_str_radix(&text[96 - 16 * (i + 1)..96 - 16 * i], 16).unwrap();
        }
        Wide(out)
    }
    #[test]
    fn full_width_division_matches_independent_bigints() {
        for row in include_str!("../tests/test-data/wide_division.tsv").lines() {
            let mut cells = row.split('\t');
            let n = parse(cells.next().unwrap());
            let d = parse(cells.next().unwrap());
            let q = cells.next().unwrap();
            let mut w = Work::new(Limits {
                steps: 100_000,
                reads: 0,
                bytes: 0,
            });
            if q == "refuse" {
                assert_eq!(n.quotient_wide(d, &mut w), Err(Failure::Arithmetic));
            } else {
                assert_eq!(
                    n.quotient_wide(d, &mut w),
                    Ok((parse(q), parse(cells.next().unwrap()))),
                    "{row}"
                );
            }
        }
    }
}
