use zenofcis_zusd_v1::{
    fees, liquidation as l, redemption as r, stability as sp, staking as st, vaults as v, *,
};
fn limits() -> Limits {
    Limits {
        steps: 100_000_000,
        reads: 1_000_000,
        bytes: 10_000_000,
    }
}
fn bounds() -> sp::Bounds {
    sp::Bounds {
        history: 8,
        epochs: 4,
        deposits: 4,
        front_ends: 2,
        vaults: 4,
    }
}
fn id(x: u8) -> Id {
    let mut out = [0; 32];
    out[31] = x;
    out
}
fn prices() -> v::Prices {
    v::Prices {
        committed: 100_000_000,
        pending: None,
        status: v::PriceStatus::Usable,
        pending_blocked: false,
    }
}
fn time() -> Option<fees::LogicalTime> {
    Some(fees::LogicalTime {
        epoch: 0,
        elapsed_minutes: 0,
    })
}
fn fee() -> fees::State {
    fees::State {
        base: 0,
        last_epoch: 0,
    }
}
fn system() -> v::System {
    v::System {
        collateral: 0,
        debt: 0,
        active: 0,
        stakes: 0,
        stakes_snapshot: 0,
        collateral_snapshot: 0,
        reward_c: Accumulator256::from(0),
        reward_d: Accumulator256::from(0),
    }
}
fn vault(x: u8, c: u128, d: u128, reserve: u128) -> v::Vault {
    v::Vault {
        parameter_version: 1,
        id: id(x),
        owner: id(x),
        status: v::Status::Active,
        collateral: c,
        debt: d,
        reserve,
        stake: c,
        snapshot_c: Accumulator256::from(0),
        snapshot_d: Accumulator256::from(0),
    }
}
fn book(vaults: Vec<v::Vault>, order: Vec<Id>) -> l::Book {
    let s = v::System {
        collateral: vaults.iter().map(|v| v.collateral).sum(),
        debt: vaults.iter().map(|v| v.debt).sum(),
        active: vaults.len() as u32,
        stakes: vaults.iter().map(|v| v.stake).sum(),
        ..system()
    };
    let priorities = vaults
        .iter()
        .map(|v| {
            (
                v.id,
                *mul_div(v.collateral, NICR_Q, v.debt, limits())
                    .result()
                    .unwrap(),
            )
        })
        .collect();
    let gas_balance = vaults.iter().map(|v| v.reserve).sum();
    l::Book {
        vaults,
        order,
        priorities,
        system: s,
        default_collateral: 0,
        default_debt: 0,
        error_c: Accumulator256::from(0),
        error_d: Accumulator256::from(0),
        gas_balance,
        surplus: vec![],
        pool: sp::State::reference_genesis(sp::FUNDED_CAP).unwrap(),
    }
}
#[test]
fn fees_match_independent_integer_reference_and_unbound_clock() {
    for line in include_str!("test-data/fees.tsv").lines() {
        let a: Vec<u128> = line.split('\t').map(|s| s.parse().unwrap()).collect();
        let out = fees::borrow(
            fees::State {
                base: a[0],
                last_epoch: 0,
            },
            a[1],
            false,
            Q,
            Some(fees::LogicalTime {
                epoch: 1,
                elapsed_minutes: a[2] as u64,
            }),
            limits(),
        );
        let f = out.result().unwrap();
        assert_eq!(
            (f.state.base, f.rate, f.amount),
            (a[3], a[4], a[5]),
            "{line}"
        );
        assert_eq!(f.state.last_epoch, u64::from(a[2] > 0));
    }
    assert_eq!(
        fees::borrow(fee(), 500 * Q, false, Q, None, limits()).result(),
        Err(Failure::UnboundProfile)
    );
    let recovery = fees::borrow(
        fees::State {
            base: Q / 2,
            last_epoch: 7,
        },
        500 * Q,
        true,
        0,
        None,
        limits(),
    );
    assert_eq!(
        recovery.result().unwrap().state,
        fees::State {
            base: Q / 2,
            last_epoch: 7
        }
    );
    let redemption = fees::redemption(fee(), 100 * Q, 100_000_000, 1000 * Q, Q, time(), limits());
    let f = redemption.result().unwrap();
    assert_eq!(f.state.base, Q / 20);
    assert_eq!(f.amount, 55 * Q / 10);
}
#[test]
fn liquidation_exact_partition_corpus() {
    for line in include_str!("test-data/liquidation.tsv").lines() {
        let a: Vec<_> = line.split('\t').collect();
        let n: Vec<u128> = a[..6].iter().map(|s| s.parse().unwrap()).collect();
        let out = l::quote(
            vault(1, n[0], n[1], 1),
            v::System {
                collateral: n[4],
                debt: n[5],
                active: 2,
                ..system()
            },
            prices(),
            n[3],
            limits(),
        );
        let input = vault(1, n[0], n[1], 1);
        let s = v::System {
            collateral: n[4],
            debt: n[5],
            active: 2,
            ..system()
        };
        let candidate = out.result().copied();
        assert_eq!(
            laws::check_quote(input, s, prices(), n[3], candidate, limits()).result(),
            Ok(&())
        );
        for reason in common::REASONS {
            let test = laws::check_quote(
                input,
                s,
                prices(),
                n[3],
                Err(Failure::Reject(*reason)),
                limits(),
            );
            assert_eq!(
                test.result().is_ok(),
                candidate == Err(Failure::Reject(*reason))
            );
        }
        if a[6] == "reject" {
            assert_eq!(out.result(), Err(Failure::Reject(1044)), "{line}");
        } else {
            let e: Vec<u128> = a[6].split(',').map(|s| s.parse().unwrap()).collect();
            let p = out.result().unwrap();
            assert_eq!(
                [
                    p.offset,
                    p.redistributed_debt,
                    p.sp_collateral,
                    p.redistributed_collateral,
                    p.keeper_collateral,
                    p.surplus
                ],
                e.as_slice(),
                "{line}"
            );
        }
    }
}
#[test]
fn vault_minimum_fee_pending_and_freeze_relations() {
    let opened = v::open(
        v::OpenInputs {
            existing: None,
            id: id(1),
            owner: id(1),
            collateral: 1200 * Q,
            principal: 498 * Q,
            balances: (1200 * Q, 0),
            system: system(),
            parameters: v::Parameters::v1(),
            fee: fee(),
            prices: prices(),
            max_fee: Q,
            time: time(),
        },
        limits(),
    );
    let o = *opened.result().unwrap();
    assert_eq!(o.fee_paid, 249 * Q / 100);
    assert_eq!(o.vault.debt, 55049 * Q / 100);
    assert_eq!(o.minted + o.fee_paid + o.reserve_minted, o.vault.debt);
    let pending = v::Prices {
        pending: Some(40_000_000),
        ..prices()
    };
    assert_eq!(
        v::open(
            v::OpenInputs {
                existing: None,
                id: id(1),
                owner: id(1),
                collateral: 1200 * Q,
                principal: 498 * Q,
                balances: (1200 * Q, 0),
                system: system(),
                parameters: v::Parameters::v1(),
                fee: fee(),
                prices: pending,
                max_fee: Q,
                time: time(),
            },
            limits()
        )
        .result(),
        Err(Failure::Reject(1043))
    );
    let frozen = v::Prices {
        status: v::PriceStatus::Stale,
        ..prices()
    };
    let reduced = v::adjust(
        v::AdjustInputs {
            vault: o.vault,
            system: o.system,
            parameters: v::Parameters::v1(),
            fee: o.fee,
            prices: frozen,
            collateral_delta: v::Delta::Add(Q),
            debt_delta: v::Delta::None,
            wallet: (Q, 0),
            max_fee: Q,
            time: None,
        },
        limits(),
    );
    assert!(reduced.result().is_ok());
    assert_eq!(
        v::adjust(
            v::AdjustInputs {
                vault: o.vault,
                system: o.system,
                parameters: v::Parameters::v1(),
                fee: o.fee,
                prices: frozen,
                collateral_delta: v::Delta::Remove(Q),
                debt_delta: v::Delta::None,
                wallet: (0, 0),
                max_fee: Q,
                time: None,
            },
            limits()
        )
        .result(),
        Err(Failure::Reject(1021))
    );
    assert_eq!(
        v::close(o.vault, o.system, o.fee, prices(), 1000 * Q, limits()).result(),
        Err(Failure::Reject(1041))
    );
}
#[test]
fn d3_debt_only_pending_is_strictly_better_on_complete_lag_domain() {
    let mut strict = 0;
    for c in 0..=3 {
        for d in 0..=3 {
            for stake in 1..=3 {
                let mut v = vault(1, 10, 10, 1);
                v.stake = stake;
                let s = v::System {
                    reward_c: Accumulator256::from(c * Q),
                    reward_d: Accumulator256::from(d * Q),
                    ..system()
                };
                let out = v::apply_pending(v, s, limits());
                let fixed = out.result().unwrap();
                assert_eq!(
                    (fixed.collateral, fixed.debt),
                    (10 + c * stake, 10 + d * stake)
                );
                let old = if c == 0 {
                    (10, 10)
                } else {
                    (10 + c * stake, 10 + d * stake)
                };
                if old != (fixed.collateral, fixed.debt) {
                    assert_eq!(c, 0);
                    assert!(d > 0);
                    strict += 1;
                }
            }
        }
    }
    assert_eq!(strict, 9);
}
#[test]
fn d4_exact_backing_exhaustion_and_strict_witness() {
    let mut strict = 0;
    let mut checked = 0;
    for c in 0..=8 {
        for spc in 0..=c {
            for keeper in 0..=c - spc {
                for surplus in 0..=c - spc - keeper {
                    let p = l::Partition {
                        debt: 1,
                        collateral: 0,
                        offset: 0,
                        redistributed_debt: 1,
                        sp_collateral: spc,
                        redistributed_collateral: 0,
                        keeper_collateral: keeper,
                        surplus,
                        reserve: 0,
                    };
                    let actual = l::remaining_backing(c, 1, p, limits());
                    checked += 1;
                    let fixed = actual.result().unwrap().0;
                    assert_eq!(fixed + spc + keeper + surplus, c);
                    let old_lowest = c - spc - surplus;
                    let old_batch = c - spc;
                    assert!(fixed <= old_lowest && fixed <= old_batch);
                    if keeper + surplus > 0 {
                        assert!(fixed < old_batch);
                        strict += 1;
                    }
                }
            }
        }
    }
    assert_eq!((checked, strict), (495, 450));
    // Source incorrectly says Normal at exactly 150%; keeper collateral is already gone.
    let p = l::Partition {
        sp_collateral: 0,
        keeper_collateral: 1,
        surplus: 0,
        debt: 1,
        collateral: 0,
        offset: 0,
        redistributed_debt: 1,
        redistributed_collateral: 0,
        reserve: 0,
    };
    assert_eq!(
        l::remaining_backing(150, 100, p, limits()).result(),
        Ok(&(149, 100))
    );
    assert!(149 * Q / 100 < v::CCR);
    assert_eq!(150 * Q / 100, v::CCR);
}
#[test]
fn recovery_batch_skips_last_survivor_and_source_frame() {
    let pre = book(
        vec![vault(1, 90, 100, 1), vault(2, 90, 100, 1)],
        vec![id(1), id(2)],
    );
    let frozen = pre.clone();
    let result = l::liquidate(
        &pre,
        &l::Command::Batch(vec![id(2), id(1)]),
        prices(),
        None,
        2,
        bounds(),
        limits(),
    );
    let out = result.result().unwrap();
    assert_eq!(out.selected, vec![id(2)]);
    assert_eq!(out.book.system.active, 1);
    assert_eq!(out.keeper_zusd, 1);
    assert_eq!(out.book.system.debt, 200);
    assert_eq!(pre, frozen);
    assert_eq!(
        l::liquidate(
            &out.book,
            &l::Command::Direct(id(1)),
            prices(),
            None,
            2,
            bounds(),
            limits()
        )
        .result(),
        Err(Failure::Reject(1041))
    );
}
#[test]
fn stability_snapshot_loss_reward_host_and_claim_flow() {
    let pre = sp::Book {
        pool: sp::State::reference_genesis(sp::FUNDED_CAP).unwrap(),
        deposits: vec![],
        front_ends: vec![],
    };
    let fe = sp::change(
        sp::ChangeInputs {
            pre: &pre,
            actor: id(9),
            command: sp::Command::RegisterFrontEnd(Q / 2),
            wallet_zusd: 0,
            issuance: None,
            prices: prices(),
            inventory: &[],
            system: system(),
            bounds: bounds(),
        },
        limits(),
    );
    let b = fe.result().unwrap().book.clone();
    let deposited = sp::change(
        sp::ChangeInputs {
            pre: &b,
            actor: id(1),
            command: sp::Command::Deposit {
                amount: 100 * Q,
                tag: Some(id(9)),
            },
            wallet_zusd: 100 * Q,
            issuance: Some(0),
            prices: prices(),
            inventory: &[],
            system: system(),
            bounds: bounds(),
        },
        limits(),
    );
    let mut b = deposited.result().unwrap().book.clone();
    let offset = sp::offset(&b.pool, 50 * Q, 60 * Q, Some(10 * Q), bounds(), limits());
    b.pool = offset.result().unwrap().clone();
    let claim = sp::change(
        sp::ChangeInputs {
            pre: &b,
            actor: id(1),
            command: sp::Command::Claim,
            wallet_zusd: 0,
            issuance: Some(0),
            prices: prices(),
            inventory: &[],
            system: system(),
            bounds: bounds(),
        },
        limits(),
    );
    let o = claim.result().unwrap();
    assert_eq!(o.gains.collateral, 60 * Q);
    assert_eq!(o.gains.depositor_zdex, 5 * Q);
    assert_eq!(o.gains.front_end_zdex, 5 * Q);
    assert_eq!(o.principal_out, 0);
    assert_eq!(o.book.pool.total, 50 * Q);
    assert_eq!(o.book.pool.stock.paid, 10 * Q);
    let again = sp::change(
        sp::ChangeInputs {
            pre: &o.book,
            actor: id(1),
            command: sp::Command::Claim,
            wallet_zusd: 0,
            issuance: Some(0),
            prices: prices(),
            inventory: &[],
            system: system(),
            bounds: bounds(),
        },
        limits(),
    );
    assert_eq!(again.result().unwrap().gains.collateral, 0);
    assert_eq!(
        sp::change(
            sp::ChangeInputs {
                pre: &b,
                actor: id(1),
                command: sp::Command::Claim,
                wallet_zusd: 0,
                issuance: None,
                prices: prices(),
                inventory: &[],
                system: system(),
                bounds: bounds(),
            },
            limits()
        )
        .result(),
        Err(Failure::UnboundProfile)
    );
}
#[test]
fn stability_both_price_principal_guard_and_history_integrity() {
    let b = sp::Book {
        pool: sp::State::reference_genesis(sp::FUNDED_CAP).unwrap(),
        deposits: vec![],
        front_ends: vec![],
    };
    let dep = sp::change(
        sp::ChangeInputs {
            pre: &b,
            actor: id(1),
            command: sp::Command::Deposit {
                amount: 10,
                tag: None,
            },
            wallet_zusd: 10,
            issuance: Some(0),
            prices: prices(),
            inventory: &[],
            system: system(),
            bounds: bounds(),
        },
        limits(),
    );
    let b = dep.result().unwrap().book.clone();
    let v = vault(2, 2, 1, 0);
    let inventory_system = v::System {
        collateral: 2,
        debt: 1,
        active: 1,
        stakes: 2,
        ..system()
    };
    let p = v::Prices {
        pending: Some(50_000_000),
        ..prices()
    };
    assert_eq!(
        sp::change(
            sp::ChangeInputs {
                pre: &b,
                actor: id(1),
                command: sp::Command::Withdraw(1),
                wallet_zusd: 0,
                issuance: Some(0),
                prices: p,
                inventory: &[v],
                system: inventory_system,
                bounds: bounds(),
            },
            limits()
        )
        .result(),
        Err(Failure::Reject(1044))
    );
    assert!(
        sp::change(
            sp::ChangeInputs {
                pre: &b,
                actor: id(1),
                command: sp::Command::Claim,
                wallet_zusd: 0,
                issuance: Some(0),
                prices: p,
                inventory: &[v],
                system: inventory_system,
                bounds: bounds(),
            },
            limits()
        )
        .result()
        .is_ok()
    );
    let epoch = sp::offset(&b.pool, 10, 10, Some(0), bounds(), limits());
    let mut broken = epoch.result().unwrap().clone();
    assert_eq!(broken.epoch, 1);
    broken.history.remove(0);
    assert_eq!(
        sp::offset(&broken, 0, 0, None, bounds(), limits()).result(),
        Err(Failure::Canonical)
    );
}
#[test]
fn staking_temporal_f_and_zero_weight_are_exact() {
    let s = st::State {
        total: 0,
        f_z: Accumulator256::from(0),
        f_a: Accumulator256::from(0),
        custody_z: 0,
        custody_a: 0,
        zero_z: 0,
        zero_a: 0,
        positions: vec![],
    };
    let s = st::change(&s, id(1), st::Command::Stake(1), 1, 2, limits())
        .result()
        .unwrap()
        .state
        .clone();
    let s = st::change(&s, id(2), st::Command::Stake(1), 1, 2, limits())
        .result()
        .unwrap()
        .state
        .clone();
    let s = st::fee_event(&s, 1, 0, 2, limits())
        .result()
        .unwrap()
        .clone();
    assert_eq!(s.f_z, Accumulator256::from(Q / 2));
    let s = st::fee_event(&s, 1, 0, 2, limits())
        .result()
        .unwrap()
        .clone();
    let one = st::change(&s, id(1), st::Command::Claim, 0, 2, limits());
    assert_eq!(one.result().unwrap().zusd_gain, 1);
    let two = st::change(
        &one.result().unwrap().state,
        id(2),
        st::Command::Unstake(99),
        0,
        2,
        limits(),
    );
    assert_eq!(two.result().unwrap().zusd_gain, 1);
    assert_eq!(two.result().unwrap().stake_out, 1);
    assert_eq!(two.result().unwrap().state.custody_z, 0);
}
#[test]
fn d2_native_rounding_swap_and_tie_collapse() {
    for (debt, expected) in [(5, vec![id(2), id(1)]), (4, vec![id(1), id(2)])] {
        let pre = book(
            vec![vault(1, 3, 2, 1), vault(2, 4, 3, 1), vault(3, 2, debt, 1)],
            vec![id(1), id(2), id(3)],
        );
        let result = l::liquidate(
            &pre,
            &l::Command::Direct(id(3)),
            prices(),
            None,
            3,
            bounds(),
            limits(),
        );
        let o = result.result().unwrap();
        assert_eq!(o.book.order, expected);
        assert_eq!(o.book.vaults[0], pre.vaults[0]);
        assert_eq!(o.book.vaults[1], pre.vaults[1]);
        assert_eq!(o.book.default_collateral, 2);
        assert_eq!(o.book.default_debt, debt);
    }
}
#[test]
fn d4_native_batch_does_not_switch_to_normal_on_keeper_backing() {
    let mut pre = book(
        vec![
            vault(1, 210, 200, 1),
            vault(2, 110, 100, 1),
            vault(3, 189, 100, 1),
        ],
        vec![id(3), id(2), id(1)],
    );
    pre.pool.total = 300;
    for command in [l::Command::Batch(vec![id(1), id(2)]), l::Command::Lowest(3)] {
        let result = l::liquidate(&pre, &command, prices(), Some(0), 3, bounds(), limits());
        let o = result.result().unwrap();
        assert_eq!(o.selected, vec![id(1), id(2)]);
        assert_eq!(o.book.system.collateral, 189);
        assert_eq!(o.book.system.debt, 100);
        assert_eq!(o.keeper_collateral, 1);
        assert_eq!(o.burned, 300);
    }
    // Pinned tracker retains the first keeper atom: 300/200 is falsely Normal.
    assert_eq!((509 - 209) * Q / (400 - 200), v::CCR);
    assert!((509 - 209 - 1) * Q / (400 - 200) < v::CCR);
}
#[test]
fn d1_native_redemption_checks_each_floor_tie_and_full_first_prefix() {
    let h = NICR_Q;
    let d = 30 * h;
    let pre = book(
        vec![
            vault(1, 11 * h + 1, d, 1),
            vault(2, 11 * h - 1, d, 1),
            vault(3, 11 * h + 1, d, 1),
        ],
        vec![id(1), id(2), id(3)],
    );
    let command = r::Command {
        maximum: d - 1 + 3,
        visits: 0,
        first: Some(id(1)),
        partial_nicr: pre.priorities[0].1,
        upper: None,
        lower: None,
        max_fee: Q,
        min_out: 0,
    };
    let holder = r::Holder {
        zusd: command.maximum,
        agrs: 0,
        staking_agrs: 0,
        fee: fee(),
    };
    let p = v::Prices {
        committed: 300_000_000,
        ..prices()
    };
    let params = v::Parameters {
        minimum_net: 1,
        reserve: 1,
        ..v::Parameters::v1()
    };
    let result = r::redeem(
        r::RedeemInputs {
            pre: &pre,
            holder,
            command,
            prices: p,
            time: time(),
            bootstrap_minutes: Some(20160),
            parameters: params,
            capacity: 3,
            bounds: bounds(),
        },
        limits(),
    );
    let o = result.result().unwrap();
    assert_eq!(o.trace.visited, vec![id(3), id(2), id(1)]);
    assert_eq!(o.book.vaults[1], pre.vaults[1]);
    assert_eq!(o.trace.closed, vec![id(3)]);
    assert_eq!(o.trace.executed, command.maximum);
    assert_eq!(o.book.vaults[0].debt, d - 3);
}
#[test]
fn common_guards_earned_claim_frames_and_independent_wrong_outcomes() {
    assert_eq!(
        common::identity(id(1), Some(id(2)), 7, 0, limits()).result(),
        Err(Failure::Reject(1000))
    );
    assert_eq!(
        common::identity(id(1), Some(id(1)), 7, 0, limits()).result(),
        Err(Failure::Reject(1001))
    );
    assert_eq!(
        common::identity(id(1), None, u64::MAX, u64::MAX, limits()).result(),
        Err(Failure::Reject(1002))
    );
    assert_eq!(
        common::identity(id(1), None, 0, 0, limits()).result(),
        Ok(&1)
    );
    let rows = vec![(id(1), 10), (id(2), 11)];
    let result = common::claim_surplus(&rows, id(1), 2, limits());
    let c = result.result().unwrap();
    assert_eq!(c.amount, 10);
    assert_eq!(c.remaining, vec![(id(2), 11)]);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        common::project_collateral(10_000_000_001, 9, limits()).result(),
        Ok(&(1, 10))
    );
    let v = vault(1, 10, 10, 1);
    let s = v::System {
        reward_d: Accumulator256::from(Q),
        ..system()
    };
    let expected = v::apply_pending(v, s, limits()).result().copied();
    assert!(
        laws::check_pending(v, s, expected, limits())
            .result()
            .is_ok()
    );
    let wrong = v::Vault {
        owner: id(9),
        ..expected.unwrap()
    };
    assert_eq!(
        laws::check_pending(v, s, Ok(wrong), limits()).result(),
        Err(Failure::Law(2000))
    );
    let s = v::System {
        collateral: 20,
        debt: 20,
        active: 2,
        ..system()
    };
    let q = l::quote(v, s, prices(), 0, limits());
    let mut wrong = *q.result().unwrap();
    wrong.reserve += 1;
    assert_eq!(
        laws::check_quote(v, s, prices(), 0, Ok(wrong), limits()).result(),
        Err(Failure::Law(2001))
    );
    assert_eq!(
        laws::check_quote(v, s, prices(), 0, Err(Failure::Reject(1044)), limits()).result(),
        Err(Failure::Law(2001))
    );
}
#[test]
fn d1_all_four_vault_eligibility_patterns_preserve_ineligible_frames() {
    let h = NICR_Q;
    let debt = 30 * h;
    for pattern in 0u8..16 {
        let mut records = vec![vault(0, 20 * h, debt, 1)];
        for i in 1..=4 {
            records.push(vault(
                i,
                if pattern & (1 << (i - 1)) != 0 {
                    11 * h + 1
                } else {
                    11 * h - 1
                },
                debt,
                1,
            ));
        }
        let pre = book(records, (0..=4).map(id).collect());
        let maximum = u128::from(pattern.count_ones()) * (debt - 1) + 3;
        let partial_nicr = *mul_div(20 * h - 1, NICR_Q, debt - 3, limits())
            .result()
            .unwrap();
        let command = r::Command {
            maximum,
            visits: 0,
            first: Some(id(0)),
            partial_nicr,
            upper: None,
            lower: None,
            max_fee: Q,
            min_out: 0,
        };
        let holder = r::Holder {
            zusd: maximum,
            agrs: 0,
            staking_agrs: 0,
            fee: fees::State {
                base: Q / 10,
                last_epoch: 0,
            },
        };
        let result = r::redeem(
            r::RedeemInputs {
                pre: &pre,
                holder,
                command,
                prices: v::Prices {
                    committed: 300_000_000,
                    ..prices()
                },
                time: time(),
                bootstrap_minutes: Some(20160),
                parameters: v::Parameters {
                    minimum_net: 1,
                    reserve: 1,
                    ..v::Parameters::v1()
                },
                capacity: 5,
                bounds: sp::Bounds {
                    vaults: 5,
                    ..bounds()
                },
            },
            limits(),
        );
        let o = result.result().unwrap();
        assert_eq!(o.trace.executed, maximum, "pattern {pattern}");
        for i in 1..=4 {
            if pattern & (1 << (i - 1)) == 0 {
                assert_eq!(
                    o.book.vaults[i], pre.vaults[i],
                    "pattern {pattern}, below-MCR {i}"
                );
            }
        }
    }
}
#[test]
fn d4_capped_surplus_is_not_retained_in_recovery_backing() {
    let mut pre = book(
        vec![
            vault(1, 120, 100, 1),
            vault(2, 55, 50, 1),
            vault(3, 94, 50, 1),
        ],
        vec![id(3), id(1), id(2)],
    );
    pre.pool.total = 150;
    let result = l::liquidate(
        &pre,
        &l::Command::Batch(vec![id(1), id(2)]),
        prices(),
        Some(0),
        3,
        bounds(),
        limits(),
    );
    let o = result.result().unwrap();
    assert_eq!(o.selected, vec![id(1), id(2)]);
    assert_eq!(o.book.surplus, vec![(id(1), 10)]);
    assert_eq!(o.book.system.collateral, 94);
}
#[test]
fn owner_adjust_close_recovery_and_floor_cases() {
    let vault = vault(1, 2000 * Q, 1000 * Q, 50 * Q);
    let s = v::System {
        collateral: 4000 * Q,
        debt: 2000 * Q,
        active: 2,
        stakes: 4000 * Q,
        ..system()
    };
    let closed = v::close(vault, s, fee(), prices(), 950 * Q, limits());
    let c = closed.result().unwrap();
    assert_eq!(c.burned, 950 * Q);
    assert_eq!(c.reserve_burned, 50 * Q);
    assert_eq!(c.collateral_out, 2000 * Q);
    assert_eq!(c.vault.status, v::Status::ClosedOwner);
    assert_eq!(c.vault.parameter_version, 1);
    let repaid = v::adjust(
        v::AdjustInputs {
            vault,
            system: s,
            parameters: v::Parameters::v1(),
            fee: fee(),
            prices: prices(),
            collateral_delta: v::Delta::None,
            debt_delta: v::Delta::Remove(450 * Q),
            wallet: (0, 450 * Q),
            max_fee: Q,
            time: None,
        },
        limits(),
    );
    assert_eq!(repaid.result().unwrap().vault.debt, 550 * Q);
    assert_eq!(
        v::adjust(
            v::AdjustInputs {
                vault,
                system: s,
                parameters: v::Parameters::v1(),
                fee: fee(),
                prices: prices(),
                collateral_delta: v::Delta::None,
                debt_delta: v::Delta::Remove(450 * Q + 1),
                wallet: (0, 1000 * Q),
                max_fee: Q,
                time: None,
            },
            limits()
        )
        .result(),
        Err(Failure::Reject(1040))
    );
    assert_eq!(
        v::open(
            v::OpenInputs {
                existing: Some(vault),
                id: id(1),
                owner: id(1),
                collateral: 2000 * Q,
                principal: 500 * Q,
                balances: (2000 * Q, 0),
                system: s,
                parameters: v::Parameters::v1(),
                fee: fee(),
                prices: prices(),
                max_fee: Q,
                time: time(),
            },
            limits()
        )
        .result(),
        Err(Failure::Reject(1011))
    );
    let s = v::System {
        collateral: 2000 * Q,
        ..s
    };
    let recovered = v::adjust(
        v::AdjustInputs {
            vault,
            system: s,
            parameters: v::Parameters::v1(),
            fee: fee(),
            prices: prices(),
            collateral_delta: v::Delta::Add(500 * Q),
            debt_delta: v::Delta::Add(100 * Q),
            wallet: (500 * Q, 0),
            max_fee: 0,
            time: None,
        },
        limits(),
    );
    let o = recovered.result().unwrap();
    assert_eq!(o.fee_paid, 0);
    assert_eq!(o.fee, fee());
    assert_eq!(o.vault.debt, 1100 * Q);
    assert_eq!(
        v::adjust(
            v::AdjustInputs {
                vault,
                system: s,
                parameters: v::Parameters::v1(),
                fee: fee(),
                prices: prices(),
                collateral_delta: v::Delta::None,
                debt_delta: v::Delta::Add(100 * Q),
                wallet: (0, 0),
                max_fee: 0,
                time: None,
            },
            limits()
        )
        .result(),
        Err(Failure::Reject(1043))
    );
}
#[test]
fn sp_scale_residual_only_empty_claim_and_funded_issuance_bound() {
    let b = sp::Book {
        pool: sp::State::reference_genesis(sp::FUNDED_CAP).unwrap(),
        deposits: vec![],
        front_ends: vec![],
    };
    let dep = sp::change(
        sp::ChangeInputs {
            pre: &b,
            actor: id(1),
            command: sp::Command::Deposit {
                amount: 1000 * Q,
                tag: None,
            },
            wallet_zusd: 1000 * Q,
            issuance: Some(0),
            prices: prices(),
            inventory: &[],
            system: system(),
            bounds: bounds(),
        },
        limits(),
    );
    let mut b = dep.result().unwrap().book.clone();
    let off = sp::offset(&b.pool, 1000 * Q - 2000, Q, Some(0), bounds(), limits());
    b.pool = off.result().unwrap().clone();
    assert_eq!(b.pool.scale, 1);
    assert_eq!(b.pool.p, SCALE);
    let liabilities = sp::liabilities(&b, bounds(), limits());
    assert_eq!(liabilities.result().unwrap().residual_only, 2000);
    let claim = sp::change(
        sp::ChangeInputs {
            pre: &b,
            actor: id(1),
            command: sp::Command::Withdraw(u128::MAX),
            wallet_zusd: 0,
            issuance: Some(0),
            prices: prices(),
            inventory: &[],
            system: system(),
            bounds: bounds(),
        },
        limits(),
    );
    let o = claim.result().unwrap();
    assert_eq!(o.gains.collateral, Q);
    assert_eq!(o.principal_out, 0);
    assert!(o.book.deposits.is_empty());
    assert_eq!(o.book.pool.total, 2000);
    let fail = sp::offset(&b.pool, 1, 0, Some(35_000_000 * Q + 1), bounds(), limits());
    assert_eq!(fail.result(), Err(Failure::Conservation));
    let no_offset = sp::offset(&b.pool, 0, 0, None, bounds(), limits());
    assert_eq!(no_offset.result(), Ok(&b.pool));
}
#[test]
fn fee_rejection_classes_and_zero_stake_entitlements() {
    assert_eq!(
        fees::borrow(fee(), Q, false, 0, time(), limits()).result(),
        Err(Failure::Reject(1030))
    );
    assert_eq!(
        fees::borrow(
            fees::State {
                base: Q,
                last_epoch: 0
            },
            Q,
            false,
            fees::FLOOR,
            time(),
            limits()
        )
        .result(),
        Err(Failure::Reject(1052))
    );
    assert_eq!(
        fees::redemption(fee(), 1, 1, 1000 * Q, Q, time(), limits()).result(),
        Err(Failure::Reject(1050))
    );
    assert_eq!(
        fees::redemption(
            fees::State {
                base: Q,
                last_epoch: 0
            },
            Q,
            100_000_000,
            1000 * Q,
            Q,
            time(),
            limits()
        )
        .result(),
        Err(Failure::Reject(1051))
    );
    let pre = st::State {
        total: 0,
        f_z: Accumulator256::from(0),
        f_a: Accumulator256::from(0),
        custody_z: 0,
        custody_a: 0,
        zero_z: 0,
        zero_a: 0,
        positions: vec![],
    };
    let paid = st::fee_event(&pre, 5, 7, 1, limits())
        .result()
        .unwrap()
        .clone();
    assert_eq!(
        (paid.f_z, paid.f_a, paid.zero_z, paid.zero_a),
        (Accumulator256::ZERO, Accumulator256::ZERO, 5, 7)
    );
    let joined = st::change(&paid, id(1), st::Command::Stake(1), 1, 1, limits())
        .result()
        .unwrap()
        .state
        .clone();
    let claim = st::change(&joined, id(1), st::Command::Claim, 0, 1, limits());
    assert_eq!(
        (
            claim.result().unwrap().zusd_gain,
            claim.result().unwrap().agrs_gain
        ),
        (0, 0)
    );
    assert_eq!(claim.result().unwrap().state.custody_z, 5);
}
#[test]
fn native_refusal_retry_keeps_inputs_and_complete_work() {
    let pre = book(
        vec![vault(1, 90, 100, 1), vault(2, 90, 100, 1)],
        vec![id(1), id(2)],
    );
    let command = l::Command::Batch(vec![id(2), id(1)]);
    let baseline = l::liquidate(&pre, &command, prices(), None, 2, bounds(), limits());
    assert!(baseline.result().is_ok());
    let capped = Limits {
        steps: baseline.usage().steps() - 1,
        ..limits()
    };
    let frozen = pre.clone();
    let first = l::liquidate(&pre, &command, prices(), None, 2, bounds(), capped);
    let second = l::liquidate(&pre, &command, prices(), None, 2, bounds(), capped);
    assert!(matches!(first.result(), Err(Failure::Budget { .. })));
    assert_eq!(first.usage(), second.usage());
    assert_eq!(pre, frozen);
}
#[test]
fn redemption_full_partial_prefix_caps_and_last_vault() {
    let pre = book(
        vec![
            vault(1, 2000 * Q, 1000 * Q, 50 * Q),
            vault(2, 1500 * Q, 1000 * Q, 50 * Q),
            vault(3, 1200 * Q, 1000 * Q, 50 * Q),
        ],
        vec![id(1), id(2), id(3)],
    );
    let holder = r::Holder {
        zusd: 3000 * Q,
        agrs: 0,
        staking_agrs: 0,
        fee: fee(),
    };
    let command = r::Command {
        maximum: 1000 * Q,
        visits: 0,
        first: Some(id(1)),
        partial_nicr: 0,
        upper: None,
        lower: None,
        max_fee: Q,
        min_out: 0,
    };
    let result = r::redeem(
        r::RedeemInputs {
            pre: &pre,
            holder,
            command,
            prices: prices(),
            time: time(),
            bootstrap_minutes: Some(20160),
            parameters: v::Parameters::v1(),
            capacity: 3,
            bounds: bounds(),
        },
        limits(),
    );
    let o = result.result().unwrap();
    assert_eq!(o.trace.closed, vec![id(3)]);
    assert_eq!(o.trace.executed, 950 * Q);
    assert_eq!(o.trace.unfilled, 50 * Q);
    assert_eq!(o.trace.stop, r::Stop::CancelledPartial);
    assert_eq!(o.trace.reserve_burn, 50 * Q);
    assert_eq!(o.trace.settled, vec![id(3), id(2)]);
    assert_eq!(pre.system.active, 3);
    let all = r::Command {
        maximum: 2850 * Q,
        ..command
    };
    assert_eq!(
        r::redeem(
            r::RedeemInputs {
                pre: &pre,
                holder,
                command: all,
                prices: prices(),
                time: time(),
                bootstrap_minutes: Some(20160),
                parameters: v::Parameters::v1(),
                capacity: 3,
                bounds: bounds(),
            },
            limits()
        )
        .result(),
        Err(Failure::Reject(1041))
    );
    let limited = r::Command {
        visits: 1,
        ..command
    };
    assert_eq!(
        r::redeem(
            r::RedeemInputs {
                pre: &pre,
                holder,
                command: limited,
                prices: prices(),
                time: time(),
                bootstrap_minutes: Some(20160),
                parameters: v::Parameters::v1(),
                capacity: 3,
                bounds: bounds(),
            },
            limits()
        )
        .result()
        .unwrap()
        .trace
        .stop,
        r::Stop::VisitCap
    );
    assert_eq!(
        r::redeem(
            r::RedeemInputs {
                pre: &pre,
                holder,
                command,
                prices: prices(),
                time: None,
                bootstrap_minutes: None,
                parameters: v::Parameters::v1(),
                capacity: 3,
                bounds: bounds(),
            },
            limits()
        )
        .result(),
        Err(Failure::UnboundProfile)
    );
}

