//! ZenoDEX-specific profile values for ZenoFCIS.
//!
//! The first profile binds the current single-vault zUSD state shape and stable
//! rejection registry without claiming that the mounted ZenoDEX runtime already
//! emits complete ZenoFCIS candidates. Runtime promotion still requires exact
//! [`zeno_fcis_refine`] evidence.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::StableReason;
use zeno_fcis_refine::{PromotionPolicy, RefineError, ToolKind};
use zeno_fcis_value::{Field, Value};

/// Decimal E8 scale used by the initial zUSD profile.
pub const E8: u128 = 100_000_000;
/// Basis-point scale used by the initial zUSD profile.
pub const BPS_SCALE: u128 = 10_000;
/// Current stored-value bound mirrored from the ZenoDEX Rust zUSD kernel.
pub const MAX_AMOUNT_E8: u128 = 1_000_000_000_000_000_000_000_000_000_000;
/// Stable type identifier for the zUSD state value.
pub const ZUSD_STATE_TYPE_V1: u32 = 0x5a55_5301;
/// Stable type identifier for the zUSD command value.
pub const ZUSD_COMMAND_TYPE_V1: u32 = 0x5a55_4301;

/// ZenoDEX protocol lane.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ZenoDexLane {
    /// Spot balances, pools, liquidity, swaps, and nonces.
    Spot = 0,
    /// zUSD debt, collateral, Stability Pool, and liquidation state.
    Zusd = 1,
    /// Perpetual markets and positions.
    Perps = 2,
    /// Oracle observation and finalization authority.
    Oracle = 3,
    /// Governance and breaker state.
    Governance = 4,
}

/// Versioned ZenoDEX commitment and refinement profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ZenoDexProfileV1 {
    lane: ZenoDexLane,
    profile_version: u16,
    state_type: u32,
    command_type: u32,
    state_domain: &'static str,
    command_domain: &'static str,
    context_domain: &'static str,
    schema_hash: Hash32,
    precedence_hash: Hash32,
    algorithm_hash: Hash32,
}

impl ZenoDexProfileV1 {
    /// Creates the zUSD profile with externally reviewed schema, precedence, and algorithm commitments.
    #[must_use]
    pub const fn zusd(
        schema_hash: Hash32,
        precedence_hash: Hash32,
        algorithm_hash: Hash32,
    ) -> Self {
        Self {
            lane: ZenoDexLane::Zusd,
            profile_version: 1,
            state_type: ZUSD_STATE_TYPE_V1,
            command_type: ZUSD_COMMAND_TYPE_V1,
            state_domain: "zenodex/zusd/state",
            command_domain: "zenodex/zusd/command",
            context_domain: "zenodex/zusd/context",
            schema_hash,
            precedence_hash,
            algorithm_hash,
        }
    }

    /// Returns the lane.
    #[must_use]
    pub const fn lane(self) -> ZenoDexLane {
        self.lane
    }

    /// Returns the profile version.
    #[must_use]
    pub const fn profile_version(self) -> u16 {
        self.profile_version
    }

    /// Returns the state type identifier.
    #[must_use]
    pub const fn state_type(self) -> u32 {
        self.state_type
    }

    /// Returns the command type identifier.
    #[must_use]
    pub const fn command_type(self) -> u32 {
        self.command_type
    }

    /// Returns the state commitment domain.
    #[must_use]
    pub const fn state_domain(self) -> &'static str {
        self.state_domain
    }

    /// Returns the command commitment domain.
    #[must_use]
    pub const fn command_domain(self) -> &'static str {
        self.command_domain
    }

    /// Returns the context commitment domain.
    #[must_use]
    pub const fn context_domain(self) -> &'static str {
        self.context_domain
    }

    /// Returns the state/command schema commitment.
    #[must_use]
    pub const fn schema_hash(self) -> Hash32 {
        self.schema_hash
    }

    /// Returns the total reason-precedence commitment.
    #[must_use]
    pub const fn precedence_hash(self) -> Hash32 {
        self.precedence_hash
    }

    /// Returns the algorithm and codec profile commitment.
    #[must_use]
    pub const fn algorithm_hash(self) -> Hash32 {
        self.algorithm_hash
    }

    /// Computes the complete profile commitment.
    pub fn commitment<H: CommitmentHasher>(self) -> Result<Hash32, ProfileError> {
        let bytes = self.canonical_bytes().map_err(ProfileError::Encode)?;
        let domain = Domain::new("zenodex/profile", 1).map_err(ProfileError::Encode)?;
        commitment::<H>(domain, &bytes).map_err(ProfileError::Encode)
    }
}

