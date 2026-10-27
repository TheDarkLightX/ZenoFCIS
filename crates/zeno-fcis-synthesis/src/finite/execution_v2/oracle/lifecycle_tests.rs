use super::CollectionLimits;
use super::test_verifier::{self as fixture, TestVerifier};
use super::*;
use alloc::{collections::BTreeMap, vec::Vec};
use zeno_fcis_codec::CommitmentHasher;

fn id(n: u8) -> Id {
    Id::new(&[n]).unwrap_or_else(|_| panic!("ID"))
}
fn limits() -> Limits {
    [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ]
    .into_iter()
    .fold(zero_limits(), |l, r| l.with_limit(r, 100_000_000))
}
pub(super) fn profile() -> LifecycleProfile {
    LifecycleProfile {
        zdex_asset: id(10),
        reward_split: RewardSplit {
            reporter: 1,
            treasury: 0,
            burn: 0,
            denominator: 1,
            treasury_account: id(8),
        },
        parameters: Parameters {
            clock: Some(ClockParameters {
                backend: [5; 32],
                heartbeat: 1,
                confirmation: 1,
                dispute_open: 2,
                inclusion: 0,
                max_age: 4,
                max_epoch_seconds: 1,
                max_time_lag: 2,
            }),
            disputes: Some(DisputeParameters {
                adjudicator: [6; 32],
                minimum_bond: ZdexE18(1),
                challenge_bond: ZdexE18(1),
                slash: ZdexE18(1),
                exit_delay: 1,
                finality_delay: 0,
                appeal_delay: 0,
                false_price_slashable: true,
                treasury: id(8),
                slash_recipient: EscrowRecipient::Challenger,
                upheld_bond_recipient: EscrowRecipient::Challenger,
                rejected_bond_recipient: EscrowRecipient::ReporterOwner,
            }),
            economy: Some(EconomicParameters {
                reward_unit: RewardUnit {
                    asset: id(9),
                    scale: 18,
                },
                reward_per_report: 1,
                alpha_numerator: 1,
                alpha_denominator: 2,
                stress_source: [7; 32],
                stress_haircut_numerator: 1,
                stress_haircut_denominator: 1,
                debt_ceiling: UsdExposureE18(0),
                liquidation_cap: UsdExposureE18(0),
                redemption_cap: UsdExposureE18(0),
                other_cap: UsdExposureE18(0),
                window: 4,
            }),
        },
        bounds: CollectionLimits {
            max_entries: 64,
            max_item_bytes: 8192,
            max_snapshot_bytes: 1_000_000,
        },
        frame_bytes: 2_000_000,
    }
}
fn initial() -> LifecycleState {
    LifecycleState::new(
        id(99),
        profile(),
        BTreeMap::from([
            (id(1), ZdexE18(u128::MAX)),
            (id(2), ZdexE18(10)),
            (id(3), ZdexE18(10)),
        ]),
    )
    .unwrap_or_else(|e| panic!("{e:?}"))
}
fn context(s: &LifecycleState, actor: u8, epoch: u64) -> LifecycleContext {
    LifecycleContext {
        pre_root: s.root().unwrap_or_else(|e| panic!("{e:?}")),
        nonce: s.nonce(&id(actor)),
        clock: ClockView {
            backend: [5; 32],
            finality_root: [9; 32],
            epoch,
        },
    }
}
fn apply(s: &LifecycleState, actor: u8, epoch: u64, c: &LifecycleCommand) -> LifecycleState {
    let ctx = context(s, actor, epoch);
    let typed = lifecycle_relation(&TestVerifier, s, &id(actor), ctx, c, limits())
        .into_parts()
        .0
        .unwrap_or_else(|e| panic!("typed {c:?}: {e:?}"))
        .unwrap_or_else(|e| panic!("typed {c:?}: {e:?}"));
    let bytes = lifecycle_command_bytes(s, &id(actor), ctx, c)
        .unwrap_or_else(|e| panic!("encode {c:?}: {e:?}"));
    let decoded = lifecycle_relation_bytes(&TestVerifier, s, &bytes, limits())
        .into_parts()
        .0
        .unwrap_or_else(|e| panic!("bytes {c:?}: {e:?}"))
        .unwrap_or_else(|e| panic!("bytes {c:?}: {e:?}"));
    assert_eq!(typed, decoded);
    typed.into_state()
}
fn key(reporter: u8, version: u64, sk: u64) -> KeyRegistration {
    let (public_key, possession) = fixture::key(sk);
    KeyRegistration {
        reporter: id(reporter),
        owner: id(reporter),
        version,
        public_key,
        possession,
    }
}
fn register(s: &LifecycleState, reporter: u8, sk: u64) -> LifecycleState {
    let k = key(reporter, 0, sk);
    let ctx = context(s, reporter, 0);
    let message =
        registration_message(s, ctx.pre_root, ctx.nonce, &k).unwrap_or_else(|e| panic!("{e:?}"));
    apply(
        s,
        reporter,
        0,
        &LifecycleCommand::RegisterKey {
            key: k,
            authorization: fixture::sign(sk, &message),
        },
    )
}
fn feed(s: &LifecycleState) -> FeedRegistration {
    FeedRegistration {
        version: 1,
        base: id(80),
        quote: id(81),
        rules: FeedRules {
            feed: [0; 32],
            profile: s.profile_id(),
            query: [3; 32],
            diversity: Diversity {
                minimum: [3; 5],
                max_same_class: [1; 5],
            },
            max_deviation_bps: 10_000,
            clock: s
                .profile()
                .parameters
                .clock
                .clone()
                .unwrap_or_else(|| panic!("clock")),
            bounds: s.profile().bounds,
            frame_bytes: s.profile().frame_bytes,
        },
        sources: (1..=3)
            .map(|i| Source {
                id: id(i),
                operator: id(i),
                venue: id(i),
                family: id(i),
                transport: id(i),
                jurisdiction: id(i),
            })
            .collect(),
        assignments: (1..=3).map(|i| (id(i), id(i))).collect(),
        first_open: 0,
        cutoff_delta: 1,
    }
    .with_bound_id()
    .unwrap_or_else(|e| panic!("{e:?}"))
}
fn report(
    s: &LifecycleState,
    reporter: u8,
    sk: u64,
    feed: Hash,
    round: u64,
    value: u128,
    epoch: u64,
) -> RawReport {
    let r = s
        .reporter(&id(reporter))
        .unwrap_or_else(|| panic!("reporter"));
    let f = s.feed(&feed).unwrap_or_else(|| panic!("feed"));
    let payload = ReportPayload {
        chain: s.chain().clone(),
        reporter: id(reporter),
        key_version: r.active_version,
        feed,
        profile: s.profile_id(),
        query: f.rules.query,
        source: id(reporter),
        round,
        value: PriceE8::new(value).unwrap_or_else(|_| panic!("price")),
        observed_epoch: epoch,
        observation_seconds: epoch,
        sequence: r.chain.next_sequence,
        previous: r.chain.previous,
    };
    let signature = fixture::sign(sk, &payload.message());
    let payload_hash =
        *zeno_fcis_crypto::RustCryptoSha256::hash(&payload.canonical_json()).as_bytes();
    RawReport {
        id: payload.report_id(&signature),
        payload,
        payload_hash,
        signature,
    }
}

