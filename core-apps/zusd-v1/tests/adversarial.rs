// Permanent Round 3 regressions adapted from the independent adversarial driver.
use zenofcis_zusd_v1::{
    composition, fees, liquidation as l, redemption as r, stability as sp, staking as st,
    vaults as v, *,
};
fn limits() -> Limits {
    Limits {
        steps: 1_000_000_000,
        reads: 10_000_000,
        bytes: 100_000_000,
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
fn system0() -> v::System {
    v::System {
        collateral: 0,
        debt: 0,
        active: 0,
        stakes: 0,
        stakes_snapshot: 0,
        collateral_snapshot: 0,
        reward_c: Accumulator256::ZERO,
        reward_d: Accumulator256::ZERO,
    }
}
fn vault(x: u8, c: u128, d: u128) -> v::Vault {
    v::Vault {
        parameter_version: 1,
        id: id(x),
        owner: id(x),
        status: v::Status::Active,
        collateral: c,
        debt: d,
        reserve: 50 * Q,
        stake: c,
        snapshot_c: Accumulator256::ZERO,
        snapshot_d: Accumulator256::ZERO,
    }
}
fn book(vaults: Vec<v::Vault>, order: Vec<Id>, pool: sp::State) -> l::Book {
    let s = v::System {
        collateral: vaults.iter().map(|v| v.collateral).sum(),
        debt: vaults.iter().map(|v| v.debt).sum(),
        active: vaults.len() as u32,
        stakes: vaults.iter().map(|v| v.stake).sum(),
        ..system0()
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
        error_c: Accumulator256::ZERO,
        error_d: Accumulator256::ZERO,
        gas_balance,
        surplus: vec![],
        pool,
    }
}
fn genesis_pool() -> sp::Book {
    sp::Book {
        pool: sp::State::reference_genesis(sp::FUNDED_CAP).unwrap(),
        deposits: vec![],
        front_ends: vec![],
    }
}
fn deposit(
    b: &sp::Book,
    who: u8,
    amount: u128,
    tag: Option<Id>,
    issuance: u128,
) -> Result<sp::Outcome, Failure> {
    sp::change(
        sp::ChangeInputs {
            pre: b,
            actor: id(who),
            command: sp::Command::Deposit { amount, tag },
            wallet_zusd: amount,
            issuance: Some(issuance),
            prices: prices(),
            inventory: &[],
            system: system0(),
            bounds: bounds(),
        },
        limits(),
    )
    .into_parts()
    .0
}
fn withdraw(b: &sp::Book, who: u8, x: u128, issuance: u128) -> Result<sp::Outcome, Failure> {
    sp::change(
        sp::ChangeInputs {
            pre: b,
            actor: id(who),
            command: sp::Command::Withdraw(x),
            wallet_zusd: 0,
            issuance: Some(issuance),
            prices: prices(),
            inventory: &[],
            system: system0(),
            bounds: bounds(),
        },
        limits(),
    )
    .into_parts()
    .0
}

#[test]
fn w1_sp_g_accepts_one_atom_and_all_35_funded_drips() {
    let one = deposit(&genesis_pool(), 7, 1, None, 0).unwrap().book;
    let frozen = one.clone();
    for i in [340, 341] {
        let out = deposit(&one, 1, 1000 * Q, None, i).unwrap();
        assert!(out.book.pool.history[0].g > Accumulator256::ZERO);
        let exit = withdraw(&one, 7, 1, i).unwrap();
        assert_eq!((exit.principal_out, exit.gains.depositor_zdex), (1, i));
    }
    let vs = vec![
        vault(1, 1050 * Q, 1000 * Q),
        vault(2, 3000 * Q, 1000 * Q),
        vault(3, 3000 * Q, 1000 * Q),
    ];
    let attacked = book(vs.clone(), vec![id(2), id(3), id(1)], one.pool.clone());
    let a = l::liquidate(
        &attacked,
        &l::Command::Direct(id(1)),
        prices(),
        Some(341),
        3,
        bounds(),
        limits(),
    )
    .into_parts()
    .0
    .unwrap();
    assert_eq!((a.burned, a.redistributed_debt), (1, 1000 * Q - 1));
    let clean = book(vs, vec![id(2), id(3), id(1)], genesis_pool().pool);
    let c = l::liquidate(
        &clean,
        &l::Command::Direct(id(1)),
        prices(),
        Some(341),
        3,
        bounds(),
        limits(),
    )
    .into_parts()
    .0
    .unwrap();
    assert_eq!((c.burned, c.redistributed_debt), (0, 1000 * Q));
    let mut b = deposit(&genesis_pool(), 1, 100_000 * Q, None, 0)
        .unwrap()
        .book;
    for _ in 1..=35 {
        b = deposit(&b, 2, 1, None, 1_000_000 * Q).unwrap().book;
    }
    assert_eq!(
        (b.pool.stock.scheduled, b.pool.stock.remaining),
        (sp::FUNDED_CAP, 0)
    );
    assert!(b.pool.history[0].g.limbs().0 > 0);
    let small = deposit(&genesis_pool(), 1, 1000 * Q, None, 0).unwrap().book;
    for i in [340_000 * Q, 341_000 * Q] {
        assert!(deposit(&small, 2, Q, None, i).is_ok());
    }
    assert_eq!(one, frozen);
}
fn empty_staking() -> st::State {
    st::State {
        total: 0,
        f_z: Accumulator256::ZERO,
        f_a: Accumulator256::ZERO,
        custody_z: 0,
        custody_a: 0,
        zero_z: 0,
        zero_a: 0,
        positions: vec![],
    }
}
#[test]
fn w2_one_atom_staker_does_not_block_fee_events() {
    let one = st::change(
        &empty_staking(),
        id(7),
        st::Command::Stake(1),
        1,
        4,
        limits(),
    )
    .into_parts()
    .0
    .unwrap()
    .state;
    for a in [340 * Q, 341 * Q] {
        let s = st::fee_event(&one, 0, a, 4, limits())
            .into_parts()
            .0
            .unwrap();
        let c = st::change(&s, id(7), st::Command::Claim, 0, 4, limits())
            .into_parts()
            .0
            .unwrap();
        assert_eq!(c.agrs_gain, a);
    }
    let mut s = one;
    // Original driver looped until overflow; retain the crossing in a bounded test.
    for _ in 0..140 {
        s = st::fee_event(&s, 25 * Q / 10, 0, 4, limits())
            .into_parts()
            .0
            .unwrap();
    }
    assert!(s.f_z.limbs().0 > 0);
    let claim = st::change(&s, id(7), st::Command::Claim, 0, 4, limits())
        .into_parts()
        .0
        .unwrap();
    assert_eq!(claim.zusd_gain, 350 * Q);
    let exited = st::change(&claim.state, id(7), st::Command::Unstake(1), 0, 4, limits())
        .into_parts()
        .0
        .unwrap()
        .state;
    let joined = st::change(
        &exited,
        id(8),
        st::Command::Stake(1_000_000 * Q),
        1_000_000 * Q,
        4,
        limits(),
    )
    .into_parts()
    .0
    .unwrap()
    .state;
    let next = st::fee_event(&joined, 374_000 * Q, 0, 4, limits())
        .into_parts()
        .0
        .unwrap();
    assert_eq!(
        st::change(&next, id(8), st::Command::Claim, 0, 4, limits())
            .result()
            .unwrap()
            .zusd_gain,
        374_000 * Q
    );
}
#[test]
fn w2b_precise_pump_then_400_honest_fee_events_accept() {
    let one = st::change(
        &empty_staking(),
        id(7),
        st::Command::Stake(1),
        1,
        4,
        limits(),
    )
    .into_parts()
    .0
    .unwrap()
    .state;
    let y = u128::MAX / Q;
    let pump = st::fee_event(&one, y, 0, 4, limits())
        .into_parts()
        .0
        .unwrap();
    let next = st::fee_event(&pump, 1, 0, 4, limits())
        .into_parts()
        .0
        .unwrap();
    let c = st::change(&next, id(7), st::Command::Claim, 0, 4, limits())
        .into_parts()
        .0
        .unwrap();
    assert_eq!(c.zusd_gain, y + 1);
    let mut s = st::change(
        &c.state,
        id(8),
        st::Command::Stake(1_000_000 * Q),
        1_000_000 * Q,
        4,
        limits(),
    )
    .into_parts()
    .0
    .unwrap()
    .state;
    for _ in 0..400 {
        s = st::fee_event(&s, 1000 * Q, 0, 4, limits())
            .into_parts()
            .0
            .unwrap();
    }
    assert_eq!(s.custody_z, 400_000 * Q);
    assert!(s.f_z > pump.f_z);
}
#[test]
fn w3_front_end_aggregate_retains_v1_rounding_refusal() {
    let fe = sp::change(
        sp::ChangeInputs {
            pre: &genesis_pool(),
            actor: id(9),
            command: sp::Command::RegisterFrontEnd(Q / 2),
            wallet_zusd: 0,
            issuance: None,
            prices: prices(),
            inventory: &[],
            system: system0(),
            bounds: bounds(),
        },
        limits(),
    )
    .into_parts()
    .0
    .unwrap()
    .book;
    let mut a = deposit(&fe, 1, 5, Some(id(9)), 0).unwrap().book;
    a.pool = sp::offset(&a.pool, 2, 0, Some(0), bounds(), limits())
        .into_parts()
        .0
        .unwrap();
    assert_eq!(a.pool.p, 6 * Q / 10 - 1);
    let mut b = deposit(&a, 2, 3, Some(id(9)), 0).unwrap().book;
    b.pool = sp::offset(&b.pool, 4, 0, Some(0), bounds(), limits())
        .into_parts()
        .0
        .unwrap();
    assert_eq!(b.pool.p, Q / 5);
    let out = withdraw(&b, 1, u128::MAX, 0).unwrap();
    assert_eq!((out.principal_out, out.book.front_ends[0].initial), (1, 0));
    let frozen = out.book.clone();
    assert_eq!(
        withdraw(&out.book, 2, u128::MAX, 0),
        Err(Failure::Arithmetic)
    );
    assert_eq!(out.book, frozen);
    // Claiming rewards with zero principal withdrawn remains available.
    assert!(withdraw(&out.book, 2, 0, 0).is_ok());
}
#[test]
fn w4_compositional_liquidate_gain_to_vault_self_redeem_withdraw() {
    // Victim 1 at 105%, attacker 2 at 160%, bystander 3 at 300%; price $1.
    // Bystander SP 1000 zUSD; attacker SP 1000 zUSD; attacker wallet 3000 zUSD.
    let mut pool = genesis_pool();
    pool = deposit(&pool, 3, 1000 * Q, None, 0).unwrap().book;
    pool = deposit(&pool, 2, 1000 * Q, None, 0).unwrap().book;
    let vs = vec![
        vault(1, 1050 * Q, 1000 * Q),
        vault(2, 1600 * Q, 1000 * Q),
        vault(3, 3000 * Q, 1000 * Q),
    ];
    let b0 = book(vs, vec![id(3), id(2), id(1)], pool.pool.clone());
    let sys = |b: &l::Book| {
        (
            b.system.collateral,
            b.system.debt,
            b.default_collateral,
            b.default_debt,
            b.gas_balance,
            b.pool.total,
            b.pool.collateral,
        )
    };
    println!("W4 s0 sys(c,d,defC,defD,gas,SP_D,SP_C)={:?}", sys(&b0));
    // 1. attacker liquidates the victim.
    let liq = l::liquidate(
        &b0,
        &l::Command::Direct(id(1)),
        prices(),
        Some(0),
        3,
        bounds(),
        limits(),
    )
    .into_parts()
    .0
    .unwrap();
    println!("W4 s1 partition={:?}", liq.partitions[0]);
    println!(
        "W4 s1 sys={:?} keeper z={} c={}",
        sys(&liq.book),
        liq.keeper_zusd,
        liq.keeper_collateral
    );
    pool.pool = liq.book.pool.clone();
    let lb = sp::liabilities(&pool, bounds(), limits())
        .into_parts()
        .0
        .unwrap();
    println!("W4 s1 SP liabilities {:?}", lb);
    // 2. attacker moves its SP collateral gain into its vault.
    let g = composition::move_gain_to_vault(
        composition::GainToVaultInputs {
            pre: &liq.book,
            pool: &pool,
            actor: id(2),
            parameters: v::Parameters::v1(),
            fee: fee(),
            prices: prices(),
            issuance: Some(0),
            upper: None,
            lower: None,
            capacity: 3,
            bounds: bounds(),
        },
        limits(),
    )
    .into_parts()
    .0
    .unwrap();
    println!(
        "W4 s2 gain={} attacker vault={:?}",
        g.settlement.collateral,
        (g.vaults.vaults[1].collateral, g.vaults.vaults[1].debt)
    );
    println!(
        "W4 s2 sys={:?} order={:?}",
        sys(&g.vaults),
        g.vaults.order.iter().map(|x| x[31]).collect::<Vec<_>>()
    );
    let pool2 = g.stability.clone();
    // 3. attacker redeems 500 zUSD against the lowest eligible vault (its own).
    let holder = r::Holder {
        zusd: 3000 * Q,
        agrs: 0,
        staking_agrs: 0,
        fee: fee(),
    };
    let lowest = *g.vaults.order.last().unwrap();
    let target = g.vaults.vaults.iter().find(|x| x.id == lowest).unwrap();
    let pnicr = *mul_div(
        target.collateral - 400 * Q,
        NICR_Q,
        target.debt - 400 * Q,
        limits(),
    )
    .result()
    .unwrap();
    let cmd = r::Command {
        maximum: 400 * Q,
        visits: 0,
        first: None,
        partial_nicr: pnicr,
        upper: None,
        lower: None,
        max_fee: Q,
        min_out: 0,
    };
    let red = r::redeem(
        r::RedeemInputs {
            pre: &g.vaults,
            holder,
            command: cmd,
            prices: prices(),
            time: time(),
            bootstrap_minutes: Some(20160),
            parameters: v::Parameters::v1(),
            capacity: 3,
            bounds: bounds(),
        },
        limits(),
    )
    .into_parts()
    .0
    .inspect_err(|e| {
        println!("W4 redeem err {e:?}");
    })
    .unwrap();
    println!(
        "W4 s3 trace X={} G={} F={} stop={:?} visited={:?}",
        red.trace.executed,
        red.trace.gross,
        red.trace.fee,
        red.trace.stop,
        red.trace.visited.iter().map(|x| x[31]).collect::<Vec<_>>()
    );
    println!(
        "W4 s3 sys={:?} attacker vault={:?}",
        sys(&red.book),
        (red.book.vaults[1].collateral, red.book.vaults[1].debt)
    );
    // 4. attacker withdraws all SP principal.
    let mut pool3 = pool2.clone();
    pool3.pool = red.book.pool.clone();
    let wd = sp::change(
        sp::ChangeInputs {
            pre: &pool3,
            actor: id(2),
            command: sp::Command::Withdraw(u128::MAX),
            wallet_zusd: 0,
            issuance: Some(0),
            prices: prices(),
            inventory: &red.book.vaults,
            system: red.book.system,
            bounds: bounds(),
        },
        limits(),
    )
    .into_parts()
    .0
    .unwrap();
    println!(
        "W4 s4 out principal={} coll={} SP_D={}",
        wd.principal_out, wd.gains.collateral, wd.book.pool.total
    );
    // Attacker net vector (AGRS, zUSD wallet+SP, -debt) vs start, at $1.
    let start_agrs = 1600 * Q; // vault collateral
    let start_z = 3000 * Q + 1000 * Q; // wallet + SP principal
    let start_debt = 1000 * Q;
    let end_agrs = red.book.vaults[1].collateral
        + red.holder.agrs
        + liq.keeper_collateral
        + wd.gains.collateral;
    let end_z = red.holder.zusd + liq.keeper_zusd + wd.principal_out;
    let end_debt = red.book.vaults[1].debt;
    let start = (start_agrs + start_z) as i128 - start_debt as i128;
    let end = (end_agrs + end_z) as i128 - end_debt as i128;
    let benefits = (liq.keeper_collateral + liq.keeper_zusd + g.settlement.collateral) as i128;
    let sp_loss = (1000 * Q - (wd.principal_out)) as i128;
    println!(
        "W4 attacker net change={} ; event-bound benefits(keeper+SP gain)={} ; SP principal loss={} ; fee paid={} ; residual={}",
        end - start,
        benefits,
        sp_loss,
        red.trace.fee,
        (end - start) - benefits + sp_loss + red.trace.fee as i128
    );
    // Global: supply = debt. Supply change = -offset (burn) - redemption X - reserves.
    let d_change = b0.system.debt as i128 - red.book.system.debt as i128;
    println!(
        "W4 debt fall={} = offset {} + X {} + closed reserves {}",
        d_change, liq.burned, red.trace.executed, red.trace.reserve_burn
    );
    let c_change = b0.system.collateral as i128 - red.book.system.collateral as i128;
    println!(
        "W4 system coll fall={} = SP {} + keeper {} + surplus {} - gain-to-vault {} + G {} + redemption surplus {}",
        c_change,
        liq.partitions[0].sp_collateral,
        liq.keeper_collateral,
        liq.partitions[0].surplus,
        g.settlement.collateral,
        red.trace.gross,
        red.trace.surplus
    );
    assert_eq!(liq.burned, 1000 * Q);
    assert_eq!(g.settlement.collateral, 522375 * Q / 1000);
    assert_eq!(
        (red.trace.executed, red.trace.gross, red.trace.fee),
        (400 * Q, 400 * Q, 42 * Q)
    );
    assert_eq!(wd.principal_out, 500 * Q - 1000);
    assert_eq!(
        (end - start) - benefits + sp_loss + red.trace.fee as i128,
        0
    );
    assert_eq!(end - start, (35625 * Q / 1000 - 1000) as i128);
    assert_eq!(d_change, 1400 * Q as i128);
    assert_eq!(c_change, (927625 * Q / 1000) as i128);
}

#[test]
fn w5_s_overflow_at_low_agrs_price() {
    let pool = deposit(&genesis_pool(), 3, 1000 * Q, None, 0).unwrap().book;
    let p = v::Prices {
        committed: 300_000,
        ..prices()
    }; // $0.003 per AGRS
    // Victim ICR 105%: c = 1.05 * 1000 / 0.003 AGRS; two healthy vaults keep Normal mode.
    let vs = vec![
        vault(1, 350_000 * Q, 1000 * Q),
        vault(2, 3_000_000 * Q, 1000 * Q),
        vault(3, 3_000_000 * Q, 1000 * Q),
    ];
    let b = book(vs, vec![id(2), id(3), id(1)], pool.pool.clone());
    let res = l::liquidate(
        &b,
        &l::Command::Direct(id(1)),
        p,
        Some(0),
        3,
        bounds(),
        limits(),
    );
    assert_eq!(res.result().unwrap().burned, 1000 * Q);
    assert!(res.result().unwrap().book.pool.history[0].s.limbs().0 > 0);
    let p2 = v::Prices {
        committed: 400_000,
        ..prices()
    };
    let vs = vec![
        vault(1, 262_500 * Q, 1000 * Q),
        vault(2, 3_000_000 * Q, 1000 * Q),
        vault(3, 3_000_000 * Q, 1000 * Q),
    ];
    let b = book(vs, vec![id(2), id(3), id(1)], pool.pool.clone());
    let res = l::liquidate(
        &b,
        &l::Command::Direct(id(1)),
        p2,
        Some(0),
        3,
        bounds(),
        limits(),
    );
    assert_eq!(res.result().unwrap().burned, 1000 * Q);
}

#[test]
fn liquidation_below_point_zero_zero_three_remains_defined() {
    let pool = deposit(&genesis_pool(), 3, 1000 * Q, None, 0).unwrap().book;
    let b = book(
        vec![
            vault(1, 1_050_000 * Q, 1000 * Q),
            vault(2, 3_000_000 * Q, 1000 * Q),
            vault(3, 3_000_000 * Q, 1000 * Q),
        ],
        vec![id(2), id(3), id(1)],
        pool.pool,
    );
    let p = v::Prices {
        committed: 100_000,
        ..prices()
    };
    assert_eq!(
        l::liquidate(
            &b,
            &l::Command::Direct(id(1)),
            p,
            Some(0),
            3,
            bounds(),
            limits()
        )
        .result()
        .unwrap()
        .burned,
        1000 * Q
    );
}

#[test]
fn maximum_u128_amounts_use_wide_per_unit_and_checked_final_payouts() {
    let one = st::change(
        &empty_staking(),
        id(7),
        st::Command::Stake(1),
        1,
        4,
        limits(),
    )
    .into_parts()
    .0
    .unwrap()
    .state;
    let s = st::fee_event(&one, u128::MAX, u128::MAX, 4, limits())
        .into_parts()
        .0
        .unwrap();
    let c = st::change(&s, id(7), st::Command::Claim, 0, 4, limits())
        .into_parts()
        .0
        .unwrap();
    assert_eq!((c.zusd_gain, c.agrs_gain), (u128::MAX, u128::MAX));
    let mut pool = deposit(&genesis_pool(), 1, 1, None, 0).unwrap().book;
    pool.pool = sp::offset(&pool.pool, 1, u128::MAX, Some(0), bounds(), limits())
        .into_parts()
        .0
        .unwrap();
    let out = sp::change(
        sp::ChangeInputs {
            pre: &pool,
            actor: id(1),
            command: sp::Command::Claim,
            wallet_zusd: 0,
            issuance: Some(0),
            prices: prices(),
            inventory: &[],
            system: system0(),
            bounds: bounds(),
        },
        limits(),
    )
    .into_parts()
    .0
    .unwrap();
    assert_eq!(
        (out.gains.collateral, out.book.pool.collateral),
        (u128::MAX, 0)
    );
    // Source uint256 exhaustion is still checked, even with no pending gain.
    let mut pre = one;
    pre.f_z = Accumulator256::MAX;
    pre.positions[0].snapshot_z = Accumulator256::MAX;
    let frozen = pre.clone();
    let a = st::fee_event(&pre, 1, 0, 4, limits());
    let b = st::fee_event(&pre, 1, 0, 4, limits());
    assert_eq!(a.result(), Err(Failure::Arithmetic));
    assert_eq!(a.usage(), b.usage());
    assert_eq!(pre, frozen);
}
#[test]
fn redistribution_l_and_snapshots_keep_256_bits() {
    // 2^128/Q is already a counterexample to U128 L; represent both assets.
    let l = Accumulator256::from_limbs(Q - 1, u128::MAX - Q + 1); // M * Q exactly
    let pre = v::Vault {
        stake: 1,
        collateral: 0,
        debt: 0,
        ..vault(1, 1, 1)
    };
    let s = v::System {
        reward_c: l,
        reward_d: l,
        ..system0()
    };
    let a = v::apply_pending(pre, s, limits()).into_parts().0.unwrap();
    assert_eq!((a.collateral, a.debt), (u128::MAX, u128::MAX));
    assert_eq!((a.snapshot_c, a.snapshot_d), (l, l));
    assert!(
        zenofcis_zusd_v1::laws::check_pending(pre, s, Ok(a), limits())
            .result()
            .is_ok()
    );
}
