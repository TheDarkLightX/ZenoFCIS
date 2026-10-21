//! Independent complete reward-account outcomes, including all ordered reasons
//! and exact recipient/dust/claim/frame equations. No producer dispatch.
use super::bonds::AccountContext;
use super::bonds::{BondState, DisputePhase};
use super::rewards::{ClaimPhase, RewardClaim, RewardCommand, RewardRules, RewardState};
use super::types::{Hash, Id, REASONS, Reason};
use super::wire;
use super::{Evaluation, Failure, Limits, Meter, measured};
use crate::finite::value::wide;
use alloc::vec;

/// Check the exact reward-account outcome and ordered refusal independently.
pub fn check_reward_laws(
    r: &RewardRules,
    context: AccountContext<'_>,
    s: &RewardState,
    b: &BondState,
    c: &RewardCommand,
    proposed: Result<&RewardState, Reason>,
    limits: Limits,
) -> Evaluation<()> {
    measured(limits, |m| check(r, context, s, Some(b), c, proposed, m))
}
pub(super) fn check(
    r: &RewardRules,
    context: AccountContext<'_>,
    s: &RewardState,
    b: Option<&BondState>,
    c: &RewardCommand,
    proposed: Result<&RewardState, Reason>,
    m: &mut Meter,
) -> Result<(), Failure> {
    let capacity = context.capacity;
    super::rewards::validate(s, capacity, m)?;
    if let Some(b) = b {
        super::bonds::validate(b, capacity, m)?;
    }
    wire::reserve_scans(
        s.wallets
            .len()
            .checked_add(s.claims.len())
            .and_then(|n| n.checked_add(s.nonces.len()))
            .and_then(|n| n.checked_add(8))
            .ok_or(Failure::Arithmetic)?,
        m,
    )?;
    if proposed.as_ref().is_ok_and(|x| {
        [x.claims.len(), x.wallets.len(), x.nonces.len()]
            .iter()
            .any(|n| *n > capacity as usize)
    }) {
        return Err(Failure::Law(114_001));
    }
    let want = reference(r, context, s, b, c)?;
    match (&want, proposed) {
        (Ok(want), Ok(actual)) if want == actual => Ok(()),
        (Err(want), Err(actual)) if *want == actual => Ok(()),
        (Ok(_), Err(reason)) => Err(Failure::Law(114_100 + reason as u32)),
        (Err(_), Ok(_)) => Err(Failure::Law(114_200)),
        _ => Err(Failure::Law(114_201)),
    }
}
fn add(a: u128, b: u128) -> Result<u128, Failure> {
    a.checked_add(b).ok_or(Failure::Arithmetic)
}
fn claim_id(r: &RewardRules, event: Hash, report: Hash, owner: &Id) -> Result<Hash, Failure> {
    wire::hash(
        b"zeno.oracle.reward/1\0",
        &wire::record(vec![
            wire::blob(&r.feed)?,
            wire::blob(&r.profile)?,
            wire::blob(&event)?,
            wire::blob(&report)?,
            wire::blob(owner.bytes())?,
            wire::blob(r.unit.asset.bytes())?,
            zeno_fcis_value::Value::unsigned(u128::from(r.unit.scale)),
        ])?,
    )
}
pub(super) fn reference(
    r: &RewardRules,
    context: AccountContext<'_>,
    s: &RewardState,
    b: Option<&BondState>,
    c: &RewardCommand,
) -> Result<Result<RewardState, Reason>, Failure> {
    let AccountContext {
        parameters: p,
        actor,
        nonce,
        clock,
        capacity,
    } = context;
    p.validate()?;
    r.validate()?;
    let e = p.economy.as_ref().ok_or(Failure::UnboundParameter)?;
    let dispute_rule = p.disputes.as_ref().ok_or(Failure::UnboundParameter)?;
    if r.unit != e.reward_unit
        || r.per_report != e.reward_per_report
        || s.unit != r.unit
        || s.feed != r.feed
        || s.profile != r.profile
    {
        return Err(Failure::InvalidProfile);
    }
    if clock.backend != p.clock.as_ref().ok_or(Failure::UnboundParameter)?.backend
        || clock.finality_root == [0; 32]
    {
        return Err(Failure::InvalidEvidence);
    }
    let mut f = [false; 32];
    f[1] = nonce != s.nonces.get(actor).copied().unwrap_or(0);
    f[2] = nonce == u64::MAX;
    f[3] = clock.epoch < s.epoch;
    let mut id = None;
    let mut selected_liability = None;
    match c {
        RewardCommand::Fund(amount) => {
            f[11] = *amount == 0;
            f[17] = s.wallets.get(actor).copied().unwrap_or(0) < *amount;
        }
        RewardCommand::Accrue { aggregate, report } => {
            let b = b.ok_or(Failure::InvalidState)?;
            let selected = aggregate.reports().iter().position(|x| x == report);
            selected_liability = b
                .liabilities
                .iter()
                .find(|l| l.report == *report && l.feed == r.feed);
            f[4] = selected.is_none() || selected_liability.is_none();
            f[9] = aggregate.feed() != r.feed
                || aggregate.profile() != r.profile
                || selected.is_some_and(|i| aggregate.reporters()[i] != b.reporter);
            let hash = claim_id(r, aggregate.id(), *report, &b.owner)?;
            id = Some(hash);
            f[26] = s.remaining < r.per_report;
            f[27] = s.claims.iter().any(|x| x.id == hash);
            f[30] = s.claims.len() >= capacity as usize;
        }
        RewardCommand::Claim(hash) | RewardCommand::Clawback(hash) => {
            let b = b.ok_or(Failure::InvalidState)?;
            let claim = s.claims.iter().find(|x| x.id == *hash);
            let li = claim.and_then(|x| b.liabilities.iter().find(|l| l.id == x.liability));
            f[0] = matches!(c, RewardCommand::Claim(_))
                && claim.is_some_and(|x| &x.beneficiary != actor);
            f[4] = claim.is_none() || li.is_none();
            if let (Some(claim), Some(l)) = (claim, li) {
                f[9] = b.owner != claim.beneficiary || l.report != claim.report;
                if matches!(c, RewardCommand::Claim(_)) {
                    f[25] = !l.cleared
                        || b.disputes
                            .iter()
                            .any(|x| x.liability == l.id && x.phase == DisputePhase::Open);
                } else {
                    f[24] = !b.disputes.iter().any(|x| {
                        x.liability == l.id
                            && (x.phase == DisputePhase::ResolvedUpheld
                                || x.phase == DisputePhase::Open
                                    && x.slash == dispute_rule.slash
                                    && x.slash_verdict.as_ref().is_some_and(|v| {
                                        v.upheld && x.violation == Some(v.violation)
                                    }))
                    });
                }
                f[27] = claim.phase != ClaimPhase::Held;
            }
        }
    }
    if let Some(i) = f.iter().position(|x| *x) {
        return Ok(Err(REASONS[i]));
    }
    let mut out = s.clone();
    match c {
        RewardCommand::Fund(amount) => {
            let allocation = |share| {
                wide::mul_div_floor(*amount, share, r.denominator).ok_or(Failure::Arithmetic)
            };
            let reporter = allocation(r.reporter_share)?;
            let treasury = allocation(r.treasury_share)?;
            let burn = allocation(r.burn_share)?;
            let residual = amount
                .checked_sub(add(add(reporter, treasury)?, burn)?)
                .ok_or(Failure::Arithmetic)?;
            out.wallets.insert(
                actor.clone(),
                s.wallets.get(actor).copied().unwrap_or(0) - amount,
            );
            if treasury > 0 {
                out.wallets.insert(
                    r.treasury.clone(),
                    add(out.wallets.get(&r.treasury).copied().unwrap_or(0), treasury)?,
                );
            }
            out.funded = add(s.funded, *amount)?;
            out.remaining = add(s.remaining, reporter)?;
            out.treasury_paid = add(s.treasury_paid, treasury)?;
            out.burned = add(s.burned, burn)?;
            out.residual = add(s.residual, residual)?;
        }
        RewardCommand::Accrue { aggregate, report } => {
            let b = b.ok_or(Failure::InvalidState)?;
            out.remaining = s.remaining - r.per_report;
            out.claims.push(RewardClaim {
                id: id.ok_or(Failure::InvalidState)?,
                aggregate: aggregate.id(),
                report: *report,
                liability: selected_liability.ok_or(Failure::InvalidState)?.id,
                beneficiary: b.owner.clone(),
                amount: r.per_report,
                phase: ClaimPhase::Held,
            });
        }
        RewardCommand::Claim(hash) => {
            let claim = out
                .claims
                .iter_mut()
                .find(|x| x.id == *hash)
                .ok_or(Failure::InvalidState)?;
            claim.phase = ClaimPhase::Paid;
            out.wallets.insert(
                claim.beneficiary.clone(),
                add(
                    s.wallets.get(&claim.beneficiary).copied().unwrap_or(0),
                    claim.amount,
                )?,
            );
            out.paid = add(s.paid, claim.amount)?;
        }
        RewardCommand::Clawback(hash) => {
            let claim = out
                .claims
                .iter_mut()
                .find(|x| x.id == *hash)
                .ok_or(Failure::InvalidState)?;
            claim.phase = ClaimPhase::ClawedBack;
            out.remaining = add(s.remaining, claim.amount)?;
        }
    }
    out.epoch = clock.epoch;
    out.nonces.insert(
        actor.clone(),
        nonce.checked_add(1).ok_or(Failure::Arithmetic)?,
    );
    if [out.claims.len(), out.wallets.len(), out.nonces.len()]
        .iter()
        .any(|n| *n > capacity as usize)
    {
        return Ok(Err(Reason::Full));
    }
    Ok(Ok(out))
}
