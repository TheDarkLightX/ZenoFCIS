//! Buffered evidence recording. No vault-health or monetary eligibility input
//! exists here; security failure changes read status, never market evidence.

use super::aggregation::{Aggregate, FeedRules, TimeQuorum};
use super::arithmetic;
use super::types::{ClockParameters, Hash, Parameters, Reason, StressE8, UsdExposureE18, ZdexE18};
use super::wire;
use super::{Evaluation, Failure, Limits, Meter, measured, step};
use alloc::{vec, vec::Vec};
use zeno_fcis_value::Value;

/// Immutable policy, clock and allocation premises for a price command.
#[derive(Clone, Copy, Debug)]
pub struct PriceContext<'a> {
    /// Fully bound oracle policy.
    pub parameters: &'a Parameters,
    /// Expected price-state nonce.
    pub nonce: u64,
    /// Authenticated clock premise supplied by the caller.
    pub clock: ClockView,
    /// Already allocated, collectible security inputs.
    pub security: &'a SecurityInputs,
}
/// Staged aggregate and its publication timing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pending {
    /// Aggregate selected for this operation.
    pub aggregate: Aggregate,
    /// Epoch at which the aggregate was staged.
    pub published: u64,
    /// Exclusive end of the publication dispute window.
    pub dispute_until: u64,
    /// Earliest epoch at which the pending aggregate may commit.
    pub earliest: u64,
}
/// Owned committed, buffered and quarantined price evidence.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct PriceState {
    /// Last aggregate that completed the publication buffer.
    pub committed: Option<Aggregate>,
    /// Aggregate awaiting its full publication buffer.
    pub pending: Option<Pending>,
    /// Retained committed aggregate history.
    pub history: Vec<Aggregate>,
    /// Report identities barred from monetary eligibility.
    pub quarantines: Vec<Hash>,
    /// Monotone authenticated observation time in seconds.
    pub watermark: u64,
    /// Logical epoch of this state or evidence.
    pub epoch: u64,
    /// Replay counter before the command.
    pub nonce: u64,
}
/// Requested buffered-price transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PriceCommand {
    /// Buffer a complete admitted price aggregate.
    Stage(alloc::boxed::Box<Aggregate>),
    /// Commit the buffered aggregate after its full delay.
    Commit,
    /// Advance the watermark from a complete time-only quorum.
    SyncTime(TimeQuorum),
    /// Restrict a report’s monetary eligibility.
    Quarantine(Hash),
    /// Advance logical time without changing market evidence.
    AdvanceEpoch,
}
/// Authenticated epoch acquisition is outside these nonpublishing relations.
/// This context names the exact backend/root premise; it is not an auth verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockView {
    /// Identity of the authenticated clock backend.
    pub backend: Hash,
    /// Authenticated finality evidence root.
    pub finality_root: Hash,
    /// Logical epoch of this state or evidence.
    pub epoch: u64,
}
/// Derived economic exposure capacity status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapacityStatus {
    /// Bound exposure fits within this collectible USD E18 capacity.
    Available(UsdExposureE18),
    /// Bound exposure exceeds this collectible USD E18 capacity.
    OverCap(UsdExposureE18),
    /// Required collectible security or valuation evidence is unavailable.
    Unavailable,
}
/// Already allocated bond and stress premises for capacity calculation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityInputs {
    /// Per selected reporter, already net of other-feed reservations, exits and
    /// uncollectible penalties. This pure relation assumes that allocation port;
    /// it does not authenticate it or derive it from total registered stake.
    pub selected_bonds: [ZdexE18; 3],
    /// Bound stress valuation in E8 units, if available.
    pub stress: Option<StressE8>,
    /// Identity of the bound stress valuation source.
    pub stress_source: Hash,
    /// Authenticated stress valuation evidence root.
    pub valuation_root: Hash,
}
/// Owned price successor and its independently checked capacity status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PricePlan {
    state: PriceState,
    status: CapacityStatus,
}
impl PricePlan {
    /// Borrow the immutable checked successor.
    pub fn state(&self) -> &PriceState {
        &self.state
    }
    /// Return the derived exposure capacity status.
    pub fn capacity_status(&self) -> CapacityStatus {
        self.status
    }
    /// Consume the plan and return its owned successor.
    pub fn into_state(self) -> PriceState {
        self.state
    }
}

