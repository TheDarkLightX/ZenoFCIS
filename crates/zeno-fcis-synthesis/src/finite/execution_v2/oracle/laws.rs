//! Independent complete decision laws for the price and sequence subrelations.
//! Reject implications, acceptance eligibility and exact post/frame are separate;
//! neither producer dispatch nor generated case-conformance law 991 is invoked.

use super::aggregation::FeedRules;
use super::prices::PriceContext;
use super::prices::{CapacityStatus, ClockView, Pending, PriceCommand, PriceState, SecurityInputs};
use super::reports::{ReportChain, ReportPayload};
use super::types::{Hash, Parameters, REASONS, Reason};
use super::wire;
use super::{Evaluation, Failure, Limits, Meter, measured, step};
use crate::finite::value::wide;

fn decision(failures: &[bool; 32], reason: Option<Reason>, m: &mut Meter) -> Result<(), Failure> {
    let first = failures.iter().position(|x| *x);
    for (i, code) in REASONS.iter().enumerate() {
        step(m)?;
        if reason == Some(*code) && first != Some(i) {
            return Err(Failure::Law(110_000 + *code as u32));
        }
    }
    step(m)?;
    if reason.is_none() && first.is_some() {
        return Err(Failure::Law(111_000));
    }
    // A Reject when no predicate holds fails its reason implication above.
    Ok(())
}
/// Check the exact sequence and timestamp cursor or its ordered refusal.
pub fn check_report_chain_laws(
    pre: &ReportChain,
    payload: &ReportPayload,
    report: Hash,
    now: u64,
    max_age: u64,
    proposed: Result<&ReportChain, Reason>,
    limits: Limits,
) -> Evaluation<()> {
    measured(limits, |m| {
        chain(pre, payload, report, now, max_age, proposed, m)
    })
}
pub(super) fn chain(
    pre: &ReportChain,
    p: &ReportPayload,
    id: Hash,
    now: u64,
    max_age: u64,
    proposed: Result<&ReportChain, Reason>,
    m: &mut Meter,
) -> Result<(), Failure> {
    let mut f = [false; 32];
    f[2] = pre.next_sequence == u64::MAX;
    f[10] = p.sequence != pre.next_sequence || p.previous != pre.previous;
    f[12] = p.observed_epoch > now;
    f[13] = pre.last_seconds.is_some_and(|x| x > p.observation_seconds);
    f[14] = p.observed_epoch <= now && now - p.observed_epoch > max_age;
    decision(&f, proposed.err(), m)?;
    if let Ok(post) = proposed {
        step(m)?;
        if post.next_sequence
            != pre
                .next_sequence
                .checked_add(1)
                .ok_or(Failure::Law(111_001))?
            || post.previous != Some(id)
            || post.last_seconds != Some(p.observation_seconds)
        {
            return Err(Failure::Law(111_002));
        }
    }
    Ok(())
}

