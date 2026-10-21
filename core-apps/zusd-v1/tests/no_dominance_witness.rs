//! A fixed-price sequence-law witness, using the existing conditional fixtures.
#![cfg(feature = "std")]
#[path = "support/holdings.rs"]
mod holdings;
#[path = "support/hmac.rs"]
mod issuer;
#[path = "support/requests.rs"]
mod support;
use support::*;
use zenofcis_zusd_v1::{
    NICR_Q, Q, application::*, ledger::Asset, liquidation, redemption, stability, vaults,
};

fn execute(p: &Policy, state: &mut State, request: Request) -> Prepared {
    let pre = state.clone();
    let request = sign(p, &pre, request).unwrap();
    let out = decide(p, &pre, &request, &TestVerifier, limits())
        .into_parts()
        .0
        .unwrap_or_else(|error| panic!("{:?}: {error:?}", request.command));
    assert_eq!(*state, pre);
    if let Some(path) = std::env::var_os("ZUSD_WITNESS_EVIDENCE") {
        let path =
            std::path::PathBuf::from(path).join(std::thread::current().name().unwrap_or("witness"));
        std::fs::create_dir_all(&path).unwrap();
        let prefix = format!("{:03}", pre.version());
        for (suffix, bytes) in [
            ("pre.bin", pre.canonical_bytes()),
            ("post.bin", out.post().canonical_bytes()),
            ("bundle.bin", out.canonical_bytes().to_vec()),
            ("trace.bin", out.trace().to_vec()),
            ("request.txt", format!("{request:#?}\n").into_bytes()),
            (
                "effects.txt",
                format!("{:#?}\n", out.movements()).into_bytes(),
            ),
        ] {
            std::fs::write(path.join(format!("{prefix}-{suffix}")), bytes).unwrap();
        }
        for message in receipt_messages(p, &pre, &request).unwrap() {
            std::fs::write(
                path.join(format!("{prefix}-message-{:?}.bin", message.domain)),
                message.bytes,
            )
            .unwrap();
        }
        std::fs::write(
            path.join("policy.txt"),
            format!("{:#?}\nidentity={:?}\n", p.config(), p.identity()),
        )
        .unwrap();
    }
    *state = out.post().clone();
    out
}

