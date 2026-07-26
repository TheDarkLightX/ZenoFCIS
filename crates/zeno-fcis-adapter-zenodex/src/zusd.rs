//! Concrete, evidence-only mount for the pinned ZenoDEX single-vault zUSD transition.

use core::fmt;

use serde::ser::SerializeStruct as _;
use serde::{Deserialize, Serialize};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::DecisionKind;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_patch::{CanonicalPatch, PatchError, PatchOp, ValuePath, hash_value};
use zeno_fcis_plan::{CommitPlan, OutboxPlan};
use zeno_fcis_profile_zenodex::{
    ProfileError, ZUSD_COMMAND_TYPE_V1, ZUSD_STATE_TYPE_V1, ZenoDexProfileV1, ZusdCommandTagV1,
    ZusdCommandV1, ZusdRejectV1, ZusdStateError, ZusdStateFieldV1, ZusdStateV1,
    zusd_precedence_hash_v1,
};
use zeno_fcis_receipt::{CandidateBindings, CandidateBuilder, RejectReceipt, SealError};
use zeno_fcis_refine::{NormalizedDecision, RefineError};
use zeno_fcis_value::{Field, TextError, Value, ValueError};

/// Exact ZenoDEX revision mounted by the v1 evidence profile.
pub const PINNED_ZENODEX_COMMIT: &str = "e3cf1aad40e487893230ae0c55f1f0dda62f9955";
/// Parent revision containing the unchanged Rust and Python zUSD transitions.
pub const PINNED_ZENODEX_TRANSITION_COMMIT: &str = "e0122cd1fa2aa6a4c5da594c99c6e7d6edf04ce2";
/// Exact Python authority entry point in the pinned ZenoDEX tree.
pub const PINNED_PYTHON_ENTRYPOINT: &str = "tools/runtime/zusd_fcis_op.py";
/// Exact Rust runtime subcommand in the pinned ZenoDEX tree.
pub const PINNED_RUST_SUBCOMMAND: &str = "zusd-fcis-op";
/// Maximum canonical native request or response line.
pub const MAX_NATIVE_LINE_BYTES: usize = 64 * 1024;
/// Maximum patch operations emitted by this profile.
pub const MAX_MOUNT_PATCH_OPERATIONS: u128 = 1;
/// Maximum commit-plan effects emitted by this evidence profile.
pub const MAX_MOUNT_EFFECTS: u128 = 0;
/// Maximum outbox entries emitted by this evidence profile.
pub const MAX_MOUNT_OUTBOX_ENTRIES: u128 = 0;

/// Complete immutable input to one pinned zUSD mount decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZusdMountInputV1 {
    state: ZusdStateV1,
    command: ZusdCommandV1,
}

impl ZusdMountInputV1 {
    /// Creates one mounted input. Runtime revision and policy are pinned by the profile.
    #[must_use]
    pub const fn new(state: ZusdStateV1, command: ZusdCommandV1) -> Self {
        Self { state, command }
    }

    /// Returns the exact pre-state.
    #[must_use]
    pub const fn state(&self) -> &ZusdStateV1 {
        &self.state
    }

    /// Returns the exact command.
    #[must_use]
    pub const fn command(&self) -> ZusdCommandV1 {
        self.command
    }
}

/// Fixed commitments shared by every case in the pinned zUSD mount.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ZusdMountBindingsV1 {
    profile: ZenoDexProfileV1,
    profile_hash: Hash32,
    schema_hash: Hash32,
    precedence_hash: Hash32,
    algorithm_hash: Hash32,
    budget_hash: Hash32,
    context_hash: Hash32,
}

impl ZusdMountBindingsV1 {
    /// Constructs all fixed identities from closed canonical values.
    pub fn pinned() -> Result<Self, ZusdMountError> {
        let schema_hash = schema_hash()?;
        let precedence_hash =
            zusd_precedence_hash_v1::<RustCryptoSha256>().map_err(ZusdMountError::Profile)?;
        let algorithm_hash = algorithm_hash()?;
        let profile = ZenoDexProfileV1::zusd(schema_hash, precedence_hash, algorithm_hash);
        let profile_hash = profile
            .commitment::<RustCryptoSha256>()
            .map_err(ZusdMountError::Profile)?;
        Ok(Self {
            profile,
            profile_hash,
            schema_hash,
            precedence_hash,
            algorithm_hash,
            budget_hash: hash_canonical("zeno-fcis/zenodex/zusd/mount-budget", &budget_value()?)?,
            context_hash: hash_canonical(
                "zeno-fcis/zenodex/zusd/mount-context",
                &context_value()?,
            )?,
        })
    }

    /// Returns the mounted protocol profile.
    #[must_use]
    pub const fn profile(self) -> ZenoDexProfileV1 {
        self.profile
    }

    /// Returns the complete profile commitment.
    #[must_use]
    pub const fn profile_hash(self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the state/command schema commitment.
    #[must_use]
    pub const fn schema_hash(self) -> Hash32 {
        self.schema_hash
    }

    /// Returns the stable rejection-precedence commitment.
    #[must_use]
    pub const fn precedence_hash(self) -> Hash32 {
        self.precedence_hash
    }

    /// Returns the codec, hashing, patching, and sealing algorithm commitment.
    #[must_use]
    pub const fn algorithm_hash(self) -> Hash32 {
        self.algorithm_hash
    }

    /// Returns the deterministic resource-budget commitment.
    #[must_use]
    pub const fn budget_hash(self) -> Hash32 {
        self.budget_hash
    }

    /// Returns the pinned runtime revision and policy commitment.
    #[must_use]
    pub const fn context_hash(self) -> Hash32 {
        self.context_hash
    }
}

/// Validated native ZenoDEX result before ZenoFCIS candidate sealing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeZusdDecisionV1 {
    input_id: Hash32,
    accept: bool,
    reject_reason: Option<Box<str>>,
    receipt_hash: Option<Hash32>,
    receipt_tag: Option<Box<str>>,
    pre_state_root: Hash32,
    post_state_root: Hash32,
    post_state: ZusdStateV1,
}

