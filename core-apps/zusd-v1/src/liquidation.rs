//! Source-shaped liquidation branches and batch transition with D2/D3/D4.
use crate::{
    Accumulator256, Evaluation, Failure, Id, Limits, NICR_Q, Q, Work, add, muldiv, run, stability,
    sub,
    vaults::{self, MCR, Prices, Status, System, Vault},
    wide::Wide,
};
use alloc::vec::Vec;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Partition {
    pub debt: u128,
    pub collateral: u128,
    pub offset: u128,
    pub redistributed_debt: u128,
    pub sp_collateral: u128,
    pub redistributed_collateral: u128,
    pub keeper_collateral: u128,
    pub surplus: u128,
    pub reserve: u128,
}
pub(crate) fn partition(
    v: Vault,
    system: System,
    price: u128,
    available: u128,
    mode: bool,
    w: &mut Work,
) -> Result<Option<Partition>, Failure> {
    let icr = vaults::ratio(v.collateral, v.debt, price, w)?;
    let tcr = vaults::ratio(system.collateral, system.debt, price, w)?;
    let mut out = Partition {
        debt: v.debt,
        collateral: v.collateral,
        offset: 0,
        redistributed_debt: 0,
        sp_collateral: 0,
        redistributed_collateral: 0,
        keeper_collateral: 0,
        surplus: 0,
        reserve: v.reserve,
    };
    if mode && system.active <= 1 {
        return Ok(None);
    }
    if (!mode && icr >= MCR) || (mode && icr >= MCR && (icr >= tcr || available < v.debt)) {
        return Ok(None);
    }
    if mode && icr >= MCR {
        let denominator = Wide::from(price).mul(10_000_000_000, w)?;
        let cap = Wide::from(v.debt).mul(MCR, w)?.quotient(denominator, w)?.0;
        out.keeper_collateral = cap / 200;
        out.sp_collateral = sub(cap, out.keeper_collateral, w)?;
        out.offset = v.debt;
        out.surplus = sub(v.collateral, cap, w)?;
    } else {
        out.keeper_collateral = v.collateral / 200;
        let c = sub(v.collateral, out.keeper_collateral, w)?;
        if !mode || icr > Q {
            out.offset = v.debt.min(available);
            out.sp_collateral = muldiv(c, out.offset, v.debt, w)?;
        }
        out.redistributed_debt = sub(v.debt, out.offset, w)?;
        out.redistributed_collateral = sub(c, out.sp_collateral, w)?;
    }
    // Independent partition equations; no reserve enters the offset twice.
    if add(out.offset, out.redistributed_debt, w)? != out.debt
        || add(
            add(out.sp_collateral, out.redistributed_collateral, w)?,
            add(out.keeper_collateral, out.surplus, w)?,
            w,
        )? != out.collateral
    {
        return Err(Failure::Conservation);
    }
    Ok(Some(out))
}
pub fn quote(
    v: Vault,
    system: System,
    prices: Prices,
    available: u128,
    limits: Limits,
) -> Evaluation<Partition> {
    run(limits, |w| {
        if v.status != Status::Active {
            return Err(Failure::Reject(1011));
        }
        prices.usable()?;
        if system.active <= 1 {
            return Err(Failure::Reject(1041));
        }
        let v = vaults::effective(v, system, w)?;
        partition(
            v,
            system,
            prices.committed,
            available,
            vaults::recovery(system, prices.committed, w)?,
            w,
        )?
        .ok_or(Failure::Reject(1044))
    })
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Book {
    pub vaults: Vec<Vault>,
    pub order: Vec<Id>,
    pub priorities: Vec<(Id, u128)>,
    pub system: System,
    pub default_collateral: u128,
    pub default_debt: u128,
    pub error_c: Accumulator256,
    pub error_d: Accumulator256,
    pub gas_balance: u128,
    pub surplus: Vec<(Id, u128)>,
    pub pool: stability::State,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    Direct(Id),
    Lowest(u32),
    Batch(Vec<Id>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub book: Book,
    pub selected: Vec<Id>,
    pub partitions: Vec<Partition>,
    pub keeper_zusd: u128,
    pub keeper_collateral: u128,
    pub burned: u128,
    pub redistributed_debt: u128,
    pub redistributed_collateral: u128,
}
pub(crate) fn index(book: &Book, id: Id, w: &mut Work) -> Result<Option<usize>, Failure> {
    w.charge(book.vaults.len() as u64, book.vaults.len() as u64, 0)?;
    Ok(book.vaults.binary_search_by_key(&id, |v| v.id).ok())
}
pub(crate) fn fresh(book: &Book, id: Id, w: &mut Work) -> Result<u128, Failure> {
    let i = index(book, id, w)?.ok_or(Failure::Domain)?;
    let v = vaults::effective(book.vaults[i], book.system, w)?;
    let p = muldiv(v.collateral, NICR_Q, v.debt, w)?;
    if p == 0 {
        return Err(Failure::Arithmetic);
    }
    Ok(p)
}
pub(crate) fn validate(
    book: &Book,
    capacity: u32,
    b: stability::Bounds,
    w: &mut Work,
) -> Result<(), Failure> {
    if book.vaults.len() > capacity as usize
        || book.order.len() > capacity as usize
        || book.priorities.len() > capacity as usize
        || book.surplus.len() > capacity as usize
    {
        return Err(Failure::Capacity);
    }
    w.charge(
        book.vaults.len() as u64,
        book.vaults.len() as u64,
        book.vaults.len() as u64 * 288
            + book.order.len() as u64 * 64
            + book.surplus.len() as u64 * 48,
    )?;
    if book.vaults.windows(2).any(|v| v[0].id >= v[1].id)
        || book.priorities.windows(2).any(|v| v[0].0 >= v[1].0)
        || book.surplus.windows(2).any(|v| v[0].0 >= v[1].0)
    {
        return Err(Failure::Canonical);
    }
    let active: Vec<_> = book
        .vaults
        .iter()
        .filter(|v| v.status == Status::Active)
        .collect();
    for v in &book.vaults {
        if v.status != Status::Active
            && (v.snapshot_c != Accumulator256::ZERO
                || v.snapshot_d != Accumulator256::ZERO
                || [v.collateral, v.debt, v.reserve, v.stake]
                    .iter()
                    .any(|x| *x != 0))
        {
            return Err(Failure::Domain);
        }
    }
    if active.len() != book.order.len()
        || active.len() != book.priorities.len()
        || active.len() != book.system.active as usize
    {
        return Err(Failure::Domain);
    }
    let mut c = Wide::from(book.default_collateral);
    let mut d = Wide::from(book.default_debt);
    let mut stakes = Wide::from(0);
    let mut reserves = Wide::from(0);
    let mut pending_c = Wide::from(0);
    let mut pending_d = Wide::from(0);
    for (i, v) in active.iter().enumerate() {
        w.charge(active.len() as u64, active.len() as u64, 0)?;
        if active
            .iter()
            .skip(i + 1)
            .any(|other| other.owner == v.owner)
        {
            return Err(Failure::Domain);
        }
        if v.debt <= v.reserve {
            return Err(Failure::Domain);
        }
        c = c.add(Wide::from(v.collateral), w)?;
        d = d.add(Wide::from(v.debt), w)?;
        stakes = stakes.add(Wide::from(v.stake), w)?;
        reserves = reserves.add(Wide::from(v.reserve), w)?;
        let entire = vaults::effective(**v, book.system, w)?;
        pending_c = pending_c.add(Wide::from(sub(entire.collateral, v.collateral, w)?), w)?;
        pending_d = pending_d.add(Wide::from(sub(entire.debt, v.debt, w)?), w)?;
        if book.order.iter().filter(|id| **id == v.id).count() != 1
            || book.priorities.get(i).map(|p| p.0) != Some(v.id)
            || book.priorities[i].1 != fresh(book, v.id, w)?
        {
            return Err(Failure::Domain);
        }
    }
    if pending_c.cmp(Wide::from(book.default_collateral)).is_gt()
        || pending_d.cmp(Wide::from(book.default_debt)).is_gt()
    {
        return Err(Failure::Conservation);
    }
    if c != Wide::from(book.system.collateral)
        || d != Wide::from(book.system.debt)
        || stakes != Wide::from(book.system.stakes)
        || reserves.cmp(Wide::from(book.gas_balance)).is_gt()
    {
        return Err(Failure::Conservation);
    }
    for pair in book.order.windows(2) {
        if fresh(book, pair[0], w)? < fresh(book, pair[1], w)? {
            return Err(Failure::Domain);
        }
    }
    stability::validate(&book.pool, b, w)?;
    Ok(())
}
fn advance_accumulator(
    amount: u128,
    error: Accumulator256,
    stakes: u128,
    w: &mut Work,
) -> Result<(Accumulator256, Accumulator256), Failure> {
    let n = Wide::from(amount).mul(Q, w)?.add(Wide::from(error), w)?;
    let (q, r) = n.quotient_wide(Wide::from(stakes), w)?;
    Ok((q.accumulator()?, r.accumulator()?))
}
fn repair(book: &mut Book, w: &mut Work) -> Result<(), Failure> {
    // Stable sorted-prefix reinsertion. Only an inverting member moves; placing
    // after existing equals is a valid exact V1 cut (ROUND_B2 amendment).
    let old = book.order.clone();
    let mut result = Vec::with_capacity(old.len());
    for id in old {
        let p = fresh(book, id, w)?;
        let mut at = result.len();
        for (i, other) in result.iter().enumerate() {
            w.charge(1, 1, 64)?;
            if fresh(book, *other, w)? < p {
                at = i;
                break;
            }
        }
        result.insert(at, id);
    }
    book.order = result;
    let mut priorities = Vec::with_capacity(book.order.len());
    for v in &book.vaults {
        if v.status == Status::Active {
            priorities.push((v.id, fresh(book, v.id, w)?));
        }
    }
    book.priorities = priorities;
    Ok(())
}
/// D4: actual remaining backing. The keeper and surplus legs stay outside TCR.
#[cfg(test)]
mod repair_tests {
    use super::*;
    fn permutations(xs: &mut [Id], at: usize, out: &mut Vec<Vec<Id>>) {
        if at == xs.len() {
            out.push(xs.to_vec());
            return;
        }
        for i in at..xs.len() {
            xs.swap(at, i);
            permutations(xs, at + 1, out);
            xs.swap(at, i);
        }
    }
    #[test]
    fn d2_all_small_priority_histories_are_stable_and_framed() {
        let limits = Limits {
            steps: 1_000_000,
            reads: 100_000,
            bytes: 100_000,
        };
        let mut checked = 0;
        for n in 0u32..=4 {
            let mut ids: Vec<Id> = (0..n)
                .map(|i| {
                    let mut id = [0; 32];
                    id[31] = i as u8;
                    id
                })
                .collect();
            let mut histories = Vec::new();
            permutations(&mut ids, 0, &mut histories);
            for values in 0..3u32.pow(n) {
                let mut digits = values;
                let mut vaults = Vec::new();
                for id in &ids {
                    let c = u128::from(digits % 3 + 1);
                    digits /= 3;
                    vaults.push(Vault {
                        parameter_version: 1,
                        id: *id,
                        owner: *id,
                        status: Status::Active,
                        collateral: c,
                        debt: 1,
                        reserve: 0,
                        stake: c,
                        snapshot_c: Accumulator256::ZERO,
                        snapshot_d: Accumulator256::ZERO,
                    });
                }
                for history in &histories {
                    let mut book = Book {
                        vaults: vaults.clone(),
                        order: history.clone(),
                        priorities: Vec::new(),
                        system: System {
                            collateral: 0,
                            debt: 0,
                            active: n,
                            stakes: 0,
                            stakes_snapshot: 0,
                            collateral_snapshot: 0,
                            reward_c: Accumulator256::ZERO,
                            reward_d: Accumulator256::ZERO,
                        },
                        default_collateral: 0,
                        default_debt: 0,
                        error_c: Accumulator256::ZERO,
                        error_d: Accumulator256::ZERO,
                        gas_balance: 0,
                        surplus: Vec::new(),
                        pool: stability::State::reference_genesis(stability::FUNDED_CAP).unwrap(),
                    };
                    let before = book.clone();
                    let mut expected = history.clone();
                    expected.sort_by_key(|id| {
                        core::cmp::Reverse(vaults[usize::from(id[31])].collateral)
                    });
                    let mut work = Work::new(limits);
                    repair(&mut book, &mut work).unwrap();
                    assert_eq!(book.order, expected);
                    assert_eq!(book.vaults, before.vaults);
                    assert_eq!(book.system, before.system);
                    assert_eq!(book.pool, before.pool);
                    assert_eq!(book.surplus, before.surplus);
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 2128);
    }
}
pub fn remaining_backing(
    collateral: u128,
    debt: u128,
    p: Partition,
    limits: Limits,
) -> Evaluation<(u128, u128)> {
    run(limits, |w| {
        Ok((
            sub(
                sub(sub(collateral, p.sp_collateral, w)?, p.keeper_collateral, w)?,
                p.surplus,
                w,
            )?,
            sub(debt, p.offset, w)?,
        ))
    })
}
pub fn liquidate(
    pre: &Book,
    command: &Command,
    prices: Prices,
    issuance: Option<u128>,
    capacity: u32,
    b: stability::Bounds,
    limits: Limits,
) -> Evaluation<Outcome> {
    run(limits, |w| {
        validate(pre, capacity, b, w)?;
        if matches!(command, Command::Lowest(0))
            || matches!(command,Command::Batch(ids) if ids.is_empty())
        {
            return Err(Failure::Reject(1010));
        }
        if let Command::Batch(ids) = command
            && (ids.len() > capacity as usize)
        {
            return Err(Failure::Capacity);
        }
        if let Command::Direct(id) = command {
            let i = index(pre, *id, w)?.ok_or(Failure::Reject(1011))?;
            if pre.vaults[i].status != Status::Active {
                return Err(Failure::Reject(1011));
            }
        }
        prices.usable()?;
        if (matches!(command, Command::Direct(_))) && (pre.system.active <= 1) {
            return Err(Failure::Reject(1041));
        }
        w.charge(
            1,
            0,
            pre.vaults.len() as u64 * 512
                + pre.order.len() as u64 * 96
                + match command {
                    Command::Batch(ids) => ids.len() as u64 * 32,
                    _ => 32,
                }
                + pre.pool.history.len() as u64 * 96
                + 640,
        )?;
        let mut book = pre.clone();
        let mut running = pre.system;
        let mut mode = vaults::recovery(running, prices.committed, w)?;
        let recovery_start = mode;
        let initial_head = pre.order.first().copied();
        let ids: Vec<Id> = match command {
            Command::Direct(id) => alloc::vec![*id],
            Command::Batch(ids) => ids.clone(),
            Command::Lowest(n) => pre.order.iter().rev().take(*n as usize).copied().collect(),
        };
        let mut selected = Vec::new();
        let mut partitions = Vec::new();
        let mut offset = 0;
        let mut sp_c = 0;
        let mut red_d = 0;
        let mut red_c = 0;
        let mut keeper_c = 0;
        let mut keeper_z = 0;
        for id in ids {
            w.charge(1, 1, 32)?;
            if matches!(command, Command::Lowest(_)) && recovery_start && Some(id) == initial_head {
                break;
            }
            let Some(i) = index(&book, id, w)? else {
                continue;
            };
            if book.vaults[i].status != Status::Active {
                continue;
            }
            let v = vaults::effective(book.vaults[i], book.system, w)?;
            let available = sub(pre.pool.total, offset, w)?;
            let icr = vaults::ratio(v.collateral, v.debt, prices.committed, w)?;
            if matches!(command, Command::Lowest(_))
                && ((mode && icr >= MCR && available == 0) || (!mode && icr >= MCR))
            {
                break;
            }
            let Some(p) = partition(v, running, prices.committed, available, mode, w)? else {
                continue;
            };
            if running.active <= 1 {
                return Err(Failure::Reject(1041));
            }
            book.default_collateral = sub(
                book.default_collateral,
                sub(v.collateral, book.vaults[i].collateral, w)?,
                w,
            )?;
            book.default_debt = sub(book.default_debt, sub(v.debt, book.vaults[i].debt, w)?, w)?;
            book.system.stakes = sub(book.system.stakes, v.stake, w)?;
            book.vaults[i] = Vault {
                status: Status::ClosedLiquidation,
                collateral: 0,
                debt: 0,
                reserve: 0,
                stake: 0,
                snapshot_c: Accumulator256::ZERO,
                snapshot_d: Accumulator256::ZERO,
                ..v
            };
            book.order.retain(|x| *x != id);
            if p.surplus > 0 {
                let at = book.surplus.binary_search_by_key(&v.owner, |x| x.0);
                match at {
                    Ok(j) => book.surplus[j].1 = add(book.surplus[j].1, p.surplus, w)?,
                    Err(j) => {
                        if book.surplus.len() >= capacity as usize {
                            return Err(Failure::Reject(1061));
                        }
                        book.surplus.insert(j, (v.owner, p.surplus));
                    }
                }
            }
            offset = add(offset, p.offset, w)?;
            sp_c = add(sp_c, p.sp_collateral, w)?;
            red_d = add(red_d, p.redistributed_debt, w)?;
            red_c = add(red_c, p.redistributed_collateral, w)?;
            keeper_c = add(keeper_c, p.keeper_collateral, w)?;
            keeper_z = add(keeper_z, p.reserve, w)?;
            running.collateral = sub(
                sub(
                    sub(running.collateral, p.sp_collateral, w)?,
                    p.keeper_collateral,
                    w,
                )?,
                p.surplus,
                w,
            )?;
            running.debt = sub(running.debt, p.offset, w)?;
            running.active -= 1;
            if mode {
                mode = vaults::recovery(running, prices.committed, w)?;
            }
            selected.push(id);
            partitions.push(p);
        }
        if selected.is_empty() {
            return Err(Failure::Reject(if matches!(command, Command::Direct(_)) {
                1044
            } else {
                1045
            }));
        }
        book.pool = stability::offset_inner(&pre.pool, offset, sp_c, issuance, b, w)?;
        book.gas_balance = sub(book.gas_balance, keeper_z, w)?;
        book.default_collateral = add(book.default_collateral, red_c, w)?;
        book.default_debt = add(book.default_debt, red_d, w)?;
        if red_d > 0 {
            let (c, ec) = advance_accumulator(red_c, book.error_c, book.system.stakes, w)?;
            let (d, ed) = advance_accumulator(red_d, book.error_d, book.system.stakes, w)?;
            book.error_c = ec;
            book.error_d = ed;
            book.system.reward_c = book.system.reward_c.add(c, w)?;
            book.system.reward_d = book.system.reward_d.add(d, w)?;
        }
        book.system.collateral = running.collateral;
        book.system.debt = running.debt;
        book.system.active = running.active;
        book.system.stakes_snapshot = book.system.stakes;
        book.system.collateral_snapshot = running.collateral;
        repair(&mut book, w)?;
        validate(&book, capacity, b, w)?;
        Ok(Outcome {
            book,
            selected,
            partitions,
            keeper_zusd: keeper_z,
            keeper_collateral: keeper_c,
            burned: offset,
            redistributed_debt: red_d,
            redistributed_collateral: red_c,
        })
    })
}
