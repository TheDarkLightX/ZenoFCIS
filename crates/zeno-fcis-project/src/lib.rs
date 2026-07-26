//! Project-neutral protocol profiles, stable registries, and evolution checks.
//!
//! ZenoFCIS projects differ in domain semantics, but they share one authority
//! problem: stable identifiers, canonical bindings, and migration intent must be
//! explicit values rather than conventions hidden in source trees. This crate
//! provides that common layer without knowing anything about exchanges, mail,
//! storage, scientific ledgers, agent orchestration, or operating systems.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};

/// Canonical project-profile format version.
pub const PROJECT_PROFILE_FORMAT_VERSION: u16 = 1;
/// Maximum byte length of a project, subsystem, or registry name.
pub const MAX_STABLE_NAME_BYTES: usize = 64;
/// Maximum byte length of a domain prefix.
pub const MAX_DOMAIN_PREFIX_BYTES: usize = 160;
/// Maximum number of stable registry entries in one profile.
pub const MAX_REGISTRY_ENTRIES: usize = 65_536;

/// A normalized, lowercase ASCII identifier used for human-readable stable names.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StableName(Box<str>);

impl StableName {
    /// Creates a stable name from `[a-z0-9][a-z0-9._-]*`.
    pub fn try_new(value: impl Into<String>) -> Result<Self, ProfileError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_STABLE_NAME_BYTES {
            return Err(ProfileError::InvalidStableName);
        }
        let bytes = value.as_bytes();
        if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
            return Err(ProfileError::InvalidStableName);
        }
        if !bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        }) {
            return Err(ProfileError::InvalidStableName);
        }
        Ok(Self(value.into_boxed_str()))
    }

    /// Returns the normalized name.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        &self.0
    }
}

impl CanonicalEncode for StableName {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_u16_blob(output, self.0.as_bytes())
    }
}

/// A normalized domain namespace such as `zenostorage/agreement`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DomainPrefix(Box<str>);

impl DomainPrefix {
    /// Creates a lowercase ASCII domain prefix.
    pub fn try_new(value: impl Into<String>) -> Result<Self, ProfileError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_DOMAIN_PREFIX_BYTES {
            return Err(ProfileError::InvalidDomainPrefix);
        }
        let bytes = value.as_bytes();
        if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
            return Err(ProfileError::InvalidDomainPrefix);
        }
        if !bytes.iter().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'/' | b'.' | b'_' | b'-' | b':')
        }) {
            return Err(ProfileError::InvalidDomainPrefix);
        }
        Ok(Self(value.into_boxed_str()))
    }

    /// Returns the normalized prefix.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        &self.0
    }
}

impl CanonicalEncode for DomainPrefix {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_u16_blob(output, self.0.as_bytes())
    }
}

/// Nonzero stable identifier inside one registry kind.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SemanticId(u32);

