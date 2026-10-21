use std::collections::BTreeSet;
use zenofcis_zusd_v1::{stability as sp, staking as st, vaults as v, *};
fn id(x: u8) -> Id {
    let mut id = [0; 32];
    id[31] = x;
    id
}
fn limits() -> Limits {
    Limits {
        steps: 100_000_000,
        reads: 10_000_000,
        bytes: 10_000_000,
    }
}
fn bounds() -> sp::Bounds {
    sp::Bounds {
        history: 8,
        epochs: 4,
        deposits: 2,
        front_ends: 2,
        vaults: 2,
    }
}
fn prices() -> v::Prices {
    v::Prices {
        committed: 100_000_000,
        pending: None,
        status: v::PriceStatus::Usable,
        pending_blocked: false,
    }
}
fn system() -> v::System {
    v::System {
        collateral: 3000 * Q,
        debt: 2000 * Q,
        active: 2,
        stakes: 3000 * Q,
        stakes_snapshot: 0,
        collateral_snapshot: 0,
        reward_c: 0,
        reward_d: 0,
    }
}
fn vault(x: u8) -> v::Vault {
    v::Vault {
        parameter_version: 1,
        id: id(x),
        owner: id(x),
        status: v::Status::Active,
        collateral: 1500 * Q,
        debt: 1000 * Q,
        reserve: 50 * Q,
        stake: 1500 * Q,
        snapshot_c: 0,
        snapshot_d: 0,
    }
}
include!("test-data/sp_positions.rs");
include!("test-data/staking_positions.rs");
/// Finite row relation: all ordered F_i, legitimate acceptance, failure
/// prohibition and exact complete outcomes. Reference rows come solely from
/// the Python integer relation, never from a producer case or law 991.
fn lawful<T: PartialEq>(candidate: &Result<T, Failure>, reference: &Result<T, Failure>) -> bool {
    match candidate {
        Err(Failure::Reject(id)) => {
            common::REASONS.contains(id) && reference.as_ref().err() == Some(&Failure::Reject(*id))
        }
        Ok(value) => reference.as_ref().ok() == Some(value),
        Err(other) => {
            matches!(reference, Err(Failure::UnboundProfile)) && *other == Failure::UnboundProfile
        }
    }
}
fn row<T: PartialEq + std::fmt::Debug>(
    candidate: Result<T, Failure>,
    reference: Result<T, Failure>,
) {
    assert!(
        lawful(&candidate, &reference),
        "candidate={candidate:?}\nreference={reference:?}"
    );
    let mut legal = usize::from(reference.is_ok());
    if reference == Err(Failure::UnboundProfile) {
        legal += 1;
    }
    for id in common::REASONS {
        let alternative = Err(Failure::Reject(*id));
        let allowed = lawful(&alternative, &reference);
        assert_eq!(
            allowed,
            reference.as_ref().err() == Some(&Failure::Reject(*id))
        );
        legal += usize::from(allowed);
    }
    assert_eq!(
        legal, 1,
        "zero/reject-all/multiple lawful complete outcomes"
    );
    if reference.is_ok() {
        assert!(!lawful(&Err(Failure::UnboundProfile), &reference));
    }
}
#[test]
fn sp_complete_record_relation_all_rows_and_exact_frame_control() {
    let cases = sp_cases();
    assert_eq!(cases.len(), 153);
    let mut reasons = BTreeSet::new();
    let mut accept = 0;
    let mut unbound = 0;
    let mut frame_control = false;
    for (pre, actor, cmd, wallet, issuance, reference) in cases {
        let frozen = pre.clone();
        let out = sp::change(
            &pre,
            actor,
            cmd,
            wallet,
            issuance,
            prices(),
            &[vault(1), vault(2)],
            system(),
            bounds(),
            limits(),
        );
        assert_eq!(pre, frozen);
        match &reference {
            Ok(expected) => {
                accept += 1;
                if !frame_control {
                    let mut fault = expected.clone();
                    fault.book.pool.stock.remaining -= 1;
                    assert!(!lawful(&Ok(fault), &reference));
                    frame_control = true;
                }
            }
            Err(Failure::Reject(r)) => {
                reasons.insert(*r);
            }
            Err(Failure::UnboundProfile) => unbound += 1,
            _ => panic!(),
        }
        row(out.into_parts().0, reference);
    }
    assert!(accept > 0 && unbound > 0 && frame_control);
    assert_eq!(
        reasons,
        BTreeSet::from([1010, 1011, 1012, 1033, 1045, 1061])
    );
    println!(
        "SP complete reference: 153 rows, {accept} accept, {unbound} unbound; all 29 alternative reason conditions checked"
    );
}
#[test]
fn staking_complete_record_relation_all_rows_and_exact_frame_control() {
    let cases = staking_cases();
    assert_eq!(cases.len(), 432);
    let mut reasons = BTreeSet::new();
    let mut accept = 0;
    for (pre, actor, cmd, wallet, reference) in cases {
        let frozen = pre.clone();
        let out = st::change(&pre, actor, cmd, wallet, 2, limits());
        assert_eq!(pre, frozen);
        if let Ok(expected) = &reference {
            accept += 1;
            let mut fault = expected.clone();
            fault.state.zero_a += 1;
            assert!(!lawful(&Ok(fault), &reference));
        }
        if let Err(Failure::Reject(r)) = &reference {
            reasons.insert(*r);
        }
        row(out.into_parts().0, reference);
    }
    assert!(accept > 0);
    assert_eq!(reasons, BTreeSet::from([1010, 1011, 1033, 1061]));
    println!(
        "Staking complete reference: 432 rows, {accept} accept; all 29 alternative reason conditions checked"
    );
}
