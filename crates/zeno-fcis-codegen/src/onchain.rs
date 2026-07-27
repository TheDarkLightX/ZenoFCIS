//! Chain-neutral descriptions for generated on-chain FCIS state machines.
//!
//! The model is deliberately closed: fixed-size values, stable numeric IDs,
//! bounded observations, and catalogued capabilities. Backends may render the
//! same semantic machine for different chains without granting arbitrary call
//! authority to generated or agent-authored code.

use std::collections::BTreeSet;
use std::fmt;

use zeno_fcis_codec::{CommitmentHasher, Hash32};
use zeno_fcis_crypto::RustCryptoSha256;

/// Stable format version for chain-neutral on-chain machine descriptions.
pub const ONCHAIN_MACHINE_FORMAT_VERSION: u16 = 1;
/// Maximum state or command fields.
pub const MAX_ONCHAIN_FIELDS: usize = 64;
/// Maximum rejection reasons.
pub const MAX_ONCHAIN_REASONS: usize = 64;
/// Maximum observable event definitions.
pub const MAX_ONCHAIN_EVENTS: usize = 32;
/// Maximum fields in one observable event.
pub const MAX_ONCHAIN_EVENT_FIELDS: usize = 8;
/// Maximum effect capabilities.
pub const MAX_ONCHAIN_CAPABILITIES: usize = 16;
/// Maximum planned events or effects in one transition.
pub const MAX_ONCHAIN_PLAN_SLOTS: u8 = 16;

/// Fixed-size scalar grammar shared by generated chain backends.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OnchainScalar {
    /// Boolean value.
    Bool,
    /// Unsigned 8-bit integer.
    U8,
    /// Unsigned 16-bit integer.
    U16,
    /// Unsigned 32-bit integer.
    U32,
    /// Unsigned 64-bit integer.
    U64,
    /// Unsigned 128-bit integer.
    U128,
    /// Signed 8-bit integer.
    I8,
    /// Signed 16-bit integer.
    I16,
    /// Signed 32-bit integer.
    I32,
    /// Signed 64-bit integer.
    I64,
    /// Signed 128-bit integer.
    I128,
    /// Exact 32-byte value, also used for normalized identities.
    Bytes32,
}

impl OnchainScalar {
    /// Returns the stable one-byte grammar tag.
    #[must_use]
    pub const fn stable_tag(self) -> u8 {
        match self {
            Self::Bool => 1,
            Self::U8 => 2,
            Self::U16 => 3,
            Self::U32 => 4,
            Self::U64 => 5,
            Self::U128 => 6,
            Self::I8 => 7,
            Self::I16 => 8,
            Self::I32 => 9,
            Self::I64 => 10,
            Self::I128 => 11,
            Self::Bytes32 => 12,
        }
    }

    /// Returns the fixed cross-chain encoded width.
    #[must_use]
    pub const fn byte_width(self) -> u8 {
        match self {
            Self::Bool | Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 => 4,
            Self::U64 | Self::I64 => 8,
            Self::U128 | Self::I128 => 16,
            Self::Bytes32 => 32,
        }
    }
}

/// One stable field in a state, command, or event record.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OnchainField {
    id: u16,
    name: String,
    scalar: OnchainScalar,
}

impl OnchainField {
    /// Constructs a field with a nonzero stable ID and lower-snake name.
    pub fn try_new(
        id: u16,
        name: impl Into<String>,
        scalar: OnchainScalar,
    ) -> Result<Self, OnchainModelError> {
        if id == 0 {
            return Err(OnchainModelError::ReservedIdentifier);
        }
        let name = name.into();
        validate_lower_snake(&name)?;
        Ok(Self { id, name, scalar })
    }

    /// Returns the stable field ID.
    #[must_use]
    pub const fn id(&self) -> u16 {
        self.id
    }

    /// Returns the source-facing name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the field scalar type.
    #[must_use]
    pub const fn scalar(&self) -> OnchainScalar {
        self.scalar
    }
}

/// One stable rejection reason.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OnchainReason {
    code: u16,
    name: String,
}

impl OnchainReason {
    /// Constructs a nonzero reason code and UpperCamel name.
    pub fn try_new(code: u16, name: impl Into<String>) -> Result<Self, OnchainModelError> {
        if code == 0 {
            return Err(OnchainModelError::ReservedIdentifier);
        }
        let name = name.into();
        validate_upper_camel(&name)?;
        Ok(Self { code, name })
    }

