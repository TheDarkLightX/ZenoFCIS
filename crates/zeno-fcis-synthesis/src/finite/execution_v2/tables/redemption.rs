//! Closed U128 redemption traversal and accounting plan. This pure subcomponent
//! consumes an already authenticated command/context and committed oracle view;
//! it cannot authenticate a host, commit money or publish a custody receipt.
//! Source: pinned TroveManager.sol 0b0ba14dc297938b98aa7f130924b3525706fa6b3736fa663c72c40f483f1895.
//! ROUND_B2 deliberately strengthens first-hint verification and per-visit MCR.
use super::super::super::value::wide;
use super::super::meter::{self, Meter};
use super::{Failure, Limits, Resource, Usage};
use alloc::vec::Vec;
use core::cmp::Ordering;
const Q: u128 = 1_000_000_000_000_000_000;
const NICR_Q: u128 = 100_000_000_000_000_000_000;
const MCR: u128 = 1_100_000_000_000_000_000;
const DECAY: u128 = 999_037_758_833_783_000;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Vault {
    pub id: u32,
    pub owner: u32,
    pub active: bool,
    pub collateral: u128,
    pub debt: u128,
    pub stake: u128,
    pub snapshot_c: u128,
    pub snapshot_d: u128,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub vaults: Vec<Vault>,
    pub order: Vec<u32>,
    pub priorities: Vec<(u32, u128)>,
    pub reward_c: u128,
    pub reward_d: u128,
    pub stakes_snapshot: u128,
    pub collateral_snapshot: u128,
    pub active_collateral: u128,
    pub default_collateral: u128,
    pub active_debt: u128,
    pub default_debt: u128,
    pub holder: u32,
    pub holder_balance: u128,
    pub reserve_balance: u128,
    pub supply: u128,
    pub holder_collateral: u128,
    pub staking_collateral: u128,
    pub surplus: Vec<(u32, u128)>,
    pub nonce: u64,
    pub price_e8: u128,
    pub price_epoch: u64,
    pub oracle_profile: [u8; 32],
    pub base_rate: u128,
    pub last_fee_epoch: u64,
}
#[derive(Clone, Debug)]
pub struct Policy {
    pub capacity: u32,
    pub inventory_capacity: u32,
    pub retained_bytes: u64,
    pub minimum_net_debt: u128,
    pub reserve: u128,
    pub bootstrap_epoch: u64,
    pub epoch_seconds: u64,
    pub max_price_age: u64,
    pub oracle_profile: [u8; 32],
    pub redemption_capacity: u128,
    pub limits: Limits,
}
#[derive(Clone, Debug)]
pub struct Command {
    pub actor: u32,
    pub nonce: u64,
    pub maximum: u128,
    pub max_visits: u32,
    pub max_fee: u128,
    pub first: Option<u32>,
    pub partial_nicr: u128,
    pub upper: Option<u32>,
    pub lower: Option<u32>,
}
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub actor: u32,
    pub epoch: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    Unauthorized,
    Replay,
    NonceExhausted,
    Oracle,
    Capacity,
    FeeDomain,
    Bootstrap,
    Tcr,
    Amount,
    Balance,
    LastVault,
    NoPositiveExecution,
    FeeConsumesAll,
    FeeExceeded,
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
    pub visited: Vec<u32>,
    pub closed: Vec<u32>,
    pub first: Option<u32>,
    pub first_hint_used: bool,
    pub stop: Stop,
    pub executed: u128,
    pub unfilled: u128,
    pub gross_collateral: u128,
    pub reserve_burn: u128,
    pub surplus: u128,
    pub fee: u128,
    pub settled: Vec<u32>,
}
#[derive(Debug)]
pub struct Plan {
    pub(super) state: State,
    pub(super) trace: Trace,
}
impl Plan {
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn trace(&self) -> &Trace {
        &self.trace
    }
    pub fn into_parts(self) -> (State, Trace) {
        (self.state, self.trace)
    }
}
#[derive(Debug)]
pub struct Evaluation {
    result: Result<Result<Plan, Reason>, Failure>,
    usage: Usage,
}
impl Evaluation {
    pub fn result(&self) -> Result<Result<&Plan, Reason>, &Failure> {
        self.result.as_ref().map(|r| r.as_ref().map_err(|e| *e))
    }
    pub fn usage(&self) -> Usage {
        self.usage
    }
}
#[derive(Debug)]
pub struct Traversal {
    policy: Policy,
}
fn step(m: &mut Meter) -> Result<(), Failure> {
    m.charge(Resource::Step, 1)?;
    Ok(())
}
fn visit(m: &mut Meter) -> Result<(), Failure> {
    step(m)?;
    m.charge(Resource::Read, 1)?;
    Ok(())
}
pub(super) fn add(a: u128, b: u128, m: &mut Meter) -> Result<u128, Failure> {
    step(m)?;
    a.checked_add(b).ok_or(Failure::Arithmetic)
}
pub(super) fn sub(a: u128, b: u128, m: &mut Meter) -> Result<u128, Failure> {
    step(m)?;
    a.checked_sub(b).ok_or(Failure::Arithmetic)
}
pub(super) fn muldiv(a: u128, b: u128, d: u128, m: &mut Meter) -> Result<u128, Failure> {
    m.charge(Resource::Step, 1024)?;
    m.charge(Resource::Byte, 51)?;
    if d == 0 {
        return Err(Failure::Arithmetic);
    }
    wide::mul_div_floor(a, b, d).ok_or(Failure::Arithmetic)
}
/// Exact floor of a 384-bit product divided by a positive denominator. Binary
/// search compares qualified three-limb products; no intermediate narrowing.
pub(super) fn mul3div(a: u128, b: u128, c: u128, d: u128, m: &mut Meter) -> Result<u128, Failure> {
    if d == 0 {
        return Err(Failure::Arithmetic);
    }

    let mut lo = 0u128;
    let mut hi = 1u128 << 127;
    m.charge(Resource::Step, 128)?;
    m.charge(Resource::Byte, 68)?;
    if wide::products_ge(a, b, c, d, hi, 1) {
        // Only the newly admitted upper half takes this branch. The historical
        // lower-half search and its measured work remain unchanged.
        m.charge(Resource::Step, 128)?;
        let product = wide::multiply_three(a, b, c);
        if product[2] != 0 || product[1] >= d {
            return Err(Failure::Arithmetic);
        }
        lo = hi;
        hi = u128::MAX;
        while lo < hi {
            m.charge(Resource::Step, 128)?;
            let delta = hi - lo;
            let mid = lo + delta / 2 + delta % 2;
            if wide::products_ge(a, b, c, d, mid, 1) {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        return Ok(lo);
    }
    while hi - lo > 1 {
        m.charge(Resource::Step, 128)?;
        let mid = lo + (hi - lo) / 2;
        if wide::products_ge(a, b, c, d, mid, 1) {
            lo = mid
        } else {
            hi = mid
        }
    }
    Ok(lo)
}
pub(super) fn nicr(c: u128, d: u128, m: &mut Meter) -> Result<u128, Failure> {
    muldiv(c, NICR_Q, d, m)
}
pub(super) fn eligible(c: u128, d: u128, price: u128, m: &mut Meter) -> Result<bool, Failure> {
    m.charge(Resource::Step, 128)?;
    m.charge(Resource::Byte, 102)?;
    if d == 0 || price == 0 {
        return Err(Failure::Arithmetic);
    }
    Ok(wide::products_ge(c, price, 10_000_000_000, d, MCR, 1))
}
pub(super) fn effective(s: &State, v: &Vault, m: &mut Meter) -> Result<(u128, u128), Failure> {
    if !v.active {
        return Ok((v.collateral, v.debt));
    }
    let dc = sub(s.reward_c, v.snapshot_c, m)?;
    let dd = sub(s.reward_d, v.snapshot_d, m)?;
    Ok((
        add(v.collateral, muldiv(v.stake, dc, Q, m)?, m)?,
        add(v.debt, muldiv(v.stake, dd, Q, m)?, m)?,
    ))
}
// Native typed records use a conservative fixed logical retained-byte profile.
// This is a declared bound, not a claim about allocator/RAM or ZCVE framing.
pub(super) fn state_bytes(s: &State) -> Result<u64, Failure> {
    1024u64
        .checked_add(
            (s.vaults.len() as u64)
                .checked_mul(192)
                .ok_or(Failure::Arithmetic)?,
        )
        .and_then(|x| x.checked_add((s.order.len() as u64).checked_mul(8)?))
        .and_then(|x| x.checked_add((s.priorities.len() as u64).checked_mul(32)?))
        .and_then(|x| x.checked_add((s.surplus.len() as u64).checked_mul(32)?))
        .ok_or(Failure::Arithmetic)
}
pub(super) fn charge_state(s: &State, m: &mut Meter) -> Result<(), Failure> {
    m.charge(Resource::Byte, state_bytes(s)?)?;
    for _ in &s.vaults {
        visit(m)?;
        m.charge(Resource::Step, 16)?;
    }
    for _ in &s.order {
        visit(m)?;
    }
    for _ in &s.priorities {
        visit(m)?;
    }
    for _ in &s.surplus {
        visit(m)?;
    }
    Ok(())
}
pub(super) fn index(s: &State, id: u32, m: &mut Meter) -> Result<usize, Failure> {
    let mut out = None;
    for (i, v) in s.vaults.iter().enumerate() {
        visit(m)?;
        m.charge(Resource::Byte, 192)?;
        if v.id == id {
            out = Some(i)
        }
    }
    out.ok_or(Failure::Undefined)
}
pub(super) fn pcmp(a: u128, b: u128, m: &mut Meter) -> Result<Ordering, Failure> {
    super::ordered::number_cmp(
        super::ordered::Priority::Unsigned(a),
        super::ordered::Priority::Unsigned(b),
        m,
    )
}
pub(super) fn priority(s: &State, id: u32, m: &mut Meter) -> Result<u128, Failure> {
    let mut out = None;
    for (key, p) in &s.priorities {
        visit(m)?;
        m.charge(Resource::Byte, 32)?;
        if *key == id {
            out = Some(*p)
        }
    }
    out.ok_or(Failure::Undefined)
}
fn validate(p: &Policy, s: &State, m: &mut Meter) -> Result<(), Failure> {
    if s.vaults.len() > p.inventory_capacity as usize
        || s.order.len() > p.capacity as usize
        || s.priorities.len() > p.capacity as usize
        || s.surplus.len() > p.inventory_capacity as usize
        || state_bytes(s)? > p.retained_bytes
    {
        return Err(Failure::Wire);
    }
    charge_state(s, m)?;
    for pair in s.vaults.windows(2) {
        visit(m)?;
        visit(m)?;
        step(m)?;
        m.charge(Resource::Byte, 392)?;
        if pair[0].id >= pair[1].id {
            return Err(Failure::Wire);
        }
    }
    for pairs in [&s.priorities, &s.surplus] {
        for pair in pairs.windows(2) {
            visit(m)?;
            visit(m)?;
            step(m)?;
            m.charge(Resource::Byte, 72)?;
            if pair[0].0 >= pair[1].0 {
                return Err(Failure::Wire);
            }
        }
    }
    for x in [
        s.reward_c,
        s.reward_d,
        s.stakes_snapshot,
        s.collateral_snapshot,
        s.active_collateral,
        s.default_collateral,
        s.active_debt,
        s.default_debt,
        s.holder_balance,
        s.reserve_balance,
        s.supply,
        s.holder_collateral,
        s.staking_collateral,
        s.base_rate,
    ] {
        step(m)?;
        let _ = x; // Nonnegativity is now guaranteed by U128; retain the tariff.
    }
    if s.base_rate > Q
        || s.supply != add(s.active_debt, s.default_debt, m)?
        || add(s.holder_balance, s.reserve_balance, m)? > s.supply
    {
        return Err(Failure::Law(981));
    }
    let mut active = 0usize;
    let mut raw_c = 0;
    let mut raw_d = 0;
    let mut pending_c = 0;
    let mut pending_d = 0;
    for (i, v) in s.vaults.iter().enumerate() {
        visit(m)?;
        for x in [v.collateral, v.debt, v.stake, v.snapshot_c, v.snapshot_d] {
            step(m)?;
            let _ = x;
        }
        if !v.active {
            if v.collateral != 0
                || v.debt != 0
                || v.stake != 0
                || v.snapshot_c != 0
                || v.snapshot_d != 0
            {
                return Err(Failure::Law(982));
            }
            continue;
        }
        if v.debt <= p.reserve {
            return Err(Failure::Law(982));
        }
        for prev in &s.vaults[..i] {
            visit(m)?;
            if prev.active && prev.owner == v.owner {
                return Err(Failure::Law(982));
            }
        }
        active += 1;
        raw_c = add(raw_c, v.collateral, m)?;
        raw_d = add(raw_d, v.debt, m)?;
        let (c, d) = effective(s, v, m)?;
        pending_c = add(pending_c, sub(c, v.collateral, m)?, m)?;
        pending_d = add(pending_d, sub(d, v.debt, m)?, m)?;
        let fresh = nicr(c, d, m)?;
        if fresh == 0 || pcmp(priority(s, v.id, m)?, fresh, m)? != Ordering::Equal {
            return Err(Failure::Law(983));
        }
    }
    if active != s.order.len()
        || active != s.priorities.len()
        || raw_c != s.active_collateral
        || raw_d != s.active_debt
        || pending_c > s.default_collateral
        || pending_d > s.default_debt
    {
        return Err(Failure::Law(984));
    }
    for (i, id) in s.order.iter().enumerate() {
        visit(m)?;
        let v = &s.vaults[index(s, *id, m)?];
        if !v.active {
            return Err(Failure::Law(984));
        }
        for other in &s.order[..i] {
            visit(m)?;
            visit(m)?;
            if id == other {
                return Err(Failure::Law(984));
            }
        }
        if i > 0
            && pcmp(priority(s, s.order[i - 1], m)?, priority(s, *id, m)?, m)? == Ordering::Less
        {
            return Err(Failure::Law(985));
        }
    }
    for (id, amount) in &s.surplus {
        visit(m)?;
        let _ = amount;
        let mut owner_present = false;
        for vault in &s.vaults {
            visit(m)?;
            step(m)?;
            m.charge(Resource::Byte, 200)?;
            owner_present |= vault.owner == *id;
        }
        if !owner_present {
            return Err(Failure::Law(984));
        }
    }
    Ok(())
}
fn settle(s: &mut State, i: usize, m: &mut Meter) -> Result<bool, Failure> {
    let (c, d) = effective(s, &s.vaults[i], m)?;
    let pending = s.vaults[i].snapshot_c < s.reward_c || s.vaults[i].snapshot_d < s.reward_d;
    if pending {
        let dc = sub(c, s.vaults[i].collateral, m)?;
        let dd = sub(d, s.vaults[i].debt, m)?;
        s.default_collateral = sub(s.default_collateral, dc, m)?;
        s.default_debt = sub(s.default_debt, dd, m)?;
        s.active_collateral = add(s.active_collateral, dc, m)?;
        s.active_debt = add(s.active_debt, dd, m)?;
        s.vaults[i].collateral = c;
        s.vaults[i].debt = d;
        s.vaults[i].snapshot_c = s.reward_c;
        s.vaults[i].snapshot_d = s.reward_d;
    }
    Ok(pending)
}
fn reinsert(
    s: &mut State,
    id: u32,
    p: u128,
    upper: Option<u32>,
    lower: Option<u32>,
    m: &mut Meter,
) -> Result<(), Failure> {
    // Removing and inserting operates only on private invocation scratch.
    let mut detached = Vec::new();
    m.charge(
        Resource::Byte,
        (s.order.len() as u64)
            .checked_mul(8)
            .ok_or(Failure::Arithmetic)?,
    )?;
    for x in &s.order {
        visit(m)?;
        if *x != id {
            detached.push(*x)
        }
    }
    let mut u = None;
    let mut l = None;
    for (i, x) in detached.iter().enumerate() {
        visit(m)?;
        if Some(*x) == upper {
            u = Some(i)
        }
        if Some(*x) == lower {
            l = Some(i)
        }
    }
    let mut chosen = None;
    // Enumerate all valid cuts, preserving V1's sanitized-anchor search direction.
    let mut gaps = Vec::new();
    m.charge(
        Resource::Byte,
        (detached.len() as u64 + 1)
            .checked_mul(8)
            .ok_or(Failure::Arithmetic)?,
    )?;
    for j in 0..=detached.len() {
        step(m)?;
        if (j == 0 || pcmp(priority(s, detached[j - 1], m)?, p, m)? != Ordering::Less)
            && (j == detached.len() || pcmp(p, priority(s, detached[j], m)?, m)? != Ordering::Less)
        {
            gaps.push(j)
        }
    }
    // Reserve both bounded scratch passes: hint membership and directional selection.
    for _ in 0..2 {
        for _ in &gaps {
            visit(m)?;
            m.charge(Resource::Byte, 8)?;
        }
    }
    let hint = match (u, l) {
        (None, None) if upper.is_none() && lower.is_none() && detached.is_empty() => Some(0),
        (None, Some(0)) if upper.is_none() => Some(0),
        (Some(i), None) if lower.is_none() && i + 1 == detached.len() => Some(detached.len()),
        (Some(i), Some(j)) if i + 1 == j => Some(j),
        _ => None,
    };
    if let Some(j) = hint {
        if gaps.contains(&j) {
            chosen = Some(j)
        }
    }
    if chosen.is_none() {
        if let Some(i) = u {
            if pcmp(priority(s, detached[i], m)?, p, m)? == Ordering::Less {
                u = None
            }
        }
        if let Some(i) = l {
            if pcmp(priority(s, detached[i], m)?, p, m)? == Ordering::Greater {
                l = None
            }
        }
        chosen = match (u, l) {
            (None, Some(j)) => {
                if j + 1 == detached.len()
                    && pcmp(p, priority(s, detached[j], m)?, m)? != Ordering::Greater
                {
                    Some(detached.len())
                } else {
                    gaps.iter().rev().copied().find(|x| *x <= j)
                }
            }
            (u, _) => {
                let i = u.unwrap_or(0);
                if detached.is_empty()
                    || i == 0 && pcmp(p, priority(s, detached[0], m)?, m)? != Ordering::Less
                {
                    Some(0)
                } else {
                    gaps.iter().copied().find(|x| *x > i)
                }
            }
        };
    }
    detached.insert(chosen.ok_or(Failure::Undefined)?, id);
    s.order = detached;
    for (key, old) in &mut s.priorities {
        visit(m)?;
        if *key == id {
            *old = p;
        }
    }
    Ok(())
}
fn decmul(a: u128, b: u128, m: &mut Meter) -> Result<u128, Failure> {
    // V1 _decMul rounds to nearest, half up; do not substitute floor.
    let floor = muldiv(a, b, Q, m)?;
    m.charge(Resource::Step, 128)?;
    m.charge(Resource::Byte, 34)?;
    let product = wide::multiply(a, b);
    // a,b<=Q, so this product is <=10^36 and fits a single u128 limb.
    if product[1] != 0 {
        return Err(Failure::Arithmetic);
    }
    add(floor, if product[0] % (Q) >= (Q / 2) { 1 } else { 0 }, m)
}
pub(super) fn decay(base: u128, minutes: u64, m: &mut Meter) -> Result<u128, Failure> {
    let mut n = minutes.min(525_600_000);
    if n == 0 {
        return Ok(base);
    }
    let mut x = DECAY;
    let mut y = Q;
    while n > 1 {
        step(m)?;
        if n % 2 == 0 {
            x = decmul(x, x, m)?;
            n /= 2
        } else {
            y = decmul(x, y, m)?;
            x = decmul(x, x, m)?;
            n = (n - 1) / 2
        }
    }
    let factor = decmul(x, y, m)?;
    muldiv(base, factor, Q, m)
}
impl Traversal {
    pub fn bind(policy: Policy) -> Result<Self, Failure> {
        if policy.inventory_capacity < policy.capacity
            || policy.retained_bytes < 1024
            || policy.minimum_net_debt == 0
            || policy.reserve == 0
            || policy.epoch_seconds == 0
            || policy.redemption_capacity == 0
        {
            return Err(Failure::Descriptor);
        }
        Ok(Self { policy })
    }
    pub fn evaluate(&self, pre: &State, command: &Command, context: Context) -> Evaluation {
        let mut m = meter::new(self.policy.limits);
        let mut result = self.run(pre, command, context, &mut m);
        if let Ok(candidate) = &result {
            if let Err(failure) = super::redemption_laws::verify(
                &self.policy,
                pre,
                command,
                context,
                candidate,
                &mut m,
            ) {
                result = Err(failure);
            }
        }
        Evaluation {
            result,
            usage: m.used,
        }
    }
    fn run(
        &self,
        pre: &State,
        c: &Command,
        x: Context,
        m: &mut Meter,
    ) -> Result<Result<Plan, Reason>, Failure> {
        let p = &self.policy;
        validate(p, pre, m)?;
        m.charge(Resource::Step, 64)?;
        m.charge(Resource::Byte, 1024)?;
        step(m)?;
        // Exact precedence after wire/invariant admission.
        if c.actor != x.actor || c.actor != pre.holder {
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
            || pre.price_epoch > x.epoch
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
        let total_c = add(pre.active_collateral, pre.default_collateral, m)?;
        let total_d = add(pre.active_debt, pre.default_debt, m)?;
        if !eligible(total_c, total_d, pre.price_e8, m)? {
            return Ok(Err(Reason::Tcr));
        }
        if c.maximum == 0 {
            return Ok(Err(Reason::Amount));
        }
        if pre.holder_balance < c.maximum {
            return Ok(Err(Reason::Balance));
        }
        charge_state(pre, m)?;
        let mut s = pre.clone();
        m.charge(
            Resource::Byte,
            (s.order.len() as u64)
                .checked_mul(24)
                .ok_or(Failure::Arithmetic)?,
        )?;
        let mut trace = Trace {
            visited: Vec::new(),
            closed: Vec::new(),
            first: None,
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
        let n = s.order.len();
        let cap = if c.max_visits == 0 {
            n
        } else {
            (c.max_visits as usize).min(n)
        };
        // Independently prove the first eligible ID, including all lower entries.
        let mut start = None;
        for j in (0..n).rev() {
            visit(m)?;
            let i = index(&s, s.order[j], m)?;
            let (coll, debt) = effective(&s, &s.vaults[i], m)?;
            if eligible(coll, debt, s.price_e8, m)? {
                start = Some(j);
                break;
            }
        }
        trace.first = start.map(|j| s.order[j]);
        trace.first_hint_used = c.first.is_some() && c.first == trace.first;
        // Follow the saved original predecessor sequence; no best-paying jumps.
        if let Some(start) = start {
            for j in (0..=start).rev() {
                if trace.unfilled == 0 {
                    trace.stop = Stop::Filled;
                    break;
                }
                if trace.visited.len() == cap {
                    trace.stop = Stop::VisitCap;
                    break;
                }
                visit(m)?;
                m.charge(Resource::Step, 64)?;
                m.charge(Resource::Byte, 512)?;
                let id = pre.order[j];
                trace.visited.push(id);
                let i = index(&s, id, m)?;
                let (coll, debt) = effective(&s, &s.vaults[i], m)?;
                // Intentional ROUND_B2 divergence from V1's unchecked main-loop eligibility.
                if !eligible(coll, debt, s.price_e8, m)? {
                    continue;
                }
                if settle(&mut s, i, m)? {
                    trace.settled.push(id)
                }
                let net = sub(s.vaults[i].debt, p.reserve, m)?;
                let lot = trace.unfilled.min(net);
                let drawn = muldiv(lot, 100_000_000, s.price_e8, m)?;
                let new_c = sub(s.vaults[i].collateral, drawn, m)?;
                let new_d = sub(s.vaults[i].debt, lot, m)?;
                if lot == net {
                    if s.order.len() <= 1 {
                        return Ok(Err(Reason::LastVault));
                    }
                    trace.closed.push(id);
                    trace.reserve_burn = add(trace.reserve_burn, p.reserve, m)?;
                    trace.surplus = add(trace.surplus, new_c, m)?;
                    s.active_collateral = sub(s.active_collateral, add(drawn, new_c, m)?, m)?;
                    s.active_debt = sub(s.active_debt, add(lot, p.reserve, m)?, m)?;
                    let owner = s.vaults[i].owner;
                    let mut found = false;
                    for (key, amount) in &mut s.surplus {
                        visit(m)?;
                        if *key == owner {
                            *amount = add(*amount, new_c, m)?;
                            found = true
                        }
                    }
                    if !found {
                        m.charge(Resource::Byte, 32)?;
                        for _ in &s.surplus {
                            visit(m)?;
                        }
                        let pos = s
                            .surplus
                            .iter()
                            .position(|(key, _)| *key > owner)
                            .unwrap_or(s.surplus.len());
                        s.surplus.insert(pos, (owner, new_c));
                    }
                    s.vaults[i].active = false;
                    s.vaults[i].collateral = 0;
                    s.vaults[i].debt = 0;
                    s.vaults[i].stake = 0;
                    s.vaults[i].snapshot_c = 0;
                    s.vaults[i].snapshot_d = 0;
                    for _ in &s.order {
                        visit(m)?;
                    }
                    for _ in &s.priorities {
                        visit(m)?;
                    }
                    s.order.retain(|x| *x != id);
                    s.priorities.retain(|(x, _)| *x != id);
                } else {
                    let new_nicr = nicr(new_c, new_d, m)?;
                    if pcmp(new_nicr, c.partial_nicr, m)? != Ordering::Equal
                        || sub(new_d, p.reserve, m)? < p.minimum_net_debt
                    {
                        trace.stop = Stop::CancelledPartial;
                        break;
                    }
                    s.vaults[i].collateral = new_c;
                    s.vaults[i].debt = new_d;
                    s.vaults[i].stake = if s.collateral_snapshot == 0 {
                        new_c
                    } else {
                        if s.stakes_snapshot == 0 {
                            return Err(Failure::Law(986));
                        }
                        muldiv(new_c, s.stakes_snapshot, s.collateral_snapshot, m)?
                    };
                    s.active_collateral = sub(s.active_collateral, drawn, m)?;
                    s.active_debt = sub(s.active_debt, lot, m)?;
                    reinsert(&mut s, id, new_nicr, c.upper, c.lower, m)?;
                }
                trace.executed = add(trace.executed, lot, m)?;
                trace.unfilled = sub(trace.unfilled, lot, m)?;
                trace.gross_collateral = add(trace.gross_collateral, drawn, m)?;
                if trace.unfilled == 0 {
                    trace.stop = Stop::Filled;
                    break;
                }
            }
        }
        if trace.executed == 0 || trace.gross_collateral == 0 {
            return Ok(Err(Reason::NoPositiveExecution));
        }
        let seconds = x
            .epoch
            .checked_sub(pre.last_fee_epoch)
            .and_then(|n| n.checked_mul(p.epoch_seconds))
            .ok_or(Failure::Arithmetic)?;
        let decayed = decay(pre.base_rate, seconds / 60, m)?;
        let fraction = mul3div(
            trace.gross_collateral,
            pre.price_e8,
            10_000_000_000,
            pre.supply,
            m,
        )?;
        s.base_rate = add(decayed, fraction / 2, m)?.min(Q);
        if s.base_rate == 0 {
            return Err(Failure::Arithmetic);
        }
        if seconds >= 60 {
            s.last_fee_epoch = x.epoch;
        }
        let rate = add(Q / 200, s.base_rate, m)?.min(Q);
        trace.fee = muldiv(rate, trace.gross_collateral, Q, m)?;
        if trace.fee >= trace.gross_collateral {
            return Ok(Err(Reason::FeeConsumesAll));
        }
        if muldiv(trace.fee, Q, trace.gross_collateral, m)? > c.max_fee {
            return Ok(Err(Reason::FeeExceeded));
        }
        s.holder_balance = sub(s.holder_balance, trace.executed, m)?;
        s.reserve_balance = sub(s.reserve_balance, trace.reserve_burn, m)?;
        s.supply = sub(s.supply, add(trace.executed, trace.reserve_burn, m)?, m)?;
        s.holder_collateral = add(
            s.holder_collateral,
            sub(trace.gross_collateral, trace.fee, m)?,
            m,
        )?;
        s.staking_collateral = add(s.staking_collateral, trace.fee, m)?;
        s.nonce = pre.nonce.checked_add(1).ok_or(Failure::Arithmetic)?;
        validate(p, &s, m)?;
        if sub(pre.supply, s.supply, m)? != add(trace.executed, trace.reserve_burn, m)?
            || sub(
                add(pre.active_collateral, pre.default_collateral, m)?,
                add(s.active_collateral, s.default_collateral, m)?,
                m,
            )? != add(trace.gross_collateral, trace.surplus, m)?
        {
            return Err(Failure::Law(987));
        }
        charge_state(&s, m)?;
        m.charge(
            Resource::Byte,
            256 + (trace.visited.len() + trace.closed.len() + trace.settled.len()) as u64 * 8,
        )?;
        m.charge(Resource::Candidate, 1)?;
        Ok(Ok(Plan { state: s, trace }))
    }
    /// Independently check the complete redemption outcome.
    pub fn check_laws(
        &self,
        pre: &State,
        command: &Command,
        context: Context,
        state: Option<&State>,
        trace: Option<&Trace>,
        reason: Option<Reason>,
    ) -> OrderCheck {
        let mut m = meter::new(self.policy.limits);
        let result = (|| {
            validate(&self.policy, pre, &mut m)?;
            if let Some(s) = state {
                validate(&self.policy, s, &mut m)?;
            }
            if trace.is_some_and(|t| {
                t.visited.len() > self.policy.capacity as usize
                    || t.closed.len() > self.policy.capacity as usize
                    || t.settled.len() > self.policy.capacity as usize
            }) {
                return Err(Failure::Wire);
            }
            super::redemption_laws::probe(
                &self.policy,
                pre,
                command,
                context,
                state,
                trace,
                reason,
                &mut m,
            )
        })();
        OrderCheck {
            result,
            usage: m.used,
        }
    }
    pub fn check_redistribution(&self, pre: &State, candidate: &State) -> OrderCheck {
        let mut m = meter::new(self.policy.limits);
        let result = (|| {
            validate(&self.policy, pre, &mut m)?;
            validate(&self.policy, candidate, &mut m)?;
            super::redemption_laws::redistribution(pre, candidate, &mut m)
        })();
        OrderCheck {
            result,
            usage: m.used,
        }
    }
    /// Advance already funded reward accumulators and repair their own rounding
    /// inversions atomically. This restricted candidate changes no monetary field
    /// and carries no publication authority. External malformed order still fails.
    pub fn redistribute(&self, pre: &State, reward_c: u128, reward_d: u128) -> OrderRepair {
        let mut m = meter::new(self.policy.limits);
        let result = (|| {
            validate(&self.policy, pre, &mut m)?;
            m.charge(Resource::Step, 2)?;
            m.charge(Resource::Byte, 34)?;
            if reward_c < pre.reward_c || reward_d < pre.reward_d {
                return Err(Failure::Arithmetic);
            }
            charge_state(pre, &mut m)?;
            let mut s = pre.clone();
            s.reward_c = reward_c;
            s.reward_d = reward_d;
            // Recompute every cached key before inspecting adjacency. The inputs
            // are accumulator increments, never caller-supplied effective amounts.
            for i in 0..s.priorities.len() {
                visit(&mut m)?;
                let id = s.priorities[i].0;
                let v = &s.vaults[index(&s, id, &mut m)?];
                let (c, d) = effective(&s, v, &mut m)?;
                let fresh = nicr(c, d, &mut m)?;
                m.charge(Resource::Write, 1)?;
                s.priorities[i].1 = fresh;
            }
            // Insertion induction: the prefix is sorted and stable. Only a vault
            // that inverts that prefix is reinserted. A checked V1 cut immediately
            // after existing equal keys preserves their original committed order.
            m.charge(Resource::Byte, (pre.order.len() as u64) * 8)?;
            s.order = Vec::new();
            for id in &pre.order {
                visit(&mut m)?;
                let p = priority(&s, *id, &mut m)?;
                let inversion = if let Some(last) = s.order.last() {
                    pcmp(priority(&s, *last, &mut m)?, p, &mut m)? == Ordering::Less
                } else {
                    false
                };
                if inversion {
                    let mut upper = None;
                    let mut lower = None;
                    for x in &s.order {
                        visit(&mut m)?;
                        if pcmp(priority(&s, *x, &mut m)?, p, &mut m)? != Ordering::Less {
                            upper = Some(*x);
                        } else if lower.is_none() {
                            lower = Some(*x);
                        }
                    }
                    // Restore reInsert's existing-ID precondition; its detached
                    // output reservation then includes the full inserted slot.
                    s.order.push(*id);
                    reinsert(&mut s, *id, p, upper, lower, &mut m)?;
                } else {
                    s.order.push(*id);
                }
                m.charge(Resource::Write, 1)?;
            }
            validate(&self.policy, &s, &mut m)?;
            super::redemption_laws::redistribution(pre, &s, &mut m)?;
            charge_state(&s, &mut m)?;
            m.charge(Resource::Candidate, 1)?;
            Ok(s)
        })();
        OrderRepair {
            result,
            usage: m.used,
        }
    }
}
/// Owned restricted redistribution candidate, never a custody/publication token.
#[derive(Debug)]
pub struct OrderRepair {
    result: Result<State, Failure>,
    usage: Usage,
}
impl OrderRepair {
    pub fn result(&self) -> Result<&State, &Failure> {
        self.result.as_ref()
    }
    pub fn usage(&self) -> Usage {
        self.usage
    }
}
/// Checked observations only; there is no successor or publication capability.
#[derive(Debug)]
pub struct OrderCheck {
    result: Result<(), Failure>,
    usage: Usage,
}
impl OrderCheck {
    pub fn result(&self) -> Result<(), &Failure> {
        self.result.as_ref().map(|_| ())
    }
    pub fn usage(&self) -> Usage {
        self.usage
    }
}
