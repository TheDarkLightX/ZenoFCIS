//! Independent complete bond-account relation. No producer dispatch is called.
//! Guards define the ordered failure set; a separate exact successor defines
//! all permitted writes, including the unchanged frame and escrow ownership.
use super::bonds::AccountContext;
use super::bonds::{
    BondCommand, BondState, Dispute, DisputePhase, FinalVerdict, Liability, ReporterPhase, Verdict,
};
use super::prices::ClockView;
use super::types::{DisputeParameters, EscrowRecipient, Id, Parameters, REASONS, Reason, ZdexE18};
use super::wire;
use super::{Evaluation, Failure, Limits, Meter, measured};
use alloc::collections::BTreeMap;

/// Check the exact bonded-account outcome and ordered refusal independently.
pub fn check_bond_laws(
    context: AccountContext<'_>,
    s: &BondState,
    c: &BondCommand,
    proposed: Result<&BondState, Reason>,
    limits: Limits,
) -> Evaluation<()> {
    measured(limits, |m| check(context, s, c, proposed, m))
}
pub(super) fn check(
    context: AccountContext<'_>,
    s: &BondState,
    c: &BondCommand,
    proposed: Result<&BondState, Reason>,
    m: &mut Meter,
) -> Result<(), Failure> {
    let AccountContext {
        parameters: p,
        actor,
        nonce,
        clock,
        capacity,
    } = context;
    super::bonds::validate(s, capacity, m)?;
    let rows = s.wallets.len() + s.nonces.len() + s.liabilities.len() + s.disputes.len();
    wire::reserve_scans(rows.checked_add(8).ok_or(Failure::Arithmetic)?, m)?;
    if proposed.as_ref().is_ok_and(|x| {
        [
            x.wallets.len(),
            x.nonces.len(),
            x.liabilities.len(),
            x.disputes.len(),
        ]
        .iter()
        .any(|n| *n > capacity as usize)
    }) {
        return Err(Failure::Law(113_001));
    }
    // Conservation is checked apart from both successor computations, so a
    // shared transcription error cannot create or destroy zDEX.
    if proposed.is_ok_and(|post| custody(post) != custody(s)) {
        return Err(Failure::Law(113_400));
    }
    let expected = reference(p, s, actor, nonce, c, clock, capacity)?;
    match (&expected, proposed) {
        (Ok(want), Ok(actual)) if want == actual => Ok(()),
        (Err(want), Err(actual)) if *want == actual => Ok(()),
        (Ok(_), Err(r)) => Err(Failure::Law(113_100 + r as u32)),
        (Err(_), Ok(_)) => Err(Failure::Law(113_200)),
        _ => Err(Failure::Law(113_201)),
    }
}
/// Exact (carry, low) zDEX custody: wallets, held bond, open challenge escrow
/// and declared burns. An applied penalty is history, not custody.
fn custody(s: &BondState) -> (u128, u128) {
    s.wallets
        .values()
        .map(|w| w.0)
        .chain([s.held.0, s.burned.0])
        .chain(s.disputes.iter().map(|d| d.escrow.0))
        .fold((0, 0), |(carry, low), n| {
            let (sum, over) = low.overflowing_add(n);
            (carry + u128::from(over), sum)
        })
}
fn first(f: &[bool; 32]) -> Option<Reason> {
    f.iter().position(|x| *x).map(|i| REASONS[i])
}
/// A fallible later predicate is evaluated only when every earlier one is
/// false; its arithmetic cannot mask an already decided reason.
fn undecided(f: &[bool; 32], index: usize) -> bool {
    !f[..index].iter().any(|x| *x)
}
fn add(a: u128, b: u128) -> Result<u128, Failure> {
    a.checked_add(b).ok_or(Failure::Arithmetic)
}
fn credit(w: &mut BTreeMap<Id, ZdexE18>, id: &Id, amount: u128) -> Result<(), Failure> {
    w.insert(
        id.clone(),
        ZdexE18(add(w.get(id).copied().unwrap_or_default().0, amount)?),
    );
    Ok(())
}
fn pay(
    s: &mut BondState,
    r: &EscrowRecipient,
    challenger: &Id,
    treasury: &Id,
    amount: u128,
) -> Result<(), Failure> {
    match r {
        EscrowRecipient::Burn => s.burned.0 = add(s.burned.0, amount)?,
        EscrowRecipient::Challenger => credit(&mut s.wallets, challenger, amount)?,
        EscrowRecipient::ReporterOwner => {
            let owner = s.owner.clone();
            credit(&mut s.wallets, &owner, amount)?;
        }
        EscrowRecipient::Treasury => credit(&mut s.wallets, treasury, amount)?,
    }
    Ok(())
}
fn verdict(d: &DisputeParameters, x: &Dispute, v: &FinalVerdict, now: u64) -> bool {
    let end = x
        .opened
        .checked_add(d.finality_delay)
        .and_then(|a| a.checked_add(d.appeal_delay));
    v.adjudicator == d.adjudicator
        && v.dispute == x.id
        && v.violation != [0; 32]
        && v.finality_root != [0; 32]
        && v.final_epoch <= now
        && end.is_some_and(|t| v.final_epoch >= t)
}
pub(super) fn reference(
    p: &Parameters,
    s: &BondState,
    actor: &Id,
    nonce: u64,
    c: &BondCommand,
    clock: ClockView,
    capacity: u32,
) -> Result<Result<BondState, Reason>, Failure> {
    p.validate()?;
    let timing = p.clock.as_ref().ok_or(Failure::UnboundParameter)?;
    let d = p.disputes.as_ref().ok_or(Failure::UnboundParameter)?;
    let e = p.economy.as_ref().ok_or(Failure::UnboundParameter)?;
    if clock.backend != timing.backend || clock.finality_root == [0; 32] {
        return Err(Failure::InvalidEvidence);
    }
    let mut f = [false; 32];
    f[0] = actor != &s.owner
        && matches!(
            c,
            BondCommand::Reserve { .. } | BondCommand::Unregister | BondCommand::Withdraw(_)
        );
    f[1] = nonce != s.nonces.get(actor).copied().unwrap_or(0);
    f[2] = nonce == u64::MAX;
    f[3] = clock.epoch < s.epoch;
    let l = match c {
        BondCommand::Clear { liability }
        | BondCommand::RecordPublication { liability, .. }
        | BondCommand::OpenDispute { liability, .. } => {
            s.liabilities.iter().find(|x| x.id == *liability)
        }
        _ => None,
    };
    let x = match c {
        BondCommand::Slash { dispute, .. } | BondCommand::Resolve { dispute, .. } => {
            s.disputes.iter().find(|x| x.id == *dispute)
        }
        _ => None,
    };
    f[4] = matches!(
        c,
        BondCommand::Clear { .. }
            | BondCommand::RecordPublication { .. }
            | BondCommand::OpenDispute { .. }
    ) && l.is_none()
        || matches!(c, BondCommand::Slash { .. } | BondCommand::Resolve { .. }) && x.is_none();
    f[5] = match c {
        BondCommand::Reserve { id, .. } => s.liabilities.iter().any(|x| x.id == *id),
        BondCommand::OpenDispute { id, .. } => s.disputes.iter().any(|x| x.id == *id),
        _ => false,
    };
    f[6] = matches!(c, BondCommand::Deposit(_)) && s.phase == ReporterPhase::Retired
        || matches!(c, BondCommand::Reserve { .. } | BondCommand::Unregister)
            && s.phase != ReporterPhase::Active
        || matches!(c, BondCommand::Withdraw(_)) && s.phase == ReporterPhase::Active;
    f[11] = matches!(c,BondCommand::Deposit(a)|BondCommand::Withdraw(a) if a.0==0);
    let locked = s
        .liabilities
        .iter()
        .try_fold(0u128, |a, l| add(a, l.locked.0))?;
    let free = s.held.0.checked_sub(locked).ok_or(Failure::InvalidState)?;
    match c {
        BondCommand::Deposit(a) => {
            f[17] = s.wallets.get(actor).copied().unwrap_or_default().0 < a.0
        }
        BondCommand::Reserve { .. } => f[16] = free < d.minimum_bond.0,
        BondCommand::Withdraw(a) => {
            f[16] = a.0 > free;
            if let Some(exit) = s.exit_epoch {
                f[25] = undecided(&f, 25)
                    && (clock.epoch < exit.checked_add(d.exit_delay).ok_or(Failure::Arithmetic)?
                        || s.disputes.iter().any(|x| x.phase == DisputePhase::Open));
            }
        }
        BondCommand::RecordPublication { aggregate, .. } => {
            if let Some(l) = l {
                f[6] = l.publication.is_some() || l.cleared;
                f[9] = !aggregate.reports().contains(&l.report) || aggregate.feed() != l.feed;
            }
        }
        BondCommand::Clear { .. } => {
            if let Some(l) = l {
                f[6] = l.cleared;
                f[25] = clock.epoch < l.clear_after
                    || clock.epoch < l.exposure_until
                    || s.disputes
                        .iter()
                        .any(|x| x.liability == l.id && x.phase == DisputePhase::Open);
            }
        }
        BondCommand::OpenDispute { .. } => {
            if let Some(l) = l {
                f[6] = l.publication.is_none();
                f[17] = s.wallets.get(actor).copied().unwrap_or_default().0 < d.challenge_bond.0;
                if let Some(published) = l.publication {
                    f[20] = undecided(&f, 20)
                        && (clock.epoch < published
                            || clock.epoch
                                >= published
                                    .checked_add(timing.dispute_open)
                                    .ok_or(Failure::Arithmetic)?);
                }
                f[25] = l.cleared;
            }
        }
        BondCommand::Slash { verdict: v, .. } => {
            if let Some(x) = x {
                f[6] = x.phase != DisputePhase::Open;
                let liability = s
                    .liabilities
                    .iter()
                    .find(|l| l.id == x.liability)
                    .ok_or(Failure::InvalidState)?;
                f[16] = liability.locked.0 < d.slash.0;
                f[24] = !verdict(d, x, v, clock.epoch) || !v.upheld;
                // Both the dispute's own applied penalty and any retained
                // violation ID are consumed nullifiers.
                f[27] = x.violation.is_some()
                    || s.disputes
                        .iter()
                        .any(|other| other.violation == Some(v.violation));
            }
        }
        BondCommand::Resolve {
            verdict: resolution,
            ..
        } => {
            if let Some(x) = x {
                f[6] = x.phase != DisputePhase::Open;
                if let Verdict::Final(v) = resolution {
                    f[24] = !verdict(d, x, v, clock.epoch) || !v.upheld && x.slash.0 != 0;
                    f[25] = v.upheld && (x.slash != d.slash || x.violation != Some(v.violation));
                }
            }
        }
        _ => {}
    }
    f[30] = matches!(c, BondCommand::Reserve { .. }) && s.liabilities.len() >= capacity as usize
        || matches!(c, BondCommand::OpenDispute { .. }) && s.disputes.len() >= capacity as usize;
    // Every reason is covered by the first-true predicate order. A lawful
    // accept exists exactly when that ordered set is empty.
    if let Some(r) = first(&f) {
        return Ok(Err(r));
    }
    let mut out = s.clone();
    match c {
        BondCommand::Deposit(a) => {
            out.held.0 = add(s.held.0, a.0)?;
            out.wallets.insert(
                actor.clone(),
                ZdexE18(s.wallets.get(actor).copied().unwrap_or_default().0 - a.0),
            );
        }
        BondCommand::Reserve { id, report, feed } => {
            if [*id, *report, *feed].contains(&[0; 32]) {
                return Err(Failure::InvalidEvidence);
            }
            let end = clock
                .epoch
                .checked_add(timing.max_age)
                .and_then(|v| v.checked_add(e.window))
                .ok_or(Failure::Arithmetic)?;
            out.liabilities.push(Liability {
                id: *id,
                report: *report,
                feed: *feed,
                locked: d.minimum_bond,
                publication: None,
                exposure_until: end,
                clear_after: end,
                cleared: false,
            });
        }
        BondCommand::RecordPublication { liability, .. } => {
            let end = clock
                .epoch
                .checked_add(timing.confirmation.max(timing.dispute_open))
                .and_then(|v| v.checked_add(d.finality_delay))
                .and_then(|v| v.checked_add(d.appeal_delay))
                .ok_or(Failure::Arithmetic)?;
            let row = out
                .liabilities
                .iter_mut()
                .find(|x| x.id == *liability)
                .ok_or(Failure::InvalidState)?;
            row.publication = Some(clock.epoch);
            row.clear_after = row.clear_after.max(end);
        }
        BondCommand::Unregister => {
            out.phase = ReporterPhase::Unbonding;
            out.exit_epoch = Some(clock.epoch);
        }
        BondCommand::Clear { liability } => {
            let row = out
                .liabilities
                .iter_mut()
                .find(|x| x.id == *liability)
                .ok_or(Failure::InvalidState)?;
            row.locked = ZdexE18(0);
            row.cleared = true;
        }
        BondCommand::Withdraw(a) => {
            out.held.0 = s.held.0 - a.0;
            credit(&mut out.wallets, actor, a.0)?;
            if out.held.0 == 0 && out.liabilities.iter().all(|x| x.cleared) {
                out.phase = ReporterPhase::Retired;
            }
        }
        BondCommand::OpenDispute { id, liability } => {
            out.wallets.insert(
                actor.clone(),
                ZdexE18(s.wallets.get(actor).copied().unwrap_or_default().0 - d.challenge_bond.0),
            );
            out.disputes.push(Dispute {
                id: *id,
                liability: *liability,
                challenger: actor.clone(),
                escrow: d.challenge_bond,
                opened: clock.epoch,
                phase: DisputePhase::Open,
                slash: ZdexE18(0),
                violation: None,
                slash_verdict: None,
                resolution_verdict: None,
            });
        }
        BondCommand::Slash {
            dispute,
            verdict: v,
        } => {
            let x = out
                .disputes
                .iter_mut()
                .find(|x| x.id == *dispute)
                .ok_or(Failure::InvalidState)?;
            x.slash = d.slash;
            x.violation = Some(v.violation);
            x.slash_verdict = Some(v.clone());
            let (li, challenger) = (x.liability, x.challenger.clone());
            out.liabilities
                .iter_mut()
                .find(|l| l.id == li)
                .ok_or(Failure::InvalidState)?
                .locked
                .0 -= d.slash.0;
            out.held.0 -= d.slash.0;
            // The scheduled penalty reaches its recipient in this successor.
            pay(
                &mut out,
                &d.slash_recipient,
                &challenger,
                &d.treasury,
                d.slash.0,
            )?;
        }
        BondCommand::Resolve {
            dispute,
            verdict: resolution,
        } => {
            if let Verdict::Final(v) = resolution {
                let old = s
                    .disputes
                    .iter()
                    .find(|x| x.id == *dispute)
                    .ok_or(Failure::InvalidState)?;
                let r = if v.upheld {
                    &d.upheld_bond_recipient
                } else {
                    &d.rejected_bond_recipient
                };
                pay(&mut out, r, &old.challenger, &d.treasury, old.escrow.0)?;
                let x = out
                    .disputes
                    .iter_mut()
                    .find(|x| x.id == *dispute)
                    .ok_or(Failure::InvalidState)?;
                x.escrow = ZdexE18(0);
                x.resolution_verdict = Some(v.clone());
                x.phase = if v.upheld {
                    DisputePhase::ResolvedUpheld
                } else {
                    DisputePhase::ResolvedRejected
                };
            }
        }
    }
    out.nonces.insert(
        actor.clone(),
        nonce.checked_add(1).ok_or(Failure::Arithmetic)?,
    );
    out.epoch = clock.epoch;
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
    Ok(Ok(out))
}
