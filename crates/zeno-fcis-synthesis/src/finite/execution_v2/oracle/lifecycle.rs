//! Owned feed/key/report/round lifecycle. Clock and actor acquisition remain
//! explicit composition premises. Native signatures and all lifecycle joins
//! are recomputed here; the resulting candidate has no publication authority.
use super::aggregation::{Aggregate, FeedRules, TimeQuorum};
use super::bonds::AccountContext;
use super::bonds::{BondCommand, BondState, ReporterPhase};
use super::prices::{ClockView, PriceState};
use super::reports::{KeyRegistration, RawReport, ReportChain};
use super::rewards::RewardState;
use super::types::{
    BLS_POP_SCHEME, Hash, Id, Observation, Parameters, Reason, Source, StressE8, ZdexE18,
};
use super::verification::BlsVerifier;
use super::wire;
use super::{Evaluation, Failure, Limits, Meter, Resource, measured, step};
use alloc::{collections::BTreeMap, vec::Vec};
use zeno_fcis_codec::CommitmentHasher;

/// Explicit fractions for reporter, treasury and burned rewards.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardSplit {
    /// Numerator of the reporter reward share.
    pub reporter: u128,
    /// Numerator of the treasury reward share.
    pub treasury: u128,
    /// Numerator of the burned reward share.
    pub burn: u128,
    /// Common denominator for reward shares.
    pub denominator: u128,
    /// Treasury account identity.
    pub treasury_account: Id,
}
/// Bound native oracle policy and canonical resource limits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleProfile {
    /// Nominal zDEX bond asset identity.
    pub zdex_asset: Id,
    /// Explicit reward recipient fractions.
    pub reward_split: RewardSplit,
    /// Explicit oracle policy bindings.
    pub parameters: Parameters,
    /// Canonical collection size limits.
    pub bounds: super::CollectionLimits,
    /// Maximum bytes in one framed item.
    pub frame_bytes: u64,
}

/// Feed identities, source assignments and round schedule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedRegistration {
    /// Monotone registration version.
    pub version: u64,
    /// Base asset identity.
    pub base: Id,
    /// Quote asset identity.
    pub quote: Id,
    /// Bound rules for this feed.
    pub rules: FeedRules,
    /// Registered sources for this feed.
    pub sources: Vec<Source>,
    /// Reporter to source assignments for this feed.
    pub assignments: BTreeMap<Id, Id>,
    /// Opening epoch of the first round.
    pub first_open: u64,
    /// Epochs from round opening to its cutoff.
    pub cutoff_delta: u64,
}

/// Registered key and immutable authorization history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedKey {
    /// Registered key and its possession proof.
    pub key: KeyRegistration,
    /// Epoch at which this key became active.
    pub activated: u64,
    /// Epoch at which this key was retired, if any.
    pub retired: Option<u64>,
    /// First report sequence admitted under this key.
    pub activation_sequence: u64,
    /// Exclusive last sequence boundary for a retired key.
    pub retirement_sequence: Option<u64>,
    /// State root bound into key registration.
    pub registration_root: Hash,
    /// Actor nonce bound into key registration.
    pub registration_nonce: u64,
    /// Previous key version for a rotation, if any.
    pub old_version: Option<u64>,
    /// New key signature over the registration or rotation message.
    pub authorization: [u8; 96],
    /// Previous key signature over the rotation message, if any.
    pub old_authorization: Option<[u8; 96]>,
}

/// Reporter ownership, active key and report-chain cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReporterRecord {
    /// Account that owns the reporter bond.
    pub owner: Id,
    /// Currently active reporter key version.
    pub active_version: u64,
    /// Epoch at which the reporter registered.
    pub registered: u64,
    /// Chain identity bound into signed reports.
    pub chain: ReportChain,
}

/// Round cutoff, frozen membership and aggregate provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoundRecord {
    /// Epoch at which this round opened.
    pub opening: u64,
    /// Exclusive report admission cutoff epoch.
    pub cutoff: u64,
    /// Reporter identities frozen when the round opened.
    pub eligible: Vec<Id>,
    /// Aggregate selected for this operation.
    pub aggregate: Option<Hash>,
    /// Digest of the complete cutoff admission set.
    pub cutoff_admissions: Option<Hash>,
    /// Epoch at which the aggregate was built, if any.
    pub built_epoch: Option<u64>,
}

/// Verified report with its admission and bond reservation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedReport {
    /// Canonical report identity.
    pub report: RawReport,
    /// Epoch at which this report was admitted.
    pub submitted: u64,
    /// Canonical admission witness identity.
    pub admission: Hash,
    /// Bond liability identity associated with this record.
    pub liability: Hash,
}

/// Source-bound conservative valuation evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StressObservation {
    /// Source identity.
    pub source: Hash,
    /// Authenticated stress valuation evidence root.
    pub valuation_root: Hash,
    /// Positive price or stress observation in E8 units.
    pub value: Option<StressE8>,
    /// Logical epoch of the observation.
    pub observed_epoch: u64,
}

/// Construction and ingress own every reachable value. Private fields prevent
/// caller edits to a retained checked snapshot. Decode rechecks its full state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleState {
    pub(super) chain: Id,
    pub(super) profile: LifecycleProfile,
    pub(super) profile_id: Hash,
    pub(super) epoch: u64,
    pub(super) nonces: BTreeMap<Id, u64>,
    pub(super) wallets: BTreeMap<Id, ZdexE18>,
    pub(super) feeds: BTreeMap<Hash, FeedRegistration>,
    pub(super) keys: BTreeMap<(Id, u64), RetainedKey>,
    pub(super) reporters: BTreeMap<Id, ReporterRecord>,
    // Wallets/nonces of retained bond accounts are empty: they are projected
    // from the single shared maps above for one checked invocation only.
    pub(super) bonds: BTreeMap<Id, BondState>,
    pub(super) rounds: BTreeMap<(Hash, u64), RoundRecord>,
    pub(super) reports: BTreeMap<Hash, AdmittedReport>,
    pub(super) aggregates: BTreeMap<Hash, Aggregate>,
    pub(super) rewards: BTreeMap<Hash, RewardState>,
    pub(super) reward_wallets: BTreeMap<Id, u128>,
    pub(super) prices: BTreeMap<Hash, PriceState>,
    pub(super) time_quorums: BTreeMap<Hash, TimeQuorum>,
    pub(super) stress: BTreeMap<Hash, StressObservation>,
}

