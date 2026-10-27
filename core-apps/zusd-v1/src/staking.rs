//! LQTYStaking 94..212: cumulative F with no feedback error or lock.
use crate::{Accumulator256, Evaluation, Failure, Id, Limits, Q, Work, add, run, sub, wide::Wide};
use alloc::vec::Vec;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    pub owner: Id,
    pub stake: u128,
    pub snapshot_z: Accumulator256,
    pub snapshot_a: Accumulator256,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub total: u128,
    pub f_z: Accumulator256,
    pub f_a: Accumulator256,
    pub custody_z: u128,
    pub custody_a: u128,
    pub zero_z: u128,
    pub zero_a: u128,
    pub positions: Vec<Position>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    Stake(u128),
    Unstake(u128),
    Claim,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub state: State,
    pub stake_in: u128,
    pub stake_out: u128,
    pub zusd_gain: u128,
    pub agrs_gain: u128,
}
fn gains(s: &State, p: Position, w: &mut Work) -> Result<(u128, u128), Failure> {
    Ok((
        Wide::from(s.f_z.sub(p.snapshot_z, w)?)
            .mul(p.stake, w)?
            .quotient(Wide::from(Q), w)?
            .0,
        Wide::from(s.f_a.sub(p.snapshot_a, w)?)
            .mul(p.stake, w)?
            .quotient(Wide::from(Q), w)?
            .0,
    ))
}
fn validate(s: &State, capacity: u32, w: &mut Work) -> Result<(), Failure> {
    if s.positions.len() > capacity as usize {
        return Err(Failure::Capacity);
    }
    w.charge(
        s.positions.len() as u64,
        s.positions.len() as u64,
        s.positions.len() as u64 * 128,
    )?;
    if s.positions.windows(2).any(|p| p[0].owner >= p[1].owner) {
        return Err(Failure::Canonical);
    }
    let mut total = Wide::from(0);
    let mut z = Wide::from(s.zero_z);
    let mut a = Wide::from(s.zero_a);
    for p in &s.positions {
        if p.stake == 0 {
            return Err(Failure::Domain);
        }
        total = total.add(Wide::from(p.stake), w)?;
        let (gz, ga) = gains(s, *p, w)?;
        z = z.add(Wide::from(gz), w)?;
        a = a.add(Wide::from(ga), w)?;
    }
    if total != Wide::from(s.total)
        || z.cmp(Wide::from(s.custody_z)).is_gt()
        || a.cmp(Wide::from(s.custody_a)).is_gt()
    {
        return Err(Failure::Conservation);
    }
    Ok(())
}
pub fn fee_event(
    pre: &State,
    zusd: u128,
    agrs: u128,
    capacity: u32,
    limits: Limits,
) -> Evaluation<State> {
    run(limits, |w| {
        validate(pre, capacity, w)?;
        w.charge(1, 0, pre.positions.len() as u64 * 128 + 160)?;
        let mut s = pre.clone();
        s.custody_z = add(s.custody_z, zusd, w)?;
        s.custody_a = add(s.custody_a, agrs, w)?;
        if s.total == 0 {
            s.zero_z = add(s.zero_z, zusd, w)?;
            s.zero_a = add(s.zero_a, agrs, w)?;
        } else {
            s.f_z = s.f_z.add(
                Wide::from(zusd)
                    .mul(Q, w)?
                    .quotient_wide(Wide::from(s.total), w)?
                    .0
                    .accumulator()?,
                w,
            )?;
            s.f_a = s.f_a.add(
                Wide::from(agrs)
                    .mul(Q, w)?
                    .quotient_wide(Wide::from(s.total), w)?
                    .0
                    .accumulator()?,
                w,
            )?;
        }
        validate(&s, capacity, w)?;
        Ok(s)
    })
}
pub fn change(
    pre: &State,
    actor: Id,
    command: Command,
    wallet_zdex: u128,
    capacity: u32,
    limits: Limits,
) -> Evaluation<Outcome> {
    run(limits, |w| {
        validate(pre, capacity, w)?;
        if command == Command::Stake(0) {
            return Err(Failure::Reject(1010));
        }
        let at = pre.positions.binary_search_by_key(&actor, |p| p.owner);
        let old = at.ok().map(|i| pre.positions[i]);
        if old.is_none() && !matches!(command, Command::Stake(_)) {
            return Err(Failure::Reject(1011));
        }
        let input = if let Command::Stake(x) = command {
            x
        } else {
            0
        };
        if input > wallet_zdex {
            return Err(Failure::Reject(1033));
        }
        let (z, a) = if let Some(p) = old {
            gains(pre, p, w)?
        } else {
            (0, 0)
        };
        let stake = old.map(|p| p.stake).unwrap_or(0);
        let output = if let Command::Unstake(x) = command {
            x.min(stake)
        } else {
            0
        };
        let next = sub(add(stake, input, w)?, output, w)?;
        w.charge(1, 0, pre.positions.len() as u64 * 128 + 160)?;
        let mut s = pre.clone();
        if let Ok(i) = at {
            s.positions.remove(i);
        }
        if next > 0 {
            if s.positions.len() >= capacity as usize {
                return Err(Failure::Reject(1061));
            }
            let i = s.positions.partition_point(|p| p.owner < actor);
            s.positions.insert(
                i,
                Position {
                    owner: actor,
                    stake: next,
                    snapshot_z: s.f_z,
                    snapshot_a: s.f_a,
                },
            );
        }
        s.total = sub(add(s.total, input, w)?, output, w)?;
        s.custody_z = sub(s.custody_z, z, w)?;
        s.custody_a = sub(s.custody_a, a, w)?;
        validate(&s, capacity, w)?;
        Ok(Outcome {
            state: s,
            stake_in: input,
            stake_out: output,
            zusd_gain: z,
            agrs_gain: a,
        })
    })
}
