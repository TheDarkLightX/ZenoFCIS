//! Native oracle arithmetic, accounting and price regressions.
use zeno_fcis_synthesis::finite::v2_oracle::CollectionLimits;
use zeno_fcis_synthesis::finite::v2_oracle::*;

fn id(n: u8) -> Id {
    Id::new(&[n]).unwrap_or_else(|_| panic!("fixture ID"))
}
fn limits() -> Limits {
    zero_limits()
        .with_limit(Resource::Step, 1_000_000)
        .with_limit(Resource::Read, 1_000_000)
        .with_limit(Resource::Byte, 10_000_000)
        .with_limit(Resource::Write, 1_000_000)
}
fn setup() -> (FeedRules, Parameters, SecurityInputs, Vec<Source>) {
    let clock = ClockParameters {
        backend: [5; 32],
        heartbeat: 1,
        confirmation: 1,
        dispute_open: 2,
        inclusion: 0,
        max_age: 4,
        max_epoch_seconds: 1,
        max_time_lag: 2,
    };
    let rules = FeedRules {
        feed: [1; 32],
        profile: [2; 32],
        query: [3; 32],
        clock: clock.clone(),
        max_deviation_bps: 10_000,
        diversity: Diversity {
            minimum: [3; 5],
            max_same_class: [1; 5],
        },
        bounds: CollectionLimits {
            max_entries: 16,
            max_item_bytes: 8192,
            max_snapshot_bytes: 131_101,
        },
        frame_bytes: 200_000,
    };
    let parameters = Parameters {
        clock: Some(clock),
        disputes: Some(DisputeParameters {
            adjudicator: [6; 32],
            minimum_bond: ZdexE18(10),
            challenge_bond: ZdexE18(1),
            slash: ZdexE18(5),
            exit_delay: 1,
            finality_delay: 1,
            appeal_delay: 1,
            false_price_slashable: true,
            treasury: id(88),
            slash_recipient: EscrowRecipient::Challenger,
            upheld_bond_recipient: EscrowRecipient::Challenger,
            rejected_bond_recipient: EscrowRecipient::ReporterOwner,
        }),
        economy: Some(EconomicParameters {
            reward_unit: RewardUnit {
                asset: id(99),
                scale: 18,
            },
            reward_per_report: 1,
            alpha_numerator: 1,
            alpha_denominator: 2,
            stress_source: [7; 32],
            stress_haircut_numerator: 1,
            stress_haircut_denominator: 1,
            debt_ceiling: UsdExposureE18(1),
            liquidation_cap: UsdExposureE18(1),
            redemption_cap: UsdExposureE18(1),
            other_cap: UsdExposureE18(1),
            window: 4,
        }),
    };
    let security = SecurityInputs {
        selected_bonds: [ZdexE18(10); 3],
        stress: Some(StressE8(100_000_000)),
        stress_source: [7; 32],
        valuation_root: [8; 32],
    };
    let sources = (1..=4)
        .map(|n| Source {
            id: id(n),
            operator: id(n),
            venue: id(n),
            family: id(n),
            transport: id(n),
            jurisdiction: id(n),
        })
        .collect();
    (rules, parameters, security, sources)
}
fn reports(round: u64, epoch: u64, seconds: u64, price: u128) -> Vec<Observation> {
    (1..=4)
        .map(|n| Observation {
            id: [n; 32],
            admission: [n + 10; 32],
            reporter: id(n),
            source: id(n),
            feed: [1; 32],
            profile: [2; 32],
            query: [3; 32],
            round,
            value: PriceE8::new(price).unwrap_or_else(|_| panic!("price")),
            epoch,
            seconds,
        })
        .collect()
}
fn candidate(
    r: &FeedRules,
    sources: &[Source],
    round: u64,
    epoch: u64,
    time: u64,
    price: u128,
) -> Aggregate {
    aggregate(
        r,
        round,
        epoch,
        epoch,
        &reports(round, epoch, time, price),
        sources,
        limits(),
    )
    .into_parts()
    .0
    .unwrap_or_else(|e| panic!("aggregate refusal: {e:?}"))
    .unwrap_or_else(|e| panic!("aggregate reject: {e:?}"))
}
fn run(
    r: &FeedRules,
    p: &Parameters,
    s: &PriceState,
    c: PriceCommand,
    epoch: u64,
    security: &SecurityInputs,
) -> PriceState {
    price_relation(
        r,
        PriceContext {
            parameters: p,
            nonce: s.nonce,
            clock: ClockView {
                backend: r.clock.backend,
                finality_root: [9; 32],
                epoch,
            },
            security,
        },
        s,
        &c,
        limits(),
    )
    .into_parts()
    .0
    .unwrap_or_else(|e| panic!("price refusal: {e:?}"))
    .unwrap_or_else(|e| panic!("price reject: {e:?}"))
    .into_state()
}

