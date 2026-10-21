//! Retained-event reward joins. Funding and held claims use one denomination-
//! qualified wallet prestate; no caller supplies eligibility or a reward amount.

use super::bonds::AccountContext;
use super::bonds::DisputePhase;
use super::lifecycle::{LifecycleContext, LifecycleState, NativeRewardCommand};
use super::prices::ClockView;
use super::rewards::{ClaimPhase, RewardCommand, RewardPlan, RewardRules, RewardState};
use super::types::{Hash, Id, Reason, ZdexE18};
use super::{Failure, Meter};
use alloc::{collections::BTreeMap, vec::Vec};

pub(super) fn rules(s: &LifecycleState, feed: Hash) -> Result<RewardRules, Failure> {
    let e = s
        .profile
        .parameters
        .economy
        .as_ref()
        .ok_or(Failure::UnboundParameter)?;
    let q = &s.profile.reward_split;
    Ok(RewardRules {
        feed,
        profile: s.profile_id,
        unit: e.reward_unit.clone(),
        per_report: e.reward_per_report,
        reporter_share: q.reporter,
        treasury_share: q.treasury,
        burn_share: q.burn,
        denominator: q.denominator,
        treasury: q.treasury_account.clone(),
    })
}
pub(super) fn empty(s: &LifecycleState, feed: Hash) -> Result<RewardState, Failure> {
    Ok(RewardState {
        feed,
        profile: s.profile_id,
        unit: rules(s, feed)?.unit,
        funded: 0,
        remaining: 0,
        paid: 0,
        treasury_paid: 0,
        burned: 0,
        residual: 0,
        wallets: BTreeMap::new(),
        claims: Vec::new(),
        nonces: BTreeMap::new(),
        epoch: 0,
    })
}
fn input(s: &LifecycleState, feed: Hash) -> Result<RewardState, Failure> {
    let mut r = s.rewards.get(&feed).ok_or(Failure::InvalidState)?.clone();
    r.nonces.clear();
    r.wallets = if r.unit.asset == s.profile.zdex_asset {
        s.wallets.iter().map(|(id, n)| (id.clone(), n.0)).collect()
    } else {
        s.reward_wallets.clone()
    };
    Ok(r)
}
fn retain(s: &mut LifecycleState, feed: Hash, mut r: RewardState) {
    let wallets = core::mem::take(&mut r.wallets);
    if r.unit.asset == s.profile.zdex_asset {
        s.wallets = wallets
            .into_iter()
            .map(|(id, n)| (id, ZdexE18(n)))
            .collect();
    } else {
        s.reward_wallets = wallets;
    }
    r.nonces.clear();
    s.rewards.insert(feed, r);
}
fn lower(
    s: &LifecycleState,
    feed: Hash,
    c: &NativeRewardCommand,
) -> Result<Result<(RewardCommand, Option<Id>), Reason>, Failure> {
    Ok(Ok(match c {
        NativeRewardCommand::Fund(n) => (RewardCommand::Fund(*n), None),
        NativeRewardCommand::Accrue { aggregate, report } => {
            let Some(a) = s.aggregates.get(aggregate) else {
                return Ok(Err(Reason::UnknownReference));
            };
            let Some(r) = s.reports.get(report) else {
                return Ok(Err(Reason::UnknownReference));
            };
            if a.feed() != feed || r.report.payload.feed != feed {
                return Ok(Err(Reason::BindingMismatch));
            }
            if !s.prices.get(&feed).is_some_and(|p| {
                p.history.iter().any(|a| a.id() == *aggregate)
                    || p.pending
                        .as_ref()
                        .is_some_and(|p| p.aggregate.id() == *aggregate)
            }) {
                return Ok(Err(Reason::WrongPhase));
            }
            if super::native_ports::quarantines(s, feed).contains(report) {
                return Ok(Err(Reason::DisputedOrFrozen));
            }
            (
                RewardCommand::Accrue {
                    aggregate: (a.clone()).into(),
                    report: *report,
                },
                Some(r.report.payload.reporter.clone()),
            )
        }
        NativeRewardCommand::Claim(id) | NativeRewardCommand::Clawback(id) => {
            let Some(claim) = s
                .rewards
                .get(&feed)
                .and_then(|r| r.claims.iter().find(|r| r.id == *id))
            else {
                return Ok(Err(Reason::UnknownReference));
            };
            let Some(r) = s.reports.get(&claim.report) else {
                return Err(Failure::InvalidState);
            };
            (
                if matches!(c, NativeRewardCommand::Claim(_)) {
                    RewardCommand::Claim(*id)
                } else {
                    RewardCommand::Clawback(*id)
                },
                Some(r.report.payload.reporter.clone()),
            )
        }
    }))
}
pub(super) fn run(
    s: &LifecycleState,
    actor: &Id,
    ctx: LifecycleContext,
    feed: Hash,
    c: &NativeRewardCommand,
    m: &mut Meter,
) -> Result<Result<LifecycleState, Reason>, Failure> {
    let (c, who) = match lower(s, feed, c)? {
        Ok(v) => v,
        Err(r) => return Ok(Err(r)),
    };
    let bond = who.as_ref().and_then(|id| s.bonds.get(id));
    let result = super::rewards::checked(
        &rules(s, feed)?,
        AccountContext {
            parameters: &s.profile.parameters,
            actor,
            nonce: 0,
            clock: ctx.clock,
            capacity: s.profile.bounds.max_entries,
        },
        &input(s, feed)?,
        bond,
        &c,
        m,
    )?;
    let r = match result {
        Ok(p) => p.into_state(),
        Err(r) => return Ok(Err(r)),
    };
    let mut post = s.clone();
    retain(&mut post, feed, r);
    Ok(Ok(post))
}
pub(super) fn unauthorized(
    s: &LifecycleState,
    actor: &Id,
    feed: Hash,
    c: &NativeRewardCommand,
) -> bool {
    matches!(c,NativeRewardCommand::Claim(id) if s.rewards.get(&feed).is_some_and(|r|r.claims.iter().any(|r|r.id==*id && &r.beneficiary!=actor)))
}