impl SemanticId {
    /// Creates a nonzero identifier.
    pub const fn try_new(value: u32) -> Result<Self, ProfileError> {
        if value == 0 {
            Err(ProfileError::ZeroSemanticId)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the raw identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl CanonicalEncode for SemanticId {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

/// Closed registry namespaces shared by all projects.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum RegistryKind {
    /// Authoritative state type.
    StateType = 0,
    /// Command or event type admitted by a transition.
    CommandType = 1,
    /// Authenticated context or policy type.
    ContextType = 2,
    /// Stable rejection or committed-failure reason.
    Reason = 3,
    /// Authoritative effect operation.
    Effect = 4,
    /// External-delivery channel.
    Channel = 5,
    /// Evidence or proof artifact kind.
    Evidence = 6,
    /// Capability or authority kind.
    Capability = 7,
    /// Domain event kind.
    Event = 8,
    /// Declarative claim or invariant kind.
    Claim = 9,
    /// Explicit migration operation.
    Migration = 10,
}

impl CanonicalEncode for RegistryKind {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// One stable registry entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryEntry {
    kind: RegistryKind,
    id: SemanticId,
    name: StableName,
    definition_hash: Hash32,
}

impl RegistryEntry {
    /// Creates an entry bound to a nonzero semantic-definition commitment.
    pub fn try_new(
        kind: RegistryKind,
        id: SemanticId,
        name: StableName,
        definition_hash: Hash32,
    ) -> Result<Self, ProfileError> {
        if definition_hash == Hash32::ZERO {
            return Err(ProfileError::ZeroDefinitionHash);
        }
        Ok(Self {
            kind,
            id,
            name,
            definition_hash,
        })
    }

    /// Returns the registry namespace.
    #[must_use]
    pub const fn kind(&self) -> RegistryKind {
        self.kind
    }

    /// Returns the stable numeric identifier.
    #[must_use]
    pub const fn id(&self) -> SemanticId {
        self.id
    }

    /// Returns the stable readable name.
    #[must_use]
    pub const fn name(&self) -> &StableName {
        &self.name
    }

    /// Returns the exact semantic-definition commitment.
    #[must_use]
    pub const fn definition_hash(&self) -> Hash32 {
        self.definition_hash
    }

    fn key(&self) -> (RegistryKind, SemanticId) {
        (self.kind, self.id)
    }
}

impl CanonicalEncode for RegistryEntry {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.kind.encode_to(output)?;
        self.id.encode_to(output)?;
        self.name.encode_to(output)?;
        output.extend_from_slice(self.definition_hash.as_bytes());
        Ok(())
    }
}

/// Content commitments that bind the authority surface of one project profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileBindings {
    /// Closed state/command/context schema commitment.
    pub schema_hash: Hash32,
    /// Stable rejection and failure precedence commitment.
    pub precedence_hash: Hash32,
    /// Transition and arithmetic algorithm commitment.
    pub algorithm_hash: Hash32,
    /// Canonical codec and root-profile commitment.
    pub codec_hash: Hash32,
    /// Authoritative effect registry commitment.
    pub effect_registry_hash: Hash32,
    /// External-delivery channel registry commitment.
    pub channel_registry_hash: Hash32,
    /// Project policy and invariant commitment.
    pub policy_hash: Hash32,
}

impl ProfileBindings {
    fn validate(self) -> Result<Self, ProfileError> {
        if [
            self.schema_hash,
            self.precedence_hash,
            self.algorithm_hash,
            self.codec_hash,
            self.effect_registry_hash,
            self.channel_registry_hash,
            self.policy_hash,
        ]
        .contains(&Hash32::ZERO)
        {
            return Err(ProfileError::ZeroProfileBinding);
        }
        Ok(self)
    }
}

impl CanonicalEncode for ProfileBindings {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        for hash in [
            self.schema_hash,
            self.precedence_hash,
            self.algorithm_hash,
            self.codec_hash,
            self.effect_registry_hash,
            self.channel_registry_hash,
            self.policy_hash,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        Ok(())
    }
}

/// Project-neutral versioned protocol profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectProfile {
    project: StableName,
    subsystem: StableName,
    profile_id: SemanticId,
    version: u32,
    state_type: SemanticId,
    command_type: SemanticId,
    context_type: SemanticId,
    domain_prefix: DomainPrefix,
    bindings: ProfileBindings,
    entries: Box<[RegistryEntry]>,
}

