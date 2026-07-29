//! Schema-bound reason, effect, and channel catalogs for ZenoFCIS projects.
//!
//! [`ProjectProfile`] commits registry identities, while this crate makes those
//! commitments executable. A catalog binds stable reason precedence, effect
//! payload and authority requirements, channel destination and payload schemas,
//! and deterministic aggregate plan limits. Project shells may interpret plans,
//! but they cannot silently invent an operation, reinterpret a payload, or
//! exceed a reviewed resource envelope.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
#[cfg(test)]
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::DecisionKind;
use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan};
use zeno_fcis_project::{
    ProfileError, ProjectProfile, RegistryEntry, RegistryKind, SemanticId, StableName,
};
use zeno_fcis_schema::{Schema, SchemaError, TypeId, ValidationLimits, ValueValidationError};
use zeno_fcis_value::{Value, ValueError, ValueLimits, ValueMetrics};

/// Canonical catalog format version.
pub const CATALOG_FORMAT_VERSION: u16 = 2;
/// Maximum definitions in any one catalog namespace.
pub const MAX_CATALOG_DEFINITIONS: usize = 65_536;
/// Maximum bytes in a hash-provider identity.
pub const MAX_HASH_ALGORITHM_ID_BYTES: usize = 160;
/// Maximum effects or outbox entries admitted by a catalog limit.
pub const MAX_PLAN_ITEMS: u32 = 1_000_000;
/// Maximum nodes admitted in one closed value.
pub const MAX_VALUE_NODES: u64 = 10_000_000;
/// Maximum nodes admitted across one complete plan pair.
pub const MAX_TOTAL_VALUE_NODES: u64 = 100_000_000;
/// Maximum payload bytes admitted across one complete plan pair.
pub const MAX_TOTAL_PAYLOAD_BYTES: u64 = 1_073_741_824;
/// Maximum recursive value depth admitted by a catalog.
pub const MAX_VALUE_DEPTH: u16 = 1_024;
/// Maximum children in one collection value.
pub const MAX_COLLECTION_LENGTH: u32 = 1_000_000;
/// Maximum distinct value-flow descriptors on one effect or channel.
pub const MAX_VALUE_FLOWS: usize = 64;

/// A commitment that is statically known not to be the all-zero sentinel.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NonZeroHash(Hash32);

impl NonZeroHash {
    /// Creates a nonzero commitment.
    pub fn try_new(value: Hash32) -> Result<Self, CatalogError> {
        if value == Hash32::ZERO {
            Err(CatalogError::ZeroCommitment)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the commitment.
    #[must_use]
    pub const fn get(self) -> Hash32 {
        self.0
    }
}

impl CanonicalEncode for NonZeroHash {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.0.as_bytes());
        Ok(())
    }
}

/// Requirement imposed on an effect's authority or subject commitment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HashRequirement {
    /// Both zero and nonzero commitments are admitted.
    Any,
    /// The field must use the zero sentinel.
    Absent,
    /// The field must be nonzero.
    Present,
    /// The field must equal one reviewed commitment.
    Exact(NonZeroHash),
}

impl HashRequirement {
    /// Creates an exact nonzero requirement.
    pub fn exact(value: Hash32) -> Result<Self, CatalogError> {
        Ok(Self::Exact(NonZeroHash::try_new(value)?))
    }

    /// Returns whether one commitment satisfies this closed requirement.
    #[must_use]
    pub fn admits(self, value: Hash32) -> bool {
        match self {
            Self::Any => true,
            Self::Absent => value == Hash32::ZERO,
            Self::Present => value != Hash32::ZERO,
            Self::Exact(expected) => value == expected.get(),
        }
    }
}

impl CanonicalEncode for HashRequirement {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Any => output.push(0),
            Self::Absent => output.push(1),
            Self::Present => output.push(2),
            Self::Exact(value) => {
                output.push(3);
                value.encode_to(output)?;
            }
        }
        Ok(())
    }
}

/// One closed kind of economically meaningful flow.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ValueFlowKind {
    /// Value moves between owned subjects.
    Transfer = 0,
    /// New supply is created.
    Mint = 1,
    /// Existing supply is destroyed.
    Burn = 2,
    /// Value is locked into escrow.
    EscrowLock = 3,
    /// Value is released from escrow.
    EscrowRelease = 4,
    /// A fee, dust amount, or rounding remainder is charged or distributed.
    FeeCharge = 5,
    /// A settlement may combine transfers, fees, and balance reconciliation.
    Settlement = 6,
    /// Value crosses the FCIS boundary through an external delivery obligation.
    ExternalValueDelivery = 7,
    /// Project-specific value semantics are established by one registered claim.
    Custom = 8,
}

impl CanonicalEncode for ValueFlowKind {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// One asset-scoped value-flow descriptor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ValueFlow {
    kind: ValueFlowKind,
    asset_domain: NonZeroHash,
    custom_claim: Option<(SemanticId, NonZeroHash)>,
}

impl ValueFlow {
    /// Creates one standard closed value flow.
    pub fn standard(kind: ValueFlowKind, asset_domain: Hash32) -> Result<Self, CatalogError> {
        if kind == ValueFlowKind::Custom {
            return Err(CatalogError::MissingCustomValueClaim);
        }
        Ok(Self {
            kind,
            asset_domain: NonZeroHash::try_new(asset_domain)?,
            custom_claim: None,
        })
    }

    /// Creates a project-specific value flow bound to one exact registered claim.
    pub fn custom(
        asset_domain: Hash32,
        claim_id: SemanticId,
        claim_hash: Hash32,
    ) -> Result<Self, CatalogError> {
        Ok(Self {
            kind: ValueFlowKind::Custom,
            asset_domain: NonZeroHash::try_new(asset_domain)?,
            custom_claim: Some((claim_id, NonZeroHash::try_new(claim_hash)?)),
        })
    }

    /// Returns the closed flow kind.
    #[must_use]
    pub const fn kind(self) -> ValueFlowKind {
        self.kind
    }

    /// Returns the exact asset-domain commitment.
    #[must_use]
    pub const fn asset_domain(self) -> Hash32 {
        self.asset_domain.get()
    }

    /// Returns the custom relational claim, when required by the flow kind.
    #[must_use]
    pub const fn custom_claim(self) -> Option<(SemanticId, Hash32)> {
        match self.custom_claim {
            Some((id, hash)) => Some((id, hash.get())),
            None => None,
        }
    }
}

impl CanonicalEncode for ValueFlow {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.kind.encode_to(output)?;
        self.asset_domain.encode_to(output)?;
        match self.custom_claim {
            None => output.push(0),
            Some((id, hash)) => {
                output.push(1);
                id.encode_to(output)?;
                hash.encode_to(output)?;
            }
        }
        Ok(())
    }
}

/// Constructor-only reviewed economic classification for one effect or channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationSemantics {
    flows: Box<[ValueFlow]>,
    classification_hash: NonZeroHash,
}

impl OperationSemantics {
    /// Classifies an operation as non-value under explicit reviewed evidence.
    pub fn non_value(classification_hash: Hash32) -> Result<Self, CatalogError> {
        Ok(Self {
            flows: Box::new([]),
            classification_hash: NonZeroHash::try_new(classification_hash)?,
        })
    }

    /// Classifies an operation with a bounded canonical set of value flows.
    pub fn value(
        mut flows: Vec<ValueFlow>,
        classification_hash: Hash32,
    ) -> Result<Self, CatalogError> {
        if flows.is_empty() {
            return Err(CatalogError::EmptyValueFlows);
        }
        if flows.len() > MAX_VALUE_FLOWS {
            return Err(CatalogError::TooManyValueFlows);
        }
        flows.sort_unstable();
        if flows.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(CatalogError::DuplicateValueFlow);
        }
        Ok(Self {
            flows: flows.into_boxed_slice(),
            classification_hash: NonZeroHash::try_new(classification_hash)?,
        })
    }

    /// Returns the canonical value flows. Non-value operations return an empty slice.
    #[must_use]
    pub fn flows(&self) -> &[ValueFlow] {
        &self.flows
    }

    /// Returns whether this operation is classified as value-moving.
    #[must_use]
    pub const fn is_value_moving(&self) -> bool {
        !self.flows.is_empty()
    }

    /// Returns the reviewed classification commitment.
    #[must_use]
    pub const fn classification_hash(&self) -> Hash32 {
        self.classification_hash.get()
    }
}

impl CanonicalEncode for OperationSemantics {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(u8::from(self.is_value_moving()));
        if self.is_value_moving() {
            put_length(output, self.flows.len())?;
            for flow in self.flows.iter() {
                flow.encode_to(output)?;
            }
        }
        self.classification_hash.encode_to(output)
    }
}

/// Decision class to which a stable reason belongs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasonDisposition {
    /// Ordinary rejection with no candidate or authoritative transition.
    Reject,
    /// Intentional committed failure with a candidate-bound transition.
    CommittedFailure,
}

impl ReasonDisposition {
    fn admits(self, kind: DecisionKind) -> bool {
        matches!(
            (self, kind),
            (Self::Reject, DecisionKind::Reject)
                | (Self::CommittedFailure, DecisionKind::CommittedFailure)
        )
    }
}

