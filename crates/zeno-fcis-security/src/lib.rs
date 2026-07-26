//! Project-neutral information-flow, side-channel, and covert-channel policy values.
//!
//! Pure deterministic code can make logical work and observations explicit, but
//! physical leakage is a property of the deployed software, compiler, operating
//! system, hardware, and workload. This crate therefore separates:
//!
//! - an information-flow lattice;
//! - closed observation traces;
//! - explicit declassification;
//! - side/covert-channel rules;
//! - deployment mitigation contracts;
//! - empirical or mechanized security evidence;
//! - fail-closed promotion decisions.
//!
//! The crate does not claim that source-level equality implies physical
//! constant-time execution. Production promotion requires deployment-bound
//! evidence for every modeled side or covert channel.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};

/// Maximum compartments attached to one information-flow label.
pub const MAX_COMPARTMENTS: usize = 64;
/// Maximum observations retained in one trace.
pub const MAX_OBSERVATIONS: usize = 16_384;
/// Maximum observer clearances in one policy.
pub const MAX_OBSERVERS: usize = 4_096;
/// Maximum observation rules in one policy.
pub const MAX_LEAKAGE_RULES: usize = 4_096;
/// Maximum required mitigations in one policy or deployment.
pub const MAX_MITIGATIONS: usize = 128;
/// Maximum security evidence items evaluated at once.
pub const MAX_SECURITY_EVIDENCE: usize = 128;
/// Maximum capacity measurements evaluated at once.
pub const MAX_CAPACITY_EVIDENCE: usize = 4_096;
/// Maximum leakage reports evaluated by one promotion decision.
pub const MAX_LEAKAGE_REPORTS: usize = 4_096;
/// Maximum leakage blockers retained in one report.
pub const MAX_LEAKAGE_BLOCKERS: usize = 16_384;

/// Stable nonzero security-domain identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SecurityDomainId(u32);

impl SecurityDomainId {
    /// Creates a nonzero security-domain identity.
    pub const fn try_new(value: u32) -> Result<Self, SecurityError> {
        if value == 0 {
            Err(SecurityError::ZeroIdentifier)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the raw stable identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl CanonicalEncode for SecurityDomainId {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

/// Stable nonzero information compartment.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompartmentId(u32);

impl CompartmentId {
    /// Creates a nonzero compartment identity.
    pub const fn try_new(value: u32) -> Result<Self, SecurityError> {
        if value == 0 {
            Err(SecurityError::ZeroIdentifier)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the raw stable identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl CanonicalEncode for CompartmentId {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

/// Confidentiality/integrity label with canonical compartments.
///
/// Larger confidentiality values are more restrictive. Larger integrity values
/// are more trusted. Information may flow from `source` to `target` only when:
///
/// - source confidentiality is no greater than target confidentiality;
/// - every source compartment is present in the target;
/// - source integrity is no lower than target integrity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SecurityLabel {
    confidentiality: u16,
    integrity: u16,
    compartments: Box<[CompartmentId]>,
}

impl SecurityLabel {
    /// Creates a canonical information-flow label.
    pub fn try_new(
        confidentiality: u16,
        integrity: u16,
        mut compartments: Vec<CompartmentId>,
    ) -> Result<Self, SecurityError> {
        if compartments.len() > MAX_COMPARTMENTS {
            return Err(SecurityError::TooManyCompartments);
        }
        compartments.sort_unstable();
        compartments.dedup();
        Ok(Self {
            confidentiality,
            integrity,
            compartments: compartments.into_boxed_slice(),
        })
    }

    /// Returns the confidentiality level.
    #[must_use]
    pub const fn confidentiality(&self) -> u16 {
        self.confidentiality
    }

    /// Returns the integrity level.
    #[must_use]
    pub const fn integrity(&self) -> u16 {
        self.integrity
    }

    /// Returns compartments in canonical order.
    #[must_use]
    pub const fn compartments(&self) -> &[CompartmentId] {
        &self.compartments
    }

    /// Returns whether information with this label may flow to `target`.
    #[must_use]
    pub fn can_flow_to(&self, target: &Self) -> bool {
        self.confidentiality <= target.confidentiality
            && self.integrity >= target.integrity
            && self
                .compartments
                .iter()
                .all(|item| target.compartments.binary_search(item).is_ok())
    }
}

impl CanonicalEncode for SecurityLabel {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.confidentiality.to_be_bytes());
        output.extend_from_slice(&self.integrity.to_be_bytes());
        put_u16_length(output, self.compartments.len())?;
        for compartment in &self.compartments {
            compartment.encode_to(output)?;
        }
        Ok(())
    }
}

/// Observable surface through which information may leak.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ObservationKind {
    /// Intended protocol output.
    ExplicitOutput = 0,
    /// Error or rejection classification.
    ErrorClass = 1,
    /// Termination, crash, or completion behavior.
    Termination = 2,
    /// Output or response size.
    OutputLength = 3,
    /// Deterministic logical work or instruction class.
    LogicalWork = 4,
    /// Allocation count or allocation-size class.
    AllocationClass = 5,
    /// Secret-dependent branch class.
    BranchClass = 6,
    /// Secret-dependent memory-address class.
    MemoryAccessClass = 7,
    /// Scheduling, queueing, or wakeup behavior.
    Scheduling = 8,
    /// Explicit shared state or persistent storage.
    Storage = 9,
    /// Cache, TLB, predictor, or other microarchitectural state.
    Microarchitecture = 10,
    /// Network packet count, framing, or timing.
    Network = 11,
    /// Logs, traces, diagnostics, or metrics.
    Log = 12,
    /// Power consumption.
    Power = 13,
    /// Electromagnetic emanations.
    Electromagnetic = 14,
}

impl CanonicalEncode for ObservationKind {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Security classification of one observation channel.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ChannelClass {
    /// Intended, reviewed communication channel.
    Intended = 0,
    /// Incidental side channel observed by an attacker.
    Side = 1,
    /// Channel an untrusted component may actively modulate.
    Covert = 2,
}

impl CanonicalEncode for ChannelClass {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Explicit authority for a bounded information release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Declassification {
    authority_hash: Hash32,
    purpose_hash: Hash32,
    max_bits: u32,
}

impl Declassification {
    /// Creates an explicit bounded declassification.
    pub fn try_new(
        authority_hash: Hash32,
        purpose_hash: Hash32,
        max_bits: u32,
    ) -> Result<Self, SecurityError> {
        if hash_is_zero(authority_hash) || hash_is_zero(purpose_hash) {
            return Err(SecurityError::ZeroHash);
        }
        if max_bits == 0 {
            return Err(SecurityError::ZeroDeclassificationBudget);
        }
        Ok(Self {
            authority_hash,
            purpose_hash,
            max_bits,
        })
    }

    /// Returns the declassification-authority commitment.
    #[must_use]
    pub const fn authority_hash(self) -> Hash32 {
        self.authority_hash
    }

    /// Returns the reviewed purpose commitment.
    #[must_use]
    pub const fn purpose_hash(self) -> Hash32 {
        self.purpose_hash
    }

    /// Returns the maximum authorized released bits.
    #[must_use]
    pub const fn max_bits(self) -> u32 {
        self.max_bits
    }
}

impl CanonicalEncode for Declassification {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.authority_hash.as_bytes());
        output.extend_from_slice(self.purpose_hash.as_bytes());
        output.extend_from_slice(&self.max_bits.to_be_bytes());
        Ok(())
    }
}

/// One normalized observer-visible event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    observer: SecurityDomainId,
    kind: ObservationKind,
    channel: ChannelClass,
    label: SecurityLabel,
    value_hash: Hash32,
    quantity: u64,
    leakage_bits_upper_bound: u32,
    declassification: Option<Declassification>,
}

impl Observation {
    /// Creates a bounded observation.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        observer: SecurityDomainId,
        kind: ObservationKind,
        channel: ChannelClass,
        label: SecurityLabel,
        value_hash: Hash32,
        quantity: u64,
        leakage_bits_upper_bound: u32,
        declassification: Option<Declassification>,
    ) -> Result<Self, SecurityError> {
        if value_hash == Hash32::ZERO {
            return Err(SecurityError::ZeroHash);
        }
        Ok(Self {
            observer,
            kind,
            channel,
            label,
            value_hash,
            quantity,
            leakage_bits_upper_bound,
            declassification,
        })
    }

    /// Returns the observer.
    #[must_use]
    pub const fn observer(&self) -> SecurityDomainId {
        self.observer
    }

    /// Returns the observation kind.
    #[must_use]
    pub const fn kind(&self) -> ObservationKind {
        self.kind
    }

    /// Returns the channel classification.
    #[must_use]
    pub const fn channel(&self) -> ChannelClass {
        self.channel
    }

    /// Returns the information label.
    #[must_use]
    pub const fn label(&self) -> &SecurityLabel {
        &self.label
    }

    /// Returns the observed-value commitment.
    #[must_use]
    pub const fn value_hash(&self) -> Hash32 {
        self.value_hash
    }

    /// Returns the channel-specific public quantity.
    #[must_use]
    pub const fn quantity(&self) -> u64 {
        self.quantity
    }

    /// Returns the claimed upper bound on released bits.
    #[must_use]
    pub const fn leakage_bits_upper_bound(&self) -> u32 {
        self.leakage_bits_upper_bound
    }

    /// Returns explicit declassification, when present.
    #[must_use]
    pub const fn declassification(&self) -> Option<Declassification> {
        self.declassification
    }

    fn key(&self) -> ObservationKey {
        ObservationKey {
            observer: self.observer,
            kind: self.kind,
            channel: self.channel,
        }
    }
}

impl CanonicalEncode for Observation {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.observer.encode_to(output)?;
        self.kind.encode_to(output)?;
        self.channel.encode_to(output)?;
        put_blob(output, &self.label.canonical_bytes()?)?;
        output.extend_from_slice(self.value_hash.as_bytes());
        output.extend_from_slice(&self.quantity.to_be_bytes());
        output.extend_from_slice(&self.leakage_bits_upper_bound.to_be_bytes());
        match self.declassification {
            None => output.push(0),
            Some(value) => {
                output.push(1);
                value.encode_to(output)?;
            }
        }
        Ok(())
    }
}

/// Closed observation trace for one secret variant and one public input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationTrace {
    public_input_hash: Hash32,
    secret_variant_hash: Hash32,
    observations: Box<[Observation]>,
}

