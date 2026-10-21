//! Refusal witnesses for public money boundaries, without bypassing guards.
#![cfg(feature = "std")]
#[path = "support/hmac.rs"]
mod issuer;
#[allow(dead_code)]
#[path = "support/step4.rs"]
mod support;
use support::*;
use zenofcis_zusd_v1::{
    Accumulator256 as A, Failure as F, Q, application::*, ledger::Quantity, liquidation as l,
    stability as sp, staking as st, vaults as v,
};
fn bounds() -> sp::Bounds {
    sp::Bounds {
        history: 32,
        epochs: 16,
        deposits: 8,
        front_ends: 8,
        vaults: 8,
    }
}
fn prices() -> v::Prices {
    v::Prices {
        committed: 200_000_000,
        pending: None,
        status: v::PriceStatus::Usable,
        pending_blocked: false,
    }
}
fn empty() -> sp::Book {
    sp::Book {
        pool: sp::State::reference_genesis(sp::FUNDED_CAP).unwrap(),
        deposits: vec![],
        front_ends: vec![],
    }
}
fn deposited() -> sp::Book {
    let p = policy(false);
    let s = ready(&p);
    sp::change(
        sp::ChangeInputs {
            pre: &empty(),
            actor: id(1),
            command: sp::Command::Deposit {
                amount: Q,
                tag: None,
            },
            wallet_zusd: Q,
            issuance: Some(0),
            prices: prices(),
            inventory: &[],
            system: s.book().system,
            bounds: bounds(),
        },
        limits(),
    )
    .into_parts()
    .0
    .unwrap()
    .book
}
fn sp_error(b: &sp::Book, expected: F) {
    let before = b.clone();
    assert_eq!(
        sp::liabilities(b, bounds(), limits()).result().err(),
        Some(expected)
    );
    assert_eq!(*b, before);
}
#[test]
fn pool_rejects_each_malformed_custody_and_snapshot_shape() {
    let base = deposited();
    for case in 0..29 {
        let mut b = base.clone();
        let mut expected = F::Domain;
        match case {
            0 => {
                b.pool.history.resize(33, b.pool.history[0]);
                expected = F::Capacity;
            }
            1 => {
                b.pool.epochs.resize(17, b.pool.epochs[0]);
                expected = F::Capacity;
            }
            2 => b.pool.p = 0,
            3 => b.pool.p = Q + 1,
            4 => b.pool.epochs.clear(),
            5 => b.pool.epochs[0].epoch = 1,
            6 => b.pool.epochs[0].sealed = true,
            7 => b.pool.epochs[0].last_scale = 1,
            8 => {
                b.pool.history[0].epoch = 1;
                expected = F::Canonical;
            }
            9 => {
                b.pool.history[0].scale = 1;
                expected = F::Canonical;
            }
            10 => b.pool.history.push(b.pool.history[0]),
            11 => b.pool.scale = 1,
            12 => {
                b.pool.stock.scheduled = 1;
                expected = F::Conservation;
            }
            13 => {
                b.pool.stock.remaining -= 1;
                expected = F::Conservation;
            }
            14 => {
                b.pool.stock.paid = 1;
                expected = F::Conservation;
            }
            15 => {
                b.deposits.resize(9, b.deposits[0]);
                expected = F::Capacity;
            }
            16 => {
                b.front_ends.resize(
                    9,
                    sp::FrontEnd {
                        owner: id(5),
                        kickback: Q,
                        initial: 0,
                        snapshot: sp::FrontSnapshot {
                            p: Q,
                            g: A::ZERO,
                            epoch: 0,
                            scale: 0,
                        },
                    },
                );
                expected = F::Capacity;
            }
            17 => {
                b.deposits.push(b.deposits[0]);
                expected = F::Canonical;
            }
            18 => {
                let f = sp::FrontEnd {
                    owner: id(5),
                    kickback: Q,
                    initial: 0,
                    snapshot: sp::FrontSnapshot {
                        p: Q,
                        g: A::ZERO,
                        epoch: 0,
                        scale: 0,
                    },
                };
                b.front_ends = vec![f, f];
                expected = F::Canonical;
            }
            19 => b.front_ends.push(sp::FrontEnd {
                owner: id(5),
                kickback: Q + 1,
                initial: 0,
                snapshot: sp::FrontSnapshot {
                    p: Q,
                    g: A::ZERO,
                    epoch: 0,
                    scale: 0,
                },
            }),
            20 => b.front_ends.push(sp::FrontEnd {
                owner: id(1),
                kickback: Q,
                initial: 0,
                snapshot: sp::FrontSnapshot {
                    p: Q,
                    g: A::ZERO,
                    epoch: 0,
                    scale: 0,
                },
            }),
            21 => b.deposits[0].initial = 0,
            22 => b.deposits[0].snapshot.p = 0,
            23 => b.deposits[0].snapshot.p = Q + 1,
            24 => {
                b.front_ends.push(sp::FrontEnd {
                    owner: id(5),
                    kickback: Q,
                    initial: 2 * Q,
                    snapshot: sp::FrontSnapshot {
                        p: Q,
                        g: A::ZERO,
                        epoch: 0,
                        scale: 0,
                    },
                });
                expected = F::Conservation;
            }
            25 => {
                b.deposits[0].initial += 1;
                expected = F::Conservation;
            }
            26 => {
                b.pool.history[0].s = A::from(Q * Q);
                expected = F::Conservation;
            }
            27 => {
                b.pool.history[0].g = A::from(Q * Q);
                expected = F::Conservation;
            }
            28 => b.deposits[0].tag = Some(id(7)),
            _ => unreachable!(),
        }
        sp_error(&b, expected);
    }
    let mut scaled = base.clone();
    scaled.pool.scale = 2;
    scaled.pool.epochs[0].last_scale = 2;
    for scale in 1..=2 {
        scaled.pool.history.push(sp::Cell {
            epoch: 0,
            scale,
            s: A::ZERO,
            g: A::ZERO,
        });
    }
    assert_eq!(
        sp::liabilities(&scaled, bounds(), limits())
            .result()
            .unwrap()
            .residual_only,
        Q
    );
    for (pre, debt, b, expected) in [
        (base.pool.clone(), Q + 1, bounds(), F::Domain),
        (
            base.pool.clone(),
            Q,
            sp::Bounds {
                epochs: 1,
                ..bounds()
            },
            F::Reject(1061),
        ),
        (
            base.pool.clone(),
            Q,
            sp::Bounds {
                history: 1,
                ..bounds()
            },
            F::Reject(1061),
        ),
        (
            sp::State {
                p: 1,
                ..base.pool.clone()
            },
            Q / 2,
            sp::Bounds {
                history: 1,
                ..bounds()
            },
            F::Reject(1061),
        ),
        (
            sp::State {
                p: 1,
                ..base.pool.clone()
            },
            Q - 2,
            bounds(),
            F::Arithmetic,
        ),
    ] {
        let before = pre.clone();
        assert_eq!(
            sp::offset(&pre, debt, 0, Some(0), b, limits())
                .result()
                .err(),
            Some(expected)
        );
        assert_eq!(pre, before);
    }
}
#[test]
fn staking_rejects_each_malformed_claim_bound() {
    let base = st::State {
        total: 1,
        f_z: A::ZERO,
        f_a: A::ZERO,
        custody_z: 0,
        custody_a: 0,
        zero_z: 0,
        zero_a: 0,
        positions: vec![st::Position {
            owner: id(1),
            stake: 1,
            snapshot_z: A::ZERO,
            snapshot_a: A::ZERO,
        }],
    };
    for case in 0..7 {
        let mut s = base.clone();
        let expected = match case {
            0 => {
                s.positions.resize(9, s.positions[0]);
                F::Capacity
            }
            1 => {
                s.positions.push(s.positions[0]);
                F::Canonical
            }
            2 => {
                s.positions[0].stake = 0;
                F::Domain
            }
            3 => {
                s.total = 2;
                F::Conservation
            }
            4 => {
                s.f_z = A::from(Q);
                F::Conservation
            }
            5 => {
                s.f_a = A::from(Q);
                F::Conservation
            }
            _ => {
                s.zero_z = 1;
                F::Conservation
            }
        };
        let frozen = s.clone();
        assert_eq!(
            st::fee_event(&s, 0, 0, 8, limits()).result().err(),
            Some(expected)
        );
        assert_eq!(s, frozen);
    }
}
fn book() -> l::Book {
    let p = policy(false);
    let mut s = ready(&p);
    open(&p, &mut s, 1, 1200 * Q, 1000 * Q);
    open(&p, &mut s, 2, 3000 * Q, 1000 * Q);
    s.book().clone()
}
fn liquid_error(b: &l::Book, expected: F) {
    let before = b.clone();
    assert_eq!(
        l::liquidate(
            b,
            &l::Command::Lowest(1),
            prices(),
            None,
            8,
            bounds(),
            limits()
        )
        .result()
        .err(),
        Some(expected)
    );
    assert_eq!(*b, before);
}
#[test]
fn liquidation_rejects_each_malformed_inventory_and_partition_input() {
    let base = book();
    for case in 0..25 {
        let mut b = base.clone();
        let mut expected = F::Domain;
        match case {
            0 => {
                b.vaults.resize(9, b.vaults[0]);
                expected = F::Capacity
            }
            1 => {
                b.order.resize(9, id(1));
                expected = F::Capacity
            }
            2 => {
                b.priorities.resize(9, (id(1), 1));
                expected = F::Capacity
            }
            3 => {
                b.surplus.resize(9, (id(1), 1));
                expected = F::Capacity
            }
            4 => {
                b.vaults.swap(0, 1);
                expected = F::Canonical
            }
            5 => {
                b.priorities.swap(0, 1);
                expected = F::Canonical
            }
            6 => {
                b.surplus = vec![(id(1), 1), (id(1), 2)];
                expected = F::Canonical
            }
            7 | 8 | 9 => {
                let v = &mut b.vaults[0];
                v.status = v::Status::ClosedOwner;
                v.collateral = 0;
                v.debt = 0;
                v.reserve = 0;
                v.stake = 0;
                v.snapshot_c = A::ZERO;
                v.snapshot_d = A::ZERO;
                if case == 7 {
                    v.snapshot_c = A::from(1);
                }
                if case == 8 {
                    v.snapshot_d = A::from(1);
                }
                if case == 9 {
                    v.collateral = 1;
                }
            }
            10 => {
                b.order.pop();
            }
            11 => {
                b.priorities.pop();
            }
            12 => b.system.active += 1,
            13 => b.vaults[1].owner = b.vaults[0].owner,
            14 => b.vaults[0].reserve = b.vaults[0].debt,
            15 => b.order[1] = b.order[0],
            16 => b.priorities[0].0 = id(1),
            17 => b.priorities[0].1 += 1,
            18 => {
                b.system.reward_c = A::from(1);
                expected = F::Conservation
            }
            19 => {
                b.system.reward_d = A::from(1);
                expected = F::Conservation
            }
            20 => {
                b.system.collateral += 1;
                expected = F::Conservation
            }
            21 => {
                b.system.debt += 1;
                expected = F::Conservation
            }
            22 => {
                b.system.stakes += 1;
                expected = F::Conservation
            }
            23 => {
                b.gas_balance = 0;
                expected = F::Conservation
            }
            24 => b.order.swap(0, 1),
            _ => unreachable!(),
        }
        if case == 18 || case == 19 {
            for (vid, priority) in &mut b.priorities {
                let v = b.vaults.iter().find(|v| v.id == *vid).unwrap();
                let e = *v::apply_pending(*v, b.system, limits()).result().unwrap();
                *priority = *A::from(e.collateral)
                    .multiply_divide(zenofcis_zusd_v1::NICR_Q, e.debt, limits())
                    .result()
                    .unwrap();
            }
        }
        liquid_error(&b, expected);
    }
    let v = base.vaults[0];
    assert_eq!(
        l::quote(
            v::Vault {
                status: v::Status::ClosedOwner,
                ..v
            },
            base.system,
            prices(),
            0,
            limits()
        )
        .result()
        .err(),
        Some(F::Reject(1011))
    );
    assert_eq!(
        l::quote(
            v,
            v::System {
                active: 1,
                ..base.system
            },
            prices(),
            0,
            limits()
        )
        .result()
        .err(),
        Some(F::Reject(1041))
    );
    assert_eq!(
        l::liquidate(
            &base,
            &l::Command::Batch(vec![id(1); 9]),
            prices(),
            None,
            8,
            bounds(),
            limits()
        )
        .result()
        .err(),
        Some(F::Capacity)
    );
}
fn refused(p: &Policy, s: &State, mut r: Request, expected: F) {
    let frozen = s.clone();
    if let Ok(signed) = sign(p, s, r.clone()) {
        r = signed;
    }
    assert_eq!(
        decide(p, s, &r, &TestVerifier, limits()).result().err(),
        Some(expected),
        "{:?}",
        r.command
    );
    assert_eq!(*s, frozen);
}
#[test]
fn application_rejects_authenticated_request_and_receipt_mismatches() {
    let p = policy(false);
    let s = ready(&p);
    for case in 0..20 {
        let mut r = raw(&s, 1, Command::AdvanceClock);
        let mut expected = F::Canonical;
        match case {
            0 => {
                r.proofs.resize(
                    10,
                    Proof {
                        domain: Domain::Actor,
                        bytes: vec![],
                    },
                );
                expected = F::Capacity
            }
            1 => {
                r.proofs = vec![Proof {
                    domain: Domain::Actor,
                    bytes: vec![0; 257],
                }];
                expected = F::Capacity
            }
            2 => {
                r.drip = Some(Drip {
                    event: id(230),
                    amount: 0,
                    next: vec![0; 257],
                });
                expected = F::Capacity
            }
            3 => {
                r.command = Command::Liquidate(l::Command::Batch(vec![id(1); 9]));
                expected = F::Capacity
            }
            4 => {
                r.command = Command::ReleaseExposure {
                    events: vec![id(1); 33],
                };
                expected = F::Capacity
            }
            5 => {
                r.drip = Some(Drip {
                    event: id(230),
                    amount: 0,
                    next: vec![],
                })
            }
            6 => {
                r.allocation = Some(Allocation {
                    root: id(1),
                    capacity: 1,
                })
            }
            7 => r.clock.backend = id(1),
            8 => r.clock.finality = [0; 32],
            9 => r.clock.sync = [0; 32],
            10 => {
                r.actor = p.config().accounts[0];
                expected = F::Reject(1000)
            }
            11 => r.clock.epoch -= 1,
            12 => r.clock.watermark -= 1,
            13 => {
                r.command = Command::Stake(st::Command::Stake(1));
                r.clock.epoch += 1
            }
            14 => r.backing.agrs += 1,
            15 => {
                r.command = Command::Pool(sp::Command::MoveGainToVault);
                expected = F::Domain
            }
            16 => r.command = Command::ReleaseExposure { events: vec![] },
            17 => {
                r.command = Command::ReleaseExposure {
                    events: vec![id(2), id(1)],
                }
            }
            18 => {
                r.command = Command::Fund {
                    event: [0; 32],
                    recipient: id(1),
                    quantity: Quantity::Agrs(1),
                };
                expected = F::Canonical
            }
            _ => {
                r.command = Command::Fund {
                    event: id(230),
                    recipient: p.config().accounts[0],
                    quantity: Quantity::Agrs(1),
                };
                expected = F::Domain
            }
        }
        // Preserve deliberately malformed proofs: bounded_request runs before signing.
        if case <= 1 {
            let frozen = s.clone();
            assert_eq!(
                decide(&p, &s, &r, &TestVerifier, limits()).result().err(),
                Some(expected)
            );
            assert_eq!(s, frozen);
        } else {
            refused(&p, &s, r, expected);
        }
    }
    let mut r = sign(&p, &s, raw(&s, 1, Command::AdvanceClock)).unwrap();
    r.proofs[1].domain = Domain::Actor;
    assert_eq!(
        decide(&p, &s, &r, &TestVerifier, limits()).result().err(),
        Some(F::Canonical)
    );
    for quantity in [Quantity::Agrs(0), Quantity::Zusd(1)] {
        refused(
            &p,
            &s,
            raw(
                &s,
                1,
                Command::Fund {
                    event: id(230),
                    recipient: id(1),
                    quantity,
                },
            ),
            F::Domain,
        );
    }
    for case in 0..8 {
        let mut r = raw(
            &s,
            1,
            Command::Open {
                collateral: 1200 * Q,
                principal: 1000 * Q,
                max_fee: Q,
                upper: None,
                lower: None,
            },
        );
        use_price(&p, &s, &mut r);
        match case {
            0 => r.oracle.as_mut().unwrap().action = OracleAction::RefreshStatus,
            1 => r.oracle.as_mut().unwrap().event = Some(id(1)),
            2 => r.oracle.as_mut().unwrap().before = id(1),
            3 => r.oracle.as_mut().unwrap().after = id(1),
            4 => r.oracle.as_mut().unwrap().view.aggregate_bytes = vec![1],
            5 => r.allocation.as_mut().unwrap().root = [0; 32],
            6 => r.allocation.as_mut().unwrap().root = id(1),
            _ => r.allocation.as_mut().unwrap().capacity += 1,
        }
        refused(
            &p,
            &s,
            r,
            if case >= 5 {
                F::Reject(1024)
            } else {
                F::Canonical
            },
        );
    }
}
#[test]
fn oracle_update_rejects_each_authenticated_field_mismatch() {
    let p = policy(false);
    let s = ready(&p);
    for case in 0..19 {
        let mut r = price_request(&p, &s, OracleAction::ImportPending, 2, 150_000_000, 180);
        let o = r.oracle.as_mut().unwrap();
        match case {
            0 => o.before = id(1),
            1 => o.after = [0; 32],
            2 => o.view.aggregate_bytes.clear(),
            3 => o.view.pending.as_mut().unwrap().value_e8 = 0,
            4 => o.view.pending.as_mut().unwrap().aggregate = [0; 32],
            5 => o.view.pending.as_mut().unwrap().observed_epoch += 1,
            6 => o.view.pending.as_mut().unwrap().median_seconds += 1,
            7 => o.view.pending.as_mut().unwrap().committed_epoch = s.clock().epoch + 1,
            8 => o.view.pending.as_mut().unwrap().committed_epoch = 1,
            9 => o.view.pending.as_mut().unwrap().published_epoch -= 1,
            10 => o.view.pending.as_mut().unwrap().earliest_commit_epoch += 1,
            11 => o.view.pending.as_mut().unwrap().dispute_until += 1,
            12 => o.view.pending.as_mut().unwrap().round = 1,
            13 => o.view.pending = None,
            14 => o.view.committed.as_mut().unwrap().value_e8 += 1,
            15 => o.view.committed.as_mut().unwrap().earliest_commit_epoch += 1,
            16 => {
                let c = o.view.committed.as_mut().unwrap();
                c.published_epoch += 1;
                c.earliest_commit_epoch = c.committed_epoch;
            }
            17 => o.action = OracleAction::Use,
            _ => o.view.aggregate_bytes.resize(131073, 0),
        }
        refused(
            &p,
            &s,
            r,
            if case == 18 {
                F::Capacity
            } else {
                F::Canonical
            },
        );
    }
    let mut pending = s.clone();
    price(
        &p,
        &mut pending,
        OracleAction::ImportPending,
        2,
        150_000_000,
        180,
    );
    let r = price_request(
        &p,
        &pending,
        OracleAction::ImportPending,
        2,
        140_000_000,
        181,
    );
    refused(&p, &pending, r, F::Canonical);
    let mut matured = pending.clone();
    advance(&p, &mut matured, 4, 4);
    for case in 0..11 {
        let mut r = price_request(
            &p,
            &matured,
            OracleAction::CommitMatured,
            2,
            150_000_000,
            181,
        );
        let o = r.oracle.as_mut().unwrap();
        let c = o.view.committed.as_mut().unwrap();
        match case {
            0 => c.round += 1,
            1 => c.published_epoch = 1,
            2 => c.earliest_commit_epoch = 3,
            3 => c.dispute_until = 3,
            4 => c.committed_epoch = 3,
            5 => c.aggregate = id(1),
            6 => c.value_e8 += 1,
            7 => c.observed_epoch += 1,
            8 => c.median_seconds += 1,
            9 => c.deviation += 1,
            _ => {
                o.view.pending = Some(pending.oracle().unwrap().pending.unwrap());
            }
        }
        refused(&p, &matured, r, F::Canonical);
    }
    // Status-only refresh retains pending timing fields after the clock advances.
    let mut refresh = raw(&matured, 6, Command::ObserveOracle);
    refresh.oracle = Some(OracleReceipt {
        before: matured.oracle_root(),
        after: id(201),
        event: Some(id(202)),
        action: OracleAction::RefreshStatus,
        view: matured.oracle().unwrap().clone(),
    });
    let _ = step(&p, &mut matured, refresh);
    for case in 0..3 {
        let mut r = raw(&matured, 6, Command::ObserveOracle);
        let mut view = matured.oracle().unwrap().clone();
        match case {
            0 => view.committed.as_mut().unwrap().value_e8 += 1,
            1 => view.pending = None,
            _ => view.status = v::PriceStatus::Disputed,
        };
        r.oracle = Some(OracleReceipt {
            before: matured.oracle_root(),
            after: id(203),
            event: Some(id(204)),
            action: OracleAction::RefreshStatus,
            view,
        });
        if case < 2 {
            refused(&p, &matured, r, F::Canonical);
        } else {
            let _ = step(&p, &mut matured, r);
        }
    }
    let mut r = price_request(
        &p,
        &matured,
        OracleAction::ImportPending,
        3,
        140_000_000,
        205,
    );
    r.oracle.as_mut().unwrap().view.status = v::PriceStatus::Usable;
    refused(&p, &matured, r, F::Canonical);
}