/// Check price eligibility, ordered refusal and the exact successor independently.
pub fn check_price_laws(
    rules: &FeedRules,
    context: PriceContext<'_>,
    pre: &PriceState,
    command: &PriceCommand,
    proposed: Result<(&PriceState, CapacityStatus), Reason>,
    limits: Limits,
) -> Evaluation<()> {
    measured(limits, |m| price(rules, context, pre, command, proposed, m))
}
pub(super) fn price(
    r: &FeedRules,
    context: PriceContext<'_>,
    s: &PriceState,
    c: &PriceCommand,
    proposed: Result<(&PriceState, CapacityStatus), Reason>,
    m: &mut Meter,
) -> Result<(), Failure> {
    let PriceContext {
        parameters: p,
        nonce,
        clock,
        security,
    } = context;
    if s.history.len() > r.bounds.max_entries as usize
        || s.quarantines.len() > r.bounds.max_entries as usize
    {
        return Err(Failure::InvalidState);
    }
    wire::reserve_scans(
        s.history
            .len()
            .checked_add(s.quarantines.len())
            .ok_or(Failure::Arithmetic)?,
        m,
    )?;
    if let Ok((post, _)) = proposed
        && (post.history.len() > r.bounds.max_entries as usize
            || post.quarantines.len() > r.bounds.max_entries as usize)
    {
        return Err(Failure::Law(112_002));
    }
    let expected_reason = price_reason(r, s, nonce, c, clock, m)?;
    if proposed.as_ref().err().copied() != expected_reason {
        return Err(Failure::Law(112_100));
    }

    let Ok((post, status)) = proposed else {
        return Ok(());
    };
    wire::charge(&super::prices::value(s)?, m)?;
    wire::charge(&super::prices::value(post)?, m)?;
    let mut expected = s.clone();
    expected.nonce = s.nonce.checked_add(1).ok_or(Failure::Law(112_000))?;
    expected.epoch = clock.epoch;
    match c {
        PriceCommand::Stage(a) => {
            expected.pending = Some(Pending {
                aggregate: a.as_ref().clone(),
                published: clock.epoch,
                dispute_until: clock
                    .epoch
                    .checked_add(r.clock.dispute_open)
                    .ok_or(Failure::Arithmetic)?,
                earliest: clock
                    .epoch
                    .checked_add(r.clock.dispute_open.max(r.clock.confirmation))
                    .ok_or(Failure::Arithmetic)?,
            })
        }
        PriceCommand::Commit => {
            let a = &s.pending.as_ref().ok_or(Failure::Law(112_001))?.aggregate;
            expected.committed = Some(a.clone());
            expected.pending = None;
            expected.history.push(a.clone());
        }
        PriceCommand::SyncTime(q) => expected.watermark = s.watermark.max(q.seconds),
        PriceCommand::Quarantine(id) => expected.quarantines.push(*id),
        PriceCommand::AdvanceEpoch => {}
    }
    step(m)?;
    if *post != expected {
        return Err(Failure::Law(112_002));
    }
    security_law(p, security, status, m)?;
    Ok(())
}

fn security_law(
    p: &Parameters,
    s: &SecurityInputs,
    status: CapacityStatus,
    m: &mut Meter,
) -> Result<(), Failure> {
    let e = p.economy.as_ref().ok_or(Failure::UnboundParameter)?;
    let d = p.disputes.as_ref().ok_or(Failure::UnboundParameter)?;
    // Pair enumeration is independent of the producer's sorted-smallest-pair
    // path. Each two-bond sum is compared exactly as (carry, low).
    let sum = |i: usize, j: usize| {
        let (low, carry) = s.selected_bonds[i].0.overflowing_add(s.selected_bonds[j].0);
        (carry, low)
    };
    let mut pair = (0, 1);
    for (i, j) in [(0, 1), (0, 2), (1, 2)] {
        step(m)?;
        if sum(i, j) < sum(pair.0, pair.1) {
            pair = (i, j);
        }
    }
    let usable = d.false_price_slashable
        && s.stress_source == e.stress_source
        && s.valuation_root != [0; 32]
        && s.stress.is_some_and(|p| p.0 > 0);
    if !usable {
        return if status == CapacityStatus::Unavailable {
            Ok(())
        } else {
            Err(Failure::Law(113_000))
        };
    }
    let stress = s.stress.ok_or(Failure::Law(113_001))?.0;
    // N = (x+y)*P*a is never formed: every bound t <= N is decided against the
    // two 384-bit products by ordered subtraction.
    let x = wide::multiply_three(s.selected_bonds[pair.0].0, stress, e.alpha_numerator);
    let y = wide::multiply_three(s.selected_bonds[pair.1].0, stress, e.alpha_numerator);
    let den = wide::multiply(e.alpha_denominator, 100_000_000);
    if at_most_sum([0, den[0], den[1]], x, y) {
        return if status == CapacityStatus::Unavailable {
            Ok(())
        } else {
            Err(Failure::Law(113_002))
        };
    }
    let (CapacityStatus::Available(q) | CapacityStatus::OverCap(q)) = status else {
        return Err(Failure::Law(113_003));
    };
    step(m)?;
    let bound = |q: u128| wide::multiply_three(q, e.alpha_denominator, 100_000_000);
    if !at_most_sum(bound(q.0), x, y) {
        return Err(Failure::Law(113_004));
    }
    if let Some(next) = q.0.checked_add(1)
        && at_most_sum(bound(next), x, y)
    {
        return Err(Failure::Law(113_005));
    }
    let exposure = e
        .debt_ceiling
        .0
        .checked_add(e.liquidation_cap.0)
        .and_then(|n| n.checked_add(e.redemption_cap.0))
        .and_then(|n| n.checked_add(e.other_cap.0));
    let available = exposure.is_some_and(|n| n <= q.0);
    if available != matches!(status, CapacityStatus::Available(_)) {
        return Err(Failure::Law(113_006));
    }
    Ok(())
}
/// Whether t <= x + y for three-limb values: either t <= x, or t - x <= y.
fn at_most_sum(t: [u128; 3], x: [u128; 3], y: [u128; 3]) -> bool {
    let at_most = |a: [u128; 3], b: [u128; 3]| (a[2], a[1], a[0]) <= (b[2], b[1], b[0]);
    if at_most(t, x) {
        return true;
    }
    let mut rest = [0; 3];
    let mut borrow = false;
    for i in 0..3 {
        let (partial, first) = t[i].overflowing_sub(x[i]);
        let (difference, second) = partial.overflowing_sub(u128::from(borrow));
        rest[i] = difference;
        borrow = first || second;
    }
    at_most(rest, y)
}

