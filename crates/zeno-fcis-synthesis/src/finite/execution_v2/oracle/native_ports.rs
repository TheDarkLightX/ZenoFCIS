//! Oracle-internal transactional ports. External clock/actor/adjudicator facts
//! remain explicit premises; these joins grant no Balance or consumer authority.

use super::aggregation::Aggregate;
use super::bonds::{BondCommand, DisputePhase};
use super::lifecycle::{LifecycleContext, LifecycleState, NativePriceCommand};
use super::prices::PriceContext;
use super::prices::{ClockView, Pending, PriceCommand, PriceView, SecurityInputs};
use super::types::{Hash, Id, Reason, StressE8, ZdexE18};
use super::{Evaluation, Failure, Limits, Meter, measured};
use alloc::vec::Vec;

pub(super) fn quarantines(s: &LifecycleState, feed: Hash) -> Vec<Hash> {
    let mut result = Vec::new();
    for b in s.bonds.values() {
        for l in &b.liabilities {
            if l.feed == feed
                && b.disputes
                    .iter()
                    .any(|d| d.liability == l.id && d.phase != DisputePhase::ResolvedRejected)
            {
                result.push(l.report);
            }
        }
    }
    result.sort();
    result.dedup();
    result
}
/// Only the exact report's remaining allocation is collectible. Free deposits,
/// other rounds/feeds and already applied penalties cannot increase this port.
pub(super) fn security(
    s: &LifecycleState,
    feed: Hash,
    a: Option<&Aggregate>,
    now: u64,
) -> Result<SecurityInputs, Failure> {
    let p = &s.profile.parameters;
    let e = p.economy.as_ref().ok_or(Failure::UnboundParameter)?;
    let d = p.disputes.as_ref().ok_or(Failure::UnboundParameter)?;
    let c = p.clock.as_ref().ok_or(Failure::UnboundParameter)?;
    let mut selected_bonds = [ZdexE18(0); 3];
    if let Some(a) = a {
        for (i, slot) in selected_bonds.iter_mut().enumerate() {
            if let Some(b) = s.bonds.get(&a.reporters()[i])
                && let Some(l) = b
                    .liabilities
                    .iter()
                    .find(|l| l.report == a.reports()[i] && l.feed == feed && !l.cleared)
            {
                *slot = ZdexE18(l.locked.0.min(d.slash.0));
            }
        }
    }
    let o = s.stress.get(&feed);
    let fresh = o.filter(|o| o.observed_epoch <= now && now - o.observed_epoch <= c.max_age);
    let stress = fresh
        .and_then(|o| o.value)
        .map(|v| {
            crate::finite::value::wide::mul_div_floor(
                v.0,
                e.stress_haircut_numerator,
                e.stress_haircut_denominator,
            )
            .map(StressE8)
            .ok_or(Failure::Arithmetic)
        })
        .transpose()?;
    Ok(SecurityInputs {
        selected_bonds,
        stress,
        stress_source: e.stress_source,
        valuation_root: fresh.map_or([0; 32], |o| o.valuation_root),
    })
}
fn lower(
    s: &LifecycleState,
    feed: Hash,
    c: &NativePriceCommand,
    now: u64,
    m: &mut Meter,
) -> Result<Result<PriceCommand, Reason>, Failure> {
    let f = s.feeds.get(&feed).ok_or(Failure::InvalidState)?;
    Ok(Ok(match c {
        NativePriceCommand::Stage { aggregate } => {
            let Some(a) = s.aggregates.get(aggregate) else {
                return Ok(Err(Reason::UnknownReference));
            };
            if a.feed() != feed {
                return Ok(Err(Reason::BindingMismatch));
            }
            PriceCommand::Stage((a.clone()).into())
        }
        NativePriceCommand::Commit => PriceCommand::Commit,
        NativePriceCommand::AdvanceEpoch => PriceCommand::AdvanceEpoch,
        NativePriceCommand::SyncTime { round } => {
            let Some(r) = s.rounds.get(&(feed, *round)) else {
                return Ok(Err(Reason::UnknownReference));
            };
            let all = super::lifecycle::observations(s, feed, *round);
            let q = match super::aggregation::checked_time(
                &f.rules, *round, r.cutoff, now, &all, &f.sources, m,
            )? {
                Ok(q) => q,
                Err(r) => return Ok(Err(r)),
            };
            if q.reports.iter().any(|id| quarantines(s, feed).contains(id)) {
                return Ok(Err(Reason::DisputedOrFrozen));
            }
            PriceCommand::SyncTime(q)
        }
    }))
}
pub(super) fn run_price(
    s: &LifecycleState,
    _actor: &Id,
    ctx: LifecycleContext,
    feed: Hash,
    c: &NativePriceCommand,
    m: &mut Meter,
) -> Result<Result<LifecycleState, Reason>, Failure> {
    let command = match lower(s, feed, c, ctx.clock.epoch, m)? {
        Ok(c) => c,
        Err(r) => return Ok(Err(r)),
    };
    let mut input = s.prices.get(&feed).ok_or(Failure::InvalidState)?.clone();
    input.quarantines = quarantines(s, feed);
    // The outer invocation owns replay; this projected local nonce cannot be
    // retained or used as a second issuer authorization.
    input.nonce = 0;
    let a = match &command {
        PriceCommand::Stage(a) => Some(a.as_ref()),
        PriceCommand::Commit => input.pending.as_ref().map(|p| &p.aggregate),
        _ => input.committed.as_ref(),
    };
    let sec = security(s, feed, a, ctx.clock.epoch)?;
    let result = super::prices::checked(
        &s.feeds.get(&feed).ok_or(Failure::InvalidState)?.rules,
        PriceContext {
            parameters: &s.profile.parameters,
            nonce: 0,
            clock: ctx.clock,
            security: &sec,
        },
        &input,
        &command,
        m,
    )?;
    let mut price = match result {
        Ok(p) => p.into_state(),
        Err(r) => return Ok(Err(r)),
    };
    price.nonce = 0;
    price.quarantines.clear();
    let mut out = s.clone();
    if let PriceCommand::Stage(a) = &command {
        for reporter in a.reporters() {
            let b = s.bonds.get(reporter).ok_or(Failure::InvalidState)?;
            let l = b
                .liabilities
                .iter()
                .find(|l| a.reports().contains(&l.report) && l.feed == feed)
                .ok_or(Failure::InvalidState)?;
            let op = BondCommand::RecordPublication {
                liability: l.id,
                aggregate: (a.clone()),
            };
            let b = super::lifecycle::apply_bond(s, reporter, &b.owner, &op, ctx.clock, m)?;
            let mut b = match b {
                Ok(b) => b,
                Err(r) => return Ok(Err(r)),
            };
            b.wallets.clear();
            b.nonces.clear();
            out.bonds.insert(reporter.clone(), b);
        }
    }
    if let PriceCommand::SyncTime(q) = command {
        out.time_quorums.insert(feed, q);
    }
    out.prices.insert(feed, price);
    Ok(Ok(out))
}

