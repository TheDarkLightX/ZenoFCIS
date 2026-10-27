use super::Failure;
use alloc::boxed::Box;

/// Versioned native oracle profile identifier.
pub const PROFILE: &str = "oracle/1";
/// Approved native BLS proof-of-possession scheme identifier.
pub const BLS_POP_SCHEME: &str = "oracle/bls12381g2-pop/1";
/// Reserved hybrid report scheme; unsupported by the installed profile.
pub const HYBRID_REPORT_SCHEME: &str = "oracle/hybrid-bls-ml-dsa/1";
/// Reserved long-lived hybrid scheme; unsupported by the installed profile.
pub const HYBRID_LONG_LIVED_SCHEME: &str = "oracle/hybrid-bls-slh-dsa/1";
/// Fixed BLS proof-of-possession signature domain separation tag.
pub const SIGNATURE_DOMAIN: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";
/// Fixed BLS proof-of-possession admission domain separation tag.
pub const POSSESSION_DOMAIN: &[u8] = b"BLS_POP_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";
/// Domain separator for canonical native report messages.
pub const REPORT_DOMAIN: &[u8] = b"zeno.oracle.report/1\0";
/// Canonical 32-byte digest.
pub type Hash = [u8; 32];

/// Opaque, owned identity. Integers, empty IDs and sentinel identities are absent.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Id(Box<[u8]>);
impl Id {
    /// Construct an owned identity of one through 64 bytes.
    pub fn new(bytes: &[u8]) -> Result<Self, Failure> {
        if bytes.is_empty() || bytes.len() > 64 {
            return Err(Failure::Canonical);
        }
        Ok(Self(bytes.into()))
    }
    /// Borrow the immutable nominal identity bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}
/// zDEX amount with 18 decimal places.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ZdexE18(
    /// Raw atomic amount in this nominal unit.
    pub u128,
);
/// zUSD amount with 18 decimal places.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ZusdE18(
    /// Raw atomic amount in this nominal unit.
    pub u128,
);
/// AGRS amount with 18 decimal places.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct AgrsE18(
    /// Raw atomic amount in this nominal unit.
    pub u128,
);
/// USD exposure amount with 18 decimal places.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct UsdExposureE18(
    /// Raw atomic amount in this nominal unit.
    pub u128,
);
/// Stress valuation with eight decimal places.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct StressE8(
    /// Raw atomic amount in this nominal unit.
    pub u128,
);

/// Strictly positive price with eight decimal places.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PriceE8(u128);
impl PriceE8 {
    /// Construct a positive E8 price, refusing zero.
    pub fn new(raw: u128) -> Result<Self, Failure> {
        if raw == 0 {
            Err(Failure::Arithmetic)
        } else {
            Ok(Self(raw))
        }
    }
    /// Return the exact positive E8 integer.
    pub const fn raw(self) -> u128 {
        self.0
    }
}
/// Nominal reward denomination and decimal scale.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardUnit {
    /// Nominal asset identity.
    pub asset: Id,
    /// Number of decimal places in the reward unit.
    pub scale: u8,
}
impl RewardUnit {
    /// Check this value against its explicit profile and arithmetic bounds.
    pub fn validate(&self) -> Result<(), Failure> {
        if self.scale <= 38 {
            Ok(())
        } else {
            Err(Failure::InvalidProfile)
        }
    }
}