pub(super) fn price_reason(
    r: &FeedRules,
    s: &PriceState,
    nonce: u64,
    c: &PriceCommand,
    clock: ClockView,
    m: &mut Meter,
) -> Result<Option<Reason>, Failure> {
    let mut f = [false; 32];
    f[1] = nonce != s.nonce;
    f[2] = s.nonce == u64::MAX;
    f[3] = clock.epoch < s.epoch;
    let target = match c {
        PriceCommand::Stage(a) => Some(a.as_ref()),
        PriceCommand::Commit => s.pending.as_ref().map(|p| &p.aggregate),
        _ => None,
    };
    if matches!(c, PriceCommand::Commit) && s.pending.is_none() {
        f[6] = true;
    }
    if let Some(a) = target {
        f[9] = a.feed() != r.feed || a.profile() != r.profile || a.query() != r.query;
        let n = a.numbers();
        f[12] = n.oldest_epoch > clock.epoch;
        f[14] = n.oldest_epoch <= clock.epoch
            && (clock.epoch - n.oldest_epoch > r.clock.max_age
                || (s.watermark >= n.median_seconds
                    && s.watermark - n.median_seconds > r.clock.max_time_lag));
        f[19] = n.deviation_bps > r.max_deviation_bps;
        for quarantine in &s.quarantines {
            wire::visit(m)?;
            f[22] |= a.id() == *quarantine
                || a.reports().iter().any(|id| id == quarantine)
                || a.admissions().iter().any(|id| id == quarantine);
        }
        if let Some(old) = &s.committed {
            f[23] = a.round() <= old.round()
                || n.oldest_epoch < old.numbers().oldest_epoch
                || n.median_seconds < old.numbers().median_seconds;
        }
        if matches!(c, PriceCommand::Stage(_)) {
            f[23] |= s
                .pending
                .as_ref()
                .is_some_and(|old| a.round() <= old.aggregate.round());
        }
    }
    if let PriceCommand::Commit = c
        && let Some(p) = &s.pending
    {
        f[21] = clock.epoch < p.earliest;
    }
    if let PriceCommand::SyncTime(q) = c {
        f[9] = q.feed != r.feed || q.profile != r.profile;
        f[12] = q.oldest_epoch > clock.epoch;
        f[14] = q.oldest_epoch <= clock.epoch && clock.epoch - q.oldest_epoch > r.clock.max_age;
    }
    if let PriceCommand::Quarantine(id) = c {
        f[4] = !s
            .committed
            .iter()
            .chain(s.pending.iter().map(|x| &x.aggregate))
            .chain(s.history.iter())
            .any(|a| a.id() == *id || a.reports().contains(id) || a.admissions().contains(id));
        f[5] = s.quarantines.contains(id);
    }
    f[30] = matches!(c, PriceCommand::Commit) && s.history.len() >= r.bounds.max_entries as usize
        || matches!(c,PriceCommand::Quarantine(id) if !s.quarantines.contains(id))
            && s.quarantines.len() >= r.bounds.max_entries as usize;
    for (i, failed) in f.into_iter().enumerate() {
        step(m)?;
        if failed {
            return Ok(Some(REASONS[i]));
        }
    }
    Ok(None)
}
