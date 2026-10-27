//! Complete native lifecycle reference: ordered reason predicates and exact
//! successors/frames, independently written from the native producer.
use super::aggregation::Aggregate;
use super::bonds::{BondCommand, BondState, ReporterPhase};
use super::lifecycle::{
    AdmittedReport, FeedRegistration, LifecycleCommand, LifecycleContext, LifecycleState,
    ReporterRecord, RetainedKey, RoundRecord, registration_message, rotation_message,
};
use super::lifecycle_schema;
use super::prices::PriceState;
use super::reports::ReportChain;
use super::rewards::ClaimPhase;
use super::types::{Hash, Id, REASONS, Reason, ZdexE18};
use super::verification::BlsVerifier;
use super::wire;
use super::{Evaluation, Failure, Limits, Meter, measured};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use zeno_fcis_codec::CommitmentHasher;

/// Independently check the complete native outcome, accounting and ordered refusal.
pub fn check_lifecycle_laws(
    verifier: &impl BlsVerifier,
    s: &LifecycleState,
    actor: &Id,
    ctx: LifecycleContext,
    c: &LifecycleCommand,
    proposed: Result<&LifecycleState, Reason>,
    limits: Limits,
) -> Evaluation<()> {
    measured(limits, |m| check(verifier, s, actor, ctx, c, proposed, m))
}
pub(super) fn check(
    verifier: &impl BlsVerifier,
    s: &LifecycleState,
    actor: &Id,
    ctx: LifecycleContext,
    c: &LifecycleCommand,
    proposed: Result<&LifecycleState, Reason>,
    m: &mut Meter,
) -> Result<(), Failure> {
    wire::reserve_scans(
        super::lifecycle::charged_row_count(s, m)?
            .checked_add(32)
            .ok_or(Failure::Arithmetic)?,
        m,
    )?;
    // Every native command conserves custody, independently of the reference
    // successors below; the oracle never issues zDEX or reward value.
    if proposed.is_ok_and(|post| custody(post) != custody(s)) {
        return Err(Failure::Law(115_500));
    }
    if let LifecycleCommand::Reward { feed, command } = c {
        return super::native_rewards::check(s, actor, ctx, *feed, command, proposed, m);
    }
    if let LifecycleCommand::Price { feed, command } = c {
        return super::native_ports::check_price_port(s, actor, ctx, *feed, command, proposed, m);
    }
    let expected = reference(verifier, s, actor, ctx, c, m)?;
    match (&expected, proposed) {
        (Ok(want), Ok(actual)) if want == actual => Ok(()),
        (Err(want), Err(actual)) if *want == actual => Ok(()),
        (Ok(_), Err(r)) => Err(Failure::Law(115_100 + r as u32)),
        (Err(_), Ok(_)) => Err(Failure::Law(115_200)),
        _ => Err(Failure::Law(115_201)),
    }
}
fn reason(f: &[bool; 32]) -> Option<Reason> {
    f.iter().position(|x| *x).map(|i| REASONS[i])
}
/// Exact (carry, low) custody of the [zDEX, reward asset] domains: wallets,
/// open pool value (remaining, held claims, residual) and declared burns, plus
/// held bonds and open challenge escrow for zDEX. Applied penalties and paid
/// totals are history, not custody.
fn custody(s: &LifecycleState) -> [(u128, u128); 2] {
    fn add((carry, low): (u128, u128), n: u128) -> (u128, u128) {
        let (sum, over) = low.overflowing_add(n);
        (carry + u128::from(over), sum)
    }
    let mut zdex = s.wallets.values().fold((0, 0), |t, w| add(t, w.0));
    for b in s.bonds.values() {
        zdex = add(add(zdex, b.held.0), b.burned.0);
        zdex = b.disputes.iter().fold(zdex, |t, d| add(t, d.escrow.0));
    }
    let mut reward = s.reward_wallets.values().fold((0, 0), |t, n| add(t, *n));
    for r in s.rewards.values() {
        let pool = if r.unit.asset == s.profile.zdex_asset {
            &mut zdex
        } else {
            &mut reward
        };
        *pool = add(add(add(*pool, r.remaining), r.residual), r.burned);
        for c in r.claims.iter().filter(|c| c.phase == ClaimPhase::Held) {
            *pool = add(*pool, c.amount);
        }
    }
    [zdex, reward]
}
/// The least k with first_open + k*heartbeat + cutoff_delta > now, by exact
/// division; clock validation already refused a zero heartbeat.
fn first_live_round(f: &FeedRegistration, heartbeat: u64, now: u64) -> Result<u128, Failure> {
    let first_cutoff = u128::from(f.first_open) + u128::from(f.cutoff_delta);
    let now = u128::from(now);
    if now < first_cutoff {
        return Ok(0);
    }
    let passed = (now - first_cutoff)
        .checked_div(u128::from(heartbeat))
        .ok_or(Failure::InvalidProfile)?;
    Ok(passed + 1)
}