#[test]
fn small_money_limits_and_public_inventory_refusals() {
    let p = policy(false);
    let s = ready(&p);
    let b = deposited();
    for case in 0..5 {
        let mut rows = book().vaults;
        let mut sys = book().system;
        let expected = match case {
            0 => {
                rows.swap(0, 1);
                F::Canonical
            }
            1 => {
                rows[1].owner = rows[0].owner;
                F::Domain
            }
            2 => {
                sys.stakes += 1;
                F::Conservation
            }
            3 => {
                sys.collateral -= 1;
                F::Conservation
            }
            _ => {
                sys.debt -= 1;
                F::Conservation
            }
        };
        assert_eq!(
            sp::change(
                sp::ChangeInputs {
                    pre: &b,
                    actor: id(1),
                    command: sp::Command::Withdraw(1),
                    wallet_zusd: 0,
                    issuance: Some(0),
                    prices: prices(),
                    inventory: &rows,
                    system: sys,
                    bounds: bounds(),
                },
                limits()
            )
            .result()
            .err(),
            Some(expected)
        );
    }
    let rows = book();
    let mut blocked = prices();
    blocked.pending_blocked = true;
    assert_eq!(
        sp::change(
            sp::ChangeInputs {
                pre: &b,
                actor: id(1),
                command: sp::Command::Withdraw(1),
                wallet_zusd: 0,
                issuance: Some(0),
                prices: blocked,
                inventory: &rows.vaults,
                system: rows.system,
                bounds: bounds(),
            },
            limits()
        )
        .result()
        .err(),
        Some(F::Reject(1020))
    );
    assert_eq!(
        sp::change(
            sp::ChangeInputs {
                pre: &b,
                actor: id(1),
                command: sp::Command::MoveGainToVault,
                wallet_zusd: 0,
                issuance: Some(0),
                prices: prices(),
                inventory: &[],
                system: s.book().system,
                bounds: bounds(),
            },
            limits()
        )
        .result()
        .err(),
        Some(F::Reject(1011))
    );
    assert_eq!(
        sp::change(
            sp::ChangeInputs {
                pre: &b,
                actor: id(1),
                command: sp::Command::RegisterFrontEnd(Q),
                wallet_zusd: 0,
                issuance: None,
                prices: prices(),
                inventory: &[],
                system: s.book().system,
                bounds: bounds(),
            },
            limits()
        )
        .result()
        .err(),
        Some(F::Reject(1011))
    );
    assert_eq!(
        sp::change(
            sp::ChangeInputs {
                pre: &b,
                actor: id(1),
                command: sp::Command::Claim,
                wallet_zusd: 0,
                issuance: Some(0),
                prices: prices(),
                inventory: &vec![rows.vaults[0]; 9],
                system: rows.system,
                bounds: bounds(),
            },
            limits()
        )
        .result()
        .err(),
        Some(F::Capacity)
    );
    let mut front = empty();
    for actor in 1..=8 {
        front = sp::change(
            sp::ChangeInputs {
                pre: &front,
                actor: id(actor),
                command: sp::Command::RegisterFrontEnd(Q),
                wallet_zusd: 0,
                issuance: None,
                prices: prices(),
                inventory: &[],
                system: s.book().system,
                bounds: bounds(),
            },
            limits(),
        )
        .into_parts()
        .0
        .unwrap()
        .book;
    }
    assert_eq!(
        sp::change(
            sp::ChangeInputs {
                pre: &front,
                actor: id(9),
                command: sp::Command::RegisterFrontEnd(Q),
                wallet_zusd: 0,
                issuance: None,
                prices: prices(),
                inventory: &[],
                system: s.book().system,
                bounds: bounds(),
            },
            limits()
        )
        .result()
        .err(),
        Some(F::Reject(1061))
    );
    let mut other = empty();
    other.pool.total = 1;
    assert_eq!(
        zenofcis_zusd_v1::composition::move_gain_to_vault(
            zenofcis_zusd_v1::composition::GainToVaultInputs {
                pre: &rows,
                pool: &other,
                actor: id(1),
                parameters: v::Parameters::v1(),
                fee: s.fee_state(),
                prices: prices(),
                issuance: Some(0),
                upper: None,
                lower: None,
                capacity: 8,
                bounds: bounds(),
            },
            limits()
        )
        .result()
        .err(),
        Some(F::Conservation)
    );
}
#[test]
fn vault_scalar_domain_and_representability_guards() {
    let b = book();
    let row = b.vaults[0];
    let f = zenofcis_zusd_v1::fees::State {
        base: 0,
        last_epoch: 0,
    };
    let time = Some(zenofcis_zusd_v1::fees::LogicalTime {
        epoch: 0,
        elapsed_minutes: 0,
    });
    for case in 0..7 {
        let mut existing = None;
        let mut params = v::Parameters::v1();
        let mut price = prices();
        let mut principal = 1000 * Q;
        let expected = match case {
            0 => {
                principal = 0;
                F::Reject(1010)
            }
            1 => {
                let mut wrong_id = row.id;
                wrong_id[0] ^= 1;
                existing = Some(v::Vault {
                    status: v::Status::ClosedOwner,
                    id: wrong_id,
                    ..row
                });
                F::Domain
            }
            2 => {
                let mut wrong_owner = row.owner;
                wrong_owner[0] ^= 1;
                existing = Some(v::Vault {
                    status: v::Status::ClosedOwner,
                    owner: wrong_owner,
                    ..row
                });
                F::Domain
            }
            3 => {
                params.minimum_net = 0;
                F::Domain
            }
            4 => {
                params.reserve = 0;
                F::Domain
            }
            5 => {
                price.committed = 0;
                F::Domain
            }
            _ => {
                price.pending = Some(0);
                F::Domain
            }
        };
        assert_eq!(
            v::open(
                v::OpenInputs {
                    existing,
                    id: row.id,
                    owner: row.owner,
                    collateral: 1200 * Q,
                    principal,
                    balances: (1200 * Q, 0),
                    system: book().system,
                    parameters: params,
                    fee: f,
                    prices: price,
                    max_fee: Q,
                    time,
                },
                limits()
            )
            .result()
            .err(),
            Some(expected)
        );
    }
    let mut blocked = prices();
    blocked.pending_blocked = true;
    assert_eq!(
        v::open(
            v::OpenInputs {
                existing: None,
                id: id(1),
                owner: id(1),
                collateral: 1200 * Q,
                principal: 1000 * Q,
                balances: (1200 * Q, 0),
                system: book().system,
                parameters: v::Parameters::v1(),
                fee: f,
                prices: blocked,
                max_fee: Q,
                time,
            },
            limits()
        )
        .result()
        .err(),
        Some(F::Reject(1020))
    );
    assert_eq!(
        v::adjust(
            v::AdjustInputs {
                vault: row,
                system: b.system,
                parameters: v::Parameters::v1(),
                fee: f,
                prices: blocked,
                collateral_delta: v::Delta::Remove(1),
                debt_delta: v::Delta::None,
                wallet: (0, 0),
                max_fee: Q,
                time: None,
            },
            limits()
        )
        .result()
        .err(),
        Some(F::Reject(1020))
    );
    assert_eq!(
        v::close(row, b.system, f, blocked, 2000 * Q, limits())
            .result()
            .err(),
        Some(F::Reject(1020))
    );
    let huge = v::Prices {
        committed: u128::MAX,
        ..prices()
    };
    let sys = v::System {
        collateral: 0,
        debt: 0,
        active: 0,
        stakes: 0,
        ..b.system
    };
    assert_eq!(
        v::open(
            v::OpenInputs {
                existing: None,
                id: id(1),
                owner: id(1),
                collateral: 1,
                principal: 500 * Q,
                balances: (1, 0),
                system: sys,
                parameters: v::Parameters::v1(),
                fee: f,
                prices: huge,
                max_fee: Q,
                time,
            },
            limits()
        )
        .result()
        .err(),
        Some(F::Arithmetic)
    );
    let row = v::Vault {
        collateral: 10,
        debt: 550 * Q,
        reserve: 50 * Q,
        stake: 10,
        ..row
    };
    let sys = v::System {
        collateral: 10,
        debt: 550 * Q,
        active: 1,
        stakes: 10,
        ..sys
    };
    assert_eq!(
        v::adjust(
            v::AdjustInputs {
                vault: row,
                system: sys,
                parameters: v::Parameters::v1(),
                fee: f,
                prices: huge,
                collateral_delta: v::Delta::Remove(9),
                debt_delta: v::Delta::None,
                wallet: (0, 0),
                max_fee: Q,
                time: None,
            },
            limits()
        )
        .result()
        .err(),
        Some(F::Arithmetic)
    );
    assert_eq!(
        l::quote(
            v::Vault {
                debt: 0,
                reserve: 0,
                ..row
            },
            b.system,
            prices(),
            0,
            limits()
        )
        .result()
        .err(),
        Some(F::Reject(1044))
    );
}
#[test]
fn application_balance_nonce_seen_and_vault_capacities() {
    let p = policy(false);
    let initial = ready(&p);
    let mut s = initial.clone();
    for actor in 1..=32 {
        let r = raw(&s, actor, Command::AdvanceClock);
        let _ = step(&p, &mut s, r);
    }
    refused(&p, &s, raw(&s, 33, Command::AdvanceClock), F::Reject(1061));
    let mut s = initial.clone();
    let mut event = 1u8;
    for recipient in (1..=230).filter(|n| !p.config().accounts.contains(&id(*n))) {
        let mut r = raw(
            &s,
            6,
            Command::Fund {
                event: id(event),
                recipient: id(recipient),
                quantity: Quantity::Agrs(1),
            },
        );
        r.backing.agrs += 1;
        if s.ledger().balances.len() == 128 {
            refused(&p, &s, r, F::Reject(1061));
            break;
        }
        let _ = step(&p, &mut s, r);
        event += 1;
        if event == 110 {
            event += 1;
        }
    }
    assert_eq!(s.ledger().balances.len(), 128);
    let mut s = initial.clone();
    for event in (1..=129).filter(|n| *n != 110) {
        let r = Request {
            oracle: Some(OracleReceipt {
                before: s.oracle_root(),
                after: id(231),
                event: Some(id(event)),
                action: OracleAction::RefreshStatus,
                view: s.oracle().unwrap().clone(),
            }),
            ..raw(&s, 6, Command::ObserveOracle)
        };
        if event == 129 {
            refused(&p, &s, r, F::Reject(1061));
        } else {
            let _ = step(&p, &mut s, r);
        }
    }
    let mut s = initial;
    for who in 1..=9 {
        if who >= 7 {
            let mut r = raw(
                &s,
                6,
                Command::Fund {
                    event: id(200 + who),
                    recipient: id(who),
                    quantity: Quantity::Agrs(1200 * Q),
                },
            );
            r.backing.agrs += 1200 * Q;
            let _ = step(&p, &mut s, r);
        }
        let mut r = raw(
            &s,
            who,
            Command::Open {
                collateral: 1200 * Q,
                principal: 500 * Q,
                max_fee: Q,
                upper: None,
                lower: None,
            },
        );
        use_price(&p, &s, &mut r);
        if who == 9 {
            refused(&p, &s, r, F::Reject(1061));
        } else {
            let _ = step(&p, &mut s, r);
        }
    }
}
#[test]
fn authenticated_oracle_status_and_funding_boundaries() {
    let p = policy(false);
    let s = ready(&p);
    for (field, expected) in [
        (0, F::Conservation),
        (1, F::Conservation),
        (2, F::Conservation),
        (3, F::Conservation),
        (4, F::Conservation),
    ] {
        let mut r = raw(
            &s,
            6,
            Command::Fund {
                event: id(230),
                recipient: id(1),
                quantity: Quantity::Agrs(1),
            },
        );
        r.backing.agrs += 1;
        match field {
            0 => r.backing.custody = [0; 32],
            1 => r.backing.agrs += 1,
            2 => r.backing.zdex_supply = 0,
            3 => r.backing.zdex_burned = u128::MAX,
            _ => r.backing.zdex += 1,
        }
        refused(&p, &s, r, expected);
    }
    let mut s = s.clone();
    advance(&p, &mut s, 50_003, 50_003);
    let r = raw(&s, 1, Command::Stake(st::Command::Stake(1)));
    let _ = step(&p, &mut s, r);
    let mut s = ready(&p);
    let mut r = raw(&s, 6, Command::ObserveOracle);
    let mut view = s.oracle().unwrap().clone();
    view.committed.as_mut().unwrap().deviation = 101;
    // A different immutable committed value is rejected by a status-only refresh.
    r.oracle = Some(OracleReceipt {
        before: s.oracle_root(),
        after: id(232),
        event: Some(id(233)),
        action: OracleAction::RefreshStatus,
        view,
    });
    refused(&p, &s, r, F::Canonical);
    let mut r = price_request(&p, &s, OracleAction::ImportPending, 2, 150_000_000, 180);
    r.oracle
        .as_mut()
        .unwrap()
        .view
        .pending
        .as_mut()
        .unwrap()
        .deviation = 101;
    let _ = step(&p, &mut s, r);
    let r = raw(&s, 1, Command::Stake(st::Command::Stake(1)));
    let _ = step(&p, &mut s, r);
    advance(&p, &mut s, 4, 4);
    let r = price_request(&p, &s, OracleAction::CommitMatured, 2, 150_000_000, 181);
    let _ = step(&p, &mut s, r);
    let r = raw(&s, 1, Command::Stake(st::Command::Stake(1)));
    let _ = step(&p, &mut s, r);
    let mut s = ready(&p);
    let mut r = price_request(&p, &s, OracleAction::ImportPending, 2, 150_000_000, 180);
    r.oracle.as_mut().unwrap().view.capacity = 1;
    let _ = step(&p, &mut s, r);
    let r = raw(&s, 1, Command::Stake(st::Command::Stake(1)));
    let _ = step(&p, &mut s, r);
    let mut s = genesis(&p);
    advance(&p, &mut s, 2, 2);
    let r = price_request(&p, &s, OracleAction::ImportPending, 1, 200_000_000, 180);
    let _ = step(&p, &mut s, r);
    let r = raw(&s, 1, Command::Stake(st::Command::Stake(1)));
    let _ = step(&p, &mut s, r);
}

