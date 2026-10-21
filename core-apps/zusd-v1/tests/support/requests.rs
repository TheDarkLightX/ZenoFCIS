// TEST ONLY: shared synthetic state builders, with an explicit fixture issuer.
// These facts do not establish a live oracle/finality/custody graph.
use zeno_fcis_codec::{CommitmentHasher, RustCryptoSha256};
use zenofcis_zusd_v1::{
    Failure, Limits, Q,
    application::*,
    ledger::{self, Quantity},
    vaults,
};
pub fn id(n: u8) -> [u8; 32] {
    let mut b = [0; 32];
    b[31] = n;
    b
}
pub fn limits() -> Limits {
    Limits {
        steps: 500_000_000,
        reads: 100_000_000,
        bytes: 500_000_000,
    }
}
pub fn hash(b: &[u8]) -> [u8; 32] {
    *RustCryptoSha256::hash(b).as_bytes()
}
pub(crate) use super::issuer::{TestVerifier, issue};
pub fn policy(change: bool) -> Policy {
    Policy::bind(Config {
        chain: id(60),
        actor_registry: id(61),
        clock_backend: id(62),
        oracle_verifier: id(63),
        custody_verifier: id(64),
        funding_verifier: id(65),
        allocation_verifier: id(66),
        release_verifier: id(67),
        drip_verifier: id(68),
        clock: Some(ClockRule {
            profile: id(69),
            epochs: 1,
            minutes: 1,
        }),
        drip: Some(id(70)),
        change: change.then_some(ChangePolicy {
            policy: id(71),
            authority: id(72),
        }),
        oracle: OracleRules {
            feed: id(73),
            query: id(74),
            profile: id(75),
            scheme: id(76),
            evaluator: id(77),
            max_age: 50_000,
            max_lag: 50_000,
            max_deviation: 100,
            confirmation: 2,
            dispute_open: 2,
        },
        exposure: ExposurePolicy {
            window: 10,
            debt_ceiling: 100_000 * Q,
            liquidation_cap: 100_000 * Q,
            redemption_cap: 100_000 * Q,
        },
        bounds: Bounds {
            vaults: 8,
            balances: 128,
            deposits: 8,
            front_ends: 8,
            stakers: 8,
            history: 32,
            epochs: 16,
            nonces: 32,
            seen: 128,
            events: 32,
            params: 8,
            frame_bytes: 131_072,
            history_bytes: 32_000_000,
            proof_bytes: 256,
            drip_bytes: 256,
            verification_steps: 10_000,
        },
        accounts: core::array::from_fn(|i| id(90 + i as u8)),
    })
    .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
}
pub fn genesis(p: &Policy) -> State {
    let mut balances = Vec::new();
    for user in 1..=6 {
        balances.push(ledger::Balance {
            account: id(user),
            quantity: Quantity::Agrs(20_000 * Q),
        });
        balances.push(ledger::Balance {
            account: id(user),
            quantity: Quantity::Zdex(1000 * Q),
        })
    }
    balances.push(ledger::Balance {
        account: id(9),
        quantity: Quantity::Zdex(zenofcis_zusd_v1::stability::FUNDED_CAP),
    });
    let mut g = Genesis {
        wallets: ledger::Sheet {
            balances,
            supply: 0,
            active_debt: 0,
            default_debt: 0,
        },
        funder: id(9),
        clock: Clock {
            backend: p.config().clock_backend,
            finality: id(80),
            sync: id(81),
            epoch: 0,
            watermark: 0,
        },
        backing: Backing {
            custody: id(82),
            funding: id(83),
            supply_root: id(84),
            agrs: 120_000 * Q,
            zdex: zenofcis_zusd_v1::stability::FUNDED_CAP + 6000 * Q,
            zdex_supply: 100_000_000 * Q,
            zdex_burned: 0,
        },
        proofs: vec![],
    };
    g.proofs = genesis_messages(p, &g)
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
        .into_iter()
        .map(|m| Proof {
            domain: m.domain,
            bytes: issue(&m.bytes),
        })
        .collect();
    initialize(p, &g, &TestVerifier, limits())
        .into_parts()
        .0
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
}
pub fn raw(s: &State, actor: u8, command: Command) -> Request {
    Request {
        actor: id(actor),
        nonce: s.nonce(id(actor)),
        command,
        clock: s.clock(),
        oracle: None,
        backing: s.backing(),
        allocation: None,
        drip: None,
        proofs: vec![],
    }
}
pub fn sign(p: &Policy, s: &State, mut r: Request) -> Result<Request, Failure> {
    r.proofs = receipt_messages(p, s, &r)?
        .into_iter()
        .map(|m| Proof {
            domain: m.domain,
            bytes: issue(&m.bytes),
        })
        .collect();
    Ok(r)
}
pub fn use_price(p: &Policy, s: &State, r: &mut Request) {
    let view = s
        .oracle()
        .unwrap_or_else(|| panic!("missing test fixture value"))
        .clone();
    r.allocation = Some(Allocation {
        root: view.capacity_root,
        capacity: view.capacity,
    });
    r.oracle = Some(OracleReceipt {
        before: s.oracle_root(),
        after: s.oracle_root(),
        event: None,
        action: OracleAction::Use,
        view,
    });
    assert!(p.config().oracle.max_age > 0)
}
pub fn drip(r: &mut Request, event: u8, amount: u128) {
    r.drip = Some(Drip {
        event: id(event),
        amount,
        next: vec![event],
    })
}