/// Complete port law: independently derive the required component command,
/// check its exact outcome law, check each publication bond law, then compare
/// the entire lifecycle frame (including replay, money and old key history).
pub(super) fn check_price_port(
    s: &LifecycleState,
    actor: &Id,
    ctx: LifecycleContext,
    feed: Hash,
    c: &NativePriceCommand,
    proposed: Result<&LifecycleState, Reason>,
    m: &mut Meter,
) -> Result<(), Failure> {
    let expected = reference_price(s, actor, ctx, feed, c, proposed.as_ref().ok().copied(), m)?;
    match (&expected, proposed) {
        (Ok(w), Ok(a)) if w == a => Ok(()),
        (Err(w), Err(a)) if *w == a => Ok(()),
        (Ok(_), Err(r)) => Err(Failure::Law(116_100 + r as u32)),
        _ => Err(Failure::Law(116_200)),
    }
}
fn reference_price(
    s: &LifecycleState,
    actor: &Id,
    ctx: LifecycleContext,
    feed: Hash,
    c: &NativePriceCommand,
    actual: Option<&LifecycleState>,
    m: &mut Meter,
) -> Result<Result<LifecycleState, Reason>, Failure> {
    if s.root() != Ok(ctx.pre_root) {
        return Err(Failure::InvalidEvidence);
    }
    let timing = s
        .profile
        .parameters
        .clock
        .as_ref()
        .ok_or(Failure::UnboundParameter)?;
    if ctx.clock.backend != timing.backend || ctx.clock.finality_root == [0; 32] {
        return Err(Failure::InvalidEvidence);
    }
    if ctx.nonce != s.nonce(actor) {
        return Ok(Err(Reason::Replay));
    }
    let Some(next) = ctx.nonce.checked_add(1) else {
        return Ok(Err(Reason::CounterExhausted));
    };
    if ctx.clock.epoch < s.epoch {
        return Ok(Err(Reason::BadClockOrSync));
    }
    let Some(f) = s.feeds.get(&feed) else {
        return Ok(Err(Reason::UnknownReference));
    };
    let mut post = s.clone();
    let command = match c {
        NativePriceCommand::Stage { aggregate } => {
            let Some(a) = s.aggregates.get(aggregate) else {
                return Ok(Err(Reason::UnknownReference));
            };
            if a.feed() != feed {
                return Ok(Err(Reason::BindingMismatch));
            }
            PriceCommand::Stage((a.clone()).into())
        }
        NativePriceCommand::Commit => PriceCommand::Commit,
        NativePriceCommand::AdvanceEpoch => PriceCommand::AdvanceEpoch,
        NativePriceCommand::SyncTime { round } => {
            let Some(r) = s.rounds.get(&(feed, *round)) else {
                return Ok(Err(Reason::UnknownReference));
            };
            if ctx.clock.epoch < r.cutoff {
                return Ok(Err(Reason::WrongPhase));
            }
            let q = match super::quorum_laws::time_reference(
                &f.rules,
                *round,
                r.cutoff,
                ctx.clock.epoch,
                &super::lifecycle::observations(s, feed, *round),
                &f.sources,
                m,
            )? {
                Ok(q) => q,
                Err(r) => return Ok(Err(r)),
            };
            let blocked = quarantines(s, feed);
            if q.reports.iter().any(|id| blocked.contains(id)) {
                return Ok(Err(Reason::DisputedOrFrozen));
            }
            PriceCommand::SyncTime(q)
        }
    };
    let mut input = s.prices.get(&feed).ok_or(Failure::InvalidState)?.clone();
    input.nonce = 0;
    input.quarantines = quarantines(s, feed);
    let a = match &command {
        PriceCommand::Stage(a) => Some(a.as_ref()),
        PriceCommand::Commit => input.pending.as_ref().map(|p| &p.aggregate),
        _ => input.committed.as_ref(),
    };
    let sec = security(s, feed, a, ctx.clock.epoch)?;
    // Derive the successor rather than calling the price producer. Its complete
    // law independently decides all ordered guards, exact fields and B10 status.
    let mut candidate = input.clone();
    candidate.epoch = ctx.clock.epoch;
    candidate.nonce = 1;
    match &command {
        PriceCommand::Stage(a) => {
            candidate.pending = Some(Pending {
                aggregate: a.as_ref().clone(),
                published: ctx.clock.epoch,
                dispute_until: ctx
                    .clock
                    .epoch
                    .checked_add(timing.dispute_open)
                    .ok_or(Failure::Arithmetic)?,
                earliest: ctx
                    .clock
                    .epoch
                    .checked_add(timing.confirmation.max(timing.dispute_open))
                    .ok_or(Failure::Arithmetic)?,
            })
        }
        PriceCommand::Commit => {
            if let Some(p) = &input.pending {
                candidate.committed = Some(p.aggregate.clone());
                candidate.history.push(p.aggregate.clone());
                candidate.pending = None;
            }
        }
        PriceCommand::SyncTime(q) => candidate.watermark = input.watermark.max(q.seconds),
        PriceCommand::AdvanceEpoch => {}
        PriceCommand::Quarantine(_) => return Err(Failure::InvalidState),
    }
    if let Some(r) = super::laws::price_reason(&f.rules, &input, 0, &command, ctx.clock, m)? {
        return Ok(Err(r));
    }
    let status = super::prices::status(&s.profile.parameters, &sec, m)?;
    super::laws::price(
        &f.rules,
        PriceContext {
            parameters: &s.profile.parameters,
            nonce: 0,
            clock: ctx.clock,
            security: &sec,
        },
        &input,
        &command,
        Ok((&candidate, status)),
        m,
    )?;
    candidate.nonce = 0;
    candidate.quarantines.clear();
    if let PriceCommand::Stage(a) = &command {
        for reporter in a.reporters() {
            let mut b = s.bonds.get(reporter).ok_or(Failure::InvalidState)?.clone();
            let owner = b.owner.clone();
            let l = b
                .liabilities
                .iter()
                .find(|l| a.reports().contains(&l.report) && l.feed == feed)
                .ok_or(Failure::InvalidState)?
                .id;
            b.wallets = s.wallets.clone();
            b.nonces.clear();
            let mut b = match super::bond_laws::reference(
                &s.profile.parameters,
                &b,
                &owner,
                0,
                &BondCommand::RecordPublication {
                    liability: l,
                    aggregate: (a.clone()),
                },
                ctx.clock,
                s.profile.bounds.max_entries,
            )? {
                Ok(b) => b,
                Err(r) => return Ok(Err(r)),
            };
            b.wallets.clear();
            b.nonces.clear();
            post.bonds.insert(reporter.clone(), b);
        }
    }
    if let PriceCommand::SyncTime(q) = command {
        post.time_quorums.insert(feed, q);
    }
    post.prices.insert(feed, candidate);
    post.epoch = ctx.clock.epoch;
    post.nonces.insert(actor.clone(), next);
    // actual is intentionally unused until the full-frame comparison above.
    let _ = actual;
    match super::lifecycle_schema::fit(&post)? {
        Ok(()) => Ok(Ok(post)),
        Err(r) => Ok(Err(r)),
    }
}

/// Derived read data only. Consumer/action, Balance and exposure authorization
/// belongs to phase 4; this function cannot publish or issue such a receipt.
pub fn native_committed_view(
    s: &LifecycleState,
    feed: Hash,
    clock: ClockView,
    limits: Limits,
) -> Evaluation<Result<PriceView, Reason>> {
    measured(limits, |m| {
        super::wire::reserve_scans(super::lifecycle::charged_row_count(s, m)?, m)?;
        let Some(f) = s.feeds.get(&feed) else {
            return Ok(Err(Reason::UnknownReference));
        };
        if clock.epoch < s.epoch {
            return Ok(Err(Reason::BadClockOrSync));
        }
        let mut p = s.prices.get(&feed).ok_or(Failure::InvalidState)?.clone();
        p.quarantines = quarantines(s, feed);
        let security = security(s, feed, p.committed.as_ref(), clock.epoch)?;
        super::prices::checked_view(&f.rules, &s.profile.parameters, &p, clock, &security, m)
    })
}
