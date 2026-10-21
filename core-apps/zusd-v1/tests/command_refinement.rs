//! Exhaustive seed-state/command products; independent integer replay is in tools/.
#![cfg(feature = "std")]
#[path = "support/hmac.rs"]
mod issuer;
#[allow(dead_code)]
#[path = "support/step4.rs"]
mod support;
use std::{fs::File, io::Write};
use support::*;
use zenofcis_zusd_v1::{
    Accumulator256, NICR_Q, Q, application::*, ledger::Quantity, liquidation, redemption,
    stability, staking, vaults,
};
fn array(rows: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", rows.into_iter().collect::<Vec<_>>().join(","))
}
fn hex(id: [u8; 32]) -> String {
    format!(
        "\"{}\"",
        id.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}
fn acc(a: Accumulator256) -> String {
    let (h, l) = a.limbs();
    format!("[{h},{l}]")
}
fn snap(s: stability::Snapshot) -> String {
    format!(
        "[{},{},{},{},{}]",
        s.p,
        acc(s.s),
        acc(s.g),
        s.epoch,
        s.scale
    )
}
fn state(s: &State) -> String {
    let b = s.book();
    let t = s.staking();
    let sp = s.pool();
    let p = &sp.pool;
    let st = p.stock;
    let bounds = stability::Bounds {
        history: 32,
        epochs: 16,
        deposits: 8,
        front_ends: 8,
        vaults: 8,
    };
    let l = *stability::liabilities(sp, bounds, limits())
        .result()
        .unwrap();
    let vaults = array(b.vaults.iter().map(|v| {
        format!(
            "[{},{},{:?},{},{},{},{},{},{}]",
            hex(v.id),
            hex(v.owner),
            format!("{:?}", v.status),
            v.collateral,
            v.debt,
            v.reserve,
            v.stake,
            acc(v.snapshot_c),
            acc(v.snapshot_d)
        )
    }));
    let deposits = array(sp.deposits.iter().map(|d| {
        format!(
            "[{},{},{},{}]",
            hex(d.owner),
            d.initial,
            d.tag.map(hex).unwrap_or("null".into()),
            snap(d.snapshot)
        )
    }));
    let fronts = array(sp.front_ends.iter().map(|f| {
        format!(
            "[{},{},{},[{},{},{},{}]]",
            hex(f.owner),
            f.kickback,
            f.initial,
            f.snapshot.p,
            acc(f.snapshot.g),
            f.snapshot.epoch,
            f.snapshot.scale
        )
    }));
    let positions = array(t.positions.iter().map(|v| {
        format!(
            "[{},{},{},{}]",
            hex(v.owner),
            v.stake,
            acc(v.snapshot_z),
            acc(v.snapshot_a)
        )
    }));
    format!(
        r#"{{"version":{},"epoch":{},"fee":[{},{}],"price":{},"system":[{},{},{},{},{},{}],"default":[{},{}],"gas":{},"vaults":{},"order":{},"surplus":{},"ledger":{},"supply":{},"debt":[{},{}],"backing":[{},{}],"pool":{{"total":{},"p":{},"epoch":{},"scale":{},"collateral":{},"stock":[{},{},{},{},{}],"errors":[{},{},{}],"liabilities":[{},{},{},{}],"history":{},"deposits":{},"fronts":{}}},"staking":{{"total":{},"f":[{},{}],"custody":[{},{}],"zero":[{},{}],"positions":{}}}}}"#,
        s.version(),
        s.clock().epoch,
        s.fee_state().base,
        s.fee_state().last_epoch,
        s.oracle()
            .and_then(|o| o.committed)
            .map_or(0, |r| r.value_e8),
        b.system.collateral,
        b.system.debt,
        b.system.active,
        b.system.stakes,
        acc(b.system.reward_c),
        acc(b.system.reward_d),
        b.default_collateral,
        b.default_debt,
        b.gas_balance,
        vaults,
        array(b.order.iter().map(|i| hex(*i))),
        array(
            b.surplus
                .iter()
                .map(|(i, n)| format!("[{},{}]", hex(*i), n))
        ),
        array(s.ledger().balances.iter().map(|r| format!(
            "[{},{},{}]",
            hex(r.account),
            r.quantity.asset() as u8,
            r.quantity.atoms()
        ))),
        s.ledger().supply,
        s.ledger().active_debt,
        s.ledger().default_debt,
        s.backing().agrs,
        s.backing().zdex,
        p.total,
        p.p,
        p.epoch,
        p.scale,
        p.collateral,
        st.remaining,
        st.scheduled,
        st.allocated,
        st.missed,
        st.paid,
        acc(p.error_s),
        acc(p.error_loss),
        acc(p.error_g),
        l.principal_collective,
        l.residual_only,
        l.collateral_collective,
        l.zdex_collective,
        array(p.history.iter().map(|c| format!(
            "[{},{},{},{}]",
            c.epoch,
            c.scale,
            acc(c.s),
            acc(c.g)
        ))),
        deposits,
        fronts,
        t.total,
        acc(t.f_z),
        acc(t.f_a),
        t.custody_z,
        t.custody_a,
        t.zero_z,
        t.zero_a,
        positions
    )
}
fn delta(d: vaults::Delta) -> String {
    match d {
        vaults::Delta::None => "[0,0]".into(),
        vaults::Delta::Add(n) => format!("[1,{n}]"),
        vaults::Delta::Remove(n) => format!("[-1,{n}]"),
    }
}
fn command(c: &Command) -> String {
    let (kind, args) = match c {
        Command::Open {
            collateral,
            principal,
            ..
        } => ("open", format!("[{collateral},{principal}]")),
        Command::Adjust {
            collateral, debt, ..
        } => (
            "adjust",
            format!("[{},{}]", delta(*collateral), delta(*debt)),
        ),
        Command::Close => ("close", "[]".into()),
        Command::ClaimSurplus => ("surplus", "[]".into()),
        Command::Pool(c) => match c {
            stability::Command::Deposit { amount, tag } => (
                "deposit",
                format!("[{amount},{}]", tag.map(hex).unwrap_or("null".into())),
            ),
            stability::Command::Withdraw(n) => ("withdraw", format!("[{n}]")),
            stability::Command::Claim => ("pool_claim", "[]".into()),
            stability::Command::RegisterFrontEnd(k) => ("register", format!("[{k}]")),
            _ => ("invalid_pool", "[]".into()),
        },
        Command::MoveGain { .. } => ("move_gain", "[]".into()),
        Command::Stake(c) => match c {
            staking::Command::Stake(n) => ("stake", format!("[{n}]")),
            staking::Command::Unstake(n) => ("unstake", format!("[{n}]")),
            staking::Command::Claim => ("stake_claim", "[]".into()),
        },
        Command::Liquidate(c) => match c {
            liquidation::Command::Direct(i) => ("liquidate_direct", format!("[{}]", hex(*i))),
            liquidation::Command::Batch(ids) => {
                ("liquidate_batch", array(ids.iter().map(|i| hex(*i))))
            }
            liquidation::Command::Lowest(n) => ("liquidate_lowest", format!("[{n}]")),
        },
        Command::Redeem(c) => (
            "redeem",
            format!("[{},{},{}]", c.maximum, c.visits, c.partial_nicr),
        ),
        Command::Fund {
            recipient,
            quantity,
            ..
        } => (
            "fund",
            format!(
                "[{},{},{}]",
                hex(*recipient),
                quantity.asset() as u8,
                quantity.atoms()
            ),
        ),
        Command::AdvanceClock => ("clock", "[]".into()),
        Command::ObserveOracle => ("oracle", "[]".into()),
        _ => ("other", "[]".into()),
    };
    format!(r#"{{"kind":"{kind}","args":{args}}}"#)
}
fn check(s: &State) {
    let mut sum = [0u128; 3];
    for r in &s.ledger().balances {
        sum[r.quantity.asset() as usize] += r.quantity.atoms();
    }
    assert_eq!(sum[0], s.ledger().supply);
    assert_eq!(sum[0], s.book().system.debt);
    assert_eq!(sum[1], s.backing().agrs);
    assert_eq!(sum[2], s.backing().zdex);
    assert_eq!(
        s.ledger().active_debt + s.ledger().default_debt,
        s.ledger().supply
    );
}
fn attempt(
    p: &Policy,
    s: &State,
    mut r: Request,
    label: &str,
    log: &mut Option<File>,
    counts: &mut [usize; 2],
) -> Option<State> {
    let frozen = s.clone();
    let policy_before = p.config().clone();
    let signed = sign(p, s, r.clone()).unwrap();
    r.proofs = signed.proofs;
    let e = decide(p, s, &r, &TestVerifier, limits());
    let usage = e.usage();
    let result = e.into_parts().0;
    assert_eq!(*s, frozen);
    assert_eq!(*p.config(), policy_before);
    check(s);
    let row = match &result {
        Ok(out) => {
            counts[0] += 1;
            check(out.post());
            assert_eq!(out.usage(), usage);
            if let Some(f) = log {
                writeln!(f,r#"{{"label":"{label}","actor":{},"command":{},"issuance":{},"pre":{},"post":{},"legs":{},"error":null}}"#,hex(r.actor),command(&r.command),r.drip.as_ref().map_or(0,|d|d.amount),state(s),state(out.post()),array(out.movements().iter().map(|m|format!("[{},{},{},{}]",m.from.map(hex).unwrap_or("null".into()),m.to.map(hex).unwrap_or("null".into()),m.quantity.asset() as u8,m.quantity.atoms())))).unwrap();
            }
            Some(out.post().clone())
        }
        Err(error) => {
            counts[1] += 1;
            if let Some(f) = log {
                writeln!(f,r#"{{"label":"{label}","actor":{},"command":{},"issuance":{},"pre":{},"post":null,"legs":[],"error":"{:?}"}}"#,hex(r.actor),command(&r.command),r.drip.as_ref().map_or(0,|d|d.amount),state(s),error).unwrap();
            }
            None
        }
    };
    row
}
fn perform(p: &Policy, s: &mut State, r: Request, log: &mut Option<File>, counts: &mut [usize; 2]) {
    *s = attempt(p, s, r, "prefix", log, counts).expect("admitted prefix");
}
fn opened(
    p: &Policy,
    s: &mut State,
    who: u8,
    coll: u128,
    principal: u128,
    log: &mut Option<File>,
    counts: &mut [usize; 2],
) {
    let mut r = raw(
        s,
        who,
        Command::Open {
            collateral: coll,
            principal,
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(p, s, &mut r);
    perform(p, s, r, log, counts);
}
fn issued(
    p: &Policy,
    s: &mut State,
    who: u8,
    c: stability::Command,
    event: u8,
    log: &mut Option<File>,
    counts: &mut [usize; 2],
) {
    let mut r = raw(s, who, Command::Pool(c));
    if !matches!(c, stability::Command::RegisterFrontEnd(_)) {
        drip(&mut r, event, 0);
    }
    perform(p, s, r, log, counts);
}
fn drop_price(
    p: &Policy,
    s: &mut State,
    value: u128,
    event: u8,
    log: &mut Option<File>,
    counts: &mut [usize; 2],
) {
    let r = price_request(p, s, OracleAction::ImportPending, 2, value, event);
    perform(p, s, r, log, counts);
    let r = advance_request(p, s, s.clock().epoch + 2, s.clock().watermark + 2);
    perform(p, s, r, log, counts);
    let r = price_request(p, s, OracleAction::CommitMatured, 2, value, event + 1);
    perform(p, s, r, log, counts);
}
fn priced(c: &Command) -> bool {
    matches!(
        c,
        Command::Open { .. }
            | Command::Close
            | Command::Liquidate(_)
            | Command::Redeem(_)
            | Command::Pool(stability::Command::Withdraw(1..))
    ) || matches!(
        c,
        Command::Adjust {
            collateral: vaults::Delta::Remove(_),
            ..
        } | Command::Adjust {
            debt: vaults::Delta::Add(_),
            ..
        }
    )
}
fn product_quotient(a: u128, b: u128, d: u128) -> u128 {
    let (mut q, mut r) = (0u128, 0u128);
    for bit in (0..128).rev() {
        r = r
            .checked_mul(2)
            .unwrap()
            .checked_add(if a >> bit & 1 == 1 { b } else { 0 })
            .unwrap();
        q = q.checked_mul(2).unwrap().checked_add(r / d).unwrap();
        r %= d;
    }
    q
}
fn alphabet(p: &Policy, s: &State) -> Vec<(u8, Command)> {
    let mut cs = Vec::new();
    for who in [1, 2, 3, 4, 5, 6] {
        for c in [0, 700 * Q, 1200 * Q] {
            for principal in [0, 500 * Q, 1000 * Q] {
                cs.push((
                    who,
                    Command::Open {
                        collateral: c,
                        principal,
                        max_fee: Q,
                        upper: None,
                        lower: None,
                    },
                ));
            }
        }
        for c in [
            vaults::Delta::None,
            vaults::Delta::Add(Q),
            vaults::Delta::Remove(Q),
        ] {
            for d in [
                vaults::Delta::None,
                vaults::Delta::Add(Q),
                vaults::Delta::Remove(Q),
            ] {
                cs.push((
                    who,
                    Command::Adjust {
                        collateral: c,
                        debt: d,
                        max_fee: Q,
                        upper: None,
                        lower: None,
                    },
                ));
            }
        }
        cs.extend(
            [
                Command::Close,
                Command::ClaimSurplus,
                Command::MoveGain {
                    upper: None,
                    lower: None,
                },
                Command::Pool(stability::Command::Claim),
                Command::Stake(staking::Command::Claim),
            ]
            .into_iter()
            .map(|c| (who, c)),
        );
        for n in [0, 1, Q] {
            cs.extend(
                [
                    Command::Pool(stability::Command::Deposit {
                        amount: n,
                        tag: None,
                    }),
                    Command::Pool(stability::Command::Deposit {
                        amount: n,
                        tag: Some(id(5)),
                    }),
                    Command::Pool(stability::Command::Withdraw(n)),
                    Command::Stake(staking::Command::Stake(n)),
                    Command::Stake(staking::Command::Unstake(n)),
                ]
                .into_iter()
                .map(|c| (who, c)),
            );
        }
        for k in [0, Q / 2, Q] {
            cs.push((who, Command::Pool(stability::Command::RegisterFrontEnd(k))));
        }
        for maximum in [0, Q, 500 * Q, 1005 * Q, 2010 * Q] {
            // Correct partial hint for the first eligible vault, plus a deliberately absent hint.
            let hint = s
                .book()
                .order
                .iter()
                .rev()
                .find_map(|i| {
                    let v = s.book().vaults.iter().find(|v| v.id == *i)?;
                    let v = *vaults::apply_pending(*v, s.book().system, limits())
                        .result()
                        .ok()?;
                    let price = s.oracle()?.committed?.value_e8;
                    if product_quotient(v.collateral, price * 10_000_000_000, v.debt) < vaults::MCR
                    {
                        return None;
                    }
                    let n = maximum.min(v.debt - v.reserve);
                    let c = v.collateral - n * 100_000_000 / price;
                    Some(product_quotient(c, NICR_Q, v.debt - n))
                })
                .unwrap_or(0);
            for partial_nicr in [0, hint] {
                cs.push((
                    who,
                    Command::Redeem(redemption::Command {
                        maximum,
                        visits: 0,
                        first: None,
                        partial_nicr,
                        upper: None,
                        lower: None,
                        max_fee: Q,
                        min_out: 1,
                    }),
                ));
            }
        }
    }
    for target in 1..=6 {
        cs.push((
            6,
            Command::Liquidate(liquidation::Command::Direct(vault_id(p, id(target)))),
        ));
    }
    cs.push((6, Command::Liquidate(liquidation::Command::Lowest(8))));
    cs.push((
        6,
        Command::Liquidate(liquidation::Command::Batch(
            s.book().order.iter().rev().copied().collect(),
        )),
    ));
    for quantity in [Quantity::Agrs(1), Quantity::Zdex(1)] {
        cs.push((
            6,
            Command::Fund {
                event: id(240),
                recipient: id(1),
                quantity,
            },
        ));
    }
    cs
}
#[test]
fn every_admitted_seed_command_refines_accounting() {
    let mut log = std::env::var_os("ZUSD_REFINEMENT_OUT").map(|p| File::create(p).unwrap());
    let mut counts = [0; 2];
    let p = policy(false);
    let mut s = genesis(&p);
    check(&s);
    let r = advance_request(&p, &s, 2, 2);
    perform(&p, &mut s, r, &mut log, &mut counts);
    let r = price_request(
        &p,
        &s,
        OracleAction::InitializeFirstMatured,
        1,
        200_000_000,
        110,
    );
    perform(&p, &mut s, r, &mut log, &mut counts);
    let mut seeds = vec![s.clone()];
    let r = raw(&s, 1, Command::Stake(staking::Command::Stake(3 * Q)));
    perform(&p, &mut s, r, &mut log, &mut counts);
    opened(&p, &mut s, 1, 1200 * Q, 1000 * Q, &mut log, &mut counts);
    opened(&p, &mut s, 2, 700 * Q, 1000 * Q, &mut log, &mut counts);
    opened(&p, &mut s, 3, 3000 * Q, 2000 * Q, &mut log, &mut counts);
    issued(
        &p,
        &mut s,
        5,
        stability::Command::RegisterFrontEnd(Q / 2),
        120,
        &mut log,
        &mut counts,
    ); // registration does not trigger issuance
    seeds.push(s.clone());
    let mut claimed = s.clone();
    let r = raw(&claimed, 1, Command::Stake(staking::Command::Claim));
    perform(&p, &mut claimed, r, &mut log, &mut counts);
    seeds.push(claimed);
    let mut bare = s.clone();
    drop_price(&p, &mut bare, 150_000_000, 160, &mut log, &mut counts);
    seeds.push(bare);
    for (tag, event) in [(None, 121), (Some(id(5)), 122)] {
        let mut pool = s.clone();
        issued(
            &p,
            &mut pool,
            1,
            stability::Command::Deposit {
                amount: 500 * Q,
                tag,
            },
            event,
            &mut log,
            &mut counts,
        );
        let r = advance_request(&p, &pool, 20_160, 20_160);
        perform(&p, &mut pool, r, &mut log, &mut counts);
        seeds.push(pool.clone());
        for (price, event) in [(150_000_000, 130), (50_000_000, 140)] {
            let mut low = pool.clone();
            drop_price(&p, &mut low, price, event, &mut log, &mut counts);
            seeds.push(low.clone());
            let mut r = raw(
                &low,
                6,
                Command::Liquidate(liquidation::Command::Direct(vault_id(&p, id(2)))),
            );
            use_price(&p, &low, &mut r);
            if price == 150_000_000 {
                drip(&mut r, event + 2, 3 * Q);
            }
            perform(&p, &mut low, r, &mut log, &mut counts);
            seeds.push(low);
        }
        // Full redemption and its surplus/fee claims are in the state domain.
        let mut r = raw(
            &pool,
            3,
            Command::Redeem(redemption::Command {
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
        use_price(&p, &pool, &mut r);
        perform(&p, &mut pool, r, &mut log, &mut counts);
        seeds.push(pool);
    }
    // Recovery with a fully funded capped liquidation, including surplus.
    let mut capped = s.clone();
    issued(
        &p,
        &mut capped,
        3,
        stability::Command::Deposit {
            amount: 1500 * Q,
            tag: None,
        },
        123,
        &mut log,
        &mut counts,
    );
    drop_price(&p, &mut capped, 120_000_000, 150, &mut log, &mut counts);
    seeds.push(capped);
    let seed_count = seeds.len();
    let mut pairs = 0;
    for (i, s) in seeds.iter().enumerate() {
        for (who, c) in alphabet(&p, s) {
            pairs += 1;
            let mut r = raw(s, who, c.clone());
            if priced(&c) {
                use_price(&p, s, &mut r);
            }
            if matches!(
                c,
                Command::Pool(
                    stability::Command::Deposit { .. }
                        | stability::Command::Withdraw(_)
                        | stability::Command::Claim
                ) | Command::MoveGain { .. }
            ) || matches!(c, Command::Liquidate(_)) && s.pool().pool.total > 0
            {
                drip(&mut r, 230, 0);
            }
            if let Command::Fund { quantity, .. } = c {
                match quantity {
                    Quantity::Agrs(n) => r.backing.agrs += n,
                    Quantity::Zdex(n) => r.backing.zdex += n,
                    _ => (),
                }
            }
            let _ = attempt(&p, s, r, &format!("seed-{i}"), &mut log, &mut counts);
        }
    }
    println!(
        "finite domain: seeds={seed_count} command_pairs={pairs} accepted={} refused={}",
        counts[0], counts[1]
    );
    assert!(counts[0] > 100);
    assert!(counts[1] > 100);
}