/// Stable ordered business refusal code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Reason {
    /// The actor lacks permission for this command.
    Unauthorized = 1,
    /// The actor nonce does not match the retained cursor.
    Replay = 2,
    /// The monotone counter cannot advance.
    CounterExhausted = 3,
    /// Clock or watermark evidence is behind the required bound.
    BadClockOrSync = 4,
    /// A referenced retained record is absent.
    UnknownReference = 5,
    /// The referenced identity is already retained.
    AlreadyExists = 6,
    /// The requested operation is unavailable in the current phase.
    WrongPhase = 7,
    /// The report or authorization signature is invalid.
    BadSignature = 8,
    /// Possession or rotation authorization is invalid.
    BadPoPOrRotation = 9,
    /// Evidence does not match the bound feed, profile, query or report.
    BindingMismatch = 10,
    /// The report sequence or predecessor link is incorrect.
    BadSequence = 11,
    /// A positive amount or price was required.
    ZeroAmountOrPrice = 12,
    /// The observation epoch lies in the future.
    FutureObservation = 13,
    /// The observation timestamp moved backwards.
    BackwardObservation = 14,
    /// The observation exceeds the bound age or watermark lag.
    StaleObservation = 15,
    /// Source assignment or quorum diversity is insufficient.
    WrongSourceOrDiversity = 16,
    /// Collectible free bond is below the required amount.
    InsufficientBond = 17,
    /// The actor’s available balance is insufficient.
    InsufficientBalance = 18,
    /// The cutoff set lacks a complete admissible quorum.
    NoCompleteQuorum = 19,
    /// The quorum price spread exceeds the bound deviation.
    ExcessUncertainty = 20,
    /// The operation lies outside its allowed time window.
    WindowClosed = 21,
    /// The pending aggregate has not completed its full buffer.
    BufferNotMature = 22,
    /// Disputed or quarantined evidence restricts monetary use.
    DisputedOrFrozen = 23,
    /// Commitment would regress the retained evidence order.
    NonMonotoneCommit = 24,
    /// Adjudication evidence fails the bound finality policy.
    InvalidVerdict = 25,
    /// Outstanding exposure or disputes prevent liability release.
    LiabilityLocked = 26,
    /// The funded pool cannot cover the requested reward.
    InsufficientRewardBudget = 27,
    /// The reward or violation nullifier is already consumed.
    AlreadyClaimedOrSlashed = 28,
    /// Collectible security cannot cover the bound exposure.
    SecurityBudgetExceeded = 29,
    /// The canonical item exceeds its byte limit.
    ItemTooLarge = 30,
    /// A bounded retained collection is at capacity.
    Full = 31,
    /// The canonical snapshot exceeds its byte limit.
    SnapshotTooLarge = 32,
}
/// All business refusal codes in their frozen precedence order.
pub const REASONS: [Reason; 32] = [
    Reason::Unauthorized,
    Reason::Replay,
    Reason::CounterExhausted,
    Reason::BadClockOrSync,
    Reason::UnknownReference,
    Reason::AlreadyExists,
    Reason::WrongPhase,
    Reason::BadSignature,
    Reason::BadPoPOrRotation,
    Reason::BindingMismatch,
    Reason::BadSequence,
    Reason::ZeroAmountOrPrice,
    Reason::FutureObservation,
    Reason::BackwardObservation,
    Reason::StaleObservation,
    Reason::WrongSourceOrDiversity,
    Reason::InsufficientBond,
    Reason::InsufficientBalance,
    Reason::NoCompleteQuorum,
    Reason::ExcessUncertainty,
    Reason::WindowClosed,
    Reason::BufferNotMature,
    Reason::DisputedOrFrozen,
    Reason::NonMonotoneCommit,
    Reason::InvalidVerdict,
    Reason::LiabilityLocked,
    Reason::InsufficientRewardBudget,
    Reason::AlreadyClaimedOrSlashed,
    Reason::SecurityBudgetExceeded,
    Reason::ItemTooLarge,
    Reason::Full,
    Reason::SnapshotTooLarge,
];