impl ObservationTrace {
    /// Creates a bounded trace. Observation order remains semantic.
    pub fn try_new(
        public_input_hash: Hash32,
        secret_variant_hash: Hash32,
        observations: Vec<Observation>,
    ) -> Result<Self, SecurityError> {
        if public_input_hash == Hash32::ZERO || secret_variant_hash == Hash32::ZERO {
            return Err(SecurityError::ZeroHash);
        }
        if observations.len() > MAX_OBSERVATIONS {
            return Err(SecurityError::TooManyObservations);
        }
        Ok(Self {
            public_input_hash,
            secret_variant_hash,
            observations: observations.into_boxed_slice(),
        })
    }

    /// Returns the public-input commitment.
    #[must_use]
    pub const fn public_input_hash(&self) -> Hash32 {
        self.public_input_hash
    }

    /// Returns the secret-variant commitment.
    #[must_use]
    pub const fn secret_variant_hash(&self) -> Hash32 {
        self.secret_variant_hash
    }

    /// Returns observations in semantic order.
    #[must_use]
    pub const fn observations(&self) -> &[Observation] {
        &self.observations
    }

    /// Computes the trace commitment.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, SecurityError> {
        hash_canonical::<H>("zeno-fcis/security-trace", self)
    }
}

impl CanonicalEncode for ObservationTrace {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-SECURITY-TRACE\0");
        output.extend_from_slice(self.public_input_hash.as_bytes());
        output.extend_from_slice(self.secret_variant_hash.as_bytes());
        put_u32_length(output, self.observations.len())?;
        for observation in &self.observations {
            put_blob(output, &observation.canonical_bytes()?)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ObservationKey {
    observer: SecurityDomainId,
    kind: ObservationKind,
    channel: ChannelClass,
}

impl CanonicalEncode for ObservationKey {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.observer.encode_to(output)?;
        self.kind.encode_to(output)?;
        self.channel.encode_to(output)
    }
}

/// Admitted relation between two traces for one observation class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleMode {
    /// This observation must not occur.
    Prohibit,
    /// Value, quantity, shape, and declassification absence must be identical.
    Exact,
    /// Value and shape must be identical; quantity may differ by a public bound.
    BoundedQuantity {
        /// Maximum absolute quantity difference.
        max_delta: u64,
    },
    /// Value differences are permitted only under this exact declassification.
    Declassified {
        /// Required release authority.
        authority_hash: Hash32,
        /// Required release purpose.
        purpose_hash: Hash32,
        /// Maximum bits released by one observation.
        max_bits: u32,
    },
}

impl CanonicalEncode for RuleMode {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Prohibit => output.push(0),
            Self::Exact => output.push(1),
            Self::BoundedQuantity { max_delta } => {
                output.push(2);
                output.extend_from_slice(&max_delta.to_be_bytes());
            }
            Self::Declassified {
                authority_hash,
                purpose_hash,
                max_bits,
            } => {
                output.push(3);
                output.extend_from_slice(authority_hash.as_bytes());
                output.extend_from_slice(purpose_hash.as_bytes());
                output.extend_from_slice(&max_bits.to_be_bytes());
            }
        }
        Ok(())
    }
}

/// One observer/channel leakage rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeakageRule {
    key: ObservationKey,
    mode: RuleMode,
}

impl LeakageRule {
    /// Creates a leakage rule.
    pub fn try_new(
        observer: SecurityDomainId,
        kind: ObservationKind,
        channel: ChannelClass,
        mode: RuleMode,
    ) -> Result<Self, SecurityError> {
        if let RuleMode::Declassified {
            authority_hash,
            purpose_hash,
            max_bits,
        } = mode
        {
            if hash_is_zero(authority_hash) || hash_is_zero(purpose_hash) {
                return Err(SecurityError::ZeroHash);
            }
            if max_bits == 0 {
                return Err(SecurityError::ZeroDeclassificationBudget);
            }
        }
        Ok(Self {
            key: ObservationKey {
                observer,
                kind,
                channel,
            },
            mode,
        })
    }

    /// Returns the observer.
    #[must_use]
    pub const fn observer(self) -> SecurityDomainId {
        self.key.observer
    }

    /// Returns the observation kind.
    #[must_use]
    pub const fn kind(self) -> ObservationKind {
        self.key.kind
    }

    /// Returns the channel classification.
    #[must_use]
    pub const fn channel(self) -> ChannelClass {
        self.key.channel
    }

    /// Returns the admitted relation.
    #[must_use]
    pub const fn mode(self) -> RuleMode {
        self.mode
    }

    fn key(self) -> ObservationKey {
        self.key
    }
}

impl CanonicalEncode for LeakageRule {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.key.encode_to(output)?;
        self.mode.encode_to(output)
    }
}

/// Deployment mitigation required by a leakage policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Mitigation {
    /// Secret values do not influence control flow.
    ConstantTimeControlFlow = 0,
    /// Secret values do not influence memory addresses.
    SecretIndependentMemoryAccess = 1,
    /// Observer-visible output sizes are padded.
    FixedOutputSize = 2,
    /// Logical work is padded or otherwise secret independent.
    FixedWork = 3,
    /// Termination and error behavior is normalized.
    FixedTermination = 4,
    /// Responses or events are rate limited.
    RateLimit = 5,
    /// Queues are not shared across security domains.
    QueuePartition = 6,
    /// Processor cores are dedicated or time partitioned.
    CorePartition = 7,
    /// Cache/TLB resources are partitioned.
    CachePartition = 8,
    /// Physical memory is partitioned.
    MemoryPartition = 9,
    /// DMA is constrained by an IOMMU or equivalent.
    IommuIsolation = 10,
    /// Microarchitectural state is reset at security-domain switches.
    FlushMicroarchitecture = 11,
    /// Scheduling is deterministic with respect to secret state.
    DeterministicScheduling = 12,
    /// Logs and diagnostics are redacted.
    LogRedaction = 13,
    /// Core dumps and comparable memory snapshots are disabled.
    CoreDumpDisabled = 14,
    /// Secret buffers are zeroized on release.
    SecretZeroization = 15,
    /// Security domains do not share mutable state without a reviewed channel.
    NoSharedMutableState = 16,
    /// Compiler output is translation validated or otherwise independently checked.
    VerifiedCompilation = 17,
}