impl NativeZusdDecisionV1 {
    /// Returns whether the native transition accepted.
    #[must_use]
    pub const fn accepted(&self) -> bool {
        self.accept
    }

    /// Returns the stable rejection reason, if any.
    #[must_use]
    pub fn reject_reason(&self) -> Option<&str> {
        self.reject_reason.as_deref()
    }

    /// Returns the validated native successor state.
    #[must_use]
    pub const fn post_state(&self) -> &ZusdStateV1 {
        &self.post_state
    }
}

/// Encodes the sole accepted native zUSD request-line representation.
pub fn encode_zusd_native_request_line_v1(
    input: &ZusdMountInputV1,
) -> Result<Vec<u8>, ZusdMountError> {
    let wire = WireRequest {
        version: 1,
        state: WireInputState(input.state()),
        tx: WireCommand::from_command(input.command()),
        require_oracle_authorization: false,
    };
    encode_native_line(&wire)
}

/// Strictly decodes and independently validates one canonical native result line.
pub fn decode_zusd_native_decision_line_v1(
    bytes: &[u8],
    input: &ZusdMountInputV1,
) -> Result<NativeZusdDecisionV1, ZusdMountError> {
    validate_native_line(bytes)?;
    let payload = &bytes[..bytes.len() - 1];
    let wire: WireDecision = serde_json::from_slice(payload).map_err(ZusdMountError::Json)?;
    let canonical = encode_native_line(&wire)?;
    if canonical != bytes {
        return Err(ZusdMountError::NonCanonicalJson);
    }
    if wire.version != 1 || wire.kernel != "zusd" {
        return Err(ZusdMountError::InvalidHeader);
    }
    let post_state = wire.post_state.into_state()?;
    let expected_pre = native_state_root(input.state());
    let expected_post = native_state_root(&post_state);
    let pre_state_root = parse_native_hash(&wire.pre_state_root)?;
    let post_state_root = parse_native_hash(&wire.post_state_root)?;
    if pre_state_root != expected_pre || post_state_root != expected_post {
        return Err(ZusdMountError::NativeRootMismatch);
    }

    let expected_tag = input.command().tag().native_name();
    let (reject_reason, receipt_hash, receipt_tag) = if wire.accept {
        if wire.reject_reason.is_some() || wire.receipt_hash.is_none() || wire.receipt.is_none() {
            return Err(ZusdMountError::InvalidDecisionShape);
        }
        let receipt = wire.receipt.ok_or(ZusdMountError::InvalidDecisionShape)?;
        if receipt.tag != expected_tag {
            return Err(ZusdMountError::NativeReceiptMismatch);
        }
        let actual_receipt = parse_native_hash(
            wire.receipt_hash
                .as_deref()
                .ok_or(ZusdMountError::InvalidDecisionShape)?,
        )?;
        if actual_receipt != native_receipt_hash(expected_tag, post_state_root) {
            return Err(ZusdMountError::NativeReceiptMismatch);
        }
        (
            None,
            Some(actual_receipt),
            Some(receipt.tag.into_boxed_str()),
        )
    } else {
        let reason = wire
            .reject_reason
            .ok_or(ZusdMountError::InvalidDecisionShape)?;
        if wire.receipt_hash.is_some() || wire.receipt.is_some() || post_state != *input.state() {
            return Err(ZusdMountError::InvalidDecisionShape);
        }
        if ZusdRejectV1::from_code(&reason).is_none() {
            return Err(ZusdMountError::UnknownReason(reason));
        }
        (Some(reason.into_boxed_str()), None, None)
    };

    Ok(NativeZusdDecisionV1 {
        input_id: zusd_mount_case_id_v1(input)?,
        accept: wire.accept,
        reject_reason,
        receipt_hash,
        receipt_tag,
        pre_state_root,
        post_state_root,
        post_state,
    })
}

