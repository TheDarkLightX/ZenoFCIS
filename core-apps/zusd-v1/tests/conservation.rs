//! Native aggregate-bridge regressions; the all-input local proof is separate.
use zenofcis_zusd_v1::{Failure, Id, Limits, ledger::*};
fn limits() -> Limits {
    Limits {
        steps: 1_000_000,
        reads: 1_000_000,
        bytes: 1_000_000,
    }
}
fn id(n: u8) -> Id {
    [n; 32]
}
fn sheet(amount: u128) -> Sheet {
    Sheet {
        balances: vec![
            Balance {
                account: id(1),
                quantity: Quantity::Zusd(amount),
            },
            Balance {
                account: id(1),
                quantity: Quantity::Agrs(amount),
            },
            Balance {
                account: id(1),
                quantity: Quantity::Zdex(amount),
            },
        ],
        supply: amount,
        active_debt: amount / 2,
        default_debt: amount - amount / 2,
    }
}
#[test]
fn exact_composite_debt_and_wide_totals_refuse_wraparound() {
    let good = sheet(u128::MAX);
    assert!(observe_sheet(&good, 8, limits()).result().is_ok());
    let mut bad = good.clone();
    bad.active_debt = u128::MAX;
    bad.default_debt = 1;
    assert_eq!(
        observe_sheet(&bad, 8, limits()).result().err(),
        Some(Failure::Conservation)
    );
    let mut bad = sheet(0);
    bad.balances[0].quantity = Quantity::Zusd(u128::MAX);
    bad.balances.push(Balance {
        account: id(2),
        quantity: Quantity::Zusd(1),
    });
    assert_eq!(
        observe_sheet(&bad, 8, limits()).result().err(),
        Some(Failure::Conservation)
    );
}
#[test]
fn ordinary_transfers_and_self_transfers_keep_each_asset() {
    for amount in [0, 1, 1u128 << 64, u128::MAX] {
        let pre = sheet(amount);
        for to in [id(1), id(2)] {
            for quantity in [
                Quantity::Zusd(amount),
                Quantity::Agrs(amount),
                Quantity::Zdex(amount),
            ] {
                let plan = compose(
                    &pre,
                    &[Movement {
                        from: Some(id(1)),
                        to: Some(to),
                        quantity,
                    }],
                    (pre.active_debt, pre.default_debt),
                    8,
                    limits(),
                )
                .into_parts()
                .0
                .unwrap();
                assert_eq!(plan.post().supply, amount);
                assert!(
                    check_transition(&pre, plan.post(), 8, limits())
                        .result()
                        .is_ok()
                );
                assert_eq!(pre, sheet(amount));
            }
        }
    }
}
#[test]
fn external_asset_issuance_and_burn_refuse_with_stable_order_and_work() {
    let pre = sheet(9);
    for quantity in [Quantity::Agrs(1), Quantity::Zdex(1)] {
        for (from, to, reason) in [
            (None, None, Failure::Domain),
            (None, Some(id(2)), Failure::Conservation),
            (Some(id(1)), None, Failure::Conservation),
        ] {
            let e = compose(
                &pre,
                &[Movement { from, to, quantity }],
                (4, 5),
                8,
                limits(),
            );
            assert_eq!(e.result().err(), Some(reason));
            assert_eq!(
                (e.usage().steps(), e.usage().reads(), e.usage().bytes()),
                (30, 4, 960)
            );
        }
    }
}
#[test]
fn matched_zusd_mint_burn_and_redistribution_conserve() {
    for amount in [1, 1u128 << 64, u128::MAX] {
        let pre = sheet(0);
        let minted = compose(
            &pre,
            &[Movement {
                from: None,
                to: Some(id(1)),
                quantity: Quantity::Zusd(amount),
            }],
            (amount, 0),
            8,
            limits(),
        )
        .into_parts()
        .0
        .unwrap();
        assert_eq!(minted.post().supply, amount);
        let moved = compose(minted.post(), &[], (0, amount), 8, limits())
            .into_parts()
            .0
            .unwrap();
        assert_eq!(moved.post().supply, amount);
        let burned = compose(
            moved.post(),
            &[Movement {
                from: Some(id(1)),
                to: None,
                quantity: Quantity::Zusd(amount),
            }],
            (0, 0),
            8,
            limits(),
        )
        .into_parts()
        .0
        .unwrap();
        assert_eq!(burned.post().supply, 0);
        assert!(observe_sheet(burned.post(), 8, limits()).result().is_ok());
        assert_eq!(pre, sheet(0));
    }
}
#[test]
fn insufficient_debit_refuses_before_quantity_arithmetic_charge() {
    let pre = sheet(0);
    let e = compose(
        &pre,
        &[Movement {
            from: Some(id(1)),
            to: Some(id(2)),
            quantity: Quantity::Agrs(1),
        }],
        (0, 0),
        8,
        limits(),
    );
    assert_eq!(e.result().err(), Some(Failure::Reject(1033)));
    assert_eq!(
        (e.usage().steps(), e.usage().reads(), e.usage().bytes()),
        (33, 7, 960)
    );
    assert_eq!(pre, sheet(0));
}
