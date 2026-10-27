//! Pure owner-vault relations. Sorted insertion, ledger publication and receipt
//! authentication are explicit composition obligations, not caller booleans.
use crate::{
    Accumulator256, Evaluation, Failure, Id, Limits, NICR_Q, Q, Work, add, fees, muldiv, run, sub,
    wide::Wide,
};
pub const MCR: u128 = 1_100_000_000_000_000_000;
pub const CCR: u128 = 1_500_000_000_000_000_000;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Parameters {
    pub version: u64,
    pub minimum_net: u128,
    pub reserve: u128,
    pub clock_profile: Option<Id>,
    pub drip_profile: Option<Id>,
}
impl Parameters {
    pub const fn v1() -> Self {
        Self {
            version: 1,
            minimum_net: 500 * Q,
            reserve: 50 * Q,
            clock_profile: None,
            drip_profile: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Active,
    ClosedOwner,
    ClosedLiquidation,
    ClosedRedemption,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Vault {
    pub parameter_version: u64,
    pub id: Id,
    pub owner: Id,
    pub status: Status,
    pub collateral: u128,
    pub debt: u128,
    pub reserve: u128,
    pub stake: u128,
    pub snapshot_c: Accumulator256,
    pub snapshot_d: Accumulator256,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct System {
    pub collateral: u128,
    pub debt: u128,
    pub active: u32,
    pub stakes: u128,
    pub stakes_snapshot: u128,
    pub collateral_snapshot: u128,
    pub reward_c: Accumulator256,
    pub reward_d: Accumulator256,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PriceStatus {
    Usable,
    Unavailable,
    Stale,
    Disputed,
    Uncertain,
    OverBudget,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Prices {
    pub committed: u128,
    pub pending: Option<u128>,
    pub status: PriceStatus,
    pub pending_blocked: bool,
}
impl Prices {
    pub(crate) fn usable(self) -> Result<(), Failure> {
        match self.status {
            PriceStatus::Usable => (),
            PriceStatus::Unavailable => return Err(Failure::Reject(1020)),
            PriceStatus::Stale => return Err(Failure::Reject(1021)),
            PriceStatus::Disputed => return Err(Failure::Reject(1022)),
            PriceStatus::Uncertain => return Err(Failure::Reject(1023)),
            PriceStatus::OverBudget => return Err(Failure::Reject(1024)),
        }
        if self.committed == 0 || self.pending == Some(0) {
            return Err(Failure::Domain);
        }
        Ok(())
    }
}
pub(crate) fn ratio(c: u128, d: u128, p: u128, w: &mut Work) -> Result<u128, Failure> {
    if d == 0 {
        return Ok(u128::MAX);
    }
    if p == 0 {
        return Err(Failure::Domain);
    }
    Wide::from(c)
        .mul(p, w)?
        .mul(10_000_000_000, w)?
        .quotient(Wide::from(d), w)
        .map(|(q, _)| q)
}
pub(crate) fn recovery(s: System, p: u128, w: &mut Work) -> Result<bool, Failure> {
    Ok(s.debt > 0 && ratio(s.collateral, s.debt, p, w)? < CCR)
}
pub(crate) fn effective(v: Vault, s: System, w: &mut Work) -> Result<Vault, Failure> {
    if v.status != Status::Active {
        return Err(Failure::Reject(1011));
    }
    let mut out = v;
    // D3: either lag is sufficient, including debt-only changes.
    out.collateral = add(
        v.collateral,
        Wide::from(s.reward_c.sub(v.snapshot_c, w)?)
            .mul(v.stake, w)?
            .quotient(Wide::from(Q), w)?
            .0,
        w,
    )?;
    out.debt = add(
        v.debt,
        Wide::from(s.reward_d.sub(v.snapshot_d, w)?)
            .mul(v.stake, w)?
            .quotient(Wide::from(Q), w)?
            .0,
        w,
    )?;
    out.snapshot_c = s.reward_c;
    out.snapshot_d = s.reward_d;
    Ok(out)
}
/// The principal guard must see the complete declared active inventory. This
/// cannot authenticate its root, but prevents omission within the native input.
pub(crate) fn inventory(rows: &[Vault], s: System, w: &mut Work) -> Result<(), Failure> {
    w.charge(
        rows.len() as u64,
        rows.len() as u64,
        rows.len() as u64 * 288,
    )?;
    if rows.windows(2).any(|pair| pair[0].id >= pair[1].id) {
        return Err(Failure::Canonical);
    }
    let mut n = 0u32;
    let mut c = Wide::from(0);
    let mut d = Wide::from(0);
    let mut stakes = Wide::from(0);
    for (i, v) in rows.iter().enumerate() {
        if v.status != Status::Active {
            continue;
        }
        w.charge(rows.len() as u64, rows.len() as u64, 0)?;
        if rows[i + 1..]
            .iter()
            .any(|other| other.status == Status::Active && other.owner == v.owner)
        {
            return Err(Failure::Domain);
        }
        n = n.checked_add(1).ok_or(Failure::Arithmetic)?;
        let entire = effective(*v, s, w)?;
        c = c.add(Wide::from(entire.collateral), w)?;
        d = d.add(Wide::from(entire.debt), w)?;
        stakes = stakes.add(Wide::from(v.stake), w)?;
    }
    if n != s.active
        || stakes != Wide::from(s.stakes)
        || c.cmp(Wide::from(s.collateral)).is_gt()
        || d.cmp(Wide::from(s.debt)).is_gt()
    {
        return Err(Failure::Conservation);
    }
    Ok(())
}
pub(crate) fn stake(c: u128, s: System, w: &mut Work) -> Result<u128, Failure> {
    if s.collateral_snapshot == 0 {
        Ok(c)
    } else {
        muldiv(c, s.stakes_snapshot, s.collateral_snapshot, w)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Delta {
    None,
    Add(u128),
    Remove(u128),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub vault: Vault,
    pub system: System,
    pub fee: fees::State,
    pub minted: u128,
    pub burned: u128,
    pub fee_paid: u128,
    pub collateral_in: u128,
    pub collateral_out: u128,
    pub reserve_minted: u128,
    pub reserve_burned: u128,
    /// Default-to-Active reclassification, derived inside this relation.
    pub pending_collateral: u128,
    pub pending_debt: u128,
    pub nicr: u128,
}
/// Immutable inputs for risk.
struct RiskInputs {
    vault: Vault,
    old: Option<Vault>,
    system: System,
    post_system: System,
    price: u128,
    withdrawing: bool,
    borrowing: bool,
    opening: bool,
}
fn risk(input: RiskInputs, w: &mut Work) -> Result<(), Failure> {
    let RiskInputs {
        vault: v,
        old,
        system: s,
        post_system: post,
        price: p,
        withdrawing: withdraw,
        borrowing: borrow,
        opening: open,
    } = input;
    let icr = ratio(v.collateral, v.debt, p, w)?;
    if recovery(s, p, w)? {
        if withdraw {
            return Err(Failure::Reject(1042));
        }
        if open || borrow {
            if icr < CCR {
                return Err(Failure::Reject(1043));
            }
            if let Some(o) = old
                && (icr < ratio(o.collateral, o.debt, p, w)?)
            {
                return Err(Failure::Reject(1043));
            }
        }
    } else if icr < MCR || ratio(post.collateral, post.debt, p, w)? < CCR {
        return Err(Failure::Reject(1043));
    }
    Ok(())
}
/// Immutable inputs for open.
pub struct OpenInputs {
    pub existing: Option<Vault>,
    pub id: Id,
    pub owner: Id,
    pub collateral: u128,
    pub principal: u128,
    pub balances: (u128, u128),
    pub system: System,
    pub parameters: Parameters,
    pub fee: fees::State,
    pub prices: Prices,
    pub max_fee: u128,
    pub time: Option<fees::LogicalTime>,
}
pub fn open(input: OpenInputs, limits: Limits) -> Evaluation<Outcome> {
    let OpenInputs {
        existing,
        id,
        owner,
        collateral,
        principal,
        balances,
        system: s,
        parameters: params,
        fee,
        prices,
        max_fee,
        time,
    } = input;
    run(limits, |w| {
        w.charge(1, 1, 288)?;
        if collateral == 0 || principal == 0 {
            return Err(Failure::Reject(1010));
        }
        if existing.is_some_and(|v| v.status == Status::Active) {
            return Err(Failure::Reject(1011));
        }
        if existing.is_some_and(|v| v.id != id || v.owner != owner) {
            return Err(Failure::Domain);
        }
        if params.minimum_net == 0 || params.reserve == 0 {
            return Err(Failure::Domain);
        }
        prices.usable()?;
        if prices.pending_blocked {
            return Err(Failure::Reject(1020));
        }
        let recovery = recovery(s, prices.committed, w)?;
        fees::borrow_domain(recovery, max_fee)?;
        if balances.0 < collateral {
            return Err(Failure::Reject(1033));
        }
        let f = fees::borrow_inner(fee, principal, recovery, Q, time, w)?;
        let net = add(principal, f.amount, w)?;
        if net < params.minimum_net {
            return Err(Failure::Reject(1040));
        }
        let debt = add(net, params.reserve, w)?;
        let v = Vault {
            parameter_version: params.version,
            id,
            owner,
            status: Status::Active,
            collateral,
            debt,
            reserve: params.reserve,
            stake: stake(collateral, s, w)?,
            snapshot_c: s.reward_c,
            snapshot_d: s.reward_d,
        };
        let post = System {
            collateral: add(s.collateral, collateral, w)?,
            debt: add(s.debt, debt, w)?,
            active: s.active.checked_add(1).ok_or(Failure::Arithmetic)?,
            stakes: add(s.stakes, v.stake, w)?,
            ..s
        };
        risk(
            RiskInputs {
                vault: v,
                old: None,
                system: s,
                post_system: post,
                price: prices.committed,
                withdrawing: false,
                borrowing: true,
                opening: true,
            },
            w,
        )?;
        if let Some(p) = prices.pending {
            risk(
                RiskInputs {
                    vault: v,
                    old: None,
                    system: s,
                    post_system: post,
                    price: p,
                    withdrawing: false,
                    borrowing: true,
                    opening: true,
                },
                w,
            )?;
        }
        if muldiv(f.amount, Q, principal, w)? > max_fee {
            return Err(Failure::Reject(1052));
        }
        let nicr = muldiv(collateral, NICR_Q, debt, w)?;
        if nicr == 0 {
            return Err(Failure::Arithmetic);
        }
        Ok(Outcome {
            vault: v,
            system: post,
            fee: f.state,
            minted: principal,
            burned: 0,
            fee_paid: f.amount,
            collateral_in: collateral,
            collateral_out: 0,
            reserve_minted: params.reserve,
            reserve_burned: 0,
            pending_collateral: 0,
            pending_debt: 0,
            nicr,
        })
    })
}
/// Immutable inputs for adjust.
pub struct AdjustInputs {
    pub vault: Vault,
    pub system: System,
    pub parameters: Parameters,
    pub fee: fees::State,
    pub prices: Prices,
    pub collateral_delta: Delta,
    pub debt_delta: Delta,
    pub wallet: (u128, u128),
    pub max_fee: u128,
    pub time: Option<fees::LogicalTime>,
}
pub fn adjust(input: AdjustInputs, limits: Limits) -> Evaluation<Outcome> {
    let AdjustInputs {
        vault: v,
        system: s,
        parameters: params,
        fee,
        prices,
        collateral_delta: coll,
        debt_delta: debt,
        wallet,
        max_fee,
        time,
    } = input;
    run(limits, |w| {
        w.charge(1, 1, 288)?;
        if coll == Delta::None && debt == Delta::None
            || matches!(coll, Delta::Add(0) | Delta::Remove(0))
            || matches!(debt, Delta::Add(0) | Delta::Remove(0))
        {
            return Err(Failure::Reject(1010));
        }
        let old = effective(v, s, w)?;
        let borrowing = matches!(debt, Delta::Add(_));
        let withdrawing = matches!(coll, Delta::Remove(_));
        let reducing = !borrowing && !withdrawing;
        let frozen = prices.status != PriceStatus::Usable;
        if !reducing || !frozen {
            prices.usable()?;
        }
        if (borrowing || withdrawing) && prices.pending_blocked {
            return Err(Failure::Reject(1020));
        }
        let recovery = if borrowing {
            let recovery = recovery(s, prices.committed, w)?;
            fees::borrow_domain(recovery, max_fee)?;
            recovery
        } else {
            false
        };
        let mut post_v = old;
        let mut c_in = 0;
        let mut c_out = 0;
        let mut mint = 0;
        let mut burn = 0;
        let mut f = fees::Fee {
            state: fee,
            rate: 0,
            amount: 0,
        };
        match coll {
            Delta::Add(x) => {
                if x > wallet.0 {
                    return Err(Failure::Reject(1033));
                }
                post_v.collateral = add(old.collateral, x, w)?;
                c_in = x;
            }
            Delta::Remove(x) => {
                if x > old.collateral {
                    return Err(Failure::Reject(1033));
                }
                post_v.collateral = sub(old.collateral, x, w)?;
                c_out = x;
            }
            Delta::None => (),
        }
        match debt {
            Delta::Add(x) => {
                f = fees::borrow_inner(fee, x, recovery, Q, time, w)?;
                post_v.debt = add(old.debt, add(x, f.amount, w)?, w)?;
                mint = x;
            }
            Delta::Remove(x) => {
                if x > wallet.1 {
                    return Err(Failure::Reject(1033));
                }
                if x > sub(old.debt, old.reserve, w)? {
                    return Err(Failure::Reject(1040));
                }
                post_v.debt = sub(old.debt, x, w)?;
                burn = x;
            }
            Delta::None => (),
        }
        if sub(post_v.debt, old.reserve, w)? < params.minimum_net {
            return Err(Failure::Reject(1040));
        }
        post_v.stake = stake(post_v.collateral, s, w)?;
        let post = System {
            collateral: sub(add(s.collateral, c_in, w)?, c_out, w)?,
            debt: sub(add(s.debt, add(mint, f.amount, w)?, w)?, burn, w)?,
            stakes: add(sub(s.stakes, v.stake, w)?, post_v.stake, w)?,
            ..s
        };
        if !frozen {
            risk(
                RiskInputs {
                    vault: post_v,
                    old: Some(old),
                    system: s,
                    post_system: post,
                    price: prices.committed,
                    withdrawing,
                    borrowing,
                    opening: false,
                },
                w,
            )?;
            if let Some(p) = prices.pending
                && (borrowing || withdrawing)
            {
                risk(
                    RiskInputs {
                        vault: post_v,
                        old: Some(old),
                        system: s,
                        post_system: post,
                        price: p,
                        withdrawing,
                        borrowing,
                        opening: false,
                    },
                    w,
                )?;
            }
        }
        if borrowing && muldiv(f.amount, Q, mint, w)? > max_fee {
            return Err(Failure::Reject(1052));
        }
        let nicr = muldiv(post_v.collateral, NICR_Q, post_v.debt, w)?;
        if nicr == 0 {
            return Err(Failure::Arithmetic);
        }
        Ok(Outcome {
            vault: post_v,
            system: post,
            fee: f.state,
            minted: mint,
            burned: burn,
            fee_paid: f.amount,
            collateral_in: c_in,
            collateral_out: c_out,
            reserve_minted: 0,
            reserve_burned: 0,
            pending_collateral: sub(old.collateral, v.collateral, w)?,
            pending_debt: sub(old.debt, v.debt, w)?,
            nicr,
        })
    })
}
pub fn close(
    v: Vault,
    s: System,
    fee: fees::State,
    prices: Prices,
    wallet_zusd: u128,
    limits: Limits,
) -> Evaluation<Outcome> {
    run(limits, |w| {
        let old = effective(v, s, w)?;
        prices.usable()?;
        if prices.pending_blocked {
            return Err(Failure::Reject(1020));
        }
        let net = sub(old.debt, old.reserve, w)?;
        if wallet_zusd < net {
            return Err(Failure::Reject(1033));
        }
        if s.active <= 1 {
            return Err(Failure::Reject(1041));
        }
        let post = System {
            collateral: sub(s.collateral, old.collateral, w)?,
            debt: sub(s.debt, old.debt, w)?,
            active: s.active - 1,
            stakes: sub(s.stakes, old.stake, w)?,
            ..s
        };
        for p in [Some(prices.committed), prices.pending]
            .into_iter()
            .flatten()
        {
            if recovery(s, p, w)? {
                return Err(Failure::Reject(1042));
            }
            if ratio(post.collateral, post.debt, p, w)? < CCR {
                return Err(Failure::Reject(1043));
            }
        }
        let closed = Vault {
            status: Status::ClosedOwner,
            collateral: 0,
            debt: 0,
            reserve: 0,
            stake: 0,
            snapshot_c: Accumulator256::ZERO,
            snapshot_d: Accumulator256::ZERO,
            ..old
        };
        Ok(Outcome {
            vault: closed,
            system: post,
            fee,
            minted: 0,
            burned: net,
            fee_paid: 0,
            collateral_in: 0,
            collateral_out: old.collateral,
            reserve_minted: 0,
            reserve_burned: old.reserve,
            pending_collateral: sub(old.collateral, v.collateral, w)?,
            pending_debt: sub(old.debt, v.debt, w)?,
            nicr: 0,
        })
    })
}
pub fn apply_pending(v: Vault, s: System, limits: Limits) -> Evaluation<Vault> {
    run(limits, |w| effective(v, s, w))
}
