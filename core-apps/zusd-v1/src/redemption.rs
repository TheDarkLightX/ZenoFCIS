//! Native redemption over bytes32 vault identities. No epoch conversion is selected.
use crate::{
    Accumulator256, Evaluation, Failure, Id, Limits, NICR_Q, Q, Work, add, fees,
    liquidation::{self, Book},
    muldiv, run, stability, sub,
    vaults::{self, MCR, Parameters, Prices, Status, Vault},
};
use alloc::vec::Vec;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Command {
    pub maximum: u128,
    pub visits: u32,
    pub first: Option<Id>,
    pub partial_nicr: u128,
    pub upper: Option<Id>,
    pub lower: Option<Id>,
    pub max_fee: u128,
    pub min_out: u128,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Holder {
    pub zusd: u128,
    pub agrs: u128,
    pub staking_agrs: u128,
    pub fee: fees::State,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stop {
    Filled,
    VisitCap,
    CancelledPartial,
    End,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Trace {
    pub visited: Vec<Id>,
    pub closed: Vec<Id>,
    pub settled: Vec<Id>,
    pub executed: u128,
    pub unfilled: u128,
    pub gross: u128,
    pub reserve_burn: u128,
    pub surplus: u128,
    pub fee: u128,
    pub stop: Stop,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub book: Book,
    pub holder: Holder,
    pub trace: Trace,
}
pub(crate) fn cut(
    book: &mut Book,
    id: Id,
    priority: u128,
    upper: Option<Id>,
    lower: Option<Id>,
    w: &mut Work,
) -> Result<(), Failure> {
    book.order.retain(|x| *x != id);
    let n = book.order.len();
    let find = |hint: Option<Id>| hint.and_then(|v| book.order.iter().position(|x| *x == v));
    let mut u = find(upper);
    let mut l = find(lower);
    let valid =
        |u: Option<usize>, l: Option<usize>, w: &mut Work| -> Result<Option<usize>, Failure> {
            match (u, l) {
                (None, None) => Ok(if n == 0 { Some(0) } else { None }),
                (None, Some(j)) => Ok(
                    if j == 0 && priority >= liquidation::fresh(book, book.order[j], w)? {
                        Some(0)
                    } else {
                        None
                    },
                ),
                (Some(i), None) => Ok(
                    if i + 1 == n && priority <= liquidation::fresh(book, book.order[i], w)? {
                        Some(n)
                    } else {
                        None
                    },
                ),
                (Some(i), Some(j)) => Ok(
                    if i + 1 == j
                        && liquidation::fresh(book, book.order[i], w)? >= priority
                        && priority >= liquidation::fresh(book, book.order[j], w)?
                    {
                        Some(j)
                    } else {
                        None
                    },
                ),
            }
        };
    // V1 tests the original hints before discarding missing/self anchors.
    let original_present = (upper.is_none() || u.is_some()) && (lower.is_none() || l.is_some());
    if let Some(at) = if original_present {
        valid(u, l, w)?
    } else {
        None
    } {
        book.order.insert(at, id);
        return Ok(());
    }
    if let Some(i) = u {
        if priority > liquidation::fresh(book, book.order[i], w)? {
            u = None;
        }
    }
    if let Some(j) = l {
        if priority < liquidation::fresh(book, book.order[j], w)? {
            l = None;
        }
    }
    if u.is_none() && l.is_some() {
        let mut j = l.ok_or(Failure::Domain)?;
        if j + 1 == n && priority <= liquidation::fresh(book, book.order[j], w)? {
            book.order.push(id);
            return Ok(());
        }
        loop {
            w.charge(1, 1, 64)?;
            if let Some(at) = valid(j.checked_sub(1), Some(j), w)? {
                book.order.insert(at, id);
                return Ok(());
            }
            if j == 0 {
                return Err(Failure::Domain);
            }
            j -= 1;
        }
    }
    let mut i = u.unwrap_or(0);
    if n == 0 || (i == 0 && priority >= liquidation::fresh(book, book.order[0], w)?) {
        book.order.insert(0, id);
        return Ok(());
    }
    loop {
        w.charge(1, 1, 64)?;
        let next = if i + 1 < n { Some(i + 1) } else { None };
        if let Some(at) = valid(Some(i), next, w)? {
            book.order.insert(at, id);
            return Ok(());
        }
        i += 1;
        if i >= n {
            return Err(Failure::Domain);
        }
    }
}
pub fn redeem(
    pre: &Book,
    holder: Holder,
    command: Command,
    prices: Prices,
    time: Option<fees::LogicalTime>,
    bootstrap_minutes: Option<u64>,
    params: Parameters,
    capacity: u32,
    bounds: stability::Bounds,
    limits: Limits,
) -> Evaluation<Outcome> {
    run(limits, |w| {
        liquidation::validate(pre, capacity, bounds, w)?;
        if command.maximum == 0 {
            return Err(Failure::Reject(1010));
        }
        prices.usable()?;
        if !(fees::FLOOR..=Q).contains(&command.max_fee) {
            return Err(Failure::Reject(1030));
        }
        if bootstrap_minutes.ok_or(Failure::UnboundProfile)? < 20160 {
            return Err(Failure::Reject(1031));
        }
        if vaults::ratio(pre.system.collateral, pre.system.debt, prices.committed, w)? < MCR {
            return Err(Failure::Reject(1032));
        }
        if holder.zusd < command.maximum {
            return Err(Failure::Reject(1033));
        }
        w.charge(
            1,
            0,
            pre.vaults.len() as u64 * 768 + pre.pool.history.len() as u64 * 64 + 512,
        )?;
        let mut first = None;
        for id in pre.order.iter().rev() {
            w.charge(1, 1, 32)?;
            let i = liquidation::index(pre, *id, w)?.ok_or(Failure::Domain)?;
            let v = vaults::effective(pre.vaults[i], pre.system, w)?;
            if vaults::ratio(v.collateral, v.debt, prices.committed, w)? >= MCR {
                first = Some(*id);
                break;
            }
        }
        let mut book = pre.clone();
        let mut remaining = command.maximum;
        let mut gross = 0;
        let mut reserves = 0;
        let mut surplus = 0;
        let mut executed = 0;
        let mut visited = Vec::new();
        let mut closed = Vec::new();
        let mut settled = Vec::new();
        let mut cursor = first;
        let cap = if command.visits == 0 {
            pre.system.active
        } else {
            command.visits.min(pre.system.active)
        };
        let mut stop = Stop::End;
        while let Some(id) = cursor {
            if remaining == 0 {
                stop = Stop::Filled;
                break;
            }
            if visited.len() >= cap as usize {
                stop = Stop::VisitCap;
                break;
            }
            let at = book
                .order
                .iter()
                .position(|x| *x == id)
                .ok_or(Failure::Domain)?;
            cursor = at.checked_sub(1).map(|i| book.order[i]);
            visited.push(id);
            w.charge(1, 1, 32)?;
            let i = liquidation::index(&book, id, w)?.ok_or(Failure::Domain)?;
            let v = vaults::effective(book.vaults[i], book.system, w)?;
            if vaults::ratio(v.collateral, v.debt, prices.committed, w)? < MCR {
                continue;
            }
            book.default_collateral = sub(
                book.default_collateral,
                sub(v.collateral, book.vaults[i].collateral, w)?,
                w,
            )?;
            book.default_debt = sub(book.default_debt, sub(v.debt, book.vaults[i].debt, w)?, w)?;
            book.vaults[i] = v;
            settled.push(id);
            let net = sub(v.debt, v.reserve, w)?;
            let x = remaining.min(net);
            let g = muldiv(x, 100_000_000, prices.committed, w)?;
            let c = sub(v.collateral, g, w)?;
            let d = sub(v.debt, x, w)?;
            if x == net {
                if book.system.active <= 1 {
                    return Err(Failure::Reject(1041));
                }
                book.vaults[i] = Vault {
                    status: Status::ClosedRedemption,
                    collateral: 0,
                    debt: 0,
                    reserve: 0,
                    stake: 0,
                    snapshot_c: Accumulator256::ZERO,
                    snapshot_d: Accumulator256::ZERO,
                    ..v
                };
                book.order.remove(at);
                book.system.active -= 1;
                book.system.stakes = sub(book.system.stakes, v.stake, w)?;
                book.system.collateral = sub(book.system.collateral, v.collateral, w)?;
                book.system.debt = sub(book.system.debt, v.debt, w)?;
                book.gas_balance = sub(book.gas_balance, v.reserve, w)?;
                reserves = add(reserves, v.reserve, w)?;
                surplus = add(surplus, c, w)?;
                let pos = book.surplus.binary_search_by_key(&v.owner, |x| x.0);
                match pos {
                    Ok(j) => book.surplus[j].1 = add(book.surplus[j].1, c, w)?,
                    Err(j) if c > 0 => {
                        if book.surplus.len() >= capacity as usize {
                            return Err(Failure::Reject(1061));
                        }
                        book.surplus.insert(j, (v.owner, c));
                    }
                    _ => (),
                }
                closed.push(id);
            } else {
                let p = muldiv(c, NICR_Q, d, w)?;
                if sub(d, v.reserve, w)? < params.minimum_net || p != command.partial_nicr {
                    stop = Stop::CancelledPartial;
                    break;
                }
                let stake = vaults::stake(c, book.system, w)?;
                book.vaults[i] = Vault {
                    collateral: c,
                    debt: d,
                    stake,
                    ..v
                };
                book.system.stakes = add(sub(book.system.stakes, v.stake, w)?, stake, w)?;
                book.system.collateral = sub(book.system.collateral, g, w)?;
                book.system.debt = sub(book.system.debt, x, w)?;
                cut(&mut book, id, p, command.upper, command.lower, w)?;
            }
            gross = add(gross, g, w)?;
            executed = add(executed, x, w)?;
            remaining = sub(remaining, x, w)?;
            if remaining == 0 {
                stop = Stop::Filled;
                break;
            }
        }
        if executed == 0 || gross == 0 {
            return Err(Failure::Reject(1045));
        }
        let fee = fees::redemption_inner(
            holder.fee,
            gross,
            prices.committed,
            pre.system.debt,
            command.max_fee,
            time,
            w,
        )?;
        let net = sub(gross, fee.amount, w)?;
        if net < command.min_out {
            return Err(Failure::Reject(1053));
        }
        let holder = Holder {
            zusd: sub(holder.zusd, executed, w)?,
            agrs: add(holder.agrs, net, w)?,
            staking_agrs: add(holder.staking_agrs, fee.amount, w)?,
            fee: fee.state,
        };
        let mut priorities = Vec::new();
        for v in &book.vaults {
            if v.status == Status::Active {
                priorities.push((v.id, liquidation::fresh(&book, v.id, w)?));
            }
        }
        book.priorities = priorities;
        liquidation::validate(&book, capacity, bounds, w)?;
        if sub(pre.system.debt, book.system.debt, w)? != add(executed, reserves, w)?
            || sub(pre.system.collateral, book.system.collateral, w)? != add(gross, surplus, w)?
        {
            return Err(Failure::Conservation);
        }
        Ok(Outcome {
            book,
            holder,
            trace: Trace {
                visited,
                closed,
                settled,
                executed,
                unfilled: remaining,
                gross,
                reserve_burn: reserves,
                surplus,
                fee: fee.amount,
                stop,
            },
        })
    })
}
