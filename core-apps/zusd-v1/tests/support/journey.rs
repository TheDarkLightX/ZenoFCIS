use application::publication::{CrashPoint, Error as StoreError, FileLedger, Publication};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use support::*;
use zenofcis_zusd_v1::{
    application::{self, Command as AppCommand, *},
    ledger::Asset,
    *,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn directory(label: &str) -> PathBuf {
    loop {
        let p = PathBuf::from("/tmp/zf26-zusd/tmp").join(format!(
            "step4-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&p) {
            Ok(()) => return p,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => panic!("cannot reserve test directory: {e}"),
        }
    }
}
struct StepOutput {
    legs: Vec<ledger::Movement>,
}
impl StepOutput {
    fn movements(&self) -> &[ledger::Movement] {
        &self.legs
    }
}
struct Journey {
    p: Policy,
    g: State,
    s: State,
    store: FileLedger,
}
impl Journey {
    fn new(p: Policy) -> Self {
        let g = genesis(&p);
        let path = directory("journey");
        let store = FileLedger::create(&path, &p, &g)
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        println!("retained step4 journey ledger: {}", path.display());
        Self {
            p,
            g: g.clone(),
            s: g,
            store,
        }
    }
    fn step(&mut self, r: Request) -> StepOutput {
        let frozen = self.s.clone();
        let r = sign(&self.p, &self.s, r)
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        let e = decide(&self.p, &self.s, &r, &TestVerifier, limits());
        let used = e.usage();
        let out = e
            .into_parts()
            .0
            .unwrap_or_else(|e| panic!("{:?} -> {e:?}", r.command));
        assert_eq!(out.usage(), used);
        assert_eq!(self.s, frozen);
        let post = out.post().clone();
        let legs = out.movements().to_vec();
        let critical = matches!(r.command, AppCommand::Liquidate(_) | AppCommand::Redeem(_));
        let count = self
            .store
            .bundles(&self.p, &self.g, &TestVerifier, limits())
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
            .len();
        let competing = critical.then(|| prepared(&self.p, &frozen, 4));
        let out = if critical {
            assert!(matches!(
                self.store.publish_with_crash(
                    &self.p,
                    &self.g,
                    &TestVerifier,
                    out,
                    limits(),
                    Some(CrashPoint::DuringWrite)
                ),
                Err(StoreError::Crash(CrashPoint::DuringWrite))
            ));
            assert_eq!(
                self.store
                    .load(&self.p, &self.g, &TestVerifier, limits())
                    .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
                frozen
            );
            decide(&self.p, &frozen, &r, &TestVerifier, limits())
                .into_parts()
                .0
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
        } else {
            out
        };
        assert_eq!(
            self.store
                .publish(&self.p, &self.g, &TestVerifier, out, limits())
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            Publication::Committed
        );
        self.s = self
            .store
            .load(&self.p, &self.g, &TestVerifier, limits())
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        assert_eq!(self.s, post);
        if let Some(competing) = competing {
            assert!(matches!(
                self.store
                    .publish(&self.p, &self.g, &TestVerifier, competing, limits()),
                Err(StoreError::Conflict)
            ));
            let duplicate = decide(&self.p, &frozen, &r, &TestVerifier, limits())
                .into_parts()
                .0
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
            assert_eq!(
                self.store
                    .publish(&self.p, &self.g, &TestVerifier, duplicate, limits())
                    .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
                Publication::Duplicate
            );
        }
        assert_eq!(
            self.store
                .bundles(&self.p, &self.g, &TestVerifier, limits())
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                .len(),
            count + 1
        );
        StepOutput { legs }
    }
    fn reject(&self, request: &Request, expected: Failure) {
        let root = self.s.root();
        let before = self
            .store
            .bundles(&self.p, &self.g, &TestVerifier, limits())
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        let e = decide(&self.p, &self.s, request, &TestVerifier, limits());
        assert_eq!(e.result().err(), Some(expected));
        assert_eq!(
            self.store
                .load(&self.p, &self.g, &TestVerifier, limits())
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                .root(),
            root
        );
        assert_eq!(
            self.store
                .bundles(&self.p, &self.g, &TestVerifier, limits())
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            before
        );
    }
    fn advance(&mut self, epoch: u64, watermark: u64) {
        let r = advance_request(&self.p, &self.s, epoch, watermark);
        self.step(r);
    }
    fn price(&mut self, action: OracleAction, round: u64, value: u128, event: u8) {
        let r = price_request(&self.p, &self.s, action, round, value, event);
        self.step(r);
    }
    fn open(&mut self, who: u8, collateral: u128, principal: u128) {
        let mut r = raw(
            &self.s,
            who,
            AppCommand::Open {
                collateral,
                principal,
                max_fee: Q,
                upper: None,
                lower: None,
            },
        );
        use_price(&self.p, &self.s, &mut r);
        self.step(r);
    }
}
#[test]
fn multi_user_pending_commit_liquidation_redemption_claims_halt_recovery() {
    let p = policy(false);
    let mut j = Journey::new(p.clone());
    j.advance(2, 2);
    j.price(OracleAction::InitializeFirstMatured, 1, 200_000_000, 110);
    let r = raw(&j.s, 1, AppCommand::Stake(staking::Command::Stake(100 * Q)));
    let _ = j.step(r);
    j.open(1, 1200 * Q, 1000 * Q);
    j.open(2, 700 * Q, 1000 * Q);
    j.open(3, 3000 * Q, 2000 * Q);
    let r = raw(
        &j.s,
        5,
        AppCommand::Pool(stability::Command::RegisterFrontEnd(Q * 8 / 10)),
    );
    let _ = j.step(r);
    for (who, amount, event) in [(1, 400 * Q, 120), (2, 100 * Q, 121)] {
        let mut r = raw(
            &j.s,
            who,
            AppCommand::Pool(stability::Command::Deposit {
                amount,
                tag: Some(id(5)),
            }),
        );
        drip(&mut r, event, 0);
        let _ = j.step(r);
    }
    j.price(OracleAction::ImportPending, 2, 300_000_000, 111);
    let mut r = raw(
        &j.s,
        4,
        AppCommand::Open {
            collateral: 500 * Q,
            principal: 1000 * Q,
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(&p, &j.s, &mut r);
    let r = sign(&p, &j.s, r).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &j.s, &r, &TestVerifier, limits()).result().err(),
        Some(Failure::Reject(1043))
    );
    j.price(OracleAction::ImportPending, 3, 150_000_000, 112);
    let mut r = raw(&j.s, 1, AppCommand::Pool(stability::Command::Withdraw(Q)));
    use_price(&p, &j.s, &mut r);
    drip(&mut r, 122, 0);
    let r = sign(&p, &j.s, r).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &j.s, &r, &TestVerifier, limits()).result().err(),
        Some(Failure::Reject(1044))
    );
    let mut pending_only = raw(
        &j.s,
        6,
        AppCommand::Liquidate(liquidation::Command::Direct(application::vault_id(
            &p,
            id(2),
        ))),
    );
    use_price(&p, &j.s, &mut pending_only);
    let pending_only = sign(&p, &j.s, pending_only)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    j.reject(&pending_only, Failure::Reject(1044));
    j.advance(4, 4);
    j.price(OracleAction::CommitMatured, 3, 150_000_000, 113);
    let target = application::vault_id(&p, id(2));
    let mut r = raw(
        &j.s,
        6,
        AppCommand::Liquidate(liquidation::Command::Direct(target)),
    );
    use_price(&p, &j.s, &mut r);
    drip(&mut r, 123, 20 * Q);
    let out = j.step(r);
    assert!(j.s.book().default_debt > 0);
    assert_eq!(j.s.book().system.active, 2);
    assert!(
        out.movements()
            .iter()
            .any(|m| m.quantity == ledger::Quantity::Zusd(50 * Q) && m.to == Some(id(6)))
    );
    assert_eq!(j.s.pool().pool.total, 0);
    for (who, event) in [(1, 124), (2, 125)] {
        let mut r = raw(&j.s, who, AppCommand::Pool(stability::Command::Claim));
        drip(&mut r, event, 0);
        let _ = j.step(r);
    }
    assert!(balance(&j.s, 5, Asset::Zdex) > 1000 * Q);
    assert!(balance(&j.s, 1, Asset::Agrs) > 18_800 * Q);
    j.advance(20_160, 20_160);
    let a =
        j.s.book()
            .vaults
            .iter()
            .find(|v| v.owner == id(1))
            .unwrap_or_else(|| panic!("missing test fixture value"));
    let entire = *vaults::apply_pending(*a, j.s.book().system, limits())
        .result()
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let net = entire.debt - entire.reserve;
    let command = redemption::Command {
        maximum: net,
        visits: 0,
        first: None,
        partial_nicr: 0,
        upper: None,
        lower: None,
        max_fee: Q,
        min_out: 1,
    };
    let mut r = raw(&j.s, 3, AppCommand::Redeem(command));
    use_price(&p, &j.s, &mut r);
    let _ = j.step(r);
    assert_eq!(j.s.book().system.active, 1);
    assert!(
        j.s.book()
            .surplus
            .iter()
            .any(|(a, n)| *a == id(1) && *n > 0)
    );
    let r = raw(&j.s, 1, AppCommand::ClaimSurplus);
    let _ = j.step(r);
    let r = raw(&j.s, 1, AppCommand::Stake(staking::Command::Claim));
    let _ = j.step(r);
    assert!(balance(&j.s, 1, Asset::Zusd) > 600 * Q);
    assert!(j.s.staking().custody_a < 100 * Q);
    let stale = sign(
        &p,
        &j.s,
        raw(&j.s, 1, AppCommand::Stake(staking::Command::Claim)),
    )
    .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let mut dispute = raw(&j.s, 6, AppCommand::ObserveOracle);
    let mut view =
        j.s.oracle()
            .unwrap_or_else(|| panic!("missing test fixture value"))
            .clone();
    view.status = vaults::PriceStatus::Disputed;
    dispute.oracle = Some(OracleReceipt {
        before: j.s.oracle_root(),
        after: id(154),
        event: Some(id(128)),
        action: OracleAction::RefreshStatus,
        view,
    });
    j.step(dispute);
    j.reject(&stale, Failure::Reject(1000));
    j.advance(20_161, 1_000_000);
    let mut r = raw(
        &j.s,
        3,
        AppCommand::Adjust {
            collateral: vaults::Delta::Remove(Q),
            debt: vaults::Delta::None,
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(&p, &j.s, &mut r);
    let r = sign(&p, &j.s, r).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &j.s, &r, &TestVerifier, limits()).result().err(),
        Some(Failure::Reject(1022))
    );
    let r = raw(
        &j.s,
        3,
        AppCommand::Adjust {
            collateral: vaults::Delta::Add(10 * Q),
            debt: vaults::Delta::Remove(10 * Q),
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    let _ = j.step(r);
    let mut r = raw(
        &j.s,
        3,
        AppCommand::Pool(stability::Command::Deposit {
            amount: 10 * Q,
            tag: None,
        }),
    );
    drip(&mut r, 126, 0);
    let _ = j.step(r);
    j.price(OracleAction::ImportPending, 4, 50_000_000, 114);
    j.advance(20_163, 1_000_002);
    j.price(OracleAction::CommitMatured, 4, 50_000_000, 115);
    assert!(j.s.book().system.collateral / 2 < j.s.book().system.debt); // insolvency never vetoes commitment
    j.price(OracleAction::ImportPending, 5, 200_000_000, 116);
    j.advance(20_165, 1_000_004);
    j.price(OracleAction::CommitMatured, 5, 200_000_000, 117);
    let mut r = raw(
        &j.s,
        3,
        AppCommand::Pool(stability::Command::Withdraw(10 * Q)),
    );
    use_price(&p, &j.s, &mut r);
    drip(&mut r, 127, 0);
    let _ = j.step(r);
    assert_eq!(j.s.ledger().supply, j.s.book().system.debt);
    assert_eq!(j.s.exposure().len(), 2);
    let events = j.s.exposure().iter().map(|e| e.id).collect();
    let r = raw(&j.s, 6, AppCommand::ReleaseExposure { events });
    let early =
        sign(&p, &j.s, r.clone()).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    j.reject(&early, Failure::Canonical);
    j.advance(20_170, 1_000_009);
    let r = raw(&j.s, 6, r.command);
    j.step(r);
    assert!(j.s.exposure().is_empty());
    println!(
        "step4 multi-user journey: pending rise/drop, offset+redistribution, redemption, surplus, host/staking claims, halt, insolvent commitment, buffered recovery"
    );
}
fn prepared(p: &Policy, s: &State, who: u8) -> Prepared {
    let r = sign(
        p,
        s,
        raw(s, who, AppCommand::Stake(staking::Command::Stake(Q))),
    )
    .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    decide(p, s, &r, &TestVerifier, limits())
        .into_parts()
        .0
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
}
fn comparison(name: &str) -> bool {
    std::env::var("ZUSD_DISABLE_COMPARISON").ok().as_deref() != Some(name)
}
#[test]
fn receipt_bad_tag_refuses_each_required_domain() {
    let p = policy(false);
    let s = ready(&p);
    let mut r = raw(
        &s,
        1,
        AppCommand::Open {
            collateral: 1200 * Q,
            principal: 1000 * Q,
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(&p, &s, &mut r);
    let r = sign(&p, &s, r).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert!(decide(&p, &s, &r, &TestVerifier, limits()).result().is_ok());
    for i in 0..r.proofs.len() {
        let mut bad = r.clone();
        bad.proofs[i].bytes[0] ^= 1;
        let e = decide(&p, &s, &bad, &TestVerifier, limits());
        if comparison("bad-tag") {
            assert!(
                e.result().is_err(),
                "BAD_TAG_ACCEPTED {:?}",
                bad.proofs[i].domain
            )
        }
        assert!(e.usage().steps() > 0);
    }
    assert_eq!(s.version(), 2);
}
#[test]
fn price_view_without_oracle_receipt_never_authorizes() {
    let p = policy(false);
    let s = ready(&p);
    let mut r = raw(
        &s,
        1,
        AppCommand::Open {
            collateral: 1200 * Q,
            principal: 1000 * Q,
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(&p, &s, &mut r);
    let mut r = sign(&p, &s, r).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    r.proofs.retain(|p| p.domain != Domain::Oracle);
    let e = decide(&p, &s, &r, &TestVerifier, limits());
    if comparison("price-view") {
        assert!(e.result().is_err(), "PRICE_VIEW_BECAME_ACTION_AUTHORITY")
    }
}
#[test]
fn stale_borrowed_replayed_receipts_and_backend_failures_refuse() {
    let p = policy(false);
    let mut s = ready(&p);
    let signed = sign(
        &p,
        &s,
        raw(&s, 1, AppCommand::Stake(staking::Command::Stake(Q))),
    )
    .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let frozen = s.clone();
    let ok = decide(&p, &s, &signed, &TestVerifier, limits());
    let out = ok
        .into_parts()
        .0
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    s = out.post().clone();
    assert_eq!(
        decide(&p, &s, &signed, &TestVerifier, limits())
            .result()
            .err(),
        Some(Failure::Reject(1000))
    );
    let mut replay = signed.clone();
    replay.proofs.clear();
    let replay =
        sign(&p, &s, replay).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &s, &replay, &TestVerifier, limits())
            .result()
            .err(),
        Some(Failure::Reject(1001))
    );
    let mut borrowed = signed.clone();
    borrowed.actor = id(2);
    assert!(
        decide(&p, &frozen, &borrowed, &TestVerifier, limits())
            .result()
            .is_err()
    );
    let mut time = signed.clone();
    time.clock.epoch += 1;
    assert!(
        decide(&p, &frozen, &time, &TestVerifier, limits())
            .result()
            .is_err()
    );
    let mut duplicate = signed.clone();
    duplicate.proofs.insert(1, duplicate.proofs[0].clone());
    assert!(
        decide(&p, &frozen, &duplicate, &TestVerifier, limits())
            .result()
            .is_err()
    );
    struct Missing;
    impl ReceiptVerifierV1 for Missing {
        fn verify(&self, _: Domain, _: Id, _: &[u8], _: &[u8]) -> Result<(), VerificationFailure> {
            Err(VerificationFailure::Unavailable)
        }
    }
    assert_eq!(
        decide(&p, &frozen, &signed, &Missing, limits())
            .result()
            .err(),
        Some(Failure::UnboundProfile)
    );
}
#[test]
fn parameter_change_without_approved_policy_refuses() {
    let p = policy(false);
    let s = ready(&p);
    let r = raw(
        &s,
        6,
        AppCommand::ChangeParameters {
            policy: id(71),
            version: 2,
            minimum_net: 400 * Q,
            reserve: 25 * Q,
        },
    );
    let result = sign(&p, &s, r).and_then(|r| {
        decide(&p, &s, &r, &TestVerifier, limits())
            .into_parts()
            .0
            .map(|_| ())
    });
    if comparison("parameter-policy") {
        assert_eq!(
            result,
            Err(Failure::UnboundProfile),
            "UNAPPROVED_PARAMETER_CHANGE"
        )
    }
}
#[test]
fn approved_change_preserves_live_reserves_and_closes_each_once() {
    let p = policy(true);
    let mut s = ready(&p);
    let r = raw(&s, 1, AppCommand::Stake(staking::Command::Stake(100 * Q)));
    let _ = step(&p, &mut s, r);
    for who in 1..=3 {
        open(&p, &mut s, who, 2000 * Q, 1000 * Q)
    }
    let before = s.book().clone();
    let r = raw(
        &s,
        6,
        AppCommand::ChangeParameters {
            policy: id(71),
            version: 2,
            minimum_net: 400 * Q,
            reserve: 25 * Q,
        },
    );
    let _ = step(&p, &mut s, r);
    assert_eq!(s.book(), &before);
    assert_eq!(s.parameters().len(), 2);
    open(&p, &mut s, 4, 2000 * Q, 400 * Q);
    assert_eq!(s.book().gas_balance, 175 * Q);
    assert_eq!(
        s.book()
            .vaults
            .iter()
            .find(|v| v.owner == id(4))
            .unwrap_or_else(|| panic!("missing test fixture value"))
            .reserve,
        25 * Q
    );
    let r = raw(&s, 1, AppCommand::Stake(staking::Command::Claim));
    let _ = step(&p, &mut s, r);
    let mut r = raw(&s, 1, AppCommand::Close);
    use_price(&p, &s, &mut r);
    let _ = step(&p, &mut s, r);
    assert_eq!(s.book().gas_balance, 125 * Q);
    let mut r = raw(&s, 1, AppCommand::Close);
    use_price(&p, &s, &mut r);
    let r = sign(&p, &s, r).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &s, &r, &TestVerifier, limits()).result().err(),
        Some(Failure::Reject(1011))
    );
}
#[test]
fn funding_is_authenticated_asset_specific_and_single_use() {
    let p = policy(false);
    let mut s = ready(&p);
    let mut r = raw(
        &s,
        2,
        AppCommand::Fund {
            event: id(150),
            recipient: id(2),
            quantity: ledger::Quantity::Agrs(7 * Q),
        },
    );
    r.backing.agrs += 7 * Q;
    r.backing.funding = id(151);
    let _ = step(&p, &mut s, r.clone());
    assert_eq!(balance(&s, 2, Asset::Agrs), 20_007 * Q);
    r.nonce = s.nonce(id(2));
    r.backing = s.backing();
    r.backing.agrs += 7 * Q;
    let r = sign(&p, &s, r).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &s, &r, &TestVerifier, limits()).result().err(),
        Some(Failure::Reject(1001))
    );
    let r = sign(
        &p,
        &s,
        raw(
            &s,
            2,
            AppCommand::Fund {
                event: id(152),
                recipient: id(2),
                quantity: ledger::Quantity::Zusd(Q),
            },
        ),
    )
    .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &s, &r, &TestVerifier, limits()).result().err(),
        Some(Failure::Domain)
    );
    let mut bad = raw(
        &s,
        2,
        AppCommand::Fund {
            event: id(153),
            recipient: id(2),
            quantity: ledger::Quantity::Zdex(Q),
        },
    );
    bad.backing.zdex += 2 * Q;
    let bad = sign(&p, &s, bad).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &s, &bad, &TestVerifier, limits()).result().err(),
        Some(Failure::Conservation)
    );
}
#[test]
fn exhausted_budget_and_retained_inputs_have_no_successor() {
    let p = policy(false);
    let s = ready(&p);
    let frozen = s.canonical_bytes();
    let mut r = sign(
        &p,
        &s,
        raw(&s, 1, AppCommand::Stake(staking::Command::Stake(Q))),
    )
    .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let e = decide(
        &p,
        &s,
        &r,
        &TestVerifier,
        Limits {
            steps: 150_000,
            reads: 100_000_000,
            bytes: 500_000_000,
        },
    );
    assert!(matches!(e.result(), Err(Failure::Budget { .. })));
    assert!(e.usage().steps() > 0);
    assert_eq!(s.canonical_bytes(), frozen);
    let out = decide(&p, &s, &r, &TestVerifier, limits())
        .into_parts()
        .0
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let bytes = out.canonical_bytes().to_vec();
    r.proofs[0].bytes[0] ^= 1;
    r.command = AppCommand::ClaimSurplus;
    assert_eq!(out.canonical_bytes(), bytes);
    assert_eq!(s.canonical_bytes(), frozen);
}
#[test]
fn cas_conflict_retry_and_cold_replay_never_duplicate_economics() {
    let p = policy(false);
    let g = genesis(&p);
    let path = directory("cas");
    let store = FileLedger::create(&path, &p, &g)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let first = prepared(&p, &g, 1);
    let duplicate = prepared(&p, &g, 1);
    let conflict = prepared(&p, &g, 2);
    let root = first.post().root();
    assert_eq!(
        store
            .publish(&p, &g, &TestVerifier, first, limits())
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
        Publication::Committed
    );
    assert!(matches!(
        store.publish(&p, &g, &TestVerifier, conflict, limits()),
        Err(StoreError::Conflict)
    ));
    assert_eq!(
        store
            .publish(&p, &g, &TestVerifier, duplicate, limits())
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
        Publication::Duplicate
    );
    let cold = FileLedger::open(&path);
    assert_eq!(
        cold.load(&p, &g, &TestVerifier, limits())
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
            .root(),
        root
    );
    assert_eq!(
        cold.bundles(&p, &g, &TestVerifier, limits())
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
            .len(),
        1
    );
}
#[test]
fn concurrent_publishers_share_one_cas_lock() {
    use std::sync::{Arc, Barrier};
    let p = policy(false);
    let g = genesis(&p);
    let path = directory("concurrent");
    let store = FileLedger::create(&path, &p, &g)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = (1..=2)
        .map(|who| {
            let (p, g, store, barrier) = (p.clone(), g.clone(), store.clone(), barrier.clone());
            std::thread::spawn(move || {
                let plan = prepared(&p, &g, who);
                barrier.wait();
                store.publish(&p, &g, &TestVerifier, plan, limits())
            })
        })
        .collect();
    let mut committed = 0;
    let mut conflicts = 0;
    for t in threads {
        match t
            .join()
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
        {
            Ok(Publication::Committed) => committed += 1,
            Err(StoreError::Conflict) => conflicts += 1,
            other => panic!("{other:?}"),
        }
    }
    assert_eq!((committed, conflicts), (1, 1));
    assert_eq!(
        store
            .bundles(&p, &g, &TestVerifier, limits())
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
            .len(),
        1
    );
}
#[test]
fn crash_worker() {
    let Ok(value) = std::env::var("ZUSD_STEP4_CRASH") else {
        return;
    };
    let (path, index) = value
        .rsplit_once(':')
        .unwrap_or_else(|| panic!("missing test fixture value"));
    let index: usize = index
        .parse()
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let point = [
        CrashPoint::BeforeWrite,
        CrashPoint::DuringWrite,
        CrashPoint::AfterWrite,
        CrashPoint::AfterFileSync,
        CrashPoint::AfterRename,
        CrashPoint::AfterDirectorySync,
    ][index];
    let p = policy(false);
    let g = genesis(&p);
    let store = FileLedger::open(path);
    let plan = prepared(&p, &g, 1);
    match store.publish_with_crash(&p, &g, &TestVerifier, plan, limits(), Some(point)) {
        Err(StoreError::Crash(got)) if got == point => std::process::exit(77),
        other => panic!("{other:?}"),
    }
}
#[test]
fn atomic_publication_survives_all_six_crash_points() {
    for index in 0..6 {
        let p = policy(false);
        let g = genesis(&p);
        let path = directory("crash");
        let store = FileLedger::create(&path, &p, &g)
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        let root = prepared(&p, &g, 1).post().root();
        let child = std::process::Command::new(
            std::env::current_exe()
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
        )
        .args(["--exact", "crash_worker", "--nocapture"])
        .env("ZUSD_STEP4_CRASH", format!("{}:{index}", path.display()))
        .output()
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        assert_eq!(
            child.status.code(),
            Some(77),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        let loaded = match store.load(&p, &g, &TestVerifier, limits()) {
            Ok(state) => state,
            Err(e) => {
                if comparison("atomic-publication") {
                    panic!("NON_ATOMIC_PUBLICATION partial ledger at {index}: {e:?}")
                } else {
                    continue;
                }
            }
        };
        if comparison("atomic-publication") {
            assert_eq!(
                loaded.root(),
                if index < 4 { g.root() } else { root },
                "NON_ATOMIC_PUBLICATION at {index}"
            )
        }
        let committed = loaded.root() == root;
        assert_eq!(
            store
                .publish(&p, &g, &TestVerifier, prepared(&p, &g, 1), limits())
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            if committed {
                Publication::Duplicate
            } else {
                Publication::Committed
            }
        );
        assert_eq!(
            store
                .load(&p, &g, &TestVerifier, limits())
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                .root(),
            root
        );
        assert_eq!(
            store
                .bundles(&p, &g, &TestVerifier, limits())
                .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
                .len(),
            1
        );
    }
}
#[test]
fn cold_replay_rechecks_verifier_and_rejects_corrupt_bytes() {
    let p = policy(false);
    let g = genesis(&p);
    let path = directory("tamper");
    let store = FileLedger::create(&path, &p, &g)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    store
        .publish(&p, &g, &TestVerifier, prepared(&p, &g, 1), limits())
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    struct Invalid;
    impl ReceiptVerifierV1 for Invalid {
        fn verify(&self, _: Domain, _: Id, _: &[u8], _: &[u8]) -> Result<(), VerificationFailure> {
            Err(VerificationFailure::Invalid)
        }
    }
    assert!(store.load(&p, &g, &Invalid, limits()).is_err());
    let ledger = path.join("ledger");
    let mut bytes =
        std::fs::read(&ledger).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(ledger, bytes).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert!(store.load(&p, &g, &TestVerifier, limits()).is_err());
}
#[test]
fn stale_price_freezes_risk_but_not_repayment_or_evidence() {
    let p = policy(false);
    let mut s = ready(&p);
    open(&p, &mut s, 1, 2000 * Q, 1000 * Q);
    open(&p, &mut s, 2, 2000 * Q, 1000 * Q);
    advance(&p, &mut s, 3, 1_000_000);
    let mut risk = raw(
        &s,
        1,
        AppCommand::Adjust {
            collateral: vaults::Delta::Remove(Q),
            debt: vaults::Delta::None,
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(&p, &s, &mut risk);
    let risk = sign(&p, &s, risk).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &s, &risk, &TestVerifier, limits())
            .result()
            .err(),
        Some(Failure::Reject(1021))
    );
    let r = raw(
        &s,
        1,
        AppCommand::Adjust {
            collateral: vaults::Delta::Add(Q),
            debt: vaults::Delta::Remove(Q),
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    let _ = step(&p, &mut s, r);
    price(&p, &mut s, OracleAction::ImportPending, 2, 1, 160);
}
#[test]
fn pending_restrictions_buffer_and_exposure_shocks_are_preserved() {
    let p = policy(false);
    let mut s = ready(&p);
    price(&p, &mut s, OracleAction::ImportPending, 2, 100_000_000, 161);
    let early = sign(
        &p,
        &s,
        price_request(&p, &s, OracleAction::CommitMatured, 2, 100_000_000, 162),
    )
    .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &s, &early, &TestVerifier, limits())
            .result()
            .err(),
        Some(Failure::Canonical)
    );
    let mut block = raw(&s, 6, AppCommand::ObserveOracle);
    let mut view = s
        .oracle()
        .unwrap_or_else(|| panic!("missing test fixture value"))
        .clone();
    view.pending_blocked = true;
    block.oracle = Some(OracleReceipt {
        before: s.oracle_root(),
        after: id(164),
        event: Some(id(163)),
        action: OracleAction::RefreshStatus,
        view,
    });
    let _ = step(&p, &mut s, block);
    let mut clear = raw(&s, 6, AppCommand::ObserveOracle);
    let mut view = s
        .oracle()
        .unwrap_or_else(|| panic!("missing test fixture value"))
        .clone();
    view.pending_blocked = false;
    clear.oracle = Some(OracleReceipt {
        before: s.oracle_root(),
        after: id(166),
        event: Some(id(165)),
        action: OracleAction::RefreshStatus,
        view,
    });
    let clear =
        sign(&p, &s, clear).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &s, &clear, &TestVerifier, limits())
            .result()
            .err(),
        Some(Failure::Canonical)
    );
    let mut shock = raw(&s, 6, AppCommand::ObserveOracle);
    let mut view = s
        .oracle()
        .unwrap_or_else(|| panic!("missing test fixture value"))
        .clone();
    view.capacity = 0;
    shock.oracle = Some(OracleReceipt {
        before: s.oracle_root(),
        after: id(168),
        event: Some(id(167)),
        action: OracleAction::RefreshStatus,
        view,
    });
    let before = s.ledger().clone();
    let _ = step(&p, &mut s, shock);
    assert_eq!(s.ledger(), &before);
    let mut risk = raw(
        &s,
        1,
        AppCommand::Open {
            collateral: 2000 * Q,
            principal: 1000 * Q,
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(&p, &s, &mut risk);
    let risk = sign(&p, &s, risk).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &s, &risk, &TestVerifier, limits())
            .result()
            .err(),
        Some(Failure::Reject(1024))
    );
}
#[test]
fn unbound_clock_and_drip_do_not_acquire_defaults() {
    let original = policy(false);
    let mut config = original.config().clone();
    config.clock = None;
    config.drip = None;
    let p = Policy::bind(config).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let mut s = ready(&p);
    let mut r = raw(
        &s,
        1,
        AppCommand::Open {
            collateral: 2000 * Q,
            principal: 1000 * Q,
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(&p, &s, &mut r);
    let r = sign(&p, &s, r).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &s, &r, &TestVerifier, limits()).result().err(),
        Some(Failure::UnboundProfile)
    );
    let r = raw(&s, 1, AppCommand::Stake(staking::Command::Stake(Q)));
    let _ = step(&p, &mut s, r);
    let mut r = raw(&s, 1, AppCommand::Pool(stability::Command::Claim));
    drip(&mut r, 170, 0);
    let r = sign(&p, &s, r).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        decide(&p, &s, &r, &TestVerifier, limits()).result().err(),
        Some(Failure::UnboundProfile)
    );
}
#[test]
fn reference_minutes_are_derived_from_certified_epoch_differences() {
    let original = policy(false);
    let mut config = original.config().clone();
    config.clock = Some(ClockRule {
        profile: id(69),
        epochs: 10,
        minutes: 1,
    });
    let p = Policy::bind(config).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let mut s = ready(&p);
    open(&p, &mut s, 1, 2000 * Q, 1000 * Q);
    open(&p, &mut s, 2, 2000 * Q, 1000 * Q);
    advance(&p, &mut s, 9, 9);
    let mut r = raw(
        &s,
        1,
        AppCommand::Adjust {
            collateral: vaults::Delta::None,
            debt: vaults::Delta::Add(Q),
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(&p, &s, &mut r);
    let _ = step(&p, &mut s, r);
    assert_eq!(s.fee_state().last_epoch, 0);
    advance(&p, &mut s, 10, 10);
    let mut r = raw(
        &s,
        1,
        AppCommand::Adjust {
            collateral: vaults::Delta::None,
            debt: vaults::Delta::Add(Q),
            max_fee: Q,
            upper: None,
            lower: None,
        },
    );
    use_price(&p, &s, &mut r);
    let _ = step(&p, &mut s, r);
    assert_eq!(s.fee_state().last_epoch, 10);
    assert_eq!(s.clock().epoch, 10);
}
