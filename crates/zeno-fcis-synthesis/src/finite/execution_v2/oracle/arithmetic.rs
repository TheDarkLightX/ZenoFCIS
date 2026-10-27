use super::types::{PriceE8, StressE8, UsdExposureE18, ZdexE18};
use super::{Evaluation, Failure, Limits, Meter, measured, step};
use crate::finite::value::wide;

/// Exact median, deviation and observation-time statistics for three reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AggregateNumbers {
    /// Median of the three positive prices in E8 units.
    pub median: PriceE8,
    /// Maximum absolute deviation from the median in E8 price units.
    pub confidence: u128,
    /// Maximum relative price deviation in basis points.
    pub deviation_bps: u128,
    /// Oldest selected observation epoch.
    pub oldest_epoch: u64,
    /// Median observation timestamp in seconds.
    pub median_seconds: u64,
}

/// Exact unsigned reductions, including ceil-only overflow; no report admission
/// or price truth is inferred from this arithmetic result.
pub fn median3(
    values: [PriceE8; 3],
    epochs: [u64; 3],
    seconds: [u64; 3],
    limits: Limits,
) -> Evaluation<AggregateNumbers> {
    measured(limits, |meter| numbers(values, epochs, seconds, meter))
}
pub(super) fn numbers(
    values: [PriceE8; 3],
    epochs: [u64; 3],
    mut seconds: [u64; 3],
    m: &mut Meter,
) -> Result<AggregateNumbers, Failure> {
    let mut xs = values.map(PriceE8::raw);
    // Fixed sorting network: three comparisons for each median, independent of data.
    for (i, j) in [(0, 1), (1, 2), (0, 1)] {
        step(m)?;
        if xs[i] > xs[j] {
            xs.swap(i, j);
        }
        step(m)?;
        if seconds[i] > seconds[j] {
            seconds.swap(i, j);
        }
    }
    step(m)?;
    let confidence = (xs[1] - xs[0]).max(xs[2] - xs[1]);
    step(m)?;
    let deviation_bps = wide::mul_div_ceil(confidence, 10_000, xs[1]).ok_or(Failure::Arithmetic)?;
    step(m)?;
    Ok(AggregateNumbers {
        median: PriceE8::new(xs[1])?,
        confidence,
        deviation_bps,
        oldest_epoch: epochs[0].min(epochs[1]).min(epochs[2]),
        median_seconds: seconds[1],
    })
}

/// floor(S*P*a/(10^8*b)) with a full 384-bit numerator. The quotient must fit
/// U128; a larger result refuses instead of saturating or narrowing.
pub fn security_capacity(
    stake: ZdexE18,
    stress: StressE8,
    a: u128,
    b: u128,
    limits: Limits,
) -> Evaluation<UsdExposureE18> {
    measured(limits, |m| capacity(stake, stress, a, b, m))
}
pub(super) fn capacity(
    stake: ZdexE18,
    stress: StressE8,
    a: u128,
    b: u128,
    m: &mut Meter,
) -> Result<UsdExposureE18, Failure> {
    pair_capacity([stake, ZdexE18(0)], stress, a, b, m)
}
/// floor((s0+s1)*P*a/(10^8*b)) for a full-width two-reporter coalition. The
/// stake sum may need 129 bits and the numerator 385, so the two 384-bit
/// products are added into four limbs; only the final quotient is checked.
pub(super) fn pair_capacity(
    stakes: [ZdexE18; 2],
    stress: StressE8,
    a: u128,
    b: u128,
    m: &mut Meter,
) -> Result<UsdExposureE18, Failure> {
    step(m)?;
    if a == 0 || a >= b {
        return Err(Failure::InvalidProfile);
    }
    let numerator = sum(
        wide::multiply_three(stakes[0].0, stress.0, a),
        wide::multiply_three(stakes[1].0, stress.0, a),
    );
    let denominator = wide::multiply(b, 100_000_000);
    // denominator * 2^128 still fits three limbs; equality already overflows U128.
    let overflow = [0, denominator[0], denominator[1], 0];
    step(m)?;
    if compare(numerator, overflow) != core::cmp::Ordering::Less {
        return Err(Failure::Arithmetic);
    }
    let mut quotient = 0u128;
    for shift in (0..128).rev() {
        step(m)?;
        let trial = quotient | (1u128 << shift);
        let [low, middle, high] = wide::multiply_three(trial, b, 100_000_000);
        if compare([low, middle, high, 0], numerator) != core::cmp::Ordering::Greater {
            quotient = trial;
        }
    }
    Ok(UsdExposureE18(quotient))
}
/// Exact four-limb sum of two three-limb values, least significant limb first.
fn sum(x: [u128; 3], y: [u128; 3]) -> [u128; 4] {
    let mut out = [0; 4];
    let mut carry = false;
    for i in 0..3 {
        let (partial, first) = x[i].overflowing_add(y[i]);
        let (total, second) = partial.overflowing_add(u128::from(carry));
        out[i] = total;
        carry = first || second;
    }
    out[3] = u128::from(carry);
    out
}
fn compare(a: [u128; 4], b: [u128; 4]) -> core::cmp::Ordering {
    (a[3], a[2], a[1], a[0]).cmp(&(b[3], b[2], b[1], b[0]))
}