/// Exact pre-state, actor nonce and authenticated clock premise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifecycleContext {
    /// Canonical state root required before this command.
    pub pre_root: Hash,
    /// Replay counter before the command.
    pub nonce: u64,
    /// Authenticated clock premise for this invocation.
    pub clock: ClockView,
}

/// Native reward request referring to retained report evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeRewardCommand {
    /// Add funds to the denomination-bound reward pool.
    Fund(u128),
    /// Reserve a reward for one selected report.
    Accrue {
        /// Aggregate selected for this operation.
        aggregate: Hash,
        /// Canonical report identity.
        report: Hash,
    },
    /// Pay a mature unslashed reward claim.
    Claim(Hash),
    /// Reclaim a held reward associated with a slashed report.
    Clawback(Hash),
}
/// Native price request referring to retained quorum evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativePriceCommand {
    /// Buffer a complete admitted price aggregate.
    Stage {
        /// Aggregate selected for this operation.
        aggregate: Hash,
    },
    /// Commit the buffered aggregate after its full delay.
    Commit,
    /// Advance the watermark from a complete time-only quorum.
    SyncTime {
        /// Logical round identifier.
        round: u64,
    },
    /// Advance logical time without changing market evidence.
    AdvanceEpoch,
}
/// Closed set of native oracle transition requests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleCommand {
    /// Evaluate a native reward-account command.
    Reward {
        /// Canonical feed identity.
        feed: Hash,
        /// Command evaluated against the immutable pre-state.
        command: NativeRewardCommand,
    },
    /// Register a canonical feed, source set and schedule.
    RegisterFeed(FeedRegistration),
    /// Evaluate a native buffered-price command.
    Price {
        /// Canonical feed identity.
        feed: Hash,
        /// Command evaluated against the immutable pre-state.
        command: NativePriceCommand,
    },
    /// Admit a reporter key after authorization and possession checks.
    RegisterKey {
        /// Registered key and its possession proof.
        key: KeyRegistration,
        /// New key signature over the registration or rotation message.
        authorization: [u8; 96],
    },
    /// Replace an active key using both authorizations and possession checks.
    RotateKey {
        /// Registered key and its possession proof.
        key: KeyRegistration,
        /// New key signature over the registration or rotation message.
        authorization: [u8; 96],
        /// Previous key signature over the rotation message, if any.
        old_authorization: [u8; 96],
    },
    /// Freeze round membership at its scheduled opening.
    OpenRound {
        /// Canonical feed identity.
        feed: Hash,
        /// Logical round identifier.
        round: u64,
    },
    /// Admit a signed report and reserve its liability.
    SubmitReport(RawReport),
    /// Build the complete cutoff quorum from retained admissions.
    BuildAggregate {
        /// Canonical feed identity.
        feed: Hash,
        /// Logical round identifier.
        round: u64,
    },
    /// Evaluate a native bonded-account command.
    Bond {
        /// Reporter identity.
        reporter: Id,
        /// Command evaluated against the immutable pre-state.
        command: BondCommand,
    },
    /// Install source-bound stress valuation evidence.
    UpdateStress {
        /// Canonical feed identity.
        feed: Hash,
        /// Source-bound stress valuation observation.
        observation: StressObservation,
    },
}

/// Owned native successor with exact pre-root, post-root and command binding.
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecyclePlan {
    state: LifecycleState,
    pre_root: Hash,
    command_hash: Hash,
    post_root: Hash,
}
impl LifecyclePlan {
    /// Borrow the immutable checked successor.
    pub fn state(&self) -> &LifecycleState {
        &self.state
    }
    /// Consume the plan and return its owned successor.
    pub fn into_state(self) -> LifecycleState {
        self.state
    }
    /// Return the canonical source root required before the command.
    pub fn pre_root(&self) -> Hash {
        self.pre_root
    }
    /// Return the canonical command digest.
    pub fn command_hash(&self) -> Hash {
        self.command_hash
    }
    /// Return the checked successor root.
    pub fn post_root(&self) -> Hash {
        self.post_root
    }
}

