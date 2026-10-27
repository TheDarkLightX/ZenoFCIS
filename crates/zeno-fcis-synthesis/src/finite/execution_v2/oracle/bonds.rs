//! Pure bonded-accounting relation over an already registered reporter. Actor,
//! finality and adjudication acquisition remain external premises; this module
//! supplies no signature-admission or monetary publication capability.

use super::aggregation::Aggregate;
use super::prices::ClockView;
use super::types::{DisputeParameters, EscrowRecipient, Hash, Id, Parameters, Reason, ZdexE18};
use super::wire;
use super::{Evaluation, Failure, Limits, Meter, Resource, measured};
use alloc::{collections::BTreeMap, vec::Vec};

/// Immutable actor, clock and policy inputs shared by account relations.
#[derive(Clone, Copy, Debug)]
pub struct AccountContext<'a> {
    /// Fully bound oracle policy.
    pub parameters: &'a Parameters,
    /// Account submitting the command.
    pub actor: &'a Id,
    /// Expected actor nonce before this command.
    pub nonce: u64,
    /// Authenticated clock premise supplied by the caller.
    pub clock: ClockView,
    /// Maximum retained rows in each account collection.
    pub capacity: u32,
}
/// Lifecycle phase of a bonded reporter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReporterPhase {
    /// Reporter may reserve new liabilities.
    Active,
    /// Reporter is exiting while retained liabilities remain collectible.
    Unbonding,
    /// Reporter has completed withdrawal and liability release.
    Retired,
}
/// Collectible bond reservation and release schedule for one report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Liability {
    /// Canonical identity of this record.
    pub id: Hash,
    /// Canonical report identity.
    pub report: Hash,
    /// Canonical feed identity.
    pub feed: Hash,
    /// zDEX reserved against this report.
    pub locked: ZdexE18,
    /// Publication epoch for this liability, if recorded.
    pub publication: Option<u64>,
    /// Epoch through which this report remains economically exposed.
    pub exposure_until: u64,
    /// Earliest epoch at which this liability may be cleared.
    pub clear_after: u64,
    /// Whether this liability has been released.
    pub cleared: bool,
}
/// Resolution phase of a bonded report dispute.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputePhase {
    /// Dispute awaits a final admissible resolution.
    Open,
    /// Dispute resolved with an upheld final verdict.
    ResolvedUpheld,
    /// Dispute resolved with a rejected final verdict.
    ResolvedRejected,
}
/// Challenge escrow, verdicts and permanent penalty history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    /// Canonical identity of this record.
    pub id: Hash,
    /// Bond liability identity associated with this record.
    pub liability: Hash,
    /// Account that funded this dispute.
    pub challenger: Id,
    /// zDEX challenge bond still held in custody.
    pub escrow: ZdexE18,
    /// Epoch at which this dispute opened.
    pub opened: u64,
    /// Current phase of this record.
    pub phase: DisputePhase,
    /// Scheduled or applied zDEX penalty.
    pub slash: ZdexE18,
    /// Permanent violation nullifier, if recorded.
    pub violation: Option<Hash>,
    /// Final verdict used to apply the penalty, if any.
    pub slash_verdict: Option<FinalVerdict>,
    /// Final verdict used to resolve the dispute, if any.
    pub resolution_verdict: Option<FinalVerdict>,
}
/// Owned reporter custody, liabilities and replay counters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BondState {
    /// Reporter identity.
    pub reporter: Id,
    /// Account that owns the reporter bond.
    pub owner: Id,
    /// Current phase of this record.
    pub phase: ReporterPhase,
    /// zDEX bond held in custody.
    pub held: ZdexE18,
    /// Retained report bond reservations.
    pub liabilities: Vec<Liability>,
    /// Retained dispute records.
    pub disputes: Vec<Dispute>,
    /// Available balances by account in this state’s nominal asset.
    pub wallets: BTreeMap<Id, ZdexE18>,
    /// Next accepted nonce for each actor.
    pub nonces: BTreeMap<Id, u64>,
    /// Cumulative amount explicitly burned.
    pub burned: ZdexE18,
    /// Epoch at which unbonding began, if requested.
    pub exit_epoch: Option<u64>,
    /// Logical epoch of this state or evidence.
    pub epoch: u64,
}
/// Requested bonded-account transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BondCommand {
    /// Add zDEX from the actor’s available wallet.
    Deposit(ZdexE18),
    /// Reserve a bond liability for one report.
    Reserve {
        /// Canonical identity of this record.
        id: Hash,
        /// Canonical report identity.
        report: Hash,
        /// Canonical feed identity.
        feed: Hash,
    },
    /// Record the publication epoch of a selected report.
    RecordPublication {
        /// Bond liability identity associated with this record.
        liability: Hash,
        /// Aggregate selected for this operation.
        aggregate: alloc::boxed::Box<Aggregate>,
    },
    /// Begin reporter unbonding.
    Unregister,
    /// Release a liability after all exposure and dispute bounds.
    Clear {
        /// Bond liability identity associated with this record.
        liability: Hash,
    },
    /// Withdraw free zDEX after the exit delay.
    Withdraw(ZdexE18),
    /// Escrow a challenge bond against a published report.
    OpenDispute {
        /// Canonical identity of this record.
        id: Hash,
        /// Bond liability identity associated with this record.
        liability: Hash,
    },
    /// Apply the scheduled penalty using a final upheld verdict.
    Slash {
        /// Dispute identity bound into this verdict.
        dispute: Hash,
        /// Adjudication evidence for this dispute operation.
        verdict: FinalVerdict,
    },
    /// Resolve challenge escrow using admissible final adjudication.
    Resolve {
        /// Dispute identity bound into this verdict.
        dispute: Hash,
        /// Adjudication evidence for this dispute operation.
        verdict: Verdict,
    },
}
/// Final adjudication evidence bound to one dispute and violation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalVerdict {
    /// Identity of the bound final adjudicator.
    pub adjudicator: Hash,
    /// Dispute identity bound into this verdict.
    pub dispute: Hash,
    /// Permanent violation nullifier, if recorded.
    pub violation: Hash,
    /// Whether the final verdict upholds the dispute.
    pub upheld: bool,
    /// Epoch at which the adjudication became final.
    pub final_epoch: u64,
    /// Authenticated finality evidence root.
    pub finality_root: Hash,
}
/// Inconclusive or final adjudication evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// No final adjudication is available.
    Inconclusive,
    /// Final adjudication bound to one dispute.
    Final(FinalVerdict),
}
/// Owned bonded-account successor checked by independent laws.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BondPlan {
    state: BondState,
}
impl BondPlan {
    /// Borrow the immutable checked successor.
    pub fn state(&self) -> &BondState {
        &self.state
    }
    /// Consume the plan and return its owned successor.
    pub fn into_state(self) -> BondState {
        self.state
    }
}
/// Evaluate a bonded-account transition and its independent laws.
pub fn bond_relation(
    context: AccountContext<'_>,
    state: &BondState,
    command: &BondCommand,
    limits: Limits,
) -> Evaluation<Result<BondPlan, Reason>> {
    measured(limits, |m| checked(context, state, command, m))
}
pub(super) fn checked(
    context: AccountContext<'_>,
    s: &BondState,
    c: &BondCommand,
    m: &mut Meter,
) -> Result<Result<BondPlan, Reason>, Failure> {
    let result = run(context, s, c, m)?;
    super::bond_laws::check(
        context,
        s,
        c,
        result.as_ref().map(|v| v.state()).map_err(|r| *r),
        m,
    )?;
    Ok(result)
}
fn run(
    context: AccountContext<'_>,
    s: &BondState,
    c: &BondCommand,
    m: &mut Meter,
) -> Result<Result<BondPlan, Reason>, Failure> {
    let AccountContext {
        parameters: p,
        actor,
        nonce,
        clock,
        capacity,
    } = context;
    p.validate()?;
    let timing = p.clock.as_ref().ok_or(Failure::UnboundParameter)?;
    let d = p.disputes.as_ref().ok_or(Failure::UnboundParameter)?;
    let economy = p.economy.as_ref().ok_or(Failure::UnboundParameter)?;
    validate(s, capacity, m)?;
    charge(s, m)?;
    let owner_command = matches!(
        c,
        BondCommand::Reserve { .. } | BondCommand::Unregister | BondCommand::Withdraw(_)
    );
    if owner_command && actor != &s.owner {
        return Ok(Err(Reason::Unauthorized));
    }
    if nonce != s.nonces.get(actor).copied().unwrap_or(0) {
        return Ok(Err(Reason::Replay));
    }
    let Some(next) = nonce.checked_add(1) else {
        return Ok(Err(Reason::CounterExhausted));
    };
    if clock.backend != timing.backend || clock.finality_root == [0; 32] {
        return Err(Failure::InvalidEvidence);
    }
    if clock.epoch < s.epoch {
        return Ok(Err(Reason::BadClockOrSync));
    }
    charge(s, m)?;
    let mut out = s.clone();
    let now = clock.epoch;
    let zero = matches!(c,BondCommand::Deposit(x)|BondCommand::Withdraw(x) if x.0==0);
    // Presence/phase precede the positive-amount guard, as frozen R05/R07/R12.
    let target = match c {
        BondCommand::RecordPublication { liability, .. }
        | BondCommand::Clear { liability }
        | BondCommand::OpenDispute { liability, .. } => Some(*liability),
        _ => None,
    };
    let target_index = target
        .map(|id| position(&s.liabilities, |l| l.id == id, m))
        .transpose()?
        .flatten();
    if target.is_some() && target_index.is_none() {
        return Ok(Err(Reason::UnknownReference));
    }
    match c {
        BondCommand::Reserve { id, .. } if s.liabilities.iter().any(|l| l.id == *id) => {
            return Ok(Err(Reason::AlreadyExists));
        }
        BondCommand::OpenDispute { id, .. } if s.disputes.iter().any(|x| x.id == *id) => {
            return Ok(Err(Reason::AlreadyExists));
        }
        _ => {}
    }
    if matches!(c, BondCommand::Deposit(_)) && s.phase == ReporterPhase::Retired
        || matches!(c, BondCommand::Reserve { .. } | BondCommand::Unregister)
            && s.phase != ReporterPhase::Active
        || matches!(c, BondCommand::Withdraw(_)) && s.phase == ReporterPhase::Active
    {
        return Ok(Err(Reason::WrongPhase));
    }
    if zero {
        return Ok(Err(Reason::ZeroAmountOrPrice));
    }
    match c {
        BondCommand::Deposit(amount) => {
            let balance = s.wallets.get(actor).copied().unwrap_or_default();
            if balance.0 < amount.0 {
                return Ok(Err(Reason::InsufficientBalance));
            }
            out.held.0 = out
                .held
                .0
                .checked_add(amount.0)
                .ok_or(Failure::Arithmetic)?;
            out.wallets
                .insert(actor.clone(), ZdexE18(balance.0 - amount.0));
        }
        BondCommand::Reserve { id, report, feed } => {
            if *id == [0; 32] || *report == [0; 32] || *feed == [0; 32] {
                return Err(Failure::InvalidEvidence);
            }
            let locked = locked(s, m)?;
            if s.held.0 - locked < d.minimum_bond.0 {
                return Ok(Err(Reason::InsufficientBond));
            }
            if out.liabilities.len() >= capacity as usize {
                return Ok(Err(Reason::Full));
            }
            let clear_after = now
                .checked_add(timing.max_age)
                .and_then(|x| x.checked_add(economy.window))
                .ok_or(Failure::Arithmetic)?;
            out.liabilities.push(Liability {
                id: *id,
                report: *report,
                feed: *feed,
                locked: d.minimum_bond,
                publication: None,
                exposure_until: clear_after,
                clear_after,
                cleared: false,
            });
        }
        BondCommand::RecordPublication { aggregate, .. } => {
            let l = &mut out.liabilities[target_index.ok_or(Failure::InvalidState)?];
            if l.publication.is_some() || l.cleared {
                return Ok(Err(Reason::WrongPhase));
            }
            if !aggregate.reports().contains(&l.report) || aggregate.feed() != l.feed {
                return Ok(Err(Reason::BindingMismatch));
            }
            l.publication = Some(now);
            let end = now
                .checked_add(timing.confirmation.max(timing.dispute_open))
                .and_then(|x| x.checked_add(d.finality_delay))
                .and_then(|x| x.checked_add(d.appeal_delay))
                .ok_or(Failure::Arithmetic)?;
            l.clear_after = l.clear_after.max(end);
        }
        BondCommand::Unregister => {
            out.phase = ReporterPhase::Unbonding;
            out.exit_epoch = Some(now);
        }
        BondCommand::Clear { .. } => {
            let index = target_index.ok_or(Failure::InvalidState)?;
            let l = &s.liabilities[index];
            if l.cleared {
                return Ok(Err(Reason::WrongPhase));
            }
            if now < l.clear_after
                || now < l.exposure_until
                || s.disputes
                    .iter()
                    .any(|x| x.liability == l.id && x.phase == DisputePhase::Open)
            {
                return Ok(Err(Reason::LiabilityLocked));
            }
            out.liabilities[index].locked = ZdexE18(0);
            out.liabilities[index].cleared = true;
        }
        BondCommand::Withdraw(amount) => {
            if amount.0 > s.held.0 - locked(s, m)? {
                return Ok(Err(Reason::InsufficientBond));
            }
            // The exit-delay sum is needed only by the later R26 comparison.
            let exit = s
                .exit_epoch
                .ok_or(Failure::InvalidState)?
                .checked_add(d.exit_delay)
                .ok_or(Failure::Arithmetic)?;
            if now < exit || s.disputes.iter().any(|x| x.phase == DisputePhase::Open) {
                return Ok(Err(Reason::LiabilityLocked));
            }
            out.held.0 -= amount.0;
            credit(&mut out.wallets, actor, amount.0)?;
            if out.held.0 == 0 && out.liabilities.iter().all(|x| x.cleared) {
                out.phase = ReporterPhase::Retired;
            }
        }
        BondCommand::OpenDispute { id, .. } => {
            let l = &s.liabilities[target_index.ok_or(Failure::InvalidState)?];
            let Some(published) = l.publication else {
                return Ok(Err(Reason::WrongPhase));
            };
            let balance = s.wallets.get(actor).copied().unwrap_or_default();
            if balance.0 < d.challenge_bond.0 {
                return Ok(Err(Reason::InsufficientBalance));
            }
            let until = published
                .checked_add(timing.dispute_open)
                .ok_or(Failure::Arithmetic)?;
            if now < published || now >= until {
                return Ok(Err(Reason::WindowClosed));
            }
            if l.cleared {
                return Ok(Err(Reason::LiabilityLocked));
            }
            if out.disputes.len() >= capacity as usize {
                return Ok(Err(Reason::Full));
            }
            out.wallets
                .insert(actor.clone(), ZdexE18(balance.0 - d.challenge_bond.0));
            out.disputes.push(Dispute {
                id: *id,
                liability: l.id,
                challenger: actor.clone(),
                escrow: d.challenge_bond,
                opened: now,
                phase: DisputePhase::Open,
                slash: ZdexE18(0),
                violation: None,
                slash_verdict: None,
                resolution_verdict: None,
            });
        }
        BondCommand::Slash { dispute, verdict } => {
            let Some(index) = position(&s.disputes, |x| x.id == *dispute, m)? else {
                return Ok(Err(Reason::UnknownReference));
            };
            let x = &s.disputes[index];
            if x.phase != DisputePhase::Open {
                return Ok(Err(Reason::WrongPhase));
            }
            let Some(li) = position(&s.liabilities, |l| l.id == x.liability, m)? else {
                return Err(Failure::InvalidState);
            };
            if s.liabilities[li].locked.0 < d.slash.0 {
                return Ok(Err(Reason::InsufficientBond));
            }
            if !valid_verdict(d, x, verdict, now) || !verdict.upheld {
                return Ok(Err(Reason::InvalidVerdict));
            }
            // One scheduled penalty per dispute: its recorded verdict and every
            // retained violation ID are permanent punishment nullifiers.
            if x.slash_verdict.is_some()
                || s.disputes
                    .iter()
                    .any(|other| other.violation == Some(verdict.violation))
            {
                return Ok(Err(Reason::AlreadyClaimedOrSlashed));
            }
            out.held.0 -= d.slash.0;
            out.liabilities[li].locked.0 -= d.slash.0;
            // The penalty leaves custody now, to the bound recipient or burn.
            // The dispute keeps only the applied amount as history.
            let challenger = x.challenger.clone();
            pay(
                &mut out,
                &d.slash_recipient,
                &challenger,
                &d.treasury,
                d.slash.0,
            )?;
            out.disputes[index].slash = d.slash;
            out.disputes[index].violation = Some(verdict.violation);
            out.disputes[index].slash_verdict = Some(verdict.clone());
        }
        BondCommand::Resolve { dispute, verdict } => {
            let Some(index) = position(&s.disputes, |x| x.id == *dispute, m)? else {
                return Ok(Err(Reason::UnknownReference));
            };
            let x = &s.disputes[index];
            if x.phase != DisputePhase::Open {
                return Ok(Err(Reason::WrongPhase));
            }
            if let Verdict::Final(v) = verdict {
                if !valid_verdict(d, x, v, now) {
                    return Ok(Err(Reason::InvalidVerdict));
                }
                if v.upheld && (x.slash != d.slash || x.violation != Some(v.violation)) {
                    return Ok(Err(Reason::LiabilityLocked));
                }
                if !v.upheld && x.slash.0 != 0 {
                    return Ok(Err(Reason::InvalidVerdict));
                }
                let challenger = x.challenger.clone();
                let amount = x.escrow.0;
                let recipient = if v.upheld {
                    &d.upheld_bond_recipient
                } else {
                    &d.rejected_bond_recipient
                };
                // Only the challenge escrow moves here; Slash already paid the
                // penalty.
                pay(&mut out, recipient, &challenger, &d.treasury, amount)?;
                out.disputes[index].escrow = ZdexE18(0);
                out.disputes[index].resolution_verdict = Some(v.clone());
                out.disputes[index].phase = if v.upheld {
                    DisputePhase::ResolvedUpheld
                } else {
                    DisputePhase::ResolvedRejected
                };
            }
            // Inconclusive deliberately leaves the dispute OPEN and funds locked.
        }
    }
    out.nonces.insert(actor.clone(), next);
    out.epoch = now;
    if [
        out.liabilities.len(),
        out.disputes.len(),
        out.wallets.len(),
        out.nonces.len(),
    ]
    .iter()
    .any(|n| *n > capacity as usize)
    {
        return Ok(Err(Reason::Full));
    }
    validate(&out, capacity, m)?;
    charge(&out, m)?;
    Ok(Ok(BondPlan { state: out }))
}
/// Checks equality/ordering of a supplied adjudication premise only. Nonzero
/// roots do not authenticate finality; no consumer may use this audit plan as
/// an adjudicator receipt. That closed integration gate is still unbound.
fn valid_verdict(d: &DisputeParameters, x: &Dispute, v: &FinalVerdict, now: u64) -> bool {
    v.adjudicator == d.adjudicator
        && v.dispute == x.id
        && v.violation != [0; 32]
        && v.finality_root != [0; 32]
        && v.final_epoch <= now
        && x.opened
            .checked_add(d.finality_delay)
            .and_then(|n| n.checked_add(d.appeal_delay))
            .is_some_and(|n| v.final_epoch >= n)
}
fn position<T>(
    xs: &[T],
    predicate: impl Fn(&T) -> bool,
    m: &mut Meter,
) -> Result<Option<usize>, Failure> {
    let mut found = None;
    for (i, x) in xs.iter().enumerate() {
        wire::visit(m)?;
        if predicate(x) {
            found = Some(i);
        }
    }
    Ok(found)
}
fn locked(s: &BondState, m: &mut Meter) -> Result<u128, Failure> {
    s.liabilities.iter().try_fold(0u128, |n, l| {
        wire::visit(m)?;
        n.checked_add(l.locked.0).ok_or(Failure::Arithmetic)
    })
}
fn credit(wallets: &mut BTreeMap<Id, ZdexE18>, id: &Id, amount: u128) -> Result<(), Failure> {
    let old = wallets.get(id).copied().unwrap_or_default().0;
    wallets.insert(
        id.clone(),
        ZdexE18(old.checked_add(amount).ok_or(Failure::Arithmetic)?),
    );
    Ok(())
}
fn pay(
    s: &mut BondState,
    recipient: &EscrowRecipient,
    challenger: &Id,
    treasury: &Id,
    amount: u128,
) -> Result<(), Failure> {
    match recipient {
        EscrowRecipient::Challenger => credit(&mut s.wallets, challenger, amount),
        EscrowRecipient::ReporterOwner => {
            let owner = s.owner.clone();
            credit(&mut s.wallets, &owner, amount)
        }
        EscrowRecipient::Treasury => credit(&mut s.wallets, treasury, amount),
        EscrowRecipient::Burn => {
            s.burned.0 = s.burned.0.checked_add(amount).ok_or(Failure::Arithmetic)?;
            Ok(())
        }
    }
}
pub(super) fn validate(s: &BondState, capacity: u32, m: &mut Meter) -> Result<(), Failure> {
    if [
        s.liabilities.len(),
        s.disputes.len(),
        s.wallets.len(),
        s.nonces.len(),
    ]
    .iter()
    .any(|n| *n > capacity as usize)
    {
        return Err(Failure::InvalidState);
    }
    let rows = s
        .liabilities
        .len()
        .checked_add(s.disputes.len())
        .and_then(|n| n.checked_add(s.wallets.len()))
        .and_then(|n| n.checked_add(s.nonces.len()))
        .ok_or(Failure::Arithmetic)?;
    wire::reserve_scans(rows, m)?;
    if locked(s, m)? > s.held.0 || (s.phase == ReporterPhase::Active) != s.exit_epoch.is_none() {
        return Err(Failure::InvalidState);
    }
    for (i, l) in s.liabilities.iter().enumerate() {
        wire::visit(m)?;
        if l.id == [0; 32]
            || l.report == [0; 32]
            || l.feed == [0; 32]
            || (l.cleared && l.locked.0 != 0)
            || s.liabilities[..i].iter().any(|x| x.id == l.id)
        {
            return Err(Failure::InvalidState);
        }
    }
    for (i, d) in s.disputes.iter().enumerate() {
        wire::visit(m)?;
        if !s.liabilities.iter().any(|l| l.id == d.liability)
            || s.disputes[..i].iter().any(|x| x.id == d.id)
        {
            return Err(Failure::InvalidState);
        }
        for v in [&d.slash_verdict, &d.resolution_verdict]
            .into_iter()
            .flatten()
        {
            if v.dispute != d.id
                || v.finality_root == [0; 32]
                || v.violation == [0; 32]
                || v.final_epoch > s.epoch
            {
                return Err(Failure::InvalidState);
            }
        }
        if d.slash.0 == 0 && (d.violation.is_some() || d.slash_verdict.is_some())
            || d.slash.0 > 0
                && !d
                    .slash_verdict
                    .as_ref()
                    .is_some_and(|v| v.upheld && d.violation == Some(v.violation))
            || d.phase == DisputePhase::Open && d.resolution_verdict.is_some()
            || d.phase == DisputePhase::ResolvedUpheld
                && !d
                    .resolution_verdict
                    .as_ref()
                    .is_some_and(|v| v.upheld && d.violation == Some(v.violation))
            || d.phase == DisputePhase::ResolvedRejected
                && !d.resolution_verdict.as_ref().is_some_and(|v| !v.upheld)
            || d.phase == DisputePhase::ResolvedRejected && d.slash.0 != 0
            || d.phase != DisputePhase::Open && d.escrow.0 != 0
        {
            return Err(Failure::InvalidState);
        }
    }
    Ok(())
}
fn charge(s: &BondState, m: &mut Meter) -> Result<(), Failure> {
    // Closed fixed-record upper bound; complete passes and each visited entry
    // are charged. This is a new relation tariff, not tables/1 CPU measurement.
    let rows = s
        .liabilities
        .len()
        .checked_add(s.disputes.len())
        .and_then(|n| n.checked_add(s.wallets.len()))
        .and_then(|n| n.checked_add(s.nonces.len()))
        .ok_or(Failure::Arithmetic)?;
    let bytes = u64::try_from(rows)
        .map_err(|_| Failure::Arithmetic)?
        .checked_mul(1024)
        .and_then(|n| n.checked_add(512))
        .ok_or(Failure::Arithmetic)?;
    m.charge(Resource::Byte, bytes)?;
    for _ in 0..rows {
        wire::visit(m)?;
    }
    Ok(())
}
