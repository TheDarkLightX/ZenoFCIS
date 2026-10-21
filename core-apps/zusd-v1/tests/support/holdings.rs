//! Finite-fixture claim accounting; never a decision or custody authority.
use super::support::{balance, id};
use zenofcis_zusd_v1::{Accumulator256, Q, application::*, ledger::Asset, stability, vaults};
pub type Vector = [i128; 3]; // AGRS, zUSD (including vault equity), zDEX
fn scalar(a: Accumulator256) -> u128 {
    let (hi, lo) = a.limbs();
    assert_eq!(hi, 0, "finite fixture accumulator domain");
    lo
}
pub fn muldiv(a: u128, b: u128, d: u128) -> u128 {
    assert!(d > 0);
    let (mut q, mut r) = (0u128, 0u128);
    for bit in (0..128).rev() {
        r = r
            .checked_mul(2)
            .unwrap()
            .checked_add(if a >> bit & 1 == 1 { b } else { 0 })
            .unwrap();
        q = q.checked_mul(2).unwrap().checked_add(r / d).unwrap();
        r %= d;
    }
    q
}
fn gain(p: &stability::State, n: u128, snap: stability::Snapshot) -> (u128, u128, u128) {
    if n == 0 {
        return (0, 0, 0);
    }
    let first = p
        .history
        .iter()
        .find(|c| c.epoch == snap.epoch && c.scale == snap.scale)
        .unwrap();
    let next = p
        .history
        .iter()
        .find(|c| c.epoch == snap.epoch && c.scale == snap.scale + 1);
    let ds = scalar(first.s) - scalar(snap.s) + next.map_or(0, |c| scalar(c.s) / 1_000_000_000);
    let dg = scalar(first.g) - scalar(snap.g) + next.map_or(0, |c| scalar(c.g) / 1_000_000_000);
    let principal = if snap.epoch < p.epoch || p.scale - snap.scale >= 2 {
        0
    } else {
        let value = muldiv(
            n,
            p.p,
            snap.p
                * if p.scale == snap.scale {
                    1
                } else {
                    1_000_000_000
                },
        );
        if value < n / 1_000_000_000 { 0 } else { value }
    };
    (
        principal,
        muldiv(n, ds, snap.p * Q),
        muldiv(n, dg, snap.p * Q),
    )
}
/// Pool AGRS and token, staking AGRS/zUSD, and host tokens: current floors only.
pub fn rewards(s: &State, who: u8) -> Vector {
    let mut r = [0; 3];
    for d in s.pool().deposits.iter().filter(|d| d.owner == id(who)) {
        let (_, a, z) = gain(&s.pool().pool, d.initial, d.snapshot);
        let k = d.tag.map_or(Q, |tag| {
            s.pool()
                .front_ends
                .iter()
                .find(|f| f.owner == tag)
                .unwrap()
                .kickback
        });
        r[0] += i128::try_from(a).unwrap();
        r[2] += i128::try_from(muldiv(z, k, Q)).unwrap();
    }
    for f in s.pool().front_ends.iter().filter(|f| f.owner == id(who)) {
        let first = s
            .pool()
            .pool
            .history
            .iter()
            .find(|c| c.epoch == f.snapshot.epoch && c.scale == f.snapshot.scale)
            .unwrap();
        let snap = stability::Snapshot {
            p: f.snapshot.p,
            s: first.s,
            g: f.snapshot.g,
            epoch: f.snapshot.epoch,
            scale: f.snapshot.scale,
        };
        let (_, _, z) = gain(&s.pool().pool, f.initial, snap);
        r[2] += i128::try_from(muldiv(z, Q - f.kickback, Q)).unwrap();
    }
    for v in s.staking().positions.iter().filter(|v| v.owner == id(who)) {
        r[0] += i128::try_from(muldiv(
            v.stake,
            scalar(s.staking().f_a) - scalar(v.snapshot_a),
            Q,
        ))
        .unwrap();
        r[1] += i128::try_from(muldiv(
            v.stake,
            scalar(s.staking().f_z) - scalar(v.snapshot_z),
            Q,
        ))
        .unwrap();
    }
    r
}
pub fn holdings(s: &State, who: u8) -> Vector {
    let mut h = rewards(s, who);
    for (i, a) in [Asset::Agrs, Asset::Zusd, Asset::Zdex]
        .into_iter()
        .enumerate()
    {
        h[i] += i128::try_from(balance(s, who, a)).unwrap();
    }
    for v in s
        .book()
        .vaults
        .iter()
        .filter(|v| v.owner == id(who) && v.status == vaults::Status::Active)
    {
        let c = v.collateral
            + muldiv(
                v.stake,
                scalar(s.book().system.reward_c) - scalar(v.snapshot_c),
                Q,
            );
        let d = v.debt
            + muldiv(
                v.stake,
                scalar(s.book().system.reward_d) - scalar(v.snapshot_d),
                Q,
            );
        let price = s.oracle().unwrap().committed.unwrap().value_e8;
        h[1] += i128::try_from(muldiv(c, price, 100_000_000).saturating_sub(d)).unwrap();
        // Preserve the law's separately counted recorded gas escrow, once.
        h[1] += i128::try_from(v.reserve).unwrap();
    }
    for (owner, n) in &s.book().surplus {
        if *owner == id(who) {
            h[0] += i128::try_from(*n).unwrap();
        }
    }
    for d in s.pool().deposits.iter().filter(|d| d.owner == id(who)) {
        h[1] += i128::try_from(gain(&s.pool().pool, d.initial, d.snapshot).0).unwrap();
    }
    for v in s.staking().positions.iter().filter(|v| v.owner == id(who)) {
        h[2] += i128::try_from(v.stake).unwrap();
    }
    h
}
pub fn dominates(after: Vector, before: Vector) -> bool {
    (0..3).all(|i| after[i] >= before[i]) && (0..3).any(|i| after[i] > before[i])
}