    /// Returns the stable reason code.
    #[must_use]
    pub const fn code(&self) -> u16 {
        self.code
    }

    /// Returns the source-facing name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// One bounded public observation emitted after an accepted transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnchainEvent {
    code: u16,
    name: String,
    fields: Vec<OnchainField>,
}

impl OnchainEvent {
    /// Constructs and canonicalizes an event definition.
    pub fn try_new(
        code: u16,
        name: impl Into<String>,
        mut fields: Vec<OnchainField>,
    ) -> Result<Self, OnchainModelError> {
        if code == 0 {
            return Err(OnchainModelError::ReservedIdentifier);
        }
        if fields.len() > MAX_ONCHAIN_EVENT_FIELDS {
            return Err(OnchainModelError::LimitExceeded(OnchainListKind::EventFields));
        }
        let name = name.into();
        validate_upper_camel(&name)?;
        normalize_fields(&mut fields)?;
        Ok(Self { code, name, fields })
    }

    /// Returns the stable event code.
    #[must_use]
    pub const fn code(&self) -> u16 {
        self.code
    }

    /// Returns the source-facing name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns fields in canonical ID order.
    #[must_use]
    pub fn fields(&self) -> &[OnchainField] {
        &self.fields
    }
}

/// Allowed recipient source for one transfer capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecipientPolicy {
    /// The normalized transaction signer or caller.
    Caller,
    /// One fixed normalized identity.
    Fixed([u8; 32]),
    /// A `Bytes32` command field.
    CommandField(u16),
    /// A `Bytes32` pre-state field.
    StateField(u16),
}

/// Closed capability kinds admitted by the shared model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnchainCapabilityKind {
    /// Transfer a fungible asset already controlled by the shell.
    FungibleTransfer,
}

impl OnchainCapabilityKind {
    const fn stable_tag(self) -> u8 {
        match self {
            Self::FungibleTransfer => 1,
        }
    }
}

/// One bounded authority granted to an accepted transition plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnchainCapability {
    code: u16,
    name: String,
    kind: OnchainCapabilityKind,
    asset_id: [u8; 32],
    recipient: RecipientPolicy,
    max_amount: u128,
    max_uses: u8,
}

impl OnchainCapability {
    /// Constructs one catalogued capability.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        code: u16,
        name: impl Into<String>,
        kind: OnchainCapabilityKind,
        asset_id: [u8; 32],
        recipient: RecipientPolicy,
        max_amount: u128,
        max_uses: u8,
    ) -> Result<Self, OnchainModelError> {
        if code == 0 {
            return Err(OnchainModelError::ReservedIdentifier);
        }
        if asset_id == [0_u8; 32]
            || max_amount == 0
            || max_uses == 0
            || max_uses > MAX_ONCHAIN_PLAN_SLOTS
        {
            return Err(OnchainModelError::InvalidCapability);
        }
        if matches!(recipient, RecipientPolicy::Fixed(value) if value == [0_u8; 32]) {
            return Err(OnchainModelError::InvalidRecipientPolicy);
        }
        let name = name.into();
        validate_upper_camel(&name)?;
        Ok(Self {
            code,
            name,
            kind,
            asset_id,
            recipient,
            max_amount,
            max_uses,
        })
    }

    /// Returns the stable capability code.
    #[must_use]
    pub const fn code(&self) -> u16 {
        self.code
    }

    /// Returns the source-facing name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the capability kind.
    #[must_use]
    pub const fn kind(&self) -> OnchainCapabilityKind {
        self.kind
    }

    /// Returns the stable cross-chain asset ID.
    #[must_use]
    pub const fn asset_id(&self) -> [u8; 32] {
        self.asset_id
    }

    /// Returns the recipient authority policy.
    #[must_use]
    pub const fn recipient(&self) -> RecipientPolicy {
        self.recipient
    }

    /// Returns the inclusive amount ceiling per use.
    #[must_use]
    pub const fn max_amount(&self) -> u128 {
        self.max_amount
    }

    /// Returns the maximum uses per transition.
    #[must_use]
    pub const fn max_uses(&self) -> u8 {
        self.max_uses
    }
}

/// Public-observation shape policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationPolicy {
    /// Event and effect counts may vary within declared bounds.
    PublicVariableShape,
    /// Plans occupy all declared slots with canonical zero no-op padding.
    FixedShape,
}

impl ObservationPolicy {
    const fn stable_tag(self) -> u8 {
        match self {
            Self::PublicVariableShape => 1,
            Self::FixedShape => 2,
        }
    }
}