impl CanonicalEncode for Mitigation {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Exact deployed target and its claimed mitigations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeploymentContract {
    target_hash: Hash32,
    hardware_hash: Hash32,
    operating_system_hash: Hash32,
    compiler_hash: Hash32,
    topology_hash: Hash32,
    scheduler_hash: Hash32,
    mitigations: Box<[Mitigation]>,
}

impl DeploymentContract {
    /// Creates a content-bound deployment contract.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        target_hash: Hash32,
        hardware_hash: Hash32,
        operating_system_hash: Hash32,
        compiler_hash: Hash32,
        topology_hash: Hash32,
        scheduler_hash: Hash32,
        mut mitigations: Vec<Mitigation>,
    ) -> Result<Self, SecurityError> {
        if [
            target_hash,
            hardware_hash,
            operating_system_hash,
            compiler_hash,
            topology_hash,
            scheduler_hash,
        ]
        .contains(&Hash32::ZERO)
        {
            return Err(SecurityError::ZeroHash);
        }
        if mitigations.len() > MAX_MITIGATIONS {
            return Err(SecurityError::TooManyMitigations);
        }
        mitigations.sort_unstable();
        mitigations.dedup();
        Ok(Self {
            target_hash,
            hardware_hash,
            operating_system_hash,
            compiler_hash,
            topology_hash,
            scheduler_hash,
            mitigations: mitigations.into_boxed_slice(),
        })
    }

    /// Returns the deployment target commitment.
    #[must_use]
    pub const fn target_hash(&self) -> Hash32 {
        self.target_hash
    }

    /// Returns declared mitigations in canonical order.
    #[must_use]
    pub const fn mitigations(&self) -> &[Mitigation] {
        &self.mitigations
    }

    /// Returns whether the deployment declares a mitigation.
    #[must_use]
    pub fn has_mitigation(&self, mitigation: Mitigation) -> bool {
        self.mitigations.binary_search(&mitigation).is_ok()
    }

    /// Computes the deployment-contract commitment.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, SecurityError> {
        hash_canonical::<H>("zeno-fcis/security-deployment", self)
    }
}

impl CanonicalEncode for DeploymentContract {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-SECURITY-DEPLOYMENT\0");
        for hash in [
            self.target_hash,
            self.hardware_hash,
            self.operating_system_hash,
            self.compiler_hash,
            self.topology_hash,
            self.scheduler_hash,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        put_u16_length(output, self.mitigations.len())?;
        for mitigation in &self.mitigations {
            mitigation.encode_to(output)?;
        }
        Ok(())
    }
}

/// Observer clearance used by the information-flow check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObserverClearance {
    observer: SecurityDomainId,
    clearance: SecurityLabel,
}

impl ObserverClearance {
    /// Creates one observer clearance.
    #[must_use]
    pub fn new(observer: SecurityDomainId, clearance: SecurityLabel) -> Self {
        Self {
            observer,
            clearance,
        }
    }

    /// Returns the observer.
    #[must_use]
    pub const fn observer(&self) -> SecurityDomainId {
        self.observer
    }

    /// Returns the clearance label.
    #[must_use]
    pub const fn clearance(&self) -> &SecurityLabel {
        &self.clearance
    }
}

impl CanonicalEncode for ObserverClearance {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.observer.encode_to(output)?;
        put_blob(output, &self.clearance.canonical_bytes()?)
    }
}

/// Complete project/deployment leakage policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeakagePolicy {
    threat_model_hash: Hash32,
    deployment_contract_hash: Hash32,
    max_total_declassified_bits: u64,
    observers: Box<[ObserverClearance]>,
    rules: Box<[LeakageRule]>,
    required_mitigations: Box<[Mitigation]>,
}

impl LeakagePolicy {
    /// Creates a canonical fail-closed leakage policy.
    pub fn try_new(
        threat_model_hash: Hash32,
        deployment_contract_hash: Hash32,
        max_total_declassified_bits: u64,
        mut observers: Vec<ObserverClearance>,
        mut rules: Vec<LeakageRule>,
        mut required_mitigations: Vec<Mitigation>,
    ) -> Result<Self, SecurityError> {
        if threat_model_hash == Hash32::ZERO || deployment_contract_hash == Hash32::ZERO {
            return Err(SecurityError::ZeroHash);
        }
        if observers.len() > MAX_OBSERVERS {
            return Err(SecurityError::TooManyObservers);
        }
        if rules.is_empty() {
            return Err(SecurityError::EmptyLeakagePolicy);
        }
        if rules.len() > MAX_LEAKAGE_RULES {
            return Err(SecurityError::TooManyLeakageRules);
        }
        if required_mitigations.len() > MAX_MITIGATIONS {
            return Err(SecurityError::TooManyMitigations);
        }

        observers.sort_by_key(ObserverClearance::observer);
        if observers
            .windows(2)
            .any(|pair| pair[0].observer == pair[1].observer)
        {
            return Err(SecurityError::DuplicateObserver);
        }

        rules.sort_by_key(|rule| rule.key());
        if rules.windows(2).any(|pair| pair[0].key() == pair[1].key()) {
            return Err(SecurityError::DuplicateLeakageRule);
        }

        required_mitigations.sort_unstable();
        required_mitigations.dedup();

        Ok(Self {
            threat_model_hash,
            deployment_contract_hash,
            max_total_declassified_bits,
            observers: observers.into_boxed_slice(),
            rules: rules.into_boxed_slice(),
            required_mitigations: required_mitigations.into_boxed_slice(),
        })
    }

    /// Returns the threat-model commitment.
    #[must_use]
    pub const fn threat_model_hash(&self) -> Hash32 {
        self.threat_model_hash
    }

    /// Returns the required deployment-contract commitment.
    #[must_use]
    pub const fn deployment_contract_hash(&self) -> Hash32 {
        self.deployment_contract_hash
    }

    /// Returns leakage rules in canonical order.
    #[must_use]
    pub const fn rules(&self) -> &[LeakageRule] {
        &self.rules
    }

    /// Returns required mitigations.
    #[must_use]
    pub const fn required_mitigations(&self) -> &[Mitigation] {
        &self.required_mitigations
    }

    fn rule(&self, key: ObservationKey) -> Option<LeakageRule> {
        self.rules
            .binary_search_by_key(&key, |rule| rule.key())
            .ok()
            .map(|index| self.rules[index])
    }

    fn clearance(&self, observer: SecurityDomainId) -> Option<&SecurityLabel> {
        self.observers
            .binary_search_by_key(&observer, ObserverClearance::observer)
            .ok()
            .map(|index| self.observers[index].clearance())
    }

    /// Computes the policy commitment.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, SecurityError> {
        hash_canonical::<H>("zeno-fcis/security-policy", self)
    }
}

impl CanonicalEncode for LeakagePolicy {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-SECURITY-POLICY\0");
        output.extend_from_slice(self.threat_model_hash.as_bytes());
        output.extend_from_slice(self.deployment_contract_hash.as_bytes());
        output.extend_from_slice(&self.max_total_declassified_bits.to_be_bytes());

        put_u16_length(output, self.observers.len())?;
        for observer in &self.observers {
            put_blob(output, &observer.canonical_bytes()?)?;
        }

        put_u32_length(output, self.rules.len())?;
        for rule in &self.rules {
            put_blob(output, &rule.canonical_bytes()?)?;
        }

        put_u16_length(output, self.required_mitigations.len())?;
        for mitigation in &self.required_mitigations {
            mitigation.encode_to(output)?;
        }
        Ok(())
    }
}

