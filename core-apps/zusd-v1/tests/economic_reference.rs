use zenofcis_zusd_v1::{fees, vaults as v, *};
#[path = "support/reference.rs"]
mod complete;
use complete::{lawful, row};
fn id(x: u8) -> Id {
    let mut out = [0; 32];
    out[31] = x;
    out
}
fn limits() -> Limits {
    Limits {
        steps: 100_000_000,
        reads: 10_000_000,
        bytes: 10_000_000,
    }
}
#[derive(Clone, Copy)]
enum OwnerCommand {
    Open(u128, u128),
    Adjust(v::Delta, v::Delta),
    Close,
}
include!("test-data/owner_reference.rs");
#[test]
fn independent_owner_complete_outcome_reason_and_frame_rows() {
    let cases = owner_cases();
    assert_eq!(cases.len(), 471);
    let mut accepts = 0;
    let mut reasons = std::collections::BTreeSet::new();
    for (pre, system, fee, prices, wallet, max_fee, command, expected) in cases {
        let clock = Some(fees::LogicalTime {
            epoch: 0,
            elapsed_minutes: 0,
        });
        let actual = match command {
            OwnerCommand::Open(c, x) => v::open(
                v::OpenInputs {
                    existing: pre,
                    id: id(1),
                    owner: id(1),
                    collateral: c,
                    principal: x,
                    balances: wallet,
                    system,
                    parameters: v::Parameters::v1(),
                    fee,
                    prices,
                    max_fee,
                    time: clock,
                },
                limits(),
            ),
            OwnerCommand::Adjust(c, d) => v::adjust(
                v::AdjustInputs {
                    vault: pre.unwrap(),
                    system,
                    parameters: v::Parameters::v1(),
                    fee,
                    prices,
                    collateral_delta: c,
                    debt_delta: d,
                    wallet,
                    max_fee,
                    time: clock,
                },
                limits(),
            ),
            OwnerCommand::Close => v::close(pre.unwrap(), system, fee, prices, wallet.1, limits()),
        };
        if let Ok(out) = &expected {
            accepts += 1;
            let mut wrong = *out;
            wrong.vault.owner = id(99);
            assert!(!lawful(&Ok(wrong), &expected));
        }
        if let Err(Failure::Reject(id)) = &expected {
            reasons.insert(*id);
        }
        row(actual.into_parts().0, expected);
    }
    assert!(accepts > 0);
    assert_eq!(
        reasons,
        std::collections::BTreeSet::from([
            1010, 1011, 1020, 1021, 1022, 1023, 1024, 1030, 1033, 1040, 1041, 1042, 1043, 1052
        ])
    );
    println!(
        "Owner complete reference: 471 rows, {accepts} accept; all 29 alternative reason conditions checked"
    );
}

use zenofcis_zusd_v1::{liquidation as l, stability as sp};
include!("test-data/liquidation_reference.rs");
#[test]
fn independent_liquidation_full_book_outcome_and_reason_rows() {
    let cases = liquidation_cases();
    assert_eq!(cases.len(), 240);
    let mut accepts = 0;
    let mut unbound = 0;
    let mut reasons = std::collections::BTreeSet::new();
    let bounds = sp::Bounds {
        history: 8,
        epochs: 4,
        deposits: 2,
        front_ends: 2,
        vaults: 5,
    };
    let prices = v::Prices {
        committed: 100_000_000,
        pending: None,
        status: v::PriceStatus::Usable,
        pending_blocked: false,
    };
    for (pre, command, issuance, expected) in cases {
        let frozen = pre.clone();
        let actual = l::liquidate(&pre, &command, prices, issuance, 5, bounds, limits());
        assert_eq!(pre, frozen);
        match &expected {
            Ok(out) => {
                accepts += 1;
                let mut wrong = out.clone();
                wrong.book.surplus.push((id(99), 1));
                assert!(!lawful(&Ok(wrong), &expected));
            }
            Err(Failure::Reject(id)) => {
                reasons.insert(*id);
            }
            Err(Failure::UnboundProfile) => unbound += 1,
            _ => panic!(),
        }
        row(actual.into_parts().0, expected);
    }
    assert!(accepts > 0 && unbound > 0);
    assert_eq!(
        reasons,
        std::collections::BTreeSet::from([1010, 1011, 1041, 1044, 1045])
    );
    println!(
        "Liquidation complete reference: 240 rows, {accepts} accept, {unbound} unbound; all 29 alternative reason conditions checked"
    );
}

use zenofcis_zusd_v1::redemption as r;
include!("test-data/redemption_reference.rs");
#[test]
fn independent_redemption_complete_outcomes_trace_recipient_and_reason_rows() {
    let cases = redemption_cases();
    assert_eq!(cases.len(), 116);
    let mut accepts = 0;
    let mut unbound = 0;
    let mut reasons = std::collections::BTreeSet::new();
    let mut stops = std::collections::BTreeSet::new();
    let bounds = sp::Bounds {
        history: 8,
        epochs: 4,
        deposits: 2,
        front_ends: 2,
        vaults: 5,
    };
    for (pre, command, base, price, balance, age, clock, expected) in cases {
        let prices = v::Prices {
            committed: price,
            pending: None,
            status: v::PriceStatus::Usable,
            pending_blocked: false,
        };
        let holder = r::Holder {
            zusd: balance,
            agrs: 0,
            staking_agrs: 0,
            fee: fees::State {
                base,
                last_epoch: 0,
            },
        };
        let frozen = pre.clone();
        let actual = r::redeem(
            r::RedeemInputs {
                pre: &pre,
                holder,
                command,
                prices,
                time: clock.then_some(fees::LogicalTime {
                    epoch: 0,
                    elapsed_minutes: 0,
                }),
                bootstrap_minutes: age,
                parameters: v::Parameters::v1(),
                capacity: 5,
                bounds,
            },
            limits(),
        );
        assert_eq!(pre, frozen);
        match &expected {
            Ok(out) => {
                accepts += 1;
                let mut wrong = out.clone();
                wrong.holder.agrs += 1;
                assert!(!lawful(&Ok(wrong), &expected));
                stops.insert(format!("{:?}", out.trace.stop));
            }
            Err(Failure::Reject(id)) => {
                reasons.insert(*id);
            }
            Err(Failure::UnboundProfile) => unbound += 1,
            _ => panic!(),
        }
        row(actual.into_parts().0, expected);
    }
    assert!(accepts > 0 && unbound > 0);
    assert_eq!(
        reasons,
        std::collections::BTreeSet::from([
            1010, 1030, 1031, 1032, 1033, 1041, 1045, 1050, 1051, 1052, 1053, 1061
        ])
    );
    assert_eq!(
        stops,
        std::collections::BTreeSet::from([
            "Filled".to_owned(),
            "VisitCap".to_owned(),
            "CancelledPartial".to_owned(),
            "End".to_owned()
        ])
    );
    println!(
        "Redemption complete reference: 116 rows, {accepts} accept, {unbound} unbound; all 29 alternative reason conditions checked"
    );
}