/// Converts a validated native result into the complete ZenoFCIS decision surface.
pub fn normalize_zusd_native_decision_v1(
    input: &ZusdMountInputV1,
    native: &NativeZusdDecisionV1,
) -> Result<NormalizedDecision, ZusdMountError> {
    if native.input_id != zusd_mount_case_id_v1(input)? {
        return Err(ZusdMountError::NativeInputMismatch);
    }
    let fixed = ZusdMountBindingsV1::pinned()?;
    let command_value = input.command().to_value().map_err(ZusdMountError::State)?;
    let command_hash = hash_value::<RustCryptoSha256>(
        Domain::new(
            fixed.profile().command_domain(),
            fixed.profile().profile_version(),
        )
        .map_err(ZusdMountError::Encode)?,
        &command_value,
    )
    .map_err(ZusdMountError::Patch)?;
    let bindings = CandidateBindings {
        profile_hash: fixed.profile_hash(),
        command_hash,
        context_hash: fixed.context_hash(),
        precedence_hash: fixed.precedence_hash(),
        algorithm_hash: fixed.algorithm_hash(),
        budget_hash: fixed.budget_hash(),
    };
    let pre_state = input.state().to_value().map_err(ZusdMountError::State)?;
    let state_domain = Domain::new(
        fixed.profile().state_domain(),
        fixed.profile().profile_version(),
    )
    .map_err(ZusdMountError::Encode)?;
    let pre_root =
        hash_value::<RustCryptoSha256>(state_domain, &pre_state).map_err(ZusdMountError::Patch)?;

    if !native.accept {
        let reason = native
            .reject_reason()
            .ok_or(ZusdMountError::InvalidDecisionShape)?;
        let receipt =
            RejectReceipt::new(bindings, pre_root, reason).map_err(ZusdMountError::Seal)?;
        return NormalizedDecision::from_reject(&receipt).map_err(ZusdMountError::Refine);
    }

    if native.receipt_hash.is_none() || native.receipt_tag.is_none() {
        return Err(ZusdMountError::InvalidDecisionShape);
    }
    let post_state = native
        .post_state
        .to_value()
        .map_err(ZusdMountError::State)?;
    let old_hash = hash_value::<RustCryptoSha256>(
        Domain::new("zeno-fcis/value", 1).map_err(ZusdMountError::Encode)?,
        &pre_state,
    )
    .map_err(ZusdMountError::Patch)?;
    let patch = CanonicalPatch::try_new(
        ZUSD_STATE_TYPE_V1,
        pre_root,
        vec![PatchOp::Update {
            path: ValuePath::new(Vec::new()),
            expected_old_hash: old_hash,
            value: post_state,
        }],
    )
    .map_err(ZusdMountError::Patch)?;
    let bundle = CandidateBuilder::seal::<RustCryptoSha256>(
        &pre_state,
        state_domain,
        DecisionKind::Accept,
        None,
        bindings,
        patch,
        CommitPlan::empty(),
        OutboxPlan::empty(),
    )
    .map_err(ZusdMountError::Seal)?;
    NormalizedDecision::from_bundle(&bundle).map_err(ZusdMountError::Refine)
}

/// Computes the canonical case identity from the exact native request line.
pub fn zusd_mount_case_id_v1(input: &ZusdMountInputV1) -> Result<Hash32, ZusdMountError> {
    let request = encode_zusd_native_request_line_v1(input)?;
    hash_bytes("zeno-fcis/zenodex/zusd/mount-case", &request)
}

fn schema_hash() -> Result<Hash32, ZusdMountError> {
    let state_fields = ZusdStateFieldV1::ALL
        .into_iter()
        .map(|field| Value::U128(u128::from(field.id())))
        .collect();
    let command_tags = [
        ZusdCommandTagV1::AdvanceEpochLegacy,
        ZusdCommandTagV1::BootstrapOracle,
        ZusdCommandTagV1::OracleReport,
        ZusdCommandTagV1::OracleCommit,
        ZusdCommandTagV1::DepositCollateral,
        ZusdCommandTagV1::WithdrawCollateral,
        ZusdCommandTagV1::MintZusd,
        ZusdCommandTagV1::RepayZusd,
        ZusdCommandTagV1::DepositStabilityPool,
        ZusdCommandTagV1::WithdrawStabilityPool,
        ZusdCommandTagV1::RedeemZusd,
        ZusdCommandTagV1::Liquidate,
    ]
    .into_iter()
    .map(|tag| {
        Ok(Value::tuple(vec![
            Value::U128(u128::from(tag as u8)),
            Value::text_ascii(tag.native_name().to_owned()).map_err(ZusdMountError::Text)?,
        ]))
    })
    .collect::<Result<Vec<_>, ZusdMountError>>()?;
    let value = Value::record_canonical(vec![
        Field::new(0, Value::U128(u128::from(ZUSD_STATE_TYPE_V1))),
        Field::new(1, Value::vector(state_fields)),
        Field::new(2, Value::U128(u128::from(ZUSD_COMMAND_TYPE_V1))),
        Field::new(3, Value::vector(command_tags)),
    ])
    .map_err(ZusdMountError::Value)?;
    hash_canonical("zeno-fcis/zenodex/zusd/mount-schema", &value)
}

fn algorithm_hash() -> Result<Hash32, ZusdMountError> {
    let value = Value::record_canonical(vec![
        Field::new(
            0,
            Value::text_ascii("ZCVE/1".to_owned()).map_err(ZusdMountError::Text)?,
        ),
        Field::new(
            1,
            Value::text_ascii(<RustCryptoSha256 as CommitmentHasher>::ALGORITHM_ID.to_owned())
                .map_err(ZusdMountError::Text)?,
        ),
        Field::new(
            2,
            Value::text_ascii("root-replacement-patch/v1".to_owned())
                .map_err(ZusdMountError::Text)?,
        ),
        Field::new(
            3,
            Value::text_ascii("candidate-seal/v1".to_owned()).map_err(ZusdMountError::Text)?,
        ),
    ])
    .map_err(ZusdMountError::Value)?;
    hash_canonical("zeno-fcis/zenodex/zusd/mount-algorithm", &value)
}