impl CanonicalEncode for ReasonDisposition {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(match self {
            Self::Reject => 0,
            Self::CommittedFailure => 1,
        });
        Ok(())
    }
}

/// One stable rejection or committed-failure reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReasonDefinition {
    id: SemanticId,
    name: StableName,
    disposition: ReasonDisposition,
    precedence: u32,
    predicate_hash: NonZeroHash,
}

impl ReasonDefinition {
    /// Creates one reason bound to a reviewed applicability predicate.
    pub fn try_new(
        id: SemanticId,
        name: StableName,
        disposition: ReasonDisposition,
        precedence: u32,
        predicate_hash: Hash32,
    ) -> Result<Self, CatalogError> {
        Ok(Self {
            id,
            name,
            disposition,
            precedence,
            predicate_hash: NonZeroHash::try_new(predicate_hash)?,
        })
    }

    /// Returns the stable identifier.
    #[must_use]
    pub const fn id(&self) -> SemanticId {
        self.id
    }

    /// Returns the stable readable name.
    #[must_use]
    pub const fn name(&self) -> &StableName {
        &self.name
    }

    /// Returns the decision class.
    #[must_use]
    pub const fn disposition(&self) -> ReasonDisposition {
        self.disposition
    }

    /// Returns the total-order position.
    #[must_use]
    pub const fn precedence(&self) -> u32 {
        self.precedence
    }

    /// Returns the reviewed predicate commitment.
    #[must_use]
    pub const fn predicate_hash(&self) -> Hash32 {
        self.predicate_hash.get()
    }

    /// Computes the definition commitment used by the project registry entry.
    pub fn definition_hash<H: CommitmentHasher>(&self) -> Result<Hash32, CatalogError> {
        hash_canonical::<H>("zeno-fcis/reason-definition", self)
    }
}

impl CanonicalEncode for ReasonDefinition {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.id.encode_to(output)?;
        self.name.encode_to(output)?;
        self.disposition.encode_to(output)?;
        output.extend_from_slice(&self.precedence.to_be_bytes());
        self.predicate_hash.encode_to(output)
    }
}

/// One authoritative effect operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectDefinition {
    id: SemanticId,
    name: StableName,
    payload_type: TypeId,
    authority: HashRequirement,
    subject: HashRequirement,
    semantics: OperationSemantics,
    policy_hash: NonZeroHash,
}

impl EffectDefinition {
    /// Creates an effect definition.
    pub fn try_new(
        id: SemanticId,
        name: StableName,
        payload_type: TypeId,
        authority: HashRequirement,
        subject: HashRequirement,
        semantics: OperationSemantics,
        policy_hash: Hash32,
    ) -> Result<Self, CatalogError> {
        Ok(Self {
            id,
            name,
            payload_type,
            authority,
            subject,
            semantics,
            policy_hash: NonZeroHash::try_new(policy_hash)?,
        })
    }

    /// Returns the stable identifier used by [`Effect::operation`].
    #[must_use]
    pub const fn id(&self) -> SemanticId {
        self.id
    }

    /// Returns the stable readable name.
    #[must_use]
    pub const fn name(&self) -> &StableName {
        &self.name
    }

    /// Returns the closed payload schema type.
    #[must_use]
    pub const fn payload_type(&self) -> TypeId {
        self.payload_type
    }

    /// Returns the authority commitment requirement.
    #[must_use]
    pub const fn authority_requirement(&self) -> HashRequirement {
        self.authority
    }

    /// Returns the subject commitment requirement.
    #[must_use]
    pub const fn subject_requirement(&self) -> HashRequirement {
        self.subject
    }

    /// Returns the reviewed economic classification.
    #[must_use]
    pub const fn semantics(&self) -> &OperationSemantics {
        &self.semantics
    }

    /// Returns the project policy commitment for this operation.
    #[must_use]
    pub const fn policy_hash(&self) -> Hash32 {
        self.policy_hash.get()
    }

    /// Computes the definition commitment used by the project registry entry.
    pub fn definition_hash<H: CommitmentHasher>(&self) -> Result<Hash32, CatalogError> {
        hash_canonical::<H>("zeno-fcis/effect-definition", self)
    }
}

impl CanonicalEncode for EffectDefinition {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.id.encode_to(output)?;
        self.name.encode_to(output)?;
        output.extend_from_slice(&self.payload_type.get().to_be_bytes());
        self.authority.encode_to(output)?;
        self.subject.encode_to(output)?;
        self.semantics.encode_to(output)?;
        self.policy_hash.encode_to(output)
    }
}

/// One external-delivery channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelDefinition {
    id: SemanticId,
    name: StableName,
    destination_type: TypeId,
    payload_type: TypeId,
    semantics: OperationSemantics,
    delivery_policy_hash: NonZeroHash,
}

impl ChannelDefinition {
    /// Creates a channel definition.
    pub fn try_new(
        id: SemanticId,
        name: StableName,
        destination_type: TypeId,
        payload_type: TypeId,
        semantics: OperationSemantics,
        delivery_policy_hash: Hash32,
    ) -> Result<Self, CatalogError> {
        Ok(Self {
            id,
            name,
            destination_type,
            payload_type,
            semantics,
            delivery_policy_hash: NonZeroHash::try_new(delivery_policy_hash)?,
        })
    }

    /// Returns the stable identifier used by [`OutboxEntry::channel`].
    #[must_use]
    pub const fn id(&self) -> SemanticId {
        self.id
    }

    /// Returns the stable readable name.
    #[must_use]
    pub const fn name(&self) -> &StableName {
        &self.name
    }

    /// Returns the destination schema type.
    #[must_use]
    pub const fn destination_type(&self) -> TypeId {
        self.destination_type
    }

    /// Returns the payload schema type.
    #[must_use]
    pub const fn payload_type(&self) -> TypeId {
        self.payload_type
    }

    /// Returns the reviewed economic classification.
    #[must_use]
    pub const fn semantics(&self) -> &OperationSemantics {
        &self.semantics
    }

    /// Returns the delivery-policy commitment.
    #[must_use]
    pub const fn delivery_policy_hash(&self) -> Hash32 {
        self.delivery_policy_hash.get()
    }

    /// Computes the definition commitment used by the project registry entry.
    pub fn definition_hash<H: CommitmentHasher>(&self) -> Result<Hash32, CatalogError> {
        hash_canonical::<H>("zeno-fcis/channel-definition", self)
    }
}

impl CanonicalEncode for ChannelDefinition {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.id.encode_to(output)?;
        self.name.encode_to(output)?;
        output.extend_from_slice(&self.destination_type.get().to_be_bytes());
        output.extend_from_slice(&self.payload_type.get().to_be_bytes());
        self.semantics.encode_to(output)?;
        self.delivery_policy_hash.encode_to(output)
    }
}

/// Canonical catalog authority values constructed before a [`ProjectProfile`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogManifest {
    hash_algorithm_id: Box<str>,
    precedence_hash: Hash32,
    effect_registry_hash: Hash32,
    channel_registry_hash: Hash32,
    reasons: Box<[ReasonDefinition]>,
    effects: Box<[EffectDefinition]>,
    channels: Box<[ChannelDefinition]>,
    registry_entries: Box<[RegistryEntry]>,
}

impl CatalogManifest {
    /// Canonicalizes definitions and derives registry commitments and entries.
    pub fn try_new<H: CommitmentHasher>(
        reasons: Vec<ReasonDefinition>,
        effects: Vec<EffectDefinition>,
        channels: Vec<ChannelDefinition>,
    ) -> Result<Self, CatalogError> {
        validate_hash_algorithm_id(H::ALGORITHM_ID)?;
        let reasons = normalize_reasons(reasons)?;
        let effects = normalize_effects(effects)?;
        let channels = normalize_channels(channels)?;

        let precedence_hash = reason_registry_hash::<H>(&reasons)?;
        let effect_registry_hash =
            definition_registry_hash::<H, _>("zeno-fcis/effect-registry", &effects)?;
        let channel_registry_hash =
            definition_registry_hash::<H, _>("zeno-fcis/channel-registry", &channels)?;
        let mut entries = Vec::with_capacity(reasons.len() + effects.len() + channels.len());
        for reason in &reasons {
            entries.push(
                RegistryEntry::try_new(
                    RegistryKind::Reason,
                    reason.id(),
                    reason.name().clone(),
                    reason.definition_hash::<H>()?,
                )
                .map_err(CatalogError::Profile)?,
            );
        }
        for effect in &effects {
            entries.push(
                RegistryEntry::try_new(
                    RegistryKind::Effect,
                    effect.id(),
                    effect.name().clone(),
                    effect.definition_hash::<H>()?,
                )
                .map_err(CatalogError::Profile)?,
            );
        }
        for channel in &channels {
            entries.push(
                RegistryEntry::try_new(
                    RegistryKind::Channel,
                    channel.id(),
                    channel.name().clone(),
                    channel.definition_hash::<H>()?,
                )
                .map_err(CatalogError::Profile)?,
            );
        }
        entries.sort_by_key(|entry| (entry.kind(), entry.id()));

        Ok(Self {
            hash_algorithm_id: H::ALGORITHM_ID.into(),
            precedence_hash,
            effect_registry_hash,
            channel_registry_hash,
            reasons,
            effects,
            channels,
            registry_entries: entries.into_boxed_slice(),
        })
    }