impl CanonicalEncode for ZenoDexProfileV1 {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(self.lane as u8);
        output.extend_from_slice(&self.profile_version.to_be_bytes());
        output.extend_from_slice(&self.state_type.to_be_bytes());
        output.extend_from_slice(&self.command_type.to_be_bytes());
        put_blob(output, self.state_domain.as_bytes())?;
        put_blob(output, self.command_domain.as_bytes())?;
        put_blob(output, self.context_domain.as_bytes())?;
        output.extend_from_slice(self.schema_hash.as_bytes());
        output.extend_from_slice(self.precedence_hash.as_bytes());
        output.extend_from_slice(self.algorithm_hash.as_bytes());
        Ok(())
    }
}

/// Stable zUSD state-field identifiers in canonical record order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ZusdStateFieldV1 {
    /// Consensus-derived current epoch.
    NowEpoch = 0,
    /// Whether a finalized Oracle observation exists.
    OracleSeen = 1,
    /// Epoch of the finalized observation.
    OracleLastUpdateEpoch = 2,
    /// Finalized price in E8.
    PriceE8 = 3,
    /// Pending price in E8.
    PricePendingE8 = 4,
    /// Maximum finalized-price staleness.
    MaxOracleStalenessEpochs = 5,
    /// Vault collateral in E8.
    CollateralE8 = 6,
    /// Total zUSD debt in E8.
    DebtE8 = 7,
    /// Debt held outside the Stability Pool.
    FreeDebtE8 = 8,
    /// Debt held by the Stability Pool.
    StabilityPoolDebtE8 = 9,
    /// Stability Pool collateral in E8.
    StabilityPoolCollateralE8 = 10,
    /// Protocol-owned collateral in E8.
    ProtocolCollateralE8 = 11,
    /// Cumulative protocol zUSD revenue.
    ProtocolRevenueZusdCumulativeE8 = 12,
    /// Cumulative liquidator collateral compensation.
    LiquidatorCompensationCollateralCumulativeE8 = 13,
    /// Minimum collateral ratio in basis points.
    McrBps = 14,
    /// Critical collateral ratio in basis points.
    CcrBps = 15,
    /// Minimum open debt in E8.
    MinDebtOpenE8 = 16,
    /// Per-vault maximum debt in E8.
    MaxDebtE8 = 17,
    /// Global maximum debt supply in E8.
    MaxDebtSupplyE8 = 18,
    /// Maximum Stability Pool collateral in E8.
    MaxStabilityPoolCollateralE8 = 19,
    /// Maximum protocol collateral in E8.
    MaxProtocolCollateralE8 = 20,
    /// Current fee base rate in basis points.
    BaseRateBps = 21,
    /// Last base-rate update epoch.
    BaseRateLastEpoch = 22,
    /// Base-rate decay per epoch in basis points.
    BaseRateDecayPerEpochBps = 23,
    /// Base-rate borrowing bump in basis points.
    BaseRateBorrowBumpBps = 24,
    /// Base-rate redemption bump in basis points.
    BaseRateRedeemBumpBps = 25,
    /// Borrow fee floor in basis points.
    BorrowFeeFloorBps = 26,
    /// Borrow fee maximum in basis points.
    BorrowFeeMaxBps = 27,
    /// Redemption fee floor in basis points.
    RedemptionFeeFloorBps = 28,
    /// Redemption fee maximum in basis points.
    RedemptionFeeMaxBps = 29,
    /// Fixed liquidation gas compensation in collateral E8.
    LiquidationGasCompensationFixedCollateralE8 = 30,
    /// Variable liquidation gas compensation in basis points.
    LiquidationGasCompensationBps = 31,
}

impl ZusdStateFieldV1 {
    /// Canonical field count.
    pub const COUNT: usize = 32;

    /// Returns the canonical record identifier.
    #[must_use]
    pub const fn id(self) -> u16 {
        self as u16
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// Closed single-vault zUSD state corresponding to the current Rust kernel shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZusdStateV1 {
    fields: [u128; ZusdStateFieldV1::COUNT],
}

impl ZusdStateV1 {
    /// Constructs and checks the hard representation/accounting invariants.
    pub fn try_from_fields(
        fields: [u128; ZusdStateFieldV1::COUNT],
    ) -> Result<Self, ZusdStateError> {
        let state = Self { fields };
        state.validate_hard_invariants()?;
        Ok(state)
    }

