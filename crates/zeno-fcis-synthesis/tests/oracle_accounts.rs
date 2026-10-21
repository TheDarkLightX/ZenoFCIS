//! Native oracle arithmetic, accounting and price regressions.
use std::collections::BTreeMap;
use zeno_fcis_synthesis::finite::v2_oracle::*;

fn id(n: u8) -> Id {
    Id::new(&[n]).unwrap_or_else(|_| panic!("test ID"))
}
fn limits() -> Limits {
    zero_limits()
        .with_limit(Resource::Step, 1_000_000)
        .with_limit(Resource::Read, 1_000_000)
        .with_limit(Resource::Byte, 10_000_000)
        .with_limit(Resource::Write, 1_000_000)
}
fn parameters() -> Parameters {
    Parameters {
        clock: Some(ClockParameters {
            backend: [5; 32],
            heartbeat: 1,
            confirmation: 1,
            dispute_open: 2,
            inclusion: 0,
            max_age: 4,
            max_epoch_seconds: 1,
            max_time_lag: 2,
        }),
        disputes: Some(DisputeParameters {
            adjudicator: [6; 32],
            minimum_bond: ZdexE18(1),
            challenge_bond: ZdexE18(1),
            slash: ZdexE18(1),
            exit_delay: 1,
            finality_delay: 0,
            appeal_delay: 0,
            false_price_slashable: true,
            treasury: id(8),
            slash_recipient: EscrowRecipient::Challenger,
            upheld_bond_recipient: EscrowRecipient::Challenger,
            rejected_bond_recipient: EscrowRecipient::ReporterOwner,
        }),
        economy: Some(EconomicParameters {
            reward_unit: RewardUnit {
                asset: id(9),
                scale: 18,
            },
            reward_per_report: 1,
            alpha_numerator: 1,
            alpha_denominator: 2,
            stress_source: [7; 32],
            stress_haircut_numerator: 1,
            stress_haircut_denominator: 1,
            debt_ceiling: UsdExposureE18(0),
            liquidation_cap: UsdExposureE18(0),
            redemption_cap: UsdExposureE18(0),
            other_cap: UsdExposureE18(0),
            window: 4,
        }),
    }
}
fn clock(epoch: u64) -> ClockView {
    ClockView {
        backend: [5; 32],
        finality_root: [9; 32],
        epoch,
    }
}
fn initial() -> BondState {
    BondState {
        reporter: id(1),
        owner: id(1),
        phase: ReporterPhase::Active,
        held: ZdexE18(0),
        liabilities: vec![],
        disputes: vec![],
        wallets: BTreeMap::from([(id(1), ZdexE18(10)), (id(2), ZdexE18(10))]),
        nonces: BTreeMap::new(),
        burned: ZdexE18(0),
        exit_epoch: None,
        epoch: 0,
    }
}
fn apply(s: &BondState, actor: u8, c: BondCommand, epoch: u64) -> BondState {
    bond_relation(
        AccountContext {
            parameters: &parameters(),
            actor: &id(actor),
            nonce: s.nonces.get(&id(actor)).copied().unwrap_or(0),
            clock: clock(epoch),
            capacity: 16,
        },
        s,
        &c,
        limits(),
    )
    .into_parts()
    .0
    .unwrap_or_else(|e| panic!("refusal {e:?}"))
    .unwrap_or_else(|e| panic!("reject {e:?}"))
    .into_state()
}
// An applied slash has already left custody; dispute.slash is history only.
fn total(s: &BondState) -> u128 {
    s.held.0
        + s.wallets.values().map(|x| x.0).sum::<u128>()
        + s.burned.0
        + s.disputes.iter().map(|d| d.escrow.0).sum::<u128>()
}

#[test]
fn oracle_bonds_have_exact_deposits_locks_joint_clearance_and_exit() {
    let before = initial();
    let mut s = apply(&before, 1, BondCommand::Deposit(ZdexE18(10)), 0);
    s = apply(
        &s,
        1,
        BondCommand::Reserve {
            id: [1; 32],
            report: [2; 32],
            feed: [3; 32],
        },
        0,
    );
    assert_eq!(s.held, ZdexE18(10));
    assert_eq!(s.liabilities[0].locked, ZdexE18(1));
    s = apply(&s, 1, BondCommand::Unregister, 0);
    let held = bond_relation(
        AccountContext {
            parameters: &parameters(),
            actor: &id(1),
            nonce: s.nonces[&id(1)],
            clock: clock(1),
            capacity: 16,
        },
        &s,
        &BondCommand::Withdraw(ZdexE18(10)),
        limits(),
    );
    assert_eq!(
        held.result().map(|x| x.as_ref().map(|_| ())),
        Ok(Err(&Reason::InsufficientBond))
    );
    s = apply(&s, 2, BondCommand::Clear { liability: [1; 32] }, 8);
    assert!(s.liabilities[0].cleared);
    assert_eq!(s.liabilities[0].locked, ZdexE18(0));
    s = apply(&s, 1, BondCommand::Withdraw(ZdexE18(10)), 8);
    assert_eq!(s.phase, ReporterPhase::Retired);
    assert_eq!(s.wallets[&id(1)], ZdexE18(10));
    assert_eq!(total(&s), total(&before));
    assert_eq!(before.held, ZdexE18(0));
}