/// Closed semantic input shared by generated chain shells.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnchainMachineSpec {
    name: String,
    version: u16,
    state_fields: Vec<OnchainField>,
    command_fields: Vec<OnchainField>,
    reasons: Vec<OnchainReason>,
    events: Vec<OnchainEvent>,
    capabilities: Vec<OnchainCapability>,
    max_event_slots: u8,
    max_effect_slots: u8,
    observation_policy: ObservationPolicy,
}

impl OnchainMachineSpec {
    /// Constructs, validates, and canonicalizes a machine description.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        name: impl Into<String>,
        version: u16,
        mut state_fields: Vec<OnchainField>,
        mut command_fields: Vec<OnchainField>,
        mut reasons: Vec<OnchainReason>,
        mut events: Vec<OnchainEvent>,
        mut capabilities: Vec<OnchainCapability>,
        max_event_slots: u8,
        max_effect_slots: u8,
        observation_policy: ObservationPolicy,
    ) -> Result<Self, OnchainModelError> {
        let name = name.into();
        validate_upper_camel(&name)?;
        if version == 0 {
            return Err(OnchainModelError::ReservedIdentifier);
        }
        validate_required_len(&state_fields, MAX_ONCHAIN_FIELDS, OnchainListKind::StateFields)?;
        validate_required_len(
            &command_fields,
            MAX_ONCHAIN_FIELDS,
            OnchainListKind::CommandFields,
        )?;
        validate_required_len(&reasons, MAX_ONCHAIN_REASONS, OnchainListKind::Reasons)?;
        validate_optional_len(&events, MAX_ONCHAIN_EVENTS, OnchainListKind::Events)?;
        validate_optional_len(
            &capabilities,
            MAX_ONCHAIN_CAPABILITIES,
            OnchainListKind::Capabilities,
        )?;
        if max_event_slots > MAX_ONCHAIN_PLAN_SLOTS
            || max_effect_slots > MAX_ONCHAIN_PLAN_SLOTS
        {
            return Err(OnchainModelError::LimitExceeded(OnchainListKind::PlanSlots));
        }
        if events.is_empty() != (max_event_slots == 0)
            || capabilities.is_empty() != (max_effect_slots == 0)
        {
            return Err(OnchainModelError::InvalidPlanBounds);
        }

        normalize_fields(&mut state_fields)?;
        normalize_fields(&mut command_fields)?;
        normalize_reasons(&mut reasons)?;
        normalize_events(&mut events)?;
        normalize_capabilities(&mut capabilities)?;
        validate_recipient_policies(&state_fields, &command_fields, &capabilities)?;

        Ok(Self {
            name,
            version,
            state_fields,
            command_fields,
            reasons,
            events,
            capabilities,
            max_event_slots,
            max_effect_slots,
            observation_policy,
        })
    }

    /// Returns the machine name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the semantic version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Returns state fields in canonical ID order.
    #[must_use]
    pub fn state_fields(&self) -> &[OnchainField] {
        &self.state_fields
    }

    /// Returns command fields in canonical ID order.
    #[must_use]
    pub fn command_fields(&self) -> &[OnchainField] {
        &self.command_fields
    }

    /// Returns reasons in canonical code order.
    #[must_use]
    pub fn reasons(&self) -> &[OnchainReason] {
        &self.reasons
    }

    /// Returns public event definitions in canonical code order.
    #[must_use]
    pub fn events(&self) -> &[OnchainEvent] {
        &self.events
    }

    /// Returns capabilities in canonical code order.
    #[must_use]
    pub fn capabilities(&self) -> &[OnchainCapability] {
        &self.capabilities
    }

    /// Returns the event-plan slot bound.
    #[must_use]
    pub const fn max_event_slots(&self) -> u8 {
        self.max_event_slots
    }

    /// Returns the effect-plan slot bound.
    #[must_use]
    pub const fn max_effect_slots(&self) -> u8 {
        self.max_effect_slots
    }

    /// Returns the observation-shape policy.
    #[must_use]
    pub const fn observation_policy(&self) -> ObservationPolicy {
        self.observation_policy
    }

    /// Returns canonical bytes binding every semantic input.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend_from_slice(b"zeno-fcis/onchain-machine/v1\0");
        push_text(&mut output, &self.name);
        output.extend_from_slice(&self.version.to_be_bytes());
        output.push(self.observation_policy.stable_tag());
        output.push(self.max_event_slots);
        output.push(self.max_effect_slots);
        push_fields(&mut output, &self.state_fields);
        push_fields(&mut output, &self.command_fields);

        push_len(&mut output, self.reasons.len());
        for reason in &self.reasons {
            output.extend_from_slice(&reason.code.to_be_bytes());
            push_text(&mut output, &reason.name);
        }

        push_len(&mut output, self.events.len());
        for event in &self.events {
            output.extend_from_slice(&event.code.to_be_bytes());
            push_text(&mut output, &event.name);
            push_fields(&mut output, &event.fields);
        }

        push_len(&mut output, self.capabilities.len());
        for capability in &self.capabilities {
            output.extend_from_slice(&capability.code.to_be_bytes());
            push_text(&mut output, &capability.name);
            output.push(capability.kind.stable_tag());
            output.extend_from_slice(&capability.asset_id);
            match capability.recipient {
                RecipientPolicy::Caller => output.push(1),
                RecipientPolicy::Fixed(value) => {
                    output.push(2);
                    output.extend_from_slice(&value);
                }
                RecipientPolicy::CommandField(field_id) => {
                    output.push(3);
                    output.extend_from_slice(&field_id.to_be_bytes());
                }
                RecipientPolicy::StateField(field_id) => {
                    output.push(4);
                    output.extend_from_slice(&field_id.to_be_bytes());
                }
            }
            output.extend_from_slice(&capability.max_amount.to_be_bytes());
            output.push(capability.max_uses);
        }
        output
    }

    /// Returns the SHA-256 commitment over canonical machine bytes.
    #[must_use]
    pub fn machine_hash(&self) -> Hash32 {
        RustCryptoSha256::hash(&self.canonical_bytes())
    }
}

