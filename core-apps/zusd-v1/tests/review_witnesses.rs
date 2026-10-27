use zenofcis_zusd_v1::{fees, liquidation as l, redemption as r, stability as sp, vaults as v, *};
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
fn review_cut_self_hint_keeps_v1_tie_direction() {
    let pre = book(
        vec![
            vault(1, 1100 * Q, 550 * Q, 50 * Q),
            vault(2, 1200 * Q, 650 * Q, 50 * Q),
        ],
        vec![id(1), id(2)],
    );
    let command = r::Command {
        maximum: 100 * Q,
        visits: 0,
        first: None,
        partial_nicr: 2 * NICR_Q,
        upper: Some(id(1)),
        lower: Some(id(2)),
        max_fee: Q,
        min_out: 0,
    };
    let out = r::redeem(
        r::RedeemInputs {
            pre: &pre,
            holder: r::Holder {
                zusd: 100 * Q,
                agrs: 0,
                staking_agrs: 0,
                fee: fee(),
            },
            command,
            prices: prices(),
            time: time(),
            bootstrap_minutes: Some(20160),
            parameters: v::Parameters::v1(),
            capacity: 4,
            bounds: bounds(),
        },
        limits(),
    );
    let out = out.result().unwrap();
    println!(
        "executed={} post_c={} post_d={} order={:?}",
        out.trace.executed,
        out.book.vaults[1].collateral,
        out.book.vaults[1].debt,
        out.book.order.iter().map(|id| id[31]).collect::<Vec<_>>()
    );
    // SortedTroves.sol:124 checks the original (A,B) pair after B is removed.
    // It is invalid. :388 discards B; :401 descends from A; :327 places B BEFORE A.
    assert_eq!(out.book.order, vec![id(2), id(1)]);
}
#[test]
fn review_zero_front_end_native_snapshot_normalization() {
    let pre = sp::Book {
        pool: sp::State::reference_genesis(sp::FUNDED_CAP).unwrap(),
        deposits: vec![],
        front_ends: vec![],
    };
    let out = sp::change(
        sp::ChangeInputs {
            pre: &pre,
            actor: id(1),
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
    let out = out.result().unwrap();
    println!("front={:?}", out.book.front_ends[0]);
    // Frozen RELATION.md:9 requires positive P0. This native representation
    // differs from V1 zero storage, but zero stake returns zero gains in either.
    assert_eq!(
        out.book.front_ends[0].snapshot,
        sp::FrontSnapshot {
            p: Q,
            g: Accumulator256::from(0),
            epoch: 0,
            scale: 0
        }
    );
}

#[test]
fn review_two_scale_gain_uses_wide_sum() {
    let mut pool = sp::State::reference_genesis(sp::FUNDED_CAP).unwrap();
    pool.total = 1;
    pool.collateral = 340;
    pool.p = Q / 2;
    pool.scale = 1;
    pool.history = vec![
        sp::Cell {
            epoch: 0,
            scale: 0,
            s: Accumulator256::from(u128::MAX - 100),
            g: Accumulator256::from(0),
        },
        sp::Cell {
            epoch: 0,
            scale: 1,
            s: Accumulator256::from(101 * SCALE),
            g: Accumulator256::from(0),
        },
    ];
    pool.epochs[0].last_scale = 1;
    let pre = sp::Book {
        pool,
        deposits: vec![sp::Deposit {
            owner: id(1),
            initial: 1,
            tag: None,
            snapshot: sp::Snapshot {
                p: Q,
                s: Accumulator256::from(0),
                g: Accumulator256::from(0),
                epoch: 0,
                scale: 0,
            },
        }],
        front_ends: vec![],
    };
    // Pinned SP:675 sums the two portions before division. The sum is 2^128,
    // but the final gain is floor(2^128 / 10^36) = 340, and custody is exactly 340.
    let out = sp::change(
        sp::ChangeInputs {
            pre: &pre,
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
    println!("two_scale_claim={:?}", out.result());
    let out = out.result().unwrap();
    assert_eq!(out.gains.collateral, 340);
    assert_eq!(out.book.pool.collateral, 0);
    assert_eq!(out.book.pool.total, 1);
    assert!(out.book.deposits.is_empty());
}

#[test]
fn review_two_scale_rewards_keep_wide_depositor_and_front_end_sums() {
    let mut pool = sp::State::reference_genesis(sp::FUNDED_CAP).unwrap();
    pool.total = 1;
    pool.p = Q / 2;
    pool.scale = 1;
    pool.stock.remaining -= 340;
    pool.stock.scheduled = 340;
    pool.stock.allocated = 340;
    pool.history = vec![
        sp::Cell {
            epoch: 0,
            scale: 0,
            s: Accumulator256::ZERO,
            g: Accumulator256::from(u128::MAX - 100),
        },
        sp::Cell {
            epoch: 0,
            scale: 1,
            s: Accumulator256::ZERO,
            g: Accumulator256::from(101 * SCALE),
        },
    ];
    pool.epochs[0].last_scale = 1;
    let pre = sp::Book {
        pool,
        deposits: vec![sp::Deposit {
            owner: id(1),
            initial: 1,
            tag: Some(id(2)),
            snapshot: sp::Snapshot {
                p: Q,
                s: Accumulator256::ZERO,
                g: Accumulator256::ZERO,
                epoch: 0,
                scale: 0,
            },
        }],
        front_ends: vec![sp::FrontEnd {
            owner: id(2),
            kickback: Q / 2,
            initial: 1,
            snapshot: sp::FrontSnapshot {
                p: Q,
                g: Accumulator256::ZERO,
                epoch: 0,
                scale: 0,
            },
        }],
    };
    let out = sp::change(
        sp::ChangeInputs {
            pre: &pre,
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
    )
    .into_parts()
    .0
    .unwrap();
    assert_eq!(
        (out.gains.depositor_zdex, out.gains.front_end_zdex),
        (170, 170)
    );
    assert_eq!(out.book.pool.stock.paid, 340);
}