#[test]
fn oracle_inconclusive_dispute_stays_open_and_never_unlocks_on_timeout() {
    // A directly admitted boundary state isolates the dispute end condition;
    // this is not a signature-authenticated genesis journey.
    let mut s = initial();
    s.held = ZdexE18(1);
    s.wallets.insert(id(1), ZdexE18(9));
    s.liabilities.push(Liability {
        id: [1; 32],
        report: [2; 32],
        feed: [3; 32],
        locked: ZdexE18(1),
        publication: Some(0),
        exposure_until: 8,
        clear_after: 8,
        cleared: false,
    });
    s = apply(
        &s,
        2,
        BondCommand::OpenDispute {
            id: [4; 32],
            liability: [1; 32],
        },
        0,
    );
    s = apply(
        &s,
        2,
        BondCommand::Resolve {
            dispute: [4; 32],
            verdict: Verdict::Inconclusive,
        },
        1000,
    );
    assert_eq!(s.disputes[0].phase, DisputePhase::Open);
    assert_eq!(s.disputes[0].escrow, ZdexE18(1));
    let out = bond_relation(
        AccountContext {
            parameters: &parameters(),
            actor: &id(1),
            nonce: 0,
            clock: clock(1000),
            capacity: 16,
        },
        &s,
        &BondCommand::Clear { liability: [1; 32] },
        limits(),
    );
    assert_eq!(
        out.result().map(|x| x.as_ref().map(|_| ())),
        Ok(Err(&Reason::LiabilityLocked))
    );
}

#[test]
fn oracle_upheld_slash_and_rejected_challenge_have_exact_existing_value_dispositions() {
    for upheld in [true, false] {
        let mut s = initial();
        s.held = ZdexE18(1);
        s.wallets.insert(id(1), ZdexE18(9));
        s.liabilities.push(Liability {
            id: [1; 32],
            report: [2; 32],
            feed: [3; 32],
            locked: ZdexE18(1),
            publication: Some(0),
            exposure_until: 8,
            clear_after: 8,
            cleared: false,
        });
        let original = total(&s);
        s = apply(
            &s,
            2,
            BondCommand::OpenDispute {
                id: [4; 32],
                liability: [1; 32],
            },
            0,
        );
        let verdict = FinalVerdict {
            adjudicator: [6; 32],
            dispute: [4; 32],
            violation: [5; 32],
            upheld,
            final_epoch: 0,
            finality_root: [9; 32],
        };
        if upheld {
            s = apply(
                &s,
                2,
                BondCommand::Slash {
                    dispute: [4; 32],
                    verdict: verdict.clone(),
                },
                0,
            );
        }
        s = apply(
            &s,
            2,
            BondCommand::Resolve {
                dispute: [4; 32],
                verdict: Verdict::Final(verdict),
            },
            0,
        );
        assert_eq!(total(&s), original);
        assert_eq!(s.disputes[0].escrow, ZdexE18(0));
        if upheld {
            assert_eq!(s.wallets[&id(2)], ZdexE18(11));
            assert_eq!(s.held, ZdexE18(0));
        } else {
            assert_eq!(s.wallets[&id(1)], ZdexE18(10));
            assert_eq!(s.wallets[&id(2)], ZdexE18(9));
        }
    }
}

#[test]
fn oracle_reward_funding_never_creates_value_and_keeps_rounding_residuals_owned() {
    let p = parameters();
    let bond = initial();
    let unit = p
        .economy
        .as_ref()
        .unwrap_or_else(|| panic!("parameters"))
        .reward_unit
        .clone();
    let rules = RewardRules {
        feed: [3; 32],
        profile: [2; 32],
        unit: unit.clone(),
        per_report: 1,
        reporter_share: 1,
        treasury_share: 1,
        burn_share: 1,
        denominator: 3,
        treasury: id(8),
    };
    for amount in 1..=10 {
        let s = RewardState {
            feed: rules.feed,
            profile: rules.profile,
            unit: unit.clone(),
            funded: 0,
            remaining: 0,
            paid: 0,
            treasury_paid: 0,
            burned: 0,
            residual: 0,
            wallets: BTreeMap::from([(id(1), 10)]),
            claims: vec![],
            nonces: BTreeMap::new(),
            epoch: 0,
        };
        let out = reward_relation(
            &rules,
            AccountContext {
                parameters: &p,
                actor: &id(1),
                nonce: 0,
                clock: clock(0),
                capacity: 16,
            },
            &s,
            &bond,
            &RewardCommand::Fund(amount),
            limits(),
        )
        .into_parts()
        .0
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|e| panic!("{e:?}"))
        .into_state();
        assert_eq!(out.remaining, amount / 3);
        assert_eq!(out.treasury_paid, amount / 3);
        assert_eq!(out.burned, amount / 3);
        assert_eq!(out.residual, amount % 3);
        assert_eq!(
            out.wallets.values().sum::<u128>() + out.remaining + out.residual + out.burned,
            10
        );
        assert_eq!(s.funded, 0);
    }
}