#[test]
fn oracle_selection_is_public_value_independent_and_never_searches_a_better_triple() {
    let (mut rules, _, _, sources) = setup();
    let mut os = reports(0, 0, 0, 100);
    os[0].value = PriceE8::new(1).unwrap_or_else(|_| panic!("price"));
    rules.max_deviation_bps = 1;
    // The triple 2/3/4 agrees, but the selected 1/2/3 does not. No fallback.
    assert_eq!(
        aggregate(&rules, 0, 0, 0, &os, &sources, limits()).result(),
        Ok(&Err(Reason::ExcessUncertainty))
    );
    rules.max_deviation_bps = 10_000;
    for _ in 0..4 {
        let out = aggregate(&rules, 0, 0, 0, &os, &sources, limits());
        let a = out
            .result()
            .unwrap_or_else(|_| panic!("refused"))
            .as_ref()
            .unwrap_or_else(|_| panic!("rejected"));
        assert_eq!(a.reporters(), &[id(1), id(2), id(3)]);
        os.rotate_left(1);
    }
}

#[test]
fn the_oracle_never_blocks_a_genuine_price_fall() {
    let (r, p, mut security, sources) = setup();
    let initial = candidate(&r, &sources, 0, 0, 0, 200);
    let s = run(
        &r,
        &p,
        &PriceState::default(),
        PriceCommand::Stage((initial).into()),
        0,
        &security,
    );
    let s = run(&r, &p, &s, PriceCommand::Commit, 2, &security);
    let fall = candidate(&r, &sources, 1, 2, 2, 1);
    let s = run(&r, &p, &s, PriceCommand::Stage((fall).into()), 2, &security);
    security.selected_bonds = [ZdexE18(0); 3];
    let out = price_relation(
        &r,
        PriceContext {
            parameters: &p,
            nonce: s.nonce,
            clock: ClockView {
                backend: r.clock.backend,
                finality_root: [9; 32],
                epoch: 4,
            },
            security: &security,
        },
        &s,
        &PriceCommand::Commit,
        limits(),
    );
    let plan = out
        .result()
        .unwrap_or_else(|_| panic!("fall refused"))
        .as_ref()
        .unwrap_or_else(|_| panic!("fall blocked"));
    assert_eq!(
        plan.state()
            .committed
            .as_ref()
            .map(|a| a.numbers().median.raw()),
        Some(1)
    );
    assert_eq!(
        plan.capacity_status(),
        CapacityStatus::OverCap(UsdExposureE18(0))
    );
    assert_eq!(
        s.committed.as_ref().map(|a| a.numbers().median.raw()),
        Some(200)
    );
}

#[test]
fn oracle_bootstrap_upward_price_buffer_and_observation_age_are_exact() {
    let (r, p, security, sources) = setup();
    let a = candidate(&r, &sources, 0, 0, 0, 10);
    let s = run(
        &r,
        &p,
        &PriceState::default(),
        PriceCommand::Stage((a).into()),
        0,
        &security,
    );
    let same = price_relation(
        &r,
        PriceContext {
            parameters: &p,
            nonce: s.nonce,
            clock: ClockView {
                backend: r.clock.backend,
                finality_root: [9; 32],
                epoch: 0,
            },
            security: &security,
        },
        &s,
        &PriceCommand::Commit,
        limits(),
    );
    assert_eq!(
        same.result().map(|r| r.as_ref().map(|_| ())),
        Ok(Err(&Reason::BufferNotMature))
    );
    let s = run(&r, &p, &s, PriceCommand::Commit, 2, &security);
    let up = candidate(&r, &sources, 1, 2, 2, 100);
    let s = run(&r, &p, &s, PriceCommand::Stage((up).into()), 2, &security);
    let s = run(&r, &p, &s, PriceCommand::Commit, 4, &security);
    assert_eq!(
        s.committed.as_ref().map(|a| a.numbers().median.raw()),
        Some(100)
    );
    assert_eq!(
        s.committed.as_ref().map(|a| a.numbers().oldest_epoch),
        Some(2)
    );
    assert_eq!(s.history.len(), 2);
}