/// One generated source, manifest, or policy file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedOnchainFile {
    path: String,
    content: String,
}

impl GeneratedOnchainFile {
    pub(crate) fn new(path: String, content: String) -> Self {
        Self { path, content }
    }

    /// Returns the portable output path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns exact UTF-8 content.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Consumes the file into path and content.
    #[must_use]
    pub fn into_parts(self) -> (String, String) {
        (self.path, self.content)
    }
}

/// One canonical generated on-chain bundle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedOnchainBundle {
    generator_id: &'static str,
    machine_hash: Hash32,
    files: Vec<GeneratedOnchainFile>,
}

impl GeneratedOnchainBundle {
    pub(crate) fn try_new(
        generator_id: &'static str,
        machine_hash: Hash32,
        mut files: Vec<GeneratedOnchainFile>,
    ) -> Result<Self, OnchainModelError> {
        files.sort_by(|left, right| left.path.cmp(&right.path));
        if files.windows(2).any(|pair| pair[0].path == pair[1].path) {
            return Err(OnchainModelError::DuplicatePath);
        }
        Ok(Self {
            generator_id,
            machine_hash,
            files,
        })
    }

    /// Returns the stable backend generator ID.
    #[must_use]
    pub const fn generator_id(&self) -> &'static str {
        self.generator_id
    }

    /// Returns the shared semantic machine hash.
    #[must_use]
    pub const fn machine_hash(&self) -> Hash32 {
        self.machine_hash
    }

    /// Returns generated files in canonical path order.
    #[must_use]
    pub fn files(&self) -> &[GeneratedOnchainFile] {
        &self.files
    }
}

/// Bounded list categories used in validation errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnchainListKind {
    /// State fields.
    StateFields,
    /// Command fields.
    CommandFields,
    /// Rejection reasons.
    Reasons,
    /// Event definitions.
    Events,
    /// Fields within one event.
    EventFields,
    /// Effect capabilities.
    Capabilities,
    /// Per-transition plan slots.
    PlanSlots,
}

/// Deterministic chain-neutral model failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnchainModelError {
    /// A source-facing name violated the closed naming grammar.
    InvalidIdentifier,
    /// Zero was used for a stable ID or version.
    ReservedIdentifier,
    /// A required list was empty.
    EmptyList(OnchainListKind),
    /// A bounded list exceeded its hard limit.
    LimitExceeded(OnchainListKind),
    /// A stable ID/code or source name was duplicated.
    DuplicateName,
    /// Event/effect slot bounds disagreed with their catalogs.
    InvalidPlanBounds,
    /// A capability had malformed authority or bounds.
    InvalidCapability,
    /// A recipient field was absent or not `Bytes32`.
    InvalidRecipientPolicy,
    /// Generated bundle paths collided.
    DuplicatePath,
    /// Source rendering failed.
    Formatting,
    /// A backend binding was absent or malformed.
    InvalidBinding,
}

