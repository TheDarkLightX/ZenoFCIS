use super::CollectionLimits;
use super::arithmetic::AggregateNumbers;
use super::types::{ClockParameters, Diversity, Hash, Id, Observation, Reason, Source};
use super::wire;
use super::{Evaluation, Failure, Limits, Meter, Resource, measured, step};
use alloc::{vec, vec::Vec};
use zeno_fcis_value::Value;

/// Explicit identity, clock, diversity and resource rules for a feed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedRules {
    /// Canonical feed identity.
    pub feed: Hash,
    /// Bound profile identity.
    pub profile: Hash,
    /// Canonical query identity.
    pub query: Hash,
    /// Required source diversity across five classification axes.
    pub diversity: Diversity,
    /// Largest permitted price deviation in basis points.
    pub max_deviation_bps: u128,
    /// Bound clock rules.
    pub clock: ClockParameters,
    /// Canonical collection size limits.
    pub bounds: CollectionLimits,
    /// Maximum bytes in one framed item.
    pub frame_bytes: u64,
}
impl FeedRules {
    /// Check this value against its explicit profile and arithmetic bounds.
    pub fn validate(&self) -> Result<(), Failure> {
        self.clock.validate()?;
        self.diversity.validate()?;
        if [self.feed, self.profile, self.query].contains(&[0; 32])
            || self.bounds.max_snapshot_bytes < 29
            || self.frame_bytes < 48
        {
            return Err(Failure::InvalidProfile);
        }
        Ok(())
    }
}
/// Pure aggregate over a complete already-admitted cutoff snapshot. The input
/// assumption is explicit: this does not authenticate reports or completeness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Aggregate {
    pub(super) id: Hash,
    pub(super) feed: Hash,
    pub(super) profile: Hash,
    pub(super) query: Hash,
    pub(super) round: u64,
    pub(super) reports: [Hash; 3],
    pub(super) admissions: [Hash; 3],
    pub(super) reporters: [Id; 3],
    pub(super) sources: [Id; 3],
    pub(super) numbers: AggregateNumbers,
}
impl Aggregate {
    /// Return the canonical identity.
    pub fn id(&self) -> Hash {
        self.id
    }
    /// Return the bound feed identity.
    pub fn feed(&self) -> Hash {
        self.feed
    }
    /// Return the bound profile identity.
    pub fn profile(&self) -> Hash {
        self.profile
    }
    /// Return the bound query identity.
    pub fn query(&self) -> Hash {
        self.query
    }
    /// Return the logical round identifier.
    pub fn round(&self) -> u64 {
        self.round
    }
    /// Return the exact aggregate statistics.
    pub fn numbers(&self) -> AggregateNumbers {
        self.numbers
    }
    /// Borrow the three selected report identities.
    pub fn reports(&self) -> &[Hash; 3] {
        &self.reports
    }
    /// Borrow the three selected admission witness identities.
    pub fn admissions(&self) -> &[Hash; 3] {
        &self.admissions
    }
    /// Borrow the three selected reporter identities.
    pub fn reporters(&self) -> &[Id; 3] {
        &self.reporters
    }
    /// Borrow the three selected source identities.
    pub fn sources(&self) -> &[Id; 3] {
        &self.sources
    }
    /// Encode this value in its canonical protocol format.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Failure> {
        wire::bytes(&self.value()?)
    }
    pub(super) fn value(&self) -> Result<Value, Failure> {
        let ids = |xs: &[Hash; 3]| {
            Value::tuple(
                xs.iter()
                    .map(|x| wire::blob(x))
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(|_| Failure::Canonical)
        };
        let names = |xs: &[Id; 3]| {
            Value::tuple(
                xs.iter()
                    .map(|x| wire::blob(x.bytes()))
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(|_| Failure::Canonical)
        };
        wire::record(vec![
            wire::blob(&self.feed)?,
            wire::blob(&self.profile)?,
            wire::blob(&self.query)?,
            Value::unsigned(u128::from(self.round)),
            ids(&self.reports)?,
            ids(&self.admissions)?,
            names(&self.reporters)?,
            names(&self.sources)?,
            wire::integers(&[
                self.numbers.median.raw(),
                self.numbers.confidence,
                self.numbers.deviation_bps,
                u128::from(self.numbers.oldest_epoch),
                u128::from(self.numbers.median_seconds),
            ])?,
        ])
    }
}

/// Reduce a complete admitted cutoff snapshot into one checked price aggregate.
pub fn aggregate(
    rules: &FeedRules,
    round: u64,
    cutoff: u64,
    now: u64,
    observations: &[Observation],
    sources: &[Source],
    limits: Limits,
) -> Evaluation<Result<Aggregate, Reason>> {
    measured(limits, |m| {
        let result = build(rules, round, cutoff, now, observations, sources, m)?;
        let want =
            super::quorum_laws::reference(rules, round, cutoff, now, observations, sources, m)?;
        super::quorum_laws::check(want, result.as_ref().map_err(|r| *r))?;
        Ok(result)
    })
}
pub(super) fn build(
    r: &FeedRules,
    round: u64,
    cutoff: u64,
    now: u64,
    observations: &[Observation],
    sources: &[Source],
    m: &mut Meter,
) -> Result<Result<Aggregate, Reason>, Failure> {
    let selected = match select(r, round, cutoff, now, observations, sources, m)? {
        Ok(selected) => selected,
        Err(reason) => return Ok(Err(reason)),
    };
    let numbers = super::arithmetic::numbers(
        selected.map(|o| o.value),
        selected.map(|o| o.epoch),
        selected.map(|o| o.seconds),
        m,
    )?;
    if numbers.deviation_bps > r.max_deviation_bps {
        return Ok(Err(Reason::ExcessUncertainty));
    }
    let mut out = Aggregate {
        id: [0; 32],
        feed: r.feed,
        profile: r.profile,
        query: r.query,
        round,
        reports: selected.map(|o| o.id),
        admissions: selected.map(|o| o.admission),
        reporters: selected.map(|o| o.reporter.clone()),
        sources: selected.map(|o| o.source.clone()),
        numbers,
    };
    let value = out.value()?;
    let length = wire::charge(&value, m)?;
    if length.checked_add(48).ok_or(Failure::Arithmetic)? > r.frame_bytes {
        return Err(Failure::Canonical);
    }
    out.id = wire::hash(b"zeno.oracle.aggregate/1\0", &value)?;
    Ok(Ok(out))
}

/// Time-only reduction of a complete admitted three-report quorum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeQuorum {
    pub(super) feed: Hash,
    pub(super) profile: Hash,
    pub(super) round: u64,
    pub(super) reports: [Hash; 3],
    pub(super) oldest_epoch: u64,
    pub(super) seconds: u64,
}
impl TimeQuorum {
    /// Return the quorum observation time in seconds.
    pub fn seconds(&self) -> u64 {
        self.seconds
    }
}