#[test]
fn oracle_native_lifecycle_binds_actual_keys_complete_rounds_reservations_and_rotation() {
    let mut s = initial();
    let old = s.clone();
    for i in 1..=3 {
        s = register(&s, i, u64::from(i));
        s = apply(
            &s,
            i,
            0,
            &LifecycleCommand::Bond {
                reporter: id(i),
                command: BondCommand::Deposit(ZdexE18(5)),
            },
        );
    }
    let f = feed(&s);
    let feed = f.rules.feed;
    s = apply(&s, 7, 0, &LifecycleCommand::RegisterFeed(f));
    s = apply(&s, 7, 0, &LifecycleCommand::OpenRound { feed, round: 0 });
    let before = s.clone();
    let mut ids = Vec::new();
    for i in 1..=3 {
        let r = report(&s, i, u64::from(i), feed, 0, 100 + u128::from(i), 0);
        ids.push(r.id);
        s = apply(&s, 7, 0, &LifecycleCommand::SubmitReport(r));
    }
    assert_eq!(
        before
            .reporter(&id(1))
            .unwrap_or_else(|| panic!("row"))
            .chain
            .next_sequence,
        0
    );
    assert_eq!(
        s.reporter(&id(1))
            .unwrap_or_else(|| panic!("row"))
            .chain
            .next_sequence,
        1
    );
    assert_eq!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("bond"))
            .liabilities
            .len(),
        1
    );
    s = apply(
        &s,
        7,
        1,
        &LifecycleCommand::BuildAggregate { feed, round: 0 },
    );
    let agg = s
        .round(feed, 0)
        .and_then(|r| r.aggregate)
        .unwrap_or_else(|| panic!("aggregate"));
    assert_eq!(
        s.aggregate(&agg)
            .unwrap_or_else(|| panic!("aggregate"))
            .numbers()
            .median
            .raw(),
        102
    );
    assert_eq!(
        s.aggregate(&agg)
            .unwrap_or_else(|| panic!("aggregate"))
            .reports(),
        ids.as_slice()
    );
    let k = key(1, 1, 5);
    let ctx = context(&s, 1, 1);
    let msg =
        rotation_message(&s, ctx.pre_root, ctx.nonce, 0, &k).unwrap_or_else(|e| panic!("{e:?}"));
    s = apply(
        &s,
        1,
        1,
        &LifecycleCommand::RotateKey {
            key: k,
            authorization: fixture::sign(5, &msg),
            old_authorization: fixture::sign(1, &msg),
        },
    );
    assert_eq!(
        s.key(&id(1), 0).unwrap_or_else(|| panic!("old")).retired,
        Some(1)
    );
    assert_eq!(
        s.reporter(&id(1))
            .unwrap_or_else(|| panic!("row"))
            .chain
            .next_sequence,
        1
    );
    assert_eq!(
        s.reporter(&id(1))
            .unwrap_or_else(|| panic!("row"))
            .chain
            .previous,
        Some(ids[0])
    );
    s = apply(&s, 7, 1, &LifecycleCommand::OpenRound { feed, round: 1 });
    let r = report(&s, 1, 5, feed, 1, 1, 1);
    s = apply(&s, 7, 1, &LifecycleCommand::SubmitReport(r));
    assert_eq!(
        s.reporter(&id(1))
            .unwrap_or_else(|| panic!("row"))
            .chain
            .next_sequence,
        2
    );
    assert_eq!(old.wallet(&id(1)), ZdexE18(u128::MAX));
    assert!(old.reporter(&id(1)).is_none());
}

#[test]
fn oracle_native_flat_schema_preserves_high_bits_options_old_snapshots_and_refusal_usage() {
    let s = register(&initial(), 1, 1);
    let bytes = s.canonical_bytes().unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        check_lifecycle_snapshot_bytes(&s, &bytes, limits()).result(),
        Ok(&())
    );
    let next = apply(
        &s,
        1,
        0,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Deposit(ZdexE18(1u128 << 127)),
        },
    );
    assert_eq!(
        next.bond(&id(1)).unwrap_or_else(|| panic!("bond")).held,
        ZdexE18(1u128 << 127)
    );
    let next_bytes = next.canonical_bytes().unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        check_lifecycle_snapshot_bytes(&next, &next_bytes, limits()).result(),
        Ok(&())
    );
    assert_eq!(
        s.canonical_bytes().unwrap_or_else(|e| panic!("{e:?}")),
        bytes
    );
    let mut bad = next_bytes.clone();
    bad.push(0);
    assert!(
        check_lifecycle_snapshot_bytes(&next, &bad, limits())
            .result()
            .is_err()
    );
    assert!(
        check_lifecycle_snapshot_bytes(&next, &bytes, limits())
            .result()
            .is_err()
    );
    let limited = limits().with_limit(Resource::Byte, 1);
    let a = check_lifecycle_snapshot_bytes(&next, &next_bytes, limited);
    let b = check_lifecycle_snapshot_bytes(&next, &next_bytes, limited);
    assert!(matches!(a.result(), Err(Failure::Budget(_))));
    assert_eq!(a.usage(), b.usage());
    assert_eq!(a.result(), b.result());
}