fn budget_value() -> Result<Value, ZusdMountError> {
    Value::record_canonical(vec![
        Field::new(0, Value::U128(ZusdStateFieldV1::COUNT as u128)),
        Field::new(1, Value::U128(MAX_MOUNT_PATCH_OPERATIONS)),
        Field::new(2, Value::U128(MAX_MOUNT_EFFECTS)),
        Field::new(3, Value::U128(MAX_MOUNT_OUTBOX_ENTRIES)),
        Field::new(4, Value::U128(MAX_NATIVE_LINE_BYTES as u128)),
    ])
    .map_err(ZusdMountError::Value)
}

fn context_value() -> Result<Value, ZusdMountError> {
    Value::record_canonical(vec![
        Field::new(
            0,
            Value::text_ascii(PINNED_ZENODEX_COMMIT.to_owned()).map_err(ZusdMountError::Text)?,
        ),
        Field::new(
            1,
            Value::text_ascii(PINNED_ZENODEX_TRANSITION_COMMIT.to_owned())
                .map_err(ZusdMountError::Text)?,
        ),
        Field::new(2, Value::Bool(false)),
        Field::new(
            3,
            Value::text_ascii(PINNED_PYTHON_ENTRYPOINT.to_owned()).map_err(ZusdMountError::Text)?,
        ),
        Field::new(
            4,
            Value::text_ascii(PINNED_RUST_SUBCOMMAND.to_owned()).map_err(ZusdMountError::Text)?,
        ),
    ])
    .map_err(ZusdMountError::Value)
}

fn hash_canonical(domain: &'static str, value: &Value) -> Result<Hash32, ZusdMountError> {
    let bytes = value.canonical_bytes().map_err(ZusdMountError::Encode)?;
    hash_bytes(domain, &bytes)
}

fn hash_bytes(domain: &'static str, bytes: &[u8]) -> Result<Hash32, ZusdMountError> {
    commitment::<RustCryptoSha256>(
        Domain::new(domain, 1).map_err(ZusdMountError::Encode)?,
        bytes,
    )
    .map_err(ZusdMountError::Encode)
}

fn validate_native_line(bytes: &[u8]) -> Result<(), ZusdMountError> {
    if bytes.is_empty() || bytes.len() > MAX_NATIVE_LINE_BYTES {
        return Err(ZusdMountError::LineLength);
    }
    if bytes.last() != Some(&b'\n')
        || bytes[..bytes.len() - 1]
            .iter()
            .any(|byte| matches!(byte, b'\n' | b'\r'))
    {
        return Err(ZusdMountError::NotSingleLine);
    }
    Ok(())
}

fn encode_native_line<T: Serialize>(value: &T) -> Result<Vec<u8>, ZusdMountError> {
    let mut bytes = serde_json::to_vec(value).map_err(ZusdMountError::Json)?;
    bytes.push(b'\n');
    validate_native_line(&bytes)?;
    Ok(bytes)
}

fn native_state_root(state: &ZusdStateV1) -> Hash32 {
    let mut bytes = native_domain("zusd_state", 1);
    for field in ZusdStateFieldV1::ALL {
        bytes.extend(native_uvarint(state.field(field)));
    }
    <RustCryptoSha256 as CommitmentHasher>::hash(&bytes)
}

fn native_receipt_hash(tag: &str, post_root: Hash32) -> Hash32 {
    let mut bytes = native_domain("zusd_receipt", 1);
    bytes.extend_from_slice(b"TAG");
    bytes.extend(native_blob(tag.as_bytes()));
    bytes.extend_from_slice(b"RT");
    bytes.extend(native_blob(post_root.as_bytes()));
    <RustCryptoSha256 as CommitmentHasher>::hash(&bytes)
}

fn native_domain(label: &str, version: u32) -> Vec<u8> {
    let mut bytes = b"zenodex:".to_vec();
    bytes.extend_from_slice(label.as_bytes());
    bytes.extend_from_slice(b":v");
    bytes.extend_from_slice(version.to_string().as_bytes());
    bytes.push(0);
    bytes
}

fn native_blob(bytes: &[u8]) -> Vec<u8> {
    let mut output = native_uvarint(bytes.len() as u128);
    output.extend_from_slice(bytes);
    output
}

fn native_uvarint(mut value: u128) -> Vec<u8> {
    let mut output = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return output;
        }
    }
}

fn parse_native_hash(value: &str) -> Result<Hash32, ZusdMountError> {
    let body = value
        .strip_prefix("0x")
        .ok_or(ZusdMountError::InvalidNativeHash)?;
    if body.len() != 64 {
        return Err(ZusdMountError::InvalidNativeHash);
    }
    let mut output = [0_u8; 32];
    for (index, pair) in body.as_bytes().chunks_exact(2).enumerate() {
        let high = native_hex_nibble(pair[0]).ok_or(ZusdMountError::InvalidNativeHash)?;
        let low = native_hex_nibble(pair[1]).ok_or(ZusdMountError::InvalidNativeHash)?;
        output[index] = (high << 4) | low;
    }
    Ok(Hash32::new(output))
}

const fn native_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct WireRequest<'a> {
    version: u32,
    state: WireInputState<'a>,
    tx: WireCommand,
    require_oracle_authorization: bool,
}

struct WireInputState<'a>(&'a ZusdStateV1);