/// Time-only quorum reduction deliberately does not evaluate price spread.
pub fn time_quorum(
    rules: &FeedRules,
    round: u64,
    cutoff: u64,
    now: u64,
    observations: &[Observation],
    sources: &[Source],
    limits: Limits,
) -> Evaluation<Result<TimeQuorum, Reason>> {
    measured(limits, |m| {
        checked_time(rules, round, cutoff, now, observations, sources, m)
    })
}
pub(super) fn checked_time(
    rules: &FeedRules,
    round: u64,
    cutoff: u64,
    now: u64,
    observations: &[Observation],
    sources: &[Source],
    m: &mut Meter,
) -> Result<Result<TimeQuorum, Reason>, Failure> {
    let result = reduce_time(rules, round, cutoff, now, observations, sources, m)?;
    let want =
        super::quorum_laws::time_reference(rules, round, cutoff, now, observations, sources, m)?;
    super::quorum_laws::check(want, result.as_ref().map_err(|r| *r))?;
    Ok(result)
}
fn reduce_time(
    rules: &FeedRules,
    round: u64,
    cutoff: u64,
    now: u64,
    observations: &[Observation],
    sources: &[Source],
    m: &mut Meter,
) -> Result<Result<TimeQuorum, Reason>, Failure> {
    let selected = match select(rules, round, cutoff, now, observations, sources, m)? {
        Ok(x) => x,
        Err(r) => return Ok(Err(r)),
    };
    let mut seconds = selected.map(|o| o.seconds);
    for (i, j) in [(0, 1), (1, 2), (0, 1)] {
        step(m)?;
        if seconds[i] > seconds[j] {
            seconds.swap(i, j);
        }
    }
    let result = TimeQuorum {
        feed: rules.feed,
        profile: rules.profile,
        round,
        reports: selected.map(|o| o.id),
        oldest_epoch: selected
            .iter()
            .map(|o| o.epoch)
            .min()
            .ok_or(Failure::InvalidState)?,
        seconds: seconds[1],
    };
    Ok(Ok(result))
}

