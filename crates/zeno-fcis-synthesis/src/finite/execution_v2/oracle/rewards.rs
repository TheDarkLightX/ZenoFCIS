//! Funded, denomination-bound reward accounting. Actual transfer/publication
//! and same-root joins to Balance are deliberately outside this relation.

use super::aggregation::Aggregate;
use super::bonds::AccountContext;
use super::bonds::{BondState, DisputePhase};
use super::types::{Hash, Id, Reason, RewardUnit};
use super::wire;
use super::{Evaluation, Failure, Limits, Meter, Resource, measured, step};
use crate::finite::value::wide;
use alloc::{collections::BTreeMap, vec, vec::Vec};

/// Denomination-bound reward rates and recipient shares.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardRules {
    /// Canonical feed identity.
    pub feed: Hash,
    /// Bound profile identity.
    pub profile: Hash,
    /// Nominal reward asset and decimal scale.
    pub unit: RewardUnit,
    /// Reward amount accrued per selected report.
    pub per_report: u128,
    /// Numerator of the reporter reward share.
    pub reporter_share: u128,
    /// Numerator of the treasury reward share.
    pub treasury_share: u128,
    /// Numerator of the burned reward share.
    pub burn_share: u128,
    /// Common denominator for reward shares.
    pub denominator: u128,
    /// Treasury account identity.
    pub treasury: Id,
}
impl RewardRules {
    /// Check this value against its explicit profile and arithmetic bounds.
    pub fn validate(&self) -> Result<(), Failure> {
        self.unit.validate()?;
        let sum = self
            .reporter_share
            .checked_add(self.treasury_share)
            .and_then(|n| n.checked_add(self.burn_share))
            .ok_or(Failure::Arithmetic)?;
        if self.feed == [0; 32]
            || self.profile == [0; 32]
            || self.denominator == 0
            || sum > self.denominator
        {
            return Err(Failure::InvalidProfile);
        }
        Ok(())
    }
}
/// Payment phase of an accrued reward.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimPhase {
    /// Reward is accrued and still held.
    Held,
    /// Reward has been paid to its beneficiary.
    Paid,
    /// Reward was reclaimed because the associated report was slashed.
    ClawedBack,
}
/// Reward entitlement bound to a selected report and liability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardClaim {
    /// Canonical identity of this record.
    pub id: Hash,
    /// Aggregate selected for this operation.
    pub aggregate: Hash,
    /// Canonical report identity.
    pub report: Hash,
    /// Bond liability identity associated with this record.
    pub liability: Hash,
    /// Account entitled to the reward.
    pub beneficiary: Id,
    /// Amount in the bound reward unit.
    pub amount: u128,
    /// Current phase of this record.
    pub phase: ClaimPhase,
}
/// Owned funded reward pool, claims and replay counters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardState {
    /// Canonical feed identity.
    pub feed: Hash,
    /// Bound profile identity.
    pub profile: Hash,
    /// Nominal reward asset and decimal scale.
    pub unit: RewardUnit,
    /// Cumulative funds admitted into the reward pool.
    pub funded: u128,
    /// Unreserved funds remaining in the reward pool.
    pub remaining: u128,
    /// Cumulative rewards paid to beneficiaries.
    pub paid: u128,
    /// Cumulative rewards paid to the treasury.
    pub treasury_paid: u128,
    /// Cumulative amount explicitly burned.
    pub burned: u128,
    /// Cumulative undistributed reward rounding residue.
    pub residual: u128,
    /// Available balances by account in this state’s nominal asset.
    pub wallets: BTreeMap<Id, u128>,
    /// Retained reward entitlements and their payment status.
    pub claims: Vec<RewardClaim>,
    /// Next accepted nonce for each actor.
    pub nonces: BTreeMap<Id, u64>,
    /// Logical epoch of this state or evidence.
    pub epoch: u64,
}
/// Requested reward pool transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RewardCommand {
    /// Add funds to the denomination-bound reward pool.
    Fund(u128),
    /// Reserve a reward for one selected report.
    Accrue {
        /// Aggregate selected for this operation.
        aggregate: alloc::boxed::Box<Aggregate>,
        /// Canonical report identity.
        report: Hash,
    },
    /// Pay a mature unslashed reward claim.
    Claim(Hash),
    /// Reclaim a held reward associated with a slashed report.
    Clawback(Hash),
}
/// Owned reward successor checked by independent accounting laws.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardPlan {
    state: RewardState,
}
impl RewardPlan {
    /// Borrow the immutable checked successor.
    pub fn state(&self) -> &RewardState {
        &self.state
    }
    /// Consume the plan and return its owned successor.
    pub fn into_state(self) -> RewardState {
        self.state
    }
}
/// Evaluate a funded reward transition and its independent laws.
pub fn reward_relation(
    rules: &RewardRules,
    context: AccountContext<'_>,
    state: &RewardState,
    bond: &BondState,
    command: &RewardCommand,
    limits: Limits,
) -> Evaluation<Result<RewardPlan, Reason>> {
    measured(limits, |m| {
        checked(rules, context, state, Some(bond), command, m)
    })
}
pub(super) fn checked(
    r: &RewardRules,
    context: AccountContext<'_>,
    s: &RewardState,
    bond: Option<&BondState>,
    c: &RewardCommand,
    m: &mut Meter,
) -> Result<Result<RewardPlan, Reason>, Failure> {
    let result = run(r, context, s, bond, c, m)?;
    super::reward_laws::check(
        r,
        context,
        s,
        bond,
        c,
        result.as_ref().map(|p| p.state()).map_err(|r| *r),
        m,
    )?;
    Ok(result)
}

