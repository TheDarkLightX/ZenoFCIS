//! State-threaded observations for the archived-reference comparison.
#![cfg(feature = "std")]
#[path = "support/hmac.rs"]
mod issuer;
#[path = "support/step4.rs"]
mod support;
use support::*;
use zenofcis_zusd_v1::{Failure, Q, application::*, ledger::Asset, liquidation, stability, vaults};
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn record(
    p: &Policy,
    s: &mut State,
    mut r: Request,
    label: &str,
    kind: &str,
    who: u8,
    amount: u128,
    expected: Result<(), Failure>,
) {
    let pre = s.clone();
    if matches!(
        r.command,
        Command::Adjust { .. }
            | Command::Liquidate(_)
            | Command::Pool(stability::Command::Withdraw(_))
    ) {
        use_price(p, s, &mut r);
    }
    let r = sign(p, s, r).unwrap();
    let result = decide(p, s, &r, &TestVerifier, limits()).into_parts().0;
    assert_eq!(
        result.as_ref().map(|_| ()).map_err(|e| *e),
        expected,
        "{label}"
    );
    assert_eq!(*s, pre);
    let (post, legs) = match result {
        Ok(out) => (out.post().clone(), format!("{:?}", out.movements())),
        Err(_) => (pre.clone(), String::from("[]")),
    };
    if let Some(dir) = std::env::var_os("ZUSD_REPLAY_OUTPUT") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let v = pre
            .book()
            .vaults
            .iter()
            .find(|v| v.owner == id(who) && v.status == vaults::Status::Active);
        let o = pre.oracle().unwrap();
        let c = v.map_or(0, |v| v.collateral);
        let d = v.map_or(0, |v| v.debt);
        let pool = pre.pool().pool.total.min(d);
        let after = post
            .book()
            .vaults
            .iter()
            .find(|v| v.owner == id(who) && v.status == vaults::Status::Active);
        let tsv = format!(
            "{label}\t{kind}\t{amount}\t{}\t{}\t{}\t{c}\t{d}\t{pool}\t{}\t{}\t{:?}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            pre.clock().epoch,
            o.committed.unwrap().value_e8,
            o.pending
                .map_or(o.committed.unwrap().value_e8, |v| v.value_e8),
            post.book().system.debt,
            post.ledger().supply,
            expected,
            after.map_or(0, |v| v.collateral),
            after.map_or(0, |v| v.debt),
            post.pool().pool.total,
            post.book().default_collateral,
            post.book().default_debt,
            post.book().gas_balance,
            balance(&post, 6, Asset::Agrs),
            balance(&post, 6, Asset::Zusd)
        );
        std::fs::write(dir.join(format!("{label}.tsv")), tsv).unwrap();
        for (suffix, data) in [
            ("pre.hex", hex(&pre.canonical_bytes())),
            ("post.hex", hex(&post.canonical_bytes())),
            ("request.txt", format!("{r:#?}")),
            ("effects.txt", legs),
        ] {
            std::fs::write(dir.join(format!("{label}-{suffix}")), data).unwrap();
        }
    }
    *s = post;
}
#[test]
fn v1_departures_retain_exact_state_threaded_inputs_and_decisions() {
    let p = policy(false);
    let mut s = ready(&p);
    open(&p, &mut s, 1, 1200 * Q, 1000 * Q);
    open(&p, &mut s, 2, 10000 * Q, 6000 * Q);
    let mut r = raw(
        &s,
        2,
        Command::Pool(stability::Command::Deposit {
            amount: 1100 * Q,
            tag: None,
        }),
    );
    drip(&mut r, 119, 0);
    let _ = step(&p, &mut s, r);
    let r = price_request(&p, &s, OracleAction::ImportPending, 2, 300_000_000, 111);
    record(
        &p,
        &mut s,
        r,
        "upward-report",
        "oracle_report",
        1,
        300_000_000,
        Ok(()),
    );
    let r = raw(
        &s,
        1,
        Command::Adjust {
            collateral: vaults::Delta::None,
            debt: vaults::Delta::Add(Q),
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    record(
        &p,
        &mut s,
        r,
        "upward-pending-borrow",
        "mint_zusd",
        1,
        Q,
        Ok(()),
    );
    let r = price_request(&p, &s, OracleAction::ImportPending, 3, 50_000_000, 112);
    record(
        &p,
        &mut s,
        r,
        "pending-drop",
        "oracle_report",
        1,
        50_000_000,
        Ok(()),
    );
    let mut r = raw(
        &s,
        6,
        Command::Liquidate(liquidation::Command::Direct(vault_id(&p, id(1)))),
    );
    drip(&mut r, 122, 0);
    record(
        &p,
        &mut s,
        r,
        "pending-seizure",
        "liquidate",
        1,
        0,
        Err(Failure::Reject(1044)),
    );
    let mut r = raw(&s, 2, Command::Pool(stability::Command::Withdraw(Q)));
    drip(&mut r, 120, 0);
    record(
        &p,
        &mut s,
        r,
        "pending-withdraw",
        "withdraw_sp",
        1,
        Q,
        Err(Failure::Reject(1044)),
    );
    advance(&p, &mut s, 4, 4);
    let r = price_request(&p, &s, OracleAction::CommitMatured, 3, 50_000_000, 113);
    record(
        &p,
        &mut s,
        r,
        "insolvent-commit",
        "oracle_commit",
        1,
        0,
        Ok(()),
    );
    let r = raw(
        &s,
        6,
        Command::Liquidate(liquidation::Command::Direct(vault_id(&p, id(1)))),
    );
    record(
        &p,
        &mut s,
        r,
        "recovery-redistribution",
        "liquidate",
        1,
        0,
        Ok(()),
    );
    assert!(s.book().default_debt > 0);
    assert_eq!(s.ledger().supply, s.book().system.debt);
    assert_eq!(balance(&s, 6, Asset::Zusd), 50 * Q);
}