#[test]
fn oracle_uncertain_time_quorum_freezes_old_evidence_before_price_commit() {
    let (mut r, p, security, sources) = setup();
    let s = run(
        &r,
        &p,
        &PriceState::default(),
        PriceCommand::Stage((candidate(&r, &sources, 0, 0, 0, 10)).into()),
        0,
        &security,
    );
    let s = run(&r, &p, &s, PriceCommand::Commit, 2, &security);
    let mut os = reports(1, 2, 100, 10);
    os[0].value = PriceE8::new(1).unwrap_or_else(|_| panic!("price"));
    r.max_deviation_bps = 1;
    assert_eq!(
        aggregate(&r, 1, 2, 2, &os, &sources, limits()).result(),
        Ok(&Err(Reason::ExcessUncertainty))
    );
    let quorum = time_quorum(&r, 1, 2, 2, &os, &sources, limits())
        .into_parts()
        .0
        .unwrap_or_else(|_| panic!("time refused"))
        .unwrap_or_else(|_| panic!("time rejected"));
    let s = run(&r, &p, &s, PriceCommand::SyncTime(quorum), 2, &security);
    assert_eq!(s.watermark, 100);
    assert_eq!(
        s.committed.as_ref().map(|a| a.numbers().median_seconds),
        Some(0)
    );
    let read = committed_view(
        &r,
        &p,
        &s,
        ClockView {
            backend: r.clock.backend,
            finality_root: [9; 32],
            epoch: 2,
        },
        &security,
        limits(),
    );
    assert_eq!(read.result(), Ok(&Err(Reason::StaleObservation)));
}

#[test]
fn oracle_complete_price_laws_reject_all_wrong_reason_wrong_state_and_false_capacity() {
    let (r, p, security, sources) = setup();
    let s = PriceState::default();
    let c = PriceCommand::Stage((candidate(&r, &sources, 0, 0, 0, 100)).into());
    let context = ClockView {
        backend: r.clock.backend,
        finality_root: [9; 32],
        epoch: 0,
    };
    let native = price_relation(
        &r,
        PriceContext {
            parameters: &p,
            nonce: 0,
            clock: context,
            security: &security,
        },
        &s,
        &c,
        limits(),
    );
    let plan = native
        .result()
        .unwrap_or_else(|_| panic!("refused"))
        .as_ref()
        .unwrap_or_else(|_| panic!("rejected"));
    assert!(
        check_price_laws(
            &r,
            PriceContext {
                parameters: &p,
                nonce: 0,
                clock: context,
                security: &security
            },
            &s,
            &c,
            Ok((plan.state(), plan.capacity_status())),
            limits()
        )
        .result()
        .is_ok()
    );
    for reason in REASONS {
        let check = check_price_laws(
            &r,
            PriceContext {
                parameters: &p,
                nonce: 0,
                clock: context,
                security: &security,
            },
            &s,
            &c,
            Err(reason),
            limits(),
        );
        assert!(
            matches!(check.result(), Err(Failure::Law(_))),
            "reject-all reason {reason:?} survived"
        );
    }
    let mut wrong = plan.state().clone();
    wrong.watermark = 1;
    assert!(matches!(
        check_price_laws(
            &r,
            PriceContext {
                parameters: &p,
                nonce: 0,
                clock: context,
                security: &security
            },
            &s,
            &c,
            Ok((&wrong, plan.capacity_status())),
            limits()
        )
        .result(),
        Err(Failure::Law(112_002))
    ));
    assert!(matches!(
        check_price_laws(
            &r,
            PriceContext {
                parameters: &p,
                nonce: 0,
                clock: context,
                security: &security
            },
            &s,
            &c,
            Ok((plan.state(), CapacityStatus::Unavailable)),
            limits()
        )
        .result(),
        Err(Failure::Law(113_003))
    ));
    assert!(matches!(
        check_price_laws(
            &r,
            PriceContext {
                parameters: &p,
                nonce: 0,
                clock: context,
                security: &security
            },
            &s,
            &c,
            Ok((plan.state(), CapacityStatus::Available(UsdExposureE18(999)))),
            limits()
        )
        .result(),
        Err(Failure::Law(_))
    ));
    let committed = run(&r, &p, plan.state(), PriceCommand::Commit, 2, &security);
    let commit = PriceCommand::Commit;
    assert!(matches!(
        check_price_laws(
            &r,
            PriceContext {
                parameters: &p,
                nonce: committed.nonce,
                clock: ClockView {
                    epoch: 2,
                    ..context
                },
                security: &security
            },
            &committed,
            &commit,
            Err(Reason::Replay),
            limits()
        )
        .result(),
        Err(Failure::Law(_))
    ));
}