impl Serialize for WireInputState<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let state = self.0;
        let mut output = serializer.serialize_struct("ZusdState", ZusdStateFieldV1::COUNT)?;
        output.serialize_field("now_epoch", &state.field(ZusdStateFieldV1::NowEpoch))?;
        output.serialize_field("oracle_seen", &state.oracle_seen())?;
        output.serialize_field(
            "oracle_last_update_epoch",
            &state.field(ZusdStateFieldV1::OracleLastUpdateEpoch),
        )?;
        output.serialize_field("price_e8", &state.field(ZusdStateFieldV1::PriceE8))?;
        output.serialize_field(
            "price_pending_e8",
            &state.field(ZusdStateFieldV1::PricePendingE8),
        )?;
        output.serialize_field(
            "max_oracle_staleness_epochs",
            &state.field(ZusdStateFieldV1::MaxOracleStalenessEpochs),
        )?;
        output.serialize_field(
            "collateral_e8",
            &state.field(ZusdStateFieldV1::CollateralE8),
        )?;
        output.serialize_field("debt_e8", &state.field(ZusdStateFieldV1::DebtE8))?;
        output.serialize_field("free_debt_e8", &state.field(ZusdStateFieldV1::FreeDebtE8))?;
        output.serialize_field(
            "sp_debt_e8",
            &state.field(ZusdStateFieldV1::StabilityPoolDebtE8),
        )?;
        output.serialize_field(
            "sp_coll_e8",
            &state.field(ZusdStateFieldV1::StabilityPoolCollateralE8),
        )?;
        output.serialize_field(
            "protocol_collateral_e8",
            &state.field(ZusdStateFieldV1::ProtocolCollateralE8),
        )?;
        output.serialize_field(
            "protocol_revenue_zusd_cum_e8",
            &state.field(ZusdStateFieldV1::ProtocolRevenueZusdCumulativeE8),
        )?;
        output.serialize_field(
            "liquidator_compensation_collateral_cum_e8",
            &state.field(ZusdStateFieldV1::LiquidatorCompensationCollateralCumulativeE8),
        )?;
        output.serialize_field("mcr_bps", &state.field(ZusdStateFieldV1::McrBps))?;
        output.serialize_field("ccr_bps", &state.field(ZusdStateFieldV1::CcrBps))?;
        output.serialize_field(
            "min_debt_open_e8",
            &state.field(ZusdStateFieldV1::MinDebtOpenE8),
        )?;
        output.serialize_field("max_debt_e8", &state.field(ZusdStateFieldV1::MaxDebtE8))?;
        output.serialize_field(
            "max_debt_supply_e8",
            &state.field(ZusdStateFieldV1::MaxDebtSupplyE8),
        )?;
        output.serialize_field(
            "max_sp_coll_e8",
            &state.field(ZusdStateFieldV1::MaxStabilityPoolCollateralE8),
        )?;
        output.serialize_field(
            "max_protocol_coll_e8",
            &state.field(ZusdStateFieldV1::MaxProtocolCollateralE8),
        )?;
        output.serialize_field("base_rate_bps", &state.field(ZusdStateFieldV1::BaseRateBps))?;
        output.serialize_field(
            "base_rate_last_epoch",
            &state.field(ZusdStateFieldV1::BaseRateLastEpoch),
        )?;
        output.serialize_field(
            "base_rate_decay_per_epoch_bps",
            &state.field(ZusdStateFieldV1::BaseRateDecayPerEpochBps),
        )?;
        output.serialize_field(
            "base_rate_borrow_bump_bps",
            &state.field(ZusdStateFieldV1::BaseRateBorrowBumpBps),
        )?;
        output.serialize_field(
            "base_rate_redeem_bump_bps",
            &state.field(ZusdStateFieldV1::BaseRateRedeemBumpBps),
        )?;
        output.serialize_field(
            "borrow_fee_floor_bps",
            &state.field(ZusdStateFieldV1::BorrowFeeFloorBps),
        )?;
        output.serialize_field(
            "borrow_fee_max_bps",
            &state.field(ZusdStateFieldV1::BorrowFeeMaxBps),
        )?;
        output.serialize_field(
            "redemption_fee_floor_bps",
            &state.field(ZusdStateFieldV1::RedemptionFeeFloorBps),
        )?;
        output.serialize_field(
            "redemption_fee_max_bps",
            &state.field(ZusdStateFieldV1::RedemptionFeeMaxBps),
        )?;
        output.serialize_field(
            "liquidation_gas_comp_fixed_collateral_e8",
            &state.field(ZusdStateFieldV1::LiquidationGasCompensationFixedCollateralE8),
        )?;
        output.serialize_field(
            "liquidation_gas_comp_bps",
            &state.field(ZusdStateFieldV1::LiquidationGasCompensationBps),
        )?;
        output.end()
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum WireCommand {
    #[serde(rename = "advance_epoch")]
    AdvanceEpoch { delta: u128 },
    #[serde(rename = "bootstrap_oracle")]
    BootstrapOracle { auth_ok: bool, price_e8: u128 },
    #[serde(rename = "oracle_report")]
    OracleReport { auth_ok: bool, price_e8: u128 },
    #[serde(rename = "oracle_commit")]
    OracleCommit { auth_ok: bool },
    #[serde(rename = "deposit_collateral")]
    DepositCollateral { amount_e8: u128 },
    #[serde(rename = "withdraw_collateral")]
    WithdrawCollateral { amount_e8: u128 },
    #[serde(rename = "mint_zusd")]
    MintZusd { amount_e8: u128 },
    #[serde(rename = "repay_zusd")]
    RepayZusd { amount_e8: u128 },
    #[serde(rename = "deposit_sp")]
    DepositSp { amount_e8: u128 },
    #[serde(rename = "withdraw_sp")]
    WithdrawSp { amount_e8: u128 },
    #[serde(rename = "redeem_zusd")]
    RedeemZusd { amount_e8: u128 },
    #[serde(rename = "liquidate")]
    Liquidate,
}