#[test]
fn all_small_hint_pairs_preserve_owner_money() {
    let p = policy(false);
    let mut s = ready(&p);
    let mut first = raw(
        &s,
        1,
        Command::Open {
            collateral: 3000 * Q,
            principal: 1000 * Q,
            max_fee: Q,
            upper: Some(id(240)),
            lower: None,
        },
    );
    use_price(&p, &s, &mut first);
    let _ = step(&p, &mut s, first);
    for (who, c) in [(2, 2000), (3, 1200), (4, 700)] {
        open(&p, &mut s, who, c * Q, 1000 * Q);
    }
    let mut hints = vec![None, Some(id(240))];
    hints.extend(s.book().order.iter().copied().map(Some));
    for c in [700, 1000, 1500, 2500, 4000] {
        for upper in &hints {
            for lower in &hints {
                let mut r = raw(
                    &s,
                    5,
                    Command::Open {
                        collateral: c * Q,
                        principal: 1000 * Q,
                        max_fee: Q,
                        upper: *upper,
                        lower: *lower,
                    },
                );
                use_price(&p, &s, &mut r);
                let request = sign(&p, &s, r).unwrap();
                let out = decide(&p, &s, &request, &TestVerifier, limits())
                    .into_parts()
                    .0
                    .unwrap();
                assert_eq!(out.post().ledger().supply - s.ledger().supply, 1055 * Q);
                assert_eq!(
                    out.post().book().system.collateral - s.book().system.collateral,
                    c * Q
                );
            }
        }
    }
    let mut r = raw(
        &s,
        5,
        Command::Adjust {
            collateral: v::Delta::None,
            debt: v::Delta::Add(Q),
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(&p, &s, &mut r);
    refused(&p, &s, r, F::Reject(1011));
    let b = book();
    let row = b.vaults[0];
    let f = zenofcis_zusd_v1::fees::State {
        base: Q / 20,
        last_epoch: 0,
    };
    assert_eq!(
        v::adjust(
            v::AdjustInputs {
                vault: row,
                system: b.system,
                parameters: v::Parameters::v1(),
                fee: f,
                prices: prices(),
                collateral_delta: v::Delta::None,
                debt_delta: v::Delta::Add(Q),
                wallet: (0, 0),
                max_fee: Q / 200,
                time: Some(zenofcis_zusd_v1::fees::LogicalTime {
                    epoch: 0,
                    elapsed_minutes: 0
                }),
            },
            limits()
        )
        .result()
        .err(),
        Some(F::Reject(1052))
    );
    let mut aged = s.clone();
    advance(&p, &mut aged, 20160, 20160);
    let id = *aged.book().order.last().unwrap();
    let v = aged.book().vaults.iter().find(|v| v.id == id).unwrap();
    let hint = *A::from(v.collateral)
        .multiply_divide(zenofcis_zusd_v1::NICR_Q, v.debt - 1, limits())
        .result()
        .unwrap();
    let mut r = raw(
        &aged,
        1,
        Command::Redeem(zenofcis_zusd_v1::redemption::Command {
            maximum: 1,
            visits: 0,
            first: None,
            partial_nicr: hint,
            upper: None,
            lower: None,
            max_fee: Q,
            min_out: 0,
        }),
    );
    use_price(&p, &aged, &mut r);
    refused(&p, &aged, r, F::Reject(1045));
}
fn reindex(b: &mut l::Book) {
    b.priorities = b
        .vaults
        .iter()
        .filter(|v| v.status == v::Status::Active)
        .map(|v| {
            (
                v.id,
                *A::from(v.collateral)
                    .multiply_divide(zenofcis_zusd_v1::NICR_Q, v.debt, limits())
                    .result()
                    .unwrap(),
            )
        })
        .collect();
    b.order = b.priorities.iter().map(|(id, _)| *id).collect();
    b.order.sort_by_key(|id| {
        core::cmp::Reverse(b.priorities.iter().find(|(i, _)| i == id).unwrap().1)
    });
}
#[test]
fn liquidation_native_limits_have_unchanged_refusal_frames() {
    let mut b = book();
    let old = b.vaults[0].collateral;
    b.vaults[0].collateral = 0;
    b.default_collateral += old;
    liquid_error(&b, F::Arithmetic);
    let mut b = book();
    for v in &mut b.vaults {
        v.collateral = 100 * Q;
    }
    b.default_collateral = 9800 * Q;
    b.system.collateral = 10000 * Q;
    reindex(&mut b);
    let command = l::Command::Batch(b.order.clone());
    let frozen = b.clone();
    assert_eq!(
        l::liquidate(&b, &command, prices(), None, 8, bounds(), limits())
            .result()
            .err(),
        Some(F::Reject(1041))
    );
    assert_eq!(b, frozen);
    let mut b = book();
    b.vaults[0].collateral = 1266 * Q;
    b.vaults[1].collateral = 1688 * Q;
    b.system.collateral = 2954 * Q;
    b.pool.total = 1055 * Q;
    b.surplus = vec![(id(20), 1), (id(21), 1)];
    reindex(&mut b);
    assert_eq!(
        l::liquidate(
            &b,
            &l::Command::Direct(b.vaults[0].id),
            v::Prices {
                committed: 100_000_000,
                ..prices()
            },
            Some(0),
            2,
            bounds(),
            limits()
        )
        .result()
        .err(),
        Some(F::Reject(1061))
    );
}
fn genesis_input(p: &Policy) -> Genesis {
    Genesis {
        wallets: zenofcis_zusd_v1::ledger::Sheet {
            balances: vec![zenofcis_zusd_v1::ledger::Balance {
                account: id(9),
                quantity: Quantity::Zdex(sp::FUNDED_CAP),
            }],
            supply: 0,
            active_debt: 0,
            default_debt: 0,
        },
        funder: id(9),
        clock: Clock {
            backend: p.config().clock_backend,
            finality: id(80),
            sync: id(81),
            epoch: 0,
            watermark: 0,
        },
        backing: Backing {
            custody: id(82),
            funding: id(83),
            supply_root: id(84),
            agrs: 0,
            zdex: sp::FUNDED_CAP,
            zdex_supply: sp::FUNDED_CAP,
            zdex_burned: 0,
        },
        proofs: vec![],
    }
}
fn sign_genesis(p: &Policy, g: &mut Genesis) {
    g.proofs = genesis_messages(p, g)
        .unwrap()
        .into_iter()
        .map(|m| Proof {
            domain: m.domain,
            bytes: issue(&m.bytes),
        })
        .collect();
}
#[test]
fn genesis_authentication_and_request_preparation_boundaries() {
    let p = policy(false);
    for case in 0..9 {
        let mut g = genesis_input(&p);
        let expected = match case {
            0 => {
                g.wallets.balances.resize(129, g.wallets.balances[0]);
                F::Capacity
            }
            1 => {
                g.proofs.resize(
                    5,
                    Proof {
                        domain: Domain::Actor,
                        bytes: vec![],
                    },
                );
                F::Capacity
            }
            2 => {
                g.proofs = vec![Proof {
                    domain: Domain::Actor,
                    bytes: vec![0; 257],
                }];
                F::Capacity
            }
            3 => {
                g.funder = p.config().accounts[0];
                F::Domain
            }
            4 => {
                g.wallets.supply = 1;
                F::Domain
            }
            5 => {
                g.wallets.active_debt = 1;
                F::Domain
            }
            6 => {
                g.wallets.default_debt = 1;
                F::Domain
            }
            7 => {
                g.wallets.balances[0].account = p.config().accounts[0];
                F::Domain
            }
            _ => {
                g.clock.finality = [0; 32];
                F::Canonical
            }
        };
        if case >= 3 {
            sign_genesis(&p, &mut g);
        }
        assert_eq!(
            initialize(&p, &g, &TestVerifier, limits()).result().err(),
            Some(expected)
        );
    }
    let s = ready(&p);
    let mut request = raw(&s, 1, Command::Stake(st::Command::Stake(1)));
    use_price(&p, &s, &mut request);
    refused(&p, &s, request, F::Canonical);
    let mut cfg = p.config().clone();
    cfg.bounds.frame_bytes = 1024;
    cfg.bounds.proof_bytes = 2048;
    let tiny = Policy::bind(cfg).unwrap();
    let mut g = genesis_input(&tiny);
    g.wallets.balances.resize(21, g.wallets.balances[0]);
    assert_eq!(genesis_messages(&tiny, &g).err(), Some(F::Capacity));
    let mut g = genesis_input(&tiny);
    g.proofs = vec![Proof {
        domain: Domain::Actor,
        bytes: vec![0; 1100],
    }];
    assert!(genesis_messages(&tiny, &g).is_ok());
    let g = genesis_input(&tiny);
    let mut cfg = tiny.config().clone();
    cfg.bounds.frame_bytes = 100;
    let too_small = Policy::bind(cfg).unwrap();
    assert_eq!(genesis_messages(&too_small, &g).err(), Some(F::Capacity));
    let mut r = raw(&s, 1, Command::AdvanceClock);
    r.proofs = vec![Proof {
        domain: Domain::Actor,
        bytes: vec![0; 1100],
    }];
    assert_eq!(receipt_messages(&tiny, &s, &r).err(), Some(F::Capacity));
    let r = raw(
        &s,
        1,
        Command::ReleaseExposure {
            events: vec![id(1); 40],
        },
    );
    assert_eq!(receipt_messages(&tiny, &s, &r).err(), Some(F::Capacity));
    assert_eq!(
        receipt_messages(&too_small, &s, &raw(&s, 1, Command::AdvanceClock)).err(),
        Some(F::Capacity)
    );
    refused(
        &p,
        &s,
        raw(
            &s,
            1,
            Command::Pool(sp::Command::Deposit {
                amount: 1,
                tag: None,
            }),
        ),
        F::Reject(1033),
    );
    let mut s = ready(&p);
    open(&p, &mut s, 1, 1200 * Q, 1000 * Q);
    let mut r = raw(
        &s,
        1,
        Command::Pool(sp::Command::Deposit {
            amount: Q,
            tag: None,
        }),
    );
    drip(&mut r, 230, 0);
    let _ = step(&p, &mut s, r);
    let mut r = raw(&s, 1, Command::Pool(sp::Command::Claim));
    drip(&mut r, 231, 1);
    refused(&p, &s, r, F::Canonical);
    advance(&p, &mut s, 3, 3);
    let mut r = raw(&s, 1, Command::Pool(sp::Command::Claim));
    drip(&mut r, 231, sp::FUNDED_CAP + 1);
    refused(&p, &s, r, F::Conservation);
}

#[test]
fn debt_and_exposure_caps_refuse_without_partial_money() {
    let p = policy(false);
    let mut s = ready(&p);
    price(
        &p,
        &mut s,
        OracleAction::ImportPending,
        2,
        10_000_000_000,
        180,
    );
    advance(&p, &mut s, 4, 4);
    price(
        &p,
        &mut s,
        OracleAction::CommitMatured,
        2,
        10_000_000_000,
        181,
    );
    let mut r = raw(
        &s,
        1,
        Command::Open {
            collateral: 20000 * Q,
            principal: 100001 * Q,
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(&p, &s, &mut r);
    refused(&p, &s, r, F::Reject(1024));
    let mut s = ready(&p);
    for who in [1, 2] {
        let mut r = raw(
            &s,
            6,
            Command::Fund {
                event: id(210 + who),
                recipient: id(who),
                quantity: Quantity::Agrs(100000 * Q),
            },
        );
        r.backing.agrs += 100000 * Q;
        let _ = step(&p, &mut s, r);
    }
    open(&p, &mut s, 1, 108000 * Q, 98000 * Q);
    open(&p, &mut s, 2, 100000 * Q, 1000 * Q);
    price(&p, &mut s, OracleAction::ImportPending, 2, 100_000_000, 180);
    advance(&p, &mut s, 4, 4);
    price(&p, &mut s, OracleAction::CommitMatured, 2, 100_000_000, 181);
    let mut r = raw(
        &s,
        6,
        Command::Liquidate(l::Command::Direct(vault_id(&p, id(1)))),
    );
    use_price(&p, &s, &mut r);
    refused(&p, &s, r, F::Reject(1024));
    let mut s = ready(&p);
    open(&p, &mut s, 1, 1200 * Q, 1000 * Q);
    open(&p, &mut s, 2, 700 * Q, 1000 * Q);
    advance(&p, &mut s, 20160, 20160);
    for i in 0..33 {
        let v = s
            .book()
            .vaults
            .iter()
            .find(|v| v.id == *s.book().order.last().unwrap())
            .unwrap();
        let hint = *A::from(v.collateral - Q / 2)
            .multiply_divide(zenofcis_zusd_v1::NICR_Q, v.debt - Q, limits())
            .result()
            .unwrap();
        let mut r = raw(
            &s,
            1,
            Command::Redeem(zenofcis_zusd_v1::redemption::Command {
                maximum: Q,
                visits: 0,
                first: None,
                partial_nicr: hint,
                upper: None,
                lower: None,
                max_fee: Q,
                min_out: 1,
            }),
        );
        use_price(&p, &s, &mut r);
        if i < 32 {
            let _ = step(&p, &mut s, r);
        } else {
            refused(&p, &s, r, F::Reject(1061));
        }
    }
    let other = policy(true);
    refused(&other, &s, raw(&s, 1, Command::AdvanceClock), F::Canonical);
}

#[test]
fn late_oracle_commit_and_initialization_refusals() {
    let p = policy(false);
    let mut s = ready(&p);
    price(&p, &mut s, OracleAction::ImportPending, 2, 150_000_000, 180);
    advance(&p, &mut s, 6, 6);
    for case in 0..3 {
        let mut r = price_request(&p, &s, OracleAction::CommitMatured, 2, 150_000_000, 181);
        let o = r.oracle.as_mut().unwrap();
        match case {
            0 => o.view.committed.as_mut().unwrap().earliest_commit_epoch = 6,
            1 => o.view.committed.as_mut().unwrap().committed_epoch = 5,
            _ => {
                let mut pending = s.oracle().unwrap().pending.unwrap();
                pending.round = 3;
                pending.published_epoch = 6;
                pending.earliest_commit_epoch = 8;
                pending.dispute_until = 8;
                o.view.pending = Some(pending);
            }
        }
        refused(&p, &s, r, F::Canonical);
    }
    let r = price_request(
        &p,
        &s,
        OracleAction::InitializeFirstMatured,
        3,
        200_000_000,
        182,
    );
    refused(&p, &s, r, F::Canonical);
    let mut past = s.clone();
    advance(&p, &mut past, 50003, 50003);
    let r = raw(&past, 1, Command::Stake(st::Command::Stake(1)));
    let _ = step(&p, &mut past, r);
    let mut empty = genesis(&p);
    advance(&p, &mut empty, 2, 2);
    for case in 0..3 {
        let mut r = price_request(
            &p,
            &empty,
            OracleAction::InitializeFirstMatured,
            1,
            200_000_000,
            180,
        );
        let o = r.oracle.as_mut().unwrap();
        match case {
            0 => o.view.committed = None,
            1 => {
                let mut pending = o.view.committed.unwrap();
                pending.round = 2;
                pending.published_epoch = 2;
                pending.earliest_commit_epoch = 4;
                pending.dispute_until = 4;
                pending.committed_epoch = 0;
                o.view.pending = Some(pending);
            }
            _ => {
                o.action = OracleAction::RefreshStatus;
                o.view.committed = None;
            }
        }
        refused(&p, &empty, r, F::Canonical);
    }
    for field in 0..2 {
        let mut past = ready(&p);
        let mut r = price_request(&p, &past, OracleAction::ImportPending, 2, 150_000_000, 180);
        if field == 0 {
            r.oracle
                .as_mut()
                .unwrap()
                .view
                .pending
                .as_mut()
                .unwrap()
                .observed_epoch = 0;
        } else {
            r.oracle
                .as_mut()
                .unwrap()
                .view
                .pending
                .as_mut()
                .unwrap()
                .median_seconds = 0;
        }
        let _ = step(&p, &mut past, r);
        advance(&p, &mut past, 4, 4);
        let r = price_request(&p, &past, OracleAction::CommitMatured, 2, 150_000_000, 181);
        refused(&p, &past, r, F::Canonical);
    }
}
#[test]
fn frame_limits_retain_owned_prestate() {
    let base = policy(false);
    let mut capacities = 0;
    let mut admissions = 0;
    for frame in (1000..=6000).step_by(100) {
        let mut cfg = base.config().clone();
        cfg.bounds.frame_bytes = frame;
        let p = Policy::bind(cfg).unwrap();
        let mut g = genesis_input(&p);
        sign_genesis(&p, &mut g);
        let result = initialize(&p, &g, &TestVerifier, limits()).into_parts().0;
        let Ok(mut s) = result else {
            continue;
        };
        let r = advance_request(&p, &s, 2, 2);
        let r = sign(&p, &s, r).unwrap();
        let frozen = s.clone();
        match decide(&p, &s, &r, &TestVerifier, limits()).into_parts().0 {
            Ok(out) => {
                s = out.post().clone();
                admissions += 1;
            }
            Err(F::Capacity) => {
                capacities += 1;
                assert_eq!(s, frozen);
                continue;
            }
            Err(e) => panic!("{e:?}"),
        }
        let mut r = price_request(
            &p,
            &s,
            OracleAction::InitializeFirstMatured,
            1,
            200_000_000,
            180,
        );
        r.oracle.as_mut().unwrap().view.aggregate_bytes = vec![1; frame as usize - 1000];
        if let Ok(r) = sign(&p, &s, r) {
            let frozen = s.clone();
            match decide(&p, &s, &r, &TestVerifier, limits()).into_parts().0 {
                Ok(_) => admissions += 1,
                Err(F::Capacity) => {
                    capacities += 1;
                    assert_eq!(s, frozen);
                }
                Err(e) => panic!("{e:?}"),
            }
        }
    }
    assert!(capacities > 0);
    assert!(admissions > 0);
}

#[test]
fn zero_role_rows_and_parameter_refusals_preserve_owner_values() {
    let mut cfg = policy(true).config().clone();
    cfg.bounds.params = 1;
    let p = Policy::bind(cfg).unwrap();
    let mut g = genesis_input(&p);
    for account in p.config().accounts {
        for quantity in [Quantity::Zusd(0), Quantity::Agrs(0), Quantity::Zdex(0)] {
            g.wallets
                .balances
                .push(zenofcis_zusd_v1::ledger::Balance { account, quantity });
        }
    }
    sign_genesis(&p, &mut g);
    let s = initialize(&p, &g, &TestVerifier, limits())
        .into_parts()
        .0
        .unwrap();
    let r = raw(&s, 1, Command::AdvanceClock);
    let _ = decide(&p, &s, &sign(&p, &s, r).unwrap(), &TestVerifier, limits())
        .into_parts()
        .0
        .unwrap();
    let mut s = ready(&p);
    open(&p, &mut s, 1, 1200 * Q, 1000 * Q);
    open(&p, &mut s, 2, 700 * Q, 1000 * Q);
    open(&p, &mut s, 3, 3000 * Q, 2000 * Q);
    advance(&p, &mut s, 20160, 20160);
    let mut r = raw(
        &s,
        3,
        Command::Redeem(zenofcis_zusd_v1::redemption::Command {
            maximum: 1005 * Q,
            visits: 0,
            first: None,
            partial_nicr: 0,
            upper: None,
            lower: None,
            max_fee: Q,
            min_out: 1,
        }),
    );
    use_price(&p, &s, &mut r);
    let _ = step(&p, &mut s, r);
    assert!(
        s.book()
            .vaults
            .iter()
            .any(|v| v.status == v::Status::ClosedRedemption)
    );
    for case in 0..7 {
        let mut approval = id(71);
        let mut version = 2;
        let mut minimum_net = 500 * Q;
        let mut reserve = 50 * Q;
        let expected = match case {
            0 => {
                approval = id(1);
                F::UnboundProfile
            }
            1 => {
                version = 1;
                F::Domain
            }
            2 => {
                minimum_net = 0;
                F::Domain
            }
            3 => {
                reserve = 0;
                F::Domain
            }
            4 => {
                minimum_net = u128::MAX;
                F::Domain
            }
            5 => {
                minimum_net = 3000 * Q;
                F::UnboundProfile
            }
            _ => F::Reject(1061),
        };
        let before = s.parameters().to_vec();
        let r = raw(
            &s,
            6,
            Command::ChangeParameters {
                policy: approval,
                version,
                minimum_net,
                reserve,
            },
        );
        refused(&p, &s, r, expected);
        assert_eq!(s.parameters(), before);
    }
}