#[test]
fn oracle_native_lifecycle_complete_laws_refuse_reject_all_extra_writes_and_wrong_reasons() {
    let mut s = initial();
    let f = feed(&s);
    let feed = f.rules.feed;
    s = apply(&s, 7, 0, &LifecycleCommand::RegisterFeed(f));
    let c = LifecycleCommand::OpenRound { feed, round: 0 };
    let ctx = context(&s, 7, 0);
    for reason in REASONS {
        assert!(matches!(
            check_lifecycle_laws(&TestVerifier, &s, &id(7), ctx, &c, Err(reason), limits())
                .result(),
            Err(Failure::Law(_))
        ));
    }
    let good = apply(&s, 7, 0, &c);
    let mut bad = good.clone();
    bad.wallets.insert(id(8), ZdexE18(99));
    assert_eq!(
        check_lifecycle_laws(&TestVerifier, &s, &id(7), ctx, &c, Ok(&good), limits()).result(),
        Ok(&())
    );
    assert!(matches!(
        check_lifecycle_laws(&TestVerifier, &s, &id(7), ctx, &c, Ok(&bad), limits()).result(),
        Err(Failure::Law(_))
    ));
}

fn complete_round(
    mut s: LifecycleState,
    feed: Hash,
    round: u64,
    value: u128,
    now: u64,
) -> LifecycleState {
    s = apply(&s, 7, now, &LifecycleCommand::OpenRound { feed, round });
    for i in 1..=3 {
        let r = report(&s, i, u64::from(i), feed, round, value, now);
        s = apply(&s, 7, now, &LifecycleCommand::SubmitReport(r));
    }
    s
}
fn funded_feed() -> (LifecycleState, Hash) {
    let mut s = initial()
        .with_reward_wallets(BTreeMap::from([(id(2), 10)]))
        .unwrap_or_else(|e| panic!("{e:?}"));
    for i in 1..=3 {
        s = register(&s, i, u64::from(i));
        s = apply(
            &s,
            i,
            0,
            &LifecycleCommand::Bond {
                reporter: id(i),
                command: BondCommand::Deposit(ZdexE18(5)),
            },
        );
    }
    let f = feed(&s);
    let feed = f.rules.feed;
    s = apply(&s, 7, 0, &LifecycleCommand::RegisterFeed(f));
    (s, feed)
}
fn price(s: &LifecycleState, feed: Hash, now: u64, c: NativePriceCommand) -> LifecycleState {
    apply(s, 7, now, &LifecycleCommand::Price { feed, command: c })
}
fn build_and_stage(s: &LifecycleState, feed: Hash, round: u64, now: u64) -> LifecycleState {
    let s = apply(s, 7, now, &LifecycleCommand::BuildAggregate { feed, round });
    let aggregate = s
        .round(feed, round)
        .and_then(|r| r.aggregate)
        .unwrap_or_else(|| panic!("round"));
    price(&s, feed, now, NativePriceCommand::Stage { aggregate })
}
#[test]
fn oracle_native_publication_time_reward_exit_and_the_oracle_never_blocks_a_genuine_price_fall() {
    let (s, feed) = funded_feed();
    let s = complete_round(s, feed, 0, 100, 0);
    let s = price(&s, feed, 1, NativePriceCommand::SyncTime { round: 0 });
    assert_eq!(
        s.time_quorum(&feed)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .seconds,
        0
    );
    let s = build_and_stage(&s, feed, 0, 1);
    let a = s
        .price(&feed)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .pending
        .as_ref()
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .aggregate
        .id();
    assert!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .liabilities[0]
            .publication
            == Some(1)
    );
    let early = lifecycle_relation(
        &TestVerifier,
        &s,
        &id(7),
        context(&s, 7, 1),
        &LifecycleCommand::Price {
            feed,
            command: NativePriceCommand::Commit,
        },
        limits(),
    );
    assert!(matches!(early.result(), Ok(Err(Reason::BufferNotMature))));
    let s = complete_round(s, feed, 1, 1, 1);
    let s = price(&s, feed, 3, NativePriceCommand::Commit);
    let s = apply(
        &s,
        2,
        3,
        &LifecycleCommand::Reward {
            feed,
            command: NativeRewardCommand::Fund(3),
        },
    );
    let report = s
        .aggregate(&a)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .reports()[0];
    let s = apply(
        &s,
        7,
        3,
        &LifecycleCommand::Reward {
            feed,
            command: NativeRewardCommand::Accrue {
                aggregate: a,
                report,
            },
        },
    );
    let claim = s
        .reward(&feed)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .claims[0]
        .id;
    let locked = lifecycle_relation(
        &TestVerifier,
        &s,
        &id(1),
        context(&s, 1, 3),
        &LifecycleCommand::Reward {
            feed,
            command: NativeRewardCommand::Claim(claim),
        },
        limits(),
    );
    assert!(matches!(locked.result(), Ok(Err(Reason::LiabilityLocked))));
    let s = build_and_stage(&s, feed, 1, 3);
    let s = apply(
        &s,
        7,
        3,
        &LifecycleCommand::UpdateStress {
            feed,
            observation: StressObservation {
                source: [7; 32],
                valuation_root: [4; 32],
                value: Some(StressE8(0)),
                observed_epoch: 3,
            },
        },
    );
    let s = price(&s, feed, 5, NativePriceCommand::Commit);
    assert_eq!(
        s.price(&feed)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .committed
            .as_ref()
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .numbers()
            .median
            .raw(),
        1
    );
    assert!(
        s.price(&feed)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .pending
            .is_none()
    );
    let liability = s
        .report(&report)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .liability;
    let s = apply(
        &s,
        1,
        8,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Clear { liability },
        },
    );
    let s = apply(
        &s,
        1,
        8,
        &LifecycleCommand::Reward {
            feed,
            command: NativeRewardCommand::Claim(claim),
        },
    );
    assert_eq!(s.reward_wallet(&id(1)), 1);
    let s = apply(
        &s,
        1,
        8,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Unregister,
        },
    );
    let liability = s
        .bond(&id(1))
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .liabilities[1]
        .id;
    let s = apply(
        &s,
        1,
        9,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Clear { liability },
        },
    );
    let s = apply(
        &s,
        1,
        9,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Withdraw(ZdexE18(5)),
        },
    );
    assert_eq!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .phase,
        ReporterPhase::Retired
    );
    assert_eq!(
        check_lifecycle_snapshot_bytes(
            &s,
            &s.canonical_bytes()
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            limits()
        )
        .result(),
        Ok(&())
    );
}
#[test]
fn oracle_native_dispute_inconclusive_never_unlocks_and_rejected_resolution_restores_evidence() {
    let (s, feed) = funded_feed();
    let s = complete_round(s, feed, 0, 100, 0);
    let s = build_and_stage(&s, feed, 0, 1);
    let liability = s
        .bond(&id(1))
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .liabilities[0]
        .id;
    let dispute = [21; 32];
    let s = apply(
        &s,
        2,
        1,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::OpenDispute {
                id: dispute,
                liability,
            },
        },
    );
    let s = apply(
        &s,
        7,
        3,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Resolve {
                dispute,
                verdict: Verdict::Inconclusive,
            },
        },
    );
    let frozen = lifecycle_relation(
        &TestVerifier,
        &s,
        &id(7),
        context(&s, 7, 3),
        &LifecycleCommand::Price {
            feed,
            command: NativePriceCommand::Commit,
        },
        limits(),
    );
    assert!(matches!(frozen.result(), Ok(Err(Reason::DisputedOrFrozen))));
    let future = apply(
        &s,
        7,
        1000,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Resolve {
                dispute,
                verdict: Verdict::Inconclusive,
            },
        },
    );
    assert_eq!(
        future
            .bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .disputes[0]
            .phase,
        DisputePhase::Open
    );
    let locked = lifecycle_relation(
        &TestVerifier,
        &future,
        &id(1),
        context(&future, 1, 1000),
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Clear { liability },
        },
        limits(),
    );
    assert!(matches!(locked.result(), Ok(Err(Reason::LiabilityLocked))));
    let s = apply(
        &s,
        7,
        3,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Resolve {
                dispute,
                verdict: Verdict::Final(FinalVerdict {
                    adjudicator: [6; 32],
                    dispute,
                    violation: [22; 32],
                    upheld: false,
                    final_epoch: 3,
                    finality_root: [9; 32],
                }),
            },
        },
    );
    let s = price(&s, feed, 3, NativePriceCommand::Commit);
    assert!(
        s.price(&feed)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .committed
            .is_some()
    );
}