    /// Parses the canonical reference record representation.
    pub fn try_from_value(value: &Value) -> Result<Self, ZusdStateError> {
        let Value::Record(fields) = value else {
            return Err(ZusdStateError::WrongValueKind);
        };
        if fields.len() != ZusdStateFieldV1::COUNT {
            return Err(ZusdStateError::WrongFieldCount);
        }
        let mut values = [0_u128; ZusdStateFieldV1::COUNT];
        for (index, field) in fields.iter().enumerate() {
            if usize::from(field.id()) != index {
                return Err(ZusdStateError::WrongFieldOrder);
            }
            values[index] = if index == ZusdStateFieldV1::OracleSeen.index() {
                match field.value() {
                    Value::Bool(false) => 0,
                    Value::Bool(true) => 1,
                    _ => return Err(ZusdStateError::WrongFieldKind),
                }
            } else {
                match field.value() {
                    Value::U128(value) => *value,
                    _ => return Err(ZusdStateError::WrongFieldKind),
                }
            };
        }
        Self::try_from_fields(values)
    }

    /// Converts into the canonical reference record representation.
    pub fn to_value(&self) -> Result<Value, ZusdStateError> {
        let mut output = Vec::with_capacity(ZusdStateFieldV1::COUNT);
        for (index, value) in self.fields.iter().copied().enumerate() {
            let field_value = if index == ZusdStateFieldV1::OracleSeen.index() {
                Value::Bool(value == 1)
            } else {
                Value::U128(value)
            };
            output.push(Field::new(
                u16::try_from(index).map_err(|_| ZusdStateError::ArithmeticOverflow)?,
                field_value,
            ));
        }
        Value::record_canonical(output).map_err(|_| ZusdStateError::WrongFieldOrder)
    }

    /// Returns one raw field value; `OracleSeen` is returned as zero or one.
    #[must_use]
    pub const fn field(&self, field: ZusdStateFieldV1) -> u128 {
        self.fields[field.index()]
    }

    /// Returns whether finalized Oracle authority exists.
    #[must_use]
    pub const fn oracle_seen(&self) -> bool {
        self.field(ZusdStateFieldV1::OracleSeen) == 1
    }

    /// Rechecks all hard representation and accounting invariants.
    pub fn validate_hard_invariants(&self) -> Result<(), ZusdStateError> {
        if self.fields.iter().any(|value| *value > MAX_AMOUNT_E8) {
            return Err(ZusdStateError::AmountBound);
        }
        let now = self.field(ZusdStateFieldV1::NowEpoch);
        if self.field(ZusdStateFieldV1::OracleLastUpdateEpoch) > now
            || self.field(ZusdStateFieldV1::BaseRateLastEpoch) > now
        {
            return Err(ZusdStateError::FutureEpoch);
        }
        let seen = self.field(ZusdStateFieldV1::OracleSeen);
        if seen > 1 {
            return Err(ZusdStateError::OracleFlag);
        }
        let active = self.field(ZusdStateFieldV1::PriceE8);
        let pending = self.field(ZusdStateFieldV1::PricePendingE8);
        let last = self.field(ZusdStateFieldV1::OracleLastUpdateEpoch);
        if seen == 1 {
            if active == 0 || pending == 0 || pending > active {
                return Err(ZusdStateError::OracleShape);
            }
        } else if active != 0 || pending != 0 || last != 0 {
            return Err(ZusdStateError::OracleShape);
        }
        let free = self.field(ZusdStateFieldV1::FreeDebtE8);
        let stability = self.field(ZusdStateFieldV1::StabilityPoolDebtE8);
        let debt = self.field(ZusdStateFieldV1::DebtE8);
        if free.checked_add(stability) != Some(debt) {
            return Err(ZusdStateError::SupplyConservation);
        }
        if debt > self.field(ZusdStateFieldV1::MaxDebtSupplyE8) {
            return Err(ZusdStateError::DebtCap);
        }
        let minimum = self.field(ZusdStateFieldV1::MinDebtOpenE8);
        if debt != 0 && debt < minimum {
            return Err(ZusdStateError::DebtFloor);
        }
        let mcr = self.field(ZusdStateFieldV1::McrBps);
        let ccr = self.field(ZusdStateFieldV1::CcrBps);
        if mcr == 0 || mcr > ccr {
            return Err(ZusdStateError::CollateralRatios);
        }
        if self.field(ZusdStateFieldV1::MaxDebtE8) > self.field(ZusdStateFieldV1::MaxDebtSupplyE8) {
            return Err(ZusdStateError::DebtLimits);
        }
        let base = self.field(ZusdStateFieldV1::BaseRateBps);
        let decay = self.field(ZusdStateFieldV1::BaseRateDecayPerEpochBps);
        let borrow_bump = self.field(ZusdStateFieldV1::BaseRateBorrowBumpBps);
        let redeem_bump = self.field(ZusdStateFieldV1::BaseRateRedeemBumpBps);
        let borrow_floor = self.field(ZusdStateFieldV1::BorrowFeeFloorBps);
        let borrow_max = self.field(ZusdStateFieldV1::BorrowFeeMaxBps);
        let redeem_floor = self.field(ZusdStateFieldV1::RedemptionFeeFloorBps);
        let redeem_max = self.field(ZusdStateFieldV1::RedemptionFeeMaxBps);
        let liquidation_bps = self.field(ZusdStateFieldV1::LiquidationGasCompensationBps);
        if [
            base,
            decay,
            borrow_bump,
            redeem_bump,
            borrow_max,
            redeem_max,
            liquidation_bps,
        ]
        .into_iter()
        .any(|value| value > BPS_SCALE)
            || borrow_floor > borrow_max
            || redeem_floor > redeem_max
        {
            return Err(ZusdStateError::FeeBounds);
        }
        Ok(())
    }