    /// Returns the exact hash-provider identity used to derive commitments.
    #[must_use]
    pub const fn hash_algorithm_id(&self) -> &str {
        &self.hash_algorithm_id
    }

    /// Returns the stable total-precedence commitment.
    #[must_use]
    pub const fn precedence_hash(&self) -> Hash32 {
        self.precedence_hash
    }

    /// Returns the authoritative effect-registry commitment.
    #[must_use]
    pub const fn effect_registry_hash(&self) -> Hash32 {
        self.effect_registry_hash
    }

    /// Returns the external channel-registry commitment.
    #[must_use]
    pub const fn channel_registry_hash(&self) -> Hash32 {
        self.channel_registry_hash
    }

    /// Returns reason definitions in stable ID order.
    #[must_use]
    pub const fn reasons(&self) -> &[ReasonDefinition] {
        &self.reasons
    }

    /// Returns effect definitions in stable ID order.
    #[must_use]
    pub const fn effects(&self) -> &[EffectDefinition] {
        &self.effects
    }

    /// Returns channel definitions in stable ID order.
    #[must_use]
    pub const fn channels(&self) -> &[ChannelDefinition] {
        &self.channels
    }

    /// Returns profile registry entries in canonical `(kind, id)` order.
    #[must_use]
    pub const fn registry_entries(&self) -> &[RegistryEntry] {
        &self.registry_entries
    }

    /// Looks up a reason by stable identifier.
    #[must_use]
    pub fn reason(&self, id: SemanticId) -> Option<&ReasonDefinition> {
        self.reasons
            .binary_search_by_key(&id, ReasonDefinition::id)
            .ok()
            .map(|index| &self.reasons[index])
    }

    /// Looks up an effect by stable identifier.
    #[must_use]
    pub fn effect(&self, id: SemanticId) -> Option<&EffectDefinition> {
        self.effects
            .binary_search_by_key(&id, EffectDefinition::id)
            .ok()
            .map(|index| &self.effects[index])
    }

    /// Looks up a channel by stable identifier.
    #[must_use]
    pub fn channel(&self, id: SemanticId) -> Option<&ChannelDefinition> {
        self.channels
            .binary_search_by_key(&id, ChannelDefinition::id)
            .ok()
            .map(|index| &self.channels[index])
    }

    /// Computes a commitment to the complete inspectable manifest.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, CatalogError> {
        if self.hash_algorithm_id() != H::ALGORITHM_ID {
            return Err(CatalogError::HashAlgorithmMismatch);
        }
        hash_canonical::<H>("zeno-fcis/catalog-manifest", self)
    }
}

impl CanonicalEncode for CatalogManifest {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-CATALOG-MANIFEST\0");
        output.extend_from_slice(&CATALOG_FORMAT_VERSION.to_be_bytes());
        put_blob(output, self.hash_algorithm_id.as_bytes())?;
        output.extend_from_slice(self.precedence_hash.as_bytes());
        output.extend_from_slice(self.effect_registry_hash.as_bytes());
        output.extend_from_slice(self.channel_registry_hash.as_bytes());
        put_definitions(output, &self.reasons)?;
        put_definitions(output, &self.effects)?;
        put_definitions(output, &self.channels)
    }
}

/// Deterministic resource envelope for one commit/outbox plan pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogLimits {
    max_effects: u32,
    max_outbox_entries: u32,
    max_value_depth: u16,
    max_value_nodes: u64,
    max_total_value_nodes: u64,
    max_total_payload_bytes: u64,
    max_collection_len: u32,
}

impl CatalogLimits {
    /// Creates a bounded plan-validation envelope.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        max_effects: u32,
        max_outbox_entries: u32,
        max_value_depth: u16,
        max_value_nodes: u64,
        max_total_value_nodes: u64,
        max_total_payload_bytes: u64,
        max_collection_len: u32,
    ) -> Result<Self, CatalogError> {
        let limits = Self {
            max_effects,
            max_outbox_entries,
            max_value_depth,
            max_value_nodes,
            max_total_value_nodes,
            max_total_payload_bytes,
            max_collection_len,
        };
        limits.validate()?;
        Ok(limits)
    }

    fn validate(self) -> Result<Self, CatalogError> {
        if self.max_effects > MAX_PLAN_ITEMS
            || self.max_outbox_entries > MAX_PLAN_ITEMS
            || self.max_value_depth == 0
            || self.max_value_depth > MAX_VALUE_DEPTH
            || self.max_value_nodes == 0
            || self.max_value_nodes > MAX_VALUE_NODES
            || self.max_total_value_nodes < self.max_value_nodes
            || self.max_total_value_nodes > MAX_TOTAL_VALUE_NODES
            || self.max_total_payload_bytes == 0
            || self.max_total_payload_bytes > MAX_TOTAL_PAYLOAD_BYTES
            || self.max_collection_len == 0
            || self.max_collection_len > MAX_COLLECTION_LENGTH
        {
            return Err(CatalogError::InvalidLimits);
        }
        Ok(self)
    }

    /// Returns the maximum authoritative-effect count.
    #[must_use]
    pub const fn max_effects(self) -> u32 {
        self.max_effects
    }

    /// Returns the maximum outbox-entry count.
    #[must_use]
    pub const fn max_outbox_entries(self) -> u32 {
        self.max_outbox_entries
    }

    /// Returns the maximum recursive value depth.
    #[must_use]
    pub const fn max_value_depth(self) -> u16 {
        self.max_value_depth
    }

    /// Returns the maximum nodes admitted in any one value.
    #[must_use]
    pub const fn max_value_nodes(self) -> u64 {
        self.max_value_nodes
    }

    /// Returns the maximum nodes admitted across both plans.
    #[must_use]
    pub const fn max_total_value_nodes(self) -> u64 {
        self.max_total_value_nodes
    }

    /// Returns the maximum payload bytes admitted across both plans.
    #[must_use]
    pub const fn max_total_payload_bytes(self) -> u64 {
        self.max_total_payload_bytes
    }

    /// Returns the maximum children admitted in one collection value.
    #[must_use]
    pub const fn max_collection_len(self) -> u32 {
        self.max_collection_len
    }
}

impl Default for CatalogLimits {
    fn default() -> Self {
        Self {
            max_effects: 4_096,
            max_outbox_entries: 4_096,
            max_value_depth: 64,
            max_value_nodes: 1_000_000,
            max_total_value_nodes: 4_000_000,
            max_total_payload_bytes: 64 * 1024 * 1024,
            max_collection_len: 1_000_000,
        }
    }
}

impl CanonicalEncode for CatalogLimits {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.max_effects.to_be_bytes());
        output.extend_from_slice(&self.max_outbox_entries.to_be_bytes());
        output.extend_from_slice(&self.max_value_depth.to_be_bytes());
        output.extend_from_slice(&self.max_value_nodes.to_be_bytes());
        output.extend_from_slice(&self.max_total_value_nodes.to_be_bytes());
        output.extend_from_slice(&self.max_total_payload_bytes.to_be_bytes());
        output.extend_from_slice(&self.max_collection_len.to_be_bytes());
        Ok(())
    }
}

/// Exact structural resources consumed by successful plan validation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CatalogMetrics {
    effects: u32,
    outbox_entries: u32,
    value_nodes: u64,
    payload_bytes: u64,
    maximum_depth: u32,
}

impl CatalogMetrics {
    /// Returns the effect count.
    #[must_use]
    pub const fn effects(self) -> u32 {
        self.effects
    }

    /// Returns the outbox-entry count.
    #[must_use]
    pub const fn outbox_entries(self) -> u32 {
        self.outbox_entries
    }

    /// Returns the total closed-value node count.
    #[must_use]
    pub const fn value_nodes(self) -> u64 {
        self.value_nodes
    }

    /// Returns aggregate byte/text/map-key payload bytes.
    #[must_use]
    pub const fn payload_bytes(self) -> u64 {
        self.payload_bytes
    }

    /// Returns maximum observed value depth.
    #[must_use]
    pub const fn maximum_depth(self) -> u32 {
        self.maximum_depth
    }
}

impl CanonicalEncode for CatalogMetrics {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.effects.to_be_bytes());
        output.extend_from_slice(&self.outbox_entries.to_be_bytes());
        output.extend_from_slice(&self.value_nodes.to_be_bytes());
        output.extend_from_slice(&self.payload_bytes.to_be_bytes());
        output.extend_from_slice(&self.maximum_depth.to_be_bytes());
        Ok(())
    }
}

/// Position of a value within a plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueRole {
    /// Authoritative effect payload.
    EffectPayload,
    /// External-delivery destination.
    OutboxDestination,
    /// External-delivery payload.
    OutboxPayload,
}

/// Commitment field constrained by an effect definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectHashField {
    /// Authority-domain commitment.
    Authority,
    /// Subject commitment.
    Subject,
}