impl LifecycleProfile {
    /// Return the canonical identity.
    pub fn id(&self) -> Result<Hash, Failure> {
        self.parameters.validate()?;
        let split = &self.reward_split;
        let e = self
            .parameters
            .economy
            .as_ref()
            .ok_or(Failure::UnboundParameter)?;
        if split.denominator == 0
            || split
                .reporter
                .checked_add(split.treasury)
                .and_then(|n| n.checked_add(split.burn))
                .is_none_or(|n| n > split.denominator)
            || e.reward_unit.asset == self.zdex_asset && e.reward_unit.scale != 18
        {
            return Err(Failure::InvalidProfile);
        }

        if self.frame_bytes < 48 {
            return Err(Failure::InvalidProfile);
        }
        wire::hash(
            b"zeno.oracle.native_profile/1\0",
            &super::lifecycle_schema::profile_value(self)?,
        )
    }
}
impl FeedRegistration {
    /// Return the canonical identity.
    pub fn id(&self) -> Result<Hash, Failure> {
        wire::hash(
            b"zeno.oracle.feed/1\0",
            &super::lifecycle_schema::feed_value(self, false)?,
        )
    }
    /// Bind the feed identity to its canonical registration contents.
    pub fn with_bound_id(mut self) -> Result<Self, Failure> {
        self.rules.feed = self.id()?;
        Ok(self)
    }
}
impl LifecycleState {
    /// Initial wallets are an explicit accounting prestate, not a funding or
    /// custody receipt. Balance must establish that prestate during composition.
    pub fn new(
        chain: Id,
        profile: LifecycleProfile,
        wallets: BTreeMap<Id, ZdexE18>,
    ) -> Result<Self, Failure> {
        let profile_id = profile.id()?;
        let state = Self {
            chain,
            profile,
            profile_id,
            epoch: 0,
            nonces: BTreeMap::new(),
            wallets,
            feeds: BTreeMap::new(),
            keys: BTreeMap::new(),
            reporters: BTreeMap::new(),
            bonds: BTreeMap::new(),
            rounds: BTreeMap::new(),
            reports: BTreeMap::new(),
            aggregates: BTreeMap::new(),
            stress: BTreeMap::new(),
            rewards: BTreeMap::new(),
            reward_wallets: BTreeMap::new(),
            prices: BTreeMap::new(),
            time_quorums: BTreeMap::new(),
        };
        super::lifecycle_schema::fit(&state)?.map_err(|_| Failure::InvalidState)?;
        Ok(state)
    }
    /// Explicit foreign reward-asset prestate. When rewards are zDEX, the single
    /// zDEX map is authoritative and a second wallet map is forbidden.
    pub fn with_reward_wallets(mut self, wallets: BTreeMap<Id, u128>) -> Result<Self, Failure> {
        if !self.feeds.is_empty()
            || !self.nonces.is_empty()
            || self
                .profile
                .parameters
                .economy
                .as_ref()
                .ok_or(Failure::UnboundParameter)?
                .reward_unit
                .asset
                == self.profile.zdex_asset
        {
            return Err(Failure::InvalidState);
        }
        self.reward_wallets = wallets;
        super::lifecycle_schema::fit(&self)?.map_err(|_| Failure::InvalidState)?;
        Ok(self)
    }
    /// Borrow the retained reward pool for a feed.
    pub fn reward(&self, feed: &Hash) -> Option<&RewardState> {
        self.rewards.get(feed)
    }
    /// Return the actor’s available reward balance.
    pub fn reward_wallet(&self, actor: &Id) -> u128 {
        if self
            .profile
            .parameters
            .economy
            .as_ref()
            .is_some_and(|e| e.reward_unit.asset == self.profile.zdex_asset)
        {
            self.wallet(actor).0
        } else {
            self.reward_wallets.get(actor).copied().unwrap_or(0)
        }
    }
    /// Return the canonical native profile identity.
    pub fn profile_id(&self) -> Hash {
        self.profile_id
    }
    /// Borrow the immutable bound native profile.
    pub fn profile(&self) -> &LifecycleProfile {
        &self.profile
    }
    /// Borrow the bound chain identity.
    pub fn chain(&self) -> &Id {
        &self.chain
    }
    /// Return the logical epoch.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    /// Return the next nonce for this actor.
    pub fn nonce(&self, actor: &Id) -> u64 {
        self.nonces.get(actor).copied().unwrap_or(0)
    }
    /// Borrow a retained feed registration by its canonical identity.
    pub fn feed(&self, id: &Hash) -> Option<&FeedRegistration> {
        self.feeds.get(id)
    }
    /// Borrow a retained reporter record.
    pub fn reporter(&self, id: &Id) -> Option<&ReporterRecord> {
        self.reporters.get(id)
    }
    /// Borrow a retained reporter key version.
    pub fn key(&self, reporter: &Id, version: u64) -> Option<&RetainedKey> {
        self.keys.get(&(reporter.clone(), version))
    }
    /// Borrow a reporter’s bonded account.
    pub fn bond(&self, reporter: &Id) -> Option<&BondState> {
        self.bonds.get(reporter)
    }
    /// Borrow an admitted report by its canonical identity.
    pub fn report(&self, id: &Hash) -> Option<&AdmittedReport> {
        self.reports.get(id)
    }
    /// Borrow the retained record for the given feed and round.
    pub fn round(&self, feed: Hash, round: u64) -> Option<&RoundRecord> {
        self.rounds.get(&(feed, round))
    }
    /// Borrow a retained quorum aggregate by its canonical identity.
    pub fn aggregate(&self, id: &Hash) -> Option<&Aggregate> {
        self.aggregates.get(id)
    }
    /// Borrow a feed’s committed and buffered price state.
    pub fn price(&self, feed: &Hash) -> Option<&PriceState> {
        self.prices.get(feed)
    }
    /// Borrow a retained time-only quorum.
    pub fn time_quorum(&self, feed: &Hash) -> Option<&TimeQuorum> {
        self.time_quorums.get(feed)
    }
    /// Borrow a feed’s retained stress observation.
    pub fn stress(&self, feed: &Hash) -> Option<&StressObservation> {
        self.stress.get(feed)
    }
    /// Return the actor’s available zDEX balance.
    pub fn wallet(&self, actor: &Id) -> ZdexE18 {
        self.wallets.get(actor).copied().unwrap_or_default()
    }
    /// Encode this value in its canonical protocol format.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Failure> {
        wire::bytes(&super::lifecycle_schema::state_value(self)?)
    }
    /// Return the canonical state root.
    pub fn root(&self) -> Result<Hash, Failure> {
        wire::hash(
            b"zeno.oracle.state/1\0",
            &super::lifecycle_schema::state_value(self)?,
        )
    }
}