fn run(
    r: &RewardRules,
    context: AccountContext<'_>,
    s: &RewardState,
    bond: Option<&BondState>,
    c: &RewardCommand,
    m: &mut Meter,
) -> Result<Result<RewardPlan, Reason>, Failure> {
    let AccountContext {
        parameters: p,
        actor,
        nonce,
        clock,
        capacity,
    } = context;
    r.validate()?;
    p.validate()?;
    if let Some(b) = bond {
        super::bonds::validate(b, capacity, m)?;
    }
    let economic = p.economy.as_ref().ok_or(Failure::UnboundParameter)?;
    let dispute_rule = p.disputes.as_ref().ok_or(Failure::UnboundParameter)?;
    if r.unit != economic.reward_unit
        || r.per_report != economic.reward_per_report
        || r.unit != s.unit
        || r.feed != s.feed
        || r.profile != s.profile
    {
        return Err(Failure::InvalidProfile);
    }
    validate(s, capacity, m)?;
    charge(s, m)?;
    if let RewardCommand::Claim(id) = c
        && s.claims
            .iter()
            .any(|x| x.id == *id && x.beneficiary != *actor)
    {
        return Ok(Err(Reason::Unauthorized));
    }
    if nonce != s.nonces.get(actor).copied().unwrap_or(0) {
        return Ok(Err(Reason::Replay));
    }
    let Some(next) = nonce.checked_add(1) else {
        return Ok(Err(Reason::CounterExhausted));
    };
    if clock.backend != p.clock.as_ref().ok_or(Failure::UnboundParameter)?.backend
        || clock.finality_root == [0; 32]
    {
        return Err(Failure::InvalidEvidence);
    }
    if clock.epoch < s.epoch {
        return Ok(Err(Reason::BadClockOrSync));
    }
    charge(s, m)?;
    let mut out = s.clone();
    match c {
        RewardCommand::Fund(amount) => {
            if *amount == 0 {
                return Ok(Err(Reason::ZeroAmountOrPrice));
            }
            let wallet = s.wallets.get(actor).copied().unwrap_or(0);
            if wallet < *amount {
                return Ok(Err(Reason::InsufficientBalance));
            }
            step(m)?;
            let reporter = wide::mul_div_floor(*amount, r.reporter_share, r.denominator)
                .ok_or(Failure::Arithmetic)?;
            step(m)?;
            let treasury = wide::mul_div_floor(*amount, r.treasury_share, r.denominator)
                .ok_or(Failure::Arithmetic)?;
            step(m)?;
            let burned = wide::mul_div_floor(*amount, r.burn_share, r.denominator)
                .ok_or(Failure::Arithmetic)?;
            let paid = reporter
                .checked_add(treasury)
                .and_then(|x| x.checked_add(burned))
                .ok_or(Failure::Arithmetic)?;
            let residual = amount.checked_sub(paid).ok_or(Failure::Arithmetic)?;
            out.wallets.insert(actor.clone(), wallet - amount);
            if treasury > 0 {
                let old = out.wallets.get(&r.treasury).copied().unwrap_or(0);
                out.wallets.insert(
                    r.treasury.clone(),
                    old.checked_add(treasury).ok_or(Failure::Arithmetic)?,
                );
            }
            out.funded = s.funded.checked_add(*amount).ok_or(Failure::Arithmetic)?;
            out.remaining = s
                .remaining
                .checked_add(reporter)
                .ok_or(Failure::Arithmetic)?;
            out.treasury_paid = s
                .treasury_paid
                .checked_add(treasury)
                .ok_or(Failure::Arithmetic)?;
            out.burned = s.burned.checked_add(burned).ok_or(Failure::Arithmetic)?;
            out.residual = s
                .residual
                .checked_add(residual)
                .ok_or(Failure::Arithmetic)?;
        }
        RewardCommand::Accrue { aggregate, report } => {
            let bond = bond.ok_or(Failure::InvalidState)?;
            let selected = aggregate.reports().iter().position(|x| x == report);
            let Some(index) = selected else {
                return Ok(Err(Reason::UnknownReference));
            };
            let liability = bond
                .liabilities
                .iter()
                .find(|x| x.report == *report && x.feed == r.feed);
            let Some(liability) = liability else {
                return Ok(Err(Reason::UnknownReference));
            };
            if aggregate.feed() != r.feed
                || aggregate.profile() != r.profile
                || aggregate.reporters()[index] != bond.reporter
            {
                return Ok(Err(Reason::BindingMismatch));
            }
            let id = claim_id(r, aggregate.id(), *report, &bond.owner)?;
            if s.remaining < r.per_report {
                return Ok(Err(Reason::InsufficientRewardBudget));
            }
            if s.claims.iter().any(|x| x.id == id) {
                return Ok(Err(Reason::AlreadyClaimedOrSlashed));
            }
            if out.claims.len() >= capacity as usize {
                return Ok(Err(Reason::Full));
            }
            out.remaining -= r.per_report;
            out.claims.push(RewardClaim {
                id,
                aggregate: aggregate.id(),
                report: *report,
                liability: liability.id,
                beneficiary: bond.owner.clone(),
                amount: r.per_report,
                phase: ClaimPhase::Held,
            });
        }
        RewardCommand::Claim(id) | RewardCommand::Clawback(id) => {
            let bond = bond.ok_or(Failure::InvalidState)?;
            let Some(index) = s.claims.iter().position(|x| x.id == *id) else {
                return Ok(Err(Reason::UnknownReference));
            };
            let claim = &s.claims[index];
            let Some(liability) = bond.liabilities.iter().find(|x| x.id == claim.liability) else {
                return Ok(Err(Reason::UnknownReference));
            };
            if bond.owner != claim.beneficiary || liability.report != claim.report {
                return Ok(Err(Reason::BindingMismatch));
            }
            if matches!(c, RewardCommand::Claim(_)) {
                if !liability.cleared
                    || bond
                        .disputes
                        .iter()
                        .any(|d| d.liability == liability.id && d.phase == DisputePhase::Open)
                {
                    return Ok(Err(Reason::LiabilityLocked));
                }
                if claim.phase != ClaimPhase::Held {
                    return Ok(Err(Reason::AlreadyClaimedOrSlashed));
                }
                let balance = out.wallets.get(&claim.beneficiary).copied().unwrap_or(0);
                out.wallets.insert(
                    claim.beneficiary.clone(),
                    balance
                        .checked_add(claim.amount)
                        .ok_or(Failure::Arithmetic)?,
                );
                out.paid = s
                    .paid
                    .checked_add(claim.amount)
                    .ok_or(Failure::Arithmetic)?;
                out.claims[index].phase = ClaimPhase::Paid;
            } else {
                if !bond.disputes.iter().any(|d| {
                    d.liability == liability.id
                        && (d.phase == DisputePhase::ResolvedUpheld
                            || d.phase == DisputePhase::Open
                                && d.slash == dispute_rule.slash
                                && d.slash_verdict
                                    .as_ref()
                                    .is_some_and(|v| v.upheld && d.violation == Some(v.violation)))
                }) {
                    return Ok(Err(Reason::InvalidVerdict));
                }
                if claim.phase != ClaimPhase::Held {
                    return Ok(Err(Reason::AlreadyClaimedOrSlashed));
                }
                out.remaining = s
                    .remaining
                    .checked_add(claim.amount)
                    .ok_or(Failure::Arithmetic)?;
                out.claims[index].phase = ClaimPhase::ClawedBack;
            }
        }
    }
    out.nonces.insert(actor.clone(), next);
    out.epoch = clock.epoch;
    if [out.claims.len(), out.wallets.len(), out.nonces.len()]
        .iter()
        .any(|n| *n > capacity as usize)
    {
        return Ok(Err(Reason::Full));
    }
    validate(&out, capacity, m)?;
    charge(&out, m)?;
    Ok(Ok(RewardPlan { state: out }))
}
pub(super) fn claim_id(
    r: &RewardRules,
    aggregate: Hash,
    report: Hash,
    beneficiary: &Id,
) -> Result<Hash, Failure> {
    wire::hash(
        b"zeno.oracle.reward/1\0",
        &wire::record(vec![
            wire::blob(&r.feed)?,
            wire::blob(&r.profile)?,
            wire::blob(&aggregate)?,
            wire::blob(&report)?,
            wire::blob(beneficiary.bytes())?,
            wire::blob(r.unit.asset.bytes())?,
            zeno_fcis_value::Value::unsigned(u128::from(r.unit.scale)),
        ])?,
    )
}
pub(super) fn validate(s: &RewardState, capacity: u32, m: &mut Meter) -> Result<(), Failure> {
    s.unit.validate()?;
    if [s.claims.len(), s.wallets.len(), s.nonces.len()]
        .iter()
        .any(|n| *n > capacity as usize)
    {
        return Err(Failure::InvalidState);
    }
    let rows = s
        .claims
        .len()
        .checked_add(s.wallets.len())
        .and_then(|n| n.checked_add(s.nonces.len()))
        .ok_or(Failure::Arithmetic)?;
    wire::reserve_scans(rows, m)?;
    let mut held = 0u128;
    let mut paid = 0u128;
    for (i, c) in s.claims.iter().enumerate() {
        wire::visit(m)?;
        if s.claims[..i].iter().any(|old| old.id == c.id) {
            return Err(Failure::InvalidState);
        }
        if c.phase == ClaimPhase::Held {
            held = held.checked_add(c.amount).ok_or(Failure::Arithmetic)?;
        }
        if c.phase == ClaimPhase::Paid {
            paid = paid.checked_add(c.amount).ok_or(Failure::Arithmetic)?;
        }
    }
    let total = s
        .remaining
        .checked_add(held)
        .and_then(|x| x.checked_add(s.paid))
        .and_then(|x| x.checked_add(s.treasury_paid))
        .and_then(|x| x.checked_add(s.burned))
        .and_then(|x| x.checked_add(s.residual))
        .ok_or(Failure::Arithmetic)?;
    if total != s.funded || paid != s.paid {
        return Err(Failure::InvalidState);
    }
    Ok(())
}
fn charge(s: &RewardState, m: &mut Meter) -> Result<(), Failure> {
    let rows = s
        .claims
        .len()
        .checked_add(s.wallets.len())
        .and_then(|n| n.checked_add(s.nonces.len()))
        .ok_or(Failure::Arithmetic)?;
    let bound = u64::try_from(rows)
        .map_err(|_| Failure::Arithmetic)?
        .checked_mul(512)
        .and_then(|n| n.checked_add(256))
        .ok_or(Failure::Arithmetic)?;
    m.charge(Resource::Byte, bound)?;
    for _ in 0..rows {
        wire::visit(m)?;
    }
    Ok(())
}