/// A profile, schema, manifest, and resource envelope ready to validate plans.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectCatalog {
    profile: ProjectProfile,
    schema: Schema,
    manifest: CatalogManifest,
    limits: CatalogLimits,
    profile_hash: Hash32,
    schema_hash: Hash32,
}

impl ProjectCatalog {
    /// Validates every cross-binding and owns one complete project catalog.
    pub fn try_new<H: CommitmentHasher>(
        profile: ProjectProfile,
        schema: Schema,
        manifest: CatalogManifest,
        limits: CatalogLimits,
    ) -> Result<Self, CatalogError> {
        limits.validate()?;
        if manifest.hash_algorithm_id() != H::ALGORITHM_ID {
            return Err(CatalogError::HashAlgorithmMismatch);
        }
        let schema_hash = schema.schema_hash::<H>().map_err(CatalogError::Schema)?;
        let bindings = profile.bindings();
        if bindings.schema_hash != schema_hash {
            return Err(CatalogError::SchemaBindingMismatch);
        }
        if bindings.precedence_hash != manifest.precedence_hash() {
            return Err(CatalogError::PrecedenceBindingMismatch);
        }
        if bindings.effect_registry_hash != manifest.effect_registry_hash() {
            return Err(CatalogError::EffectRegistryBindingMismatch);
        }
        if bindings.channel_registry_hash != manifest.channel_registry_hash() {
            return Err(CatalogError::ChannelRegistryBindingMismatch);
        }
        let state_type = TypeId::new(profile.state_type().get());
        let command_type = TypeId::new(profile.command_type().get());
        let context_type = TypeId::new(profile.context_type().get());
        for id in [state_type, command_type, context_type] {
            require_schema_type(&schema, id)?;
        }
        if schema.root_type() != state_type {
            return Err(CatalogError::RootStateTypeMismatch {
                profile: state_type,
                schema: schema.root_type(),
            });
        }
        for effect in manifest.effects() {
            require_schema_type(&schema, effect.payload_type())?;
        }
        for channel in manifest.channels() {
            require_schema_type(&schema, channel.destination_type())?;
            require_schema_type(&schema, channel.payload_type())?;
        }
        validate_profile_registry(&profile, manifest.registry_entries())?;
        let profile_hash = profile.commitment::<H>().map_err(CatalogError::Profile)?;
        Ok(Self {
            profile,
            schema,
            manifest,
            limits,
            profile_hash,
            schema_hash,
        })
    }

    /// Returns the exact project profile.
    #[must_use]
    pub const fn profile(&self) -> &ProjectProfile {
        &self.profile
    }

    /// Returns the exact closed schema.
    #[must_use]
    pub const fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Returns the exact catalog manifest.
    #[must_use]
    pub const fn manifest(&self) -> &CatalogManifest {
        &self.manifest
    }

    /// Returns the deterministic resource envelope.
    #[must_use]
    pub const fn limits(&self) -> CatalogLimits {
        self.limits
    }

    /// Returns the exact profile commitment.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the exact schema commitment.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.schema_hash
    }

    /// Validates a stable reason against its decision class.
    pub fn validate_reason(
        &self,
        raw_id: u32,
        decision: DecisionKind,
    ) -> Result<&ReasonDefinition, CatalogError> {
        let id = semantic_id(raw_id, RegistryKind::Reason)?;
        let reason = self
            .manifest
            .reason(id)
            .ok_or(CatalogError::UnknownReason(raw_id))?;
        if !reason.disposition().admits(decision) {
            return Err(CatalogError::ReasonDispositionMismatch {
                id,
                expected: reason.disposition(),
                actual: decision,
            });
        }
        Ok(reason)
    }

    /// Validates an authoritative plan and returns exact resource metrics.
    pub fn validate_commit_plan(
        &self,
        commit_plan: &CommitPlan,
    ) -> Result<CatalogMetrics, CatalogError> {
        self.validate_plans(commit_plan, &OutboxPlan::empty())
    }

    /// Validates an outbox plan and returns exact resource metrics.
    pub fn validate_outbox_plan(
        &self,
        outbox_plan: &OutboxPlan,
    ) -> Result<CatalogMetrics, CatalogError> {
        self.validate_plans(&CommitPlan::empty(), outbox_plan)
    }

    /// Validates both plans against IDs, schemas, authority requirements, and limits.
    pub fn validate_plans(
        &self,
        commit_plan: &CommitPlan,
        outbox_plan: &OutboxPlan,
    ) -> Result<CatalogMetrics, CatalogError> {
        let effects =
            u32::try_from(commit_plan.effects().len()).map_err(|_| CatalogError::MetricOverflow)?;
        let outbox_entries =
            u32::try_from(outbox_plan.entries().len()).map_err(|_| CatalogError::MetricOverflow)?;
        if effects > self.limits.max_effects {
            return Err(CatalogError::EffectCountExceeded {
                limit: self.limits.max_effects,
                actual: effects,
            });
        }
        if outbox_entries > self.limits.max_outbox_entries {
            return Err(CatalogError::OutboxCountExceeded {
                limit: self.limits.max_outbox_entries,
                actual: outbox_entries,
            });
        }
        let mut metrics = CatalogMetrics {
            effects,
            outbox_entries,
            ..CatalogMetrics::default()
        };
        for effect in commit_plan.effects() {
            self.validate_effect(effect, &mut metrics)?;
        }
        for entry in outbox_plan.entries() {
            self.validate_outbox_entry(entry, &mut metrics)?;
        }
        Ok(metrics)
    }

    /// Computes a commitment to the complete catalog and limits.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, CatalogError> {
        if self.manifest.hash_algorithm_id() != H::ALGORITHM_ID {
            return Err(CatalogError::HashAlgorithmMismatch);
        }
        hash_canonical::<H>("zeno-fcis/project-catalog", self)
    }

    fn validate_effect(
        &self,
        effect: &Effect,
        metrics: &mut CatalogMetrics,
    ) -> Result<(), CatalogError> {
        let id = semantic_id(effect.operation(), RegistryKind::Effect)?;
        let definition = self
            .manifest
            .effect(id)
            .ok_or(CatalogError::UnknownEffect(effect.operation()))?;
        if !definition
            .authority_requirement()
            .admits(effect.authority())
        {
            return Err(CatalogError::EffectHashRequirementMismatch {
                ordinal: effect.ordinal(),
                field: EffectHashField::Authority,
            });
        }
        if !definition.subject_requirement().admits(effect.subject()) {
            return Err(CatalogError::EffectHashRequirementMismatch {
                ordinal: effect.ordinal(),
                field: EffectHashField::Subject,
            });
        }
        self.admit_value(
            ValueRole::EffectPayload,
            effect.ordinal(),
            definition.payload_type(),
            effect.payload(),
            metrics,
        )
    }

    fn validate_outbox_entry(
        &self,
        entry: &OutboxEntry,
        metrics: &mut CatalogMetrics,
    ) -> Result<(), CatalogError> {
        let id = semantic_id(entry.channel(), RegistryKind::Channel)?;
        let definition = self
            .manifest
            .channel(id)
            .ok_or(CatalogError::UnknownChannel(entry.channel()))?;
        self.admit_value(
            ValueRole::OutboxDestination,
            entry.ordinal(),
            definition.destination_type(),
            entry.destination(),
            metrics,
        )?;
        self.admit_value(
            ValueRole::OutboxPayload,
            entry.ordinal(),
            definition.payload_type(),
            entry.payload(),
            metrics,
        )
    }

    fn admit_value(
        &self,
        role: ValueRole,
        ordinal: u32,
        type_id: TypeId,
        value: &Value,
        aggregate: &mut CatalogMetrics,
    ) -> Result<(), CatalogError> {
        self.schema
            .validate_value(
                type_id,
                value,
                ValidationLimits {
                    max_depth: self.limits.max_value_depth,
                    max_nodes: self.limits.max_value_nodes,
                },
            )
            .map_err(|error| CatalogError::SchemaValue {
                role,
                ordinal,
                error,
            })?;
        let metrics = value
            .validate_limits(ValueLimits {
                max_depth: u32::from(self.limits.max_value_depth),
                max_nodes: self.limits.max_value_nodes,
                max_payload_bytes: self.limits.max_total_payload_bytes,
                max_collection_len: self.limits.max_collection_len,
            })
            .map_err(|error| CatalogError::StructuralValue {
                role,
                ordinal,
                error,
            })?;
        add_value_metrics(aggregate, metrics, self.limits)
    }
}

impl CanonicalEncode for ProjectCatalog {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-PROJECT-CATALOG\0");
        output.extend_from_slice(&CATALOG_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.profile_hash.as_bytes());
        output.extend_from_slice(self.schema_hash.as_bytes());
        put_blob(output, &self.manifest.canonical_bytes()?)?;
        self.limits.encode_to(output)
    }
}

fn normalize_reasons(
    mut definitions: Vec<ReasonDefinition>,
) -> Result<Box<[ReasonDefinition]>, CatalogError> {
    validate_definition_count(RegistryKind::Reason, definitions.len())?;
    definitions.sort_by_key(ReasonDefinition::id);
    validate_unique_definitions(
        RegistryKind::Reason,
        &definitions,
        ReasonDefinition::id,
        ReasonDefinition::name,
    )?;
    let mut precedence = definitions
        .iter()
        .map(ReasonDefinition::precedence)
        .collect::<Vec<_>>();
    precedence.sort_unstable();
    for (expected, actual) in precedence.into_iter().enumerate() {
        let expected = u32::try_from(expected).map_err(|_| CatalogError::MetricOverflow)?;
        if actual != expected {
            return Err(CatalogError::NonContiguousPrecedence { expected, actual });
        }
    }
    Ok(definitions.into_boxed_slice())
}