/// Construct the exact new-key authorization message for registration.
pub fn registration_message(
    state: &LifecycleState,
    root: Hash,
    nonce: u64,
    key: &KeyRegistration,
) -> Result<Hash, Failure> {
    signing_message(state, root, nonce, None, key)
}
/// Construct the exact message signed by both keys during rotation.
pub fn rotation_message(
    state: &LifecycleState,
    root: Hash,
    nonce: u64,
    old_version: u64,
    key: &KeyRegistration,
) -> Result<Hash, Failure> {
    signing_message(state, root, nonce, Some(old_version), key)
}
pub(super) fn signing_message(
    state: &LifecycleState,
    root: Hash,
    nonce: u64,
    old: Option<u64>,
    key: &KeyRegistration,
) -> Result<Hash, Failure> {
    let value = wire::record(alloc::vec![
        wire::blob(state.chain.bytes())?,
        wire::blob(&state.profile_id)?,
        wire::blob(&root)?,
        wire::integers(&[u128::from(nonce), u128::from(key.version)])?,
        super::lifecycle_schema::maybe_key_version(old)?,
        wire::blob(key.reporter.bytes())?,
        wire::blob(key.owner.bytes())?,
        wire::blob(BLS_POP_SCHEME.as_bytes())?,
        wire::blob(&key.public_key)?,
        wire::blob(&key.possession)?
    ])?;
    wire::hash(
        if old.is_some() {
            b"zeno.oracle.rotate/1\0"
        } else {
            b"zeno.oracle.register/1\0"
        },
        &value,
    )
}

/// Evaluate the native command and check its complete independent outcome laws.
pub fn lifecycle_relation(
    verifier: &impl BlsVerifier,
    state: &LifecycleState,
    actor: &Id,
    context: LifecycleContext,
    command: &LifecycleCommand,
    limits: Limits,
) -> Evaluation<Result<LifecyclePlan, Reason>> {
    measured(limits, |m| {
        let result = run(verifier, state, actor, context, command, m)?;
        super::lifecycle_laws::check(
            verifier,
            state,
            actor,
            context,
            command,
            result.as_ref().map(|p| p.state()).map_err(|r| *r),
            m,
        )?;
        Ok(result)
    })
}

pub(super) fn row_count(s: &LifecycleState) -> Result<usize, Failure> {
    [
        s.nonces.len(),
        s.wallets.len(),
        s.feeds.len(),
        s.keys.len(),
        s.reporters.len(),
        s.bonds.len(),
        s.rounds.len(),
        s.reports.len(),
        s.aggregates.len(),
        s.rewards.len(),
        s.reward_wallets.len(),
        s.stress.len(),
        s.prices.len(),
        s.time_quorums.len(),
    ]
    .into_iter()
    .try_fold(0usize, |a, b| a.checked_add(b).ok_or(Failure::Arithmetic))
    .and_then(|n| {
        s.bonds.values().try_fold(n, |a, b| {
            a.checked_add(b.liabilities.len())
                .and_then(|v| v.checked_add(b.disputes.len()))
                .ok_or(Failure::Arithmetic)
        })
    })
    .and_then(|n| {
        s.feeds.values().try_fold(n, |a, f| {
            a.checked_add(f.sources.len())
                .and_then(|v| v.checked_add(f.assignments.len()))
                .ok_or(Failure::Arithmetic)
        })
    })
    .and_then(|n| {
        s.rounds.values().try_fold(n, |a, r| {
            a.checked_add(r.eligible.len()).ok_or(Failure::Arithmetic)
        })
    })
    .and_then(|n| {
        s.prices.values().try_fold(n, |n, p| {
            n.checked_add(p.history.len()).ok_or(Failure::Arithmetic)
        })
    })
    .and_then(|n| {
        s.rewards.values().try_fold(n, |n, r| {
            n.checked_add(r.claims.len()).ok_or(Failure::Arithmetic)
        })
    })
}

pub(super) fn charged_row_count(s: &LifecycleState, m: &mut Meter) -> Result<usize, Failure> {
    let visits = [
        s.bonds.len(),
        s.feeds.len(),
        s.rounds.len(),
        s.prices.len(),
        s.rewards.len(),
    ]
    .into_iter()
    .try_fold(0u64, |a, n| {
        a.checked_add(u64::try_from(n).map_err(|_| Failure::Arithmetic)?)
            .ok_or(Failure::Arithmetic)
    })?;
    m.charge(Resource::Read, visits)?;
    m.charge(Resource::Step, visits)?;
    row_count(s)
}