#[test]
fn oracle_native_held_claim_clawback_is_atomic_with_slash_and_resolution_retains_proofs() {
    let (s, feed) = funded_feed();
    let s = complete_round(s, feed, 0, 100, 0);
    let s = build_and_stage(&s, feed, 0, 1);
    let aggregate = s
        .price(&feed)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .pending
        .as_ref()
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .aggregate
        .id();
    let report = s
        .aggregate(&aggregate)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .reports()[0];
    let liability = s
        .report(&report)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .liability;
    let s = apply(
        &s,
        2,
        1,
        &LifecycleCommand::Reward {
            feed,
            command: NativeRewardCommand::Fund(3),
        },
    );
    let s = apply(
        &s,
        7,
        1,
        &LifecycleCommand::Reward {
            feed,
            command: NativeRewardCommand::Accrue { aggregate, report },
        },
    );
    let dispute = [23; 32];
    let s = apply(
        &s,
        2,
        1,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::OpenDispute {
                id: dispute,
                liability,
            },
        },
    );
    let proof = FinalVerdict {
        adjudicator: [6; 32],
        dispute,
        violation: [24; 32],
        upheld: true,
        final_epoch: 1,
        finality_root: [9; 32],
    };
    let s = apply(
        &s,
        7,
        1,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Slash {
                dispute,
                verdict: proof.clone(),
            },
        },
    );
    assert_eq!(
        s.reward(&feed)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .claims[0]
            .phase,
        ClaimPhase::ClawedBack
    );
    assert_eq!(
        s.reward(&feed)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .remaining,
        3
    );
    assert_eq!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .held,
        ZdexE18(4)
    );
    assert_eq!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .disputes[0]
            .phase,
        DisputePhase::Open
    );
    assert_eq!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .disputes[0]
            .slash_verdict,
        Some(proof.clone())
    );
    let old = s.clone();
    let s = apply(
        &s,
        7,
        2,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Resolve {
                dispute,
                verdict: Verdict::Final(FinalVerdict {
                    final_epoch: 2,
                    ..proof.clone()
                }),
            },
        },
    );
    assert_eq!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .disputes[0]
            .slash_verdict,
        Some(proof)
    );
    assert_eq!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .disputes[0]
            .resolution_verdict
            .as_ref()
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .final_epoch,
        2
    );
    assert_eq!(
        old.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .disputes[0]
            .phase,
        DisputePhase::Open
    );
    assert_eq!(
        check_lifecycle_snapshot_bytes(
            &s,
            &s.canonical_bytes()
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            limits()
        )
        .result(),
        Ok(&())
    );
}
#[test]
fn oracle_native_canonical_ingress_scope_unknowns_malformed_crypto_and_refusal_retry() {
    use zeno_fcis_value::{Value, ValueRef};
    let s = initial();
    let c = LifecycleCommand::RegisterFeed(feed(&s));
    let ctx = context(&s, 7, 0);
    let value = super::lifecycle_schema::command_value(&s, &id(7), ctx, &c)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let ValueRef::Record(fs) = value.view() else {
        panic!("record")
    };
    let fields: Vec<Value> = fs.iter().map(|f| f.value().clone()).collect();
    for (index, replacement, expected) in [
        (6, Value::unsigned(0), Failure::Canonical),
        (
            8,
            super::wire::blob(b"wrong-domain")
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            Failure::InvalidProfile,
        ),
        (
            9,
            super::wire::blob(&[98])
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            Failure::InvalidProfile,
        ),
        (
            10,
            super::wire::blob(&[1; 32])
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            Failure::InvalidProfile,
        ),
        (
            1,
            super::wire::blob(&[2; 32])
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            Failure::InvalidEvidence,
        ),
    ] {
        let mut f = fields.clone();
        f[index] = replacement;
        let b = super::wire::bytes(
            &super::wire::record(f)
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        assert_eq!(
            lifecycle_relation_bytes(&TestVerifier, &s, &b, limits())
                .into_parts()
                .0,
            Err(expected)
        );
    }
    let bytes = lifecycle_command_bytes(&s, &id(7), ctx, &c)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        lifecycle_relation_bytes(&TestVerifier, &s, &trailing, limits())
            .into_parts()
            .0,
        Err(Failure::Canonical)
    );
    let small = lifecycle_relation_bytes(&TestVerifier, &s, &bytes, zero_limits());
    assert!(matches!(small.result(), Err(Failure::Budget(_))));
    assert_eq!(
        lifecycle_relation_bytes(&TestVerifier, &s, &bytes, limits())
            .into_parts()
            .0
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
            .pre_root(),
        ctx.pre_root
    );
    let k = key(1, 0, 1);
    let malformed = LifecycleCommand::RegisterKey {
        key: k.clone(),
        authorization: [0; 96],
    };
    assert_eq!(
        lifecycle_relation(
            &TestVerifier,
            &s,
            &id(1),
            context(&s, 1, 0),
            &malformed,
            limits()
        )
        .into_parts()
        .0,
        Err(Failure::Canonical)
    );
    let wrong = LifecycleCommand::RegisterKey {
        key: k,
        authorization: fixture::sign(1, b"wrong subject"),
    };
    assert!(matches!(
        lifecycle_relation(
            &TestVerifier,
            &s,
            &id(1),
            context(&s, 1, 0),
            &wrong,
            limits()
        )
        .result(),
        Ok(Err(Reason::BadSignature))
    ));
    assert!(s.feed(&[1; 32]).is_none());
}
#[test]
fn oracle_native_time_quorum_survives_spread_failure_and_security_uses_exclusive_remaining_locks() {
    let (s, feed) = funded_feed();
    let mut s = apply(&s, 7, 0, &LifecycleCommand::OpenRound { feed, round: 0 });
    for (i, value) in [(1, 1), (2, 1), (3, 100)] {
        let r = report(&s, i, u64::from(i), feed, 0, value, 0);
        s = apply(&s, 7, 0, &LifecycleCommand::SubmitReport(r));
    }
    let uncertain = lifecycle_relation(
        &TestVerifier,
        &s,
        &id(7),
        context(&s, 7, 1),
        &LifecycleCommand::BuildAggregate { feed, round: 0 },
        limits(),
    );
    assert!(matches!(
        uncertain.result(),
        Ok(Err(Reason::ExcessUncertainty))
    ));
    let s = price(&s, feed, 1, NativePriceCommand::SyncTime { round: 0 });
    assert!(s.time_quorum(&feed).is_some());
    assert!(
        s.round(feed, 0)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .aggregate
            .is_none()
    );
    let (s, feed) = funded_feed();
    let s = complete_round(s, feed, 0, 100, 0);
    let s = build_and_stage(&s, feed, 0, 1);
    let a = &s
        .price(&feed)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .pending
        .as_ref()
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .aggregate;
    let sec = super::native_ports::security(&s, feed, Some(a), 1)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(sec.selected_bonds, [ZdexE18(1); 3]);
    assert!(sec.stress.is_none());
    assert_ne!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .held,
        sec.selected_bonds[0]
    );
}