/// One fail-closed leakage-policy violation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LeakageBlocker {
    /// Traces do not bind the same public input.
    PublicInputMismatch,
    /// A comparison used the same secret variant twice.
    SameSecretVariant,
    /// Deployment commitment differs from the policy.
    DeploymentMismatch,
    /// A required deployment mitigation is absent.
    MissingMitigation(Mitigation),
    /// Trace lengths differ.
    TraceLengthMismatch {
        /// Left observation count.
        left: u32,
        /// Right observation count.
        right: u32,
    },
    /// Observation key or label differs at one position.
    ObservationShapeMismatch {
        /// Observation index.
        index: u32,
    },
    /// No policy rule classifies an observation.
    UnclassifiedObservation {
        /// Observation index.
        index: u32,
    },
    /// An observer has no declared clearance.
    MissingObserverClearance {
        /// Observation index.
        index: u32,
    },
    /// Information label cannot flow to the observer.
    LabelFlowViolation {
        /// Observation index.
        index: u32,
    },
    /// A prohibited observation occurred.
    ProhibitedObservation {
        /// Observation index.
        index: u32,
    },
    /// Exact-policy value commitments differ.
    ValueMismatch {
        /// Observation index.
        index: u32,
    },
    /// Exact-policy quantities differ.
    QuantityMismatch {
        /// Observation index.
        index: u32,
    },
    /// A bounded quantity exceeded its delta.
    QuantityDeltaExceeded {
        /// Observation index.
        index: u32,
        /// Observed absolute delta.
        delta: u64,
        /// Allowed delta.
        maximum: u64,
    },
    /// Declassification appeared where it is not allowed.
    UnexpectedDeclassification {
        /// Observation index.
        index: u32,
    },
    /// Required declassification is missing or differs.
    DeclassificationMismatch {
        /// Observation index.
        index: u32,
    },
    /// Released bits exceed the per-observation rule.
    DeclassificationBitsExceeded {
        /// Observation index.
        index: u32,
        /// Observed bound.
        observed: u32,
        /// Allowed bound.
        maximum: u32,
    },
    /// Aggregate declassification exceeds the policy.
    TotalDeclassificationBitsExceeded {
        /// Observed aggregate bound.
        observed: u64,
        /// Allowed aggregate bound.
        maximum: u64,
    },
    /// Leakage-bit metadata was present without declassification.
    UnjustifiedLeakageBits {
        /// Observation index.
        index: u32,
    },
    /// Arithmetic overflowed while accumulating a leakage bound.
    ArithmeticOverflow,
}

