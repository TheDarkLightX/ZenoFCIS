//! Independent complete relations for native scalar/record subjects. These
//! check all outcomes, not producer-selected cases or a conformance law 991.
//! The full collection/effect A63 binding remains a separate integration gate.
use crate::{
    Evaluation, Failure, Limits, Q, Work, add,
    liquidation::Partition,
    muldiv, run, sub,
    vaults::{CCR, MCR, PriceStatus, Prices, Status, System, Vault},
    wide::Wide,
};
fn expected_pending(v: Vault, s: System, w: &mut Work) -> Result<Vault, Failure> {
    if v.status != Status::Active {
        return Err(Failure::Reject(1011));
    }
    let delta_c = sub(s.reward_c, v.snapshot_c, w)?;
    let delta_d = sub(s.reward_d, v.snapshot_d, w)?;
    Ok(Vault {
        collateral: add(v.collateral, muldiv(v.stake, delta_c, Q, w)?, w)?,
        debt: add(v.debt, muldiv(v.stake, delta_d, Q, w)?, w)?,
        snapshot_c: s.reward_c,
        snapshot_d: s.reward_d,
        ..v
    })
}
fn ratio(c: u128, d: u128, p: u128, w: &mut Work) -> Result<u128, Failure> {
    if d == 0 {
        return Ok(u128::MAX);
    }
    Wide::from(c)
        .mul(p, w)?
        .mul(10_000_000_000, w)?
        .quotient(Wide::from(d), w)
        .map(|(q, _)| q)
}
fn expected_quote(
    v: Vault,
    s: System,
    p: Prices,
    available: u128,
    w: &mut Work,
) -> Result<Partition, Failure> {
    if v.status != Status::Active {
        return Err(Failure::Reject(1011));
    }
    let reason = match p.status {
        PriceStatus::Usable => None,
        PriceStatus::Unavailable => Some(1020),
        PriceStatus::Stale => Some(1021),
        PriceStatus::Disputed => Some(1022),
        PriceStatus::Uncertain => Some(1023),
        PriceStatus::OverBudget => Some(1024),
    };
    if let Some(r) = reason {
        return Err(Failure::Reject(r));
    }
    if p.committed == 0 || p.pending == Some(0) {
        return Err(Failure::Domain);
    }
    if s.active <= 1 {
        return Err(Failure::Reject(1041));
    }
    let v = expected_pending(v, s, w)?;
    let icr = ratio(v.collateral, v.debt, p.committed, w)?;
    let tcr = ratio(s.collateral, s.debt, p.committed, w)?;
    let recovery = s.debt > 0 && tcr < CCR;
    let eligible = if recovery {
        icr < MCR || (icr < tcr && available >= v.debt)
    } else {
        icr < MCR
    };
    if !eligible {
        return Err(Failure::Reject(1044));
    }
    let capped = recovery && icr >= MCR;
    let seized = if capped {
        Wide::from(v.debt)
            .mul(MCR, w)?
            .quotient(Wide::from(p.committed).mul(10_000_000_000, w)?, w)?
            .0
    } else {
        v.collateral
    };
    let keeper = seized / 200;
    let offset = if capped {
        v.debt
    } else if recovery && icr <= Q {
        0
    } else {
        available.min(v.debt)
    };
    let distributable = sub(seized, keeper, w)?;
    let sp = if capped {
        distributable
    } else {
        muldiv(distributable, offset, v.debt, w)?
    };
    Ok(Partition {
        debt: v.debt,
        collateral: v.collateral,
        offset,
        redistributed_debt: sub(v.debt, offset, w)?,
        sp_collateral: sp,
        redistributed_collateral: if capped {
            0
        } else {
            sub(distributable, sp, w)?
        },
        keeper_collateral: keeper,
        surplus: sub(v.collateral, seized, w)?,
        reserve: v.reserve,
    })
}
pub fn check_pending(
    v: Vault,
    s: System,
    candidate: Result<Vault, Failure>,
    limits: Limits,
) -> Evaluation<()> {
    run(limits, |w| {
        let reference = expected_pending(v, s, w);
        if let Err(e) = reference {
            if !matches!(e, Failure::Reject(_)) {
                return Err(e);
            }
        }
        if reference != candidate {
            return Err(Failure::Law(2000));
        }
        Ok(())
    })
}
pub fn check_quote(
    v: Vault,
    s: System,
    p: Prices,
    available: u128,
    candidate: Result<Partition, Failure>,
    limits: Limits,
) -> Evaluation<()> {
    run(limits, |w| {
        let reference = expected_quote(v, s, p, available, w);
        if let Err(e) = reference {
            if !matches!(e, Failure::Reject(_)) {
                return Err(e);
            }
        }
        if reference != candidate {
            return Err(Failure::Law(2001));
        }
        Ok(())
    })
}