pub(super) fn run(
    verifier: &impl BlsVerifier,
    s: &LifecycleState,
    actor: &Id,
    ctx: LifecycleContext,
    c: &LifecycleCommand,
    m: &mut Meter,
) -> Result<Result<LifecyclePlan, Reason>, Failure> {
    s.profile.parameters.validate()?;
    let command_rows = if let LifecycleCommand::RegisterFeed(f) = c {
        if f.sources.len() > s.profile.bounds.max_entries as usize
            || f.assignments.len() > s.profile.bounds.max_entries as usize
        {
            return Err(Failure::Canonical);
        }
        f.sources
            .len()
            .checked_add(f.assignments.len())
            .ok_or(Failure::Arithmetic)?
    } else {
        0
    };
    wire::reserve_scans(
        charged_row_count(s, m)?
            .checked_add(command_rows)
            .and_then(|n| n.checked_add(16))
            .ok_or(Failure::Arithmetic)?,
        m,
    )?;
    crypto_shape(verifier, c, m)?;
    let pre_root = s.root()?;
    if pre_root != ctx.pre_root {
        return Err(Failure::InvalidEvidence);
    }
    let clock = s
        .profile
        .parameters
        .clock
        .as_ref()
        .ok_or(Failure::UnboundParameter)?;
    if ctx.clock.backend != clock.backend || ctx.clock.finality_root == [0; 32] {
        return Err(Failure::InvalidEvidence);
    }
    if unauthorized(s, actor, c) {
        return Ok(Err(Reason::Unauthorized));
    }
    if ctx.nonce != s.nonce(actor) {
        return Ok(Err(Reason::Replay));
    }
    let Some(next) = ctx.nonce.checked_add(1) else {
        return Ok(Err(Reason::CounterExhausted));
    };
    if exhausted(s, c) {
        return Ok(Err(Reason::CounterExhausted));
    }
    if ctx.clock.epoch < s.epoch {
        return Ok(Err(Reason::BadClockOrSync));
    }
    if unknown(s, c) {
        return Ok(Err(Reason::UnknownReference));
    }
    if duplicate(s, c) {
        return Ok(Err(Reason::AlreadyExists));
    }
    if wrong_phase(s, c, ctx.clock.epoch) {
        return Ok(Err(Reason::WrongPhase));
    }
    let mut out = s.clone();
    let now = ctx.clock.epoch;
    match c {
        LifecycleCommand::Reward { feed, command } => {
            out = match super::native_rewards::run(s, actor, ctx, *feed, command, m)? {
                Ok(p) => p,
                Err(r) => return Ok(Err(r)),
            };
        }
        LifecycleCommand::Price { feed, command } => {
            out = match super::native_ports::run_price(s, actor, ctx, *feed, command, m)? {
                Ok(p) => p,
                Err(r) => return Ok(Err(r)),
            };
        }
        LifecycleCommand::RegisterFeed(feed) => {
            feed.rules.validate()?;
            if feed.id()? != feed.rules.feed
                || feed.rules.profile != s.profile_id
                || feed.rules.clock != *clock
                || feed.rules.bounds != s.profile.bounds
                || feed.rules.frame_bytes != s.profile.frame_bytes
            {
                return Ok(Err(Reason::BindingMismatch));
            }
            if feed.cutoff_delta == 0
                || feed.cutoff_delta > clock.heartbeat
                || feed.base == feed.quote
            {
                return Err(Failure::InvalidProfile);
            }
            if feed
                .sources
                .iter()
                .enumerate()
                .any(|(i, x)| feed.sources[..i].iter().any(|y| x.id == y.id))
                || feed
                    .assignments
                    .values()
                    .any(|id| !feed.sources.iter().any(|x| &x.id == id))
            {
                return Ok(Err(Reason::WrongSourceOrDiversity));
            }
            out.feeds.insert(feed.rules.feed, feed.clone());
            out.prices.insert(feed.rules.feed, PriceState::default());
            out.rewards.insert(
                feed.rules.feed,
                super::native_rewards::empty(s, feed.rules.feed)?,
            );
        }
        LifecycleCommand::RegisterKey { key, authorization } => {
            charge_crypto(m, 272)?;
            let message = registration_message(s, pre_root, ctx.nonce, key)?;
            if !verifier.signature(&key.public_key, &message, authorization) {
                return Ok(Err(Reason::BadSignature));
            }
            if key.version != 0 || !verifier.possession(&key.public_key, &key.possession) {
                return Ok(Err(Reason::BadPoPOrRotation));
            }
            out.keys.insert(
                (key.reporter.clone(), 0),
                RetainedKey {
                    key: key.clone(),
                    activated: now,
                    retired: None,
                    activation_sequence: 0,
                    retirement_sequence: None,
                    registration_root: pre_root,
                    registration_nonce: ctx.nonce,
                    old_version: None,
                    authorization: *authorization,
                    old_authorization: None,
                },
            );
            out.reporters.insert(
                key.reporter.clone(),
                ReporterRecord {
                    owner: key.owner.clone(),
                    active_version: 0,
                    registered: now,
                    chain: ReportChain::default(),
                },
            );
            out.bonds.insert(
                key.reporter.clone(),
                BondState {
                    reporter: key.reporter.clone(),
                    owner: key.owner.clone(),
                    phase: ReporterPhase::Active,
                    held: ZdexE18(0),
                    liabilities: Vec::new(),
                    disputes: Vec::new(),
                    wallets: BTreeMap::new(),
                    nonces: BTreeMap::new(),
                    burned: ZdexE18(0),
                    exit_epoch: None,
                    epoch: now,
                },
            );
        }
        LifecycleCommand::RotateKey {
            key,
            authorization,
            old_authorization,
        } => {
            let old_version = s
                .reporters
                .get(&key.reporter)
                .ok_or(Failure::InvalidState)?
                .active_version;
            let old = s
                .keys
                .get(&(key.reporter.clone(), old_version))
                .ok_or(Failure::InvalidState)?;
            charge_crypto(m, 400)?;
            let message = rotation_message(s, pre_root, ctx.nonce, old_version, key)?;
            if !verifier.signature(&old.key.public_key, &message, old_authorization)
                || !verifier.signature(&key.public_key, &message, authorization)
            {
                return Ok(Err(Reason::BadSignature));
            }
            if key.version != old_version.checked_add(1).ok_or(Failure::Arithmetic)?
                || key.owner != old.key.owner
                || !verifier.possession(&key.public_key, &key.possession)
            {
                return Ok(Err(Reason::BadPoPOrRotation));
            }
            out.keys
                .get_mut(&(key.reporter.clone(), old_version))
                .ok_or(Failure::InvalidState)?
                .retired = Some(now);
            let cutover = s
                .reporters
                .get(&key.reporter)
                .ok_or(Failure::InvalidState)?
                .chain
                .next_sequence;
            out.keys
                .get_mut(&(key.reporter.clone(), old_version))
                .ok_or(Failure::InvalidState)?
                .retirement_sequence = Some(cutover);
            out.keys.insert(
                (key.reporter.clone(), key.version),
                RetainedKey {
                    key: key.clone(),
                    activated: now,
                    retired: None,
                    activation_sequence: cutover,
                    retirement_sequence: None,
                    registration_root: pre_root,
                    registration_nonce: ctx.nonce,
                    old_version: Some(old_version),
                    authorization: *authorization,
                    old_authorization: Some(*old_authorization),
                },
            );
            out.reporters
                .get_mut(&key.reporter)
                .ok_or(Failure::InvalidState)?
                .active_version = key.version;
        }
        LifecycleCommand::OpenRound { feed, round } => {
            let f = s.feeds.get(feed).ok_or(Failure::InvalidState)?;
            let opening = f
                .first_open
                .checked_add(
                    round
                        .checked_mul(clock.heartbeat)
                        .ok_or(Failure::Arithmetic)?,
                )
                .ok_or(Failure::Arithmetic)?;
            let cutoff = opening
                .checked_add(f.cutoff_delta)
                .ok_or(Failure::Arithmetic)?;
            if now < opening || now >= cutoff {
                return Ok(Err(Reason::WindowClosed));
            }
            let eligible = f
                .assignments
                .keys()
                .filter(|id| {
                    s.bonds
                        .get(*id)
                        .is_some_and(|b| b.phase == ReporterPhase::Active)
                })
                .cloned()
                .collect();
            out.rounds.insert(
                (*feed, *round),
                RoundRecord {
                    opening,
                    cutoff,
                    eligible,
                    aggregate: None,
                    cutoff_admissions: None,
                    built_epoch: None,
                },
            );
        }
        LifecycleCommand::SubmitReport(report) => {
            let p = &report.payload;
            let reporter = s.reporters.get(&p.reporter).ok_or(Failure::InvalidState)?;
            let key = s
                .keys
                .get(&(p.reporter.clone(), p.key_version))
                .ok_or(Failure::InvalidState)?;
            let f = s.feeds.get(&p.feed).ok_or(Failure::InvalidState)?;
            let round = s
                .rounds
                .get(&(p.feed, p.round))
                .ok_or(Failure::InvalidState)?;
            charge_crypto(m, 128)?;
            if !verifier.signature(&key.key.public_key, &p.message(), &report.signature) {
                return Ok(Err(Reason::BadSignature));
            }
            if p.chain != s.chain
                || p.profile != f.rules.profile
                || p.query != f.rules.query
                || *zeno_fcis_crypto::RustCryptoSha256::hash(&p.canonical_json()).as_bytes()
                    != report.payload_hash
                || p.report_id(&report.signature) != report.id
            {
                return Ok(Err(Reason::BindingMismatch));
            }
            if p.sequence != reporter.chain.next_sequence || p.previous != reporter.chain.previous {
                return Ok(Err(Reason::BadSequence));
            }
            if p.observed_epoch > now {
                return Ok(Err(Reason::FutureObservation));
            }
            if reporter
                .chain
                .last_seconds
                .is_some_and(|t| p.observation_seconds < t)
            {
                return Ok(Err(Reason::BackwardObservation));
            }
            if now - p.observed_epoch > clock.max_age {
                return Ok(Err(Reason::StaleObservation));
            }
            if !round.eligible.contains(&p.reporter)
                || f.assignments.get(&p.reporter) != Some(&p.source)
            {
                return Ok(Err(Reason::WrongSourceOrDiversity));
            }
            let bond = s.bonds.get(&p.reporter).ok_or(Failure::InvalidState)?;
            let locked = bond.liabilities.iter().try_fold(0u128, |a, l| {
                a.checked_add(l.locked.0).ok_or(Failure::Arithmetic)
            })?;
            let minimum = s
                .profile
                .parameters
                .disputes
                .as_ref()
                .ok_or(Failure::UnboundParameter)?
                .minimum_bond;
            if bond
                .held
                .0
                .checked_sub(locked)
                .ok_or(Failure::InvalidState)?
                < minimum.0
            {
                return Ok(Err(Reason::InsufficientBond));
            }
            if now < round.opening || now >= round.cutoff {
                return Ok(Err(Reason::WindowClosed));
            }
            let liability = liability_id(s, report)?;
            let reserve = BondCommand::Reserve {
                id: liability,
                report: report.id,
                feed: p.feed,
            };
            let new_bond = apply_bond(s, &p.reporter, &reporter.owner, &reserve, ctx.clock, m)?;
            let new_bond = match new_bond {
                Ok(x) => x,
                Err(r) => return Ok(Err(r)),
            };
            out.bonds.insert(p.reporter.clone(), new_bond);
            let admission = admission_id(s, report, now, liability)?;
            out.reports.insert(
                report.id,
                AdmittedReport {
                    report: report.clone(),
                    submitted: now,
                    admission,
                    liability,
                },
            );
            out.reporters
                .get_mut(&p.reporter)
                .ok_or(Failure::InvalidState)?
                .chain = ReportChain {
                next_sequence: p.sequence.checked_add(1).ok_or(Failure::Arithmetic)?,
                previous: Some(report.id),
                last_seconds: Some(p.observation_seconds),
            };
        }
        LifecycleCommand::BuildAggregate { feed, round } => {
            let f = s.feeds.get(feed).ok_or(Failure::InvalidState)?;
            let r = s
                .rounds
                .get(&(*feed, *round))
                .ok_or(Failure::InvalidState)?;
            let all = observations(s, *feed, *round);
            let built =
                super::aggregation::build(&f.rules, *round, r.cutoff, now, &all, &f.sources, m)?;
            let a = match built {
                Ok(x) => x,
                Err(r) => return Ok(Err(r)),
            };
            if a.reports()
                .iter()
                .any(|id| super::native_ports::quarantines(s, *feed).contains(id))
            {
                return Ok(Err(Reason::DisputedOrFrozen));
            }
            let row = out
                .rounds
                .get_mut(&(*feed, *round))
                .ok_or(Failure::InvalidState)?;
            row.aggregate = Some(a.id());
            row.cutoff_admissions = Some(super::lifecycle::cutoff_commitment(s, *feed, *round)?);
            row.built_epoch = Some(now);
            out.aggregates.insert(a.id(), a);
        }
        LifecycleCommand::Bond { reporter, command } => {
            if matches!(
                command,
                BondCommand::Reserve { .. } | BondCommand::RecordPublication { .. }
            ) {
                return Err(Failure::UnboundParameter);
            }
            let result = apply_bond(s, reporter, actor, command, ctx.clock, m)?;
            let mut bond = match result {
                Ok(x) => x,
                Err(r) => return Ok(Err(r)),
            };
            // There is one zDEX wallet map for every reporter account. No
            // account can fabricate another copy of its owner's free balance.
            out.wallets = core::mem::take(&mut bond.wallets);
            bond.nonces.clear();
            out.bonds.insert(reporter.clone(), bond);
            if let BondCommand::Slash { dispute, .. } = command {
                let liability = out
                    .bonds
                    .get(reporter)
                    .and_then(|b| b.disputes.iter().find(|d| d.id == *dispute))
                    .ok_or(Failure::InvalidState)?
                    .liability;
                super::native_rewards::slash_claims(
                    &mut out, reporter, liability, ctx.clock, false, m,
                )?;
            }
        }
        LifecycleCommand::UpdateStress {
            feed,
            observation: o,
        } => {
            let e = s
                .profile
                .parameters
                .economy
                .as_ref()
                .ok_or(Failure::UnboundParameter)?;
            if o.source != e.stress_source || o.valuation_root == [0; 32] {
                return Err(Failure::InvalidEvidence);
            }
            if o.observed_epoch > now {
                return Ok(Err(Reason::FutureObservation));
            }
            if s.stress
                .get(feed)
                .is_some_and(|old| o.observed_epoch < old.observed_epoch)
            {
                return Ok(Err(Reason::BackwardObservation));
            }
            if now - o.observed_epoch > clock.max_age {
                return Ok(Err(Reason::StaleObservation));
            }
            // A zero or unavailable value replaces usable old data. There is
            // no capacity/exposure permission guard on recording a shock.
            out.stress.insert(*feed, o.clone());
        }
    }
    out.epoch = now;
    out.nonces.insert(actor.clone(), next);
    // SubmitReport used an internal owner invocation; only this actual issuer's
    // outer nonce advances. All retained account-local nonce maps stay empty.
    for bond in out.bonds.values_mut() {
        bond.wallets.clear();
        bond.nonces.clear();
    }
    if let Err(reason) = super::lifecycle_schema::fit(&out)? {
        return Ok(Err(reason));
    }
    let command_hash = wire::hash(
        b"zeno.oracle.command/1\0",
        &super::lifecycle_schema::command_value(s, actor, ctx, c)?,
    )?;
    let post_root = out.root()?;
    Ok(Ok(LifecyclePlan {
        state: out,
        pre_root,
        command_hash,
        post_root,
    }))
}