pub(super) fn check(
    s: &LifecycleState,
    actor: &Id,
    ctx: LifecycleContext,
    feed: Hash,
    c: &NativeRewardCommand,
    actual: Result<&LifecycleState, Reason>,
    m: &mut Meter,
) -> Result<(), Failure> {
    let want = reference(s, actor, ctx, feed, c, m)?;
    match (&want, actual) {
        (Ok(w), Ok(a)) if w == a => Ok(()),
        (Err(w), Err(a)) if *w == a => Ok(()),
        (Ok(_), Err(r)) => Err(Failure::Law(117_100 + r as u32)),
        _ => Err(Failure::Law(117_200)),
    }
}
fn reference(
    s: &LifecycleState,
    actor: &Id,
    ctx: LifecycleContext,
    feed: Hash,
    c: &NativeRewardCommand,
    m: &mut Meter,
) -> Result<Result<LifecycleState, Reason>, Failure> {
    let p = &s.profile.parameters;
    let t = p.clock.as_ref().ok_or(Failure::UnboundParameter)?;
    if s.root() != Ok(ctx.pre_root)
        || ctx.clock.backend != t.backend
        || ctx.clock.finality_root == [0; 32]
    {
        return Err(Failure::InvalidEvidence);
    }
    if let NativeRewardCommand::Claim(id) = c
        && s.rewards.get(&feed).is_some_and(|r| {
            r.claims
                .iter()
                .any(|r| r.id == *id && &r.beneficiary != actor)
        })
    {
        return Ok(Err(Reason::Unauthorized));
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
    let Some(pool) = s.rewards.get(&feed) else {
        return Ok(Err(Reason::UnknownReference));
    };
    let (command, who) = match c {
        NativeRewardCommand::Fund(n) => (RewardCommand::Fund(*n), None),
        NativeRewardCommand::Accrue { aggregate, report } => {
            let (Some(a), Some(row)) = (s.aggregates.get(aggregate), s.reports.get(report)) else {
                return Ok(Err(Reason::UnknownReference));
            };
            if a.feed() != feed || row.report.payload.feed != feed {
                return Ok(Err(Reason::BindingMismatch));
            }
            if !s.prices.get(&feed).is_some_and(|p| {
                p.history.iter().any(|a| a.id() == *aggregate)
                    || p.pending
                        .as_ref()
                        .is_some_and(|p| p.aggregate.id() == *aggregate)
            }) {
                return Ok(Err(Reason::WrongPhase));
            }
            if s.bonds.values().any(|b| {
                b.liabilities.iter().any(|l| {
                    l.report == *report
                        && b.disputes.iter().any(|d| {
                            d.liability == l.id && d.phase != DisputePhase::ResolvedRejected
                        })
                })
            }) {
                return Ok(Err(Reason::DisputedOrFrozen));
            }
            (
                RewardCommand::Accrue {
                    aggregate: (a.clone()).into(),
                    report: *report,
                },
                Some(row.report.payload.reporter.clone()),
            )
        }
        NativeRewardCommand::Claim(id) | NativeRewardCommand::Clawback(id) => {
            let Some(claim) = pool.claims.iter().find(|r| r.id == *id) else {
                return Ok(Err(Reason::UnknownReference));
            };
            let r = s.reports.get(&claim.report).ok_or(Failure::InvalidState)?;
            (
                if matches!(c, NativeRewardCommand::Claim(_)) {
                    RewardCommand::Claim(*id)
                } else {
                    RewardCommand::Clawback(*id)
                },
                Some(r.report.payload.reporter.clone()),
            )
        }
    };
    let mut projected = pool.clone();
    projected.nonces.clear();
    projected.wallets = if pool.unit.asset == s.profile.zdex_asset {
        s.wallets.iter().map(|(id, n)| (id.clone(), n.0)).collect()
    } else {
        s.reward_wallets.clone()
    };
    let result = super::reward_laws::reference(
        &rules(s, feed)?,
        AccountContext {
            parameters: p,
            actor,
            nonce: 0,
            clock: ctx.clock,
            capacity: s.profile.bounds.max_entries,
        },
        &projected,
        who.as_ref().and_then(|id| s.bonds.get(id)),
        &command,
    )?;
    let r = match result {
        Ok(r) => r,
        Err(r) => return Ok(Err(r)),
    };
    super::rewards::validate(&r, s.profile.bounds.max_entries, m)?;
    let mut post = s.clone();
    retain(&mut post, feed, r);
    post.epoch = ctx.clock.epoch;
    post.nonces.insert(actor.clone(), next);
    match super::lifecycle_schema::fit(&post)? {
        Ok(()) => Ok(Ok(post)),
        Err(r) => Ok(Err(r)),
    }
}
/// A slash cancels all still-held linked claims in the same candidate. It cannot
/// debit a paid wallet or leave the reward clawback to a later optional caller.
pub(super) fn slash_claims(
    s: &mut LifecycleState,
    reporter: &Id,
    liability: Hash,
    clock: ClockView,
    reference: bool,
    m: &mut Meter,
) -> Result<(), Failure> {
    let targets: Vec<(Hash, Hash)> = s
        .rewards
        .iter()
        .flat_map(|(feed, r)| {
            r.claims
                .iter()
                .filter(move |c| c.liability == liability && c.phase == ClaimPhase::Held)
                .map(|c| (*feed, c.id))
        })
        .collect();
    for (feed, id) in targets {
        let b = s.bonds.get(reporter).ok_or(Failure::InvalidState)?;
        let actor = b.owner.clone();
        let r = input(s, feed)?;
        let rules = rules(s, feed)?;
        let post = if reference {
            super::reward_laws::reference(
                &rules,
                AccountContext {
                    parameters: &s.profile.parameters,
                    actor: &actor,
                    nonce: 0,
                    clock,
                    capacity: s.profile.bounds.max_entries,
                },
                &r,
                Some(b),
                &RewardCommand::Clawback(id),
            )?
        } else {
            super::rewards::checked(
                &rules,
                AccountContext {
                    parameters: &s.profile.parameters,
                    actor: &actor,
                    nonce: 0,
                    clock,
                    capacity: s.profile.bounds.max_entries,
                },
                &r,
                Some(b),
                &RewardCommand::Clawback(id),
                m,
            )?
            .map(RewardPlan::into_state)
        };
        retain(s, feed, post.map_err(|_| Failure::Law(117_300))?);
    }
    Ok(())
}