    /// Computes the state root under the supplied vetted provider and profile domain.
    pub fn root<H: CommitmentHasher>(
        &self,
        profile: ZenoDexProfileV1,
    ) -> Result<Hash32, ProfileError> {
        let value = self.to_value().map_err(ProfileError::State)?;
        let bytes = value.canonical_bytes().map_err(ProfileError::Encode)?;
        let domain = Domain::new(profile.state_domain(), profile.profile_version())
            .map_err(ProfileError::Encode)?;
        commitment::<H>(domain, &bytes).map_err(ProfileError::Encode)
    }
}

/// Current zUSD command tags, including the legacy caller-driven epoch test command.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ZusdCommandTagV1 {
    /// Legacy test/migration command; not production authority.
    AdvanceEpochLegacy = 0,
    /// Bootstrap finalized Oracle authority.
    BootstrapOracle = 1,
    /// Submit a pending Oracle observation.
    OracleReport = 2,
    /// Finalize a pending Oracle observation.
    OracleCommit = 3,
    /// Deposit vault collateral.
    DepositCollateral = 4,
    /// Withdraw vault collateral.
    WithdrawCollateral = 5,
    /// Mint zUSD debt.
    MintZusd = 6,
    /// Repay zUSD debt.
    RepayZusd = 7,
    /// Move free debt into the Stability Pool.
    DepositStabilityPool = 8,
    /// Withdraw Stability Pool debt.
    WithdrawStabilityPool = 9,
    /// Redeem zUSD for collateral.
    RedeemZusd = 10,
    /// Liquidate an unsafe vault.
    Liquidate = 11,
}

impl ZusdCommandTagV1 {
    /// Returns whether the command is admitted as production protocol authority.
    #[must_use]
    pub const fn is_protocol_authority(self) -> bool {
        !matches!(self, Self::AdvanceEpochLegacy)
    }
}