#[test]
fn common_reason_precedence_and_exact_pending_owner_outcome() {
    assert_eq!(
        fees::borrow(fee(), 0, false, 0, None, limits()).result(),
        Err(Failure::Reject(1010))
    );
    let params = v::Parameters::v1();
    let high_fee = fees::State {
        base: Q,
        last_epoch: 0,
    };
    let open = |collateral, principal, wallet, max_fee| {
        v::open(
            v::OpenInputs {
                existing: None,
                id: id(1),
                owner: id(1),
                collateral,
                principal,
                balances: (wallet, 0),
                system: system(),
                parameters: params,
                fee: high_fee,
                prices: prices(),
                max_fee,
                time: time(),
            },
            limits(),
        )
    };
    assert_eq!(
        open(1000 * Q, 500 * Q, 0, 0).result(),
        Err(Failure::Reject(1030))
    );
    assert_eq!(
        open(1000 * Q, 100 * Q, 1000 * Q, fees::FLOOR).result(),
        Err(Failure::Reject(1040))
    );
    assert_eq!(
        open(500 * Q, 500 * Q, 500 * Q, fees::FLOOR).result(),
        Err(Failure::Reject(1043))
    );
    let old = vault(1, 1000 * Q, 600 * Q, 50 * Q);
    let s = v::System {
        collateral: 4000 * Q,
        debt: 2000 * Q,
        active: 2,
        stakes: 2000 * Q,
        reward_c: Accumulator256::from(Q / 10),
        reward_d: Accumulator256::from(Q / 10),
        ..system()
    };
    let out = v::adjust(
        v::AdjustInputs {
            vault: old,
            system: s,
            parameters: params,
            fee: fee(),
            prices: prices(),
            collateral_delta: v::Delta::None,
            debt_delta: v::Delta::Remove(50 * Q),
            wallet: (0, 50 * Q),
            max_fee: 0,
            time: None,
        },
        limits(),
    );
    assert_eq!(
        out.result(),
        Ok(&v::Outcome {
            vault: v::Vault {
                collateral: 1100 * Q,
                debt: 650 * Q,
                stake: 1100 * Q,
                snapshot_c: Accumulator256::from(Q / 10),
                snapshot_d: Accumulator256::from(Q / 10),
                ..old
            },
            system: v::System {
                debt: 1950 * Q,
                stakes: 2100 * Q,
                ..s
            },
            fee: fee(),
            minted: 0,
            burned: 50 * Q,
            fee_paid: 0,
            collateral_in: 0,
            collateral_out: 0,
            reserve_minted: 0,
            reserve_burned: 0,
            pending_collateral: 100 * Q,
            pending_debt: 100 * Q,
            nicr: (1100 * NICR_Q) / 650,
        })
    );
    assert_eq!(
        v::adjust(
            v::AdjustInputs {
                vault: old,
                system: s,
                parameters: params,
                fee: fee(),
                prices: prices(),
                collateral_delta: v::Delta::Remove(2000 * Q),
                debt_delta: v::Delta::None,
                wallet: (0, 0),
                max_fee: 0,
                time: None,
            },
            limits()
        )
        .result(),
        Err(Failure::Reject(1033))
    );
    let pre = book(
        vec![vault(1, 90, 100, 1), vault(2, 90, 100, 1)],
        vec![id(1), id(2)],
    );
    let stale = v::Prices {
        status: v::PriceStatus::Stale,
        ..prices()
    };
    assert_eq!(
        l::liquidate(
            &pre,
            &l::Command::Batch(vec![]),
            stale,
            None,
            2,
            bounds(),
            limits()
        )
        .result(),
        Err(Failure::Reject(1010))
    );
    assert_eq!(
        l::liquidate(
            &pre,
            &l::Command::Direct(id(3)),
            stale,
            None,
            2,
            bounds(),
            limits()
        )
        .result(),
        Err(Failure::Reject(1011))
    );
    for (status, reason) in [
        (v::PriceStatus::Unavailable, 1020),
        (v::PriceStatus::Stale, 1021),
        (v::PriceStatus::Disputed, 1022),
        (v::PriceStatus::Uncertain, 1023),
        (v::PriceStatus::OverBudget, 1024),
    ] {
        let p = v::Prices { status, ..prices() };
        let candidate = l::quote(pre.vaults[0], pre.system, p, 0, limits());
        assert_eq!(candidate.result(), Err(Failure::Reject(reason)));
        assert_eq!(
            laws::check_quote(
                pre.vaults[0],
                pre.system,
                p,
                0,
                Err(Failure::Reject(reason)),
                limits()
            )
            .result(),
            Ok(&())
        );
    }
}