#[test]
fn oracle_bond_complete_small_domain_has_exact_ordered_reasons_and_full_frames() {
    let p = parameters();
    let mut rows = 0;
    let mut accepts = 0;
    for phase in [ReporterPhase::Active, ReporterPhase::Unbonding] {
        for held in 0..=2 {
            for locked in 0..=held {
                for owner_wallet in 0..=2 {
                    for other_wallet in 0..=2 {
                        for state_nonce in 0..=1 {
                            for nonce in 0..=1 {
                                for actor in [1, 2] {
                                    for now in 0..=9 {
                                        let mut s = initial();
                                        s.phase = phase;
                                        s.held = ZdexE18(held);
                                        s.exit_epoch = if phase == ReporterPhase::Active {
                                            None
                                        } else {
                                            Some(0)
                                        };
                                        s.wallets = BTreeMap::from([
                                            (id(1), ZdexE18(owner_wallet)),
                                            (id(2), ZdexE18(other_wallet)),
                                        ]);
                                        s.nonces = BTreeMap::from([
                                            (id(1), state_nonce),
                                            (id(2), state_nonce),
                                        ]);
                                        if locked > 0 {
                                            s.liabilities.push(Liability {
                                                id: [1; 32],
                                                report: [2; 32],
                                                feed: [3; 32],
                                                locked: ZdexE18(locked),
                                                publication: None,
                                                exposure_until: 8,
                                                clear_after: 8,
                                                cleared: false,
                                            });
                                        }
                                        let mut commands = vec![
                                            BondCommand::Unregister,
                                            BondCommand::Clear { liability: [1; 32] },
                                        ];
                                        for amount in 0..=2 {
                                            commands.push(BondCommand::Deposit(ZdexE18(amount)));
                                            commands.push(BondCommand::Withdraw(ZdexE18(amount)));
                                        }
                                        for command in commands {
                                            // Separately authored guard table. Every listed reason is checked,
                                            // including reasons the producer never returns on these domains.
                                            let mut failures = [false; 32];
                                            failures[0] = actor != 1
                                                && matches!(
                                                    command,
                                                    BondCommand::Unregister
                                                        | BondCommand::Withdraw(_)
                                                );
                                            failures[1] = nonce != state_nonce;
                                            failures[4] =
                                                matches!(command, BondCommand::Clear { .. })
                                                    && locked == 0;
                                            failures[6] =
                                                matches!(command, BondCommand::Unregister)
                                                    && phase != ReporterPhase::Active
                                                    || matches!(command, BondCommand::Withdraw(_))
                                                        && phase == ReporterPhase::Active;
                                            failures[11] = matches!(command,BondCommand::Deposit(x)|BondCommand::Withdraw(x) if x.0==0);
                                            failures[16] = matches!(command,BondCommand::Withdraw(x) if x.0>held-locked);
                                            let actor_wallet = if actor == 1 {
                                                owner_wallet
                                            } else {
                                                other_wallet
                                            };
                                            failures[17] = matches!(command,BondCommand::Deposit(x) if x.0>actor_wallet);
                                            failures[25] =
                                                matches!(command, BondCommand::Withdraw(_))
                                                    && now < 1
                                                    || matches!(command, BondCommand::Clear { .. })
                                                        && locked > 0
                                                        && now < 8;
                                            let expected_reason = failures
                                                .iter()
                                                .position(|x| *x)
                                                .map(|i| REASONS[i]);
                                            let out = bond_relation(
                                                AccountContext {
                                                    parameters: &p,
                                                    actor: &id(actor),
                                                    nonce,
                                                    clock: clock(now),
                                                    capacity: 16,
                                                },
                                                &s,
                                                &command,
                                                limits(),
                                            );
                                            let outcome = out.result().unwrap_or_else(|e| {
                                                panic!("unexpected technical refusal: {e:?}")
                                            });
                                            if let Some(reason) = expected_reason {
                                                assert_eq!(
                                                    outcome.as_ref().map(|_| ()),
                                                    Err(&reason)
                                                );
                                                assert_eq!(
                                                    REASONS
                                                        .iter()
                                                        .filter(|r| Some(**r) == expected_reason)
                                                        .count(),
                                                    1
                                                );
                                            } else {
                                                accepts += 1;
                                                let plan = outcome.as_ref().unwrap_or_else(|r| {
                                                    panic!("legitimate acceptance rejected: {r:?}")
                                                });
                                                let mut expected = s.clone();
                                                expected.epoch = now;
                                                expected.nonces.insert(id(actor), nonce + 1);
                                                match command {
                                                    BondCommand::Deposit(x) => {
                                                        expected.held = ZdexE18(held + x.0);
                                                        expected.wallets.insert(
                                                            id(actor),
                                                            ZdexE18(actor_wallet - x.0),
                                                        );
                                                    }
                                                    BondCommand::Withdraw(x) => {
                                                        expected.held = ZdexE18(held - x.0);
                                                        expected.wallets.insert(
                                                            id(actor),
                                                            ZdexE18(actor_wallet + x.0),
                                                        );
                                                        if held == x.0 && locked == 0 {
                                                            expected.phase = ReporterPhase::Retired;
                                                        }
                                                    }
                                                    BondCommand::Unregister => {
                                                        expected.phase = ReporterPhase::Unbonding;
                                                        expected.exit_epoch = Some(now);
                                                    }
                                                    BondCommand::Clear { .. } => {
                                                        expected.liabilities[0].locked = ZdexE18(0);
                                                        expected.liabilities[0].cleared = true;
                                                    }
                                                    _ => panic!("closed command domain"),
                                                }
                                                assert_eq!(plan.state(), &expected);
                                                assert_eq!(total(plan.state()), total(&s));
                                                assert!(failures.iter().all(|f| !*f));
                                            }
                                            rows += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(rows, 69_120);
    assert!(accepts > 0);
}

#[test]
fn oracle_rewards_are_held_until_the_exact_liability_is_cleared_and_paid_once() {
    let p = parameters();
    let mut bond = initial();
    bond.held = ZdexE18(1);
    bond.wallets.insert(id(1), ZdexE18(9));
    bond.liabilities.push(Liability {
        id: [1; 32],
        report: [2; 32],
        feed: [3; 32],
        locked: ZdexE18(1),
        publication: Some(0),
        exposure_until: 8,
        clear_after: 8,
        cleared: false,
    });
    let clock_rules = p.clock.as_ref().unwrap_or_else(|| panic!("clock")).clone();
    let feed = FeedRules {
        feed: [3; 32],
        profile: [2; 32],
        query: [4; 32],
        clock: clock_rules,
        max_deviation_bps: 0,
        diversity: Diversity {
            minimum: [3; 5],
            max_same_class: [1; 5],
        },
        bounds: zeno_fcis_synthesis::finite::v2_oracle::CollectionLimits {
            max_entries: 16,
            max_item_bytes: 10000,
            max_snapshot_bytes: 100000,
        },
        frame_bytes: 200000,
    };
    let sources: Vec<_> = (1..=3)
        .map(|n| Source {
            id: id(n),
            operator: id(n),
            venue: id(n),
            family: id(n),
            transport: id(n),
            jurisdiction: id(n),
        })
        .collect();
    let reports: Vec<_> = (1..=3)
        .map(|n| Observation {
            id: [n + 1; 32],
            admission: [n + 10; 32],
            reporter: id(n),
            source: id(n),
            feed: [3; 32],
            profile: [2; 32],
            query: [4; 32],
            round: 0,
            value: PriceE8::new(10).unwrap_or_else(|_| panic!("price")),
            epoch: 0,
            seconds: 0,
        })
        .collect();
    let aggregate = aggregate(&feed, 0, 0, 0, &reports, &sources, limits())
        .into_parts()
        .0
        .unwrap_or_else(|_| panic!("aggregate"))
        .unwrap_or_else(|_| panic!("aggregate"));
    let unit = p
        .economy
        .as_ref()
        .unwrap_or_else(|| panic!("economy"))
        .reward_unit
        .clone();
    let rules = RewardRules {
        feed: [3; 32],
        profile: [2; 32],
        unit: unit.clone(),
        per_report: 1,
        reporter_share: 1,
        treasury_share: 0,
        burn_share: 0,
        denominator: 1,
        treasury: id(8),
    };
    let mut state = RewardState {
        feed: rules.feed,
        profile: rules.profile,
        unit,
        funded: 0,
        remaining: 0,
        paid: 0,
        treasury_paid: 0,
        burned: 0,
        residual: 0,
        wallets: BTreeMap::from([(id(2), 10)]),
        claims: vec![],
        nonces: BTreeMap::new(),
        epoch: 0,
    };
    let evaluate =
        |state: &RewardState, bond: &BondState, actor: u8, command: &RewardCommand, epoch: u64| {
            reward_relation(
                &rules,
                AccountContext {
                    parameters: &p,
                    actor: &id(actor),
                    nonce: state.nonces.get(&id(actor)).copied().unwrap_or(0),
                    clock: clock(epoch),
                    capacity: 16,
                },
                state,
                bond,
                command,
                limits(),
            )
        };
    state = evaluate(&state, &bond, 2, &RewardCommand::Fund(2), 0)
        .into_parts()
        .0
        .unwrap_or_else(|_| panic!("fund"))
        .unwrap_or_else(|_| panic!("fund"))
        .into_state();
    assert!(!state.wallets.contains_key(&id(8)));
    let accrue = RewardCommand::Accrue {
        aggregate: aggregate.into(),
        report: [2; 32],
    };
    state = evaluate(&state, &bond, 2, &accrue, 0)
        .into_parts()
        .0
        .unwrap_or_else(|_| panic!("accrue"))
        .unwrap_or_else(|_| panic!("accrue"))
        .into_state();
    let claim = RewardCommand::Claim(state.claims[0].id);
    assert_eq!(
        evaluate(&state, &bond, 1, &claim, 0)
            .result()
            .map(|x| x.as_ref().map(|_| ())),
        Ok(Err(&Reason::LiabilityLocked))
    );
    assert_eq!(
        evaluate(&state, &bond, 2, &accrue, 0)
            .result()
            .map(|x| x.as_ref().map(|_| ())),
        Ok(Err(&Reason::AlreadyClaimedOrSlashed))
    );
    bond = apply(&bond, 2, BondCommand::Clear { liability: [1; 32] }, 8);
    state = evaluate(&state, &bond, 1, &claim, 8)
        .into_parts()
        .0
        .unwrap_or_else(|_| panic!("claim"))
        .unwrap_or_else(|_| panic!("claim"))
        .into_state();
    assert_eq!(state.wallets[&id(1)], 1);
    assert_eq!(state.remaining, 1);
    assert_eq!(state.paid, 1);
    assert_eq!(
        evaluate(&state, &bond, 1, &claim, 8)
            .result()
            .map(|x| x.as_ref().map(|_| ())),
        Ok(Err(&Reason::AlreadyClaimedOrSlashed))
    );
}

// Round 3 review witnesses: slash custody and nullifiers, full-width coalition
// capacity and ordered precedence over later fallible arithmetic.
fn round3_parameters(recipient: EscrowRecipient) -> Parameters {
    let mut p = parameters();
    let d = p.disputes.as_mut().unwrap_or_else(|| panic!("disputes"));
    d.minimum_bond = ZdexE18(2);
    d.slash_recipient = recipient;
    p
}
fn round3_open_bond() -> BondState {
    let mut s = initial();
    s.held = ZdexE18(2);
    s.wallets.insert(id(1), ZdexE18(8));
    s.wallets.insert(id(2), ZdexE18(9));
    s.liabilities.push(Liability {
        id: [1; 32],
        report: [2; 32],
        feed: [3; 32],
        locked: ZdexE18(2),
        publication: Some(0),
        exposure_until: 8,
        clear_after: 8,
        cleared: false,
    });
    s.disputes.push(Dispute {
        id: [4; 32],
        liability: [1; 32],
        challenger: id(2),
        escrow: ZdexE18(1),
        opened: 0,
        phase: DisputePhase::Open,
        slash: ZdexE18(0),
        violation: None,
        slash_verdict: None,
        resolution_verdict: None,
    });
    s
}
fn round3_upheld(violation: u8) -> FinalVerdict {
    FinalVerdict {
        adjudicator: [6; 32],
        dispute: [4; 32],
        violation: [violation; 32],
        upheld: true,
        final_epoch: 0,
        finality_root: [9; 32],
    }
}
fn round3_slash_command(violation: u8) -> BondCommand {
    BondCommand::Slash {
        dispute: [4; 32],
        verdict: round3_upheld(violation),
    }
}
fn round3_outcome(
    p: &Parameters,
    s: &BondState,
    actor: u8,
    c: &BondCommand,
    now: u64,
) -> Result<Result<BondState, Reason>, Failure> {
    bond_relation(
        AccountContext {
            parameters: p,
            actor: &id(actor),
            nonce: s.nonces.get(&id(actor)).copied().unwrap_or(0),
            clock: clock(now),
            capacity: 16,
        },
        s,
        c,
        limits(),
    )
    .into_parts()
    .0
    .map(|r| r.map(BondPlan::into_state))
}
fn round3_accept(p: &Parameters, s: &BondState, actor: u8, c: &BondCommand) -> BondState {
    round3_outcome(p, s, actor, c, 0)
        .unwrap_or_else(|e| panic!("refusal {e:?}"))
        .unwrap_or_else(|e| panic!("reject {e:?}"))
}

#[test]
fn round3_repeated_slash_rejects_without_overwriting_history() {
    let p = round3_parameters(EscrowRecipient::Challenger);
    let s = round3_open_bond();
    let first = round3_accept(&p, &s, 2, &round3_slash_command(5));
    assert_eq!(total(&first), total(&s));
    assert_eq!(first.held, ZdexE18(1));
    assert_eq!(first.liabilities[0].locked, ZdexE18(1));
    // The same and a fresh violation on the already slashed dispute both
    // consume a nullifier, although one zDEX of the allocation remains locked.
    for violation in [5, 6] {
        assert_eq!(
            round3_outcome(&p, &first, 2, &round3_slash_command(violation), 0),
            Ok(Err(Reason::AlreadyClaimedOrSlashed))
        );
    }
    assert_eq!(first.disputes[0].violation, Some([5; 32]));
    assert_eq!(first.disputes[0].slash_verdict, Some(round3_upheld(5)));
    // The successor the old relation produced loses one zDEX. A candidate that
    // conserves custody is still a second penalty. The laws refuse both.
    let mut lost = first.clone();
    lost.held = ZdexE18(0);
    lost.liabilities[0].locked = ZdexE18(0);
    lost.disputes[0].violation = Some([6; 32]);
    lost.disputes[0].slash_verdict = Some(round3_upheld(6));
    lost.nonces.insert(id(2), 2);
    let mut paid = lost.clone();
    paid.wallets.insert(id(2), ZdexE18(11));
    for (candidate, law) in [(&lost, 113_400), (&paid, 113_200)] {
        assert_eq!(
            check_bond_laws(
                AccountContext {
                    parameters: &p,
                    actor: &id(2),
                    nonce: 1,
                    clock: clock(0),
                    capacity: 16
                },
                &first,
                &round3_slash_command(6),
                Ok(candidate),
                limits()
            )
            .result(),
            Err(&Failure::Law(law))
        );
    }
}

#[test]
fn round3_slash_recipient_is_paid_immediately() {
    for recipient in [
        EscrowRecipient::Challenger,
        EscrowRecipient::ReporterOwner,
        EscrowRecipient::Treasury,
        EscrowRecipient::Burn,
    ] {
        let p = round3_parameters(recipient.clone());
        let s = round3_open_bond();
        let slashed = round3_accept(&p, &s, 2, &round3_slash_command(5));
        let mut wallets = s.wallets.clone();
        let mut burned = 0;
        match recipient {
            EscrowRecipient::Challenger => {
                wallets.insert(id(2), ZdexE18(10));
            }
            EscrowRecipient::ReporterOwner => {
                wallets.insert(id(1), ZdexE18(9));
            }
            EscrowRecipient::Treasury => {
                wallets.insert(id(8), ZdexE18(1));
            }
            EscrowRecipient::Burn => burned = 1,
        }
        assert_eq!(slashed.wallets, wallets, "{recipient:?}");
        assert_eq!(slashed.burned, ZdexE18(burned));
        assert_eq!(slashed.held, ZdexE18(1));
        assert_eq!(slashed.disputes[0].slash, ZdexE18(1));
        assert_eq!(slashed.disputes[0].escrow, ZdexE18(1));
        assert_eq!(total(&slashed), total(&s));
        // Upheld resolution moves only the challenge escrow (to the challenger
        // under these parameters); the penalty is never paid a second time.
        let resolved = round3_accept(
            &p,
            &slashed,
            2,
            &BondCommand::Resolve {
                dispute: [4; 32],
                verdict: Verdict::Final(round3_upheld(5)),
            },
        );
        wallets.insert(id(2), ZdexE18(wallets[&id(2)].0 + 1));
        assert_eq!(resolved.wallets, wallets, "{recipient:?}");
        assert_eq!(resolved.burned, slashed.burned);
        assert_eq!(resolved.held, slashed.held);
        assert_eq!(resolved.disputes[0].escrow, ZdexE18(0));
        assert_eq!(resolved.disputes[0].phase, DisputePhase::ResolvedUpheld);
        assert_eq!(total(&resolved), total(&s));
    }
}

#[test]
fn round3_full_width_pair_capacity() {
    let p = parameters();
    let half = 1u128 << 127;
    for (bonds, capacity) in [
        ([half; 3], half),
        ([half, u128::MAX, half], half),
        ([u128::MAX; 3], u128::MAX),
    ] {
        let inputs = SecurityInputs {
            selected_bonds: bonds.map(ZdexE18),
            stress: Some(StressE8(100_000_000)),
            stress_source: [7; 32],
            valuation_root: [8; 32],
        };
        assert_eq!(
            capacity_status(&p, &inputs, limits()).result(),
            Ok(&CapacityStatus::Available(UsdExposureE18(capacity)))
        );
    }
}

#[test]
fn round3_bond_replay_precedes_later_overflow() {
    let mut s = initial();
    s.phase = ReporterPhase::Unbonding;
    s.exit_epoch = Some(u64::MAX);
    s.epoch = u64::MAX;
    s.held = ZdexE18(1);
    s.wallets.insert(id(1), ZdexE18(9));
    let p = parameters();
    let withdraw = |nonce, amount| {
        bond_relation(
            AccountContext {
                parameters: &p,
                actor: &id(1),
                nonce,
                clock: clock(u64::MAX),
                capacity: 16,
            },
            &s,
            &BondCommand::Withdraw(ZdexE18(amount)),
            limits(),
        )
        .into_parts()
        .0
        .map(|r| r.map(|_| ()))
    };
    // exit_epoch + exit_delay overflows; only the R26 comparison needs it.
    assert_eq!(withdraw(1, 1), Ok(Err(Reason::Replay)));
    assert_eq!(withdraw(0, 0), Ok(Err(Reason::ZeroAmountOrPrice)));
    assert_eq!(withdraw(0, 2), Ok(Err(Reason::InsufficientBond)));
    assert_eq!(withdraw(0, 1), Err(Failure::Arithmetic));
    let mut active = s.clone();
    active.phase = ReporterPhase::Active;
    active.exit_epoch = None;
    assert_eq!(
        round3_outcome(&p, &active, 1, &BondCommand::Withdraw(ZdexE18(1)), u64::MAX)
            .map(|r| r.map(|_| ())),
        Ok(Err(Reason::WrongPhase))
    );
    // publication + dispute_open overflows; only the R21 comparison needs it.
    let mut published = initial();
    published.held = ZdexE18(1);
    published.epoch = u64::MAX;
    published.wallets.insert(id(2), ZdexE18(0));
    published.liabilities.push(Liability {
        id: [1; 32],
        report: [2; 32],
        feed: [3; 32],
        locked: ZdexE18(1),
        publication: Some(u64::MAX),
        exposure_until: u64::MAX,
        clear_after: u64::MAX,
        cleared: false,
    });
    let open = BondCommand::OpenDispute {
        id: [4; 32],
        liability: [1; 32],
    };
    let open_dispute =
        |s: &BondState| round3_outcome(&p, s, 2, &open, u64::MAX).map(|r| r.map(|_| ()));
    assert_eq!(
        open_dispute(&published),
        Ok(Err(Reason::InsufficientBalance))
    );
    published.wallets.insert(id(2), ZdexE18(1));
    assert_eq!(open_dispute(&published), Err(Failure::Arithmetic));
}

#[test]
fn round3_slash_sequences_apply_one_penalty_and_conserve_for_every_parameter() {
    // Bounded product: minimum 1..3, slash 1..minimum, free surplus 0..2, all
    // four recipients and every sequence of up to three slash/resolve commands.
    let resolve = |verdict| BondCommand::Resolve {
        dispute: [4; 32],
        verdict,
    };
    let commands = [
        round3_slash_command(5),
        round3_slash_command(6),
        resolve(Verdict::Final(round3_upheld(5))),
        resolve(Verdict::Final(round3_upheld(6))),
        resolve(Verdict::Final(FinalVerdict {
            upheld: false,
            ..round3_upheld(5)
        })),
        resolve(Verdict::Inconclusive),
    ];
    let mut cases = 0;
    let mut slashes = 0;
    for minimum in 1..=3 {
        for slash in 1..=minimum {
            for surplus in 0..=2 {
                for recipient in [
                    EscrowRecipient::Challenger,
                    EscrowRecipient::ReporterOwner,
                    EscrowRecipient::Treasury,
                    EscrowRecipient::Burn,
                ] {
                    let mut p = round3_parameters(recipient.clone());
                    let d = p.disputes.as_mut().unwrap_or_else(|| panic!("disputes"));
                    d.minimum_bond = ZdexE18(minimum);
                    d.slash = ZdexE18(slash);
                    let mut start = round3_open_bond();
                    start.held = ZdexE18(minimum + surplus);
                    start.liabilities[0].locked = ZdexE18(minimum);
                    let mut frontier = vec![start.clone()];
                    for _ in 0..3 {
                        let mut next = Vec::new();
                        for s in &frontier {
                            for c in &commands {
                                cases += 1;
                                let Ok(after) =
                                    round3_outcome(&p, s, 2, c, 0).unwrap_or_else(|e| {
                                        panic!("refusal {e:?} on {c:?} from {s:?}")
                                    })
                                else {
                                    continue;
                                };
                                assert_eq!(total(&after), total(&start));
                                let before = &s.disputes[0];
                                let x = &after.disputes[0];
                                assert!(
                                    before.violation.is_none() || before.violation == x.violation
                                );
                                assert!(x.slash == ZdexE18(0) || x.slash == ZdexE18(slash));
                                assert_eq!(start.held.0 - after.held.0, x.slash.0);
                                if before.slash_verdict.is_none() && x.slash_verdict.is_some() {
                                    slashes += 1;
                                    let gained = |n: u8| {
                                        after.wallets.get(&id(n)).map_or(0, |w| w.0)
                                            - s.wallets.get(&id(n)).map_or(0, |w| w.0)
                                    };
                                    let paid = match recipient {
                                        EscrowRecipient::Challenger => gained(2),
                                        EscrowRecipient::ReporterOwner => gained(1),
                                        EscrowRecipient::Treasury => gained(8),
                                        EscrowRecipient::Burn => after.burned.0 - s.burned.0,
                                    };
                                    assert_eq!(paid, slash);
                                }
                                next.push(after);
                            }
                        }
                        frontier = next;
                    }
                }
            }
        }
    }
    // 72 parameter cases; per case 6 + 24 + 48 commands, and a first penalty
    // on two violations after zero, one or two inconclusive resolutions.
    assert_eq!((cases, slashes), (72 * 78, 72 * 6));
}

#[test]
fn round3_bond_zero_max_and_reservation_neighbors() {
    let p = parameters();
    for n in [0, 1, u128::MAX - 1, u128::MAX] {
        let mut s = initial();
        s.wallets = BTreeMap::from([(id(1), ZdexE18(u128::MAX))]);
        let out = bond_relation(
            AccountContext {
                parameters: &p,
                actor: &id(1),
                nonce: 0,
                clock: clock(0),
                capacity: 16,
            },
            &s,
            &BondCommand::Deposit(ZdexE18(n)),
            limits(),
        );
        if n == 0 {
            assert!(matches!(out.result(), Ok(Err(Reason::ZeroAmountOrPrice))));
        } else {
            let after = out
                .into_parts()
                .0
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                .into_state();
            assert_eq!(after.held.0, n);
            assert_eq!(after.wallets[&id(1)].0, u128::MAX - n);
        }
    }
    for n in [0, 1, 2, u128::MAX] {
        let mut s = initial();
        s.held = ZdexE18(n);
        s.wallets.clear();
        let out = bond_relation(
            AccountContext {
                parameters: &p,
                actor: &id(1),
                nonce: 0,
                clock: clock(0),
                capacity: 16,
            },
            &s,
            &BondCommand::Reserve {
                id: [1; 32],
                report: [2; 32],
                feed: [3; 32],
            },
            limits(),
        );
        if n == 0 {
            assert!(matches!(out.result(), Ok(Err(Reason::InsufficientBond))));
        } else {
            let after = out
                .into_parts()
                .0
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                .into_state();
            assert_eq!(after.liabilities[0].locked.0, 1);
            assert_eq!(after.held.0, n);
        }
    }
}
#[test]
fn round3_quorum_threshold_and_epoch_neighbors() {
    use zeno_fcis_synthesis::finite::v2_oracle::CollectionLimits;
    let sources: Vec<_> = (1..=3)
        .map(|n| Source {
            id: id(n),
            operator: id(n),
            venue: id(n),
            family: id(n),
            transport: id(n),
            jurisdiction: id(n),
        })
        .collect();
    let mut os: Vec<_> = (1..=3)
        .map(|n| Observation {
            id: [n; 32],
            admission: [n + 10; 32],
            reporter: id(n),
            source: id(n),
            feed: [1; 32],
            profile: [2; 32],
            query: [3; 32],
            round: 0,
            value: PriceE8::new(10000)
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            epoch: 0,
            seconds: u64::MAX,
        })
        .collect();
    let r = FeedRules {
        feed: [1; 32],
        profile: [2; 32],
        query: [3; 32],
        diversity: Diversity {
            minimum: [3; 5],
            max_same_class: [1; 5],
        },
        max_deviation_bps: 10,
        clock: parameters()
            .clock
            .unwrap_or_else(|| panic!("test fixture value is absent")),
        bounds: CollectionLimits {
            max_entries: 64,
            max_item_bytes: 8192,
            max_snapshot_bytes: 1000000,
        },
        frame_bytes: 2000000,
    };
    for deviation in [9, 10, 11] {
        os[2].value = PriceE8::new(10000 + deviation)
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        let out = aggregate(&r, 0, 0, 0, &os, &sources, limits());
        if deviation > 10 {
            assert!(matches!(out.result(), Ok(Err(Reason::ExcessUncertainty))));
        } else {
            let a = out
                .into_parts()
                .0
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
            assert_eq!(a.numbers().deviation_bps, deviation);
            assert_eq!(a.numbers().median_seconds, u64::MAX);
        }
    }
    os[2].value =
        PriceE8::new(10000).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    for now in [3, 4, 5] {
        let out = aggregate(&r, 0, 0, now, &os, &sources, limits());
        if now == 5 {
            assert!(matches!(out.result(), Ok(Err(Reason::StaleObservation))));
        } else {
            assert!(matches!(out.result(), Ok(Ok(_))));
        }
    }
    os[0].epoch = 1;
    assert!(matches!(
        aggregate(&r, 0, 0, 0, &os, &sources, limits()).result(),
        Ok(Err(Reason::FutureObservation))
    ));
}