#[test]
fn oracle_native_port_laws_reject_all_wrong_frames_and_borrowed_rewards() {
    let (s, feed) = funded_feed();
    for command in [
        LifecycleCommand::Price {
            feed,
            command: NativePriceCommand::AdvanceEpoch,
        },
        LifecycleCommand::Reward {
            feed,
            command: NativeRewardCommand::Fund(1),
        },
    ] {
        let ctx = context(&s, 2, 0);
        let actual = lifecycle_relation(&TestVerifier, &s, &id(2), ctx, &command, limits())
            .into_parts()
            .0
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        for reason in REASONS {
            assert!(matches!(
                check_lifecycle_laws(
                    &TestVerifier,
                    &s,
                    &id(2),
                    ctx,
                    &command,
                    Err(reason),
                    limits()
                )
                .result(),
                Err(Failure::Law(_))
            ));
        }
        let mut bad = actual.state().clone();
        bad.wallets.insert(id(2), ZdexE18(99));
        assert!(matches!(
            check_lifecycle_laws(&TestVerifier, &s, &id(2), ctx, &command, Ok(&bad), limits())
                .result(),
            Err(Failure::Law(_))
        ));
        assert_eq!(
            check_lifecycle_laws(
                &TestVerifier,
                &s,
                &id(2),
                ctx,
                &command,
                Ok(actual.state()),
                limits()
            )
            .result(),
            Ok(&())
        );
    }
    let mut r = super::native_rewards::empty(&s, feed)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    r.feed = [1; 32];
    let result = reward_relation(
        &super::native_rewards::rules(&s, feed)
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
        AccountContext {
            parameters: &s.profile.parameters,
            actor: &id(2),
            nonce: 0,
            clock: context(&s, 2, 0).clock,
            capacity: 64,
        },
        &r,
        &super::account_law_tests::bond(),
        &RewardCommand::Fund(1),
        limits(),
    );
    assert!(matches!(result.result(), Err(Failure::InvalidProfile)));
}