impl WireCommand {
    const fn from_command(command: ZusdCommandV1) -> Self {
        match command {
            ZusdCommandV1::AdvanceEpochLegacy { delta } => Self::AdvanceEpoch { delta },
            ZusdCommandV1::BootstrapOracle { auth_ok, price_e8 } => {
                Self::BootstrapOracle { auth_ok, price_e8 }
            }
            ZusdCommandV1::OracleReport { auth_ok, price_e8 } => {
                Self::OracleReport { auth_ok, price_e8 }
            }
            ZusdCommandV1::OracleCommit { auth_ok } => Self::OracleCommit { auth_ok },
            ZusdCommandV1::DepositCollateral { amount_e8 } => Self::DepositCollateral { amount_e8 },
            ZusdCommandV1::WithdrawCollateral { amount_e8 } => {
                Self::WithdrawCollateral { amount_e8 }
            }
            ZusdCommandV1::MintZusd { amount_e8 } => Self::MintZusd { amount_e8 },
            ZusdCommandV1::RepayZusd { amount_e8 } => Self::RepayZusd { amount_e8 },
            ZusdCommandV1::DepositStabilityPool { amount_e8 } => Self::DepositSp { amount_e8 },
            ZusdCommandV1::WithdrawStabilityPool { amount_e8 } => Self::WithdrawSp { amount_e8 },
            ZusdCommandV1::RedeemZusd { amount_e8 } => Self::RedeemZusd { amount_e8 },
            ZusdCommandV1::Liquidate => Self::Liquidate,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WireDecision {
    version: u32,
    kernel: String,
    accept: bool,
    reject_reason: Option<String>,
    receipt_hash: Option<String>,
    receipt: Option<WireReceipt>,
    pre_state_root: String,
    post_state_root: String,
    post_state: WireState,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WireReceipt {
    tag: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WireState {
    now_epoch: String,
    oracle_seen: bool,
    oracle_last_update_epoch: String,
    price_e8: String,
    price_pending_e8: String,
    max_oracle_staleness_epochs: String,
    collateral_e8: String,
    debt_e8: String,
    free_debt_e8: String,
    sp_debt_e8: String,
    sp_coll_e8: String,
    protocol_collateral_e8: String,
    protocol_revenue_zusd_cum_e8: String,
    liquidator_compensation_collateral_cum_e8: String,
    mcr_bps: String,
    ccr_bps: String,
    min_debt_open_e8: String,
    max_debt_e8: String,
    max_debt_supply_e8: String,
    max_sp_coll_e8: String,
    max_protocol_coll_e8: String,
    base_rate_bps: String,
    base_rate_last_epoch: String,
    base_rate_decay_per_epoch_bps: String,
    base_rate_borrow_bump_bps: String,
    base_rate_redeem_bump_bps: String,
    borrow_fee_floor_bps: String,
    borrow_fee_max_bps: String,
    redemption_fee_floor_bps: String,
    redemption_fee_max_bps: String,
    liquidation_gas_comp_fixed_collateral_e8: String,
    liquidation_gas_comp_bps: String,
}

impl WireState {
    #[cfg(test)]
    fn from_state(state: &ZusdStateV1) -> Self {
        let get = |field| state.field(field).to_string();
        Self {
            now_epoch: get(ZusdStateFieldV1::NowEpoch),
            oracle_seen: state.oracle_seen(),
            oracle_last_update_epoch: get(ZusdStateFieldV1::OracleLastUpdateEpoch),
            price_e8: get(ZusdStateFieldV1::PriceE8),
            price_pending_e8: get(ZusdStateFieldV1::PricePendingE8),
            max_oracle_staleness_epochs: get(ZusdStateFieldV1::MaxOracleStalenessEpochs),
            collateral_e8: get(ZusdStateFieldV1::CollateralE8),
            debt_e8: get(ZusdStateFieldV1::DebtE8),
            free_debt_e8: get(ZusdStateFieldV1::FreeDebtE8),
            sp_debt_e8: get(ZusdStateFieldV1::StabilityPoolDebtE8),
            sp_coll_e8: get(ZusdStateFieldV1::StabilityPoolCollateralE8),
            protocol_collateral_e8: get(ZusdStateFieldV1::ProtocolCollateralE8),
            protocol_revenue_zusd_cum_e8: get(ZusdStateFieldV1::ProtocolRevenueZusdCumulativeE8),
            liquidator_compensation_collateral_cum_e8: get(
                ZusdStateFieldV1::LiquidatorCompensationCollateralCumulativeE8,
            ),
            mcr_bps: get(ZusdStateFieldV1::McrBps),
            ccr_bps: get(ZusdStateFieldV1::CcrBps),
            min_debt_open_e8: get(ZusdStateFieldV1::MinDebtOpenE8),
            max_debt_e8: get(ZusdStateFieldV1::MaxDebtE8),
            max_debt_supply_e8: get(ZusdStateFieldV1::MaxDebtSupplyE8),
            max_sp_coll_e8: get(ZusdStateFieldV1::MaxStabilityPoolCollateralE8),
            max_protocol_coll_e8: get(ZusdStateFieldV1::MaxProtocolCollateralE8),
            base_rate_bps: get(ZusdStateFieldV1::BaseRateBps),
            base_rate_last_epoch: get(ZusdStateFieldV1::BaseRateLastEpoch),
            base_rate_decay_per_epoch_bps: get(ZusdStateFieldV1::BaseRateDecayPerEpochBps),
            base_rate_borrow_bump_bps: get(ZusdStateFieldV1::BaseRateBorrowBumpBps),
            base_rate_redeem_bump_bps: get(ZusdStateFieldV1::BaseRateRedeemBumpBps),
            borrow_fee_floor_bps: get(ZusdStateFieldV1::BorrowFeeFloorBps),
            borrow_fee_max_bps: get(ZusdStateFieldV1::BorrowFeeMaxBps),
            redemption_fee_floor_bps: get(ZusdStateFieldV1::RedemptionFeeFloorBps),
            redemption_fee_max_bps: get(ZusdStateFieldV1::RedemptionFeeMaxBps),
            liquidation_gas_comp_fixed_collateral_e8: get(
                ZusdStateFieldV1::LiquidationGasCompensationFixedCollateralE8,
            ),
            liquidation_gas_comp_bps: get(ZusdStateFieldV1::LiquidationGasCompensationBps),
        }
    }

    fn into_state(self) -> Result<ZusdStateV1, ZusdMountError> {
        let values = [
            parse_u128(&self.now_epoch)?,
            u128::from(self.oracle_seen),
            parse_u128(&self.oracle_last_update_epoch)?,
            parse_u128(&self.price_e8)?,
            parse_u128(&self.price_pending_e8)?,
            parse_u128(&self.max_oracle_staleness_epochs)?,
            parse_u128(&self.collateral_e8)?,
            parse_u128(&self.debt_e8)?,
            parse_u128(&self.free_debt_e8)?,
            parse_u128(&self.sp_debt_e8)?,
            parse_u128(&self.sp_coll_e8)?,
            parse_u128(&self.protocol_collateral_e8)?,
            parse_u128(&self.protocol_revenue_zusd_cum_e8)?,
            parse_u128(&self.liquidator_compensation_collateral_cum_e8)?,
            parse_u128(&self.mcr_bps)?,
            parse_u128(&self.ccr_bps)?,
            parse_u128(&self.min_debt_open_e8)?,
            parse_u128(&self.max_debt_e8)?,
            parse_u128(&self.max_debt_supply_e8)?,
            parse_u128(&self.max_sp_coll_e8)?,
            parse_u128(&self.max_protocol_coll_e8)?,
            parse_u128(&self.base_rate_bps)?,
            parse_u128(&self.base_rate_last_epoch)?,
            parse_u128(&self.base_rate_decay_per_epoch_bps)?,
            parse_u128(&self.base_rate_borrow_bump_bps)?,
            parse_u128(&self.base_rate_redeem_bump_bps)?,
            parse_u128(&self.borrow_fee_floor_bps)?,
            parse_u128(&self.borrow_fee_max_bps)?,
            parse_u128(&self.redemption_fee_floor_bps)?,
            parse_u128(&self.redemption_fee_max_bps)?,
            parse_u128(&self.liquidation_gas_comp_fixed_collateral_e8)?,
            parse_u128(&self.liquidation_gas_comp_bps)?,
        ];
        ZusdStateV1::try_from_fields(values).map_err(ZusdMountError::State)
    }
}

fn parse_u128(value: &str) -> Result<u128, ZusdMountError> {
    if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
        return Err(ZusdMountError::InvalidNativeInteger);
    }
    value
        .parse::<u128>()
        .map_err(|_| ZusdMountError::InvalidNativeInteger)
}

/// Concrete mounted-profile failure.
#[derive(Debug)]
pub enum ZusdMountError {
    /// Native line exceeds its explicit byte bound.
    LineLength,
    /// Native transport is not exactly one LF-terminated line.
    NotSingleLine,
    /// Native JSON syntax or exact-field validation failed.
    Json(serde_json::Error),
    /// Native JSON has an alternate byte representation.
    NonCanonicalJson,
    /// Native version or kernel identity differs.
    InvalidHeader,
    /// Native decision mixes accept and reject fields.
    InvalidDecisionShape,
    /// Native fixed-width lowercase hash is malformed.
    InvalidNativeHash,
    /// Native state root disagrees with the independently computed root.
    NativeRootMismatch,
    /// Native receipt tag or hash disagrees with the independently computed receipt.
    NativeReceiptMismatch,
    /// Decoded native decision was produced for a different canonical request.
    NativeInputMismatch,
    /// Native decimal integer is noncanonical or out of range.
    InvalidNativeInteger,
    /// Native runtime returned a reason outside the pinned registry.
    UnknownReason(String),
    /// Profile construction failed.
    Profile(ProfileError),
    /// State construction failed.
    State(ZusdStateError),
    /// ASCII construction failed.
    Text(TextError),
    /// Canonical value construction failed.
    Value(ValueError),
    /// Canonical encoding or hashing failed.
    Encode(EncodeError),
    /// Patch construction or validation failed.
    Patch(PatchError),
    /// Candidate sealing failed.
    Seal(SealError),
    /// Complete decision normalization failed.
    Refine(RefineError),
}

impl fmt::Display for ZusdMountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LineLength => formatter.write_str("native JSON line exceeds its bound"),
            Self::NotSingleLine => {
                formatter.write_str("native output is not one LF-terminated line")
            }
            Self::Json(error) => write!(formatter, "native JSON failed strict decoding: {error}"),
            Self::NonCanonicalJson => formatter.write_str("native JSON line is not byte-canonical"),
            Self::InvalidHeader => {
                formatter.write_str("native zUSD version or kernel identity differs")
            }
            Self::InvalidDecisionShape => {
                formatter.write_str("native zUSD decision has an invalid shape")
            }
            Self::InvalidNativeHash => formatter.write_str("native zUSD hash is not canonical"),
            Self::NativeRootMismatch => formatter.write_str("native zUSD state root mismatch"),
            Self::NativeReceiptMismatch => formatter.write_str("native zUSD receipt mismatch"),
            Self::NativeInputMismatch => {
                formatter.write_str("native zUSD decision input identity mismatch")
            }
            Self::InvalidNativeInteger => {
                formatter.write_str("native zUSD integer is not canonical")
            }
            Self::UnknownReason(reason) => {
                write!(formatter, "unknown native zUSD rejection: {reason}")
            }
            Self::Profile(error) => error.fmt(formatter),
            Self::State(error) => error.fmt(formatter),
            Self::Text(error) => error.fmt(formatter),
            Self::Value(error) => error.fmt(formatter),
            Self::Encode(error) => error.fmt(formatter),
            Self::Patch(error) => error.fmt(formatter),
            Self::Seal(error) => error.fmt(formatter),
            Self::Refine(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ZusdMountError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn reject_line(input: &ZusdMountInputV1, reason: &str) -> Vec<u8> {
        let state = WireState::from_state(input.state());
        encode_native_line(&WireDecision {
            version: 1,
            kernel: "zusd".to_owned(),
            accept: false,
            reject_reason: Some(reason.to_owned()),
            receipt_hash: None,
            receipt: None,
            pre_state_root: format!("0x{}", input_native_root_hex(input.state())),
            post_state_root: format!("0x{}", input_native_root_hex(input.state())),
            post_state: state,
        })
        .unwrap_or_else(|error| panic!("wire: {error}"))
    }

    fn input_native_root_hex(state: &ZusdStateV1) -> String {
        let hash = native_state_root(state);
        hash.as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn rejection_normalizes_without_candidate_artifacts() {
        let state =
            ZusdStateV1::reference_initial().unwrap_or_else(|error| panic!("state: {error}"));
        let input = ZusdMountInputV1::new(state, ZusdCommandV1::MintZusd { amount_e8: 1 });
        let native = decode_zusd_native_decision_line_v1(
            &reject_line(&input, "mint_blocked_oracle"),
            &input,
        )
        .unwrap_or_else(|error| panic!("decode: {error}"));
        let decision = normalize_zusd_native_decision_v1(&input, &native)
            .unwrap_or_else(|error| panic!("normalize: {error}"));
        assert_eq!(decision.artifacts().kind, DecisionKind::Reject);
        assert!(decision.artifacts().candidate_id.is_none());
    }

    #[test]
    fn decoded_decision_cannot_be_normalized_for_another_input() {
        let state =
            ZusdStateV1::reference_initial().unwrap_or_else(|error| panic!("state: {error}"));
        let decoded_input =
            ZusdMountInputV1::new(state.clone(), ZusdCommandV1::MintZusd { amount_e8: 1 });
        let other_input = ZusdMountInputV1::new(state, ZusdCommandV1::Liquidate);
        let native = decode_zusd_native_decision_line_v1(
            &reject_line(&decoded_input, "mint_blocked_oracle"),
            &decoded_input,
        )
        .unwrap_or_else(|error| panic!("decode: {error}"));
        assert!(matches!(
            normalize_zusd_native_decision_v1(&other_input, &native),
            Err(ZusdMountError::NativeInputMismatch)
        ));
    }

    #[test]
    fn unknown_reason_fails_closed() {
        let state =
            ZusdStateV1::reference_initial().unwrap_or_else(|error| panic!("state: {error}"));
        let input = ZusdMountInputV1::new(state, ZusdCommandV1::Liquidate);
        assert!(matches!(
            decode_zusd_native_decision_line_v1(&reject_line(&input, "invented"), &input),
            Err(ZusdMountError::UnknownReason(_))
        ));
    }

    #[test]
    fn whitespace_alias_fails_closed() {
        let state =
            ZusdStateV1::reference_initial().unwrap_or_else(|error| panic!("state: {error}"));
        let input = ZusdMountInputV1::new(state, ZusdCommandV1::Liquidate);
        let mut line = reject_line(&input, "liquidate_oracle_uninitialized");
        line.insert(1, b' ');
        assert!(matches!(
            decode_zusd_native_decision_line_v1(&line, &input),
            Err(ZusdMountError::NonCanonicalJson)
        ));
    }

    #[test]
    fn wrong_native_root_fails_closed() {
        let state =
            ZusdStateV1::reference_initial().unwrap_or_else(|error| panic!("state: {error}"));
        let input = ZusdMountInputV1::new(state, ZusdCommandV1::Liquidate);
        let mut line = reject_line(&input, "liquidate_oracle_uninitialized");
        let position = line
            .windows(66)
            .position(|window| window.starts_with(b"0x"))
            .unwrap_or_else(|| panic!("hash"));
        line[position + 2] = if line[position + 2] == b'0' {
            b'1'
        } else {
            b'0'
        };
        assert!(matches!(
            decode_zusd_native_decision_line_v1(&line, &input),
            Err(ZusdMountError::NonCanonicalJson | ZusdMountError::NativeRootMismatch)
        ));
    }
}
