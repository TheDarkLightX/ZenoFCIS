use zenofcis_zusd_v1::{Accumulator256, Limits, Q, ledger::*, staking};
fn limits() -> Limits {
    Limits {
        steps: 100_000_000,
        reads: 100_000,
        bytes: 100_000_000,
    }
}
#[test]
fn ledger_debit_matches_the_exact_asset_transfer() {
    let a = [1; 32];
    let b = [2; 32];
    let pre = Sheet {
        balances: vec![
            Balance {
                account: a,
                quantity: Quantity::Agrs(10),
            },
            Balance {
                account: b,
                quantity: Quantity::Agrs(10),
            },
        ],
        supply: 0,
        active_debt: 0,
        default_debt: 0,
    };
    let movement = Movement {
        from: Some(a),
        to: Some(b),
        quantity: Quantity::Agrs(5),
    };
    let out = compose(&pre, &[movement], (0, 0), 8, limits());
    let plan = out.result().expect("legitimate transfer must accept");
    assert_eq!(
        plan.post().balances[0].quantity,
        Quantity::Agrs(5),
        "LEDGER_DEBIT_MISSING"
    );
    assert_eq!(
        plan.post().balances[1].quantity,
        Quantity::Agrs(15),
        "ASSET_SWAP"
    );
    assert_eq!(pre.balances[0].quantity, Quantity::Agrs(10));
}
#[test]
fn staking_reward_is_consumed_once_with_other_claims_outstanding() {
    let pre = staking::State {
        total: 0,
        f_z: Accumulator256::ZERO,
        f_a: Accumulator256::ZERO,
        custody_z: 0,
        custody_a: 0,
        zero_z: 0,
        zero_a: 0,
        positions: vec![],
    };
    let a = staking::change(&pre, [1; 32], staking::Command::Stake(Q), Q, 4, limits())
        .into_parts()
        .0
        .unwrap()
        .state;
    let b = staking::change(&a, [2; 32], staking::Command::Stake(Q), Q, 4, limits())
        .into_parts()
        .0
        .unwrap()
        .state;
    let funded = staking::fee_event(&b, 100 * Q, 100 * Q, 4, limits())
        .into_parts()
        .0
        .unwrap();
    let first = staking::change(&funded, [1; 32], staking::Command::Claim, 0, 4, limits())
        .into_parts()
        .0
        .unwrap();
    assert_eq!((first.zusd_gain, first.agrs_gain), (50 * Q, 50 * Q));
    let second = staking::change(
        &first.state,
        [1; 32],
        staking::Command::Claim,
        0,
        4,
        limits(),
    )
    .into_parts()
    .0
    .expect("repeat claim may return zero");
    assert_eq!(
        (second.zusd_gain, second.agrs_gain),
        (0, 0),
        "REWARD_CLAIMED_TWICE"
    );
    assert_eq!(
        (second.state.custody_z, second.state.custody_a),
        (50 * Q, 50 * Q)
    );
}

#[test]
fn ledger_capacity_refusals_preserve_both_accounts() {
    let a = [1; 32];
    let b = [2; 32];
    let c = [3; 32];
    let pre = Sheet {
        balances: vec![
            Balance {
                account: a,
                quantity: Quantity::Agrs(10),
            },
            Balance {
                account: b,
                quantity: Quantity::Agrs(10),
            },
        ],
        supply: 0,
        active_debt: 0,
        default_debt: 0,
    };
    let frozen = pre.clone();
    let leg = Movement {
        from: Some(a),
        to: Some(c),
        quantity: Quantity::Agrs(5),
    };
    assert_eq!(
        compose(&pre, &[leg], (0, 0), 2, limits()).result().err(),
        Some(zenofcis_zusd_v1::Failure::Reject(1061))
    );
    assert_eq!(
        compose(&pre, &[leg; 3], (0, 0), 2, limits()).result().err(),
        Some(zenofcis_zusd_v1::Failure::Capacity)
    );
    assert_eq!(pre, frozen);
}

#[test]
fn invalid_fee_domains_and_logical_times_have_no_value_result() {
    use zenofcis_zusd_v1::{Failure, fees};
    let state = fees::State {
        base: 0,
        last_epoch: 2,
    };
    let time = Some(fees::LogicalTime {
        epoch: 2,
        elapsed_minutes: 0,
    });
    let bad = fees::State {
        base: Q + 1,
        ..state
    };
    assert_eq!(
        fees::borrow(bad, Q, false, Q, time, limits())
            .result()
            .err(),
        Some(Failure::Domain)
    );
    for max in [0, Q + 1] {
        assert_eq!(
            fees::borrow(state, Q, false, max, time, limits())
                .result()
                .err(),
            Some(Failure::Reject(1030))
        );
        assert_eq!(
            fees::redemption(state, Q, 100_000_000, 100 * Q, max, time, limits())
                .result()
                .err(),
            Some(Failure::Reject(1030))
        );
    }
    for invalid in [
        fees::LogicalTime {
            epoch: 1,
            elapsed_minutes: 0,
        },
        fees::LogicalTime {
            epoch: 2,
            elapsed_minutes: 1,
        },
    ] {
        assert_eq!(
            fees::borrow(state, Q, false, Q, Some(invalid), limits())
                .result()
                .err(),
            Some(Failure::Domain)
        );
    }
    assert_eq!(
        fees::redemption(bad, Q, 100_000_000, 100 * Q, Q, time, limits())
            .result()
            .err(),
        Some(Failure::Domain)
    );
    assert_eq!(
        fees::redemption(state, 0, 100_000_000, 100 * Q, Q, time, limits())
            .result()
            .err(),
        Some(Failure::Reject(1045))
    );
    for (price, debt) in [(0, 100 * Q), (100_000_000, 0)] {
        assert_eq!(
            fees::redemption(state, Q, price, debt, Q, time, limits())
                .result()
                .err(),
            Some(Failure::Domain)
        );
    }
}
