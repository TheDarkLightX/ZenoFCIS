//! Common native identity/replay and earned-claim relations. Actor authentication
//! and cross-component atomicity must be bound outside these pure calculations.
use crate::{Evaluation, Failure, Id, Limits, run, wide::Wide};
use alloc::vec::Vec;
pub const REASONS: &[u16] = &[
    1000, 1001, 1002, 1010, 1011, 1012, 1020, 1021, 1022, 1023, 1024, 1030, 1031, 1032, 1033, 1040,
    1041, 1042, 1043, 1044, 1045, 1050, 1051, 1052, 1053, 1060, 1061, 1062, 1063,
];
pub fn identity(
    actor: Id,
    owner: Option<Id>,
    nonce: u64,
    expected: u64,
    limits: Limits,
) -> Evaluation<u64> {
    run(limits, |w| {
        w.charge(1, 1, 80)?;
        if owner.is_some_and(|id| id != actor) {
            return Err(Failure::Reject(1000));
        }
        if nonce != expected {
            return Err(Failure::Reject(1001));
        }
        expected.checked_add(1).ok_or(Failure::Reject(1002))
    })
}
#[derive(Debug, Eq, PartialEq)]
pub struct SurplusClaim {
    pub remaining: Vec<(Id, u128)>,
    pub amount: u128,
}
pub fn claim_surplus(
    pre: &[(Id, u128)],
    owner: Id,
    capacity: u32,
    limits: Limits,
) -> Evaluation<SurplusClaim> {
    run(limits, |w| {
        if pre.len() > capacity as usize {
            return Err(Failure::Capacity);
        }
        w.charge(pre.len() as u64, pre.len() as u64, pre.len() as u64 * 96)?;
        if pre.windows(2).any(|x| x[0].0 >= x[1].0) {
            return Err(Failure::Canonical);
        }
        let at = pre
            .binary_search_by_key(&owner, |x| x.0)
            .map_err(|_| Failure::Reject(1045))?;
        let amount = pre[at].1;
        if amount == 0 {
            return Err(Failure::Reject(1045));
        }
        let mut remaining = pre.to_vec();
        remaining.remove(at);
        Ok(SurplusClaim { remaining, amount })
    })
}
/// Exact AGRS E18/E8 math only. This does not choose or authenticate a physical rail.
pub fn project_collateral(
    entitlement: u128,
    prior_remainder: u128,
    limits: Limits,
) -> Evaluation<(u128, u128)> {
    run(limits, |w| {
        if prior_remainder >= 10_000_000_000 {
            return Err(Failure::Domain);
        }
        let total = Wide::from(entitlement).add(Wide::from(prior_remainder), w)?;
        let (paid, remaining) = total.quotient(Wide::from(10_000_000_000), w)?;
        Ok((paid, remaining.narrow()?))
    })
}