fn normalize_effects(
    mut definitions: Vec<EffectDefinition>,
) -> Result<Box<[EffectDefinition]>, CatalogError> {
    validate_definition_count(RegistryKind::Effect, definitions.len())?;
    definitions.sort_by_key(EffectDefinition::id);
    validate_unique_definitions(
        RegistryKind::Effect,
        &definitions,
        EffectDefinition::id,
        EffectDefinition::name,
    )?;
    Ok(definitions.into_boxed_slice())
}

fn normalize_channels(
    mut definitions: Vec<ChannelDefinition>,
) -> Result<Box<[ChannelDefinition]>, CatalogError> {
    validate_definition_count(RegistryKind::Channel, definitions.len())?;
    definitions.sort_by_key(ChannelDefinition::id);
    validate_unique_definitions(
        RegistryKind::Channel,
        &definitions,
        ChannelDefinition::id,
        ChannelDefinition::name,
    )?;
    Ok(definitions.into_boxed_slice())
}

fn validate_definition_count(kind: RegistryKind, count: usize) -> Result<(), CatalogError> {
    if count > MAX_CATALOG_DEFINITIONS {
        Err(CatalogError::TooManyDefinitions(kind))
    } else {
        Ok(())
    }
}

fn validate_unique_definitions<T, F, N>(
    kind: RegistryKind,
    definitions: &[T],
    id: F,
    name: N,
) -> Result<(), CatalogError>
where
    F: Fn(&T) -> SemanticId,
    N: Fn(&T) -> &StableName,
{
    for pair in definitions.windows(2) {
        if id(&pair[0]) == id(&pair[1]) {
            return Err(CatalogError::DuplicateDefinitionId {
                kind,
                id: id(&pair[0]),
            });
        }
    }
    let mut names = BTreeSet::new();
    for definition in definitions {
        if !names.insert(name(definition).clone()) {
            return Err(CatalogError::DuplicateDefinitionName(kind));
        }
    }
    Ok(())
}

fn reason_registry_hash<H: CommitmentHasher>(
    reasons: &[ReasonDefinition],
) -> Result<Hash32, CatalogError> {
    let mut ordered = reasons.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|reason| (reason.precedence(), reason.id()));
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&CATALOG_FORMAT_VERSION.to_be_bytes());
    put_length(&mut bytes, ordered.len())?;
    for reason in ordered {
        put_blob(&mut bytes, &reason.canonical_bytes()?)?;
    }
    hash_bytes::<H>("zeno-fcis/reason-registry", &bytes)
}

fn definition_registry_hash<H: CommitmentHasher, T: CanonicalEncode>(
    domain: &'static str,
    definitions: &[T],
) -> Result<Hash32, CatalogError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&CATALOG_FORMAT_VERSION.to_be_bytes());
    put_definitions(&mut bytes, definitions)?;
    hash_bytes::<H>(domain, &bytes)
}

fn validate_hash_algorithm_id(value: &str) -> Result<(), CatalogError> {
    if value.is_empty() || value.len() > MAX_HASH_ALGORITHM_ID_BYTES || !value.is_ascii() {
        Err(CatalogError::InvalidHashAlgorithmId)
    } else {
        Ok(())
    }
}

fn require_schema_type(schema: &Schema, type_id: TypeId) -> Result<(), CatalogError> {
    if schema.type_by_id(type_id).is_none() {
        Err(CatalogError::UnknownSchemaType(type_id))
    } else {
        Ok(())
    }
}

fn validate_profile_registry(
    profile: &ProjectProfile,
    expected: &[RegistryEntry],
) -> Result<(), CatalogError> {
    for entry in expected {
        let Some(actual) = profile.entry(entry.kind(), entry.id()) else {
            return Err(CatalogError::MissingProfileEntry {
                kind: entry.kind(),
                id: entry.id(),
            });
        };
        if actual != entry {
            return Err(CatalogError::ProfileEntryMismatch {
                kind: entry.kind(),
                id: entry.id(),
            });
        }
    }
    for actual in profile.entries().iter().filter(|entry| {
        matches!(
            entry.kind(),
            RegistryKind::Reason | RegistryKind::Effect | RegistryKind::Channel
        )
    }) {
        if expected
            .binary_search_by_key(&(actual.kind(), actual.id()), |entry| {
                (entry.kind(), entry.id())
            })
            .is_err()
        {
            return Err(CatalogError::UnexpectedProfileEntry {
                kind: actual.kind(),
                id: actual.id(),
            });
        }
    }
    Ok(())
}

fn semantic_id(raw: u32, kind: RegistryKind) -> Result<SemanticId, CatalogError> {
    SemanticId::try_new(raw).map_err(|_| CatalogError::ZeroPlanIdentifier(kind))
}

fn add_value_metrics(
    aggregate: &mut CatalogMetrics,
    value: ValueMetrics,
    limits: CatalogLimits,
) -> Result<(), CatalogError> {
    aggregate.value_nodes = aggregate
        .value_nodes
        .checked_add(value.nodes)
        .ok_or(CatalogError::MetricOverflow)?;
    aggregate.payload_bytes = aggregate
        .payload_bytes
        .checked_add(value.payload_bytes)
        .ok_or(CatalogError::MetricOverflow)?;
    aggregate.maximum_depth = aggregate.maximum_depth.max(value.depth);
    if aggregate.value_nodes > limits.max_total_value_nodes {
        return Err(CatalogError::AggregateNodeLimit {
            limit: limits.max_total_value_nodes,
            actual: aggregate.value_nodes,
        });
    }
    if aggregate.payload_bytes > limits.max_total_payload_bytes {
        return Err(CatalogError::AggregatePayloadLimit {
            limit: limits.max_total_payload_bytes,
            actual: aggregate.payload_bytes,
        });
    }
    Ok(())
}

fn hash_canonical<H: CommitmentHasher>(
    domain: &'static str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, CatalogError> {
    let bytes = value.canonical_bytes().map_err(CatalogError::Encode)?;
    hash_bytes::<H>(domain, &bytes)
}

fn hash_bytes<H: CommitmentHasher>(
    domain: &'static str,
    bytes: &[u8],
) -> Result<Hash32, CatalogError> {
    let domain = Domain::new(domain, CATALOG_FORMAT_VERSION).map_err(CatalogError::Encode)?;
    let hash = commitment::<H>(domain, bytes).map_err(CatalogError::Encode)?;
    if hash == Hash32::ZERO {
        Err(CatalogError::ZeroDerivedCommitment)
    } else {
        Ok(hash)
    }
}

fn put_definitions<T: CanonicalEncode>(
    output: &mut Vec<u8>,
    definitions: &[T],
) -> Result<(), EncodeError> {
    put_length(output, definitions.len())?;
    for definition in definitions {
        put_blob(output, &definition.canonical_bytes()?)?;
    }
    Ok(())
}