/// Stable exhaustive rejection registry mirrored from the current Rust zUSD kernel.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u16)]
pub enum ZusdRejectV1 {
    /// Required positive integer is absent, zero, negative, or malformed.
    NotPositiveInt = 0,
    /// Stored or computed value exceeds a declared bound.
    BoundedCheckFailed = 1,
    /// Pre-state or successor violates a hard invariant.
    InvariantViolation = 2,
    /// Command tag is unknown.
    UnknownAction = 3,
    /// Oracle bootstrap was attempted after initialization.
    OracleAlreadyBootstrapped = 4,
    /// Oracle bootstrap lacks authority.
    BootstrapRequiresAuth = 5,
    /// Operation requires bootstrapped Oracle state.
    OracleNotBootstrapped = 6,
    /// Oracle report lacks authority.
    ReportRequiresAuth = 7,
    /// Report violates the current non-increasing profile.
    ReportPriceNotNonIncreasing = 8,
    /// Oracle commit lacks authority.
    CommitRequiresAuth = 9,
    /// Pending observation is stale at commit.
    CommitStaleOracle = 10,
    /// Vault lacks requested collateral.
    InsufficientCollateral = 11,
    /// Withdrawal is blocked by Oracle/recovery state.
    WithdrawBlockedOracle = 12,
    /// Withdrawal violates MCR.
    WithdrawViolatesMcr = 13,
    /// Mint is blocked by Oracle/recovery state.
    MintBlockedOracle = 14,
    /// Mint leaves nonzero debt below the floor.
    MintBelowMinDebt = 15,
    /// Mint exceeds per-vault debt maximum.
    MintExceedsMaxDebt = 16,
    /// Mint exceeds authoritative total supply cap.
    MintExceedsMaxSupply = 17,
    /// Mint violates MCR.
    MintViolatesMcr = 18,
    /// Repayment exceeds total debt.
    RepayExceedsDebt = 19,
    /// Repayment exceeds free debt.
    RepayExceedsFreeDebt = 20,
    /// Repayment leaves nonzero debt below the floor.
    RepayBelowMinDebt = 21,
    /// Stability Pool deposit exceeds free debt.
    DepositSpExceedsFreeDebt = 22,
    /// Stability Pool deposit exceeds supply policy.
    DepositSpExceedsMaxSupply = 23,
    /// Stability Pool withdrawal exceeds pool debt.
    WithdrawSpExceedsSpDebt = 24,
    /// Stability Pool withdrawal is blocked by Oracle/recovery state.
    WithdrawSpBlockedOracle = 25,
    /// Stability Pool withdrawal violates MCR.
    WithdrawSpBelowMcr = 26,
    /// Redemption lacks initialized finalized Oracle authority.
    RedeemOracleUninitialized = 27,
    /// Redemption sees pending/finalized Oracle mismatch.
    RedeemPendingMismatch = 28,
    /// Redemption sees stale finalized Oracle authority.
    RedeemStaleOracle = 29,
    /// Redemption exceeds total debt.
    RedeemExceedsDebt = 30,
    /// Redemption exceeds free debt.
    RedeemExceedsFreeDebt = 31,
    /// Redemption amount is below its exact minimum.
    RedeemAmountTooSmall = 32,
    /// Redemption lacks sufficient collateral.
    RedeemInsufficientCollateral = 33,
    /// Redemption fee consumes the entire output.
    RedeemFeeConsumesAll = 34,
    /// Redemption exceeds protocol collateral capacity.
    RedeemProtocolCapExceeded = 35,
    /// Redemption leaves nonzero debt below the floor.
    RedeemBelowMinDebt = 36,
    /// Redemption successor violates MCR.
    RedeemViolatesMcr = 37,
    /// Liquidation lacks initialized finalized Oracle authority.
    LiquidateOracleUninitialized = 38,
    /// Liquidation sees pending/finalized Oracle mismatch.
    LiquidatePendingMismatch = 39,
    /// Liquidation sees stale finalized Oracle authority.
    LiquidateStaleOracle = 40,
    /// Liquidation target has no debt.
    LiquidateNoDebt = 41,
    /// Liquidation target is not below MCR.
    LiquidateNotUnderMcr = 42,
    /// Stability Pool cannot absorb liquidation debt.
    LiquidateSpCannotAbsorb = 43,
    /// Liquidation exceeds Stability Pool collateral capacity.
    LiquidateSpCapExceeded = 44,
}