#[test]
fn withdrawal_requires_complete_inventory_and_redemption_guard_witnesses() {
    let pre = sp::Book {
        pool: sp::State::reference_genesis(sp::FUNDED_CAP).unwrap(),
        deposits: vec![],
        front_ends: vec![],
    };
    let dep = sp::change(
        sp::ChangeInputs {
            pre: &pre,
            actor: id(1),
            command: sp::Command::Deposit {
                amount: 10,
                tag: None,
            },
            wallet_zusd: 10,
            issuance: Some(0),
            prices: prices(),
            inventory: &[],
            system: system(),
            bounds: bounds(),
        },
        limits(),
    );
    let pre = &dep.result().unwrap().book;
    let omitted = v::System {
        active: 1,
        collateral: 1,
        debt: 1,
        stakes: 1,
        ..system()
    };
    assert_eq!(
        sp::change(
            sp::ChangeInputs {
                pre,
                actor: id(1),
                command: sp::Command::Withdraw(1),
                wallet_zusd: 0,
                issuance: Some(0),
                prices: prices(),
                inventory: &[],
                system: omitted,
                bounds: bounds(),
            },
            limits()
        )
        .result(),
        Err(Failure::Conservation)
    );
    assert_eq!(
        sp::change(
            sp::ChangeInputs {
                pre,
                actor: id(1),
                command: sp::Command::Deposit {
                    amount: 1,
                    tag: Some(id(99))
                },
                wallet_zusd: 1,
                issuance: None,
                prices: prices(),
                inventory: &[],
                system: system(),
                bounds: bounds(),
            },
            limits()
        )
        .result(),
        Err(Failure::Reject(1012))
    );
    let book = book(
        vec![
            vault(1, 2000 * Q, 1000 * Q, 50 * Q),
            vault(2, 1500 * Q, 1000 * Q, 50 * Q),
        ],
        vec![id(1), id(2)],
    );
    let holder = r::Holder {
        zusd: 1000 * Q,
        agrs: 0,
        staking_agrs: 0,
        fee: fee(),
    };
    let c = r::Command {
        maximum: 950 * Q,
        visits: 1,
        first: None,
        partial_nicr: 0,
        upper: None,
        lower: None,
        max_fee: Q,
        min_out: 0,
    };
    let invoke = |holder, command, price, age| {
        r::redeem(
            r::RedeemInputs {
                pre: &book,
                holder,
                command,
                prices: price,
                time: time(),
                bootstrap_minutes: age,
                parameters: v::Parameters::v1(),
                capacity: 2,
                bounds: bounds(),
            },
            limits(),
        )
    };
    assert_eq!(
        invoke(holder, c, prices(), Some(20159)).result(),
        Err(Failure::Reject(1031))
    );
    assert_eq!(
        invoke(
            holder,
            c,
            v::Prices {
                committed: 50_000_000,
                ..prices()
            },
            Some(20160)
        )
        .result(),
        Err(Failure::Reject(1032))
    );
    assert_eq!(
        invoke(r::Holder { zusd: 1, ..holder }, c, prices(), Some(20160)).result(),
        Err(Failure::Reject(1033))
    );
    assert_eq!(
        invoke(
            holder,
            r::Command { maximum: Q, ..c },
            prices(),
            Some(20160)
        )
        .result(),
        Err(Failure::Reject(1045))
    );
    assert_eq!(
        invoke(
            holder,
            r::Command {
                min_out: u128::MAX,
                ..c
            },
            prices(),
            Some(20160)
        )
        .result(),
        Err(Failure::Reject(1053))
    );
    assert!(invoke(holder, c, prices(), Some(20160)).result().is_ok());
}