impl CanonicalEncode for LeakageBlocker {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::PublicInputMismatch => output.push(0),
            Self::SameSecretVariant => output.push(1),
            Self::DeploymentMismatch => output.push(2),
            Self::MissingMitigation(value) => {
                output.push(3);
                value.encode_to(output)?;
            }
            Self::TraceLengthMismatch { left, right } => {
                output.push(4);
                output.extend_from_slice(&left.to_be_bytes());
                output.extend_from_slice(&right.to_be_bytes());
            }
            Self::ObservationShapeMismatch { index } => {
                output.push(5);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::UnclassifiedObservation { index } => {
                output.push(6);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::MissingObserverClearance { index } => {
                output.push(7);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::LabelFlowViolation { index } => {
                output.push(8);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::ProhibitedObservation { index } => {
                output.push(9);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::ValueMismatch { index } => {
                output.push(10);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::QuantityMismatch { index } => {
                output.push(11);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::QuantityDeltaExceeded {
                index,
                delta,
                maximum,
            } => {
                output.push(12);
                output.extend_from_slice(&index.to_be_bytes());
                output.extend_from_slice(&delta.to_be_bytes());
                output.extend_from_slice(&maximum.to_be_bytes());
            }
            Self::UnexpectedDeclassification { index } => {
                output.push(13);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::DeclassificationMismatch { index } => {
                output.push(14);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::DeclassificationBitsExceeded {
                index,
                observed,
                maximum,
            } => {
                output.push(15);
                output.extend_from_slice(&index.to_be_bytes());
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&maximum.to_be_bytes());
            }
            Self::TotalDeclassificationBitsExceeded { observed, maximum } => {
                output.push(16);
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&maximum.to_be_bytes());
            }
            Self::UnjustifiedLeakageBits { index } => {
                output.push(17);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::ArithmeticOverflow => output.push(18),
        }
        Ok(())
    }
}

/// Result of comparing two secret variants under one leakage policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeakageReport {
    deployment_hash: Hash32,
    declassified_bits: u64,
    blockers: Box<[LeakageBlocker]>,
}

impl LeakageReport {
    /// Returns true only when no blocker was found.
    #[must_use]
    pub fn is_secure(&self) -> bool {
        self.blockers.is_empty()
    }

    /// Returns the deployment commitment used by the comparison.
    #[must_use]
    pub const fn deployment_hash(&self) -> Hash32 {
        self.deployment_hash
    }

    /// Returns the aggregate explicit declassification bound.
    #[must_use]
    pub const fn declassified_bits(&self) -> u64 {
        self.declassified_bits
    }

    /// Returns blockers in deterministic evaluation order.
    #[must_use]
    pub const fn blockers(&self) -> &[LeakageBlocker] {
        &self.blockers
    }
}

impl CanonicalEncode for LeakageReport {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.deployment_hash.as_bytes());
        output.extend_from_slice(&self.declassified_bits.to_be_bytes());
        put_u32_length(output, self.blockers.len())?;
        for blocker in &self.blockers {
            put_blob(output, &blocker.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Compares two traces that differ only in secret state.
///
/// This establishes observational equality only for the closed observation model
/// and exact deployment contract supplied by the caller. It is not a physical
/// side-channel measurement.
pub fn compare_traces<H: CommitmentHasher>(
    policy: &LeakagePolicy,
    deployment: &DeploymentContract,
    left: &ObservationTrace,
    right: &ObservationTrace,
) -> LeakageReport {
    let deployment_hash = deployment.commitment::<H>().unwrap_or(Hash32::ZERO);
    let mut blockers = Vec::new();
    let mut declassified_bits = 0_u64;

    if deployment_hash != policy.deployment_contract_hash {
        push_blocker(&mut blockers, LeakageBlocker::DeploymentMismatch);
    }
    for mitigation in &policy.required_mitigations {
        if !deployment.has_mitigation(*mitigation) {
            push_blocker(
                &mut blockers,
                LeakageBlocker::MissingMitigation(*mitigation),
            );
        }
    }
    if left.public_input_hash != right.public_input_hash {
        push_blocker(&mut blockers, LeakageBlocker::PublicInputMismatch);
    }
    if left.secret_variant_hash == right.secret_variant_hash {
        push_blocker(&mut blockers, LeakageBlocker::SameSecretVariant);
    }

    if left.observations.len() != right.observations.len() {
        let left_length = u32::try_from(left.observations.len()).unwrap_or(u32::MAX);
        let right_length = u32::try_from(right.observations.len()).unwrap_or(u32::MAX);
        push_blocker(
            &mut blockers,
            LeakageBlocker::TraceLengthMismatch {
                left: left_length,
                right: right_length,
            },
        );
    }

    for (position, (left_observation, right_observation)) in left
        .observations
        .iter()
        .zip(right.observations.iter())
        .enumerate()
    {
        let index = u32::try_from(position).unwrap_or(u32::MAX);
        if left_observation.key() != right_observation.key()
            || left_observation.label != right_observation.label
        {
            push_blocker(
                &mut blockers,
                LeakageBlocker::ObservationShapeMismatch { index },
            );
            continue;
        }

        let key = left_observation.key();
        let Some(rule) = policy.rule(key) else {
            push_blocker(
                &mut blockers,
                LeakageBlocker::UnclassifiedObservation { index },
            );
            continue;
        };

        let Some(clearance) = policy.clearance(key.observer) else {
            push_blocker(
                &mut blockers,
                LeakageBlocker::MissingObserverClearance { index },
            );
            continue;
        };

        match rule.mode {
            RuleMode::Prohibit => {
                push_blocker(
                    &mut blockers,
                    LeakageBlocker::ProhibitedObservation { index },
                );
            }
            RuleMode::Exact => {
                check_undeclassified(left_observation, right_observation, index, &mut blockers);
                if !left_observation.label.can_flow_to(clearance) {
                    push_blocker(&mut blockers, LeakageBlocker::LabelFlowViolation { index });
                }
                if left_observation.value_hash != right_observation.value_hash {
                    push_blocker(&mut blockers, LeakageBlocker::ValueMismatch { index });
                }
                if left_observation.quantity != right_observation.quantity {
                    push_blocker(&mut blockers, LeakageBlocker::QuantityMismatch { index });
                }
            }
            RuleMode::BoundedQuantity { max_delta } => {
                check_undeclassified(left_observation, right_observation, index, &mut blockers);
                if !left_observation.label.can_flow_to(clearance) {
                    push_blocker(&mut blockers, LeakageBlocker::LabelFlowViolation { index });
                }
                if left_observation.value_hash != right_observation.value_hash {
                    push_blocker(&mut blockers, LeakageBlocker::ValueMismatch { index });
                }
                let delta = left_observation
                    .quantity
                    .abs_diff(right_observation.quantity);
                if delta > max_delta {
                    push_blocker(
                        &mut blockers,
                        LeakageBlocker::QuantityDeltaExceeded {
                            index,
                            delta,
                            maximum: max_delta,
                        },
                    );
                }
            }
            RuleMode::Declassified {
                authority_hash,
                purpose_hash,
                max_bits,
            } => {
                let expected = Declassification {
                    authority_hash,
                    purpose_hash,
                    max_bits,
                };
                if left_observation.declassification != Some(expected)
                    || right_observation.declassification != Some(expected)
                {
                    push_blocker(
                        &mut blockers,
                        LeakageBlocker::DeclassificationMismatch { index },
                    );
                    continue;
                }
                let observed = left_observation
                    .leakage_bits_upper_bound
                    .max(right_observation.leakage_bits_upper_bound);
                if observed > max_bits {
                    push_blocker(
                        &mut blockers,
                        LeakageBlocker::DeclassificationBitsExceeded {
                            index,
                            observed,
                            maximum: max_bits,
                        },
                    );
                }
                match declassified_bits.checked_add(u64::from(observed)) {
                    Some(value) => declassified_bits = value,
                    None => push_blocker(&mut blockers, LeakageBlocker::ArithmeticOverflow),
                }
            }
        }
    }

    if declassified_bits > policy.max_total_declassified_bits {
        push_blocker(
            &mut blockers,
            LeakageBlocker::TotalDeclassificationBitsExceeded {
                observed: declassified_bits,
                maximum: policy.max_total_declassified_bits,
            },
        );
    }

    LeakageReport {
        deployment_hash,
        declassified_bits,
        blockers: blockers.into_boxed_slice(),
    }
}

fn check_undeclassified(
    left: &Observation,
    right: &Observation,
    index: u32,
    blockers: &mut Vec<LeakageBlocker>,
) {
    if left.declassification.is_some() || right.declassification.is_some() {
        push_blocker(
            blockers,
            LeakageBlocker::UnexpectedDeclassification { index },
        );
    }
    if left.leakage_bits_upper_bound != 0 || right.leakage_bits_upper_bound != 0 {
        push_blocker(blockers, LeakageBlocker::UnjustifiedLeakageBits { index });
    }
}

fn push_blocker(blockers: &mut Vec<LeakageBlocker>, blocker: LeakageBlocker) {
    if blockers.len() < MAX_LEAKAGE_BLOCKERS {
        blockers.push(blocker);
    }
}

/// Security evidence class required by a production policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum SecurityEvidenceKind {
    /// Source-level information-flow or noninterference proof.
    NoninterferenceProof = 0,
    /// Symbolic constant-time analysis of compiled code.
    ConstantTimeSymbolic = 1,
    /// Dynamic branch/memory constant-time analysis.
    ConstantTimeDynamic = 2,
    /// Statistical timing analysis.
    TimingStatistical = 3,
    /// Cache/TLB/predictor channel experiment.
    MicroarchitecturalExperiment = 4,
    /// Storage-channel and authority-graph analysis.
    StorageChannelAnalysis = 5,
    /// Active covert-channel capacity experiment.
    CovertChannelCapacity = 6,
    /// Compiler or binary translation validation.
    TranslationValidation = 7,
    /// Deployment configuration audit.
    DeploymentAudit = 8,
    /// Hardware/firmware measurement or attestation.
    HardwareAttestation = 9,
}

impl CanonicalEncode for SecurityEvidenceKind {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Content-bound security evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecurityEvidence {
    kind: SecurityEvidenceKind,
    claim_hash: Hash32,
    artifact_hash: Hash32,
    toolchain_hash: Hash32,
    deployment_hash: Hash32,
}

impl SecurityEvidence {
    /// Creates deployment-bound evidence.
    pub fn try_new(
        kind: SecurityEvidenceKind,
        claim_hash: Hash32,
        artifact_hash: Hash32,
        toolchain_hash: Hash32,
        deployment_hash: Hash32,
    ) -> Result<Self, SecurityError> {
        if hash_is_zero(claim_hash)
            || hash_is_zero(artifact_hash)
            || hash_is_zero(toolchain_hash)
            || hash_is_zero(deployment_hash)
        {
            return Err(SecurityError::ZeroHash);
        }
        Ok(Self {
            kind,
            claim_hash,
            artifact_hash,
            toolchain_hash,
            deployment_hash,
        })
    }

    /// Returns the evidence kind.
    #[must_use]
    pub const fn kind(self) -> SecurityEvidenceKind {
        self.kind
    }

    /// Returns the deployment commitment.
    #[must_use]
    pub const fn deployment_hash(self) -> Hash32 {
        self.deployment_hash
    }
}

impl CanonicalEncode for SecurityEvidence {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.kind.encode_to(output)?;
        output.extend_from_slice(self.claim_hash.as_bytes());
        output.extend_from_slice(self.artifact_hash.as_bytes());
        output.extend_from_slice(self.toolchain_hash.as_bytes());
        output.extend_from_slice(self.deployment_hash.as_bytes());
        Ok(())
    }
}

/// Empirical upper bound for one side/covert-channel rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapacityEvidence {
    key: ObservationKey,
    deployment_hash: Hash32,
    upper_bound_millibits_per_second: u64,
    confidence_ppm: u32,
    artifact_hash: Hash32,
    toolchain_hash: Hash32,
}

impl CapacityEvidence {
    /// Creates an empirical channel-capacity bound.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        observer: SecurityDomainId,
        kind: ObservationKind,
        channel: ChannelClass,
        deployment_hash: Hash32,
        upper_bound_millibits_per_second: u64,
        confidence_ppm: u32,
        artifact_hash: Hash32,
        toolchain_hash: Hash32,
    ) -> Result<Self, SecurityError> {
        if matches!(channel, ChannelClass::Intended) {
            return Err(SecurityError::CapacityForIntendedChannel);
        }
        if hash_is_zero(deployment_hash)
            || hash_is_zero(artifact_hash)
            || hash_is_zero(toolchain_hash)
        {
            return Err(SecurityError::ZeroHash);
        }
        if confidence_ppm == 0 || confidence_ppm > 1_000_000 {
            return Err(SecurityError::InvalidConfidence);
        }
        Ok(Self {
            key: ObservationKey {
                observer,
                kind,
                channel,
            },
            deployment_hash,
            upper_bound_millibits_per_second,
            confidence_ppm,
            artifact_hash,
            toolchain_hash,
        })
    }

    /// Returns the upper capacity bound.
    #[must_use]
    pub const fn upper_bound_millibits_per_second(self) -> u64 {
        self.upper_bound_millibits_per_second
    }

    /// Returns the confidence in parts per million.
    #[must_use]
    pub const fn confidence_ppm(self) -> u32 {
        self.confidence_ppm
    }

    fn key(self) -> ObservationKey {
        self.key
    }
}

impl CanonicalEncode for CapacityEvidence {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.key.encode_to(output)?;
        output.extend_from_slice(self.deployment_hash.as_bytes());
        output.extend_from_slice(&self.upper_bound_millibits_per_second.to_be_bytes());
        output.extend_from_slice(&self.confidence_ppm.to_be_bytes());
        output.extend_from_slice(self.artifact_hash.as_bytes());
        output.extend_from_slice(self.toolchain_hash.as_bytes());
        Ok(())
    }
}

/// Fail-closed production security policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityPromotionPolicy {
    deployment_hash: Hash32,
    threat_model_hash: Hash32,
    required_evidence: Box<[SecurityEvidenceKind]>,
    max_capacity_millibits_per_second: u64,
    min_capacity_confidence_ppm: u32,
}

impl SecurityPromotionPolicy {
    /// Creates a canonical promotion policy.
    pub fn try_new(
        deployment_hash: Hash32,
        threat_model_hash: Hash32,
        mut required_evidence: Vec<SecurityEvidenceKind>,
        max_capacity_millibits_per_second: u64,
        min_capacity_confidence_ppm: u32,
    ) -> Result<Self, SecurityError> {
        if deployment_hash == Hash32::ZERO || threat_model_hash == Hash32::ZERO {
            return Err(SecurityError::ZeroHash);
        }
        if required_evidence.is_empty() {
            return Err(SecurityError::EmptySecurityEvidencePolicy);
        }
        if required_evidence.len() > MAX_SECURITY_EVIDENCE {
            return Err(SecurityError::TooManySecurityEvidence);
        }
        if min_capacity_confidence_ppm == 0 || min_capacity_confidence_ppm > 1_000_000 {
            return Err(SecurityError::InvalidConfidence);
        }
        required_evidence.sort_unstable();
        required_evidence.dedup();
        Ok(Self {
            deployment_hash,
            threat_model_hash,
            required_evidence: required_evidence.into_boxed_slice(),
            max_capacity_millibits_per_second,
            min_capacity_confidence_ppm,
        })
    }

    /// Returns the exact deployment commitment.
    #[must_use]
    pub const fn deployment_hash(&self) -> Hash32 {
        self.deployment_hash
    }

    /// Returns the exact threat-model commitment.
    #[must_use]
    pub const fn threat_model_hash(&self) -> Hash32 {
        self.threat_model_hash
    }
}

impl CanonicalEncode for SecurityPromotionPolicy {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.deployment_hash.as_bytes());
        output.extend_from_slice(self.threat_model_hash.as_bytes());
        put_u16_length(output, self.required_evidence.len())?;
        for kind in &self.required_evidence {
            kind.encode_to(output)?;
        }
        output.extend_from_slice(&self.max_capacity_millibits_per_second.to_be_bytes());
        output.extend_from_slice(&self.min_capacity_confidence_ppm.to_be_bytes());
        Ok(())
    }
}