impl ZusdRejectV1 {
    /// Complete registry in exact precedence order.
    pub const ALL: [Self; 45] = [
        Self::NotPositiveInt,
        Self::BoundedCheckFailed,
        Self::InvariantViolation,
        Self::UnknownAction,
        Self::OracleAlreadyBootstrapped,
        Self::BootstrapRequiresAuth,
        Self::OracleNotBootstrapped,
        Self::ReportRequiresAuth,
        Self::ReportPriceNotNonIncreasing,
        Self::CommitRequiresAuth,
        Self::CommitStaleOracle,
        Self::InsufficientCollateral,
        Self::WithdrawBlockedOracle,
        Self::WithdrawViolatesMcr,
        Self::MintBlockedOracle,
        Self::MintBelowMinDebt,
        Self::MintExceedsMaxDebt,
        Self::MintExceedsMaxSupply,
        Self::MintViolatesMcr,
        Self::RepayExceedsDebt,
        Self::RepayExceedsFreeDebt,
        Self::RepayBelowMinDebt,
        Self::DepositSpExceedsFreeDebt,
        Self::DepositSpExceedsMaxSupply,
        Self::WithdrawSpExceedsSpDebt,
        Self::WithdrawSpBlockedOracle,
        Self::WithdrawSpBelowMcr,
        Self::RedeemOracleUninitialized,
        Self::RedeemPendingMismatch,
        Self::RedeemStaleOracle,
        Self::RedeemExceedsDebt,
        Self::RedeemExceedsFreeDebt,
        Self::RedeemAmountTooSmall,
        Self::RedeemInsufficientCollateral,
        Self::RedeemFeeConsumesAll,
        Self::RedeemProtocolCapExceeded,
        Self::RedeemBelowMinDebt,
        Self::RedeemViolatesMcr,
        Self::LiquidateOracleUninitialized,
        Self::LiquidatePendingMismatch,
        Self::LiquidateStaleOracle,
        Self::LiquidateNoDebt,
        Self::LiquidateNotUnderMcr,
        Self::LiquidateSpCannotAbsorb,
        Self::LiquidateSpCapExceeded,
    ];
}

impl StableReason for ZusdRejectV1 {
    fn code(&self) -> &'static str {
        match self {
            Self::NotPositiveInt => "not_positive_int",
            Self::BoundedCheckFailed => "bounded_check_failed",
            Self::InvariantViolation => "invariant_violation",
            Self::UnknownAction => "unknown_action",
            Self::OracleAlreadyBootstrapped => "oracle_already_bootstrapped",
            Self::BootstrapRequiresAuth => "bootstrap_requires_auth",
            Self::OracleNotBootstrapped => "oracle_not_bootstrapped",
            Self::ReportRequiresAuth => "report_requires_auth",
            Self::ReportPriceNotNonIncreasing => "report_price_not_non_increasing",
            Self::CommitRequiresAuth => "commit_requires_auth",
            Self::CommitStaleOracle => "commit_stale_oracle",
            Self::InsufficientCollateral => "insufficient_collateral",
            Self::WithdrawBlockedOracle => "withdraw_blocked_oracle",
            Self::WithdrawViolatesMcr => "withdraw_violates_mcr",
            Self::MintBlockedOracle => "mint_blocked_oracle",
            Self::MintBelowMinDebt => "mint_below_min_debt",
            Self::MintExceedsMaxDebt => "mint_exceeds_max_debt",
            Self::MintExceedsMaxSupply => "mint_exceeds_max_supply",
            Self::MintViolatesMcr => "mint_violates_mcr",
            Self::RepayExceedsDebt => "repay_exceeds_debt",
            Self::RepayExceedsFreeDebt => "repay_exceeds_free_debt",
            Self::RepayBelowMinDebt => "repay_below_min_debt",
            Self::DepositSpExceedsFreeDebt => "deposit_sp_exceeds_free_debt",
            Self::DepositSpExceedsMaxSupply => "deposit_sp_exceeds_max_supply",
            Self::WithdrawSpExceedsSpDebt => "withdraw_sp_exceeds_sp_debt",
            Self::WithdrawSpBlockedOracle => "withdraw_sp_blocked_oracle",
            Self::WithdrawSpBelowMcr => "withdraw_sp_below_mcr",
            Self::RedeemOracleUninitialized => "redeem_oracle_uninitialized",
            Self::RedeemPendingMismatch => "redeem_pending_mismatch",
            Self::RedeemStaleOracle => "redeem_stale_oracle",
            Self::RedeemExceedsDebt => "redeem_exceeds_debt",
            Self::RedeemExceedsFreeDebt => "redeem_exceeds_free_debt",
            Self::RedeemAmountTooSmall => "redeem_amount_too_small",
            Self::RedeemInsufficientCollateral => "redeem_insufficient_collateral",
            Self::RedeemFeeConsumesAll => "redeem_fee_consumes_all",
            Self::RedeemProtocolCapExceeded => "redeem_protocol_cap_exceeded",
            Self::RedeemBelowMinDebt => "redeem_below_min_debt",
            Self::RedeemViolatesMcr => "redeem_violates_mcr",
            Self::LiquidateOracleUninitialized => "liquidate_oracle_uninitialized",
            Self::LiquidatePendingMismatch => "liquidate_pending_mismatch",
            Self::LiquidateStaleOracle => "liquidate_stale_oracle",
            Self::LiquidateNoDebt => "liquidate_no_debt",
            Self::LiquidateNotUnderMcr => "liquidate_not_under_mcr",
            Self::LiquidateSpCannotAbsorb => "liquidate_sp_cannot_absorb",
            Self::LiquidateSpCapExceeded => "liquidate_sp_cap_exceeded",
        }
    }

    fn precedence(&self) -> u16 {
        *self as u16
    }
}

