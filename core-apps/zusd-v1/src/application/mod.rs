//! Step-4 composition, authenticated given ReceiptVerifierV1. No live backend
//! or owner-selected clock, drip, custody, exposure or change policy is supplied.
mod encoding;
#[cfg(feature = "std")]
pub mod publication;
pub mod receipts;
mod source_closure;
mod transition;
use crate::{Id, fees, ledger, liquidation, stability, staking, vaults};
use alloc::vec::Vec;
pub use receipts::{Domain, Proof, ReceiptVerifierV1, VerificationFailure};
pub use transition::{decide, genesis_messages, initialize, receipt_messages};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Clock {
    pub backend: Id,
    pub finality: Id,
    pub sync: Id,
    pub epoch: u64,
    pub watermark: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockRule {
    pub profile: Id,
    pub epochs: u64,
    pub minutes: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OracleRules {
    pub feed: Id,
    pub query: Id,
    pub profile: Id,
    pub scheme: Id,
    pub evaluator: Id,
    pub max_age: u64,
    pub max_lag: u64,
    pub max_deviation: u128,
    pub confirmation: u64,
    pub dispute_open: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PriceRecord {
    pub aggregate: Id,
    pub round: u64,
    pub published_epoch: u64,
    pub earliest_commit_epoch: u64,
    pub dispute_until: u64,
    pub value_e8: u128,
    pub observed_epoch: u64,
    pub median_seconds: u64,
    pub committed_epoch: u64,
    pub deviation: u128,
}
/// Derived oracle data. Possession of this public record grants NO authority.
/// ReceiptVerifierV1's Oracle clause must check exact extraction from 5e13a1d's
/// committed record, not merely a signature on an unrelated price projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceView {
    pub committed: Option<PriceRecord>,
    pub pending: Option<PriceRecord>,
    pub pending_blocked: bool,
    pub status: vaults::PriceStatus,
    pub capacity: u128,
    pub capacity_root: Id,
    pub aggregate_bytes: Vec<u8>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OracleAction {
    Use,
    ImportPending,
    CommitMatured,
    InitializeFirstMatured,
    RefreshStatus,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleReceipt {
    pub before: Id,
    pub after: Id,
    pub event: Option<Id>,
    pub action: OracleAction,
    pub view: PriceView,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Backing {
    pub custody: Id,
    pub funding: Id,
    pub supply_root: Id,
    pub agrs: u128,
    pub zdex: u128,
    pub zdex_supply: u128,
    pub zdex_burned: u128,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Allocation {
    pub root: Id,
    pub capacity: u128,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Drip {
    pub event: Id,
    pub amount: u128,
    pub next: Vec<u8>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChangePolicy {
    pub policy: Id,
    pub authority: Id,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bounds {
    pub vaults: u32,
    pub balances: u32,
    pub deposits: u32,
    pub front_ends: u32,
    pub stakers: u32,
    pub history: u32,
    pub epochs: u32,
    pub nonces: u32,
    pub seen: u32,
    pub events: u32,
    pub params: u32,
    pub frame_bytes: u64,
    pub history_bytes: u64,
    pub proof_bytes: u32,
    pub drip_bytes: u32,
    pub verification_steps: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExposurePolicy {
    pub window: u64,
    pub debt_ceiling: u128,
    pub liquidation_cap: u128,
    pub redemption_cap: u128,
}
/// Installation of a Config is a separate trusted adoption boundary. This
/// constructor validates a conditional profile; it cannot approve its owner policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub chain: Id,
    pub actor_registry: Id,
    pub clock_backend: Id,
    pub oracle_verifier: Id,
    pub custody_verifier: Id,
    pub funding_verifier: Id,
    pub allocation_verifier: Id,
    pub release_verifier: Id,
    pub drip_verifier: Id,
    pub clock: Option<ClockRule>,
    pub drip: Option<Id>,
    pub change: Option<ChangePolicy>,
    pub oracle: OracleRules,
    pub exposure: ExposurePolicy,
    pub bounds: Bounds,
    pub accounts: [Id; 9],
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    config: Config,
    id: Id,
}
impl Policy {
    pub fn bind(config: Config) -> Result<Self, crate::Failure> {
        use crate::Failure;
        if [
            config.chain,
            config.actor_registry,
            config.clock_backend,
            config.oracle_verifier,
            config.custody_verifier,
            config.funding_verifier,
            config.allocation_verifier,
            config.release_verifier,
            config.drip_verifier,
            config.oracle.feed,
            config.oracle.query,
            config.oracle.profile,
            config.oracle.scheme,
            config.oracle.evaluator,
        ]
        .contains(&[0; 32])
            || config.oracle.confirmation == 0
            || config.oracle.dispute_open == 0
        {
            return Err(Failure::UnboundProfile);
        }
        if config
            .accounts
            .iter()
            .enumerate()
            .any(|(i, a)| *a == [0; 32] || config.accounts[i + 1..].contains(a))
            || config.exposure.window == 0
            || config.bounds.frame_bytes == 0
            || usize::try_from(config.bounds.frame_bytes).is_err()
            || usize::try_from(config.bounds.history_bytes).is_err()
            || config.bounds.verification_steps == 0
            || config.bounds.history_bytes < config.bounds.frame_bytes
        {
            return Err(Failure::Domain);
        }
        if config
            .clock
            .is_some_and(|c| c.epochs == 0 || c.minutes == 0 || c.profile == [0; 32])
            || config
                .change
                .is_some_and(|c| c.policy == [0; 32] || c.authority == [0; 32])
        {
            return Err(Failure::Domain);
        }
        config
            .exposure
            .debt_ceiling
            .checked_add(config.exposure.liquidation_cap)
            .and_then(|v| v.checked_add(config.exposure.redemption_cap))
            .ok_or(Failure::Arithmetic)?;
        let id = encoding::hash(&encoding::config(&config));
        Ok(Self { config, id })
    }
    pub fn identity(&self) -> Id {
        self.id
    }
    pub fn config(&self) -> &Config {
        &self.config
    }
    pub(super) fn sp_bounds(&self) -> stability::Bounds {
        let b = self.config.bounds;
        stability::Bounds {
            history: b.history,
            epochs: b.epochs,
            deposits: b.deposits,
            front_ends: b.front_ends,
            vaults: b.vaults,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    Open {
        collateral: u128,
        principal: u128,
        max_fee: u128,
        upper: Option<Id>,
        lower: Option<Id>,
    },
    Adjust {
        collateral: vaults::Delta,
        debt: vaults::Delta,
        max_fee: u128,
        upper: Option<Id>,
        lower: Option<Id>,
    },
    Close,
    ClaimSurplus,
    Pool(stability::Command),
    MoveGain {
        upper: Option<Id>,
        lower: Option<Id>,
    },
    Stake(staking::Command),
    Liquidate(liquidation::Command),
    Redeem(crate::redemption::Command),
    ObserveOracle,
    AdvanceClock,
    Fund {
        event: Id,
        recipient: Id,
        quantity: ledger::Quantity,
    },
    ChangeParameters {
        policy: Id,
        version: u64,
        minimum_net: u128,
        reserve: u128,
    },
    ReleaseExposure {
        events: Vec<Id>,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub actor: Id,
    pub nonce: u64,
    pub command: Command,
    pub clock: Clock,
    pub oracle: Option<OracleReceipt>,
    pub backing: Backing,
    pub allocation: Option<Allocation>,
    pub drip: Option<Drip>,
    pub proofs: Vec<Proof>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Genesis {
    pub wallets: ledger::Sheet,
    pub funder: Id,
    pub clock: Clock,
    pub backing: Backing,
    pub proofs: Vec<Proof>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExposureClass {
    Liquidation,
    Redemption,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExposureEvent {
    pub id: Id,
    pub epoch: u64,
    pub class: ExposureClass,
    pub amount: u128,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    policy: Id,
    version: u64,
    deployed: u64,
    clock: Clock,
    backing: Backing,
    oracle_root: Id,
    oracle: Option<PriceView>,
    allocation_root: Id,
    book: liquidation::Book,
    pool: stability::Book,
    staking: staking::State,
    ledger: ledger::Sheet,
    fee: fees::State,
    params: Vec<vaults::Parameters>,
    nonces: Vec<(Id, u64)>,
    seen: Vec<Id>,
    events: Vec<ExposureEvent>,
    drip_state: Vec<u8>,
    last_trigger: Option<u64>,
}
impl State {
    pub fn root(&self) -> Id {
        encoding::hash(&encoding::state(self))
    }
    pub fn canonical_bytes(&self) -> Vec<u8> {
        encoding::state(self)
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn fee_state(&self) -> fees::State {
        self.fee
    }
    pub fn clock(&self) -> Clock {
        self.clock
    }
    pub fn backing(&self) -> Backing {
        self.backing
    }
    pub fn oracle_root(&self) -> Id {
        self.oracle_root
    }
    pub fn book(&self) -> &liquidation::Book {
        &self.book
    }
    pub fn pool(&self) -> &stability::Book {
        &self.pool
    }
    pub fn staking(&self) -> &staking::State {
        &self.staking
    }
    pub fn ledger(&self) -> &ledger::Sheet {
        &self.ledger
    }
    pub fn oracle(&self) -> Option<&PriceView> {
        self.oracle.as_ref()
    }
    pub fn exposure(&self) -> &[ExposureEvent] {
        &self.events
    }
    pub fn parameters(&self) -> &[vaults::Parameters] {
        &self.params
    }
    pub fn nonce(&self, actor: Id) -> u64 {
        self.nonces
            .binary_search_by_key(&actor, |r| r.0)
            .ok()
            .map(|i| self.nonces[i].1)
            .unwrap_or(0)
    }
}
/// Non-Clone, crate-constructed decision witness. Accessors expose shared data
/// only; the publication shell consumes it and independently replays its input.
#[derive(Debug)]
#[must_use]
pub struct Prepared {
    pre: Id,
    request: Request,
    post: State,
    legs: Vec<ledger::Movement>,
    trace: Vec<u8>,
    usage: crate::Usage,
    bytes: Vec<u8>,
}
impl Prepared {
    pub fn pre_root(&self) -> Id {
        self.pre
    }
    pub fn request(&self) -> &Request {
        &self.request
    }
    pub fn post(&self) -> &State {
        &self.post
    }
    pub fn movements(&self) -> &[ledger::Movement] {
        &self.legs
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn trace(&self) -> &[u8] {
        &self.trace
    }
    pub fn usage(&self) -> crate::Usage {
        self.usage
    }
}
pub fn vault_id(policy: &Policy, actor: Id) -> Id {
    let mut b = alloc::vec::Vec::from(&b"zusd.vault/1\0"[..]);
    b.extend_from_slice(&policy.id);
    b.extend_from_slice(&actor);
    encoding::hash(&b)
}

/// Source-bound step-4 evaluator identity; schema/serializer preparation only.
pub fn evaluator_identity() -> Id {
    encoding::core()
}