impl ProjectProfile {
    /// Creates, validates, and canonicalizes one complete project profile.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        project: StableName,
        subsystem: StableName,
        profile_id: SemanticId,
        version: u32,
        state_type: SemanticId,
        command_type: SemanticId,
        context_type: SemanticId,
        domain_prefix: DomainPrefix,
        bindings: ProfileBindings,
        mut entries: Vec<RegistryEntry>,
    ) -> Result<Self, ProfileError> {
        if version == 0 {
            return Err(ProfileError::ZeroProfileVersion);
        }
        if entries.len() > MAX_REGISTRY_ENTRIES {
            return Err(ProfileError::TooManyRegistryEntries);
        }
        let bindings = bindings.validate()?;
        entries.sort_by_key(RegistryEntry::key);
        let mut names = BTreeSet::new();
        for pair in entries.windows(2) {
            if pair[0].key() == pair[1].key() {
                return Err(ProfileError::DuplicateRegistryId {
                    kind: pair[0].kind,
                    id: pair[0].id,
                });
            }
        }
        for entry in &entries {
            if !names.insert((entry.kind, entry.name.clone())) {
                return Err(ProfileError::DuplicateRegistryName {
                    kind: entry.kind,
                    name: entry.name.clone(),
                });
            }
        }
        require_root_entry(&entries, RegistryKind::StateType, state_type)?;
        require_root_entry(&entries, RegistryKind::CommandType, command_type)?;
        require_root_entry(&entries, RegistryKind::ContextType, context_type)?;
        Ok(Self {
            project,
            subsystem,
            profile_id,
            version,
            state_type,
            command_type,
            context_type,
            domain_prefix,
            bindings,
            entries: entries.into_boxed_slice(),
        })
    }

    /// Returns the project name.
    #[must_use]
    pub const fn project(&self) -> &StableName {
        &self.project
    }

    /// Returns the subsystem or bounded-context name.
    #[must_use]
    pub const fn subsystem(&self) -> &StableName {
        &self.subsystem
    }

    /// Returns the stable profile family identifier.
    #[must_use]
    pub const fn profile_id(&self) -> SemanticId {
        self.profile_id
    }

    /// Returns the monotonic profile version.
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    /// Returns the root state type.
    #[must_use]
    pub const fn state_type(&self) -> SemanticId {
        self.state_type
    }

    /// Returns the root command type.
    #[must_use]
    pub const fn command_type(&self) -> SemanticId {
        self.command_type
    }

    /// Returns the root authenticated context type.
    #[must_use]
    pub const fn context_type(&self) -> SemanticId {
        self.context_type
    }

    /// Returns the project domain prefix.
    #[must_use]
    pub const fn domain_prefix(&self) -> &DomainPrefix {
        &self.domain_prefix
    }

    /// Returns the bound authority commitments.
    #[must_use]
    pub const fn bindings(&self) -> ProfileBindings {
        self.bindings
    }

    /// Returns registry entries in canonical `(kind, id)` order.
    #[must_use]
    pub const fn entries(&self) -> &[RegistryEntry] {
        &self.entries
    }

    /// Looks up one stable registry entry.
    #[must_use]
    pub fn entry(&self, kind: RegistryKind, id: SemanticId) -> Option<&RegistryEntry> {
        self.entries
            .binary_search_by_key(&(kind, id), RegistryEntry::key)
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Computes the complete content-derived profile commitment.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, ProfileError> {
        let bytes = self.canonical_bytes().map_err(ProfileError::Encode)?;
        let domain = Domain::new("zeno-fcis/project-profile", PROJECT_PROFILE_FORMAT_VERSION)
            .map_err(ProfileError::Encode)?;
        commitment::<H>(domain, &bytes).map_err(ProfileError::Encode)
    }
}

impl CanonicalEncode for ProjectProfile {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-PROJECT\0");
        output.extend_from_slice(&PROJECT_PROFILE_FORMAT_VERSION.to_be_bytes());
        self.project.encode_to(output)?;
        self.subsystem.encode_to(output)?;
        self.profile_id.encode_to(output)?;
        output.extend_from_slice(&self.version.to_be_bytes());
        self.state_type.encode_to(output)?;
        self.command_type.encode_to(output)?;
        self.context_type.encode_to(output)?;
        self.domain_prefix.encode_to(output)?;
        self.bindings.encode_to(output)?;
        put_u32_length(output, self.entries.len())?;
        for entry in &self.entries {
            entry.encode_to(output)?;
        }
        Ok(())
    }
}

/// How a successor profile is intended to relate to its predecessor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvolutionMode {
    /// Existing identifiers and definitions remain unchanged; new entries may be added.
    Additive,
    /// A reviewed migration explicitly translates old values and semantics.
    Migrated {
        /// Nonzero migration-specification commitment.
        migration_hash: Hash32,
    },
}