/// Canonically encodes the complete zUSD rejection-precedence registry.
pub fn zusd_precedence_bytes_v1() -> Result<Vec<u8>, EncodeError> {
    let mut output = Vec::new();
    let count = u16::try_from(ZusdRejectV1::ALL.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&count.to_be_bytes());
    for reason in ZusdRejectV1::ALL {
        output.extend_from_slice(&reason.precedence().to_be_bytes());
        put_blob(&mut output, reason.code().as_bytes())?;
    }
    Ok(output)
}

/// Computes the total zUSD precedence commitment.
pub fn zusd_precedence_hash_v1<H: CommitmentHasher>() -> Result<Hash32, ProfileError> {
    let bytes = zusd_precedence_bytes_v1().map_err(ProfileError::Encode)?;
    let domain = Domain::new("zenodex/zusd/precedence", 1).map_err(ProfileError::Encode)?;
    commitment::<H>(domain, &bytes).map_err(ProfileError::Encode)
}

/// High-assurance promotion policy for the first ZenoDEX profile.
pub fn zusd_promotion_policy_v1() -> Result<PromotionPolicy, RefineError> {
    PromotionPolicy::try_new(
        vec![
            ToolKind::Z3,
            ToolKind::Cvc5,
            ToolKind::Lean,
            ToolKind::Kani,
            ToolKind::TranslationValidation,
            ToolKind::CodecVectors,
            ToolKind::RuntimeRefinement,
        ],
        true,
    )
}

/// Hard zUSD state-shape failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZusdStateError {
    /// Reference value is not a record.
    WrongValueKind,
    /// Record does not contain exactly 32 fields.
    WrongFieldCount,
    /// Field identifiers do not equal their canonical positions.
    WrongFieldOrder,
    /// A field has the wrong machine value kind.
    WrongFieldKind,
    /// One stored field exceeds the profile bound.
    AmountBound,
    /// Oracle flag is not zero or one.
    OracleFlag,
    /// State contains future authority timestamps.
    FutureEpoch,
    /// Oracle fields are inconsistent with initialization state.
    OracleShape,
    /// Free plus Stability Pool debt does not equal total debt.
    SupplyConservation,
    /// Total debt exceeds global supply policy.
    DebtCap,
    /// Nonzero debt is below the opening floor.
    DebtFloor,
    /// MCR/CCR ordering is invalid.
    CollateralRatios,
    /// Per-vault and global debt limits are inconsistent.
    DebtLimits,
    /// Fee parameters exceed basis-point or floor/maximum constraints.
    FeeBounds,
    /// Checked conversion or arithmetic failed.
    ArithmeticOverflow,
}

impl fmt::Display for ZusdStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::WrongValueKind => "zUSD state is not a record",
            Self::WrongFieldCount => "zUSD state has the wrong field count",
            Self::WrongFieldOrder => "zUSD state fields are not in canonical order",
            Self::WrongFieldKind => "zUSD state field has the wrong value kind",
            Self::AmountBound => "zUSD state field exceeds the stored-value bound",
            Self::OracleFlag => "zUSD Oracle flag is not zero or one",
            Self::FutureEpoch => "zUSD authority timestamp is in the future",
            Self::OracleShape => "zUSD Oracle state is structurally inconsistent",
            Self::SupplyConservation => "free plus Stability Pool debt does not equal total debt",
            Self::DebtCap => "zUSD total debt exceeds global supply cap",
            Self::DebtFloor => "zUSD nonzero debt is below the floor",
            Self::CollateralRatios => "zUSD collateral ratios are invalid",
            Self::DebtLimits => "zUSD debt limits are inconsistent",
            Self::FeeBounds => "zUSD fee parameters are invalid",
            Self::ArithmeticOverflow => "zUSD checked arithmetic failed",
        })
    }
}