impl fmt::Display for OnchainModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier => formatter.write_str("invalid on-chain identifier"),
            Self::ReservedIdentifier => formatter.write_str("reserved zero identifier"),
            Self::EmptyList(kind) => write!(formatter, "empty on-chain list: {kind:?}"),
            Self::LimitExceeded(kind) => write!(formatter, "on-chain limit exceeded: {kind:?}"),
            Self::DuplicateName => formatter.write_str("duplicate on-chain ID or name"),
            Self::InvalidPlanBounds => formatter.write_str("invalid on-chain plan bounds"),
            Self::InvalidCapability => formatter.write_str("invalid on-chain capability"),
            Self::InvalidRecipientPolicy => {
                formatter.write_str("invalid capability recipient policy")
            }
            Self::DuplicatePath => formatter.write_str("duplicate generated path"),
            Self::Formatting => formatter.write_str("on-chain source formatting failed"),
            Self::InvalidBinding => formatter.write_str("invalid backend binding"),
        }
    }
}

impl std::error::Error for OnchainModelError {}

impl From<fmt::Error> for OnchainModelError {
    fn from(_: fmt::Error) -> Self {
        Self::Formatting
    }
}

fn validate_required_len<T>(
    values: &[T],
    maximum: usize,
    kind: OnchainListKind,
) -> Result<(), OnchainModelError> {
    if values.is_empty() {
        return Err(OnchainModelError::EmptyList(kind));
    }
    validate_optional_len(values, maximum, kind)
}

fn validate_optional_len<T>(
    values: &[T],
    maximum: usize,
    kind: OnchainListKind,
) -> Result<(), OnchainModelError> {
    if values.len() > maximum {
        return Err(OnchainModelError::LimitExceeded(kind));
    }
    Ok(())
}

fn normalize_fields(fields: &mut [OnchainField]) -> Result<(), OnchainModelError> {
    fields.sort_by_key(OnchainField::id);
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for field in fields {
        if !ids.insert(field.id()) || !names.insert(field.name()) {
            return Err(OnchainModelError::DuplicateName);
        }
    }
    Ok(())
}

fn normalize_reasons(reasons: &mut [OnchainReason]) -> Result<(), OnchainModelError> {
    reasons.sort_by_key(OnchainReason::code);
    let mut codes = BTreeSet::new();
    let mut names = BTreeSet::new();
    for reason in reasons {
        if !codes.insert(reason.code()) || !names.insert(reason.name()) {
            return Err(OnchainModelError::DuplicateName);
        }
    }
    Ok(())
}

fn normalize_events(events: &mut [OnchainEvent]) -> Result<(), OnchainModelError> {
    events.sort_by_key(OnchainEvent::code);
    let mut codes = BTreeSet::new();
    let mut names = BTreeSet::new();
    for event in events {
        if !codes.insert(event.code()) || !names.insert(event.name()) {
            return Err(OnchainModelError::DuplicateName);
        }
    }
    Ok(())
}

fn normalize_capabilities(
    capabilities: &mut [OnchainCapability],
) -> Result<(), OnchainModelError> {
    capabilities.sort_by_key(OnchainCapability::code);
    let mut codes = BTreeSet::new();
    let mut names = BTreeSet::new();
    for capability in capabilities {
        if !codes.insert(capability.code()) || !names.insert(capability.name()) {
            return Err(OnchainModelError::DuplicateName);
        }
    }
    Ok(())
}

fn validate_recipient_policies(
    state_fields: &[OnchainField],
    command_fields: &[OnchainField],
    capabilities: &[OnchainCapability],
) -> Result<(), OnchainModelError> {
    for capability in capabilities {
        let selected = match capability.recipient() {
            RecipientPolicy::Caller | RecipientPolicy::Fixed(_) => continue,
            RecipientPolicy::CommandField(id) => {
                command_fields.iter().find(|field| field.id() == id)
            }
            RecipientPolicy::StateField(id) => state_fields.iter().find(|field| field.id() == id),
        };
        if !matches!(selected, Some(field) if field.scalar() == OnchainScalar::Bytes32) {
            return Err(OnchainModelError::InvalidRecipientPolicy);
        }
    }
    Ok(())
}

fn push_fields(output: &mut Vec<u8>, fields: &[OnchainField]) {
    push_len(output, fields.len());
    for field in fields {
        output.extend_from_slice(&field.id.to_be_bytes());
        push_text(output, &field.name);
        output.push(field.scalar.stable_tag());
        output.push(field.scalar.byte_width());
    }
}

