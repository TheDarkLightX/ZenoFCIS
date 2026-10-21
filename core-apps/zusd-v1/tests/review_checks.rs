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
fn review_liquidation_threshold_atom_matrix() {
    let d = 1000 * Q;
    let sd = 3000 * Q;
    let mut rows = 0;
    // Each ratio threshold has an exact collateral value; test that value and adjacent atoms.
    for threshold in [Q, v::MCR] {
        for dc in [-1i128, 0, 1] {
            let c = (threshold * (d / Q)).checked_add_signed(dc).unwrap();
            for t in [v::MCR, v::CCR] {
                for sc in [-1i128, 0, 1] {
                    let sysc = (t * (sd / Q)).checked_add_signed(sc).unwrap();
                    let s = v::System {
                        collateral: sysc,
                        debt: sd,
                        active: 3,
                        stakes: sysc,
                        ..system()
                    };
                    for available in [0, d / 2, d] {
                        let icr = c / (d / Q);
                        let tcr = sysc / (sd / Q);
                        let rec = tcr < v::CCR;
                        let actual =
                            l::quote(vault(1, c, d, 50 * Q), s, prices(), available, limits());
                        let eligible = if rec {
                            icr < v::MCR || (icr < tcr && available >= d)
                        } else {
                            icr < v::MCR
                        };
                        if !eligible {
                            assert_eq!(actual.result(), Err(Failure::Reject(1044)));
                        } else {
                            let a = actual.result().unwrap();
                            let (offset, cap) = if rec && icr >= v::MCR {
                                (d, d * 11 / 10)
                            } else {
                                (if rec && icr <= Q { 0 } else { d.min(available) }, c)
                            };
                            let keeper = cap / 200;
                            let sp = if offset == 0 {
                                0
                            } else {
                                (cap - keeper) / (d / offset)
                            };
                            assert_eq!(
                                (a.offset, a.sp_collateral, a.keeper_collateral, a.surplus),
                                (offset, sp, keeper, c - cap)
                            );
                            assert_eq!(a.offset + a.redistributed_debt, d);
                            assert_eq!(
                                a.sp_collateral
                                    + a.redistributed_collateral
                                    + a.keeper_collateral
                                    + a.surplus,
                                c
                            );
                        }
                        rows += 1;
                    }
                }
            }
        }
    }
    assert_eq!(rows, 108);
}
#[test]
fn review_minimum_open_adjacent_atoms() {
    let min_principal = (500 * Q * 200 + 200) / 201;
    for rec in [false, true] {
        for delta in [-1i128, 0, 1] {
            let x = (if rec { 500 * Q } else { min_principal })
                .checked_add_signed(delta)
                .unwrap();
            let s = if rec {
                v::System {
                    collateral: 1400 * Q,
                    debt: 1000 * Q,
                    active: 2,
                    stakes: 1400 * Q,
                    ..system()
                }
            } else {
                system()
            };
            let actual = v::open(
                v::OpenInputs {
                    existing: None,
                    id: id(3),
                    owner: id(3),
                    collateral: 2000 * Q,
                    principal: x,
                    balances: (2000 * Q, 0),
                    system: s,
                    parameters: v::Parameters::v1(),
                    fee: fee(),
                    prices: prices(),
                    max_fee: Q,
                    time: time(),
                },
                limits(),
            );
            if delta < 0 {
                assert_eq!(actual.result(), Err(Failure::Reject(1040)));
            } else {
                let a = actual.result().unwrap();
                let f = if rec { 0 } else { x / 200 };
                assert_eq!(a.vault.debt, x + f + 50 * Q);
                assert_eq!(
                    a.system.debt - s.debt,
                    a.minted + a.fee_paid + a.reserve_minted
                );
            }
        }
    }
}
#[test]
fn review_redemption_tcr_adjacent_atoms() {
    for delta in [-1i128, 0, 1] {
        // Both vaults eligible at the boundary; below it the pre-TCR rule must win.
        let c = (1100 * Q).checked_add_signed(delta).unwrap();
        let order = if delta < 0 {
            vec![id(2), id(1)]
        } else {
            vec![id(1), id(2)]
        };
        let pre = book(
            vec![
                vault(1, c, 1000 * Q, 50 * Q),
                vault(2, 1100 * Q, 1000 * Q, 50 * Q),
            ],
            order,
        );
        let cmd = r::Command {
            maximum: 100 * Q,
            visits: 0,
            first: None,
            partial_nicr: 1000 * NICR_Q / 900,
            upper: None,
            lower: None,
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
                command: cmd,
                prices: prices(),
                time: time(),
                bootstrap_minutes: Some(20160),
                parameters: v::Parameters::v1(),
                capacity: 4,
                bounds: bounds(),
            },
            limits(),
        );
        if delta < 0 {
            assert_eq!(out.result(), Err(Failure::Reject(1032)));
        } else {
            assert_eq!(out.result().unwrap().trace.executed, 100 * Q);
        }
    }
}
#[test]
fn review_close_remaining_tcr_adjacent_atoms() {
    for delta in [-1i128, 0, 1] {
        let c = (3500 * Q).checked_add_signed(delta).unwrap();
        let s = v::System {
            collateral: c,
            debt: 2000 * Q,
            active: 2,
            stakes: c,
            ..system()
        };
        let out = v::close(
            vault(1, 2000 * Q, 1000 * Q, 50 * Q),
            s,
            fee(),
            prices(),
            950 * Q,
            limits(),
        );
        if delta < 0 {
            assert_eq!(out.result(), Err(Failure::Reject(1043)));
        } else {
            let a = out.result().unwrap();
            assert_eq!(s.debt - a.system.debt, a.burned + a.reserve_burned);
        }
    }
}