/// ZenoDEX profile construction or commitment failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileError {
    /// Canonical encoding failed.
    Encode(EncodeError),
    /// zUSD state construction failed.
    State(ZusdStateError),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(error) => error.fmt(formatter),
            Self::State(error) => error.fmt(formatter),
        }
    }
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    let length = u16::try_from(bytes.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHasher;

    impl CommitmentHasher for TestHasher {
        const ALGORITHM_ID: &'static str = "test/fold/v1";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().copied().enumerate() {
                output[index % 32] = output[index % 32].wrapping_add(byte);
            }
            Hash32::new(output)
        }
    }

    fn default_fields() -> [u128; ZusdStateFieldV1::COUNT] {
        let mut fields = [0_u128; ZusdStateFieldV1::COUNT];
        fields[ZusdStateFieldV1::MaxOracleStalenessEpochs.index()] = 100;
        fields[ZusdStateFieldV1::McrBps.index()] = 11_000;
        fields[ZusdStateFieldV1::CcrBps.index()] = 15_000;
        fields[ZusdStateFieldV1::MinDebtOpenE8.index()] = 100 * E8;
        fields[ZusdStateFieldV1::MaxDebtE8.index()] = 10_000_000 * E8;
        fields[ZusdStateFieldV1::MaxDebtSupplyE8.index()] = 20_000_000 * E8;
        fields[ZusdStateFieldV1::MaxStabilityPoolCollateralE8.index()] = 20_000_000 * E8;
        fields[ZusdStateFieldV1::MaxProtocolCollateralE8.index()] = 20_000_000 * E8;
        fields[ZusdStateFieldV1::BorrowFeeMaxBps.index()] = 1_000;
        fields[ZusdStateFieldV1::RedemptionFeeMaxBps.index()] = 1_000;
        fields
    }

    #[test]
    fn zusd_state_round_trip_preserves_canonical_fields() {
        let state = ZusdStateV1::try_from_fields(default_fields())
            .unwrap_or_else(|error| panic!("state: {error}"));
        let value = state
            .to_value()
            .unwrap_or_else(|error| panic!("value: {error}"));
        assert_eq!(ZusdStateV1::try_from_value(&value), Ok(state));
    }

    #[test]
    fn total_debt_cap_and_conservation_are_hard_invariants() {
        let mut fields = default_fields();
        fields[ZusdStateFieldV1::DebtE8.index()] = 2;
        fields[ZusdStateFieldV1::FreeDebtE8.index()] = 2;
        fields[ZusdStateFieldV1::MaxDebtSupplyE8.index()] = 1;
        assert_eq!(
            ZusdStateV1::try_from_fields(fields),
            Err(ZusdStateError::DebtCap)
        );
    }

    #[test]
    fn reason_registry_has_stable_total_precedence() {
        assert_eq!(ZusdRejectV1::ALL.len(), 45);
        assert_eq!(ZusdRejectV1::NotPositiveInt.precedence(), 0);
        assert_eq!(ZusdRejectV1::LiquidateSpCapExceeded.precedence(), 44);
        assert_eq!(
            ZusdRejectV1::MintExceedsMaxSupply.code(),
            "mint_exceeds_max_supply"
        );
    }

    #[test]
    fn legacy_epoch_command_is_not_protocol_authority() {
        assert!(!ZusdCommandTagV1::AdvanceEpochLegacy.is_protocol_authority());
        assert!(ZusdCommandTagV1::Liquidate.is_protocol_authority());
    }

    #[test]
    fn profile_and_precedence_commitments_are_deterministic() {
        let precedence = zusd_precedence_hash_v1::<TestHasher>()
            .unwrap_or_else(|error| panic!("precedence: {error}"));
        let profile =
            ZenoDexProfileV1::zusd(Hash32::new([1; 32]), precedence, Hash32::new([2; 32]));
        let first = profile
            .commitment::<TestHasher>()
            .unwrap_or_else(|error| panic!("profile: {error}"));
        let second = profile
            .commitment::<TestHasher>()
            .unwrap_or_else(|error| panic!("profile: {error}"));
        assert_eq!(first, second);
    }

    #[test]
    fn promotion_policy_requires_independent_and_mounted_evidence() {
        let policy =
            zusd_promotion_policy_v1().unwrap_or_else(|error| panic!("promotion policy: {error}"));
        assert_eq!(policy.required_tools().len(), 7);
        assert!(policy.require_exact_cases());
    }
}
