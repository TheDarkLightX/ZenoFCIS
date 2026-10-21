//! TroveManager 1352..1466 and LiquityMath 40..84, preserving every floor.
use crate::{Evaluation, Failure, Limits, Q, Work, add, muldiv, run, wide::Wide};
pub const FLOOR: u128 = 5_000_000_000_000_000;
pub const DECAY: u128 = 999_037_758_833_783_000;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct State {
    pub base: u128,
    pub last_epoch: u64,
}
/// Abstract reference-time input to this UNMOUNTED relation. This is neither a
/// clock profile nor an authenticated receipt. No epoch-to-minute map is chosen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogicalTime {
    pub epoch: u64,
    pub elapsed_minutes: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fee {
    pub state: State,
    pub amount: u128,
    pub rate: u128,
}
pub(crate) fn pow(mut x: u128, mut n: u64, w: &mut Work) -> Result<u128, Failure> {
    if x > Q {
        return Err(Failure::Domain);
    }
    n = n.min(525600000);
    if n == 0 {
        return Ok(Q);
    }
    let mut y = Q;
    fn rounded(a: u128, b: u128, w: &mut Work) -> Result<u128, Failure> {
        Wide::from(a)
            .mul(b, w)?
            .add(Wide::from(Q / 2), w)?
            .quotient(Wide::from(Q), w)
            .map(|(q, _)| q)
    }
    while n > 1 {
        w.charge(1, 0, 0)?;
        if n % 2 == 1 {
            y = rounded(x, y, w)?;
        }
        x = rounded(x, x, w)?;
        n /= 2;
    }
    rounded(x, y, w)
}
pub(crate) fn decayed(
    pre: State,
    time: Option<LogicalTime>,
    w: &mut Work,
) -> Result<State, Failure> {
    if pre.base > Q {
        return Err(Failure::Domain);
    }
    let t = time.ok_or(Failure::UnboundProfile)?;
    if t.epoch < pre.last_epoch || (t.epoch == pre.last_epoch && t.elapsed_minutes != 0) {
        return Err(Failure::Domain);
    }
    let base = muldiv(pre.base, pow(DECAY, t.elapsed_minutes, w)?, Q, w)?;
    Ok(State {
        base,
        last_epoch: if t.elapsed_minutes > 0 {
            t.epoch
        } else {
            pre.last_epoch
        },
    })
}
pub(crate) fn borrow_inner(
    pre: State,
    principal: u128,
    recovery: bool,
    max_fee: u128,
    time: Option<LogicalTime>,
    w: &mut Work,
) -> Result<Fee, Failure> {
    w.charge(1, 0, 0)?;
    if principal == 0 {
        return Err(Failure::Reject(1010));
    }
    borrow_domain(recovery, max_fee)?;
    if pre.base > Q {
        return Err(Failure::Domain);
    }
    if recovery {
        return Ok(Fee {
            state: pre,
            amount: 0,
            rate: 0,
        });
    }
    let state = decayed(pre, time, w)?;
    let rate = add(FLOOR, state.base, w)?.min(Q / 20);
    let amount = muldiv(rate, principal, Q, w)?;
    if muldiv(amount, Q, principal, w)? > max_fee {
        return Err(Failure::Reject(1052));
    }
    Ok(Fee {
        state,
        amount,
        rate,
    })
}
pub(crate) fn borrow_domain(recovery: bool, max_fee: u128) -> Result<(), Failure> {
    if max_fee > Q || (!recovery && max_fee < FLOOR) {
        return Err(Failure::Reject(1030));
    }
    Ok(())
}
pub fn borrow(
    pre: State,
    principal: u128,
    recovery: bool,
    max_fee: u128,
    time: Option<LogicalTime>,
    limits: Limits,
) -> Evaluation<Fee> {
    run(limits, |w| {
        borrow_inner(pre, principal, recovery, max_fee, time, w)
    })
}
pub(crate) fn redemption_inner(
    pre: State,
    gross: u128,
    price_e8: u128,
    system_debt: u128,
    max_fee: u128,
    time: Option<LogicalTime>,
    w: &mut Work,
) -> Result<Fee, Failure> {
    if !(FLOOR..=Q).contains(&max_fee) {
        return Err(Failure::Reject(1030));
    }
    if gross == 0 {
        return Err(Failure::Reject(1045));
    }
    if price_e8 == 0 || system_debt == 0 {
        return Err(Failure::Domain);
    }
    let mut state = decayed(pre, time, w)?;
    let fraction = Wide::from(gross)
        .mul(price_e8, w)?
        .mul(10_000_000_000, w)?
        .quotient(Wide::from(system_debt), w)?
        .0;
    state.base = add(state.base, fraction / 2, w)?.min(Q);
    if state.base == 0 {
        return Err(Failure::Reject(1050));
    }
    let rate = add(FLOOR, state.base, w)?.min(Q);
    let amount = muldiv(rate, gross, Q, w)?;
    if amount >= gross {
        return Err(Failure::Reject(1051));
    }
    if muldiv(amount, Q, gross, w)? > max_fee {
        return Err(Failure::Reject(1052));
    }
    Ok(Fee {
        state,
        amount,
        rate,
    })
}
pub fn redemption(
    pre: State,
    gross: u128,
    price_e8: u128,
    system_debt: u128,
    max_fee: u128,
    time: Option<LogicalTime>,
    limits: Limits,
) -> Evaluation<Fee> {
    run(limits, |w| {
        redemption_inner(pre, gross, price_e8, system_debt, max_fee, time, w)
    })
}
pub fn decay_power(minutes: u64, limits: Limits) -> Evaluation<u128> {
    run(limits, |w| pow(DECAY, minutes, w))
}