fn select<'a>(
    r: &FeedRules,
    round: u64,
    cutoff: u64,
    now: u64,
    observations: &'a [Observation],
    sources: &[Source],
    m: &mut Meter,
) -> Result<Result<[&'a Observation; 3], Reason>, Failure> {
    r.validate()?;
    step(m)?;
    if now < cutoff {
        return Ok(Err(Reason::WrongPhase));
    }
    if observations.len() > r.bounds.max_entries as usize
        || sources.len() > r.bounds.max_entries as usize
    {
        return Err(Failure::InvalidState);
    }
    wire::reserve_scans(
        observations
            .len()
            .checked_add(sources.len())
            .ok_or(Failure::Arithmetic)?,
        m,
    )?;
    // Every row in the complete supplied snapshot is admitted and bound; no
    // value-dependent filtering or optimistic subset fallback is permitted.
    let mut seen_reporters = Vec::new();
    let mut seen_reports = Vec::new();
    for o in observations {
        wire::visit(m)?;
        wire::charge(&wire::observation(o)?, m)?;
        if o.id == [0; 32]
            || o.admission == [0; 32]
            || seen_reporters.contains(&o.reporter)
            || seen_reports.contains(&o.id)
        {
            return Err(Failure::InvalidState);
        }
        seen_reporters.push(o.reporter.clone());
        seen_reports.push(o.id);
    }
    for (i, s) in sources.iter().enumerate() {
        wire::visit(m)?;
        wire::charge(&wire::source(s)?, m)?;
        if sources[..i].iter().any(|old| old.id == s.id) {
            return Err(Failure::InvalidState);
        }
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
    if observations
        .iter()
        .any(|o| !sources.iter().any(|s| s.id == o.source))
    {
        return Ok(Err(Reason::WrongSourceOrDiversity));
    }
    let mut ordered: Vec<&Observation> = observations.iter().collect();
    // A bounded public rank, with comparison charging before work.
    for i in 1..ordered.len() {
        let mut j = i;
        while j > 0 {
            step(m)?;
            m.charge(
                Resource::Byte,
                u64::try_from(
                    ordered[j - 1].reporter.bytes().len() + ordered[j].reporter.bytes().len(),
                )
                .map_err(|_| Failure::Arithmetic)?,
            )?;
            if (&ordered[j - 1].reporter, &ordered[j - 1].source)
                <= (&ordered[j].reporter, &ordered[j].source)
            {
                break;
            }
            ordered.swap(j - 1, j);
            j -= 1;
        }
    }
    let mut selected = None;
    'triple: for i in 0..ordered.len() {
        for j in i + 1..ordered.len() {
            for k in j + 1..ordered.len() {
                wire::visit(m)?;
                let triple = [ordered[i], ordered[j], ordered[k]];
                if diverse(&triple, sources, &r.diversity, m)? {
                    selected = Some(triple);
                    break 'triple;
                }
            }
        }
    }
    let Some(selected) = selected else {
        return Ok(Err(Reason::NoCompleteQuorum));
    };
    Ok(Ok(selected))
}
fn diverse(
    os: &[&Observation; 3],
    sources: &[Source],
    rules: &Diversity,
    m: &mut Meter,
) -> Result<bool, Failure> {
    if os[0].source == os[1].source || os[0].source == os[2].source || os[1].source == os[2].source
    {
        return Ok(false);
    }
    let mut selected = Vec::new();
    for o in os {
        let mut found = None;
        for source in sources {
            wire::visit(m)?;
            if source.id == o.source {
                found = Some(source);
            }
        }
        selected.push(found.ok_or(Failure::InvalidState)?);
    }
    for class in 0..5 {
        let ids: Vec<&Id> = selected
            .iter()
            .map(|s| match class {
                0 => &s.operator,
                1 => &s.venue,
                2 => &s.family,
                3 => &s.transport,
                _ => &s.jurisdiction,
            })
            .collect();
        let mut distinct = 0;
        let mut maximum = 0;
        for i in 0..3 {
            step(m)?;
            if !ids[..i].contains(&ids[i]) {
                distinct += 1;
            }
            maximum = maximum.max(ids.iter().filter(|x| **x == ids[i]).count());
        }
        if distinct < usize::from(rules.minimum[class])
            || maximum > usize::from(rules.max_same_class[class])
        {
            return Ok(false);
        }
    }
    Ok(true)
}