/// Production-security promotion blocker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecurityPromotionBlocker {
    /// A trace comparison found a modeled leak.
    LeakageReportFailed {
        /// Report index.
        index: u32,
    },
    /// A trace report belongs to another deployment.
    LeakageDeploymentMismatch {
        /// Report index.
        index: u32,
    },
    /// Leakage-policy deployment differs from the promotion policy.
    LeakagePolicyDeploymentMismatch,
    /// Leakage-policy threat model differs from the promotion policy.
    ThreatModelMismatch,
    /// A production decision supplied no leakage comparison.
    MissingLeakageReport,
    /// Leakage-report count exceeds the deterministic evaluation bound.
    TooManyLeakageReports {
        /// Supplied report count.
        observed: u32,
        /// Maximum evaluated report count.
        maximum: u32,
    },
    /// Security-evidence count exceeds the deterministic evaluation bound.
    TooManyEvidenceItems {
        /// Supplied evidence count.
        observed: u32,
        /// Maximum evaluated evidence count.
        maximum: u32,
    },
    /// Capacity-evidence count exceeds the deterministic evaluation bound.
    TooManyCapacityItems {
        /// Supplied capacity count.
        observed: u32,
        /// Maximum evaluated capacity count.
        maximum: u32,
    },
    /// Required evidence is absent.
    MissingEvidence(SecurityEvidenceKind),
    /// Two evidence items use the same required kind.
    DuplicateEvidence(SecurityEvidenceKind),
    /// Evidence is bound to another deployment.
    EvidenceDeploymentMismatch(SecurityEvidenceKind),
    /// A side/covert-channel rule has no capacity measurement.
    MissingCapacityEvidence {
        /// Observer.
        observer: SecurityDomainId,
        /// Observation kind.
        kind: ObservationKind,
        /// Channel class.
        channel: ChannelClass,
    },
    /// Duplicate capacity evidence exists for one rule.
    DuplicateCapacityEvidence,
    /// Capacity evidence is bound to another deployment.
    CapacityDeploymentMismatch,
    /// Measured capacity exceeds the policy.
    CapacityExceeded {
        /// Observed upper bound.
        observed: u64,
        /// Allowed upper bound.
        maximum: u64,
    },
    /// Measurement confidence is too low.
    CapacityConfidenceTooLow {
        /// Observed confidence.
        observed: u32,
        /// Required confidence.
        minimum: u32,
    },
}

impl CanonicalEncode for SecurityPromotionBlocker {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::LeakageReportFailed { index } => {
                output.push(0);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::LeakageDeploymentMismatch { index } => {
                output.push(1);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::MissingEvidence(kind) => {
                output.push(2);
                kind.encode_to(output)?;
            }
            Self::DuplicateEvidence(kind) => {
                output.push(3);
                kind.encode_to(output)?;
            }
            Self::EvidenceDeploymentMismatch(kind) => {
                output.push(4);
                kind.encode_to(output)?;
            }
            Self::MissingCapacityEvidence {
                observer,
                kind,
                channel,
            } => {
                output.push(5);
                observer.encode_to(output)?;
                kind.encode_to(output)?;
                channel.encode_to(output)?;
            }
            Self::DuplicateCapacityEvidence => output.push(6),
            Self::CapacityDeploymentMismatch => output.push(7),
            Self::CapacityExceeded { observed, maximum } => {
                output.push(8);
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&maximum.to_be_bytes());
            }
            Self::CapacityConfidenceTooLow { observed, minimum } => {
                output.push(9);
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&minimum.to_be_bytes());
            }
            Self::LeakagePolicyDeploymentMismatch => output.push(10),
            Self::ThreatModelMismatch => output.push(11),
            Self::MissingLeakageReport => output.push(12),
            Self::TooManyLeakageReports { observed, maximum } => {
                output.push(13);
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&maximum.to_be_bytes());
            }
            Self::TooManyEvidenceItems { observed, maximum } => {
                output.push(14);
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&maximum.to_be_bytes());
            }
            Self::TooManyCapacityItems { observed, maximum } => {
                output.push(15);
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&maximum.to_be_bytes());
            }
        }
        Ok(())
    }
}

/// Security-promotion evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityPromotionReport {
    blockers: Box<[SecurityPromotionBlocker]>,
}

impl SecurityPromotionReport {
    /// Returns true only when every security gate passed.
    #[must_use]
    pub fn is_promoted(&self) -> bool {
        self.blockers.is_empty()
    }

    /// Returns blockers in deterministic order.
    #[must_use]
    pub const fn blockers(&self) -> &[SecurityPromotionBlocker] {
        &self.blockers
    }
}