pub(super) fn charge_crypto(m: &mut Meter, bytes: u64) -> Result<(), Failure> {
    step(m)?;
    m.charge(Resource::WitnessByte, bytes)?;
    Ok(())
}
fn unauthorized(s: &LifecycleState, actor: &Id, c: &LifecycleCommand) -> bool {
    match c {
        LifecycleCommand::Reward { feed, command } => {
            super::native_rewards::unauthorized(s, actor, *feed, command)
        }
        LifecycleCommand::RegisterKey { key, .. } => actor != &key.owner,
        LifecycleCommand::RotateKey { key, .. } => s
            .reporters
            .get(&key.reporter)
            .is_some_and(|r| &r.owner != actor),
        LifecycleCommand::Bond {
            reporter,
            command: BondCommand::Unregister | BondCommand::Withdraw(_),
        } => s.reporters.get(reporter).is_some_and(|r| &r.owner != actor),
        _ => false,
    }
}
fn exhausted(s: &LifecycleState, c: &LifecycleCommand) -> bool {
    match c {
        LifecycleCommand::RotateKey { key, .. } => s
            .reporters
            .get(&key.reporter)
            .is_some_and(|r| r.active_version == u64::MAX),
        LifecycleCommand::SubmitReport(r) => s
            .reporters
            .get(&r.payload.reporter)
            .is_some_and(|p| p.chain.next_sequence == u64::MAX),
        LifecycleCommand::OpenRound { feed, .. } => {
            s.rounds.keys().any(|(f, r)| f == feed && *r == u64::MAX)
        }
        _ => false,
    }
}
fn unknown(s: &LifecycleState, c: &LifecycleCommand) -> bool {
    match c {
        LifecycleCommand::Price { feed, .. } | LifecycleCommand::Reward { feed, .. } => {
            !s.feeds.contains_key(feed)
        }
        LifecycleCommand::RotateKey { key, .. } => !s.reporters.contains_key(&key.reporter),
        LifecycleCommand::SubmitReport(r) => {
            !s.reporters.contains_key(&r.payload.reporter)
                || !s
                    .keys
                    .contains_key(&(r.payload.reporter.clone(), r.payload.key_version))
                || !s.feeds.contains_key(&r.payload.feed)
                || !s.rounds.contains_key(&(r.payload.feed, r.payload.round))
        }
        LifecycleCommand::OpenRound { feed, .. } | LifecycleCommand::UpdateStress { feed, .. } => {
            !s.feeds.contains_key(feed)
        }
        LifecycleCommand::BuildAggregate { feed, round } => {
            !s.feeds.contains_key(feed) || !s.rounds.contains_key(&(*feed, *round))
        }
        LifecycleCommand::Bond { reporter, .. } => !s.bonds.contains_key(reporter),
        _ => false,
    }
}
fn duplicate(s: &LifecycleState, c: &LifecycleCommand) -> bool {
    match c {
        LifecycleCommand::RegisterFeed(f) => s.feeds.contains_key(&f.rules.feed),
        LifecycleCommand::RegisterKey { key, .. } => s.reporters.contains_key(&key.reporter),
        LifecycleCommand::RotateKey { key, .. } => {
            s.keys.contains_key(&(key.reporter.clone(), key.version))
        }
        LifecycleCommand::OpenRound { feed, round } => s.rounds.contains_key(&(*feed, *round)),
        LifecycleCommand::SubmitReport(r) => {
            s.reports.contains_key(&r.id)
                || s.reports.values().any(|x| {
                    x.report.payload.reporter == r.payload.reporter
                        && x.report.payload.feed == r.payload.feed
                        && x.report.payload.round == r.payload.round
                })
        }
        _ => false,
    }
}
fn wrong_phase(s: &LifecycleState, c: &LifecycleCommand, now: u64) -> bool {
    match c {
        LifecycleCommand::RotateKey { key, .. } => s
            .bonds
            .get(&key.reporter)
            .is_some_and(|b| b.phase != ReporterPhase::Active),
        LifecycleCommand::SubmitReport(r) => {
            s.bonds
                .get(&r.payload.reporter)
                .is_some_and(|b| b.phase != ReporterPhase::Active)
                || s.reporters
                    .get(&r.payload.reporter)
                    .is_some_and(|p| p.active_version != r.payload.key_version)
                || s.rounds
                    .get(&(r.payload.feed, r.payload.round))
                    .is_some_and(|q| q.aggregate.is_some() || now < q.opening || now >= q.cutoff)
        }
        LifecycleCommand::OpenRound { feed, round } => {
            let next = s
                .rounds
                .keys()
                .filter(|(f, _)| f == feed)
                .map(|(_, r)| *r)
                .max()
                .map_or(Some(0), |old| old.checked_add(1));
            // Skipping is allowed only past rounds whose fixed cutoff has
            // already passed unopened; a live round is never skipped.
            next.is_none_or(|next| {
                *round < next || *round > next && !cutoff_passed(s, feed, *round - 1, now)
            })
        }
        LifecycleCommand::BuildAggregate { feed, round } => s
            .rounds
            .get(&(*feed, *round))
            .is_some_and(|q| q.aggregate.is_some()),
        _ => false,
    }
}
/// Whether first_open + round*heartbeat + cutoff_delta <= now. The sum is exact
/// in u128 (at most 2^128-1); an unknown feed already failed R05.
fn cutoff_passed(s: &LifecycleState, feed: &Hash, round: u64, now: u64) -> bool {
    let (Some(f), Some(c)) = (s.feeds.get(feed), s.profile.parameters.clock.as_ref()) else {
        return false;
    };
    u128::from(f.first_open)
        + u128::from(round) * u128::from(c.heartbeat)
        + u128::from(f.cutoff_delta)
        <= u128::from(now)
}