#[test]
fn oracle_price_laws_value_full_width_coalitions_without_narrowing() {
    let (r, mut p, mut security, sources) = setup();
    let s = PriceState::default();
    let c = PriceCommand::Stage((candidate(&r, &sources, 0, 0, 0, 100)).into());
    let context = ClockView {
        backend: r.clock.backend,
        finality_root: [9; 32],
        epoch: 0,
    };
    let half = 1u128 << 127;
    let law = |p: &Parameters, security: &SecurityInputs, status| {
        let plan = price_relation(
            &r,
            PriceContext {
                parameters: p,
                nonce: 0,
                clock: context,
                security,
            },
            &s,
            &c,
            limits(),
        )
        .into_parts()
        .0
        .unwrap_or_else(|e| panic!("refused {e:?}"))
        .unwrap_or_else(|e| panic!("rejected {e:?}"));
        (
            plan.capacity_status(),
            check_price_laws(
                &r,
                PriceContext {
                    parameters: p,
                    nonce: 0,
                    clock: context,
                    security,
                },
                &s,
                &c,
                Ok((plan.state(), status)),
                limits(),
            )
            .into_parts()
            .0,
        )
    };
    // The cheapest pair sums to exactly 2^128, one past U128.
    security.selected_bonds = [ZdexE18(half), ZdexE18(u128::MAX), ZdexE18(half)];
    let exact = CapacityStatus::Available(UsdExposureE18(half));
    assert_eq!(law(&p, &security, exact), (exact, Ok(())));
    for (status, code) in [
        (CapacityStatus::Available(UsdExposureE18(half - 1)), 113_005),
        (CapacityStatus::Available(UsdExposureE18(half + 1)), 113_004),
        (CapacityStatus::OverCap(UsdExposureE18(half)), 113_006),
        (CapacityStatus::Unavailable, 113_003),
    ] {
        assert_eq!(law(&p, &security, status).1, Err(Failure::Law(code)));
    }
    // 2*(2^128-1)*(M-1)/M exceeds U128, so the quotient is unavailable.
    security.selected_bonds = [ZdexE18(u128::MAX); 3];
    let e = p.economy.as_mut().unwrap_or_else(|| panic!("economy"));
    (e.alpha_numerator, e.alpha_denominator) = (u128::MAX - 1, u128::MAX);
    let unavailable = CapacityStatus::Unavailable;
    assert_eq!(law(&p, &security, unavailable), (unavailable, Ok(())));
    assert_eq!(
        law(
            &p,
            &security,
            CapacityStatus::Available(UsdExposureE18(u128::MAX))
        )
        .1,
        Err(Failure::Law(113_002))
    );
}

#[test]
fn oracle_price_failure_and_retry_preserve_inputs_and_exact_charged_prefix() {
    let (r, p, security, sources) = setup();
    let s = PriceState::default();
    let old = s.clone();
    let c = PriceCommand::Stage((candidate(&r, &sources, 0, 0, 0, 100)).into());
    let clock = ClockView {
        backend: r.clock.backend,
        finality_root: [9; 32],
        epoch: 0,
    };
    let full = price_relation(
        &r,
        PriceContext {
            parameters: &p,
            nonce: 0,
            clock,
            security: &security,
        },
        &s,
        &c,
        limits(),
    );
    assert!(matches!(full.result(), Ok(Ok(_))));
    for resource in [Resource::Step, Resource::Read, Resource::Byte] {
        let usage = full.usage().used(resource);
        if usage == 0 {
            continue;
        }
        let bound = limits().with_limit(resource, usage - 1);
        let first = price_relation(
            &r,
            PriceContext {
                parameters: &p,
                nonce: 0,
                clock,
                security: &security,
            },
            &s,
            &c,
            bound,
        );
        let retry = price_relation(
            &r,
            PriceContext {
                parameters: &p,
                nonce: 0,
                clock,
                security: &security,
            },
            &s,
            &c,
            bound,
        );
        assert!(matches!(first.result(), Err(Failure::Budget(_))));
        assert_eq!(first.usage(), retry.usage());
        assert_eq!(first.result(), retry.result());
        assert_eq!(s, old);
    }
}