impl CanonicalEncode for SecurityPromotionReport {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_u32_length(output, self.blockers.len())?;
        for blocker in &self.blockers {
            put_blob(output, &blocker.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Evaluates deployment-bound side/covert-channel promotion evidence.
#[must_use]
pub fn evaluate_security_promotion(
    policy: &SecurityPromotionPolicy,
    leakage_policy: &LeakagePolicy,
    reports: &[LeakageReport],
    mut evidence: Vec<SecurityEvidence>,
    mut capacities: Vec<CapacityEvidence>,
) -> SecurityPromotionReport {
    let mut blockers = Vec::new();

    if leakage_policy.deployment_contract_hash != policy.deployment_hash {
        blockers.push(SecurityPromotionBlocker::LeakagePolicyDeploymentMismatch);
    }
    if leakage_policy.threat_model_hash != policy.threat_model_hash {
        blockers.push(SecurityPromotionBlocker::ThreatModelMismatch);
    }
    if reports.is_empty() {
        blockers.push(SecurityPromotionBlocker::MissingLeakageReport);
    }
    if reports.len() > MAX_LEAKAGE_REPORTS {
        blockers.push(SecurityPromotionBlocker::TooManyLeakageReports {
            observed: u32::try_from(reports.len()).unwrap_or(u32::MAX),
            maximum: u32::try_from(MAX_LEAKAGE_REPORTS).unwrap_or(u32::MAX),
        });
    }
    if evidence.len() > MAX_SECURITY_EVIDENCE {
        blockers.push(SecurityPromotionBlocker::TooManyEvidenceItems {
            observed: u32::try_from(evidence.len()).unwrap_or(u32::MAX),
            maximum: u32::try_from(MAX_SECURITY_EVIDENCE).unwrap_or(u32::MAX),
        });
        evidence.truncate(MAX_SECURITY_EVIDENCE);
    }
    if capacities.len() > MAX_CAPACITY_EVIDENCE {
        blockers.push(SecurityPromotionBlocker::TooManyCapacityItems {
            observed: u32::try_from(capacities.len()).unwrap_or(u32::MAX),
            maximum: u32::try_from(MAX_CAPACITY_EVIDENCE).unwrap_or(u32::MAX),
        });
        capacities.truncate(MAX_CAPACITY_EVIDENCE);
    }

    for (position, report) in reports.iter().take(MAX_LEAKAGE_REPORTS).enumerate() {
        let index = u32::try_from(position).unwrap_or(u32::MAX);
        if report.deployment_hash != policy.deployment_hash {
            blockers.push(SecurityPromotionBlocker::LeakageDeploymentMismatch { index });
        }
        if !report.is_secure() {
            blockers.push(SecurityPromotionBlocker::LeakageReportFailed { index });
        }
    }

    evidence.sort_by_key(|item| item.kind);
    for pair in evidence.windows(2) {
        if pair[0].kind == pair[1].kind {
            blockers.push(SecurityPromotionBlocker::DuplicateEvidence(pair[0].kind));
        }
    }
    for required in &policy.required_evidence {
        match evidence.binary_search_by_key(required, |item| item.kind) {
            Ok(index) => {
                if evidence[index].deployment_hash != policy.deployment_hash {
                    blockers.push(SecurityPromotionBlocker::EvidenceDeploymentMismatch(
                        *required,
                    ));
                }
            }
            Err(_) => blockers.push(SecurityPromotionBlocker::MissingEvidence(*required)),
        }
    }

    capacities.sort_by_key(|item| item.key());
    for pair in capacities.windows(2) {
        if pair[0].key() == pair[1].key() {
            blockers.push(SecurityPromotionBlocker::DuplicateCapacityEvidence);
        }
    }

    for rule in leakage_policy
        .rules
        .iter()
        .filter(|rule| !matches!(rule.key.channel, ChannelClass::Intended))
    {
        match capacities.binary_search_by_key(&rule.key(), |item| item.key()) {
            Ok(index) => {
                let capacity = capacities[index];
                if capacity.deployment_hash != policy.deployment_hash {
                    blockers.push(SecurityPromotionBlocker::CapacityDeploymentMismatch);
                }
                if capacity.upper_bound_millibits_per_second
                    > policy.max_capacity_millibits_per_second
                {
                    blockers.push(SecurityPromotionBlocker::CapacityExceeded {
                        observed: capacity.upper_bound_millibits_per_second,
                        maximum: policy.max_capacity_millibits_per_second,
                    });
                }
                if capacity.confidence_ppm < policy.min_capacity_confidence_ppm {
                    blockers.push(SecurityPromotionBlocker::CapacityConfidenceTooLow {
                        observed: capacity.confidence_ppm,
                        minimum: policy.min_capacity_confidence_ppm,
                    });
                }
            }
            Err(_) => blockers.push(SecurityPromotionBlocker::MissingCapacityEvidence {
                observer: rule.key.observer,
                kind: rule.key.kind,
                channel: rule.key.channel,
            }),
        }
    }

    SecurityPromotionReport {
        blockers: blockers.into_boxed_slice(),
    }
}

/// Security model construction or commitment failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecurityError {
    /// A stable numeric identifier was zero.
    ZeroIdentifier,
    /// A required commitment was zero.
    ZeroHash,
    /// A label exceeded the compartment bound.
    TooManyCompartments,
    /// A trace exceeded the observation bound.
    TooManyObservations,
    /// A policy exceeded the observer bound.
    TooManyObservers,
    /// A leakage policy contained no rules.
    EmptyLeakagePolicy,
    /// A policy exceeded the leakage-rule bound.
    TooManyLeakageRules,
    /// Two observer clearances share one identifier.
    DuplicateObserver,
    /// Two rules classify the same observer/channel tuple.
    DuplicateLeakageRule,
    /// A declassification bound was zero.
    ZeroDeclassificationBudget,
    /// A policy or deployment exceeded the mitigation bound.
    TooManyMitigations,
    /// Capacity evidence was supplied for an intended channel.
    CapacityForIntendedChannel,
    /// A confidence value was zero or above one million parts per million.
    InvalidConfidence,
    /// A promotion policy required no evidence.
    EmptySecurityEvidencePolicy,
    /// A promotion policy exceeded the evidence bound.
    TooManySecurityEvidence,
    /// Canonical encoding failed.
    Encode(EncodeError),
}

impl From<EncodeError> for SecurityError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl fmt::Display for SecurityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroIdentifier => formatter.write_str("security identifier is zero"),
            Self::ZeroHash => formatter.write_str("security commitment is zero"),
            Self::TooManyCompartments => formatter.write_str("too many security compartments"),
            Self::TooManyObservations => formatter.write_str("too many observations"),
            Self::TooManyObservers => formatter.write_str("too many observer clearances"),
            Self::EmptyLeakagePolicy => formatter.write_str("leakage policy has no rules"),
            Self::TooManyLeakageRules => formatter.write_str("too many leakage rules"),
            Self::DuplicateObserver => formatter.write_str("duplicate observer clearance"),
            Self::DuplicateLeakageRule => formatter.write_str("duplicate leakage rule"),
            Self::ZeroDeclassificationBudget => {
                formatter.write_str("declassification bit budget is zero")
            }
            Self::TooManyMitigations => formatter.write_str("too many mitigations"),
            Self::CapacityForIntendedChannel => {
                formatter.write_str("capacity evidence is only for side or covert channels")
            }
            Self::InvalidConfidence => {
                formatter.write_str("invalid confidence in parts per million")
            }
            Self::EmptySecurityEvidencePolicy => {
                formatter.write_str("security promotion policy requires no evidence")
            }
            Self::TooManySecurityEvidence => {
                formatter.write_str("too many security evidence kinds")
            }
            Self::Encode(error) => write!(formatter, "security encoding failed: {error}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SecurityError {}

fn hash_is_zero(hash: Hash32) -> bool {
    hash == Hash32::ZERO
}

fn hash_canonical<H: CommitmentHasher>(
    domain_name: &'static str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, SecurityError> {
    let bytes = value.canonical_bytes().map_err(SecurityError::Encode)?;
    let domain = Domain::new(domain_name, 1).map_err(SecurityError::Encode)?;
    commitment::<H>(domain, &bytes).map_err(SecurityError::Encode)
}

fn put_u16_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u16::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_u32_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    put_u32_length(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    struct TestHasher;

    impl CommitmentHasher for TestHasher {
        const ALGORITHM_ID: &'static str = "test-only/1";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().enumerate() {
                let slot = index % output.len();
                output[slot] = output[slot]
                    .wrapping_add(*byte)
                    .rotate_left((index % 8) as u32);
            }
            Hash32::new(output)
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn domain(value: u32) -> SecurityDomainId {
        SecurityDomainId::try_new(value).unwrap_or_else(|error| panic!("domain: {error}"))
    }

    fn compartment(value: u32) -> CompartmentId {
        CompartmentId::try_new(value).unwrap_or_else(|error| panic!("compartment: {error}"))
    }

    fn label(
        confidentiality: u16,
        integrity: u16,
        compartments: Vec<CompartmentId>,
    ) -> SecurityLabel {
        SecurityLabel::try_new(confidentiality, integrity, compartments)
            .unwrap_or_else(|error| panic!("label: {error}"))
    }

    fn deployment() -> DeploymentContract {
        DeploymentContract::try_new(
            hash(1),
            hash(2),
            hash(3),
            hash(4),
            hash(5),
            hash(6),
            vec![
                Mitigation::ConstantTimeControlFlow,
                Mitigation::SecretIndependentMemoryAccess,
                Mitigation::FixedOutputSize,
            ],
        )
        .unwrap_or_else(|error| panic!("deployment: {error}"))
    }

    fn deployment_hash() -> Hash32 {
        deployment()
            .commitment::<TestHasher>()
            .unwrap_or_else(|error| panic!("deployment hash: {error}"))
    }

    fn policy(mode: RuleMode, channel: ChannelClass) -> LeakagePolicy {
        LeakagePolicy::try_new(
            hash(10),
            deployment_hash(),
            64,
            vec![ObserverClearance::new(
                domain(1),
                label(10, 0, vec![compartment(1)]),
            )],
            vec![
                LeakageRule::try_new(domain(1), ObservationKind::OutputLength, channel, mode)
                    .unwrap_or_else(|error| panic!("rule: {error}")),
            ],
            vec![
                Mitigation::ConstantTimeControlFlow,
                Mitigation::SecretIndependentMemoryAccess,
                Mitigation::FixedOutputSize,
            ],
        )
        .unwrap_or_else(|error| panic!("policy: {error}"))
    }

    fn observation(
        value_byte: u8,
        quantity: u64,
        bits: u32,
        channel: ChannelClass,
        declassification: Option<Declassification>,
    ) -> Observation {
        Observation::try_new(
            domain(1),
            ObservationKind::OutputLength,
            channel,
            label(1, 5, vec![compartment(1)]),
            hash(value_byte),
            quantity,
            bits,
            declassification,
        )
        .unwrap_or_else(|error| panic!("observation: {error}"))
    }

    fn trace(secret_byte: u8, observation: Observation) -> ObservationTrace {
        ObservationTrace::try_new(hash(20), hash(secret_byte), vec![observation])
            .unwrap_or_else(|error| panic!("trace: {error}"))
    }

    #[test]
    fn information_flow_lattice_is_explicit() {
        let low = label(1, 10, vec![compartment(1)]);
        let high = label(2, 5, vec![compartment(1), compartment(2)]);
        assert!(low.can_flow_to(&high));
        assert!(!high.can_flow_to(&low));
    }

    #[test]
    fn policy_construction_is_history_independent() {
        let first = LeakageRule::try_new(
            domain(1),
            ObservationKind::OutputLength,
            ChannelClass::Side,
            RuleMode::Exact,
        )
        .unwrap_or_else(|error| panic!("first: {error}"));
        let second = LeakageRule::try_new(
            domain(1),
            ObservationKind::Termination,
            ChannelClass::Side,
            RuleMode::Exact,
        )
        .unwrap_or_else(|error| panic!("second: {error}"));
        let make = |rules| {
            LeakagePolicy::try_new(
                hash(10),
                deployment_hash(),
                0,
                vec![ObserverClearance::new(
                    domain(1),
                    label(10, 0, vec![compartment(1)]),
                )],
                rules,
                vec![Mitigation::FixedOutputSize, Mitigation::FixedWork],
            )
            .unwrap_or_else(|error| panic!("policy: {error}"))
        };
        let left = make(vec![first, second]);
        let right = make(vec![second, first]);
        assert_eq!(left, right);
        assert_eq!(left.canonical_bytes(), right.canonical_bytes());
    }

    #[test]
    fn exact_rule_detects_output_length_channel() {
        let policy = policy(RuleMode::Exact, ChannelClass::Side);
        let left = trace(21, observation(30, 64, 0, ChannelClass::Side, None));
        let right = trace(22, observation(30, 96, 0, ChannelClass::Side, None));
        let report = compare_traces::<TestHasher>(&policy, &deployment(), &left, &right);
        assert!(matches!(
            report.blockers(),
            [LeakageBlocker::QuantityMismatch { .. }]
        ));
    }

    #[test]
    fn bounded_declassification_is_explicit() {
        let release = Declassification::try_new(hash(40), hash(41), 8)
            .unwrap_or_else(|error| panic!("declassification: {error}"));
        let policy = policy(
            RuleMode::Declassified {
                authority_hash: hash(40),
                purpose_hash: hash(41),
                max_bits: 8,
            },
            ChannelClass::Intended,
        );
        let left = trace(
            21,
            observation(30, 1, 8, ChannelClass::Intended, Some(release)),
        );
        let right = trace(
            22,
            observation(31, 1, 8, ChannelClass::Intended, Some(release)),
        );
        let report = compare_traces::<TestHasher>(&policy, &deployment(), &left, &right);
        assert!(report.is_secure());
        assert_eq!(report.declassified_bits(), 8);
    }

    #[test]
    fn missing_deployment_mitigation_fails_closed() {
        let policy = policy(RuleMode::Exact, ChannelClass::Side);
        let weak = DeploymentContract::try_new(
            hash(1),
            hash(2),
            hash(3),
            hash(4),
            hash(5),
            hash(6),
            vec![Mitigation::FixedOutputSize],
        )
        .unwrap_or_else(|error| panic!("weak deployment: {error}"));
        let left = trace(21, observation(30, 64, 0, ChannelClass::Side, None));
        let right = trace(22, observation(30, 64, 0, ChannelClass::Side, None));
        let report = compare_traces::<TestHasher>(&policy, &weak, &left, &right);
        assert!(
            report
                .blockers()
                .iter()
                .any(|item| matches!(item, LeakageBlocker::MissingMitigation(_)))
        );
    }

    #[test]
    fn covert_capacity_is_a_promotion_gate() {
        let leakage_policy = policy(RuleMode::Exact, ChannelClass::Covert);
        let left = trace(21, observation(30, 64, 0, ChannelClass::Covert, None));
        let right = trace(22, observation(30, 64, 0, ChannelClass::Covert, None));
        let report = compare_traces::<TestHasher>(&leakage_policy, &deployment(), &left, &right);
        assert!(report.is_secure());

        let promotion = SecurityPromotionPolicy::try_new(
            deployment_hash(),
            hash(10),
            vec![
                SecurityEvidenceKind::NoninterferenceProof,
                SecurityEvidenceKind::CovertChannelCapacity,
            ],
            100,
            990_000,
        )
        .unwrap_or_else(|error| panic!("promotion: {error}"));
        let evidence = vec![
            SecurityEvidence::try_new(
                SecurityEvidenceKind::NoninterferenceProof,
                hash(50),
                hash(51),
                hash(52),
                deployment_hash(),
            )
            .unwrap_or_else(|error| panic!("proof: {error}")),
            SecurityEvidence::try_new(
                SecurityEvidenceKind::CovertChannelCapacity,
                hash(53),
                hash(54),
                hash(55),
                deployment_hash(),
            )
            .unwrap_or_else(|error| panic!("capacity evidence: {error}")),
        ];
        let capacity = CapacityEvidence::try_new(
            domain(1),
            ObservationKind::OutputLength,
            ChannelClass::Covert,
            deployment_hash(),
            101,
            999_000,
            hash(60),
            hash(61),
        )
        .unwrap_or_else(|error| panic!("capacity: {error}"));
        let result = evaluate_security_promotion(
            &promotion,
            &leakage_policy,
            &[report],
            evidence,
            vec![capacity],
        );
        assert!(matches!(
            result.blockers(),
            [SecurityPromotionBlocker::CapacityExceeded { .. }]
        ));
    }

    #[test]
    fn promotion_binds_threat_model_and_input_bounds() {
        let leakage_policy = policy(RuleMode::Exact, ChannelClass::Side);
        let promotion = SecurityPromotionPolicy::try_new(
            deployment_hash(),
            hash(99),
            vec![SecurityEvidenceKind::NoninterferenceProof],
            100,
            990_000,
        )
        .unwrap_or_else(|error| panic!("promotion: {error}"));
        let item = SecurityEvidence::try_new(
            SecurityEvidenceKind::NoninterferenceProof,
            hash(50),
            hash(51),
            hash(52),
            deployment_hash(),
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        let result = evaluate_security_promotion(
            &promotion,
            &leakage_policy,
            &[],
            vec![item; MAX_SECURITY_EVIDENCE + 1],
            Vec::new(),
        );
        assert!(
            result
                .blockers()
                .iter()
                .any(|item| matches!(item, SecurityPromotionBlocker::ThreatModelMismatch))
        );
        assert!(
            result
                .blockers()
                .iter()
                .any(|item| matches!(item, SecurityPromotionBlocker::MissingLeakageReport))
        );
        assert!(
            result
                .blockers()
                .iter()
                .any(|item| matches!(item, SecurityPromotionBlocker::TooManyEvidenceItems { .. }))
        );
    }

    #[test]
    fn promotion_succeeds_only_with_complete_deployment_evidence() {
        let leakage_policy = policy(RuleMode::Exact, ChannelClass::Side);
        let left = trace(21, observation(30, 64, 0, ChannelClass::Side, None));
        let right = trace(22, observation(30, 64, 0, ChannelClass::Side, None));
        let report = compare_traces::<TestHasher>(&leakage_policy, &deployment(), &left, &right);

        let promotion = SecurityPromotionPolicy::try_new(
            deployment_hash(),
            hash(10),
            vec![SecurityEvidenceKind::NoninterferenceProof],
            100,
            990_000,
        )
        .unwrap_or_else(|error| panic!("promotion: {error}"));
        let evidence = vec![
            SecurityEvidence::try_new(
                SecurityEvidenceKind::NoninterferenceProof,
                hash(50),
                hash(51),
                hash(52),
                deployment_hash(),
            )
            .unwrap_or_else(|error| panic!("proof: {error}")),
        ];
        let capacity = CapacityEvidence::try_new(
            domain(1),
            ObservationKind::OutputLength,
            ChannelClass::Side,
            deployment_hash(),
            0,
            999_000,
            hash(60),
            hash(61),
        )
        .unwrap_or_else(|error| panic!("capacity: {error}"));
        let result = evaluate_security_promotion(
            &promotion,
            &leakage_policy,
            &[report],
            evidence,
            vec![capacity],
        );
        assert!(result.is_promoted());
    }
}
