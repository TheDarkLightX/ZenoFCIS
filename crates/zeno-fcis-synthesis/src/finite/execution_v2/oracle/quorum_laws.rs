//! Independent quorum laws: class-count sets and sorted triple enumeration,
//! exact median/ceil reduction, and time-only reduction without price arithmetic.
use super::aggregation::{Aggregate, FeedRules, TimeQuorum};
use super::arithmetic::AggregateNumbers;
use super::types::{Diversity, Observation, PriceE8, Reason, Source};
use super::wire;
use super::{Failure, Meter, Resource};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
fn diverse(os: &[&Observation; 3], sources: &[Source], d: &Diversity) -> bool {
    let mut ids = BTreeSet::new();
    for o in os {
        if !ids.insert(&o.source) {
            return false;
        }
    }
    let Some(a) = sources.iter().find(|x| x.id == os[0].source) else {
        return false;
    };
    let Some(b) = sources.iter().find(|x| x.id == os[1].source) else {
        return false;
    };
    let Some(c) = sources.iter().find(|x| x.id == os[2].source) else {
        return false;
    };
    let dims = |s: &Source| {
        [
            s.operator.clone(),
            s.venue.clone(),
            s.family.clone(),
            s.transport.clone(),
            s.jurisdiction.clone(),
        ]
    };
    let aa = dims(a);
    let bb = dims(b);
    let cc = dims(c);
    (0..5).all(|i| {
        let mut counts = BTreeMap::new();
        for x in [&aa[i], &bb[i], &cc[i]] {
            *counts.entry(x).or_insert(0u8) += 1;
        }
        counts.len() >= usize::from(d.minimum[i])
            && counts.values().all(|n| *n <= d.max_same_class[i])
    })
}

pub(super) fn selected(
    r: &FeedRules,
    round: u64,
    cutoff: u64,
    now: u64,
    observations: &[Observation],
    sources: &[Source],
    m: &mut Meter,
) -> Result<Result<[Observation; 3], Reason>, Failure> {
    r.validate()?;
    super::wire::reserve_scans(
        observations
            .len()
            .checked_add(sources.len())
            .ok_or(Failure::Arithmetic)?,
        m,
    )?;
    if now < cutoff {
        return Ok(Err(Reason::WrongPhase));
    }
    if observations.len() > r.bounds.max_entries as usize
        || sources.len() > r.bounds.max_entries as usize
    {
        return Err(Failure::InvalidState);
    }
    let reporters: BTreeSet<_> = observations.iter().map(|o| &o.reporter).collect();
    let reports: BTreeSet<_> = observations.iter().map(|o| o.id).collect();
    let source_ids: BTreeSet<_> = sources.iter().map(|s| &s.id).collect();
    if reporters.len() != observations.len()
        || reports.len() != observations.len()
        || source_ids.len() != sources.len()
        || observations
            .iter()
            .any(|o| o.id == [0; 32] || o.admission == [0; 32])
    {
        return Err(Failure::InvalidState);
    }
    if observations.iter().any(|o| {
        o.feed != r.feed || o.profile != r.profile || o.query != r.query || o.round != round
    }) {
        return Ok(Err(Reason::BindingMismatch));
    }
    if observations.iter().any(|o| o.epoch > now) {
        return Ok(Err(Reason::FutureObservation));
    }
    if observations.iter().any(|o| now - o.epoch > r.clock.max_age) {
        return Ok(Err(Reason::StaleObservation));
    }
    if observations.iter().any(|o| !source_ids.contains(&o.source)) {
        return Ok(Err(Reason::WrongSourceOrDiversity));
    }
    let mut os: Vec<&Observation> = observations.iter().collect();
    os.sort_by_key(|o| (&o.reporter, &o.source));
    for i in 0..os.len() {
        for j in i + 1..os.len() {
            for k in j + 1..os.len() {
                wire::visit(m)?;
                m.charge(
                    Resource::Read,
                    u64::try_from(sources.len())
                        .map_err(|_| Failure::Arithmetic)?
                        .checked_mul(3)
                        .ok_or(Failure::Arithmetic)?,
                )?;
                if diverse(&[os[i], os[j], os[k]], sources, &r.diversity) {
                    return Ok(Ok([os[i].clone(), os[j].clone(), os[k].clone()]));
                }
            }
        }
    }
    Ok(Err(Reason::NoCompleteQuorum))
}
pub(super) fn reference(
    r: &FeedRules,
    round: u64,
    cutoff: u64,
    now: u64,
    observations: &[Observation],
    sources: &[Source],
    m: &mut Meter,
) -> Result<Result<Aggregate, Reason>, Failure> {
    let three = match selected(r, round, cutoff, now, observations, sources, m)? {
        Ok(t) => t,
        Err(r) => return Ok(Err(r)),
    };
    let mut prices = three.each_ref().map(|o| o.value.raw());
    prices.sort();
    let median = prices[1];
    let confidence = prices[2].abs_diff(median).max(median - prices[0]);
    let deviation = crate::finite::value::wide::mul_div_ceil(confidence, 10_000, median)
        .ok_or(Failure::Arithmetic)?;
    if deviation > r.max_deviation_bps {
        return Ok(Err(Reason::ExcessUncertainty));
    }
    let mut seconds = three.each_ref().map(|o| o.seconds);
    seconds.sort();
    let mut a = Aggregate {
        id: [0; 32],
        feed: r.feed,
        profile: r.profile,
        query: r.query,
        round,
        reports: three.each_ref().map(|o| o.id),
        admissions: three.each_ref().map(|o| o.admission),
        reporters: three.each_ref().map(|o| o.reporter.clone()),
        sources: three.each_ref().map(|o| o.source.clone()),
        numbers: AggregateNumbers {
            median: PriceE8::new(median)?,
            confidence,
            deviation_bps: deviation,
            oldest_epoch: three
                .iter()
                .map(|o| o.epoch)
                .min()
                .ok_or(Failure::InvalidState)?,
            median_seconds: seconds[1],
        },
    };
    if wire::charge(&a.value()?, m)?
        .checked_add(48)
        .ok_or(Failure::Arithmetic)?
        > r.frame_bytes
    {
        return Err(Failure::Canonical);
    }
    a.id = wire::hash(b"zeno.oracle.aggregate/1\0", &a.value()?)?;
    Ok(Ok(a))
}
pub(super) fn time_reference(
    r: &FeedRules,
    round: u64,
    cutoff: u64,
    now: u64,
    observations: &[Observation],
    sources: &[Source],
    m: &mut Meter,
) -> Result<Result<TimeQuorum, Reason>, Failure> {
    let three = match selected(r, round, cutoff, now, observations, sources, m)? {
        Ok(t) => t,
        Err(r) => return Ok(Err(r)),
    };
    let mut seconds = three.each_ref().map(|o| o.seconds);
    seconds.sort();
    Ok(Ok(TimeQuorum {
        feed: r.feed,
        profile: r.profile,
        round,
        reports: three.each_ref().map(|o| o.id),
        oldest_epoch: three
            .iter()
            .map(|o| o.epoch)
            .min()
            .ok_or(Failure::InvalidState)?,
        seconds: seconds[1],
    }))
}
pub(super) fn check<T: Eq>(
    expected: Result<T, Reason>,
    actual: Result<&T, Reason>,
) -> Result<(), Failure> {
    match (expected, actual) {
        (Ok(w), Ok(a)) if &w == a => Ok(()),
        (Err(w), Err(a)) if w == a => Ok(()),
        (Ok(_), Err(r)) => Err(Failure::Law(118_100 + r as u32)),
        _ => Err(Failure::Law(118_200)),
    }
}