fn push_text(output: &mut Vec<u8>, value: &str) {
    push_len(output, value.len());
    output.extend_from_slice(value.as_bytes());
}

fn push_len(output: &mut Vec<u8>, length: usize) {
    output.extend_from_slice(&u16::try_from(length).unwrap_or(u16::MAX).to_be_bytes());
}

fn validate_lower_snake(value: &str) -> Result<(), OnchainModelError> {
    if value.is_empty() || value.len() > 64 {
        return Err(OnchainModelError::InvalidIdentifier);
    }
    let bytes = value.as_bytes();
    if !bytes[0].is_ascii_lowercase()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
        || value.ends_with('_')
        || value.contains("__")
    {
        return Err(OnchainModelError::InvalidIdentifier);
    }
    Ok(())
}

fn validate_upper_camel(value: &str) -> Result<(), OnchainModelError> {
    if value.is_empty() || value.len() > 64 {
        return Err(OnchainModelError::InvalidIdentifier);
    }
    let bytes = value.as_bytes();
    if !bytes[0].is_ascii_uppercase()
        || !bytes.iter().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(OnchainModelError::InvalidIdentifier);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(id: u16, name: &str, scalar: OnchainScalar) -> OnchainField {
        match OnchainField::try_new(id, name, scalar) {
            Ok(value) => value,
            Err(error) => panic!("field rejected: {error}"),
        }
    }

    fn reason(code: u16, name: &str) -> OnchainReason {
        match OnchainReason::try_new(code, name) {
            Ok(value) => value,
            Err(error) => panic!("reason rejected: {error}"),
        }
    }

    fn fixture(reverse: bool) -> OnchainMachineSpec {
        let mut state = vec![
            field(1, "owner", OnchainScalar::Bytes32),
            field(2, "balance", OnchainScalar::U128),
        ];
        let mut command = vec![
            field(1, "recipient", OnchainScalar::Bytes32),
            field(2, "amount", OnchainScalar::U128),
        ];
        if reverse {
            state.reverse();
            command.reverse();
        }
        let capability = match OnchainCapability::try_new(
            7,
            "Payout",
            OnchainCapabilityKind::FungibleTransfer,
            [9_u8; 32],
            RecipientPolicy::CommandField(1),
            1_000,
            1,
        ) {
            Ok(value) => value,
            Err(error) => panic!("capability rejected: {error}"),
        };
        match OnchainMachineSpec::try_new(
            "TreasuryMachine",
            1,
            state,
            command,
            vec![reason(1, "Unauthorized"), reason(2, "InsufficientBalance")],
            Vec::new(),
            vec![capability],
            0,
            1,
            ObservationPolicy::PublicVariableShape,
        ) {
            Ok(value) => value,
            Err(error) => panic!("machine rejected: {error}"),
        }
    }

    #[test]
    fn declaration_order_does_not_change_hash() {
        assert_eq!(fixture(false).machine_hash(), fixture(true).machine_hash());
    }

    #[test]
    fn recipient_fields_must_be_bytes32() {
        let capability = match OnchainCapability::try_new(
            1,
            "Payout",
            OnchainCapabilityKind::FungibleTransfer,
            [7_u8; 32],
            RecipientPolicy::CommandField(1),
            10,
            1,
        ) {
            Ok(value) => value,
            Err(error) => panic!("capability rejected: {error}"),
        };
        let result = OnchainMachineSpec::try_new(
            "InvalidRecipient",
            1,
            vec![field(1, "balance", OnchainScalar::U128)],
            vec![field(1, "recipient", OnchainScalar::U128)],
            vec![reason(1, "Rejected")],
            Vec::new(),
            vec![capability],
            0,
            1,
            ObservationPolicy::PublicVariableShape,
        );
        assert_eq!(result, Err(OnchainModelError::InvalidRecipientPolicy));
    }

    #[test]
    fn plan_bounds_must_match_catalogs() {
        let result = OnchainMachineSpec::try_new(
            "BadBounds",
            1,
            vec![field(1, "value", OnchainScalar::U64)],
            vec![field(1, "delta", OnchainScalar::U64)],
            vec![reason(1, "Rejected")],
            Vec::new(),
            Vec::new(),
            1,
            0,
            ObservationPolicy::PublicVariableShape,
        );
        assert_eq!(result, Err(OnchainModelError::InvalidPlanBounds));
    }
}