/// Explicit epoch, finality and observation-age bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClockParameters {
    /// Identity of the authenticated clock backend.
    pub backend: Hash,
    /// Maximum scheduled interval between feed rounds.
    pub heartbeat: u64,
    /// Epochs required between staging and commitment.
    pub confirmation: u64,
    /// Epochs during which a published report may be disputed.
    pub dispute_open: u64,
    /// Epoch allowance for evidence inclusion.
    pub inclusion: u64,
    /// Maximum observation age in epochs.
    pub max_age: u64,
    /// Upper bound on seconds represented by one epoch.
    pub max_epoch_seconds: u64,
    /// Maximum lag behind the authenticated time watermark, in seconds.
    pub max_time_lag: u64,
}
impl ClockParameters {
    /// Check this value against its explicit profile and arithmetic bounds.
    pub fn validate(&self) -> Result<(), Failure> {
        if self.backend == [0; 32]
            || self.heartbeat == 0
            || self.confirmation == 0
            || self.dispute_open == 0
            || self.max_epoch_seconds == 0
            || self.max_time_lag == 0
        {
            return Err(Failure::InvalidProfile);
        }
        let bound = self
            .heartbeat
            .checked_add(self.confirmation.max(self.dispute_open))
            .and_then(|n| n.checked_add(self.inclusion))
            .ok_or(Failure::Arithmetic)?;
        if bound > self.max_age {
            return Err(Failure::InvalidProfile);
        }
        Ok(())
    }
}
/// Bound destination of an applied penalty or challenge escrow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowRecipient {
    /// Pay the account that opened the dispute.
    Challenger,
    /// Pay the reporter’s owner.
    ReporterOwner,
    /// Pay the bound treasury account.
    Treasury,
    /// Record the amount as burned.
    Burn,
}
/// Explicit bond, finality, appeal and penalty policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeParameters {
    /// Identity of the bound final adjudicator.
    pub adjudicator: Hash,
    /// Minimum collectible zDEX reservation per report.
    pub minimum_bond: ZdexE18,
    /// zDEX escrow required to open a dispute.
    pub challenge_bond: ZdexE18,
    /// Scheduled or applied zDEX penalty.
    pub slash: ZdexE18,
    /// Epochs required before an unbonding withdrawal.
    pub exit_delay: u64,
    /// Epochs reserved for dispute finality.
    pub finality_delay: u64,
    /// Additional epochs reserved for appeals.
    pub appeal_delay: u64,
    /// Whether the bound adjudication policy permits false-price slashing.
    pub false_price_slashable: bool,
    /// Treasury account identity.
    pub treasury: Id,
    /// Recipient of an applied zDEX penalty.
    pub slash_recipient: EscrowRecipient,
    /// Recipient of challenge escrow when the dispute is upheld.
    pub upheld_bond_recipient: EscrowRecipient,
    /// Recipient of challenge escrow when the dispute is rejected.
    pub rejected_bond_recipient: EscrowRecipient,
}
/// Explicit reward, security fraction and USD exposure policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EconomicParameters {
    /// Nominal reward asset and decimal scale.
    pub reward_unit: RewardUnit,
    /// Reward amount per selected report in the bound unit.
    pub reward_per_report: u128,
    /// Numerator of the security budget fraction.
    pub alpha_numerator: u128,
    /// Denominator of the security budget fraction.
    pub alpha_denominator: u128,
    /// Identity of the bound stress valuation source.
    pub stress_source: Hash,
    /// Numerator of the conservative stress valuation haircut.
    pub stress_haircut_numerator: u128,
    /// Denominator of the conservative stress valuation haircut.
    pub stress_haircut_denominator: u128,
    /// Maximum debt exposure in USD E18.
    pub debt_ceiling: UsdExposureE18,
    /// Maximum liquidation throughput exposure in USD E18.
    pub liquidation_cap: UsdExposureE18,
    /// Maximum redemption throughput exposure in USD E18.
    pub redemption_cap: UsdExposureE18,
    /// Other bounded throughput exposure in USD E18.
    pub other_cap: UsdExposureE18,
    /// Economic exposure window in epochs.
    pub window: u64,
}
/// None is a binding refusal, never a default or production profile.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Parameters {
    /// Bound clock rules.
    pub clock: Option<ClockParameters>,
    /// Retained dispute records.
    pub disputes: Option<DisputeParameters>,
    /// Explicit economic policy, or an unbound refusal.
    pub economy: Option<EconomicParameters>,
}
impl Parameters {
    /// Check this value against its explicit profile and arithmetic bounds.
    pub fn validate(&self) -> Result<(), Failure> {
        let c = self.clock.as_ref().ok_or(Failure::UnboundParameter)?;
        let d = self.disputes.as_ref().ok_or(Failure::UnboundParameter)?;
        let e = self.economy.as_ref().ok_or(Failure::UnboundParameter)?;
        c.validate()?;
        e.reward_unit.validate()?;
        if d.adjudicator == [0; 32]
            || d.minimum_bond.0 == 0
            || d.challenge_bond.0 == 0
            || d.slash.0 == 0
            || d.slash.0 > d.minimum_bond.0
            || e.stress_source == [0; 32]
            || e.alpha_numerator == 0
            || e.alpha_numerator >= e.alpha_denominator
            || e.stress_haircut_numerator == 0
            || e.stress_haircut_numerator > e.stress_haircut_denominator
            || e.window == 0
        {
            return Err(Failure::InvalidProfile);
        }
        let horizon = c
            .confirmation
            .max(c.dispute_open)
            .checked_add(c.inclusion)
            .and_then(|x| x.checked_add(d.finality_delay))
            .and_then(|x| x.checked_add(d.appeal_delay))
            .ok_or(Failure::Arithmetic)?;
        if e.window < horizon {
            return Err(Failure::InvalidProfile);
        }
        Ok(())
    }
}

/// Already admitted report facts used by the pure quorum relation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    /// Canonical identity of this record.
    pub id: Hash,
    /// Canonical admission witness identity.
    pub admission: Hash,
    /// Reporter identity.
    pub reporter: Id,
    /// Source identity.
    pub source: Id,
    /// Canonical feed identity.
    pub feed: Hash,
    /// Bound profile identity.
    pub profile: Hash,
    /// Canonical query identity.
    pub query: Hash,
    /// Logical round identifier.
    pub round: u64,
    /// Positive price or stress observation in E8 units.
    pub value: PriceE8,
    /// Logical epoch of this state or evidence.
    pub epoch: u64,
    /// Observation time in seconds.
    pub seconds: u64,
}
/// Nominal source identity and five diversity classifications.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Source {
    /// Canonical identity of this record.
    pub id: Id,
    /// Source operator classification.
    pub operator: Id,
    /// Source venue classification.
    pub venue: Id,
    /// Source family classification.
    pub family: Id,
    /// Source transport classification.
    pub transport: Id,
    /// Source jurisdiction classification.
    pub jurisdiction: Id,
}
/// Distinct-class minima and shared-class maxima for five source axes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diversity {
    /// Minimum distinct classes on each diversity axis.
    pub minimum: [u8; 5],
    /// Maximum sources sharing a class on each diversity axis.
    pub max_same_class: [u8; 5],
}
impl Diversity {
    /// Check this value against its explicit profile and arithmetic bounds.
    pub fn validate(&self) -> Result<(), Failure> {
        if self.minimum.iter().any(|n| *n == 0 || *n > 3)
            || self.max_same_class.iter().any(|n| *n == 0 || *n > 3)
        {
            return Err(Failure::InvalidProfile);
        }
        Ok(())
    }
}