fn opened(p: &Policy, state: &mut State, actor: u8, collateral: u128, principal: u128) {
    let mut request = raw(
        state,
        actor,
        Command::Open {
            collateral,
            principal,
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(p, state, &mut request);
    let _ = execute(p, state, request);
}

fn fallen_price_state(p: &Policy) -> State {
    let mut state = genesis(p);
    let request = advance_request(&p, &state, 2, 2);
    let _ = execute(&p, &mut state, request);
    let request = price_request(
        &p,
        &state,
        OracleAction::InitializeFirstMatured,
        1,
        200_000_000,
        110,
    );
    let _ = execute(&p, &mut state, request);
    opened(&p, &mut state, 1, 1200 * Q, 1000 * Q);
    opened(&p, &mut state, 2, 10_000 * Q, 2000 * Q);
    let mut request = raw(
        &state,
        2,
        Command::Pool(stability::Command::Deposit {
            amount: 1055 * Q,
            tag: None,
        }),
    );
    drip(&mut request, 120, 0);
    let _ = execute(&p, &mut state, request);
    let request = advance_request(&p, &state, 20_160, 20_160);
    let _ = execute(&p, &mut state, request);
    let request = price_request(&p, &state, OracleAction::ImportPending, 2, 50_000_000, 111);
    let _ = execute(&p, &mut state, request);
    let request = advance_request(&p, &state, 20_162, 20_162);
    let _ = execute(&p, &mut state, request);
    let request = price_request(&p, &state, OracleAction::CommitMatured, 2, 50_000_000, 112);
    let _ = execute(&p, &mut state, request);
    state
}
fn liquidate_then_redeem(p: &Policy, before: &State, keeper: u8) -> (State, u128, u128) {
    let mut state = before.clone();
    let mut request = raw(
        &state,
        keeper,
        Command::Liquidate(liquidation::Command::Direct(vault_id(&p, id(1)))),
    );
    use_price(&p, &state, &mut request);
    drip(&mut request, 121, 0);
    let liquidation = execute(&p, &mut state, request);
    let keeper_a = liquidation
        .movements()
        .iter()
        .filter(|m| m.to == Some(id(keeper)) && m.quantity.asset() == Asset::Agrs)
        .map(|m| m.quantity.atoms())
        .sum::<u128>();
    let keeper_z = liquidation
        .movements()
        .iter()
        .filter(|m| m.to == Some(id(keeper)) && m.quantity.asset() == Asset::Zusd)
        .map(|m| m.quantity.atoms())
        .sum::<u128>();
    assert_eq!((keeper_a, keeper_z), (6 * Q, 50 * Q));
    let mut request = raw(
        &state,
        1,
        Command::Redeem(redemption::Command {
            maximum: 1000 * Q,
            visits: 0,
            first: None,
            partial_nicr: 8000 * NICR_Q / 1060,
            upper: None,
            lower: None,
            max_fee: Q,
            min_out: 1,
        }),
    );
    use_price(&p, &state, &mut request);
    let redemption = execute(&p, &mut state, request);
    let fraction = 1000 * Q / 2060;
    let exact_fee = 2000 * (Q / 200 + fraction / 2);
    let expected_payout = 2000 * Q - exact_fee;
    let actual_payout = redemption
        .movements()
        .iter()
        .filter(|m| m.to == Some(id(1)) && m.quantity.asset() == Asset::Agrs)
        .map(|m| m.quantity.atoms())
        .sum::<u128>();
    assert_eq!(
        actual_payout, expected_payout,
        "independent V1 integer payout"
    );
    assert_eq!(state.clock(), before.clock());
    assert_eq!(state.oracle(), before.oracle());
    assert_eq!(state.backing(), before.backing());
    assert_eq!(
        state
            .book()
            .vaults
            .iter()
            .find(|v| v.owner == id(1))
            .unwrap()
            .status,
        vaults::Status::ClosedLiquidation
    );
    (state, keeper_a, keeper_z)
}
#[test]
fn self_liquidation_then_redemption_cannot_dominate_after_exact_benefit_subtraction() {
    let p = policy(false);
    let before = fallen_price_state(&p);
    let own = before
        .book()
        .vaults
        .iter()
        .find(|v| v.owner == id(1))
        .unwrap();
    assert_eq!(
        (own.collateral, own.debt, own.reserve),
        (1200 * Q, 1055 * Q, 50 * Q)
    );
    let (after, ka, kz) = liquidate_then_redeem(&p, &before, 1);
    let start = holdings::holdings(&before, 1);
    let raw_end = holdings::holdings(&after, 1);
    let end = [raw_end[0] - ka as i128, raw_end[1] - kz as i128, raw_end[2]];
    let q = Q as i128;
    let payout = 2000 * Q - 2000 * (Q / 200 + (1000 * Q / 2060) / 2);
    assert_eq!(start, [18800 * q, 1050 * q, 1000 * q]);
    assert_eq!(raw_end, [(18806 * Q + payout) as i128, 50 * q, 1000 * q]);
    assert_eq!(end, [(18800 * Q + payout) as i128, 0, 1000 * q]);
    assert!(!holdings::dominates(end, start));
    println!(
        "NON_RECOURSE before={start:?} raw_after={raw_end:?} adjusted_after={end:?} keeper={:?}",
        [ka, kz, 0]
    );
}
#[test]
fn pool_shortfall_is_exact_and_third_party_liquidation_changes_only_keeper_compensation() {
    let p = policy(false);
    let before = fallen_price_state(&p);
    let (own, ka, kz) = liquidate_then_redeem(&p, &before, 1);
    let (other, other_ka, other_kz) = liquidate_then_redeem(&p, &before, 6);
    assert_eq!((ka, kz), (other_ka, other_kz));
    assert_eq!(before.pool().pool.total, 1055 * Q);
    assert_eq!(own.pool().pool.total, 0);
    assert_eq!(own.pool(), other.pool());
    assert_eq!(own.book(), other.book());
    assert_eq!(own.staking(), other.staking());
    assert_eq!(own.pool().pool.collateral, 1194 * Q);
    assert_eq!(
        [before.pool().pool.collateral, before.pool().pool.total, 0],
        [0, 1055 * Q, 0]
    );
    assert_eq!(
        [own.pool().pool.collateral, own.pool().pool.total, 0],
        [1194 * Q, 0, 0]
    );
    let borrower_own = holdings::holdings(&own, 1);
    let borrower_other = holdings::holdings(&other, 1);
    assert_eq!(
        borrower_other,
        [
            borrower_own[0] - ka as i128,
            borrower_own[1] - kz as i128,
            borrower_own[2]
        ]
    );
    let depositor_claim = holdings::rewards(&own, 2)[0];
    assert_eq!(depositor_claim, (1194 * Q - 260) as i128);
    assert_eq!(own.pool().pool.collateral - depositor_claim as u128, 260);
    assert_eq!(
        1055 * Q - holdings::muldiv(depositor_claim as u128, 50_000_000, 100_000_000),
        458 * Q + 130
    );
    let received_value = holdings::muldiv(own.pool().pool.collateral, 50_000_000, 100_000_000);
    let loss = before.pool().pool.total - received_value;
    let payout = 1_504_563_106_796_116_506_000u128;
    assert_eq!(
        borrower_other,
        [(18800 * Q + payout) as i128, 0, (1000 * Q) as i128]
    );
    assert_eq!(received_value, 597 * Q);
    assert_eq!(loss, 458 * Q);
    println!(
        "POOL debt_absorbed={} collateral_received={} collateral_value={} loss={} borrower_third_party={borrower_other:?}",
        1055 * Q,
        1194 * Q,
        received_value,
        loss
    );
}