pub(super) fn liability_id(s: &LifecycleState, r: &RawReport) -> Result<Hash, Failure> {
    wire::hash(
        b"zeno.oracle.liability/1\0",
        &wire::record(alloc::vec![
            wire::blob(&r.id)?,
            wire::blob(r.payload.reporter.bytes())?,
            wire::blob(&r.payload.feed)?,
            wire::blob(&s.profile_id)?,
            wire::blob(BLS_POP_SCHEME.as_bytes())?,
            wire::integers(&[u128::from(r.payload.key_version)])?
        ])?,
    )
}
pub(super) fn admission_id(
    s: &LifecycleState,
    r: &RawReport,
    now: u64,
    liability: Hash,
) -> Result<Hash, Failure> {
    wire::hash(
        b"zeno.oracle.admission/1\0",
        &wire::record(alloc::vec![
            wire::blob(s.chain.bytes())?,
            wire::blob(&r.id)?,
            wire::blob(&s.profile_id)?,
            wire::blob(&liability)?,
            wire::integers(&[u128::from(now)])?
        ])?,
    )
}
pub(super) fn observations(s: &LifecycleState, feed: Hash, round: u64) -> Vec<Observation> {
    s.reports
        .values()
        .filter(|r| r.report.payload.feed == feed && r.report.payload.round == round)
        .map(|r| {
            let p = &r.report.payload;
            Observation {
                id: r.report.id,
                admission: r.admission,
                reporter: p.reporter.clone(),
                source: p.source.clone(),
                feed: p.feed,
                profile: p.profile,
                query: p.query,
                round: p.round,
                value: p.value,
                epoch: p.observed_epoch,
                seconds: p.observation_seconds,
            }
        })
        .collect()
}
pub(super) fn apply_bond(
    s: &LifecycleState,
    reporter: &Id,
    actor: &Id,
    command: &BondCommand,
    clock: ClockView,
    m: &mut Meter,
) -> Result<Result<BondState, Reason>, Failure> {
    let mut input = s.bonds.get(reporter).ok_or(Failure::InvalidState)?.clone();
    input.wallets = s.wallets.clone();
    input.nonces = s.nonces.clone();
    // These are derived subcommands, not a second external owner invocation.
    // The signed parent report and fresh report/liability ID supply replay.
    let nonce = if matches!(
        command,
        BondCommand::Reserve { .. } | BondCommand::RecordPublication { .. }
    ) {
        input.nonces.clear();
        0
    } else {
        s.nonce(actor)
    };
    let result = super::bonds::checked(
        AccountContext {
            parameters: &s.profile.parameters,
            actor,
            nonce,
            clock,
            capacity: s.profile.bounds.max_entries,
        },
        &input,
        command,
        m,
    )?;
    Ok(result.map(|p| p.into_state()))
}