/// One incompatibility discovered between profile revisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompatibilityBlocker {
    /// The successor names a different project.
    DifferentProject,
    /// The successor names a different subsystem.
    DifferentSubsystem,
    /// The stable profile family identifier changed.
    DifferentProfileId,
    /// The successor version is not strictly greater.
    NonIncreasingVersion,
    /// Additive evolution changed a root type.
    RootTypeChanged,
    /// Additive evolution changed an existing authority binding.
    BindingChanged(&'static str),
    /// An existing stable entry was removed.
    RemovedEntry {
        /// Registry namespace.
        kind: RegistryKind,
        /// Stable identifier.
        id: SemanticId,
    },
    /// An existing stable identifier was rebound to a new name or definition.
    ReboundEntry {
        /// Registry namespace.
        kind: RegistryKind,
        /// Stable identifier.
        id: SemanticId,
    },
    /// Migrated evolution omitted its migration commitment.
    MissingMigrationHash,
}

/// Deterministic compatibility evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityReport {
    blockers: Box<[CompatibilityBlocker]>,
}

impl CompatibilityReport {
    /// Returns whether no compatibility blocker was found.
    #[must_use]
    pub fn is_compatible(&self) -> bool {
        self.blockers.is_empty()
    }

    /// Returns blockers in deterministic evaluation order.
    #[must_use]
    pub const fn blockers(&self) -> &[CompatibilityBlocker] {
        &self.blockers
    }
}

/// Checks whether `next` is an admitted successor of `previous` under an explicit mode.
#[must_use]
pub fn compare_successor(
    previous: &ProjectProfile,
    next: &ProjectProfile,
    mode: EvolutionMode,
) -> CompatibilityReport {
    let mut blockers = Vec::new();
    if previous.project != next.project {
        blockers.push(CompatibilityBlocker::DifferentProject);
    }
    if previous.subsystem != next.subsystem {
        blockers.push(CompatibilityBlocker::DifferentSubsystem);
    }
    if previous.profile_id != next.profile_id {
        blockers.push(CompatibilityBlocker::DifferentProfileId);
    }
    if next.version <= previous.version {
        blockers.push(CompatibilityBlocker::NonIncreasingVersion);
    }

    match mode {
        EvolutionMode::Additive => {
            if previous.state_type != next.state_type
                || previous.command_type != next.command_type
                || previous.context_type != next.context_type
            {
                blockers.push(CompatibilityBlocker::RootTypeChanged);
            }
            compare_binding(
                previous.bindings.codec_hash,
                next.bindings.codec_hash,
                "codec",
                &mut blockers,
            );
            compare_binding(
                previous.bindings.precedence_hash,
                next.bindings.precedence_hash,
                "precedence",
                &mut blockers,
            );
            compare_binding(
                previous.bindings.algorithm_hash,
                next.bindings.algorithm_hash,
                "algorithm",
                &mut blockers,
            );
            compare_binding(
                previous.bindings.policy_hash,
                next.bindings.policy_hash,
                "policy",
                &mut blockers,
            );
            for entry in &previous.entries {
                match next.entry(entry.kind, entry.id) {
                    None => blockers.push(CompatibilityBlocker::RemovedEntry {
                        kind: entry.kind,
                        id: entry.id,
                    }),
                    Some(successor)
                        if successor.name != entry.name
                            || successor.definition_hash != entry.definition_hash =>
                    {
                        blockers.push(CompatibilityBlocker::ReboundEntry {
                            kind: entry.kind,
                            id: entry.id,
                        });
                    }
                    Some(_) => {}
                }
            }
        }
        EvolutionMode::Migrated { migration_hash } => {
            if migration_hash == Hash32::ZERO {
                blockers.push(CompatibilityBlocker::MissingMigrationHash);
            }
        }
    }

    CompatibilityReport {
        blockers: blockers.into_boxed_slice(),
    }
}

fn compare_binding(
    previous: Hash32,
    next: Hash32,
    label: &'static str,
    blockers: &mut Vec<CompatibilityBlocker>,
) {
    if previous != next {
        blockers.push(CompatibilityBlocker::BindingChanged(label));
    }
}

fn require_root_entry(
    entries: &[RegistryEntry],
    kind: RegistryKind,
    id: SemanticId,
) -> Result<(), ProfileError> {
    if entries
        .binary_search_by_key(&(kind, id), RegistryEntry::key)
        .is_ok()
    {
        Ok(())
    } else {
        Err(ProfileError::MissingRootEntry { kind, id })
    }
}