#[test]
fn sp_offset_full_state_matches_independent_integer_reference() {
    assert_eq!(
        sp::State::reference_genesis(0),
        Err(Failure::UnboundProfile)
    );
    for line in include_str!("test-data/offset.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let a: Vec<u128> = fields[..5].iter().map(|s| s.parse().unwrap()).collect();
        let mut pre = sp::State::reference_genesis(sp::FUNDED_CAP).unwrap();
        pre.total = a[0];
        pre.p = a[1];
        let frozen = pre.clone();
        let out = sp::offset(&pre, a[2], a[3], Some(a[4]), bounds(), limits());
        assert_eq!(pre, frozen);
        if fields[5] == "refuse" {
            assert!(
                matches!(out.result(), Err(Failure::Arithmetic | Failure::Domain)),
                "{line}: {:?}",
                out.result()
            );
            continue;
        }
        let cells: Vec<_> = fields[5].split(',').collect();
        let e: Vec<u128> = cells
            .iter()
            .map(|s| {
                if s.contains(':') {
                    0
                } else {
                    s.parse().unwrap()
                }
            })
            .collect();
        let acc = |i: usize| {
            let (high, low) = cells[i].split_once(':').unwrap();
            Accumulator256::from_limbs(high.parse().unwrap(), low.parse().unwrap())
        };
        let mut expected = pre.clone();
        expected.total = e[0];
        expected.p = e[1];
        expected.epoch = e[2] as u64;
        expected.scale = e[3] as u64;
        expected.error_s = acc(4);
        expected.error_loss = acc(5);
        expected.error_g = acc(6);
        expected.collateral = e[7];
        expected.stock = sp::Stock {
            remaining: e[8],
            scheduled: e[9],
            allocated: e[10],
            missed: e[11],
            paid: e[12],
        };
        expected.history[0].s = acc(13);
        expected.history[0].g = acc(14);
        if expected.epoch == 1 {
            expected.epochs[0].sealed = true;
            expected.epochs.push(sp::Epoch {
                epoch: 1,
                last_scale: 0,
                sealed: false,
            });
            expected.history.push(sp::Cell {
                epoch: 1,
                scale: 0,
                s: Accumulator256::from(0),
                g: Accumulator256::from(0),
            });
        } else if expected.scale == 1 {
            expected.epochs[0].last_scale = 1;
            expected.history.push(sp::Cell {
                epoch: 0,
                scale: 1,
                s: Accumulator256::from(0),
                g: Accumulator256::from(0),
            });
        }
        assert_eq!(out.result(), Ok(&expected), "{line}");
    }
}