pub fn advance_request(_p: &Policy, s: &State, epoch: u64, watermark: u64) -> Request {
    let mut r = raw(s, 6, Command::AdvanceClock);
    r.clock.epoch = epoch;
    r.clock.watermark = watermark;
    r.clock.finality = hash(&epoch.to_be_bytes());
    r.clock.sync = hash(&watermark.to_be_bytes());
    r
}

pub fn price_request(
    _p: &Policy,
    s: &State,
    action: OracleAction,
    round: u64,
    value: u128,
    event: u8,
) -> Request {
    let old = s.oracle().cloned();
    let mut record = PriceRecord {
        aggregate: id(event),
        round,
        published_epoch: s.clock().epoch,
        earliest_commit_epoch: s.clock().epoch + 2,
        dispute_until: s.clock().epoch + 2,
        value_e8: value,
        observed_epoch: s.clock().epoch,
        median_seconds: s.clock().watermark,
        committed_epoch: 0,
        deviation: 0,
    };
    let (committed, pending) = match action {
        OracleAction::InitializeFirstMatured => {
            record.published_epoch = s.clock().epoch - 2;
            record.earliest_commit_epoch = s.clock().epoch;
            record.dispute_until = s.clock().epoch;
            record.committed_epoch = s.clock().epoch;
            (Some(record), None)
        }
        OracleAction::ImportPending => (old.as_ref().and_then(|v| v.committed), Some(record)),
        OracleAction::CommitMatured => {
            let mut rec = old
                .as_ref()
                .unwrap_or_else(|| panic!("missing test fixture value"))
                .pending
                .unwrap_or_else(|| panic!("missing test fixture value"));
            rec.committed_epoch = s.clock().epoch;
            (Some(rec), None)
        }
        _ => panic!(),
    };
    let view = PriceView {
        committed,
        pending,
        pending_blocked: false,
        status: if action == OracleAction::ImportPending
            && old
                .as_ref()
                .is_some_and(|v| v.status == vaults::PriceStatus::Disputed)
        {
            vaults::PriceStatus::Disputed
        } else {
            vaults::PriceStatus::Usable
        },
        capacity: 1_000_000 * Q,
        capacity_root: id(85),
        aggregate_bytes: vec![event],
    };
    let mut r = raw(s, 6, Command::ObserveOracle);
    r.oracle = Some(OracleReceipt {
        before: s.oracle_root(),
        after: hash(&[event, round as u8]),
        event: Some(id(event)),
        action,
        view,
    });
    r
}

pub fn balance(s: &State, who: u8, asset: ledger::Asset) -> u128 {
    s.ledger()
        .balances
        .iter()
        .find(|r| r.account == id(who) && r.quantity.asset() == asset)
        .map_or(0, |r| r.quantity.atoms())
}