pub(super) fn cutoff_commitment(
    s: &LifecycleState,
    feed: Hash,
    round: u64,
) -> Result<Hash, Failure> {
    let values = s
        .reports
        .values()
        .filter(|r| r.report.payload.feed == feed && r.report.payload.round == round)
        .map(|r| {
            wire::record(alloc::vec![
                wire::blob(&r.report.id)?,
                wire::blob(&r.admission)?,
                wire::integers(&[u128::from(r.submitted)])?
            ])
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    wire::hash(
        b"zeno.oracle.cutoff_admissions/1\0",
        &zeno_fcis_value::Value::vector(values).map_err(|_| Failure::Canonical)?,
    )
}

pub(super) fn crypto_shape(
    verifier: &impl BlsVerifier,
    c: &LifecycleCommand,
    m: &mut Meter,
) -> Result<(), Failure> {
    let valid = match c {
        LifecycleCommand::RegisterKey { key, authorization } => {
            charge_crypto(m, 240)?;
            verifier.canonical_key(&key.public_key)
                && verifier.canonical_signature(&key.possession)
                && verifier.canonical_signature(authorization)
        }
        LifecycleCommand::RotateKey {
            key,
            authorization,
            old_authorization,
        } => {
            charge_crypto(m, 336)?;
            verifier.canonical_key(&key.public_key)
                && verifier.canonical_signature(&key.possession)
                && verifier.canonical_signature(authorization)
                && verifier.canonical_signature(old_authorization)
        }
        LifecycleCommand::SubmitReport(r) => {
            charge_crypto(m, 96)?;
            verifier.canonical_signature(&r.signature)
        }
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(Failure::Canonical)
    }
}