#[test]
fn gain_to_vault_is_one_pure_successor_and_late_refusal_discards_both() {
    let pool = sp::Book {
        pool: sp::State::reference_genesis(sp::FUNDED_CAP).unwrap(),
        deposits: vec![],
        front_ends: vec![],
    };
    let d = sp::change(
        sp::ChangeInputs {
            pre: &pool,
            actor: id(1),
            command: sp::Command::Deposit {
                amount: 10 * Q,
                tag: None,
            },
            wallet_zusd: 10 * Q,
            issuance: Some(0),
            prices: prices(),
            inventory: &[],
            system: system(),
            bounds: bounds(),
        },
        limits(),
    );
    let mut pool = d.result().unwrap().book.clone();
    pool.pool = sp::offset(&pool.pool, 5 * Q, 2 * Q, Some(0), bounds(), limits())
        .result()
        .unwrap()
        .clone();
    let mut pre = book(
        vec![
            vault(1, 1500 * Q, 1000 * Q, 50 * Q),
            vault(2, 1500 * Q, 1000 * Q, 50 * Q),
        ],
        vec![id(1), id(2)],
    );
    pre.pool = pool.pool.clone();
    let frozen = (pre.clone(), pool.clone());
    let invoke = |params, lim| {
        composition::move_gain_to_vault(
            composition::GainToVaultInputs {
                pre: &pre,
                pool: &pool,
                actor: id(1),
                parameters: params,
                fee: fee(),
                prices: prices(),
                issuance: Some(0),
                upper: None,
                lower: None,
                capacity: 2,
                bounds: bounds(),
            },
            lim,
        )
    };
    let out = invoke(v::Parameters::v1(), limits());
    let result = out.result().unwrap();
    let mut expected = pre.clone();
    expected.vaults[0].collateral += 2 * Q;
    expected.vaults[0].stake += 2 * Q;
    expected.system.collateral += 2 * Q;
    expected.system.stakes += 2 * Q;
    expected.pool = result.stability.pool.clone();
    expected.priorities[0].1 = 1502 * NICR_Q / 1000;
    assert_eq!(result.vaults, expected);
    assert_eq!(result.settlement.collateral, 2 * Q);
    assert_eq!(result.stability.pool.collateral, 0);
    assert_eq!(result.owner.pending_collateral, 0);
    assert_eq!(result.owner.pending_debt, 0);
    assert_eq!(result.stability.deposits[0].initial, 5 * Q - 10);
    assert_eq!((pre.clone(), pool.clone()), frozen);
    assert_eq!(
        invoke(
            v::Parameters {
                minimum_net: 2000 * Q,
                ..v::Parameters::v1()
            },
            limits()
        )
        .result(),
        Err(Failure::Reject(1040))
    );
    for lane in 0..3 {
        let used = out.usage();
        let mut budget = limits();
        match lane {
            0 => budget.steps = used.steps() - 1,
            1 => budget.reads = used.reads() - 1,
            _ => budget.bytes = used.bytes() - 1,
        };
        let a = invoke(v::Parameters::v1(), budget);
        let b = invoke(v::Parameters::v1(), budget);
        assert!(matches!(a.result(),Err(Failure::Budget{lane:l,..}) if l==lane));
        assert_eq!(a.usage(), b.usage());
        assert_eq!((pre.clone(), pool.clone()), frozen);
    }
    let frozen_prices = v::Prices {
        status: v::PriceStatus::Disputed,
        ..prices()
    };
    let freeze = composition::move_gain_to_vault(
        composition::GainToVaultInputs {
            pre: &pre,
            pool: &pool,
            actor: id(1),
            parameters: v::Parameters::v1(),
            fee: fee(),
            prices: frozen_prices,
            issuance: Some(0),
            upper: None,
            lower: None,
            capacity: 2,
            bounds: bounds(),
        },
        limits(),
    );
    assert_eq!(freeze.result(), out.result());
    for divisor in [4, 2] {
        let budget = Limits {
            steps: out.usage().steps() / divisor,
            ..limits()
        };
        let denied = invoke(v::Parameters::v1(), budget);
        assert!(
            matches!(denied.result(),Err(Failure::Budget{lane:0,limit,attempted}) if limit==budget.steps&&attempted>limit)
        );
        assert_eq!((pre.clone(), pool.clone()), frozen);
    }
}
