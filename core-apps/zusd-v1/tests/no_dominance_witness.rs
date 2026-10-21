//! A fixed-price sequence-law witness, using the existing conditional fixtures.
#![cfg(feature = "std")]
#[path = "support/hmac.rs"]
mod issuer;
#[path = "support/step4.rs"]
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
        let path = std::path::PathBuf::from(path);
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
    execute(p, state, request);
}

#[test]
#[ignore = "owner decision pending: insolvent-vault accounting"]
fn self_liquidation_then_redemption_cannot_dominate_after_exact_benefit_subtraction() {
    let p = policy(false);
    let mut state = genesis(&p);
    let request = advance_request(&p, &state, 2, 2);
    execute(&p, &mut state, request);
    let request = price_request(
        &p,
        &state,
        OracleAction::InitializeFirstMatured,
        1,
        200_000_000,
        110,
    );
    execute(&p, &mut state, request);
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
    execute(&p, &mut state, request);
    let request = advance_request(&p, &state, 20_160, 20_160);
    execute(&p, &mut state, request);
    let request = price_request(&p, &state, OracleAction::ImportPending, 2, 50_000_000, 111);
    execute(&p, &mut state, request);
    let request = advance_request(&p, &state, 20_162, 20_162);
    execute(&p, &mut state, request);
    let request = price_request(&p, &state, OracleAction::CommitMatured, 2, 50_000_000, 112);
    execute(&p, &mut state, request);
    let before = state.clone();
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
    assert_eq!(before.pool().pool.total, 1055 * Q);
    // Include the entire gas reserve as owned escrow, a conservative addition
    // to the borrower's starting holdings even though it cannot be withdrawn.
    let start_a = balance(&before, 1, Asset::Agrs) + own.collateral;
    let start_z = balance(&before, 1, Asset::Zusd) as i128 + own.reserve as i128 - own.debt as i128;
    let start_t = balance(&before, 1, Asset::Zdex);
    let mut request = raw(
        &state,
        1,
        Command::Liquidate(liquidation::Command::Direct(vault_id(&p, id(1)))),
    );
    use_price(&p, &state, &mut request);
    drip(&mut request, 121, 0);
    let liquidation = execute(&p, &mut state, request);
    let keeper_a = liquidation
        .movements()
        .iter()
        .filter(|m| m.to == Some(id(1)) && m.quantity.asset() == Asset::Agrs)
        .map(|m| m.quantity.atoms())
        .sum::<u128>();
    let keeper_z = liquidation
        .movements()
        .iter()
        .filter(|m| m.to == Some(id(1)) && m.quantity.asset() == Asset::Zusd)
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
    let end_a = balance(&state, 1, Asset::Agrs) - keeper_a;
    let end_z = balance(&state, 1, Asset::Zusd) as i128 - keeper_z as i128;
    let end_t = balance(&state, 1, Asset::Zdex);
    println!(
        "NO_FREE_VALUE before=({start_a},{start_z},{start_t}) adjusted_after=({end_a},{end_z},{end_t}) keeper=({keeper_a},{keeper_z},0) fee={exact_fee} payout={expected_payout}"
    );
    println!(
        "NO_FREE_VALUE start_root={:?} end_root={:?} actor=1 sequence=LiquidateOwn,Redeem epoch=20162 price_e8=50000000",
        before.root(),
        state.root()
    );
    let dominates = end_a >= start_a
        && end_z >= start_z
        && end_t >= start_t
        && (end_a > start_a || end_z > start_z || end_t > start_t);
    assert!(
        !dominates,
        "NO_FREE_VALUE_SELF_LIQUIDATION_REDEMPTION: exact permitted benefits subtracted"
    );
}
