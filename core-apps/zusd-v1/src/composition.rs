//! Pure paired transitions. No authenticated receipt or publication is created.
use crate::{
    Evaluation, Failure, Id, Limits, Work, fees, liquidation as l, redemption, run,
    stability as sp, sub, vaults as v,
};
use alloc::vec::Vec;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GainToVault {
    pub vaults: l::Book,
    pub stability: sp::Book,
    pub settlement: sp::Gains,
    pub front_end: Option<Id>,
    pub owner: v::Outcome,
}
/// Settles SP entitlement, applies BOTH pending rewards, adds the earned gain,
/// updates stake, and reinserts through the exact committed V1 cut, or returns
/// no paired successor. The pool custody and vault backing cannot diverge.
pub fn move_gain_to_vault(
    pre: &l::Book,
    pool: &sp::Book,
    actor: Id,
    params: v::Parameters,
    fee: fees::State,
    prices: v::Prices,
    issuance: Option<u128>,
    upper: Option<Id>,
    lower: Option<Id>,
    capacity: u32,
    bounds: sp::Bounds,
    limits: Limits,
) -> Evaluation<GainToVault> {
    run(limits, |w| {
        l::validate(pre, capacity, bounds, w)?;
        w.charge(1, 1, 0)?;
        if pre.pool != pool.pool {
            return Err(Failure::Conservation);
        }
        let claim = w.invoke(|limits| {
            sp::change(
                pool,
                actor,
                sp::Command::MoveGainToVault,
                0,
                issuance,
                prices,
                &pre.vaults,
                pre.system,
                bounds,
                limits,
            )
        })?;
        let at = pre
            .vaults
            .iter()
            .position(|row| row.owner == actor && row.status == v::Status::Active)
            .ok_or(Failure::Reject(1011))?;
        w.charge(pre.vaults.len() as u64, pre.vaults.len() as u64, 0)?;
        let gain = claim.gains.collateral;
        let owner = w.invoke(|limits| {
            v::adjust(
                pre.vaults[at],
                pre.system,
                params,
                fee,
                prices,
                v::Delta::Add(gain),
                v::Delta::None,
                (gain, 0),
                0,
                None,
                limits,
            )
        })?;
        w.charge(
            1,
            0,
            pre.vaults.len() as u64 * 768 + pre.order.len() as u64 * 96 + 512,
        )?;
        let mut book = pre.clone();
        book.vaults[at] = owner.vault;
        book.default_collateral = sub(pre.default_collateral, owner.pending_collateral, w)?;
        book.default_debt = sub(pre.default_debt, owner.pending_debt, w)?;
        book.system = owner.system;
        book.pool = claim.book.pool.clone();
        redemption::cut(&mut book, owner.vault.id, owner.nicr, upper, lower, w)?;
        refresh(&mut book, w)?;
        l::validate(&book, capacity, bounds, w)?;
        if claim.principal_in != 0
            || claim.principal_out != 0
            || !claim.gain_to_vault
            || owner.minted != 0
            || owner.burned != 0
            || owner.collateral_in != gain
            || owner.collateral_out != 0
        {
            return Err(Failure::Conservation);
        }
        Ok(GainToVault {
            vaults: book,
            stability: claim.book,
            settlement: claim.gains,
            front_end: claim.front_end,
            owner,
        })
    })
}
fn refresh(book: &mut l::Book, w: &mut Work) -> Result<(), Failure> {
    w.charge(
        book.vaults.len() as u64,
        book.vaults.len() as u64,
        book.vaults.len() as u64 * 48,
    )?;
    let mut keys = Vec::new();
    for row in &book.vaults {
        if row.status == v::Status::Active {
            keys.push((row.id, l::fresh(book, row.id, w)?));
        }
    }
    book.priorities = keys;
    Ok(())
}