fn put_u16_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    let length = u16::try_from(bytes.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn put_u32_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

/// Project-profile construction failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileError {
    /// Stable name is empty, oversized, non-lowercase, or contains an invalid character.
    InvalidStableName,
    /// Domain prefix is empty, oversized, or contains an invalid character.
    InvalidDomainPrefix,
    /// Stable numeric identifiers are nonzero.
    ZeroSemanticId,
    /// Profile versions are nonzero.
    ZeroProfileVersion,
    /// Registry definitions must be content-bound.
    ZeroDefinitionHash,
    /// Every authority binding must be nonzero, including empty-registry commitments.
    ZeroProfileBinding,
    /// Registry entry count exceeds its deterministic bound.
    TooManyRegistryEntries,
    /// Two entries reuse one `(kind, id)` pair.
    DuplicateRegistryId {
        /// Registry namespace.
        kind: RegistryKind,
        /// Reused identifier.
        id: SemanticId,
    },
    /// Two entries in one namespace reuse one stable readable name.
    DuplicateRegistryName {
        /// Registry namespace.
        kind: RegistryKind,
        /// Reused stable name.
        name: StableName,
    },
    /// A declared root type has no matching registry entry.
    MissingRootEntry {
        /// Registry namespace.
        kind: RegistryKind,
        /// Missing identifier.
        id: SemanticId,
    },
    /// Canonical encoding or commitment failed.
    Encode(EncodeError),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStableName => formatter.write_str("invalid stable name"),
            Self::InvalidDomainPrefix => formatter.write_str("invalid domain prefix"),
            Self::ZeroSemanticId => formatter.write_str("semantic identifier must be nonzero"),
            Self::ZeroProfileVersion => formatter.write_str("profile version must be nonzero"),
            Self::ZeroDefinitionHash => formatter.write_str("registry definition hash is zero"),
            Self::ZeroProfileBinding => formatter.write_str("profile binding is zero"),
            Self::TooManyRegistryEntries => formatter.write_str("registry exceeds entry bound"),
            Self::DuplicateRegistryId { kind, id } => {
                write!(formatter, "duplicate {kind:?} identifier {}", id.get())
            }
            Self::DuplicateRegistryName { kind, name } => {
                write!(formatter, "duplicate {kind:?} name {}", name.as_str())
            }
            Self::MissingRootEntry { kind, id } => {
                write!(formatter, "missing {kind:?} root entry {}", id.get())
            }
            Self::Encode(error) => write!(formatter, "profile encoding failed: {error}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ProfileError {}

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

    fn name(value: &str) -> StableName {
        StableName::try_new(value).unwrap_or_else(|error| panic!("name: {error}"))
    }

    fn id(value: u32) -> SemanticId {
        SemanticId::try_new(value).unwrap_or_else(|error| panic!("id: {error}"))
    }

    fn entry(kind: RegistryKind, raw_id: u32, label: &str, byte: u8) -> RegistryEntry {
        RegistryEntry::try_new(kind, id(raw_id), name(label), hash(byte))
            .unwrap_or_else(|error| panic!("entry: {error}"))
    }

    fn bindings() -> ProfileBindings {
        ProfileBindings {
            schema_hash: hash(1),
            precedence_hash: hash(2),
            algorithm_hash: hash(3),
            codec_hash: hash(4),
            effect_registry_hash: hash(5),
            channel_registry_hash: hash(6),
            policy_hash: hash(7),
        }
    }

    fn profile(version: u32, mut extras: Vec<RegistryEntry>) -> ProjectProfile {
        let mut entries = vec![
            entry(RegistryKind::StateType, 1, "state", 11),
            entry(RegistryKind::CommandType, 2, "command", 12),
            entry(RegistryKind::ContextType, 3, "context", 13),
        ];
        entries.append(&mut extras);
        ProjectProfile::try_new(
            name("example"),
            name("core"),
            id(100),
            version,
            id(1),
            id(2),
            id(3),
            DomainPrefix::try_new("example/core").unwrap_or_else(|error| panic!("domain: {error}")),
            bindings(),
            entries,
        )
        .unwrap_or_else(|error| panic!("profile: {error}"))
    }

    #[test]
    fn names_are_normalized_and_fail_closed() {
        assert!(StableName::try_new("zenostorage").is_ok());
        assert_eq!(
            StableName::try_new("ZenoStorage"),
            Err(ProfileError::InvalidStableName)
        );
        assert_eq!(
            DomainPrefix::try_new("bad prefix"),
            Err(ProfileError::InvalidDomainPrefix)
        );
    }

    #[test]
    fn registry_declaration_order_does_not_change_bytes() {
        let left = profile(
            1,
            vec![
                entry(RegistryKind::Effect, 8, "write", 20),
                entry(RegistryKind::Reason, 4, "denied", 21),
            ],
        );
        let right = profile(
            1,
            vec![
                entry(RegistryKind::Reason, 4, "denied", 21),
                entry(RegistryKind::Effect, 8, "write", 20),
            ],
        );
        assert_eq!(left, right);
        assert_eq!(left.canonical_bytes(), right.canonical_bytes());
        assert_eq!(
            left.commitment::<TestHasher>(),
            right.commitment::<TestHasher>()
        );
    }

    #[test]
    fn duplicate_stable_identifier_and_name_are_rejected() {
        let duplicate_id = vec![
            entry(RegistryKind::StateType, 1, "state", 11),
            entry(RegistryKind::StateType, 1, "other-state", 12),
            entry(RegistryKind::CommandType, 2, "command", 13),
            entry(RegistryKind::ContextType, 3, "context", 14),
        ];
        let result = ProjectProfile::try_new(
            name("example"),
            name("core"),
            id(100),
            1,
            id(1),
            id(2),
            id(3),
            DomainPrefix::try_new("example/core").unwrap_or_else(|error| panic!("domain: {error}")),
            bindings(),
            duplicate_id,
        );
        assert!(matches!(
            result,
            Err(ProfileError::DuplicateRegistryId {
                kind: RegistryKind::StateType,
                ..
            })
        ));

        let duplicate_name = vec![
            entry(RegistryKind::StateType, 1, "state", 11),
            entry(RegistryKind::CommandType, 2, "command", 12),
            entry(RegistryKind::ContextType, 3, "context", 13),
            entry(RegistryKind::Reason, 4, "denied", 14),
            entry(RegistryKind::Reason, 7, "denied", 15),
        ];
        assert!(matches!(
            ProjectProfile::try_new(
                name("example"),
                name("core"),
                id(100),
                1,
                id(1),
                id(2),
                id(3),
                DomainPrefix::try_new("example/core")
                    .unwrap_or_else(|error| panic!("domain: {error}")),
                bindings(),
                duplicate_name,
            ),
            Err(ProfileError::DuplicateRegistryName {
                kind: RegistryKind::Reason,
                ..
            })
        ));
    }

    #[test]
    fn additive_successor_may_add_but_not_rebind_entries() {
        let previous = profile(1, vec![entry(RegistryKind::Reason, 10, "denied", 30)]);
        let additive = profile(
            2,
            vec![
                entry(RegistryKind::Reason, 10, "denied", 30),
                entry(RegistryKind::Reason, 11, "expired", 31),
            ],
        );
        assert!(compare_successor(&previous, &additive, EvolutionMode::Additive).is_compatible());

        let rebound = profile(2, vec![entry(RegistryKind::Reason, 10, "denied", 99)]);
        let report = compare_successor(&previous, &rebound, EvolutionMode::Additive);
        assert_eq!(
            report.blockers(),
            &[CompatibilityBlocker::ReboundEntry {
                kind: RegistryKind::Reason,
                id: id(10),
            }]
        );
    }

    #[test]
    fn migration_requires_an_explicit_nonzero_commitment() {
        let previous = profile(1, Vec::new());
        let next = profile(2, Vec::new());
        let report = compare_successor(
            &previous,
            &next,
            EvolutionMode::Migrated {
                migration_hash: Hash32::ZERO,
            },
        );
        assert_eq!(
            report.blockers(),
            &[CompatibilityBlocker::MissingMigrationHash]
        );
        assert!(
            compare_successor(
                &previous,
                &next,
                EvolutionMode::Migrated {
                    migration_hash: hash(42),
                },
            )
            .is_compatible()
        );
    }
}
