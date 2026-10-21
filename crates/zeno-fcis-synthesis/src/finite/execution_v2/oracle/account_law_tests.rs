//! Normative guard/conflict witnesses and complete finite accounting domains.
use super::*;
use alloc::{collections::BTreeMap, vec};

pub(super) fn id(n: u8) -> Id {
    Id::new(&[n]).unwrap_or_else(|_| panic!("ID"))
}
pub(super) fn limits() -> Limits {
    [
        Resource::Read,
        Resource::Write,
        Resource::Step,
        Resource::Byte,
        Resource::WitnessByte,
    ]
    .into_iter()
    .fold(zero_limits(), |l, r| l.with_limit(r, 10_000_000))
}
pub(super) fn params() -> Parameters {
    super::lifecycle_tests::profile().parameters
}
pub(super) fn clock(epoch: u64) -> ClockView {
    ClockView {
        backend: [5; 32],
        finality_root: [9; 32],
        epoch,
    }
}
pub(super) fn aggregate_fixture(feed: Hash) -> Aggregate {
    let p = params();
    let c = p.clock.unwrap_or_else(|| panic!("clock"));
    let sources = (1..=3)
        .map(|i| Source {
            id: id(i),
            operator: id(i),
            venue: id(i),
            family: id(i),
            transport: id(i),
            jurisdiction: id(i),
        })
        .collect::<alloc::vec::Vec<_>>();
    let observations = (1..=3)
        .map(|i| Observation {
            id: [i; 32],
            admission: [i + 10; 32],
            reporter: id(i),
            source: id(i),
            feed,
            profile: [2; 32],
            query: [3; 32],
            round: 0,
            value: PriceE8::new(100).unwrap_or_else(|_| panic!("price")),
            epoch: 0,
            seconds: 0,
        })
        .collect::<alloc::vec::Vec<_>>();
    let rules = FeedRules {
        feed,
        profile: [2; 32],
        query: [3; 32],
        diversity: Diversity {
            minimum: [3; 5],
            max_same_class: [1; 5],
        },
        max_deviation_bps: 10000,
        clock: c,
        bounds: super::lifecycle_tests::profile().bounds,
        frame_bytes: 2_000_000,
    };
    aggregate(&rules, 0, 0, 0, &observations, &sources, limits())
        .into_parts()
        .0
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|e| panic!("{e:?}"))
}
pub(super) fn bond() -> BondState {
    BondState {
        reporter: id(1),
        owner: id(1),
        phase: ReporterPhase::Active,
        held: ZdexE18(1),
        liabilities: vec![Liability {
            id: [4; 32],
            report: [1; 32],
            feed: [1; 32],
            locked: ZdexE18(1),
            publication: Some(0),
            exposure_until: 8,
            clear_after: 8,
            cleared: false,
        }],
        disputes: vec![],
        wallets: BTreeMap::from([(id(1), ZdexE18(2)), (id(2), ZdexE18(2))]),
        nonces: BTreeMap::new(),
        burned: ZdexE18(0),
        exit_epoch: None,
        epoch: 0,
    }
}
pub(super) fn rules() -> RewardRules {
    RewardRules {
        feed: [1; 32],
        profile: [2; 32],
        unit: RewardUnit {
            asset: id(9),
            scale: 18,
        },
        per_report: 1,
        reporter_share: 1,
        treasury_share: 0,
        burn_share: 0,
        denominator: 1,
        treasury: id(8),
    }
}
pub(super) fn reward() -> RewardState {
    RewardState {
        feed: rules().feed,
        profile: rules().profile,
        unit: rules().unit,
        funded: 1,
        remaining: 1,
        paid: 0,
        treasury_paid: 0,
        burned: 0,
        residual: 0,
        wallets: BTreeMap::new(),
        claims: vec![],
        nonces: BTreeMap::new(),
        epoch: 0,
    }
}
fn accrued() -> RewardState {
    reward_relation(
        &rules(),
        AccountContext {
            parameters: &params(),
            actor: &id(1),
            nonce: 0,
            clock: clock(0),
            capacity: 16,
        },
        &reward(),
        &bond(),
        &RewardCommand::Accrue {
            aggregate: (aggregate_fixture([1; 32])).into(),
            report: [1; 32],
        },
        limits(),
    )
    .into_parts()
    .0
    .unwrap_or_else(|e| panic!("{e:?}"))
    .unwrap_or_else(|e| panic!("{e:?}"))
    .into_state()
}

#[test]
fn oracle_ordered_publication_wrong_phase_precedes_binding_mismatch() {
    let c = BondCommand::RecordPublication {
        liability: [4; 32],
        aggregate: (aggregate_fixture([9; 32])).into(),
    };
    let result = bond_relation(
        AccountContext {
            parameters: &params(),
            actor: &id(1),
            nonce: 0,
            clock: clock(0),
            capacity: 16,
        },
        &bond(),
        &c,
        limits(),
    );
    assert!(
        matches!(result.result(), Ok(Err(Reason::WrongPhase))),
        "{:?}",
        result.result()
    );
}

#[test]
fn oracle_ordered_reward_budget_precedes_duplicate_entitlement() {
    let s = accrued();
    let c = RewardCommand::Accrue {
        aggregate: (aggregate_fixture([1; 32])).into(),
        report: [1; 32],
    };
    let result = reward_relation(
        &rules(),
        AccountContext {
            parameters: &params(),
            actor: &id(1),
            nonce: 1,
            clock: clock(0),
            capacity: 16,
        },
        &s,
        &bond(),
        &c,
        limits(),
    );
    assert!(
        matches!(result.result(), Ok(Err(Reason::InsufficientRewardBudget))),
        "{:?}",
        result.result()
    );
}

#[test]
fn oracle_ordered_clawback_can_precede_resolution_of_an_upheld_slash() {
    let p = params();
    let b = bond();
    let open = bond_relation(
        AccountContext {
            parameters: &p,
            actor: &id(2),
            nonce: 0,
            clock: clock(0),
            capacity: 16,
        },
        &b,
        &BondCommand::OpenDispute {
            id: [5; 32],
            liability: [4; 32],
        },
        limits(),
    )
    .into_parts()
    .0
    .unwrap_or_else(|e| panic!("{e:?}"))
    .unwrap_or_else(|e| panic!("{e:?}"))
    .into_state();
    let verdict = FinalVerdict {
        adjudicator: [6; 32],
        dispute: [5; 32],
        violation: [6; 32],
        upheld: true,
        final_epoch: 0,
        finality_root: [9; 32],
    };
    let slashed = bond_relation(
        AccountContext {
            parameters: &p,
            actor: &id(2),
            nonce: 1,
            clock: clock(0),
            capacity: 16,
        },
        &open,
        &BondCommand::Slash {
            dispute: [5; 32],
            verdict,
        },
        limits(),
    )
    .into_parts()
    .0
    .unwrap_or_else(|e| panic!("{e:?}"))
    .unwrap_or_else(|e| panic!("{e:?}"))
    .into_state();
    assert_eq!(slashed.disputes[0].phase, DisputePhase::Open);
    let s = accrued();
    let claim = s.claims[0].id;
    let result = reward_relation(
        &rules(),
        AccountContext {
            parameters: &p,
            actor: &id(2),
            nonce: 0,
            clock: clock(0),
            capacity: 16,
        },
        &s,
        &slashed,
        &RewardCommand::Clawback(claim),
        limits(),
    );
    assert!(
        matches!(result.result(), Ok(Ok(_))),
        "{:?}",
        result.result()
    );
}