fn put_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    put_length(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Catalog construction or plan-admission failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogError {
    /// A caller supplied the all-zero commitment where absence was not permitted.
    ZeroCommitment,
    /// A derived registry or catalog commitment unexpectedly used the zero sentinel.
    ZeroDerivedCommitment,
    /// The hash-provider identity is empty, oversized, or non-ASCII.
    InvalidHashAlgorithmId,
    /// A custom value flow omitted its required registered claim.
    MissingCustomValueClaim,
    /// A value classification contained no flow descriptors.
    EmptyValueFlows,
    /// A value classification exceeded [`MAX_VALUE_FLOWS`].
    TooManyValueFlows,
    /// A value classification repeated one exact flow descriptor.
    DuplicateValueFlow,
    /// A manifest was used with a different commitment provider.
    HashAlgorithmMismatch,
    /// One namespace exceeds the definition-count bound.
    TooManyDefinitions(RegistryKind),
    /// Two definitions share one stable identifier.
    DuplicateDefinitionId {
        /// Registry namespace.
        kind: RegistryKind,
        /// Duplicated identifier.
        id: SemanticId,
    },
    /// Two definitions in one namespace share one stable name.
    DuplicateDefinitionName(RegistryKind),
    /// Reason precedence is not exactly the gap-free range `0..n`.
    NonContiguousPrecedence {
        /// Required next ordinal.
        expected: u32,
        /// Observed ordinal.
        actual: u32,
    },
    /// The plan/resource envelope is invalid or exceeds hard bounds.
    InvalidLimits,
    /// Closed-schema construction or commitment failed.
    Schema(SchemaError),
    /// Project-profile construction or commitment failed.
    Profile(ProfileError),
    /// Canonical encoding failed.
    Encode(EncodeError),
    /// Project profile binds a different schema.
    SchemaBindingMismatch,
    /// Project profile binds a different reason precedence.
    PrecedenceBindingMismatch,
    /// Project profile binds a different effect registry.
    EffectRegistryBindingMismatch,
    /// Project profile binds a different channel registry.
    ChannelRegistryBindingMismatch,
    /// A profile or definition references an unknown schema type.
    UnknownSchemaType(TypeId),
    /// The profile's state type differs from the schema's declared root type.
    RootStateTypeMismatch {
        /// State type declared by the project profile.
        profile: TypeId,
        /// Root type declared by the closed schema.
        schema: TypeId,
    },
    /// A catalog definition has no corresponding profile entry.
    MissingProfileEntry {
        /// Registry namespace.
        kind: RegistryKind,
        /// Stable identifier.
        id: SemanticId,
    },
    /// A profile entry does not match the catalog definition.
    ProfileEntryMismatch {
        /// Registry namespace.
        kind: RegistryKind,
        /// Stable identifier.
        id: SemanticId,
    },
    /// A profile contains a reason, effect, or channel absent from the catalog.
    UnexpectedProfileEntry {
        /// Registry namespace.
        kind: RegistryKind,
        /// Stable identifier.
        id: SemanticId,
    },
    /// A plan operation or channel used the forbidden zero identifier.
    ZeroPlanIdentifier(RegistryKind),
    /// A reason identifier is not registered.
    UnknownReason(u32),
    /// An effect operation is not registered.
    UnknownEffect(u32),
    /// An outbox channel is not registered.
    UnknownChannel(u32),
    /// A reason was used for the wrong decision class.
    ReasonDispositionMismatch {
        /// Stable reason identifier.
        id: SemanticId,
        /// Catalogued decision class.
        expected: ReasonDisposition,
        /// Actual decision kind.
        actual: DecisionKind,
    },
    /// An effect authority/subject commitment violates its definition.
    EffectHashRequirementMismatch {
        /// Effect ordinal.
        ordinal: u32,
        /// Invalid commitment field.
        field: EffectHashField,
    },
    /// A value does not satisfy its declared closed schema.
    SchemaValue {
        /// Position in the plan.
        role: ValueRole,
        /// Effect or outbox ordinal.
        ordinal: u32,
        /// Exact schema failure.
        error: ValueValidationError,
    },
    /// A value violates structural resource or canonical-order rules.
    StructuralValue {
        /// Position in the plan.
        role: ValueRole,
        /// Effect or outbox ordinal.
        ordinal: u32,
        /// Exact structural failure.
        error: ValueError,
    },
    /// Effect count exceeds the catalog envelope.
    EffectCountExceeded {
        /// Configured bound.
        limit: u32,
        /// Observed count.
        actual: u32,
    },
    /// Outbox count exceeds the catalog envelope.
    OutboxCountExceeded {
        /// Configured bound.
        limit: u32,
        /// Observed count.
        actual: u32,
    },
    /// Aggregate value nodes exceed the plan-pair envelope.
    AggregateNodeLimit {
        /// Configured bound.
        limit: u64,
        /// Observed count.
        actual: u64,
    },
    /// Aggregate payload bytes exceed the plan-pair envelope.
    AggregatePayloadLimit {
        /// Configured bound.
        limit: u64,
        /// Observed bytes.
        actual: u64,
    },
    /// Metric arithmetic or platform conversion overflowed.
    MetricOverflow,
}

impl From<EncodeError> for CatalogError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCommitment => formatter.write_str("catalog commitment is zero"),
            Self::ZeroDerivedCommitment => {
                formatter.write_str("derived catalog commitment is zero")
            }
            Self::InvalidHashAlgorithmId => formatter.write_str("invalid hash algorithm identity"),
            Self::MissingCustomValueClaim => {
                formatter.write_str("custom value flow is missing its registered claim")
            }
            Self::EmptyValueFlows => formatter.write_str("value semantics contain no flows"),
            Self::TooManyValueFlows => formatter.write_str("too many value-flow descriptors"),
            Self::DuplicateValueFlow => formatter.write_str("duplicate value-flow descriptor"),
            Self::HashAlgorithmMismatch => formatter.write_str("catalog hash algorithm mismatch"),
            Self::TooManyDefinitions(kind) => write!(formatter, "too many {kind:?} definitions"),
            Self::DuplicateDefinitionId { kind, id } => {
                write!(formatter, "duplicate {kind:?} definition {}", id.get())
            }
            Self::DuplicateDefinitionName(kind) => {
                write!(formatter, "duplicate {kind:?} definition name")
            }
            Self::NonContiguousPrecedence { expected, actual } => write!(
                formatter,
                "reason precedence expected {expected} but observed {actual}"
            ),
            Self::InvalidLimits => formatter.write_str("invalid catalog resource limits"),
            Self::Schema(error) => write!(formatter, "schema failed: {error}"),
            Self::Profile(error) => write!(formatter, "profile failed: {error}"),
            Self::Encode(error) => write!(formatter, "catalog encoding failed: {error}"),
            Self::SchemaBindingMismatch => formatter.write_str("profile schema binding mismatch"),
            Self::PrecedenceBindingMismatch => {
                formatter.write_str("profile precedence binding mismatch")
            }
            Self::EffectRegistryBindingMismatch => {
                formatter.write_str("profile effect-registry binding mismatch")
            }
            Self::ChannelRegistryBindingMismatch => {
                formatter.write_str("profile channel-registry binding mismatch")
            }
            Self::UnknownSchemaType(id) => write!(formatter, "unknown schema type {id}"),
            Self::RootStateTypeMismatch { profile, schema } => write!(
                formatter,
                "profile state type {profile} differs from schema root type {schema}"
            ),
            Self::MissingProfileEntry { kind, id } => {
                write!(formatter, "profile is missing {kind:?} entry {}", id.get())
            }
            Self::ProfileEntryMismatch { kind, id } => {
                write!(
                    formatter,
                    "profile {kind:?} entry {} differs from catalog",
                    id.get()
                )
            }
            Self::UnexpectedProfileEntry { kind, id } => {
                write!(formatter, "unexpected profile {kind:?} entry {}", id.get())
            }
            Self::ZeroPlanIdentifier(kind) => write!(formatter, "zero {kind:?} plan identifier"),
            Self::UnknownReason(id) => write!(formatter, "unknown reason {id}"),
            Self::UnknownEffect(id) => write!(formatter, "unknown effect {id}"),
            Self::UnknownChannel(id) => write!(formatter, "unknown channel {id}"),
            Self::ReasonDispositionMismatch { id, .. } => {
                write!(
                    formatter,
                    "reason {} has the wrong decision class",
                    id.get()
                )
            }
            Self::EffectHashRequirementMismatch { ordinal, field } => write!(
                formatter,
                "effect {ordinal} violates its {field:?} requirement"
            ),
            Self::SchemaValue {
                role,
                ordinal,
                error,
            } => write!(
                formatter,
                "{role:?} at ordinal {ordinal} failed schema: {error}"
            ),
            Self::StructuralValue {
                role,
                ordinal,
                error,
            } => write!(
                formatter,
                "{role:?} at ordinal {ordinal} failed structural validation: {error}"
            ),
            Self::EffectCountExceeded { limit, actual } => {
                write!(formatter, "effect count {actual} exceeds {limit}")
            }
            Self::OutboxCountExceeded { limit, actual } => {
                write!(formatter, "outbox count {actual} exceeds {limit}")
            }
            Self::AggregateNodeLimit { limit, actual } => {
                write!(formatter, "aggregate node count {actual} exceeds {limit}")
            }
            Self::AggregatePayloadLimit { limit, actual } => {
                write!(
                    formatter,
                    "aggregate payload bytes {actual} exceeds {limit}"
                )
            }
            Self::MetricOverflow => formatter.write_str("catalog metric arithmetic overflow"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CatalogError {}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use zeno_fcis_project::{DomainPrefix, ProfileBindings};
    use zeno_fcis_schema::{SchemaLimits, TypeDef, TypeKind};

    #[derive(Clone, Copy, Debug)]
    struct TestHasher;

    impl CommitmentHasher for TestHasher {
        const ALGORITHM_ID: &'static str = "test/catalog/1";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().copied().enumerate() {
                let slot = index % output.len();
                output[slot] = output[slot]
                    .wrapping_add(byte)
                    .rotate_left((index % 7) as u32);
            }
            if output == [0_u8; 32] {
                output[0] = 1;
            }
            Hash32::new(output)
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn name(value: &str) -> StableName {
        StableName::try_new(value).unwrap_or_else(|error| panic!("name: {error}"))
    }

    fn id(value: u32) -> SemanticId {
        SemanticId::try_new(value).unwrap_or_else(|error| panic!("id: {error}"))
    }

    fn type_def(id: u32, label: &str, kind: TypeKind) -> TypeDef {
        TypeDef::try_new(TypeId::new(id), label, kind, SchemaLimits::default())
            .unwrap_or_else(|error| panic!("type: {error}"))
    }

    fn schema() -> Schema {
        Schema::try_new(
            "CatalogFixture",
            1,
            TypeId::new(1),
            vec![
                type_def(1, "State", TypeKind::Bool),
                type_def(2, "Command", TypeKind::Bool),
                type_def(3, "Context", TypeKind::Bool),
                type_def(4, "Amount", TypeKind::U128 { min: 0, max: 100 }),
                type_def(
                    5,
                    "Destination",
                    TypeKind::Text {
                        min_len: 1,
                        max_len: 32,
                    },
                ),
                type_def(6, "Notification", TypeKind::Bool),
            ],
            SchemaLimits::default(),
        )
        .unwrap_or_else(|error| panic!("schema: {error}"))
    }

    fn definitions() -> (
        Vec<ReasonDefinition>,
        Vec<EffectDefinition>,
        Vec<ChannelDefinition>,
    ) {
        (
            vec![
                ReasonDefinition::try_new(
                    id(10),
                    name("denied"),
                    ReasonDisposition::Reject,
                    0,
                    hash(10),
                )
                .unwrap_or_else(|error| panic!("reason: {error}")),
            ],
            vec![
                EffectDefinition::try_new(
                    id(20),
                    name("write"),
                    TypeId::new(4),
                    HashRequirement::Present,
                    HashRequirement::Absent,
                    OperationSemantics::non_value(hash(120))
                        .unwrap_or_else(|error| panic!("semantics: {error}")),
                    hash(20),
                )
                .unwrap_or_else(|error| panic!("effect: {error}")),
            ],
            vec![
                ChannelDefinition::try_new(
                    id(30),
                    name("notify"),
                    TypeId::new(5),
                    TypeId::new(6),
                    OperationSemantics::non_value(hash(130))
                        .unwrap_or_else(|error| panic!("semantics: {error}")),
                    hash(30),
                )
                .unwrap_or_else(|error| panic!("channel: {error}")),
            ],
        )
    }

    fn manifest() -> CatalogManifest {
        let (reasons, effects, channels) = definitions();
        CatalogManifest::try_new::<TestHasher>(reasons, effects, channels)
            .unwrap_or_else(|error| panic!("manifest: {error}"))
    }

    fn root_entry(kind: RegistryKind, raw_id: u32, label: &str, byte: u8) -> RegistryEntry {
        RegistryEntry::try_new(kind, id(raw_id), name(label), hash(byte))
            .unwrap_or_else(|error| panic!("root entry: {error}"))
    }

    fn profile(schema: &Schema, manifest: &CatalogManifest) -> ProjectProfile {
        profile_with_effect_hash(
            schema,
            manifest,
            manifest.effect_registry_hash(),
            Vec::new(),
        )
    }

    fn profile_with_effect_hash(
        schema: &Schema,
        manifest: &CatalogManifest,
        effect_hash: Hash32,
        extras: Vec<RegistryEntry>,
    ) -> ProjectProfile {
        profile_with_state_type_and_effect_hash(schema, manifest, 1, effect_hash, extras)
    }

    fn profile_with_state_type_and_effect_hash(
        schema: &Schema,
        manifest: &CatalogManifest,
        state_type: u32,
        effect_hash: Hash32,
        mut extras: Vec<RegistryEntry>,
    ) -> ProjectProfile {
        let mut entries = vec![
            root_entry(RegistryKind::StateType, state_type, "state", 1),
            root_entry(RegistryKind::CommandType, 2, "command", 2),
            root_entry(RegistryKind::ContextType, 3, "context", 3),
        ];
        entries.extend_from_slice(manifest.registry_entries());
        entries.append(&mut extras);
        ProjectProfile::try_new(
            name("example"),
            name("core"),
            id(100),
            1,
            id(state_type),
            id(2),
            id(3),
            DomainPrefix::try_new("example/core").unwrap_or_else(|error| panic!("domain: {error}")),
            ProfileBindings {
                schema_hash: schema
                    .schema_hash::<TestHasher>()
                    .unwrap_or_else(|error| panic!("schema hash: {error}")),
                precedence_hash: manifest.precedence_hash(),
                algorithm_hash: hash(40),
                codec_hash: hash(41),
                effect_registry_hash: effect_hash,
                channel_registry_hash: manifest.channel_registry_hash(),
                policy_hash: hash(42),
            },
            entries,
        )
        .unwrap_or_else(|error| panic!("profile: {error}"))
    }

    fn catalog() -> ProjectCatalog {
        let schema = schema();
        let manifest = manifest();
        let profile = profile(&schema, &manifest);
        ProjectCatalog::try_new::<TestHasher>(profile, schema, manifest, CatalogLimits::default())
            .unwrap_or_else(|error| panic!("catalog: {error}"))
    }

    #[test]
    fn declaration_order_does_not_change_manifest() {
        let (mut reasons, mut effects, mut channels) = definitions();
        reasons.push(
            ReasonDefinition::try_new(
                id(11),
                name("committed-denial"),
                ReasonDisposition::CommittedFailure,
                1,
                hash(11),
            )
            .unwrap_or_else(|error| panic!("second reason: {error}")),
        );
        effects.push(
            EffectDefinition::try_new(
                id(21),
                name("write-secondary"),
                TypeId::new(4),
                HashRequirement::Any,
                HashRequirement::Any,
                OperationSemantics::non_value(hash(121))
                    .unwrap_or_else(|error| panic!("semantics: {error}")),
                hash(21),
            )
            .unwrap_or_else(|error| panic!("second effect: {error}")),
        );
        channels.push(
            ChannelDefinition::try_new(
                id(31),
                name("notify-secondary"),
                TypeId::new(5),
                TypeId::new(6),
                OperationSemantics::non_value(hash(131))
                    .unwrap_or_else(|error| panic!("semantics: {error}")),
                hash(31),
            )
            .unwrap_or_else(|error| panic!("second channel: {error}")),
        );
        let left = CatalogManifest::try_new::<TestHasher>(
            reasons.clone(),
            effects.clone(),
            channels.clone(),
        )
        .unwrap_or_else(|error| panic!("left: {error}"));
        let right = CatalogManifest::try_new::<TestHasher>(
            reasons.into_iter().rev().collect(),
            effects.into_iter().rev().collect(),
            channels.into_iter().rev().collect(),
        )
        .unwrap_or_else(|error| panic!("right: {error}"));
        assert_eq!(left, right);
        assert_eq!(left.canonical_bytes(), right.canonical_bytes());
    }

    #[test]
    fn value_flows_are_nonempty_bounded_canonical_sets() {
        assert_eq!(
            OperationSemantics::value(Vec::new(), hash(140)),
            Err(CatalogError::EmptyValueFlows)
        );
        let transfer = ValueFlow::standard(ValueFlowKind::Transfer, hash(141))
            .unwrap_or_else(|error| panic!("flow: {error}"));
        assert_eq!(
            OperationSemantics::value(vec![transfer, transfer], hash(142)),
            Err(CatalogError::DuplicateValueFlow)
        );
        let mint = ValueFlow::standard(ValueFlowKind::Mint, hash(141))
            .unwrap_or_else(|error| panic!("flow: {error}"));
        let left = OperationSemantics::value(vec![transfer, mint], hash(142))
            .unwrap_or_else(|error| panic!("semantics: {error}"));
        let right = OperationSemantics::value(vec![mint, transfer], hash(142))
            .unwrap_or_else(|error| panic!("semantics: {error}"));
        assert_eq!(left, right);
        assert_eq!(left.canonical_bytes(), right.canonical_bytes());

        let mut boundary = (1_u8..=64)
            .map(|byte| {
                ValueFlow::standard(ValueFlowKind::Transfer, hash(byte))
                    .unwrap_or_else(|error| panic!("boundary flow: {error}"))
            })
            .collect::<Vec<_>>();
        assert!(OperationSemantics::value(boundary.clone(), hash(143)).is_ok());
        boundary.push(
            ValueFlow::standard(ValueFlowKind::Transfer, hash(65))
                .unwrap_or_else(|error| panic!("one-over flow: {error}")),
        );
        assert_eq!(
            OperationSemantics::value(boundary, hash(143)),
            Err(CatalogError::TooManyValueFlows)
        );
    }

    #[test]
    fn custom_flow_requires_exact_nonzero_claim_bindings() {
        assert_eq!(
            ValueFlow::standard(ValueFlowKind::Custom, hash(150)),
            Err(CatalogError::MissingCustomValueClaim)
        );
        assert_eq!(
            ValueFlow::custom(hash(150), id(500), Hash32::ZERO),
            Err(CatalogError::ZeroCommitment)
        );
    }

    #[test]
    fn economic_reclassification_changes_registry_and_manifest_identity() {
        let (_, mut effects, _) = definitions();
        let non_value =
            CatalogManifest::try_new::<TestHasher>(Vec::new(), effects.clone(), Vec::new())
                .unwrap_or_else(|error| panic!("non-value manifest: {error}"));
        effects[0].semantics = OperationSemantics::value(
            vec![
                ValueFlow::standard(ValueFlowKind::Transfer, hash(151))
                    .unwrap_or_else(|error| panic!("flow: {error}")),
            ],
            hash(152),
        )
        .unwrap_or_else(|error| panic!("semantics: {error}"));
        let value = CatalogManifest::try_new::<TestHasher>(Vec::new(), effects, Vec::new())
            .unwrap_or_else(|error| panic!("value manifest: {error}"));
        assert_ne!(
            non_value.effect_registry_hash(),
            value.effect_registry_hash()
        );
        assert_ne!(
            non_value.commitment::<TestHasher>(),
            value.commitment::<TestHasher>()
        );
    }

    #[test]
    fn profile_state_type_must_equal_the_schema_root() {
        let schema = schema();
        let manifest = manifest();
        let profile = profile_with_state_type_and_effect_hash(
            &schema,
            &manifest,
            2,
            manifest.effect_registry_hash(),
            Vec::new(),
        );
        assert_eq!(
            ProjectCatalog::try_new::<TestHasher>(
                profile,
                schema,
                manifest,
                CatalogLimits::default(),
            ),
            Err(CatalogError::RootStateTypeMismatch {
                profile: TypeId::new(2),
                schema: TypeId::new(1),
            })
        );
    }

    #[test]
    fn reason_precedence_must_be_total_and_gap_free() {
        let reason = ReasonDefinition::try_new(
            id(10),
            name("denied"),
            ReasonDisposition::Reject,
            1,
            hash(10),
        )
        .unwrap_or_else(|error| panic!("reason: {error}"));
        assert_eq!(
            CatalogManifest::try_new::<TestHasher>(vec![reason], Vec::new(), Vec::new()),
            Err(CatalogError::NonContiguousPrecedence {
                expected: 0,
                actual: 1,
            })
        );
    }

    #[test]
    fn profile_must_bind_the_exact_effect_registry() {
        let schema = schema();
        let manifest = manifest();
        let profile = profile_with_effect_hash(&schema, &manifest, hash(99), Vec::new());
        assert_eq!(
            ProjectCatalog::try_new::<TestHasher>(
                profile,
                schema,
                manifest,
                CatalogLimits::default(),
            ),
            Err(CatalogError::EffectRegistryBindingMismatch)
        );
    }

    #[test]
    fn profile_may_not_hide_an_extra_effect() {
        let schema = schema();
        let manifest = manifest();
        let extra =
            RegistryEntry::try_new(RegistryKind::Effect, id(21), name("hidden-write"), hash(88))
                .unwrap_or_else(|error| panic!("extra: {error}"));
        let profile = profile_with_effect_hash(
            &schema,
            &manifest,
            manifest.effect_registry_hash(),
            vec![extra],
        );
        assert!(matches!(
            ProjectCatalog::try_new::<TestHasher>(
                profile,
                schema,
                manifest,
                CatalogLimits::default(),
            ),
            Err(CatalogError::UnexpectedProfileEntry {
                kind: RegistryKind::Effect,
                ..
            })
        ));
    }

    #[test]
    fn known_typed_plans_are_admitted_with_exact_metrics() {
        let catalog = catalog();
        let commit = CommitPlan::try_new(vec![Effect::new(
            1,
            20,
            hash(50),
            Hash32::ZERO,
            Value::U128(50),
        )])
        .unwrap_or_else(|error| panic!("commit: {error}"));
        let outbox = OutboxPlan::try_new(vec![OutboxEntry::new(
            1,
            30,
            Value::text_ascii(String::from("mail"))
                .unwrap_or_else(|error| panic!("destination: {error}")),
            Value::Bool(true),
        )])
        .unwrap_or_else(|error| panic!("outbox: {error}"));
        let metrics = catalog
            .validate_plans(&commit, &outbox)
            .unwrap_or_else(|error| panic!("validation: {error}"));
        assert_eq!(metrics.effects(), 1);
        assert_eq!(metrics.outbox_entries(), 1);
        assert_eq!(metrics.value_nodes(), 3);
        assert_eq!(metrics.payload_bytes(), 4);
    }

    #[test]
    fn unregistered_effects_fail_closed() {
        let commit = CommitPlan::try_new(vec![Effect::new(
            1,
            999,
            hash(50),
            Hash32::ZERO,
            Value::U128(50),
        )])
        .unwrap_or_else(|error| panic!("commit: {error}"));
        assert_eq!(
            catalog().validate_commit_plan(&commit),
            Err(CatalogError::UnknownEffect(999))
        );
    }

    #[test]
    fn effect_authority_requirement_is_enforced() {
        let commit = CommitPlan::try_new(vec![Effect::new(
            7,
            20,
            Hash32::ZERO,
            Hash32::ZERO,
            Value::U128(50),
        )])
        .unwrap_or_else(|error| panic!("commit: {error}"));
        assert_eq!(
            catalog().validate_commit_plan(&commit),
            Err(CatalogError::EffectHashRequirementMismatch {
                ordinal: 7,
                field: EffectHashField::Authority,
            })
        );
    }

    #[test]
    fn effect_subject_requirement_is_enforced() {
        let commit = CommitPlan::try_new(vec![Effect::new(
            8,
            20,
            hash(50),
            hash(51),
            Value::U128(50),
        )])
        .unwrap_or_else(|error| panic!("commit: {error}"));
        assert_eq!(
            catalog().validate_commit_plan(&commit),
            Err(CatalogError::EffectHashRequirementMismatch {
                ordinal: 8,
                field: EffectHashField::Subject,
            })
        );
    }

    #[test]
    fn effect_payload_must_match_its_schema() {
        let commit = CommitPlan::try_new(vec![Effect::new(
            1,
            20,
            hash(50),
            Hash32::ZERO,
            Value::Bool(true),
        )])
        .unwrap_or_else(|error| panic!("commit: {error}"));
        assert!(matches!(
            catalog().validate_commit_plan(&commit),
            Err(CatalogError::SchemaValue {
                role: ValueRole::EffectPayload,
                ordinal: 1,
                error: ValueValidationError::TypeMismatch,
            })
        ));
    }

    #[test]
    fn reason_cannot_cross_decision_classes() {
        let catalog = catalog();
        assert!(catalog.validate_reason(10, DecisionKind::Reject).is_ok());
        assert!(matches!(
            catalog.validate_reason(10, DecisionKind::CommittedFailure),
            Err(CatalogError::ReasonDispositionMismatch { .. })
        ));
        assert!(matches!(
            catalog.validate_reason(10, DecisionKind::Accept),
            Err(CatalogError::ReasonDispositionMismatch { .. })
        ));
    }

    #[test]
    fn channels_fail_closed_for_unknown_ids_and_wrong_shapes() {
        let unknown = OutboxPlan::try_new(vec![OutboxEntry::new(1, 999, Value::Unit, Value::Unit)])
            .unwrap_or_else(|error| panic!("unknown outbox: {error}"));
        assert_eq!(
            catalog().validate_outbox_plan(&unknown),
            Err(CatalogError::UnknownChannel(999))
        );

        let wrong_destination = OutboxPlan::try_new(vec![OutboxEntry::new(
            2,
            30,
            Value::Bool(true),
            Value::Bool(true),
        )])
        .unwrap_or_else(|error| panic!("destination outbox: {error}"));
        assert!(matches!(
            catalog().validate_outbox_plan(&wrong_destination),
            Err(CatalogError::SchemaValue {
                role: ValueRole::OutboxDestination,
                ordinal: 2,
                error: ValueValidationError::TypeMismatch,
            })
        ));

        let wrong_payload = OutboxPlan::try_new(vec![OutboxEntry::new(
            3,
            30,
            Value::text_ascii(String::from("mail"))
                .unwrap_or_else(|error| panic!("destination: {error}")),
            Value::U128(1),
        )])
        .unwrap_or_else(|error| panic!("payload outbox: {error}"));
        assert!(matches!(
            catalog().validate_outbox_plan(&wrong_payload),
            Err(CatalogError::SchemaValue {
                role: ValueRole::OutboxPayload,
                ordinal: 3,
                error: ValueValidationError::TypeMismatch,
            })
        ));
    }

    #[test]
    fn aggregate_resource_limits_are_enforced() {
        let schema = schema();
        let manifest = manifest();
        let profile = profile(&schema, &manifest);
        let limits = CatalogLimits::try_new(10, 10, 64, 100, 100, 5, 100)
            .unwrap_or_else(|error| panic!("limits: {error}"));
        let catalog = ProjectCatalog::try_new::<TestHasher>(profile, schema, manifest, limits)
            .unwrap_or_else(|error| panic!("catalog: {error}"));
        let outbox = OutboxPlan::try_new(vec![
            OutboxEntry::new(
                1,
                30,
                Value::text_ascii(String::from("one"))
                    .unwrap_or_else(|error| panic!("first destination: {error}")),
                Value::Bool(true),
            ),
            OutboxEntry::new(
                2,
                30,
                Value::text_ascii(String::from("two"))
                    .unwrap_or_else(|error| panic!("second destination: {error}")),
                Value::Bool(false),
            ),
        ])
        .unwrap_or_else(|error| panic!("outbox: {error}"));
        assert_eq!(
            catalog.validate_outbox_plan(&outbox),
            Err(CatalogError::AggregatePayloadLimit {
                limit: 5,
                actual: 6,
            })
        );
    }

    #[test]
    fn catalog_commitment_binds_profile_schema_manifest_and_limits() {
        let catalog = catalog();
        assert_ne!(
            catalog
                .commitment::<TestHasher>()
                .unwrap_or_else(|error| panic!("commitment: {error}")),
            Hash32::ZERO
        );
    }
}