/// A derived committed-price observation, never an action authorization or a
/// verifier receipt. Exact consumer/action/root binding belongs to composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceView {
    aggregate: Aggregate,
    watermark: u64,
    epoch: u64,
    capacity: UsdExposureE18,
}
impl PriceView {
    /// Borrow the selected committed aggregate.
    pub fn aggregate(&self) -> &Aggregate {
        &self.aggregate
    }
    /// Return the authenticated observation-time watermark in seconds.
    pub fn watermark(&self) -> u64 {
        self.watermark
    }
    /// Return the logical epoch.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    /// Return collectible security capacity in USD E18.
    pub fn capacity(&self) -> UsdExposureE18 {
        self.capacity
    }
}
/// Derive monetary read eligibility from committed evidence and explicit security premises.
pub fn committed_view(
    rules: &FeedRules,
    parameters: &Parameters,
    state: &PriceState,
    clock: ClockView,
    security: &SecurityInputs,
    limits: Limits,
) -> Evaluation<Result<PriceView, Reason>> {
    measured(limits, |m| {
        checked_view(rules, parameters, state, clock, security, m)
    })
}
pub(super) fn checked_view(
    rules: &FeedRules,
    parameters: &Parameters,
    state: &PriceState,
    clock: ClockView,
    security: &SecurityInputs,
    m: &mut Meter,
) -> Result<Result<PriceView, Reason>, Failure> {
    rules.validate()?;
    parameters.validate()?;
    if parameters.clock.as_ref() != Some(&rules.clock) {
        return Err(Failure::InvalidProfile);
    }
    validate(rules, state, m)?;
    if clock.backend != rules.clock.backend || clock.finality_root == [0; 32] {
        return Err(Failure::InvalidEvidence);
    }
    if clock.epoch < state.epoch {
        return Ok(Err(Reason::BadClockOrSync));
    }
    let Some(aggregate) = &state.committed else {
        return Ok(Err(Reason::WrongPhase));
    };
    let n = aggregate.numbers();
    if state.watermark < n.median_seconds {
        return Ok(Err(Reason::BadClockOrSync));
    }
    if n.oldest_epoch > clock.epoch {
        return Ok(Err(Reason::FutureObservation));
    }
    if stale(&rules.clock, aggregate, clock.epoch, state.watermark) {
        return Ok(Err(Reason::StaleObservation));
    }
    if n.deviation_bps > rules.max_deviation_bps {
        return Ok(Err(Reason::ExcessUncertainty));
    }
    if quarantined(state, aggregate, m)? {
        return Ok(Err(Reason::DisputedOrFrozen));
    }
    let capacity = match status(parameters, security, m)? {
        CapacityStatus::Available(capacity) => capacity,
        _ => return Ok(Err(Reason::SecurityBudgetExceeded)),
    };
    wire::charge(&aggregate.value()?, m)?;
    Ok(Ok(PriceView {
        aggregate: aggregate.clone(),
        watermark: state.watermark,
        epoch: clock.epoch,
        capacity,
    }))
}

/// Evaluate a buffered-price transition and its independent laws.
pub fn price_relation(
    rules: &FeedRules,
    context: PriceContext<'_>,
    state: &PriceState,
    command: &PriceCommand,
    limits: Limits,
) -> Evaluation<Result<PricePlan, Reason>> {
    measured(limits, |m| checked(rules, context, state, command, m))
}
pub(super) fn checked(
    rules: &FeedRules,
    context: PriceContext<'_>,
    state: &PriceState,
    command: &PriceCommand,
    m: &mut Meter,
) -> Result<Result<PricePlan, Reason>, Failure> {
    let result = run(rules, context, state, command, m)?;
    super::laws::price(
        rules,
        context,
        state,
        command,
        result
            .as_ref()
            .map(|p| (p.state(), p.capacity_status()))
            .map_err(|r| *r),
        m,
    )?;
    Ok(result)
}