// Permanent independent Round 3 conformance controls.
#[test]
fn round3_slash_and_clawback_after_unregister() {
    let (s, feed) = funded_feed();
    let s = complete_round(s, feed, 0, 100, 0);
    let s = build_and_stage(&s, feed, 0, 1);
    let a = s
        .price(&feed)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .pending
        .as_ref()
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .aggregate
        .id();
    let report = s
        .aggregate(&a)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .reports()[0];
    let liability = s
        .report(&report)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .liability;
    let dispute = [51; 32];
    let s = apply(
        &s,
        2,
        1,
        &LifecycleCommand::Reward {
            feed,
            command: NativeRewardCommand::Fund(3),
        },
    );
    let s = apply(
        &s,
        7,
        1,
        &LifecycleCommand::Reward {
            feed,
            command: NativeRewardCommand::Accrue {
                aggregate: a,
                report,
            },
        },
    );
    let s = apply(
        &s,
        2,
        1,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::OpenDispute {
                id: dispute,
                liability,
            },
        },
    );
    let s = apply(
        &s,
        1,
        1,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Unregister,
        },
    );
    let v = FinalVerdict {
        adjudicator: [6; 32],
        dispute,
        violation: [52; 32],
        upheld: true,
        final_epoch: 1,
        finality_root: [9; 32],
    };
    let s = apply(
        &s,
        7,
        1,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Slash {
                dispute,
                verdict: v,
            },
        },
    );
    assert_eq!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .phase,
        ReporterPhase::Unbonding
    );
    assert_eq!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .held
            .0,
        4
    );
    assert_eq!(
        s.reward(&feed)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .claims[0]
            .phase,
        ClaimPhase::ClawedBack
    );
}
#[test]
fn round3_report_scope_and_dual_rotation_negative_controls() {
    let (s, feed) = funded_feed();
    let s = apply(&s, 7, 0, &LifecycleCommand::OpenRound { feed, round: 0 });
    let r = report(&s, 1, 1, feed, 0, 100, 0);
    let pk = s
        .key(&id(1), 0)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .key
        .public_key;
    assert!(TestVerifier.signature(&pk, &r.payload.message(), &r.signature));
    for case in 0..11 {
        let mut p = r.payload.clone();
        match case {
            0 => p.feed = [77; 32],
            1 => p.round = 1,
            2 => p.observed_epoch = 1,
            3 => p.sequence = 1,
            4 => p.profile = [77; 32],
            5 => p.chain = id(98),
            6 => p.source = id(2),
            7 => p.query = [77; 32],
            8 => p.observation_seconds = 1,
            9 => p.previous = Some([77; 32]),
            _ => p.key_version = 1,
        }
        assert!(
            !TestVerifier.signature(&pk, &p.message(), &r.signature),
            "case {case}"
        );
        if [2, 3, 4, 5, 6, 7, 8, 9].contains(&case) {
            let changed = RawReport {
                id: p.report_id(&r.signature),
                payload_hash: *zeno_fcis_crypto::RustCryptoSha256::hash(&p.canonical_json())
                    .as_bytes(),
                payload: p,
                signature: r.signature,
            };
            assert!(matches!(
                lifecycle_relation(
                    &TestVerifier,
                    &s,
                    &id(7),
                    context(&s, 7, 0),
                    &LifecycleCommand::SubmitReport(changed),
                    limits()
                )
                .result(),
                Ok(Err(Reason::BadSignature))
            ));
        }
    }
    let s = apply(&s, 7, 0, &LifecycleCommand::SubmitReport(r));
    let k = key(1, 1, 37);
    let ctx = context(&s, 1, 0);
    let reg = registration_message(&s, ctx.pre_root, ctx.nonce, &k)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let rot = rotation_message(&s, ctx.pre_root, ctx.nonce, 0, &k)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_ne!(reg, rot);
    for (old_sig, new_sig) in [
        (fixture::sign(1, &reg), fixture::sign(37, &rot)),
        (fixture::sign(1, &rot), fixture::sign(37, &reg)),
        (fixture::sign(37, &rot), fixture::sign(37, &rot)),
    ] {
        let c = LifecycleCommand::RotateKey {
            key: k.clone(),
            authorization: new_sig,
            old_authorization: old_sig,
        };
        assert!(matches!(
            lifecycle_relation(&TestVerifier, &s, &id(1), ctx, &c, limits()).result(),
            Ok(Err(Reason::BadSignature))
        ));
    }
    let mut wrong_pop = k.clone();
    wrong_pop.possession = key(2, 0, 41).possession;
    let msg = rotation_message(&s, ctx.pre_root, ctx.nonce, 0, &wrong_pop)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let c = LifecycleCommand::RotateKey {
        key: wrong_pop,
        authorization: fixture::sign(37, &msg),
        old_authorization: fixture::sign(1, &msg),
    };
    assert!(matches!(
        lifecycle_relation(&TestVerifier, &s, &id(1), ctx, &c, limits()).result(),
        Ok(Err(Reason::BadPoPOrRotation))
    ));
    let s = apply(
        &s,
        1,
        0,
        &LifecycleCommand::RotateKey {
            key: k,
            authorization: fixture::sign(37, &rot),
            old_authorization: fixture::sign(1, &rot),
        },
    );
    assert_eq!(
        s.reporter(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .chain
            .next_sequence,
        1
    );
    assert_eq!(
        s.key(&id(1), 0)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .retirement_sequence,
        Some(1)
    );
}
#[test]
fn round3_lex_first_diverse_and_no_spread_fallback() {
    let (s, f) = funded_feed();
    let mut rules = s
        .feed(&f)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .rules
        .clone();
    let mut sources: Vec<Source> = (1..=4)
        .map(|i| Source {
            id: id(i),
            operator: id(i),
            venue: id(i),
            family: id(i),
            transport: id(i),
            jurisdiction: id(i),
        })
        .collect();
    let mut os: Vec<Observation> = (1..=4)
        .map(|i| Observation {
            id: [i; 32],
            admission: [i + 10; 32],
            reporter: id(i),
            source: id(i),
            feed: f,
            profile: s.profile_id(),
            query: rules.query,
            round: 0,
            value: PriceE8::new(100)
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            epoch: 0,
            seconds: 0,
        })
        .collect();
    sources[1].operator = id(1);
    os.reverse();
    let a = aggregate(&rules, 0, 1, 1, &os, &sources, limits())
        .into_parts()
        .0
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(a.reporters(), &[id(1), id(3), id(4)]);
    sources[1].operator = id(2);
    rules.max_deviation_bps = 0;
    os.iter_mut()
        .find(|o| o.reporter == id(1))
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .value = PriceE8::new(1).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    // [2,3,4] would have zero spread, but selected [1,2,3] must fail.
    assert_eq!(
        aggregate(&rules, 0, 1, 1, &os, &sources, limits())
            .into_parts()
            .0,
        Ok(Err(Reason::ExcessUncertainty))
    );
}

#[test]
fn round3_native_repeated_slash_preserves_custody_and_punishment_history() {
    for (minimum, deposit, dispute, first_violation, second_violation) in
        [(2, 2, 4, 5, 6), (3, 5, 41, 42, 43)]
    {
        let mut p = profile();
        p.parameters
            .disputes
            .as_mut()
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .minimum_bond = ZdexE18(minimum);
        let mut s = LifecycleState::new(
            id(99),
            p,
            BTreeMap::from([
                (id(1), ZdexE18(10)),
                (id(2), ZdexE18(10)),
                (id(3), ZdexE18(10)),
            ]),
        )
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        for i in 1..=3 {
            s = register(&s, i, u64::from(i));
            s = apply(
                &s,
                i,
                0,
                &LifecycleCommand::Bond {
                    reporter: id(i),
                    command: BondCommand::Deposit(ZdexE18(deposit)),
                },
            );
        }
        let f = feed(&s);
        let feed = f.rules.feed;
        s = apply(&s, 7, 0, &LifecycleCommand::RegisterFeed(f));
        s = complete_round(s, feed, 0, 100, 0);
        s = build_and_stage(&s, feed, 0, 1);
        let liability = s
            .bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .liabilities[0]
            .id;
        let s = apply(
            &s,
            2,
            1,
            &LifecycleCommand::Bond {
                reporter: id(1),
                command: BondCommand::OpenDispute {
                    id: [dispute; 32],
                    liability,
                },
            },
        );
        let proof = FinalVerdict {
            adjudicator: [6; 32],
            dispute: [dispute; 32],
            violation: [first_violation; 32],
            upheld: true,
            final_epoch: 1,
            finality_root: [9; 32],
        };
        let first = apply(
            &s,
            7,
            1,
            &LifecycleCommand::Bond {
                reporter: id(1),
                command: BondCommand::Slash {
                    dispute: [dispute; 32],
                    verdict: proof.clone(),
                },
            },
        );
        for violation in [first_violation, second_violation] {
            let c = LifecycleCommand::Bond {
                reporter: id(1),
                command: BondCommand::Slash {
                    dispute: [dispute; 32],
                    verdict: FinalVerdict {
                        violation: [violation; 32],
                        ..proof.clone()
                    },
                },
            };
            let ctx = context(&first, 7, 1);
            let typed = lifecycle_relation(&TestVerifier, &first, &id(7), ctx, &c, limits())
                .into_parts()
                .0;
            let encoded = lifecycle_command_bytes(&first, &id(7), ctx, &c)
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
            let decoded = lifecycle_relation_bytes(&TestVerifier, &first, &encoded, limits())
                .into_parts()
                .0;
            assert!(
                matches!(typed, Ok(Err(Reason::AlreadyClaimedOrSlashed))),
                "{typed:?}"
            );
            assert_eq!(typed, decoded);
        }
        let total = first.wallets.values().map(|n| n.0).sum::<u128>()
            + first
                .bonds
                .values()
                .map(|b| {
                    b.held.0 + b.burned.0 + b.disputes.iter().map(|d| d.escrow.0).sum::<u128>()
                })
                .sum::<u128>();
        assert_eq!(total, 30);
        // A forged first-slash successor that drops the penalty instead of
        // paying it is refused by the independent conservation law.
        let slash = LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Slash {
                dispute: [dispute; 32],
                verdict: proof.clone(),
            },
        };
        let mut lost = first.clone();
        lost.wallets
            .insert(id(2), ZdexE18(first.wallet(&id(2)).0 - 1));
        for (candidate, expected) in [(&first, Ok(&())), (&lost, Err(&Failure::Law(115_500)))] {
            assert_eq!(
                check_lifecycle_laws(
                    &TestVerifier,
                    &s,
                    &id(7),
                    context(&s, 7, 1),
                    &slash,
                    Ok(candidate),
                    limits()
                )
                .result(),
                expected
            );
        }
        assert_eq!(
            first
                .bond(&id(1))
                .unwrap_or_else(|| panic!("test fixture value is absent"))
                .disputes[0]
                .slash_verdict,
            Some(proof)
        );
        assert_eq!(
            check_lifecycle_snapshot_bytes(
                &first,
                &first
                    .canonical_bytes()
                    .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
                limits()
            )
            .result(),
            Ok(&())
        );
    }
}
#[test]
fn round3_native_missed_opening_recovers_on_the_fixed_schedule() {
    let (s, feed) = funded_feed();
    for now in [1, 1000] {
        let next = apply(
            &s,
            7,
            now,
            &LifecycleCommand::OpenRound { feed, round: now },
        );
        assert!(next.round(feed, 0).is_none());
        assert_eq!(
            next.round(feed, now)
                .unwrap_or_else(|| panic!("test fixture value is absent"))
                .opening,
            now
        );
        assert_eq!(
            next.round(feed, now)
                .unwrap_or_else(|| panic!("test fixture value is absent"))
                .cutoff,
            now + 1
        );
        assert_eq!(s.bonds, next.bonds);
        assert_eq!(s.reporters, next.reporters);
    }
    let s = apply(&s, 7, 0, &LifecycleCommand::OpenRound { feed, round: 0 });
    let s = price(&s, feed, 3, NativePriceCommand::AdvanceEpoch);
    let next = apply(&s, 7, 3, &LifecycleCommand::OpenRound { feed, round: 3 });
    assert_eq!(s.round(feed, 0), next.round(feed, 0));
    assert!(next.round(feed, 1).is_none());
    assert!(next.round(feed, 2).is_none());
    assert_eq!(
        next.round(feed, 3)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .cutoff,
        4
    );
}
#[test]
fn round3_missed_opening_skips_only_expired_rounds() {
    // Heartbeat 2 and cutoff delta 1: round k is open during [2k, 2k+1).
    let mut p = profile();
    p.parameters
        .clock
        .as_mut()
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .heartbeat = 2;
    let s = LifecycleState::new(id(99), p, BTreeMap::from([(id(1), ZdexE18(10))]))
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let f = feed(&s);
    let feed = f.rules.feed;
    let s = apply(&s, 7, 0, &LifecycleCommand::RegisterFeed(f));
    let open = |s: &LifecycleState, now: u64, round: u64| {
        let ctx = context(s, 7, now);
        let c = LifecycleCommand::OpenRound { feed, round };
        let typed = lifecycle_relation(&TestVerifier, s, &id(7), ctx, &c, limits())
            .into_parts()
            .0
            .map(|r| r.map(|_| ()));
        let bytes = lifecycle_command_bytes(s, &id(7), ctx, &c)
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        let decoded = lifecycle_relation_bytes(&TestVerifier, s, &bytes, limits())
            .into_parts()
            .0
            .map(|r| r.map(|_| ()));
        assert_eq!(typed, decoded);
        typed
    };
    // Epoch 1: round 0 has closed and round 1 is not open yet.
    assert_eq!(open(&s, 1, 0), Ok(Err(Reason::WindowClosed)));
    assert_eq!(open(&s, 1, 1), Ok(Err(Reason::WindowClosed)));
    assert_eq!(open(&s, 1, 2), Ok(Err(Reason::WrongPhase)));
    assert_eq!(open(&s, 1, u64::MAX), Ok(Err(Reason::WrongPhase)));
    // Epoch 4: rounds 0 and 1 expired unopened; only round 2 can open.
    assert_eq!(open(&s, 4, 1), Ok(Err(Reason::WindowClosed)));
    assert_eq!(open(&s, 4, 3), Ok(Err(Reason::WrongPhase)));
    let next = apply(&s, 7, 4, &LifecycleCommand::OpenRound { feed, round: 2 });
    let row = next
        .round(feed, 2)
        .unwrap_or_else(|| panic!("test fixture value is absent"));
    assert_eq!((row.opening, row.cutoff), (4, 5));
    // Skipped rounds stay skipped, and an opened round is not reopened.
    assert_eq!(open(&next, 4, 0), Ok(Err(Reason::WrongPhase)));
    assert_eq!(open(&next, 4, 1), Ok(Err(Reason::WrongPhase)));
    assert_eq!(open(&next, 4, 2), Ok(Err(Reason::AlreadyExists)));
    // Round 3 keeps its own window; by epoch 10 rounds 3 and 4 have expired.
    assert_eq!(open(&next, 5, 3), Ok(Err(Reason::WindowClosed)));
    assert_eq!(open(&next, 10, 4), Ok(Err(Reason::WindowClosed)));
    assert_eq!(open(&next, 10, 6), Ok(Err(Reason::WrongPhase)));
    let later = apply(
        &next,
        7,
        10,
        &LifecycleCommand::OpenRound { feed, round: 5 },
    );
    let row = later
        .round(feed, 5)
        .unwrap_or_else(|| panic!("test fixture value is absent"));
    assert_eq!((row.opening, row.cutoff), (10, 11));
    assert_eq!(later.round(feed, 2), next.round(feed, 2));
    assert!(later.round(feed, 3).is_none() && later.round(feed, 4).is_none());
}
#[test]
fn round3_zdex_rewards_and_slash_conserve_one_custody_domain() {
    // zDEX rewards share the one wallet map with bonds, escrow and burns. A
    // genesis wallet at u128::MAX makes every custody total carry past U128.
    let mut p = profile();
    p.parameters
        .economy
        .as_mut()
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .reward_unit = RewardUnit {
        asset: id(10),
        scale: 18,
    };
    p.reward_split = RewardSplit {
        reporter: 2,
        treasury: 1,
        burn: 1,
        denominator: 5,
        treasury_account: id(8),
    };
    let wallets = BTreeMap::from([
        (id(1), ZdexE18(u128::MAX)),
        (id(2), ZdexE18(10)),
        (id(3), ZdexE18(10)),
    ]);
    let mut s = LifecycleState::new(id(99), p, wallets)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    for i in 1..=3 {
        s = register(&s, i, u64::from(i));
        s = apply(
            &s,
            i,
            0,
            &LifecycleCommand::Bond {
                reporter: id(i),
                command: BondCommand::Deposit(ZdexE18(5)),
            },
        );
    }
    let f = feed(&s);
    let feed = f.rules.feed;
    s = apply(&s, 7, 0, &LifecycleCommand::RegisterFeed(f));
    let s = complete_round(s, feed, 0, 100, 0);
    let s = build_and_stage(&s, feed, 0, 1);
    let a = s
        .price(&feed)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .pending
        .as_ref()
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .aggregate
        .id();
    let report = s
        .aggregate(&a)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .reports()[0];
    let liability = s
        .report(&report)
        .unwrap_or_else(|| panic!("test fixture value is absent"))
        .liability;
    // Every apply below also runs the independent conservation law.
    let fund = LifecycleCommand::Reward {
        feed,
        command: NativeRewardCommand::Fund(10),
    };
    let funded = apply(&s, 1, 1, &fund);
    assert_eq!(funded.wallet(&id(8)), ZdexE18(2));
    assert_eq!(
        funded
            .reward(&feed)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .remaining,
        4
    );
    let mut forged = funded.clone();
    forged.wallets.remove(&id(8));
    assert_eq!(
        check_lifecycle_laws(
            &TestVerifier,
            &s,
            &id(1),
            context(&s, 1, 1),
            &fund,
            Ok(&forged),
            limits()
        )
        .result(),
        Err(&Failure::Law(115_500))
    );
    let s = apply(
        &funded,
        7,
        1,
        &LifecycleCommand::Reward {
            feed,
            command: NativeRewardCommand::Accrue {
                aggregate: a,
                report,
            },
        },
    );
    let dispute = [61; 32];
    let s = apply(
        &s,
        2,
        1,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::OpenDispute {
                id: dispute,
                liability,
            },
        },
    );
    let s = apply(
        &s,
        7,
        1,
        &LifecycleCommand::Bond {
            reporter: id(1),
            command: BondCommand::Slash {
                dispute,
                verdict: FinalVerdict {
                    adjudicator: [6; 32],
                    dispute,
                    violation: [62; 32],
                    upheld: true,
                    final_epoch: 1,
                    finality_root: [9; 32],
                },
            },
        },
    );
    assert_eq!(s.wallet(&id(2)), ZdexE18(5));
    assert_eq!(
        s.bond(&id(1))
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .held,
        ZdexE18(4)
    );
    assert_eq!(
        s.reward(&feed)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .claims[0]
            .phase,
        ClaimPhase::ClawedBack
    );
    assert_eq!(
        s.reward(&feed)
            .unwrap_or_else(|| panic!("test fixture value is absent"))
            .remaining,
        4
    );
}
