//! Complete products of the frozen accounting projections in finite-domains.json.
//! These are bounded execution/reference checks, not whole-product proofs.
use super::account_law_tests::{aggregate_fixture, clock, id, limits, params, rules};
use super::*;
use alloc::{collections::BTreeMap, vec, vec::Vec};
fn proof(upheld: bool) -> FinalVerdict {
    FinalVerdict {
        adjudicator: [6; 32],
        dispute: [5; 32],
        violation: [6; 32],
        upheld,
        final_epoch: 0,
        finality_root: [9; 32],
    }
}
fn state(
    template: u8,
    held: u128,
    wallet: u128,
    inactive: bool,
    clear: u64,
    exposure: u64,
) -> BondState {
    let mut s = BondState {
        reporter: id(1),
        owner: id(1),
        phase: if inactive {
            ReporterPhase::Unbonding
        } else {
            ReporterPhase::Active
        },
        held: ZdexE18(held),
        liabilities: vec![],
        disputes: vec![],
        wallets: BTreeMap::from([(id(1), ZdexE18(wallet)), (id(2), ZdexE18(wallet))]),
        nonces: BTreeMap::new(),
        burned: ZdexE18(0),
        exit_epoch: inactive.then_some(0),
        epoch: 0,
    };
    if template > 0 {
        s.liabilities.push(Liability {
            id: [4; 32],
            report: [1; 32],
            feed: [1; 32],
            locked: ZdexE18(if [3, 5, 7].contains(&template) {
                0
            } else {
                held
            }),
            publication: if template == 1 { None } else { Some(0) },
            exposure_until: exposure,
            clear_after: clear,
            cleared: template == 3,
        });
    }
    if template >= 4 {
        let slash = if template == 5 || template == 7 { 1 } else { 0 };
        let phase = match template {
            6 => DisputePhase::ResolvedRejected,
            7 => DisputePhase::ResolvedUpheld,
            _ => DisputePhase::Open,
        };
        s.disputes.push(Dispute {
            id: [5; 32],
            liability: [4; 32],
            challenger: id(2),
            escrow: ZdexE18(if phase == DisputePhase::Open { 1 } else { 0 }),
            opened: 0,
            phase,
            slash: ZdexE18(slash),
            violation: (slash == 1).then_some([6; 32]),
            slash_verdict: (slash == 1).then_some(proof(true)),
            resolution_verdict: match phase {
                DisputePhase::Open => None,
                DisputePhase::ResolvedRejected => Some(proof(false)),
                DisputePhase::ResolvedUpheld => Some(proof(true)),
            },
        });
    }
    s
}
fn total(s: &BondState) -> u128 {
    // A slash pays its recipient at once, so dispute.slash is history only.
    s.held.0
        + s.burned.0
        + s.wallets.values().map(|n| n.0).sum::<u128>()
        + s.disputes.iter().map(|d| d.escrow.0).sum::<u128>()
}
fn max_usage(out: &mut [u64; 3], used: Usage) {
    for (i, r) in [Resource::Step, Resource::Read, Resource::Byte]
        .into_iter()
        .enumerate()
    {
        out[i] = out[i].max(used.used(r));
    }
}
#[test]
fn oracle_finite_bond_all_command_phases_have_unique_exact_outcomes() {
    let a = aggregate_fixture([1; 32]);
    let mut foreign = a.clone();
    foreign.feed = [9; 32];
    let mut commands = Vec::new();
    for n in 0..=2 {
        commands.push(BondCommand::Deposit(ZdexE18(n)));
        commands.push(BondCommand::Withdraw(ZdexE18(n)));
    }
    commands.extend([
        BondCommand::Reserve {
            id: [7; 32],
            report: [2; 32],
            feed: [1; 32],
        },
        BondCommand::Reserve {
            id: [4; 32],
            report: [1; 32],
            feed: [1; 32],
        },
        BondCommand::RecordPublication {
            liability: [4; 32],
            aggregate: (a).into(),
        },
        BondCommand::RecordPublication {
            liability: [4; 32],
            aggregate: (foreign).into(),
        },
        BondCommand::Unregister,
        BondCommand::Clear { liability: [4; 32] },
        BondCommand::Clear {
            liability: [99; 32],
        },
        BondCommand::OpenDispute {
            id: [8; 32],
            liability: [4; 32],
        },
        BondCommand::OpenDispute {
            id: [5; 32],
            liability: [4; 32],
        },
        BondCommand::OpenDispute {
            id: [8; 32],
            liability: [99; 32],
        },
        BondCommand::Slash {
            dispute: [5; 32],
            verdict: proof(true),
        },
        BondCommand::Slash {
            dispute: [5; 32],
            verdict: proof(false),
        },
        BondCommand::Slash {
            dispute: [5; 32],
            verdict: FinalVerdict {
                adjudicator: [99; 32],
                ..proof(true)
            },
        },
        BondCommand::Slash {
            dispute: [99; 32],
            verdict: proof(true),
        },
        BondCommand::Resolve {
            dispute: [5; 32],
            verdict: Verdict::Inconclusive,
        },
        BondCommand::Resolve {
            dispute: [5; 32],
            verdict: Verdict::Final(proof(true)),
        },
        BondCommand::Resolve {
            dispute: [5; 32],
            verdict: Verdict::Final(proof(false)),
        },
        BondCommand::Resolve {
            dispute: [99; 32],
            verdict: Verdict::Inconclusive,
        },
    ]);
    assert_eq!(commands.len(), 24);
    let p = params();
    let mut count = 0;
    let mut accepts = 0u16;
    let mut worst = [0; 3];
    for t in 0..8 {
        for held in 0..=2 {
            for wallet in 0..=2 {
                for inactive in [false, true] {
                    for clear in 0..=4 {
                        for exposure in 0..=4 {
                            if t == 0 && (clear != 0 || exposure != 0) {
                                continue;
                            }
                            let s = state(t, held, wallet, inactive, clear, exposure);
                            let original = s.clone();
                            for actor in 1..=2 {
                                for epoch in 0..=4 {
                                    for nonce in 0..=4 {
                                        for c in &commands {
                                            let (out, used) = bond_relation(
                                                AccountContext {
                                                    parameters: &p,
                                                    actor: &id(actor),
                                                    nonce,
                                                    clock: clock(epoch),
                                                    capacity: 4,
                                                },
                                                &s,
                                                c,
                                                limits(),
                                            )
                                            .into_parts();
                                            max_usage(&mut worst, used);
                                            let out=out.unwrap_or_else(|e|panic!("undefined domain t={t} h={held} w={wallet} i={inactive} clear={clear} exp={exposure} actor={actor} ep={epoch} nonce={nonce} c={c:?}: {e:?}"));
                                            if let Ok(plan) = out {
                                                let k = match c {
                                                    BondCommand::Deposit(_) => 0,
                                                    BondCommand::Reserve { .. } => 1,
                                                    BondCommand::RecordPublication { .. } => 2,
                                                    BondCommand::Unregister => 3,
                                                    BondCommand::Clear { .. } => 4,
                                                    BondCommand::Withdraw(_) => 5,
                                                    BondCommand::OpenDispute { .. } => 6,
                                                    BondCommand::Slash { .. } => 7,
                                                    BondCommand::Resolve { .. } => 8,
                                                };
                                                accepts |= 1 << k;
                                                assert_eq!(total(&s), total(plan.state()));
                                            }
                                            count += 1;
                                        }
                                    }
                                }
                            }
                            assert_eq!(s, original);
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 3_801_600);
    assert_eq!(accepts, 511);
    std::println!(
        "oracle finite bonds: cases={count}, accepts_by_kind={accepts}, max_step_read_byte={worst:?}"
    );
}
#[test]
fn oracle_finite_two_beneficiary_rewards_all_phases_have_unique_exact_outcomes() {
    let p = params();
    let r = rules();
    let a = aggregate_fixture(r.feed);
    let ids = [
        super::rewards::claim_id(&r, a.id(), [1; 32], &id(1))
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
        super::rewards::claim_id(&r, a.id(), [2; 32], &id(2))
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
    ];
    let mut commands = vec![
        (RewardCommand::Fund(0), 0),
        (RewardCommand::Fund(1), 0),
        (RewardCommand::Fund(2), 0),
        (
            RewardCommand::Accrue {
                aggregate: (a.clone()).into(),
                report: [1; 32],
            },
            1,
        ),
        (
            RewardCommand::Accrue {
                aggregate: (a.clone()).into(),
                report: [2; 32],
            },
            2,
        ),
        (
            RewardCommand::Accrue {
                aggregate: (a.clone()).into(),
                report: [99; 32],
            },
            1,
        ),
    ];
    for (i, claim) in ids.into_iter().enumerate() {
        commands.push((RewardCommand::Claim(claim), i as u8 + 1));
        commands.push((RewardCommand::Clawback(claim), i as u8 + 1));
    }
    commands.extend([
        (RewardCommand::Claim([99; 32]), 1),
        (RewardCommand::Clawback([99; 32]), 1),
    ]);
    assert_eq!(commands.len(), 12);
    let mut count = 0;
    let mut accepts = 0u8;
    let mut worst = [0; 3];
    for phases in 0..16u8 {
        for remaining in 0..=2 {
            for w1 in 0..=2 {
                for w2 in 0..=2 {
                    for clear in [false, true] {
                        for dispute in 0..4 {
                            let mut s = RewardState {
                                feed: r.feed,
                                profile: r.profile,
                                unit: r.unit.clone(),
                                funded: 0,
                                remaining,
                                paid: 0,
                                treasury_paid: 0,
                                burned: 0,
                                residual: 0,
                                wallets: BTreeMap::from([(id(1), w1), (id(2), w2)]),
                                claims: vec![],
                                nonces: BTreeMap::new(),
                                epoch: 0,
                            };
                            let mut held = 0;
                            for (i, claim_id) in ids.iter().copied().enumerate() {
                                let phase = (phases >> (2 * i)) & 3;
                                if phase > 0 {
                                    let phase = match phase {
                                        1 => ClaimPhase::Held,
                                        2 => ClaimPhase::Paid,
                                        _ => ClaimPhase::ClawedBack,
                                    };
                                    held += u128::from(phase == ClaimPhase::Held);
                                    s.paid += u128::from(phase == ClaimPhase::Paid);
                                    s.claims.push(RewardClaim {
                                        id: claim_id,
                                        aggregate: a.id(),
                                        report: [i as u8 + 1; 32],
                                        liability: [i as u8 + 4; 32],
                                        beneficiary: id(i as u8 + 1),
                                        amount: 1,
                                        phase,
                                    });
                                }
                            }
                            s.funded = remaining + held + s.paid;
                            let original = s.clone();
                            for actor in 1..=2 {
                                for epoch in 0..=4 {
                                    for nonce in 0..=4 {
                                        for (c, reporter) in &commands {
                                            let who = (*reporter).max(1);
                                            let mut b = state(
                                                match dispute {
                                                    0 => 2,
                                                    1 => 4,
                                                    2 => 7,
                                                    _ => 6,
                                                },
                                                1,
                                                0,
                                                false,
                                                0,
                                                0,
                                            );
                                            b.reporter = id(who);
                                            b.owner = id(who);
                                            b.liabilities[0].id = [who + 3; 32];
                                            b.liabilities[0].report = [who; 32];
                                            b.liabilities[0].cleared = clear;
                                            if clear {
                                                b.liabilities[0].locked = ZdexE18(0);
                                            }
                                            for d in &mut b.disputes {
                                                d.liability = [who + 3; 32];
                                            }
                                            let (out, used) = reward_relation(
                                                &r,
                                                AccountContext {
                                                    parameters: &p,
                                                    actor: &id(actor),
                                                    nonce,
                                                    clock: clock(epoch),
                                                    capacity: 4,
                                                },
                                                &s,
                                                &b,
                                                c,
                                                limits(),
                                            )
                                            .into_parts();
                                            max_usage(&mut worst, used);
                                            let out=out.unwrap_or_else(|e|panic!("undefined reward phases={phases} remaining={remaining} clear={clear} dispute={dispute} c={c:?}: {e:?}"));
                                            if let Ok(plan) = out {
                                                accepts |= match c {
                                                    RewardCommand::Fund(_) => 1,
                                                    RewardCommand::Accrue { .. } => 2,
                                                    RewardCommand::Claim(_) => 4,
                                                    RewardCommand::Clawback(_) => 8,
                                                };
                                                let after = plan.state();
                                                assert_eq!(
                                                    s.wallets.values().sum::<u128>() + s.funded
                                                        - s.paid
                                                        - s.treasury_paid
                                                        - s.burned,
                                                    after.wallets.values().sum::<u128>()
                                                        + after.funded
                                                        - after.paid
                                                        - after.treasury_paid
                                                        - after.burned
                                                );
                                            }
                                            count += 1;
                                        }
                                    }
                                }
                            }
                            assert_eq!(s, original);
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 2_073_600);
    assert_eq!(accepts, 15);
    std::println!(
        "oracle finite rewards: cases={count}, accepts_by_kind={accepts}, max_step_read_byte={worst:?}"
    );
}
