//! Independent complete redemption relation. Build an extensional change ledger
//! from immutable pre-vaults, then derive every successor field. Never invokes
//! Traversal::run, settle or reinsert, or accepts a caller predicate/Usage.
use super::super::meter::Meter;
use super::redemption::{self as r, *};
use super::{Failure, Resource};
use alloc::vec::Vec;
use core::cmp::Ordering;
const Q: u128 = 1_000_000_000_000_000_000;
/// Extensional redistribution relation. Admission checks fresh keys/bijection;
/// this independent pair law determines the unique stable order without calling
/// the producer's prefix repair, gap enumeration or V1 reinsertion.
pub(super) fn redistribution(pre: &State, post: &State, m: &mut Meter) -> Result<(), Failure> {
    m.charge(Resource::Step, 2)?;
    if post.reward_c < pre.reward_c || post.reward_d < pre.reward_d {
        return Err(Failure::Arithmetic);
    }
    r::charge_state(pre, m)?;
    r::charge_state(post, m)?;
    let mut frame = pre.clone();
    frame.reward_c = post.reward_c;
    frame.reward_d = post.reward_d;
    frame.priorities = post.priorities.clone();
    frame.order = post.order.clone();
    if frame != *post {
        return Err(Failure::Law(988));
    }
    for (i, left) in pre.order.iter().enumerate() {
        visit(m)?;
        for right in &pre.order[i + 1..] {
            visit(m)?;
            let mut li = None;
            let mut ri = None;
            for (k, id) in post.order.iter().enumerate() {
                visit(m)?;
                m.charge(Resource::Byte, 12)?;
                if id == left {
                    li = Some(k);
                }
                if id == right {
                    ri = Some(k);
                }
            }
            let cmp = r::pcmp(
                r::priority(post, *left, m)?,
                r::priority(post, *right, m)?,
                m,
            )?;
            let (li, ri) = (li.ok_or(Failure::Law(988))?, ri.ok_or(Failure::Law(988))?);
            // For ties, left preceded right in the immutable original order.
            if (li < ri) != (cmp != Ordering::Less) {
                return Err(Failure::Law(988));
            }
        }
    }
    Ok(())
}
#[derive(Debug)]
struct Change {
    index: usize,
    settled: bool,
    close: bool,
    partial: bool,
    c: u128,
    d: u128,
    lot: u128,
    draw: u128,
    surplus: u128,
}
fn visit(m: &mut Meter) -> Result<(), Failure> {
    m.charge(Resource::Step, 1)?;
    m.charge(Resource::Read, 1)?;
    Ok(())
}
fn cut(
    p: &Policy,
    s: &State,
    ids: &[u32],
    id: u32,
    newp: u128,
    (upper, lower): (Option<u32>, Option<u32>),
    m: &mut Meter,
) -> Result<Vec<u32>, Failure> {
    // Direct linked-neighbor transcription, independent of producer gap enumeration.
    let mut nodes = Vec::new();
    m.charge(Resource::Byte, (ids.len() as u64 + 1) * 16)?;
    for x in ids {
        visit(m)?;
        if *x != id {
            nodes.push(*x)
        }
    }
    let locate = |x: Option<u32>, m: &mut Meter| -> Result<Option<usize>, Failure> {
        let mut out = None;
        for (i, k) in nodes.iter().enumerate() {
            visit(m)?;
            if Some(*k) == x {
                out = Some(i)
            }
        }
        Ok(out)
    };
    let key = |i: usize, m: &mut Meter| r::priority(s, nodes[i], m);
    let valid = |u: Option<usize>, l: Option<usize>, m: &mut Meter| -> Result<bool, Failure> {
        m.charge(Resource::Step, 1)?;
        Ok(match (u, l) {
            (None, None) => nodes.is_empty(),
            (None, Some(j)) => j == 0 && r::pcmp(newp, key(j, m)?, m)? != Ordering::Less,
            (Some(i), None) => {
                i + 1 == nodes.len() && r::pcmp(key(i, m)?, newp, m)? != Ordering::Less
            }
            (Some(i), Some(j)) => {
                i + 1 == j
                    && r::pcmp(key(i, m)?, newp, m)? != Ordering::Less
                    && r::pcmp(newp, key(j, m)?, m)? != Ordering::Less
            }
        })
    };
    let mut u = locate(upper, m)?;
    let mut l = locate(lower, m)?;
    let usable =
        upper.is_none_or(|_| u.is_some()) && lower.is_none_or(|_| l.is_some()) && valid(u, l, m)?;
    if !usable {
        if let Some(i) = u
            && r::pcmp(newp, key(i, m)?, m)? == Ordering::Greater
        {
            u = None
        }
        if let Some(i) = l
            && r::pcmp(newp, key(i, m)?, m)? == Ordering::Less
        {
            l = None
        }
        let ascend = u.is_none() && l.is_some();
        if nodes.is_empty() {
            u = None;
            l = None;
        } else if ascend {
            let start = l.ok_or(Failure::Undefined)?;
            if start + 1 == nodes.len() && r::pcmp(newp, key(start, m)?, m)? != Ordering::Greater {
                u = Some(start);
                l = None;
            } else {
                l = Some(start);
                u = start.checked_sub(1);
                let mut traversed = 0u32;
                while !valid(u, l, m)? {
                    visit(m)?;
                    traversed = traversed.checked_add(1).ok_or(Failure::Arithmetic)?;
                    if traversed > p.capacity {
                        return Err(Failure::Undefined);
                    }
                    let j = l.ok_or(Failure::Undefined)?;
                    l = j.checked_sub(1);
                    u = l.and_then(|j| j.checked_sub(1));
                }
            }
        } else {
            let start = u.unwrap_or(0);
            if start == 0 && r::pcmp(newp, key(0, m)?, m)? != Ordering::Less {
                u = None;
                l = Some(0);
            } else {
                u = Some(start);
                l = if start + 1 < nodes.len() {
                    Some(start + 1)
                } else {
                    None
                };
                let mut traversed = 0u32;
                while !valid(u, l, m)? {
                    visit(m)?;
                    traversed = traversed.checked_add(1).ok_or(Failure::Arithmetic)?;
                    if traversed > p.capacity {
                        return Err(Failure::Undefined);
                    }
                    let i = u.ok_or(Failure::Undefined)?;
                    u = if i + 1 < nodes.len() {
                        Some(i + 1)
                    } else {
                        None
                    };
                    l = u.and_then(|i| {
                        if i + 1 < nodes.len() {
                            Some(i + 1)
                        } else {
                            None
                        }
                    });
                }
            }
        }
    }
    if !valid(u, l, m)? {
        return Err(Failure::Law(988));
    }
    nodes.insert(l.unwrap_or(nodes.len()), id);
    Ok(nodes)
}
fn expected(
    p: &Policy,
    pre: &State,
    c: &Command,
    x: Context,
    m: &mut Meter,
) -> Result<Result<(State, Trace), Reason>, Failure> {
    m.charge(Resource::Step, 64)?;
    m.charge(Resource::Byte, 1024)?;
    if c.actor != pre.holder || x.actor != c.actor {
        return Ok(Err(Reason::Unauthorized));
    }
    if c.nonce != pre.nonce {
        return Ok(Err(Reason::Replay));
    }
    if pre.nonce == u64::MAX {
        return Ok(Err(Reason::NonceExhausted));
    }
    if pre.price_e8 == 0
        || pre.oracle_profile != p.oracle_profile
        || x.epoch < pre.price_epoch
        || x.epoch - pre.price_epoch > p.max_price_age
    {
        return Ok(Err(Reason::Oracle));
    }
    if c.maximum > p.redemption_capacity {
        return Ok(Err(Reason::Capacity));
    }
    if c.max_fee < Q / 200 || c.max_fee > Q {
        return Ok(Err(Reason::FeeDomain));
    }
    if x.epoch < p.bootstrap_epoch {
        return Ok(Err(Reason::Bootstrap));
    }
    let backing = r::add(pre.active_collateral, pre.default_collateral, m)?;
    let debt = r::add(pre.active_debt, pre.default_debt, m)?;
    if !r::eligible(backing, debt, pre.price_e8, m)? {
        return Ok(Err(Reason::Tcr));
    }
    if c.maximum == 0 {
        return Ok(Err(Reason::Amount));
    }
    if pre.holder_balance < c.maximum {
        return Ok(Err(Reason::Balance));
    }
    m.charge(Resource::Byte, (pre.order.len() as u64 + 1) * 128)?;
    let mut first = None;
    let mut at = pre.order.len();
    while at > 0 {
        at -= 1;
        visit(m)?;
        let i = r::index(pre, pre.order[at], m)?;
        let (coll, debt) = r::effective(pre, &pre.vaults[i], m)?;
        if r::eligible(coll, debt, pre.price_e8, m)? {
            first = Some(at);
            break;
        }
    }
    let mut tr = Trace {
        visited: Vec::new(),
        closed: Vec::new(),
        first: first.map(|i| pre.order[i]),
        first_hint_used: false,
        stop: Stop::End,
        executed: 0,
        unfilled: c.maximum,
        gross_collateral: 0,
        reserve_burn: 0,
        surplus: 0,
        fee: 0,
        settled: Vec::new(),
    };
    tr.first_hint_used = c.first.is_some() && c.first == tr.first;
    let limit = if c.max_visits == 0 {
        pre.order.len()
    } else {
        pre.order.len().min(c.max_visits as usize)
    };
    let mut changes: Vec<Change> = Vec::new();
    let mut remaining_active = pre.order.len();
    if let Some(start) = first {
        let mut next = Some(start);
        while let Some(j) = next {
            if tr.unfilled == 0 {
                tr.stop = Stop::Filled;
                break;
            }
            if tr.visited.len() >= limit {
                tr.stop = Stop::VisitCap;
                break;
            }
            visit(m)?;
            m.charge(Resource::Step, 64)?;
            m.charge(Resource::Byte, 512)?;
            let id = pre.order[j];
            tr.visited.push(id);
            next = j.checked_sub(1);
            let i = r::index(pre, id, m)?;
            let vault = &pre.vaults[i];
            let (coll, debt) = r::effective(pre, vault, m)?;
            if !r::eligible(coll, debt, pre.price_e8, m)? {
                continue;
            }
            let settled = vault.snapshot_c < pre.reward_c || vault.snapshot_d < pre.reward_d;
            if settled {
                tr.settled.push(id)
            }
            let net = r::sub(debt, p.reserve, m)?;
            let lot = net.min(tr.unfilled);
            let draw = r::muldiv(lot, 100_000_000, pre.price_e8, m)?;
            let nc = r::sub(coll, draw, m)?;
            let nd = r::sub(debt, lot, m)?;
            let close = lot == net;
            if close {
                if remaining_active <= 1 {
                    return Ok(Err(Reason::LastVault));
                }
                remaining_active -= 1;
                tr.closed.push(id);
                tr.reserve_burn = r::add(tr.reserve_burn, p.reserve, m)?;
                tr.surplus = r::add(tr.surplus, nc, m)?;
            } else if r::pcmp(r::nicr(nc, nd, m)?, c.partial_nicr, m)? != Ordering::Equal
                || r::sub(nd, p.reserve, m)? < p.minimum_net_debt
            {
                changes.push(Change {
                    index: i,
                    settled,
                    close: false,
                    partial: false,
                    c: coll,
                    d: debt,
                    lot: 0,
                    draw: 0,
                    surplus: 0,
                });
                tr.stop = Stop::CancelledPartial;
                break;
            }
            changes.push(Change {
                index: i,
                settled,
                close,
                partial: !close,
                c: nc,
                d: nd,
                lot,
                draw,
                surplus: if close { nc } else { 0 },
            });
            tr.executed = r::add(tr.executed, lot, m)?;
            tr.unfilled = r::sub(tr.unfilled, lot, m)?;
            tr.gross_collateral = r::add(tr.gross_collateral, draw, m)?;
            if tr.unfilled == 0 {
                tr.stop = Stop::Filled;
                break;
            }
        }
    }
    if tr.executed == 0 || tr.gross_collateral == 0 {
        return Ok(Err(Reason::NoPositiveExecution));
    }
    // Fee arithmetic is a source-bound primitive; traversal and state equations
    // are independent. The separate arbitrary-integer oracle checks the primitive.
    let seconds = x
        .epoch
        .checked_sub(pre.last_fee_epoch)
        .and_then(|n| n.checked_mul(p.epoch_seconds))
        .ok_or(Failure::Arithmetic)?;
    let decayed = r::decay(pre.base_rate, seconds / 60, m)?;
    let fraction = r::mul3div(
        tr.gross_collateral,
        pre.price_e8,
        10_000_000_000,
        pre.supply,
        m,
    )?;
    let base = r::add(decayed, fraction / 2, m)?.min(Q);
    if base == 0 {
        return Err(Failure::Arithmetic);
    }
    tr.fee = r::muldiv(tr.gross_collateral, r::add(Q / 200, base, m)?.min(Q), Q, m)?;
    if tr.fee >= tr.gross_collateral {
        return Ok(Err(Reason::FeeConsumesAll));
    }
    if r::muldiv(tr.fee, Q, tr.gross_collateral, m)? > c.max_fee {
        return Ok(Err(Reason::FeeExceeded));
    }
    r::charge_state(pre, m)?;
    let mut post = pre.clone();
    let mut settled_c = 0;
    let mut settled_d = 0;
    let mut partial = None;
    // Union-domain equations retain every unvisited field, owner, lifecycle ID,
    // and skipped vault. No command case or producer candidate establishes them.
    for (i, v) in pre.vaults.iter().enumerate() {
        visit(m)?;
        let mut change = None;
        for ch in &changes {
            visit(m)?;
            if ch.index == i {
                change = Some(ch)
            }
        }
        let Some(ch) = change else { continue };
        if ch.settled {
            let (coll, debt) = r::effective(pre, v, m)?;
            settled_c = r::add(settled_c, r::sub(coll, v.collateral, m)?, m)?;
            settled_d = r::add(settled_d, r::sub(debt, v.debt, m)?, m)?;
            post.vaults[i].snapshot_c = pre.reward_c;
            post.vaults[i].snapshot_d = pre.reward_d;
        }
        post.vaults[i].collateral = ch.c;
        post.vaults[i].debt = ch.d;
        if ch.close {
            post.vaults[i].active = false;
            post.vaults[i].collateral = 0;
            post.vaults[i].debt = 0;
            post.vaults[i].stake = 0;
            post.vaults[i].snapshot_c = 0;
            post.vaults[i].snapshot_d = 0;
        }
        if ch.partial {
            post.vaults[i].stake = if pre.collateral_snapshot == 0 {
                ch.c
            } else {
                if pre.stakes_snapshot == 0 {
                    return Err(Failure::Law(986));
                }
                r::muldiv(ch.c, pre.stakes_snapshot, pre.collateral_snapshot, m)?
            };
            partial = Some(v.id);
        }
    }
    post.active_collateral = r::sub(
        r::add(pre.active_collateral, settled_c, m)?,
        r::add(tr.gross_collateral, tr.surplus, m)?,
        m,
    )?;
    post.default_collateral = r::sub(pre.default_collateral, settled_c, m)?;
    post.active_debt = r::sub(
        r::add(pre.active_debt, settled_d, m)?,
        r::add(tr.executed, tr.reserve_burn, m)?,
        m,
    )?;
    post.default_debt = r::sub(pre.default_debt, settled_d, m)?;
    let mut surplus = Vec::new();
    m.charge(
        Resource::Byte,
        (pre.surplus.len() as u64 + changes.len() as u64) * 32,
    )?;
    for (owner, amount) in &pre.surplus {
        visit(m)?;
        let mut expected = *amount;
        for ch in &changes {
            visit(m)?;
            if ch.close && pre.vaults[ch.index].owner == *owner {
                expected = r::add(expected, ch.surplus, m)?;
            }
        }
        surplus.push((*owner, expected));
    }
    for ch in &changes {
        visit(m)?;
        if ch.close {
            let owner = pre.vaults[ch.index].owner;
            let mut found = false;
            for (id, _) in &pre.surplus {
                visit(m)?;
                found |= *id == owner;
            }
            if !found {
                surplus.push((owner, ch.surplus));
            }
        }
    }
    // Only active owners can close, and admission makes them unique. Sort a new
    // surplus ledger by its canonical numeric ID; this never rebuilds vault order.
    for i in 0..surplus.len() {
        for j in i + 1..surplus.len() {
            visit(m)?;
            visit(m)?;
            if surplus[j].0 < surplus[i].0 {
                surplus.swap(i, j);
            }
        }
    }
    post.surplus = surplus;
    post.order = Vec::new();
    m.charge(Resource::Byte, pre.order.len() as u64 * 8)?;
    for id in &pre.order {
        visit(m)?;
        let mut closed = false;
        for ch in &changes {
            visit(m)?;
            closed |= ch.close && pre.vaults[ch.index].id == *id;
        }
        if !closed {
            post.order.push(*id);
        }
    }
    post.priorities = Vec::new();
    m.charge(Resource::Byte, pre.priorities.len() as u64 * 32)?;
    for (id, key) in &pre.priorities {
        visit(m)?;
        let i = r::index(&post, *id, m)?;
        if post.vaults[i].active {
            let new = if partial == Some(*id) {
                r::nicr(post.vaults[i].collateral, post.vaults[i].debt, m)?
            } else {
                *key
            };
            post.priorities.push((*id, new));
        }
    }
    if let Some(id) = partial {
        post.order = cut(
            p,
            &post,
            &post.order,
            id,
            r::priority(&post, id, m)?,
            (c.upper, c.lower),
            m,
        )?;
    }
    post.holder_balance = r::sub(pre.holder_balance, tr.executed, m)?;
    post.reserve_balance = r::sub(pre.reserve_balance, tr.reserve_burn, m)?;
    post.supply = r::sub(pre.supply, r::add(tr.executed, tr.reserve_burn, m)?, m)?;
    post.holder_collateral = r::add(
        pre.holder_collateral,
        r::sub(tr.gross_collateral, tr.fee, m)?,
        m,
    )?;
    post.staking_collateral = r::add(pre.staking_collateral, tr.fee, m)?;
    post.base_rate = base;
    post.last_fee_epoch = if seconds >= 60 {
        x.epoch
    } else {
        pre.last_fee_epoch
    };
    post.nonce = pre.nonce.checked_add(1).ok_or(Failure::Arithmetic)?;
    // Internal ledger entries retain the exact lot/draw even when a later partial
    // is cancelled; this extra sum independently checks the selected prefix.
    let mut executed = 0;
    let mut gross = 0;
    for ch in &changes {
        visit(m)?;
        executed = r::add(executed, ch.lot, m)?;
        gross = r::add(gross, ch.draw, m)?;
    }
    if executed != tr.executed || gross != tr.gross_collateral {
        return Err(Failure::Law(988));
    }
    Ok(Ok((post, tr)))
}
pub(super) fn verify(
    p: &Policy,
    pre: &State,
    c: &Command,
    x: Context,
    candidate: &Result<Plan, Reason>,
    m: &mut Meter,
) -> Result<(), Failure> {
    match candidate {
        Ok(plan) => probe(
            p,
            pre,
            c,
            x,
            (Some(plan.state()), Some(plan.trace())),
            None,
            m,
        ),
        Err(reason) => probe(p, pre, c, x, (None, None), Some(*reason), m),
    }
}
pub(super) fn probe(
    p: &Policy,
    pre: &State,
    c: &Command,
    x: Context,
    (state, trace): (Option<&State>, Option<&Trace>),
    reason: Option<Reason>,
    m: &mut Meter,
) -> Result<(), Failure> {
    let expected = expected(p, pre, c, x, m)?;
    m.charge(Resource::Step, 1)?;
    match (expected, state, trace, reason) {
        (Err(want), None, None, Some(actual)) if want == actual => Ok(()),
        (Ok((want, tr)), Some(actual), Some(actual_trace), None) => {
            r::charge_state(&want, m)?;
            r::charge_state(actual, m)?;
            m.charge(
                Resource::Byte,
                512 + (tr.visited.len()
                    + tr.closed.len()
                    + tr.settled.len()
                    + actual_trace.visited.len()
                    + actual_trace.closed.len()
                    + actual_trace.settled.len()) as u64
                    * 8,
            )?;
            if want == *actual && tr == *actual_trace {
                Ok(())
            } else {
                Err(Failure::Law(988))
            }
        }
        _ => Err(Failure::Law(988)),
    }
}