fn run(
    r: &FeedRules,
    context: PriceContext<'_>,
    s: &PriceState,
    c: &PriceCommand,
    m: &mut Meter,
) -> Result<Result<PricePlan, Reason>, Failure> {
    let PriceContext {
        parameters: p,
        nonce,
        clock,
        security,
    } = context;
    r.validate()?;
    p.validate()?;
    if p.clock.as_ref() != Some(&r.clock) {
        return Err(Failure::InvalidProfile);
    }
    validate(r, s, m)?;
    let before = value(s)?;
    let size = wire::charge(&before, m)?;
    if size.checked_add(48).ok_or(Failure::Arithmetic)? > r.frame_bytes {
        return Err(Failure::InvalidState);
    }
    step(m)?;
    if nonce != s.nonce {
        return Ok(Err(Reason::Replay));
    }
    let Some(next) = s.nonce.checked_add(1) else {
        return Ok(Err(Reason::CounterExhausted));
    };
    if clock.backend != r.clock.backend || clock.finality_root == [0; 32] {
        return Err(Failure::InvalidEvidence);
    }
    if clock.epoch < s.epoch {
        return Ok(Err(Reason::BadClockOrSync));
    }
    // All modifications belong to this invocation. Charge the complete owned
    // predecessor before cloning; errors discard it without a visible candidate.
    wire::charge(&before, m)?;
    let mut out = s.clone();
    match c {
        PriceCommand::AdvanceEpoch => {}
        PriceCommand::Stage(a) => {
            if !binding(r, a) {
                return Ok(Err(Reason::BindingMismatch));
            }
            if a.numbers.oldest_epoch > clock.epoch {
                return Ok(Err(Reason::FutureObservation));
            }
            if stale(&r.clock, a, clock.epoch, s.watermark) {
                return Ok(Err(Reason::StaleObservation));
            }
            if a.numbers.deviation_bps > r.max_deviation_bps {
                return Ok(Err(Reason::ExcessUncertainty));
            }
            if quarantined(s, a, m)? {
                return Ok(Err(Reason::DisputedOrFrozen));
            }
            if !monotone(s, a) {
                return Ok(Err(Reason::NonMonotoneCommit));
            }
            out.pending = Some(Pending {
                aggregate: a.as_ref().clone(),
                published: clock.epoch,
                dispute_until: clock
                    .epoch
                    .checked_add(r.clock.dispute_open)
                    .ok_or(Failure::Arithmetic)?,
                earliest: clock
                    .epoch
                    .checked_add(r.clock.confirmation.max(r.clock.dispute_open))
                    .ok_or(Failure::Arithmetic)?,
            });
        }
        PriceCommand::Commit => {
            let Some(pending) = &s.pending else {
                return Ok(Err(Reason::WrongPhase));
            };
            let a = &pending.aggregate;
            if a.numbers.oldest_epoch > clock.epoch {
                return Ok(Err(Reason::FutureObservation));
            }
            if stale(&r.clock, a, clock.epoch, s.watermark) {
                return Ok(Err(Reason::StaleObservation));
            }
            if a.numbers.deviation_bps > r.max_deviation_bps {
                return Ok(Err(Reason::ExcessUncertainty));
            }
            if clock.epoch < pending.earliest {
                return Ok(Err(Reason::BufferNotMature));
            }
            if quarantined(s, a, m)? {
                return Ok(Err(Reason::DisputedOrFrozen));
            }
            if s.committed.as_ref().is_some_and(|old| {
                a.round <= old.round
                    || a.numbers.oldest_epoch < old.numbers.oldest_epoch
                    || a.numbers.median_seconds < old.numbers.median_seconds
            }) {
                return Ok(Err(Reason::NonMonotoneCommit));
            }
            out.committed = Some(a.clone());
            out.history.push(a.clone());
            out.pending = None;
        }
        PriceCommand::SyncTime(q) => {
            if q.feed != r.feed || q.profile != r.profile {
                return Ok(Err(Reason::BindingMismatch));
            }
            if q.oldest_epoch > clock.epoch {
                return Ok(Err(Reason::FutureObservation));
            }
            if clock.epoch - q.oldest_epoch > r.clock.max_age {
                return Ok(Err(Reason::StaleObservation));
            }
            out.watermark = out.watermark.max(q.seconds);
        }
        PriceCommand::Quarantine(target) => {
            let exists = s
                .committed
                .iter()
                .chain(s.pending.iter().map(|x| &x.aggregate))
                .chain(s.history.iter())
                .any(|a| {
                    a.id == *target || a.reports.contains(target) || a.admissions.contains(target)
                });
            if !exists {
                return Ok(Err(Reason::UnknownReference));
            }
            if out.quarantines.contains(target) {
                return Ok(Err(Reason::AlreadyExists));
            }
            out.quarantines.push(*target);
        }
    }
    out.epoch = clock.epoch;
    out.nonce = next;
    if out.history.len() > r.bounds.max_entries as usize
        || out.quarantines.len() > r.bounds.max_entries as usize
    {
        return Ok(Err(Reason::Full));
    }
    let status = status(p, security, m)?;
    let post = value(&out)?;
    wire::charge(&post, m)?;
    if wire::bytes(&post)?
        .len()
        .checked_add(48)
        .ok_or(Failure::Arithmetic)?
        > usize::try_from(r.frame_bytes).map_err(|_| Failure::Arithmetic)?
    {
        return Err(Failure::Canonical);
    }
    validate(r, &out, m)?;
    Ok(Ok(PricePlan { state: out, status }))
}
/// Return the derived exposure capacity status.
pub fn capacity_status(
    p: &Parameters,
    s: &SecurityInputs,
    meter_limits: Limits,
) -> Evaluation<CapacityStatus> {
    measured(meter_limits, |m| status(p, s, m))
}
pub(super) fn status(
    p: &Parameters,
    s: &SecurityInputs,
    m: &mut Meter,
) -> Result<CapacityStatus, Failure> {
    p.validate()?;
    let e = p.economy.as_ref().ok_or(Failure::UnboundParameter)?;
    let d = p.disputes.as_ref().ok_or(Failure::UnboundParameter)?;
    if !d.false_price_slashable || s.stress_source != e.stress_source || s.valuation_root == [0; 32]
    {
        return Ok(CapacityStatus::Unavailable);
    }
    let Some(stress) = s.stress else {
        return Ok(CapacityStatus::Unavailable);
    };
    if stress.0 == 0 {
        return Ok(CapacityStatus::Unavailable);
    }
    let mut bonds = s.selected_bonds.map(|x| x.0);
    for (i, j) in [(0, 1), (1, 2), (0, 1)] {
        step(m)?;
        if bonds[i] > bonds[j] {
            bonds.swap(i, j);
        }
    }
    // The two smallest bonds are summed inside the wide numerator, never in U128.
    let cap = match arithmetic::pair_capacity(
        [ZdexE18(bonds[0]), ZdexE18(bonds[1])],
        stress,
        e.alpha_numerator,
        e.alpha_denominator,
        m,
    ) {
        Ok(c) => c,
        Err(Failure::Arithmetic) => return Ok(CapacityStatus::Unavailable),
        Err(other) => return Err(other),
    };
    step(m)?;
    let exposure = e
        .debt_ceiling
        .0
        .checked_add(e.liquidation_cap.0)
        .and_then(|x| x.checked_add(e.redemption_cap.0))
        .and_then(|x| x.checked_add(e.other_cap.0));
    Ok(if exposure.is_some_and(|x| x <= cap.0) {
        CapacityStatus::Available(cap)
    } else {
        CapacityStatus::OverCap(cap)
    })
}
fn binding(r: &FeedRules, a: &Aggregate) -> bool {
    a.feed == r.feed && a.profile == r.profile && a.query == r.query
}
pub(super) fn stale(c: &ClockParameters, a: &Aggregate, epoch: u64, watermark: u64) -> bool {
    epoch - a.numbers.oldest_epoch > c.max_age
        || (watermark >= a.numbers.median_seconds
            && watermark - a.numbers.median_seconds > c.max_time_lag)
}
fn monotone(s: &PriceState, a: &Aggregate) -> bool {
    s.committed.as_ref().is_none_or(|old| {
        a.round > old.round
            && a.numbers.oldest_epoch >= old.numbers.oldest_epoch
            && a.numbers.median_seconds >= old.numbers.median_seconds
    }) && s
        .pending
        .as_ref()
        .is_none_or(|old| a.round > old.aggregate.round)
}
pub(super) fn quarantined(s: &PriceState, a: &Aggregate, m: &mut Meter) -> Result<bool, Failure> {
    let mut affected = false;
    for id in &s.quarantines {
        wire::visit(m)?;
        affected |= *id == a.id || a.reports.contains(id) || a.admissions.contains(id);
    }
    Ok(affected)
}
fn validate(r: &FeedRules, s: &PriceState, m: &mut Meter) -> Result<(), Failure> {
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
    for (i, a) in s.history.iter().enumerate() {
        wire::visit(m)?;
        if !binding(r, a)
            || s.history[..i]
                .iter()
                .any(|old| old.id == a.id || old.round >= a.round)
        {
            return Err(Failure::InvalidState);
        }
    }
    if s.committed.as_ref() != s.history.last() {
        return Err(Failure::InvalidState);
    }
    if let Some(pending) = &s.pending
        && (!binding(r, &pending.aggregate)
            || pending.dispute_until
                != pending
                    .published
                    .checked_add(r.clock.dispute_open)
                    .ok_or(Failure::Arithmetic)?
            || pending.earliest
                != pending
                    .published
                    .checked_add(r.clock.confirmation.max(r.clock.dispute_open))
                    .ok_or(Failure::Arithmetic)?
            || pending.published > s.epoch
            || pending.aggregate.numbers().oldest_epoch > pending.published
            || s.committed.as_ref().is_some_and(|old| {
                pending.aggregate.round() <= old.round()
                    || pending.aggregate.numbers().oldest_epoch < old.numbers().oldest_epoch
                    || pending.aggregate.numbers().median_seconds < old.numbers().median_seconds
            }))
    {
        return Err(Failure::InvalidState);
    }
    Ok(())
}
pub(super) fn value(s: &PriceState) -> Result<Value, Failure> {
    let maybe = |a: &Option<Aggregate>| {
        a.as_ref()
            .map(Aggregate::value)
            .transpose()
            .map(|v| Value::sum(1, if v.is_some() { 1 } else { 0 }, v))
    };
    let pending = if let Some(p) = &s.pending {
        Value::sum(
            2,
            1,
            Some(wire::record(vec![
                p.aggregate.value()?,
                wire::integers(&[
                    u128::from(p.published),
                    u128::from(p.dispute_until),
                    u128::from(p.earliest),
                ])?,
            ])?),
        )
    } else {
        Value::sum(2, 0, None)
    };
    let history = Value::vector(
        s.history
            .iter()
            .map(Aggregate::value)
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(|_| Failure::Canonical)?;
    let quarantines = Value::vector(
        s.quarantines
            .iter()
            .map(|h| wire::blob(h))
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(|_| Failure::Canonical)?;
    wire::record(vec![
        maybe(&s.committed)?,
        pending,
        history,
        quarantines,
        wire::integers(&[
            u128::from(s.watermark),
            u128::from(s.epoch),
            u128::from(s.nonce),
        ])?,
    ])
}
