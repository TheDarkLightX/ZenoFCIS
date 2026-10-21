//! StabilityPool P/S/G and complete position/snapshot transitions. Schedule
//! deltas are abstract, explicitly supplied inputs; no drip is selected here.
use crate::{
    Evaluation, Failure, Id, Limits, Q, SCALE, Work, add, muldiv, run, sub, vaults, wide::Wide,
};
use alloc::vec::Vec;
pub const FUNDED_CAP: u128 = 35_000_000 * Q;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub epoch: u64,
    pub scale: u64,
    pub s: u128,
    pub g: u128,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Epoch {
    pub epoch: u64,
    pub last_scale: u64,
    pub sealed: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stock {
    pub remaining: u128,
    pub scheduled: u128,
    pub allocated: u128,
    pub missed: u128,
    pub paid: u128,
}
impl Stock {
    fn funded_reference(funded: u128) -> Result<Self, Failure> {
        if funded != FUNDED_CAP {
            return Err(Failure::UnboundProfile);
        }
        Ok(Self {
            remaining: funded,
            scheduled: 0,
            allocated: 0,
            missed: 0,
            paid: 0,
        })
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub total: u128,
    pub p: u128,
    pub epoch: u64,
    pub scale: u64,
    pub error_s: u128,
    pub error_loss: u128,
    pub error_g: u128,
    pub collateral: u128,
    pub stock: Stock,
    pub history: Vec<Cell>,
    pub epochs: Vec<Epoch>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bounds {
    pub history: u32,
    pub epochs: u32,
    pub deposits: u32,
    pub front_ends: u32,
    pub vaults: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub p: u128,
    pub s: u128,
    pub g: u128,
    pub epoch: u64,
    pub scale: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Deposit {
    pub owner: Id,
    pub initial: u128,
    pub tag: Option<Id>,
    pub snapshot: Snapshot,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrontSnapshot {
    pub p: u128,
    pub g: u128,
    pub epoch: u64,
    pub scale: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrontEnd {
    pub owner: Id,
    pub kickback: u128,
    pub initial: u128,
    pub snapshot: FrontSnapshot,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Book {
    pub pool: State,
    pub deposits: Vec<Deposit>,
    pub front_ends: Vec<FrontEnd>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Gains {
    pub principal: u128,
    pub collateral: u128,
    pub depositor_zdex: u128,
    pub front_end_zdex: u128,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub book: Book,
    pub gains: Gains,
    pub principal_in: u128,
    pub principal_out: u128,
    pub front_end: Option<Id>,
    pub gain_to_vault: bool,
}
impl State {
    /// Conditional reference state, requiring an explicit funding assumption.
    /// This equality is not custody authentication or deployment admission.
    pub fn reference_genesis(funded: u128) -> Result<Self, Failure> {
        Ok(Self {
            total: 0,
            p: Q,
            epoch: 0,
            scale: 0,
            error_s: 0,
            error_loss: 0,
            error_g: 0,
            collateral: 0,
            stock: Stock::funded_reference(funded)?,
            history: alloc::vec![Cell {
                epoch: 0,
                scale: 0,
                s: 0,
                g: 0
            }],
            epochs: alloc::vec![Epoch {
                epoch: 0,
                last_scale: 0,
                sealed: false
            }],
        })
    }
}
pub(crate) fn validate(s: &State, b: Bounds, w: &mut Work) -> Result<(), Failure> {
    w.charge(1, 0, 0)?;
    if s.history.len() > b.history as usize || s.epochs.len() > b.epochs as usize {
        return Err(Failure::Capacity);
    }
    w.charge(
        s.history.len() as u64 + s.epochs.len() as u64,
        s.history.len() as u64 + s.epochs.len() as u64,
        (s.history.len() as u64 * 64) + (s.epochs.len() as u64 * 24),
    )?;
    if s.p == 0
        || s.p > Q
        || s.epochs.len() as u64 != s.epoch.checked_add(1).ok_or(Failure::Arithmetic)?
    {
        return Err(Failure::Domain);
    }
    let mut at = 0usize;
    for (e, record) in s.epochs.iter().enumerate() {
        if record.epoch != e as u64
            || record.sealed != (record.epoch < s.epoch)
            || record.last_scale >= s.history.len() as u64
        {
            return Err(Failure::Domain);
        }
        for scale in 0..=record.last_scale {
            let cell = s.history.get(at).ok_or(Failure::Domain)?;
            if cell.epoch != record.epoch || cell.scale != scale {
                return Err(Failure::Canonical);
            }
            at += 1;
        }
    }
    if at != s.history.len() || s.epochs.last().map(|e| e.last_scale) != Some(s.scale) {
        return Err(Failure::Domain);
    }
    let scheduled = add(s.stock.allocated, s.stock.missed, w)?;
    if scheduled != s.stock.scheduled
        || add(s.stock.remaining, scheduled, w)? != FUNDED_CAP
        || s.stock.paid > s.stock.allocated
    {
        return Err(Failure::Conservation);
    }
    Ok(())
}
fn cell(s: &State, e: u64, scale: u64, w: &mut Work) -> Result<Cell, Failure> {
    w.charge(s.history.len() as u64, s.history.len() as u64, 0)?;
    let record = s
        .epochs
        .get(usize::try_from(e).map_err(|_| Failure::Domain)?)
        .ok_or(Failure::Domain)?;
    if scale > record.last_scale {
        return Ok(Cell {
            epoch: e,
            scale,
            s: 0,
            g: 0,
        });
    }
    s.history
        .iter()
        .find(|c| c.epoch == e && c.scale == scale)
        .copied()
        .ok_or(Failure::Domain)
}
fn current(s: &State) -> Snapshot {
    let c = &s.history[s.history.len() - 1];
    Snapshot {
        p: s.p,
        s: c.s,
        g: c.g,
        epoch: s.epoch,
        scale: s.scale,
    }
}
fn current_front(s: &State) -> FrontSnapshot {
    let c = &s.history[s.history.len() - 1];
    FrontSnapshot {
        p: s.p,
        g: c.g,
        epoch: s.epoch,
        scale: s.scale,
    }
}
fn accumulator(
    amount: u128,
    error: u128,
    total: u128,
    w: &mut Work,
) -> Result<(u128, u128), Failure> {
    let n = Wide::from(amount).mul(Q, w)?.add(Wide::from(error), w)?;
    let (q, r) = n.quotient(Wide::from(total), w)?;
    Ok((q, r.narrow()?))
}
fn issue(s: &mut State, delta: Option<u128>, w: &mut Work) -> Result<(), Failure> {
    let amount = delta.ok_or(Failure::UnboundProfile)?;
    if amount > s.stock.remaining {
        return Err(Failure::Conservation);
    }
    s.stock.remaining = sub(s.stock.remaining, amount, w)?;
    s.stock.scheduled = add(s.stock.scheduled, amount, w)?;
    if s.total == 0 {
        s.stock.missed = add(s.stock.missed, amount, w)?;
    } else if amount > 0 {
        s.stock.allocated = add(s.stock.allocated, amount, w)?;
        let (g, error) = accumulator(amount, s.error_g, s.total, w)?;
        s.error_g = error;
        let last = s.history.last_mut().ok_or(Failure::Domain)?;
        last.g = add(last.g, Wide::from(g).mul(s.p, w)?.narrow()?, w)?;
    }
    Ok(())
}
pub(crate) fn offset_inner(
    pre: &State,
    debt: u128,
    collateral: u128,
    delta: Option<u128>,
    b: Bounds,
    w: &mut Work,
) -> Result<State, Failure> {
    validate(pre, b, w)?;
    w.charge(
        1,
        0,
        pre.history.len() as u64 * 64 + pre.epochs.len() as u64 * 24 + 256,
    )?;
    if pre.total == 0 || debt == 0 {
        return Ok(pre.clone());
    }
    if debt > pre.total {
        return Err(Failure::Domain);
    }
    let mut s = pre.clone();
    issue(&mut s, delta, w)?;
    let (gain, error) = accumulator(collateral, s.error_s, s.total, w)?;
    s.error_s = error;
    let loss = if debt == s.total {
        s.error_loss = 0;
        Q
    } else {
        let n = Wide::from(debt)
            .mul(Q, w)?
            .sub(Wide::from(s.error_loss), w)?;
        let floor = n.quotient(Wide::from(s.total), w)?.0;
        let loss = add(floor, 1, w)?;
        s.error_loss = Wide::from(loss).mul(s.total, w)?.sub(n, w)?.narrow()?;
        loss
    };
    let factor = sub(Q, loss, w)?;
    let last = s.history.last_mut().ok_or(Failure::Domain)?;
    last.s = add(last.s, Wide::from(gain).mul(s.p, w)?.narrow()?, w)?;
    if factor == 0 {
        if s.epochs.len() >= b.epochs as usize || s.history.len() >= b.history as usize {
            return Err(Failure::Reject(1061));
        }
        s.epochs.last_mut().ok_or(Failure::Domain)?.sealed = true;
        s.epoch = s.epoch.checked_add(1).ok_or(Failure::Arithmetic)?;
        s.scale = 0;
        s.p = Q;
        s.epochs.push(Epoch {
            epoch: s.epoch,
            last_scale: 0,
            sealed: false,
        });
        s.history.push(Cell {
            epoch: s.epoch,
            scale: 0,
            s: 0,
            g: 0,
        });
    } else if muldiv(s.p, factor, Q, w)? < SCALE {
        if s.history.len() >= b.history as usize {
            return Err(Failure::Reject(1061));
        }
        s.p = Wide::from(s.p)
            .mul(factor, w)?
            .mul(SCALE, w)?
            .quotient(Wide::from(Q), w)?
            .0;
        s.scale = s.scale.checked_add(1).ok_or(Failure::Arithmetic)?;
        s.epochs.last_mut().ok_or(Failure::Domain)?.last_scale = s.scale;
        s.history.push(Cell {
            epoch: s.epoch,
            scale: s.scale,
            s: 0,
            g: 0,
        });
    } else {
        s.p = muldiv(s.p, factor, Q, w)?;
    }
    if s.p == 0 {
        return Err(Failure::Arithmetic);
    }
    s.total = sub(s.total, debt, w)?;
    s.collateral = add(s.collateral, collateral, w)?;
    validate(&s, b, w)?;
    Ok(s)
}
pub fn offset(
    pre: &State,
    debt: u128,
    collateral: u128,
    issuance: Option<u128>,
    bounds: Bounds,
    limits: Limits,
) -> Evaluation<State> {
    run(limits, |w| {
        offset_inner(pre, debt, collateral, issuance, bounds, w)
    })
}
fn compounded(
    s: &State,
    initial: u128,
    p: u128,
    epoch: u64,
    scale: u64,
    w: &mut Work,
) -> Result<u128, Failure> {
    if epoch > s.epoch || p == 0 || p > Q {
        return Err(Failure::Domain);
    }
    if epoch < s.epoch {
        return Ok(0);
    }
    let diff = s.scale.checked_sub(scale).ok_or(Failure::Domain)?;
    if diff >= 2 {
        return Ok(0);
    }
    let mut denominator = Wide::from(p);
    if diff == 1 {
        denominator = denominator.mul(SCALE, w)?;
    }
    let result = Wide::from(initial).mul(s.p, w)?.quotient(denominator, w)?.0;
    Ok(if result < initial / SCALE { 0 } else { result })
}
fn gains(
    s: &State,
    initial: u128,
    snap: Snapshot,
    w: &mut Work,
) -> Result<(u128, u128, u128), Failure> {
    if initial == 0 {
        return Ok((0, 0, 0));
    }
    let a = cell(s, snap.epoch, snap.scale, w)?;
    let next = cell(
        s,
        snap.epoch,
        snap.scale.checked_add(1).ok_or(Failure::Arithmetic)?,
        w,
    )?;
    let ds = add(sub(a.s, snap.s, w)?, next.s / SCALE, w)?;
    let dg = add(sub(a.g, snap.g, w)?, next.g / SCALE, w)?;
    let denominator = Wide::from(snap.p).mul(Q, w)?;
    Ok((
        compounded(s, initial, snap.p, snap.epoch, snap.scale, w)?,
        Wide::from(initial).mul(ds, w)?.quotient(denominator, w)?.0,
        Wide::from(initial).mul(dg, w)?.quotient(denominator, w)?.0,
    ))
}
fn front_gains(
    s: &State,
    initial: u128,
    snap: FrontSnapshot,
    w: &mut Work,
) -> Result<(u128, u128), Failure> {
    if initial == 0 {
        return Ok((0, 0));
    }
    let a = cell(s, snap.epoch, snap.scale, w)?;
    let next = cell(
        s,
        snap.epoch,
        snap.scale.checked_add(1).ok_or(Failure::Arithmetic)?,
        w,
    )?;
    let delta = add(sub(a.g, snap.g, w)?, next.g / SCALE, w)?;
    Ok((
        compounded(s, initial, snap.p, snap.epoch, snap.scale, w)?,
        Wide::from(initial)
            .mul(delta, w)?
            .quotient(Wide::from(snap.p).mul(Q, w)?, w)?
            .0,
    ))
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Liabilities {
    pub principal_collective: u128,
    pub residual_only: u128,
    pub collateral_collective: u128,
    pub zdex_collective: u128,
}
fn validate_book(book: &Book, b: Bounds, w: &mut Work) -> Result<Liabilities, Failure> {
    validate(&book.pool, b, w)?;
    if book.deposits.len() > b.deposits as usize || book.front_ends.len() > b.front_ends as usize {
        return Err(Failure::Capacity);
    }
    w.charge(
        book.deposits.len() as u64 + book.front_ends.len() as u64,
        book.deposits.len() as u64 + book.front_ends.len() as u64,
        (book.deposits.len() as u64 + book.front_ends.len() as u64) * 192,
    )?;
    if book.deposits.windows(2).any(|v| v[0].owner >= v[1].owner)
        || book.front_ends.windows(2).any(|v| v[0].owner >= v[1].owner)
    {
        return Err(Failure::Canonical);
    }
    for f in &book.front_ends {
        if f.kickback > Q || book.deposits.iter().any(|d| d.owner == f.owner) {
            return Err(Failure::Domain);
        }
    }
    let mut principal = Wide::from(0);
    let mut coll = Wide::from(0);
    let mut token = Wide::from(0);
    let mut front_principal = Wide::from(0);
    let mut eligible = false;
    for d in &book.deposits {
        if d.initial == 0 {
            return Err(Failure::Domain);
        }
        let (p, c, g) = gains(&book.pool, d.initial, d.snapshot, w)?;
        let k = if let Some(tag) = d.tag {
            book.front_ends
                .iter()
                .find(|f| f.owner == tag)
                .ok_or(Failure::Domain)?
                .kickback
        } else {
            Q
        };
        principal = principal.add(Wide::from(p), w)?;
        eligible |= p > 0;
        coll = coll.add(Wide::from(c), w)?;
        token = token.add(Wide::from(muldiv(g, k, Q, w)?), w)?;
    }
    for f in &book.front_ends {
        if f.initial > 0 {
            let (p, g) = front_gains(&book.pool, f.initial, f.snapshot, w)?;
            front_principal = front_principal.add(Wide::from(p), w)?;
            token = token.add(Wide::from(muldiv(g, Q - f.kickback, Q, w)?), w)?;
        }
    }
    if front_principal.cmp(Wide::from(book.pool.total)).is_gt()
        || principal.cmp(Wide::from(book.pool.total)).is_gt()
        || coll.cmp(Wide::from(book.pool.collateral)).is_gt()
        || token
            .cmp(Wide::from(sub(
                book.pool.stock.allocated,
                book.pool.stock.paid,
                w,
            )?))
            .is_gt()
    {
        return Err(Failure::Conservation);
    }
    let complement = Wide::from(book.pool.total).sub(principal, w)?.narrow()?;
    Ok(Liabilities {
        principal_collective: if eligible { complement } else { 0 },
        residual_only: if eligible { 0 } else { complement },
        collateral_collective: Wide::from(book.pool.collateral).sub(coll, w)?.narrow()?,
        zdex_collective: Wide::from(sub(book.pool.stock.allocated, book.pool.stock.paid, w)?)
            .sub(token, w)?
            .narrow()?,
    })
}
pub fn liabilities(book: &Book, b: Bounds, limits: Limits) -> Evaluation<Liabilities> {
    run(limits, |w| validate_book(book, b, w))
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    Deposit { amount: u128, tag: Option<Id> },
    Withdraw(u128),
    Claim,
    MoveGainToVault,
    RegisterFrontEnd(u128),
}
pub fn change(
    pre: &Book,
    actor: Id,
    command: Command,
    wallet_zusd: u128,
    issuance: Option<u128>,
    prices: vaults::Prices,
    inventory: &[vaults::Vault],
    system: vaults::System,
    b: Bounds,
    limits: Limits,
) -> Evaluation<Outcome> {
    run(limits, |w| {
        validate_book(pre, b, w)?;
        if inventory.len() > b.vaults as usize {
            return Err(Failure::Capacity);
        }
        w.charge(
            inventory.len() as u64,
            inventory.len() as u64,
            inventory.len() as u64 * 256,
        )?;
        let index = pre.deposits.binary_search_by_key(&actor, |d| d.owner).ok();
        let front = pre
            .front_ends
            .binary_search_by_key(&actor, |f| f.owner)
            .ok();
        if let Command::RegisterFrontEnd(k) = command {
            if front.is_some() || index.is_some() {
                return Err(Failure::Reject(1011));
            }
            if k > Q {
                return Err(Failure::Reject(1012));
            }
            if pre.front_ends.len() >= b.front_ends as usize {
                return Err(Failure::Reject(1061));
            }
            w.charge(
                1,
                0,
                (pre.deposits.len() as u64 + pre.front_ends.len() as u64) * 192
                    + pre.pool.history.len() as u64 * 64
                    + 512,
            )?;
            let mut book = pre.clone();
            let at = book.front_ends.partition_point(|f| f.owner < actor);
            book.front_ends.insert(
                at,
                FrontEnd {
                    owner: actor,
                    kickback: k,
                    initial: 0,
                    snapshot: current_front(&pre.pool),
                },
            );
            return Ok(Outcome {
                book,
                gains: Gains {
                    principal: 0,
                    collateral: 0,
                    depositor_zdex: 0,
                    front_end_zdex: 0,
                },
                principal_in: 0,
                principal_out: 0,
                front_end: None,
                gain_to_vault: false,
            });
        }
        let input = match command {
            Command::Deposit { amount, tag } => {
                if amount == 0 {
                    return Err(Failure::Reject(1010));
                }
                if front.is_some() {
                    return Err(Failure::Reject(1011));
                }
                if tag.is_some_and(|id| !pre.front_ends.iter().any(|f| f.owner == id)) {
                    return Err(Failure::Reject(1012));
                }
                if amount > wallet_zusd {
                    return Err(Failure::Reject(1033));
                }
                amount
            }
            _ => {
                if index.is_none() {
                    return Err(Failure::Reject(1011));
                }
                0
            }
        };
        if matches!(command,Command::Withdraw(x) if x>0) {
            vaults::inventory(inventory, system, w)?;
            prices.usable()?;
            if prices.pending_blocked {
                return Err(Failure::Reject(1020));
            }
            for p in [Some(prices.committed), prices.pending]
                .into_iter()
                .flatten()
            {
                for v in inventory {
                    w.charge(1, 1, 256)?;
                    if v.status == vaults::Status::Active {
                        let v = vaults::effective(*v, system, w)?;
                        if vaults::ratio(v.collateral, v.debt, p, w)? < vaults::MCR {
                            return Err(Failure::Reject(1044));
                        }
                    }
                }
            }
        }
        if command == Command::MoveGainToVault {
            vaults::inventory(inventory, system, w)?;
            if !inventory
                .iter()
                .any(|v| v.owner == actor && v.status == vaults::Status::Active)
            {
                return Err(Failure::Reject(1011));
            }
            let d = pre.deposits[index.ok_or(Failure::Domain)?];
            if gains(&pre.pool, d.initial, d.snapshot, w)?.1 == 0 {
                return Err(Failure::Reject(1045));
            }
        }
        w.charge(
            1,
            0,
            (pre.deposits.len() as u64 + pre.front_ends.len() as u64) * 192
                + pre.pool.history.len() as u64 * 64
                + 512,
        )?;
        let mut book = pre.clone();
        issue(&mut book.pool, issuance, w)?;
        let old = index.map(|i| pre.deposits[i]);
        let tag = old.map(|d| d.tag).unwrap_or_else(|| match command {
            Command::Deposit { tag, .. } => tag,
            _ => None,
        });
        let (principal, collateral, raw) = if let Some(d) = old {
            gains(&book.pool, d.initial, d.snapshot, w)?
        } else {
            (0, 0, 0)
        };
        let k = if let Some(id) = tag {
            book.front_ends
                .iter()
                .find(|f| f.owner == id)
                .ok_or(Failure::Domain)?
                .kickback
        } else {
            Q
        };
        let dep_gain = muldiv(raw, k, Q, w)?;
        let withdraw = if let Command::Withdraw(x) = command {
            x.min(principal)
        } else {
            0
        };
        let mut fe_gain = 0;
        if let Some(id) = tag {
            let i = book
                .front_ends
                .iter()
                .position(|f| f.owner == id)
                .ok_or(Failure::Domain)?;
            let f = book.front_ends[i];
            let (stake, raw) = if f.initial == 0 {
                (0, 0)
            } else {
                front_gains(&book.pool, f.initial, f.snapshot, w)?
            };
            fe_gain = muldiv(raw, Q - f.kickback, Q, w)?;
            book.front_ends[i] = FrontEnd {
                initial: sub(add(stake, input, w)?, withdraw, w)?,
                snapshot: current_front(&book.pool),
                ..f
            };
        }
        book.pool.total = sub(add(book.pool.total, input, w)?, withdraw, w)?;
        book.pool.collateral = sub(book.pool.collateral, collateral, w)?;
        book.pool.stock.paid = add(book.pool.stock.paid, add(dep_gain, fe_gain, w)?, w)?;
        let amount = sub(add(principal, input, w)?, withdraw, w)?;
        if let Some(i) = index {
            book.deposits.remove(i);
        }
        if amount > 0 {
            if book.deposits.len() >= b.deposits as usize {
                return Err(Failure::Reject(1061));
            }
            let at = book.deposits.partition_point(|d| d.owner < actor);
            book.deposits.insert(
                at,
                Deposit {
                    owner: actor,
                    initial: amount,
                    tag,
                    snapshot: current(&book.pool),
                },
            );
        }
        validate_book(&book, b, w)?;
        Ok(Outcome {
            book,
            gains: Gains {
                principal,
                collateral,
                depositor_zdex: dep_gain,
                front_end_zdex: fe_gain,
            },
            principal_in: input,
            principal_out: withdraw,
            front_end: tag,
            gain_to_vault: command == Command::MoveGainToVault,
        })
    })
}