pub(super) fn reference(
    verifier: &impl BlsVerifier,
    s: &LifecycleState,
    actor: &Id,
    ctx: LifecycleContext,
    c: &LifecycleCommand,
    m: &mut Meter,
) -> Result<Result<LifecycleState, Reason>, Failure> {
    super::lifecycle::crypto_shape(verifier, c, m)?;
    let p = &s.profile.parameters;
    p.validate()?;
    if s.root()? != ctx.pre_root {
        return Err(Failure::InvalidEvidence);
    }
    let timing = p.clock.as_ref().ok_or(Failure::UnboundParameter)?;
    if ctx.clock.backend != timing.backend || ctx.clock.finality_root == [0; 32] {
        return Err(Failure::InvalidEvidence);
    }
    let now = ctx.clock.epoch;
    let mut failures = [false; 32];
    let mut post = s.clone();
    failures[1] = s.nonces.get(actor).copied().unwrap_or(0) != ctx.nonce;
    failures[2] = ctx.nonce == u64::MAX;
    failures[3] = now < s.epoch;
    match c {
        LifecycleCommand::Price { .. } | LifecycleCommand::Reward { .. } => {
            return Err(Failure::Law(115_400));
        }
        LifecycleCommand::RegisterFeed(f) => failures[5] = s.feeds.contains_key(&f.rules.feed),
        LifecycleCommand::RegisterKey { key, .. } => {
            failures[0] = &key.owner != actor;
            failures[5] = s.reporters.contains_key(&key.reporter);
        }
        LifecycleCommand::RotateKey { key, .. } => {
            let r = s.reporters.get(&key.reporter);
            failures[0] = r.is_some_and(|r| &r.owner != actor);
            failures[4] = r.is_none();
            failures[2] |= r.is_some_and(|r| r.active_version == u64::MAX);
            failures[5] = s.keys.contains_key(&(key.reporter.clone(), key.version));
            failures[6] = s
                .bonds
                .get(&key.reporter)
                .is_some_and(|b| b.phase != ReporterPhase::Active);
        }
        LifecycleCommand::OpenRound { feed, round } => {
            let old = s
                .rounds
                .keys()
                .filter(|(f, _)| f == feed)
                .map(|(_, r)| *r)
                .max();
            failures[2] |= old == Some(u64::MAX);
            failures[4] = !s.feeds.contains_key(feed);
            failures[5] = s.rounds.contains_key(&(*feed, *round));
            // Relation revision 2: R07 admits rounds from the next unopened one
            // up to the earliest scheduled round whose cutoff has not passed.
            // Rounds before that one expired unopened (R21) and stay skipped.
            let next = old.map_or(0, |r| u128::from(r) + 1);
            let live = s
                .feeds
                .get(feed)
                .map(|f| first_live_round(f, timing.heartbeat, now))
                .transpose()?;
            let r = u128::from(*round);
            failures[6] = live.is_some_and(|live| r < next || r > next.max(live));
        }
        LifecycleCommand::SubmitReport(raw) => {
            let r = &raw.payload;
            let who = s.reporters.get(&r.reporter);
            failures[2] |= who.is_some_and(|x| x.chain.next_sequence == u64::MAX);
            failures[4] = who.is_none()
                || !s.keys.contains_key(&(r.reporter.clone(), r.key_version))
                || !s.feeds.contains_key(&r.feed)
                || !s.rounds.contains_key(&(r.feed, r.round));
            failures[5] = s.reports.values().any(|x| {
                x.report.id == raw.id
                    || x.report.payload.reporter == r.reporter
                        && x.report.payload.feed == r.feed
                        && x.report.payload.round == r.round
            });
            failures[6] = s
                .bonds
                .get(&r.reporter)
                .is_some_and(|b| b.phase != ReporterPhase::Active)
                || who.is_some_and(|x| x.active_version != r.key_version)
                || s.rounds
                    .get(&(r.feed, r.round))
                    .is_some_and(|x| x.aggregate.is_some() || now < x.opening || now >= x.cutoff);
        }
        LifecycleCommand::BuildAggregate { feed, round } => {
            let r = s.rounds.get(&(*feed, *round));
            failures[4] = !s.feeds.contains_key(feed) || r.is_none();
            failures[6] = r.is_some_and(|x| x.aggregate.is_some() || now < x.cutoff);
        }
        LifecycleCommand::Bond { reporter, command } => {
            failures[4] = !s.bonds.contains_key(reporter);
            failures[0] = matches!(command, BondCommand::Unregister | BondCommand::Withdraw(_))
                && s.bonds.get(reporter).is_some_and(|b| &b.owner != actor);
        }
        LifecycleCommand::UpdateStress { feed, .. } => failures[4] = !s.feeds.contains_key(feed),
    }
    // Later predicates are needed only when the earlier guard partition is
    // empty. This preserves short-circuit work without inventing validity bits.
    if let Some(r) = reason(&failures) {
        return Ok(Err(r));
    }
    match c {
        LifecycleCommand::Price { .. } | LifecycleCommand::Reward { .. } => {
            return Err(Failure::Law(115_400));
        }
        LifecycleCommand::RegisterFeed(f) => {
            f.rules.validate()?;
            failures[9] = f.id()? != f.rules.feed
                || f.rules.profile != s.profile_id
                || f.rules.clock != *timing
                || f.rules.bounds != s.profile.bounds
                || f.rules.frame_bytes != s.profile.frame_bytes;
            if f.cutoff_delta == 0 || f.cutoff_delta > timing.heartbeat || f.base == f.quote {
                return Err(Failure::InvalidProfile);
            }
            let ids: BTreeSet<_> = f.sources.iter().map(|x| &x.id).collect();
            failures[15] =
                ids.len() != f.sources.len() || f.assignments.values().any(|x| !ids.contains(x));
            post.feeds.insert(f.rules.feed, f.clone());
            post.prices.insert(f.rules.feed, PriceState::default());
            post.rewards
                .insert(f.rules.feed, super::native_rewards::empty(s, f.rules.feed)?);
        }
        LifecycleCommand::RegisterKey { key, authorization } => {
            super::lifecycle::charge_crypto(m, 272)?;
            let msg = registration_message(s, ctx.pre_root, ctx.nonce, key)?;
            failures[7] = !verifier.signature(&key.public_key, &msg, authorization);
            failures[8] =
                key.version != 0 || !verifier.possession(&key.public_key, &key.possession);
            post.keys.insert(
                (key.reporter.clone(), 0),
                RetainedKey {
                    key: key.clone(),
                    activated: now,
                    retired: None,
                    activation_sequence: 0,
                    retirement_sequence: None,
                    registration_root: ctx.pre_root,
                    registration_nonce: ctx.nonce,
                    old_version: None,
                    authorization: *authorization,
                    old_authorization: None,
                },
            );
            post.reporters.insert(
                key.reporter.clone(),
                ReporterRecord {
                    owner: key.owner.clone(),
                    active_version: 0,
                    registered: now,
                    chain: ReportChain::default(),
                },
            );
            post.bonds.insert(
                key.reporter.clone(),
                BondState {
                    reporter: key.reporter.clone(),
                    owner: key.owner.clone(),
                    phase: ReporterPhase::Active,
                    held: ZdexE18(0),
                    liabilities: Vec::new(),
                    disputes: Vec::new(),
                    wallets: BTreeMap::new(),
                    nonces: BTreeMap::new(),
                    burned: ZdexE18(0),
                    exit_epoch: None,
                    epoch: now,
                },
            );
        }
        LifecycleCommand::RotateKey {
            key,
            authorization,
            old_authorization,
        } => {
            let version = s
                .reporters
                .get(&key.reporter)
                .ok_or(Failure::InvalidState)?
                .active_version;
            let old = s
                .keys
                .get(&(key.reporter.clone(), version))
                .ok_or(Failure::InvalidState)?;
            let msg = rotation_message(s, ctx.pre_root, ctx.nonce, version, key)?;
            super::lifecycle::charge_crypto(m, 400)?;
            failures[7] = !verifier.signature(&old.key.public_key, &msg, old_authorization)
                || !verifier.signature(&key.public_key, &msg, authorization);
            failures[8] = !verifier.possession(&key.public_key, &key.possession)
                || key.owner != old.key.owner
                || key.version != version.checked_add(1).ok_or(Failure::Arithmetic)?;
            post.keys
                .get_mut(&(key.reporter.clone(), version))
                .ok_or(Failure::InvalidState)?
                .retired = Some(now);
            let cutover = s
                .reporters
                .get(&key.reporter)
                .ok_or(Failure::InvalidState)?
                .chain
                .next_sequence;
            post.keys
                .get_mut(&(key.reporter.clone(), version))
                .ok_or(Failure::InvalidState)?
                .retirement_sequence = Some(cutover);
            post.keys.insert(
                (key.reporter.clone(), key.version),
                RetainedKey {
                    key: key.clone(),
                    activated: now,
                    retired: None,
                    activation_sequence: cutover,
                    retirement_sequence: None,
                    registration_root: ctx.pre_root,
                    registration_nonce: ctx.nonce,
                    old_version: Some(version),
                    authorization: *authorization,
                    old_authorization: Some(*old_authorization),
                },
            );
            post.reporters
                .get_mut(&key.reporter)
                .ok_or(Failure::InvalidState)?
                .active_version = key.version;
        }
        LifecycleCommand::OpenRound { feed, round } => {
            let f = s.feeds.get(feed).ok_or(Failure::InvalidState)?;
            let opening = round
                .checked_mul(timing.heartbeat)
                .and_then(|r| f.first_open.checked_add(r))
                .ok_or(Failure::Arithmetic)?;
            let cutoff = opening
                .checked_add(f.cutoff_delta)
                .ok_or(Failure::Arithmetic)?;
            failures[20] = now < opening || now >= cutoff;
            let eligible = f
                .assignments
                .keys()
                .filter_map(|id| {
                    s.bonds
                        .get(id)
                        .filter(|b| b.phase == ReporterPhase::Active)
                        .map(|_| id.clone())
                })
                .collect();
            post.rounds.insert(
                (*feed, *round),
                RoundRecord {
                    opening,
                    cutoff,
                    eligible,
                    aggregate: None,
                    cutoff_admissions: None,
                    built_epoch: None,
                },
            );
        }
        LifecycleCommand::SubmitReport(raw) => {
            let r = &raw.payload;
            let who = s.reporters.get(&r.reporter).ok_or(Failure::InvalidState)?;
            let key = s
                .keys
                .get(&(r.reporter.clone(), r.key_version))
                .ok_or(Failure::InvalidState)?;
            let feed = s.feeds.get(&r.feed).ok_or(Failure::InvalidState)?;
            let round = s
                .rounds
                .get(&(r.feed, r.round))
                .ok_or(Failure::InvalidState)?;
            super::lifecycle::charge_crypto(m, 128)?;
            failures[7] = !verifier.signature(&key.key.public_key, &r.message(), &raw.signature);
            failures[9] = r.chain != s.chain
                || r.query != feed.rules.query
                || r.profile != feed.rules.profile
                || raw.id != r.report_id(&raw.signature)
                || raw.payload_hash
                    != *zeno_fcis_crypto::RustCryptoSha256::hash(&r.canonical_json()).as_bytes();
            failures[10] =
                r.sequence != who.chain.next_sequence || r.previous != who.chain.previous;
            failures[12] = r.observed_epoch > now;
            failures[13] = who
                .chain
                .last_seconds
                .is_some_and(|old| r.observation_seconds < old);
            failures[14] = r.observed_epoch <= now && now - r.observed_epoch > timing.max_age;
            failures[15] = !round.eligible.contains(&r.reporter)
                || feed.assignments.get(&r.reporter) != Some(&r.source);
            let b = s.bonds.get(&r.reporter).ok_or(Failure::InvalidState)?;
            let locked = b.liabilities.iter().try_fold(0u128, |a, l| {
                a.checked_add(l.locked.0).ok_or(Failure::Arithmetic)
            })?;
            failures[16] = b.held.0.checked_sub(locked).ok_or(Failure::InvalidState)?
                < p.disputes
                    .as_ref()
                    .ok_or(Failure::UnboundParameter)?
                    .minimum_bond
                    .0;
            failures[20] = now < round.opening || now >= round.cutoff;
            if let Some(r) = reason(&failures) {
                return Ok(Err(r));
            }
            let liability = super::lifecycle::liability_id(s, raw)?;
            let mut input = b.clone();
            input.wallets = s.wallets.clone();
            input.nonces.clear();
            let reserved = super::bond_laws::reference(
                p,
                &input,
                &who.owner,
                0,
                &BondCommand::Reserve {
                    id: liability,
                    report: raw.id,
                    feed: r.feed,
                },
                ctx.clock,
                s.profile.bounds.max_entries,
            )?;
            let mut reserved = match reserved {
                Ok(x) => x,
                Err(r) => return Ok(Err(r)),
            };
            reserved.wallets.clear();
            reserved.nonces.clear();
            post.bonds.insert(r.reporter.clone(), reserved);
            let admission = super::lifecycle::admission_id(s, raw, now, liability)?;
            post.reports.insert(
                raw.id,
                AdmittedReport {
                    report: raw.clone(),
                    submitted: now,
                    admission,
                    liability,
                },
            );
            post.reporters
                .get_mut(&r.reporter)
                .ok_or(Failure::InvalidState)?
                .chain = ReportChain {
                next_sequence: r.sequence.checked_add(1).ok_or(Failure::Arithmetic)?,
                previous: Some(raw.id),
                last_seconds: Some(r.observation_seconds),
            };
        }
        LifecycleCommand::BuildAggregate { feed, round } => {
            let f = s.feeds.get(feed).ok_or(Failure::InvalidState)?;
            let built = aggregate_reference(s, *feed, *round, now, m)?;
            let a = match built {
                Ok(x) => x,
                Err(r) => return Ok(Err(r)),
            };
            if a.reports()
                .iter()
                .any(|id| super::native_ports::quarantines(s, *feed).contains(id))
            {
                return Ok(Err(Reason::DisputedOrFrozen));
            }
            let row = post
                .rounds
                .get_mut(&(*feed, *round))
                .ok_or(Failure::InvalidState)?;
            row.aggregate = Some(a.id());
            row.cutoff_admissions = Some(super::lifecycle::cutoff_commitment(s, *feed, *round)?);
            row.built_epoch = Some(now);
            if a.profile() != f.rules.profile {
                return Err(Failure::Law(115_300));
            }
            post.aggregates.insert(a.id(), a);
        }
        LifecycleCommand::Bond { reporter, command } => {
            if matches!(
                command,
                BondCommand::Reserve { .. } | BondCommand::RecordPublication { .. }
            ) {
                return Err(Failure::UnboundParameter);
            }
            let mut input = s.bonds.get(reporter).ok_or(Failure::InvalidState)?.clone();
            input.wallets = s.wallets.clone();
            input.nonces = s.nonces.clone();
            let b = super::bond_laws::reference(
                p,
                &input,
                actor,
                ctx.nonce,
                command,
                ctx.clock,
                s.profile.bounds.max_entries,
            )?;
            let mut b = match b {
                Ok(x) => x,
                Err(r) => return Ok(Err(r)),
            };
            post.wallets = core::mem::take(&mut b.wallets);
            b.nonces.clear();
            post.bonds.insert(reporter.clone(), b);
            if let BondCommand::Slash { dispute, .. } = command {
                let liability = post
                    .bonds
                    .get(reporter)
                    .and_then(|b| b.disputes.iter().find(|d| d.id == *dispute))
                    .ok_or(Failure::InvalidState)?
                    .liability;
                super::native_rewards::slash_claims(
                    &mut post, reporter, liability, ctx.clock, true, m,
                )?;
            }
        }
        LifecycleCommand::UpdateStress {
            feed,
            observation: o,
        } => {
            if o.source
                != p.economy
                    .as_ref()
                    .ok_or(Failure::UnboundParameter)?
                    .stress_source
                || o.valuation_root == [0; 32]
            {
                return Err(Failure::InvalidEvidence);
            }
            failures[12] = o.observed_epoch > now;
            failures[13] = s
                .stress
                .get(feed)
                .is_some_and(|old| o.observed_epoch < old.observed_epoch);
            failures[14] = o.observed_epoch <= now && now - o.observed_epoch > timing.max_age;
            post.stress.insert(*feed, o.clone());
        }
    }
    if let Some(r) = reason(&failures) {
        return Ok(Err(r));
    }
    post.epoch = now;
    post.nonces.insert(
        actor.clone(),
        ctx.nonce.checked_add(1).ok_or(Failure::Arithmetic)?,
    );
    match lifecycle_schema::fit(&post)? {
        Ok(()) => Ok(Ok(post)),
        Err(r) => Ok(Err(r)),
    }
}

pub(super) fn aggregate_reference(
    s: &LifecycleState,
    feed: Hash,
    round: u64,
    now: u64,
    m: &mut Meter,
) -> Result<Result<Aggregate, Reason>, Failure> {
    let f = s.feeds.get(&feed).ok_or(Failure::InvalidState)?;
    let r = s.rounds.get(&(feed, round)).ok_or(Failure::InvalidState)?;
    super::quorum_laws::reference(
        &f.rules,
        round,
        r.cutoff,
        now,
        &super::lifecycle::observations(s, feed, round),
        &f.sources,
        m,
    )
}
