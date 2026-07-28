//! Proof-carrying assume-guarantee contracts and deterministic composition.
//!
//! Composition is accepted only when every local claim, assumption discharge,
//! frame permission, conflict law, and sequential/parallel parity statement is
//! bound to the exact canonical composition specification and independently
//! checked by an [`EvidenceVerifier`]. Effects and outbox obligations conflict
//! conservatively unless the specification declares an exact commutativity law
//! with accepted evidence.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};

/// Canonical format version for composition specifications and proof claims.
pub const COMPOSITION_FORMAT_VERSION: u16 = 2;
/// Maximum atoms in one hierarchical access path.
pub const MAX_PATH_ATOMS: usize = 64;
const MAX_PATHS_PER_SET: usize = 4_096;
const MAX_COMPONENTS: usize = 4_096;
const MAX_CLAIMS: usize = 16_384;
const MAX_PARALLEL_LAWS: usize = 16_384;

/// Stable identifier of one composed component.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ComponentId(u32);

impl ComponentId {
    /// Creates a component identifier.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the numeric identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl CanonicalEncode for ComponentId {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

/// One navigation atom in a semantic read, write, context, effect, or outbox path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PathAtom {
    /// Stable record field identifier.
    Field(u16),
    /// Stable tuple position.
    TupleIndex(u32),
    /// Stable vector position.
    VectorIndex(u32),
    /// Payload of the current closed-sum variant.
    SumPayload,
    /// Commitment of a canonical map key or destination value.
    MapKey(Hash32),
    /// Terminal wildcard matching any descendant of the current path.
    AnyDescendant,
}

impl CanonicalEncode for PathAtom {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Field(id) => {
                output.push(0);
                output.extend_from_slice(&id.to_be_bytes());
            }
            Self::TupleIndex(index) => {
                output.push(1);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::VectorIndex(index) => {
                output.push(2);
                output.extend_from_slice(&index.to_be_bytes());
            }
            Self::SumPayload => output.push(3),
            Self::MapKey(hash) => {
                output.push(4);
                output.extend_from_slice(hash.as_bytes());
            }
            Self::AnyDescendant => output.push(5),
        }
        Ok(())
    }
}

/// A hierarchical semantic path inside one declared namespace.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AccessPath {
    namespace: u32,
    atoms: Box<[PathAtom]>,
}

impl AccessPath {
    /// Creates a bounded path and requires a wildcard, when present, to be last.
    pub fn try_new(namespace: u32, atoms: Vec<PathAtom>) -> Result<Self, ContractError> {
        if namespace == 0 {
            return Err(ContractError::ZeroIdentifier);
        }
        if atoms.len() > MAX_PATH_ATOMS {
            return Err(ContractError::PathTooDeep);
        }
        if atoms.iter().enumerate().any(|(index, atom)| {
            matches!(atom, PathAtom::AnyDescendant) && index + 1 != atoms.len()
        }) {
            return Err(ContractError::NonTerminalWildcard);
        }
        Ok(Self {
            namespace,
            atoms: atoms.into_boxed_slice(),
        })
    }

    /// Returns the namespace identifier.
    #[must_use]
    pub const fn namespace(&self) -> u32 {
        self.namespace
    }

    /// Returns path atoms.
    #[must_use]
    pub const fn atoms(&self) -> &[PathAtom] {
        &self.atoms
    }

    /// Returns whether two paths may designate at least one common value.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        if self.namespace != other.namespace {
            return false;
        }
        let common = self.atoms.len().min(other.atoms.len());
        for index in 0..common {
            let left = &self.atoms[index];
            let right = &other.atoms[index];
            if matches!(left, PathAtom::AnyDescendant) || matches!(right, PathAtom::AnyDescendant) {
                return true;
            }
            if left != right {
                return false;
            }
        }
        true
    }

    /// Returns whether this declared path contains every value designated by `other`.
    #[must_use]
    pub fn covers(&self, other: &Self) -> bool {
        if self.namespace != other.namespace || self.atoms.len() > other.atoms.len() {
            return false;
        }
        for (declared, requested) in self.atoms.iter().zip(other.atoms.iter()) {
            if matches!(declared, PathAtom::AnyDescendant) {
                return true;
            }
            if declared != requested {
                return false;
            }
        }
        true
    }
}

impl CanonicalEncode for AccessPath {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.namespace.to_be_bytes());
        put_u16_length(output, self.atoms.len())?;
        for atom in &self.atoms {
            atom.encode_to(output)?;
        }
        Ok(())
    }
}

/// Canonically ordered, duplicate-free set of access paths.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PathSet {
    paths: Box<[AccessPath]>,
}

impl PathSet {
    /// Sorts paths and rejects duplicates or excessive cardinality.
    pub fn try_new(mut paths: Vec<AccessPath>) -> Result<Self, ContractError> {
        if paths.len() > MAX_PATHS_PER_SET {
            return Err(ContractError::PathSetTooLarge);
        }
        paths.sort();
        if paths.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ContractError::DuplicatePath);
        }
        Ok(Self {
            paths: paths.into_boxed_slice(),
        })
    }

    /// Returns the empty path set.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            paths: Vec::new().into_boxed_slice(),
        }
    }

    /// Returns canonical paths.
    #[must_use]
    pub const fn paths(&self) -> &[AccessPath] {
        &self.paths
    }

    /// Returns whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Returns whether any path in this set overlaps any path in another set.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.paths
            .iter()
            .any(|left| other.paths.iter().any(|right| left.overlaps(right)))
    }

    /// Returns whether a path is contained by a declared member.
    #[must_use]
    pub fn covers(&self, path: &AccessPath) -> bool {
        self.paths.iter().any(|declared| declared.covers(path))
    }
}

impl CanonicalEncode for PathSet {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_u32_length(output, self.paths.len())?;
        for path in &self.paths {
            put_blob(output, &path.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Complete state/context/effect footprint of one component or task.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Footprint {
    reads: PathSet,
    writes: PathSet,
    contexts: PathSet,
    effects: PathSet,
}

impl Footprint {
    /// Creates a footprint from exact path sets.
    #[must_use]
    pub const fn new(reads: PathSet, writes: PathSet, contexts: PathSet, effects: PathSet) -> Self {
        Self {
            reads,
            writes,
            contexts,
            effects,
        }
    }

    /// Returns state reads.
    #[must_use]
    pub const fn reads(&self) -> &PathSet {
        &self.reads
    }

    /// Returns state writes.
    #[must_use]
    pub const fn writes(&self) -> &PathSet {
        &self.writes
    }

    /// Returns explicit context reads.
    #[must_use]
    pub const fn contexts(&self) -> &PathSet {
        &self.contexts
    }

    /// Returns planned effect paths.
    #[must_use]
    pub const fn effects(&self) -> &PathSet {
        &self.effects
    }
}

impl CanonicalEncode for Footprint {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        for set in [&self.reads, &self.writes, &self.contexts, &self.effects] {
            put_blob(output, &set.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Direction or resource class of one deterministic-parallel conflict.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ConflictKind {
    /// Both components may write an overlapping value.
    WriteWrite = 0,
    /// The left component may write a value read by the right component.
    LeftWriteRightRead = 1,
    /// The right component may write a value read by the left component.
    RightWriteLeftRead = 2,
    /// Both components stage authoritative effects; conflict is conservative.
    EffectEffect = 3,
    /// Both components stage outbox obligations; conflict is conservative.
    OutboxOutbox = 4,
    /// The left component stages effects while the right stages outbox obligations.
    LeftEffectRightOutbox = 5,
    /// The right component stages effects while the left stages outbox obligations.
    RightEffectLeftOutbox = 6,
}

impl CanonicalEncode for ConflictKind {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// One detected semantic conflict.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Conflict {
    kind: ConflictKind,
}

impl Conflict {
    /// Returns the conflict direction or resource class.
    #[must_use]
    pub const fn kind(self) -> ConflictKind {
        self.kind
    }
}

impl CanonicalEncode for Conflict {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.kind.encode_to(output)
    }
}

/// Computes default state and effect conflicts between two footprints.
///
/// Effects conflict whenever both components stage any authoritative effect.
/// A verified [`ParallelConflictLaw`] is required to waive that conservative
/// default in a complete composition specification.
#[must_use]
pub fn conflicts(left: &Footprint, right: &Footprint) -> Vec<Conflict> {
    let mut output = Vec::new();
    if left.writes.overlaps(&right.writes) {
        output.push(Conflict {
            kind: ConflictKind::WriteWrite,
        });
    }
    if left.writes.overlaps(&right.reads) {
        output.push(Conflict {
            kind: ConflictKind::LeftWriteRightRead,
        });
    }
    if right.writes.overlaps(&left.reads) {
        output.push(Conflict {
            kind: ConflictKind::RightWriteLeftRead,
        });
    }
    if !left.effects.is_empty() && !right.effects.is_empty() {
        output.push(Conflict {
            kind: ConflictKind::EffectEffect,
        });
    }
    output
}

/// One environmental assumption made by a component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Assumption {
    claim: Hash32,
    depends_on: PathSet,
}

impl Assumption {
    /// Creates an assumption identified by a canonical claim commitment.
    #[must_use]
    pub const fn new(claim: Hash32, depends_on: PathSet) -> Self {
        Self { claim, depends_on }
    }

    /// Returns the claim commitment.
    #[must_use]
    pub const fn claim(&self) -> Hash32 {
        self.claim
    }

    /// Returns values on which the assumption depends.
    #[must_use]
    pub const fn depends_on(&self) -> &PathSet {
        &self.depends_on
    }
}

impl CanonicalEncode for Assumption {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.claim.as_bytes());
        put_blob(output, &self.depends_on.canonical_bytes()?)
    }
}

/// One guarantee established by a component under its assumptions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Guarantee {
    claim: Hash32,
    depends_on: PathSet,
}

impl Guarantee {
    /// Creates a guarantee identified by a canonical claim commitment.
    #[must_use]
    pub const fn new(claim: Hash32, depends_on: PathSet) -> Self {
        Self { claim, depends_on }
    }

    /// Returns the claim commitment.
    #[must_use]
    pub const fn claim(&self) -> Hash32 {
        self.claim
    }

    /// Returns values on which the guarantee depends.
    #[must_use]
    pub const fn depends_on(&self) -> &PathSet {
        &self.depends_on
    }
}

impl CanonicalEncode for Guarantee {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.claim.as_bytes());
        put_blob(output, &self.depends_on.canonical_bytes()?)
    }
}

/// Frame theorem protecting a path from unauthorized environment writes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameRule {
    protected: AccessPath,
    allowed_writers: Box<[ComponentId]>,
    proof_claim: Hash32,
}

impl FrameRule {
    /// Creates a rule and canonicalizes the allowed-writer set.
    pub fn try_new(
        protected: AccessPath,
        mut allowed_writers: Vec<ComponentId>,
        proof_claim: Hash32,
    ) -> Result<Self, ContractError> {
        if proof_claim == Hash32::ZERO {
            return Err(ContractError::ZeroHash);
        }
        allowed_writers.sort();
        if allowed_writers.iter().any(|component| component.get() == 0)
            || allowed_writers.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(ContractError::DuplicateComponent);
        }
        Ok(Self {
            protected,
            allowed_writers: allowed_writers.into_boxed_slice(),
            proof_claim,
        })
    }

    /// Returns the protected path.
    #[must_use]
    pub const fn protected(&self) -> &AccessPath {
        &self.protected
    }

    /// Returns permitted external writers.
    #[must_use]
    pub const fn allowed_writers(&self) -> &[ComponentId] {
        &self.allowed_writers
    }

    /// Returns the frame-proof claim commitment.
    #[must_use]
    pub const fn proof_claim(&self) -> Hash32 {
        self.proof_claim
    }
}

impl CanonicalEncode for FrameRule {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, &self.protected.canonical_bytes()?)?;
        put_u16_length(output, self.allowed_writers.len())?;
        for writer in &self.allowed_writers {
            writer.encode_to(output)?;
        }
        output.extend_from_slice(self.proof_claim.as_bytes());
        Ok(())
    }
}

/// Strict contract for one composable FCIS component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentContract {
    id: ComponentId,
    profile_hash: Hash32,
    footprint: Footprint,
    outbox: PathSet,
    assumptions: Box<[Assumption]>,
    guarantees: Box<[Guarantee]>,
    frames: Box<[FrameRule]>,
}

impl ComponentContract {
    /// Creates a contract with no declared outbox footprint.
    pub fn try_new(
        id: ComponentId,
        profile_hash: Hash32,
        footprint: Footprint,
        assumptions: Vec<Assumption>,
        guarantees: Vec<Guarantee>,
        frames: Vec<FrameRule>,
    ) -> Result<Self, ContractError> {
        Self::try_new_with_outbox(
            id,
            profile_hash,
            footprint,
            PathSet::empty(),
            assumptions,
            guarantees,
            frames,
        )
    }

    /// Creates a contract with an explicit channel/destination outbox footprint.
    pub fn try_new_with_outbox(
        id: ComponentId,
        profile_hash: Hash32,
        footprint: Footprint,
        outbox: PathSet,
        mut assumptions: Vec<Assumption>,
        mut guarantees: Vec<Guarantee>,
        mut frames: Vec<FrameRule>,
    ) -> Result<Self, ContractError> {
        if id.get() == 0 {
            return Err(ContractError::ZeroIdentifier);
        }
        if profile_hash == Hash32::ZERO {
            return Err(ContractError::ZeroHash);
        }
        if assumptions.len() + guarantees.len() + frames.len() > MAX_CLAIMS {
            return Err(ContractError::TooManyClaims);
        }
        if assumptions.iter().any(|item| item.claim == Hash32::ZERO)
            || guarantees.iter().any(|item| item.claim == Hash32::ZERO)
        {
            return Err(ContractError::ZeroHash);
        }
        assumptions.sort_by_key(Assumption::claim);
        guarantees.sort_by_key(Guarantee::claim);
        frames.sort_by(|left, right| left.protected.cmp(&right.protected));
        if assumptions
            .windows(2)
            .any(|pair| pair[0].claim == pair[1].claim)
            || guarantees
                .windows(2)
                .any(|pair| pair[0].claim == pair[1].claim)
            || frames
                .windows(2)
                .any(|pair| pair[0].protected == pair[1].protected)
        {
            return Err(ContractError::DuplicateClaim);
        }
        Ok(Self {
            id,
            profile_hash,
            footprint,
            outbox,
            assumptions: assumptions.into_boxed_slice(),
            guarantees: guarantees.into_boxed_slice(),
            frames: frames.into_boxed_slice(),
        })
    }

    /// Returns the component identifier.
    #[must_use]
    pub const fn id(&self) -> ComponentId {
        self.id
    }

    /// Returns the component profile commitment.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the complete state/context/effect footprint.
    #[must_use]
    pub const fn footprint(&self) -> &Footprint {
        &self.footprint
    }

    /// Returns the complete outbox channel/destination footprint.
    #[must_use]
    pub const fn outbox(&self) -> &PathSet {
        &self.outbox
    }

    /// Returns assumptions.
    #[must_use]
    pub const fn assumptions(&self) -> &[Assumption] {
        &self.assumptions
    }

    /// Returns guarantees.
    #[must_use]
    pub const fn guarantees(&self) -> &[Guarantee] {
        &self.guarantees
    }

    /// Returns frame rules.
    #[must_use]
    pub const fn frames(&self) -> &[FrameRule] {
        &self.frames
    }
}

impl CanonicalEncode for ComponentContract {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.id.encode_to(output)?;
        output.extend_from_slice(self.profile_hash.as_bytes());
        put_blob(output, &self.footprint.canonical_bytes()?)?;
        put_blob(output, &self.outbox.canonical_bytes()?)?;
        put_u32_length(output, self.assumptions.len())?;
        for assumption in &self.assumptions {
            put_blob(output, &assumption.canonical_bytes()?)?;
        }
        put_u32_length(output, self.guarantees.len())?;
        for guarantee in &self.guarantees {
            put_blob(output, &guarantee.canonical_bytes()?)?;
        }
        put_u32_length(output, self.frames.len())?;
        for frame in &self.frames {
            put_blob(output, &frame.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Typed wiring from one effect output to another component boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Wiring {
    source_component: ComponentId,
    source_effect: AccessPath,
    destination_component: ComponentId,
    destination_path: AccessPath,
    schema_hash: Hash32,
}

impl Wiring {
    /// Creates a wiring value.
    #[must_use]
    pub const fn new(
        source_component: ComponentId,
        source_effect: AccessPath,
        destination_component: ComponentId,
        destination_path: AccessPath,
        schema_hash: Hash32,
    ) -> Self {
        Self {
            source_component,
            source_effect,
            destination_component,
            destination_path,
            schema_hash,
        }
    }

    /// Returns the source component.
    #[must_use]
    pub const fn source_component(&self) -> ComponentId {
        self.source_component
    }

    /// Returns the source effect path.
    #[must_use]
    pub const fn source_effect(&self) -> &AccessPath {
        &self.source_effect
    }

    /// Returns the destination component.
    #[must_use]
    pub const fn destination_component(&self) -> ComponentId {
        self.destination_component
    }

    /// Returns the destination path.
    #[must_use]
    pub const fn destination_path(&self) -> &AccessPath {
        &self.destination_path
    }

    /// Returns the shared semantic schema commitment.
    #[must_use]
    pub const fn schema_hash(&self) -> Hash32 {
        self.schema_hash
    }
}

impl CanonicalEncode for Wiring {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.source_component.encode_to(output)?;
        put_blob(output, &self.source_effect.canonical_bytes()?)?;
        self.destination_component.encode_to(output)?;
        put_blob(output, &self.destination_path.canonical_bytes()?)?;
        output.extend_from_slice(self.schema_hash.as_bytes());
        Ok(())
    }
}

/// Exact provider component and guarantee used to discharge an assumption.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProviderGuarantee {
    component: ComponentId,
    guarantee: Hash32,
}

impl ProviderGuarantee {
    /// Creates an exact provider binding.
    pub fn try_new(component: ComponentId, guarantee: Hash32) -> Result<Self, ContractError> {
        if component.get() == 0 {
            return Err(ContractError::ZeroIdentifier);
        }
        if guarantee == Hash32::ZERO {
            return Err(ContractError::ZeroHash);
        }
        Ok(Self {
            component,
            guarantee,
        })
    }

    /// Returns the provider component.
    #[must_use]
    pub const fn component(self) -> ComponentId {
        self.component
    }

    /// Returns the provider guarantee claim.
    #[must_use]
    pub const fn guarantee(self) -> Hash32 {
        self.guarantee
    }
}

impl CanonicalEncode for ProviderGuarantee {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.component.encode_to(output)?;
        output.extend_from_slice(self.guarantee.as_bytes());
        Ok(())
    }
}

/// Reviewed law allowing one exact parallel conflict to commute.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ParallelConflictLaw {
    left: ComponentId,
    right: ComponentId,
    kind: ConflictKind,
    claim: Hash32,
}

impl ParallelConflictLaw {
    /// Creates a canonical component pair and nonzero law claim.
    pub fn try_new(
        first: ComponentId,
        second: ComponentId,
        kind: ConflictKind,
        claim: Hash32,
    ) -> Result<Self, ContractError> {
        if first.get() == 0 || second.get() == 0 || first == second {
            return Err(ContractError::InvalidParallelLaw);
        }
        if claim == Hash32::ZERO {
            return Err(ContractError::ZeroHash);
        }
        let (left, right) = if first < second {
            (first, second)
        } else {
            (second, first)
        };
        Ok(Self {
            left,
            right,
            kind,
            claim,
        })
    }

    /// Returns the lower canonical component identifier.
    #[must_use]
    pub const fn left(self) -> ComponentId {
        self.left
    }

    /// Returns the higher canonical component identifier.
    #[must_use]
    pub const fn right(self) -> ComponentId {
        self.right
    }

    /// Returns the waived conflict kind.
    #[must_use]
    pub const fn kind(self) -> ConflictKind {
        self.kind
    }

    /// Returns the exact commutativity-law claim.
    #[must_use]
    pub const fn claim(self) -> Hash32 {
        self.claim
    }
}

impl CanonicalEncode for ParallelConflictLaw {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.left.encode_to(output)?;
        self.right.encode_to(output)?;
        self.kind.encode_to(output)?;
        output.extend_from_slice(self.claim.as_bytes());
        Ok(())
    }
}

/// Canonical composition specification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionSpec {
    version: u16,
    components: Box<[ComponentContract]>,
    wirings: Box<[Wiring]>,
    coupling_claims: Box<[Hash32]>,
    merge_order: Box<[ComponentId]>,
    parallel_laws: Box<[ParallelConflictLaw]>,
}

impl CompositionSpec {
    /// Creates a bounded specification with no conflict-waiver laws.
    pub fn try_new(
        version: u16,
        components: Vec<ComponentContract>,
        wirings: Vec<Wiring>,
        coupling_claims: Vec<Hash32>,
        merge_order: Vec<ComponentId>,
    ) -> Result<Self, ContractError> {
        Self::try_new_with_parallel_laws(
            version,
            components,
            wirings,
            coupling_claims,
            merge_order,
            Vec::new(),
        )
    }

    /// Creates a bounded specification and validates all component and law references.
    pub fn try_new_with_parallel_laws(
        version: u16,
        mut components: Vec<ComponentContract>,
        mut wirings: Vec<Wiring>,
        mut coupling_claims: Vec<Hash32>,
        merge_order: Vec<ComponentId>,
        mut parallel_laws: Vec<ParallelConflictLaw>,
    ) -> Result<Self, ContractError> {
        if version == 0 {
            return Err(ContractError::ZeroIdentifier);
        }
        if components.is_empty() || components.len() > MAX_COMPONENTS {
            return Err(ContractError::ComponentCardinality);
        }
        components.sort_by_key(ComponentContract::id);
        if components.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(ContractError::DuplicateComponent);
        }
        let canonical_ids: Vec<ComponentId> =
            components.iter().map(ComponentContract::id).collect();
        let mut sorted_merge = merge_order.clone();
        sorted_merge.sort();
        if sorted_merge != canonical_ids {
            return Err(ContractError::InvalidMergeOrder);
        }
        wirings.sort_by(|left, right| {
            (
                left.source_component,
                &left.source_effect,
                left.destination_component,
                &left.destination_path,
            )
                .cmp(&(
                    right.source_component,
                    &right.source_effect,
                    right.destination_component,
                    &right.destination_path,
                ))
        });
        if wirings.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ContractError::DuplicateWiring);
        }
        for wiring in &wirings {
            if wiring.schema_hash == Hash32::ZERO
                || components
                    .binary_search_by_key(&wiring.source_component, ComponentContract::id)
                    .is_err()
                || components
                    .binary_search_by_key(&wiring.destination_component, ComponentContract::id)
                    .is_err()
            {
                return Err(ContractError::UnknownComponent);
            }
        }
        if coupling_claims.contains(&Hash32::ZERO) {
            return Err(ContractError::ZeroHash);
        }
        coupling_claims.sort();
        if coupling_claims.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ContractError::DuplicateClaim);
        }
        if parallel_laws.len() > MAX_PARALLEL_LAWS {
            return Err(ContractError::TooManyParallelLaws);
        }
        parallel_laws.sort();
        if parallel_laws.windows(2).any(|pair| {
            (pair[0].left, pair[0].right, pair[0].kind)
                == (pair[1].left, pair[1].right, pair[1].kind)
        }) {
            return Err(ContractError::DuplicateParallelLaw);
        }
        for law in &parallel_laws {
            if components
                .binary_search_by_key(&law.left, ComponentContract::id)
                .is_err()
                || components
                    .binary_search_by_key(&law.right, ComponentContract::id)
                    .is_err()
            {
                return Err(ContractError::InvalidParallelLaw);
            }
        }
        Ok(Self {
            version,
            components: components.into_boxed_slice(),
            wirings: wirings.into_boxed_slice(),
            coupling_claims: coupling_claims.into_boxed_slice(),
            merge_order: merge_order.into_boxed_slice(),
            parallel_laws: parallel_laws.into_boxed_slice(),
        })
    }

    /// Returns the composition version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Returns canonical component contracts.
    #[must_use]
    pub const fn components(&self) -> &[ComponentContract] {
        &self.components
    }

    /// Returns canonical wiring.
    #[must_use]
    pub const fn wirings(&self) -> &[Wiring] {
        &self.wirings
    }

    /// Returns global coupling claims.
    #[must_use]
    pub const fn coupling_claims(&self) -> &[Hash32] {
        &self.coupling_claims
    }

    /// Returns the protocol-visible deterministic merge order.
    #[must_use]
    pub const fn merge_order(&self) -> &[ComponentId] {
        &self.merge_order
    }

    /// Returns exact conflict-waiver laws.
    #[must_use]
    pub const fn parallel_laws(&self) -> &[ParallelConflictLaw] {
        &self.parallel_laws
    }

    /// Computes the canonical specification commitment.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, ContractError> {
        hash_canonical::<H>("zeno-fcis/composition-spec", self)
    }

    fn component(&self, id: ComponentId) -> Option<&ComponentContract> {
        self.components
            .binary_search_by_key(&id, ComponentContract::id)
            .ok()
            .map(|index| &self.components[index])
    }

    fn parallel_law(
        &self,
        first: ComponentId,
        second: ComponentId,
        kind: ConflictKind,
    ) -> Option<ParallelConflictLaw> {
        let (left, right) = if first < second {
            (first, second)
        } else {
            (second, first)
        };
        self.parallel_laws
            .binary_search_by_key(&(left, right, kind), |law| (law.left, law.right, law.kind))
            .ok()
            .map(|index| self.parallel_laws[index])
    }
}

impl CanonicalEncode for CompositionSpec {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-COMPOSITION-SPEC\0");
        output.extend_from_slice(&COMPOSITION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(&self.version.to_be_bytes());
        put_u32_length(output, self.components.len())?;
        for component in &self.components {
            put_blob(output, &component.canonical_bytes()?)?;
        }
        put_u32_length(output, self.wirings.len())?;
        for wiring in &self.wirings {
            put_blob(output, &wiring.canonical_bytes()?)?;
        }
        put_u32_length(output, self.coupling_claims.len())?;
        for claim in &self.coupling_claims {
            output.extend_from_slice(claim.as_bytes());
        }
        put_u32_length(output, self.merge_order.len())?;
        for component in &self.merge_order {
            component.encode_to(output)?;
        }
        put_u32_length(output, self.parallel_laws.len())?;
        for law in &self.parallel_laws {
            put_blob(output, &law.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Evidence binding one local claim to one external proof artifact.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClaimEvidence {
    claim: Hash32,
    artifact: Hash32,
}

impl ClaimEvidence {
    /// Creates an evidence binding.
    #[must_use]
    pub const fn new(claim: Hash32, artifact: Hash32) -> Self {
        Self { claim, artifact }
    }

    /// Returns the claim commitment.
    #[must_use]
    pub const fn claim(self) -> Hash32 {
        self.claim
    }

    /// Returns the artifact commitment.
    #[must_use]
    pub const fn artifact(self) -> Hash32 {
        self.artifact
    }
}

impl CanonicalEncode for ClaimEvidence {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.claim.as_bytes());
        output.extend_from_slice(self.artifact.as_bytes());
        Ok(())
    }
}

/// Evidence that exact peer guarantees establish one assumption.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssumptionDischarge {
    component: ComponentId,
    assumption: Hash32,
    providers: Box<[ProviderGuarantee]>,
    artifact: Hash32,
}

impl AssumptionDischarge {
    /// Creates an exact assumption discharge and canonicalizes provider bindings.
    pub fn try_new(
        component: ComponentId,
        assumption: Hash32,
        mut providers: Vec<ProviderGuarantee>,
        artifact: Hash32,
    ) -> Result<Self, ContractError> {
        if component.get() == 0 {
            return Err(ContractError::ZeroIdentifier);
        }
        if assumption == Hash32::ZERO || artifact == Hash32::ZERO {
            return Err(ContractError::ZeroHash);
        }
        providers.sort();
        if providers.is_empty() || providers.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ContractError::InvalidAssumptionDischarge);
        }
        Ok(Self {
            component,
            assumption,
            providers: providers.into_boxed_slice(),
            artifact,
        })
    }

    /// Returns the consumer component.
    #[must_use]
    pub const fn component(&self) -> ComponentId {
        self.component
    }

    /// Returns the discharged assumption.
    #[must_use]
    pub const fn assumption(&self) -> Hash32 {
        self.assumption
    }

    /// Returns exact provider-component and guarantee bindings.
    #[must_use]
    pub const fn providers(&self) -> &[ProviderGuarantee] {
        &self.providers
    }

    /// Returns the assumption-closure artifact.
    #[must_use]
    pub const fn artifact(&self) -> Hash32 {
        self.artifact
    }
}

impl CanonicalEncode for AssumptionDischarge {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.component.encode_to(output)?;
        output.extend_from_slice(self.assumption.as_bytes());
        put_u32_length(output, self.providers.len())?;
        for provider in &self.providers {
            provider.encode_to(output)?;
        }
        output.extend_from_slice(self.artifact.as_bytes());
        Ok(())
    }
}

/// Exact expected context for one sequential-versus-parallel theorem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParallelVerificationContext {
    composition_spec_hash: Hash32,
    source_revision_hash: Hash32,
    input_domain_hash: Hash32,
    coverage_hash: Hash32,
    partition_plan_hash: Hash32,
    algorithm_hash: Hash32,
    toolchain_hash: Hash32,
    merge_order: Box<[ComponentId]>,
}

impl ParallelVerificationContext {
    /// Creates a complete nonzero verification context.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        composition_spec_hash: Hash32,
        source_revision_hash: Hash32,
        input_domain_hash: Hash32,
        coverage_hash: Hash32,
        partition_plan_hash: Hash32,
        algorithm_hash: Hash32,
        toolchain_hash: Hash32,
        merge_order: Vec<ComponentId>,
    ) -> Result<Self, ContractError> {
        if [
            composition_spec_hash,
            source_revision_hash,
            input_domain_hash,
            coverage_hash,
            partition_plan_hash,
            algorithm_hash,
            toolchain_hash,
        ]
        .contains(&Hash32::ZERO)
        {
            return Err(ContractError::ZeroHash);
        }
        if merge_order.is_empty()
            || merge_order.iter().any(|component| component.get() == 0)
            || has_duplicate_components(&merge_order)
        {
            return Err(ContractError::InvalidMergeOrder);
        }
        Ok(Self {
            composition_spec_hash,
            source_revision_hash,
            input_domain_hash,
            coverage_hash,
            partition_plan_hash,
            algorithm_hash,
            toolchain_hash,
            merge_order: merge_order.into_boxed_slice(),
        })
    }

    /// Returns the bound composition specification.
    #[must_use]
    pub const fn composition_spec_hash(&self) -> Hash32 {
        self.composition_spec_hash
    }

    /// Returns the exact merge order.
    #[must_use]
    pub const fn merge_order(&self) -> &[ComponentId] {
        &self.merge_order
    }
}

impl CanonicalEncode for ParallelVerificationContext {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        for hash in [
            self.composition_spec_hash,
            self.source_revision_hash,
            self.input_domain_hash,
            self.coverage_hash,
            self.partition_plan_hash,
            self.algorithm_hash,
            self.toolchain_hash,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        put_u32_length(output, self.merge_order.len())?;
        for component in &self.merge_order {
            component.encode_to(output)?;
        }
        Ok(())
    }
}

/// Evidence for one complete sequential-versus-parallel equivalence claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParallelParityEvidence {
    context: ParallelVerificationContext,
    sequential_result: Hash32,
    composed_result: Hash32,
    artifact: Hash32,
}

impl ParallelParityEvidence {
    /// Creates a parity claim with exact result and artifact commitments.
    pub fn try_new(
        context: ParallelVerificationContext,
        sequential_result: Hash32,
        composed_result: Hash32,
        artifact: Hash32,
    ) -> Result<Self, ContractError> {
        if sequential_result == Hash32::ZERO
            || composed_result == Hash32::ZERO
            || artifact == Hash32::ZERO
        {
            return Err(ContractError::ZeroHash);
        }
        Ok(Self {
            context,
            sequential_result,
            composed_result,
            artifact,
        })
    }

    /// Returns the exact verification context.
    #[must_use]
    pub const fn context(&self) -> &ParallelVerificationContext {
        &self.context
    }

    /// Returns the normative sequential result commitment.
    #[must_use]
    pub const fn sequential_result(&self) -> Hash32 {
        self.sequential_result
    }

    /// Returns the composed result commitment.
    #[must_use]
    pub const fn composed_result(&self) -> Hash32 {
        self.composed_result
    }

    /// Returns the retained proof or replay artifact commitment.
    #[must_use]
    pub const fn artifact(&self) -> Hash32 {
        self.artifact
    }
}

impl CanonicalEncode for ParallelParityEvidence {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, &self.context.canonical_bytes()?)?;
        output.extend_from_slice(self.sequential_result.as_bytes());
        output.extend_from_slice(self.composed_result.as_bytes());
        output.extend_from_slice(self.artifact.as_bytes());
        Ok(())
    }
}

/// Complete bounded evidence index for a composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionEvidence {
    claim_evidence: Box<[ClaimEvidence]>,
    assumption_discharges: Box<[AssumptionDischarge]>,
    parity: Option<ParallelParityEvidence>,
}

impl CompositionEvidence {
    /// Creates an evidence index and rejects duplicate claim/discharge keys.
    pub fn try_new(
        mut claim_evidence: Vec<ClaimEvidence>,
        mut assumption_discharges: Vec<AssumptionDischarge>,
        parity: Option<ParallelParityEvidence>,
    ) -> Result<Self, ContractError> {
        if claim_evidence.len() + assumption_discharges.len() > MAX_CLAIMS {
            return Err(ContractError::TooManyClaims);
        }
        if claim_evidence
            .iter()
            .any(|item| item.claim == Hash32::ZERO || item.artifact == Hash32::ZERO)
        {
            return Err(ContractError::ZeroHash);
        }
        claim_evidence.sort();
        if claim_evidence
            .windows(2)
            .any(|pair| pair[0].claim == pair[1].claim)
        {
            return Err(ContractError::DuplicateEvidence);
        }
        assumption_discharges.sort_by_key(|item| (item.component, item.assumption));
        if assumption_discharges.windows(2).any(|pair| {
            pair[0].component == pair[1].component && pair[0].assumption == pair[1].assumption
        }) {
            return Err(ContractError::DuplicateEvidence);
        }
        Ok(Self {
            claim_evidence: claim_evidence.into_boxed_slice(),
            assumption_discharges: assumption_discharges.into_boxed_slice(),
            parity,
        })
    }

    /// Returns claim evidence.
    #[must_use]
    pub const fn claim_evidence(&self) -> &[ClaimEvidence] {
        &self.claim_evidence
    }

    /// Returns assumption discharges.
    #[must_use]
    pub const fn assumption_discharges(&self) -> &[AssumptionDischarge] {
        &self.assumption_discharges
    }

    /// Returns sequential-parallel parity evidence, when supplied.
    #[must_use]
    pub const fn parity(&self) -> Option<&ParallelParityEvidence> {
        self.parity.as_ref()
    }

    fn claim(&self, claim: Hash32) -> Option<ClaimEvidence> {
        self.claim_evidence
            .binary_search_by_key(&claim, |item| item.claim)
            .ok()
            .map(|index| self.claim_evidence[index])
    }

    fn discharge(
        &self,
        component: ComponentId,
        assumption: Hash32,
    ) -> Option<&AssumptionDischarge> {
        self.assumption_discharges
            .binary_search_by_key(&(component, assumption), |item| {
                (item.component, item.assumption)
            })
            .ok()
            .map(|index| &self.assumption_discharges[index])
    }
}

impl CanonicalEncode for CompositionEvidence {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-COMPOSITION-EVIDENCE\0");
        output.extend_from_slice(&COMPOSITION_FORMAT_VERSION.to_be_bytes());
        put_u32_length(output, self.claim_evidence.len())?;
        for item in &self.claim_evidence {
            item.encode_to(output)?;
        }
        put_u32_length(output, self.assumption_discharges.len())?;
        for item in &self.assumption_discharges {
            put_blob(output, &item.canonical_bytes()?)?;
        }
        match &self.parity {
            None => output.push(0),
            Some(parity) => {
                output.push(1);
                put_blob(output, &parity.canonical_bytes()?)?;
            }
        }
        Ok(())
    }
}

/// Exact statement presented to an independent evidence verifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompositionClaim {
    /// One component guarantee under the exact composition specification.
    Guarantee {
        /// Exact composition specification commitment.
        spec_hash: Hash32,
        /// Component declaring the guarantee.
        component: ComponentId,
        /// Guarantee claim.
        claim: Hash32,
    },
    /// One component frame theorem under the exact composition specification.
    Frame {
        /// Exact composition specification commitment.
        spec_hash: Hash32,
        /// Component owning the frame.
        component: ComponentId,
        /// Protected path.
        protected: AccessPath,
        /// Frame theorem claim.
        claim: Hash32,
    },
    /// One assumption discharged by the exact provider set and specification.
    AssumptionDischarge {
        /// Exact composition specification commitment.
        spec_hash: Hash32,
        /// Consumer component.
        component: ComponentId,
        /// Assumption claim.
        assumption: Hash32,
        /// Exact sorted providers.
        providers: Box<[ProviderGuarantee]>,
    },
    /// One global coupling theorem.
    Coupling {
        /// Exact composition specification commitment.
        spec_hash: Hash32,
        /// Coupling theorem claim.
        claim: Hash32,
    },
    /// One exact conflict commutativity law.
    ConflictLaw {
        /// Exact composition specification commitment.
        spec_hash: Hash32,
        /// Reviewed law.
        law: ParallelConflictLaw,
    },
    /// Complete sequential-versus-parallel equivalence statement.
    ParallelParity {
        /// Exact expected verification context.
        context: Box<ParallelVerificationContext>,
        /// Normative sequential result.
        sequential_result: Hash32,
        /// Composed result.
        composed_result: Hash32,
    },
}

impl CompositionClaim {
    /// Computes the canonical claim commitment for evidence indexing or tests.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, ContractError> {
        hash_canonical::<H>("zeno-fcis/composition-claim", self)
    }
}

impl CanonicalEncode for CompositionClaim {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-COMPOSITION-CLAIM\0");
        output.extend_from_slice(&COMPOSITION_FORMAT_VERSION.to_be_bytes());
        match self {
            Self::Guarantee {
                spec_hash,
                component,
                claim,
            } => {
                output.push(0);
                output.extend_from_slice(spec_hash.as_bytes());
                component.encode_to(output)?;
                output.extend_from_slice(claim.as_bytes());
            }
            Self::Frame {
                spec_hash,
                component,
                protected,
                claim,
            } => {
                output.push(1);
                output.extend_from_slice(spec_hash.as_bytes());
                component.encode_to(output)?;
                put_blob(output, &protected.canonical_bytes()?)?;
                output.extend_from_slice(claim.as_bytes());
            }
            Self::AssumptionDischarge {
                spec_hash,
                component,
                assumption,
                providers,
            } => {
                output.push(2);
                output.extend_from_slice(spec_hash.as_bytes());
                component.encode_to(output)?;
                output.extend_from_slice(assumption.as_bytes());
                put_u32_length(output, providers.len())?;
                for provider in providers {
                    provider.encode_to(output)?;
                }
            }
            Self::Coupling { spec_hash, claim } => {
                output.push(3);
                output.extend_from_slice(spec_hash.as_bytes());
                output.extend_from_slice(claim.as_bytes());
            }
            Self::ConflictLaw { spec_hash, law } => {
                output.push(4);
                output.extend_from_slice(spec_hash.as_bytes());
                law.encode_to(output)?;
            }
            Self::ParallelParity {
                context,
                sequential_result,
                composed_result,
            } => {
                output.push(5);
                put_blob(output, &context.canonical_bytes()?)?;
                output.extend_from_slice(sequential_result.as_bytes());
                output.extend_from_slice(composed_result.as_bytes());
            }
        }
        Ok(())
    }
}

/// External verifier for exact, content-addressed composition statements.
pub trait EvidenceVerifier {
    /// Returns true only when the artifact proves the complete supplied claim
    /// under the verifier's pinned semantics and toolchain.
    fn verify(&self, claim: &CompositionClaim, artifact: Hash32) -> bool;
}

/// One fail-closed composition blocker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompositionBlocker {
    /// The specification could not be canonically committed.
    CompositionIdentityFailure,
    /// A local guarantee lacks accepted evidence.
    MissingGuaranteeEvidence {
        /// Component declaring the guarantee.
        component: ComponentId,
        /// Guarantee claim.
        claim: Hash32,
    },
    /// A frame theorem lacks accepted evidence.
    MissingFrameEvidence {
        /// Component owning the frame.
        component: ComponentId,
        /// Frame claim.
        claim: Hash32,
    },
    /// An assumption lacks an exact, verified discharge.
    MissingAssumptionDischarge {
        /// Component making the assumption.
        component: ComponentId,
        /// Assumption claim.
        claim: Hash32,
    },
    /// A provider binding names a component/guarantee pair absent from the spec.
    UnknownProviderGuarantee {
        /// Assumption being discharged.
        assumption: Hash32,
        /// Provider component.
        component: ComponentId,
        /// Unknown guarantee claim.
        guarantee: Hash32,
    },
    /// A wiring reads an effect absent from the source component footprint.
    UndeclaredWiringSourceEffect {
        /// Source component.
        source: ComponentId,
        /// Effect path consumed by the wiring.
        effect: AccessPath,
    },
    /// Wiring writes a destination without an applicable directional frame permission.
    UnauthorizedWiring {
        /// Source component.
        source: ComponentId,
        /// Destination component.
        destination: ComponentId,
    },
    /// A global coupling theorem lacks accepted evidence.
    MissingCouplingEvidence {
        /// Coupling claim.
        claim: Hash32,
    },
    /// Two nominally parallel components have an unproved conflict.
    ParallelConflict {
        /// Left component.
        left: ComponentId,
        /// Right component.
        right: ComponentId,
        /// Conflict direction or resource class.
        kind: ConflictKind,
    },
    /// A declared conflict law lacks accepted exact-statement evidence.
    MissingConflictLawEvidence {
        /// Left component.
        left: ComponentId,
        /// Right component.
        right: ComponentId,
        /// Conflict kind.
        kind: ConflictKind,
        /// Law claim.
        claim: Hash32,
    },
    /// Expected parallel context does not bind the exact specification/merge order.
    ParallelContextMismatch,
    /// No complete parity evidence was supplied.
    MissingSequentialParityEvidence,
    /// The parity artifact was not accepted by the independent verifier.
    UnverifiedSequentialParityEvidence,
    /// Composed and normative sequential results differ.
    SequentialParityMismatch,
}

/// Result of bounded composition verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionReport {
    blockers: Box<[CompositionBlocker]>,
}

impl CompositionReport {
    /// Returns whether every checked composition obligation succeeded.
    #[must_use]
    pub fn is_verified(&self) -> bool {
        self.blockers.is_empty()
    }

    /// Returns fail-closed blockers.
    #[must_use]
    pub const fn blockers(&self) -> &[CompositionBlocker] {
        &self.blockers
    }
}

/// Checks exact guarantee proofs, provider-bound assumption closure, frames, and coupling.
#[must_use]
pub fn verify_assume_guarantee<H: CommitmentHasher, V: EvidenceVerifier>(
    spec: &CompositionSpec,
    evidence: &CompositionEvidence,
    verifier: &V,
) -> CompositionReport {
    let Ok(spec_hash) = spec.commitment::<H>() else {
        return CompositionReport {
            blockers: Vec::from([CompositionBlocker::CompositionIdentityFailure])
                .into_boxed_slice(),
        };
    };
    let mut blockers = Vec::new();
    for component in spec.components() {
        for guarantee in component.guarantees() {
            let claim = CompositionClaim::Guarantee {
                spec_hash,
                component: component.id(),
                claim: guarantee.claim(),
            };
            if !accepted_claim(evidence, verifier, guarantee.claim(), &claim) {
                blockers.push(CompositionBlocker::MissingGuaranteeEvidence {
                    component: component.id(),
                    claim: guarantee.claim(),
                });
            }
        }
        for frame in component.frames() {
            let claim = CompositionClaim::Frame {
                spec_hash,
                component: component.id(),
                protected: frame.protected().clone(),
                claim: frame.proof_claim(),
            };
            if !accepted_claim(evidence, verifier, frame.proof_claim(), &claim) {
                blockers.push(CompositionBlocker::MissingFrameEvidence {
                    component: component.id(),
                    claim: frame.proof_claim(),
                });
            }
        }
        for assumption in component.assumptions() {
            let Some(discharge) = evidence.discharge(component.id(), assumption.claim()) else {
                blockers.push(CompositionBlocker::MissingAssumptionDischarge {
                    component: component.id(),
                    claim: assumption.claim(),
                });
                continue;
            };
            let mut providers_valid = true;
            for provider in discharge.providers() {
                let exists = spec
                    .component(provider.component())
                    .is_some_and(|candidate| {
                        candidate
                            .guarantees()
                            .iter()
                            .any(|guarantee| guarantee.claim() == provider.guarantee())
                    });
                if !exists {
                    providers_valid = false;
                    blockers.push(CompositionBlocker::UnknownProviderGuarantee {
                        assumption: assumption.claim(),
                        component: provider.component(),
                        guarantee: provider.guarantee(),
                    });
                }
            }
            let claim = CompositionClaim::AssumptionDischarge {
                spec_hash,
                component: component.id(),
                assumption: assumption.claim(),
                providers: discharge.providers().to_vec().into_boxed_slice(),
            };
            if !providers_valid || !verifier.verify(&claim, discharge.artifact()) {
                blockers.push(CompositionBlocker::MissingAssumptionDischarge {
                    component: component.id(),
                    claim: assumption.claim(),
                });
            }
        }
    }

    for wiring in spec.wirings() {
        let source_declared = spec
            .component(wiring.source_component())
            .is_some_and(|source| source.footprint().effects().covers(wiring.source_effect()));
        if !source_declared {
            blockers.push(CompositionBlocker::UndeclaredWiringSourceEffect {
                source: wiring.source_component(),
                effect: wiring.source_effect().clone(),
            });
        }
        let permitted = spec
            .component(wiring.destination_component())
            .is_some_and(|destination| {
                destination.frames().iter().any(|frame| {
                    frame.protected().covers(wiring.destination_path())
                        && frame
                            .allowed_writers()
                            .binary_search(&wiring.source_component())
                            .is_ok()
                })
            });
        if !permitted {
            blockers.push(CompositionBlocker::UnauthorizedWiring {
                source: wiring.source_component(),
                destination: wiring.destination_component(),
            });
        }
    }

    for claim_hash in spec.coupling_claims() {
        let claim = CompositionClaim::Coupling {
            spec_hash,
            claim: *claim_hash,
        };
        if !accepted_claim(evidence, verifier, *claim_hash, &claim) {
            blockers.push(CompositionBlocker::MissingCouplingEvidence { claim: *claim_hash });
        }
    }
    CompositionReport {
        blockers: blockers.into_boxed_slice(),
    }
}

/// Checks assume-guarantee obligations, conservative effect/outbox conflicts,
/// exact conflict laws, and proof-carrying sequential parity.
#[must_use]
pub fn verify_deterministic_parallel<H: CommitmentHasher, V: EvidenceVerifier>(
    spec: &CompositionSpec,
    evidence: &CompositionEvidence,
    expected_context: &ParallelVerificationContext,
    verifier: &V,
) -> CompositionReport {
    let mut blockers = verify_assume_guarantee::<H, V>(spec, evidence, verifier)
        .blockers
        .into_vec();
    let Ok(spec_hash) = spec.commitment::<H>() else {
        if !blockers
            .iter()
            .any(|item| matches!(item, CompositionBlocker::CompositionIdentityFailure))
        {
            blockers.push(CompositionBlocker::CompositionIdentityFailure);
        }
        return CompositionReport {
            blockers: blockers.into_boxed_slice(),
        };
    };
    if expected_context.composition_spec_hash() != spec_hash
        || expected_context.merge_order() != spec.merge_order()
    {
        blockers.push(CompositionBlocker::ParallelContextMismatch);
    }

    for left_index in 0..spec.components().len() {
        for right_index in left_index + 1..spec.components().len() {
            let left = &spec.components()[left_index];
            let right = &spec.components()[right_index];
            for conflict in component_conflicts(left, right) {
                let kind = conflict.kind();
                let Some(law) = spec.parallel_law(left.id(), right.id(), kind) else {
                    blockers.push(CompositionBlocker::ParallelConflict {
                        left: left.id(),
                        right: right.id(),
                        kind,
                    });
                    continue;
                };
                let claim = CompositionClaim::ConflictLaw { spec_hash, law };
                if !accepted_claim(evidence, verifier, law.claim(), &claim) {
                    blockers.push(CompositionBlocker::MissingConflictLawEvidence {
                        left: left.id(),
                        right: right.id(),
                        kind,
                        claim: law.claim(),
                    });
                }
            }
        }
    }

    match evidence.parity() {
        None => blockers.push(CompositionBlocker::MissingSequentialParityEvidence),
        Some(parity) => {
            if parity.context() != expected_context {
                blockers.push(CompositionBlocker::ParallelContextMismatch);
            }
            if parity.sequential_result() != parity.composed_result() {
                blockers.push(CompositionBlocker::SequentialParityMismatch);
            }
            let claim = CompositionClaim::ParallelParity {
                context: Box::new(parity.context().clone()),
                sequential_result: parity.sequential_result(),
                composed_result: parity.composed_result(),
            };
            if !verifier.verify(&claim, parity.artifact()) {
                blockers.push(CompositionBlocker::UnverifiedSequentialParityEvidence);
            }
        }
    }

    CompositionReport {
        blockers: blockers.into_boxed_slice(),
    }
}

fn component_conflicts(left: &ComponentContract, right: &ComponentContract) -> Vec<Conflict> {
    let mut output = conflicts(left.footprint(), right.footprint());
    let left_effects = !left.footprint().effects().is_empty();
    let right_effects = !right.footprint().effects().is_empty();
    let left_outbox = !left.outbox().is_empty();
    let right_outbox = !right.outbox().is_empty();
    if left_outbox && right_outbox {
        output.push(Conflict {
            kind: ConflictKind::OutboxOutbox,
        });
    }
    if left_effects && right_outbox {
        output.push(Conflict {
            kind: ConflictKind::LeftEffectRightOutbox,
        });
    }
    if right_effects && left_outbox {
        output.push(Conflict {
            kind: ConflictKind::RightEffectLeftOutbox,
        });
    }
    output
}

fn accepted_claim<V: EvidenceVerifier>(
    evidence: &CompositionEvidence,
    verifier: &V,
    claim_hash: Hash32,
    claim: &CompositionClaim,
) -> bool {
    evidence
        .claim(claim_hash)
        .is_some_and(|item| verifier.verify(claim, item.artifact()))
}

fn has_duplicate_components(components: &[ComponentId]) -> bool {
    let mut copy = components.to_vec();
    copy.sort();
    copy.windows(2).any(|pair| pair[0] == pair[1])
}

fn hash_canonical<H: CommitmentHasher>(
    domain_name: &'static str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, ContractError> {
    let bytes = value.canonical_bytes().map_err(ContractError::Encode)?;
    let domain =
        Domain::new(domain_name, COMPOSITION_FORMAT_VERSION).map_err(ContractError::Encode)?;
    commitment::<H>(domain, &bytes).map_err(ContractError::Encode)
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

/// Contract construction failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractError {
    /// A stable identifier or composition version was zero.
    ZeroIdentifier,
    /// A required commitment was zero.
    ZeroHash,
    /// A path exceeds the supported nesting bound.
    PathTooDeep,
    /// A wildcard is not the terminal atom.
    NonTerminalWildcard,
    /// A path set exceeds its bound.
    PathSetTooLarge,
    /// A path appears more than once.
    DuplicatePath,
    /// The component set is empty or exceeds its bound.
    ComponentCardinality,
    /// A component identifier is duplicated.
    DuplicateComponent,
    /// A claim or frame key is duplicated.
    DuplicateClaim,
    /// A contract exceeds the claim bound.
    TooManyClaims,
    /// Merge order is not an exact component permutation.
    InvalidMergeOrder,
    /// Wiring is duplicated.
    DuplicateWiring,
    /// Wiring references an unknown component or zero schema commitment.
    UnknownComponent,
    /// Assumption discharge has no providers or duplicate providers.
    InvalidAssumptionDischarge,
    /// Evidence is duplicated.
    DuplicateEvidence,
    /// Too many parallel conflict laws were declared.
    TooManyParallelLaws,
    /// A parallel law has invalid component references.
    InvalidParallelLaw,
    /// Two parallel laws cover the same pair and conflict kind.
    DuplicateParallelLaw,
    /// Canonical encoding or commitment construction failed.
    Encode(EncodeError),
}

impl From<EncodeError> for ContractError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroIdentifier => formatter.write_str("composition identifier is zero"),
            Self::ZeroHash => formatter.write_str("composition commitment is zero"),
            Self::PathTooDeep => formatter.write_str("access path exceeds depth bound"),
            Self::NonTerminalWildcard => {
                formatter.write_str("access-path wildcard must be terminal")
            }
            Self::PathSetTooLarge => formatter.write_str("path set exceeds cardinality bound"),
            Self::DuplicatePath => formatter.write_str("path set contains a duplicate"),
            Self::ComponentCardinality => {
                formatter.write_str("component set is empty or too large")
            }
            Self::DuplicateComponent => formatter.write_str("component identifier is duplicated"),
            Self::DuplicateClaim => formatter.write_str("claim identity is duplicated"),
            Self::TooManyClaims => formatter.write_str("component contract exceeds claim bound"),
            Self::InvalidMergeOrder => {
                formatter.write_str("merge order is not an exact component permutation")
            }
            Self::DuplicateWiring => formatter.write_str("wiring is duplicated"),
            Self::UnknownComponent => {
                formatter.write_str("wiring or law references an invalid component")
            }
            Self::InvalidAssumptionDischarge => {
                formatter.write_str("assumption discharge providers are invalid")
            }
            Self::DuplicateEvidence => formatter.write_str("evidence binding is duplicated"),
            Self::TooManyParallelLaws => formatter.write_str("too many parallel conflict laws"),
            Self::InvalidParallelLaw => formatter.write_str("parallel conflict law is invalid"),
            Self::DuplicateParallelLaw => {
                formatter.write_str("parallel conflict law is duplicated")
            }
            Self::Encode(error) => write!(formatter, "composition encoding failed: {error}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ContractError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

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

    struct ExactVerifier;

    impl EvidenceVerifier for ExactVerifier {
        fn verify(&self, claim: &CompositionClaim, artifact: Hash32) -> bool {
            claim.commitment::<TestHasher>().ok() == Some(artifact)
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn path(namespace: u32, field: u16) -> AccessPath {
        AccessPath::try_new(namespace, vec![PathAtom::Field(field)])
            .unwrap_or_else(|error| panic!("path: {error}"))
    }

    fn set(paths: Vec<AccessPath>) -> PathSet {
        PathSet::try_new(paths).unwrap_or_else(|error| panic!("set: {error}"))
    }

    fn contract(
        id: u32,
        footprint: Footprint,
        outbox: PathSet,
        assumptions: Vec<Assumption>,
        guarantees: Vec<Guarantee>,
        frames: Vec<FrameRule>,
    ) -> ComponentContract {
        ComponentContract::try_new_with_outbox(
            ComponentId::new(id),
            hash(50),
            footprint,
            outbox,
            assumptions,
            guarantees,
            frames,
        )
        .unwrap_or_else(|error| panic!("contract: {error}"))
    }

    fn context(spec: &CompositionSpec) -> ParallelVerificationContext {
        ParallelVerificationContext::try_new(
            spec.commitment::<TestHasher>()
                .unwrap_or_else(|error| panic!("spec hash: {error}")),
            hash(71),
            hash(72),
            hash(73),
            hash(74),
            hash(75),
            hash(76),
            spec.merge_order().to_vec(),
        )
        .unwrap_or_else(|error| panic!("context: {error}"))
    }

    fn claim_evidence(claim_hash: Hash32, claim: CompositionClaim) -> ClaimEvidence {
        ClaimEvidence::new(
            claim_hash,
            claim
                .commitment::<TestHasher>()
                .unwrap_or_else(|error| panic!("claim hash: {error}")),
        )
    }

    #[test]
    fn directional_path_relations_are_distinct() {
        let ancestor = AccessPath::try_new(1, vec![PathAtom::Field(1)])
            .unwrap_or_else(|error| panic!("ancestor: {error}"));
        let descendant = AccessPath::try_new(1, vec![PathAtom::Field(1), PathAtom::Field(2)])
            .unwrap_or_else(|error| panic!("descendant: {error}"));
        assert!(ancestor.overlaps(&descendant));
        assert!(ancestor.covers(&descendant));
        assert!(!descendant.covers(&ancestor));
    }

    #[test]
    fn effects_conflict_conservatively_even_when_paths_differ() {
        let left = Footprint::new(
            PathSet::empty(),
            PathSet::empty(),
            PathSet::empty(),
            set(vec![path(10, 1)]),
        );
        let right = Footprint::new(
            PathSet::empty(),
            PathSet::empty(),
            PathSet::empty(),
            set(vec![path(11, 2)]),
        );
        assert_eq!(
            conflicts(&left, &right),
            vec![Conflict {
                kind: ConflictKind::EffectEffect
            }]
        );
    }

    #[test]
    fn provider_set_is_part_of_the_verified_assumption_claim() {
        let guarantee_claim = hash(20);
        let assumption_claim = hash(21);
        let provider = contract(
            1,
            Footprint::default(),
            PathSet::empty(),
            vec![],
            vec![Guarantee::new(guarantee_claim, PathSet::empty())],
            vec![],
        );
        let consumer = contract(
            2,
            Footprint::default(),
            PathSet::empty(),
            vec![Assumption::new(assumption_claim, PathSet::empty())],
            vec![],
            vec![],
        );
        let spec = CompositionSpec::try_new(
            2,
            vec![provider, consumer],
            vec![],
            vec![],
            vec![ComponentId::new(1), ComponentId::new(2)],
        )
        .unwrap_or_else(|error| panic!("spec: {error}"));
        let spec_hash = spec
            .commitment::<TestHasher>()
            .unwrap_or_else(|error| panic!("spec hash: {error}"));
        let providers = vec![
            ProviderGuarantee::try_new(ComponentId::new(1), guarantee_claim)
                .unwrap_or_else(|error| panic!("provider: {error}")),
        ];
        let discharge_claim = CompositionClaim::AssumptionDischarge {
            spec_hash,
            component: ComponentId::new(2),
            assumption: assumption_claim,
            providers: providers.clone().into_boxed_slice(),
        };
        let discharge = AssumptionDischarge::try_new(
            ComponentId::new(2),
            assumption_claim,
            providers,
            discharge_claim
                .commitment::<TestHasher>()
                .unwrap_or_else(|error| panic!("discharge hash: {error}")),
        )
        .unwrap_or_else(|error| panic!("discharge: {error}"));
        let guarantee = CompositionClaim::Guarantee {
            spec_hash,
            component: ComponentId::new(1),
            claim: guarantee_claim,
        };
        let evidence = CompositionEvidence::try_new(
            vec![claim_evidence(guarantee_claim, guarantee)],
            vec![discharge],
            None,
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        assert!(
            verify_assume_guarantee::<TestHasher, _>(&spec, &evidence, &ExactVerifier)
                .is_verified()
        );

        let wrong_provider = ProviderGuarantee::try_new(ComponentId::new(2), guarantee_claim)
            .unwrap_or_else(|error| panic!("wrong provider: {error}"));
        let substituted = AssumptionDischarge::try_new(
            ComponentId::new(2),
            assumption_claim,
            vec![wrong_provider],
            evidence.assumption_discharges()[0].artifact(),
        )
        .unwrap_or_else(|error| panic!("substituted: {error}"));
        let substituted_evidence = CompositionEvidence::try_new(
            evidence.claim_evidence().to_vec(),
            vec![substituted],
            None,
        )
        .unwrap_or_else(|error| panic!("substituted evidence: {error}"));
        assert!(
            !verify_assume_guarantee::<TestHasher, _>(&spec, &substituted_evidence, &ExactVerifier)
                .is_verified()
        );
    }

    #[test]
    fn effect_and_outbox_conflicts_require_exact_laws_and_parity() {
        let left_id = ComponentId::new(1);
        let right_id = ComponentId::new(2);
        let left = contract(
            1,
            Footprint::new(
                PathSet::empty(),
                PathSet::empty(),
                PathSet::empty(),
                set(vec![path(20, 1)]),
            ),
            PathSet::empty(),
            vec![],
            vec![],
            vec![],
        );
        let right = contract(
            2,
            Footprint::default(),
            set(vec![path(30, 1)]),
            vec![],
            vec![],
            vec![],
        );
        let law_claim = hash(40);
        let law = ParallelConflictLaw::try_new(
            left_id,
            right_id,
            ConflictKind::LeftEffectRightOutbox,
            law_claim,
        )
        .unwrap_or_else(|error| panic!("law: {error}"));
        let spec = CompositionSpec::try_new_with_parallel_laws(
            2,
            vec![left, right],
            vec![],
            vec![],
            vec![left_id, right_id],
            vec![law],
        )
        .unwrap_or_else(|error| panic!("spec: {error}"));
        let expected = context(&spec);
        let result_hash = hash(80);
        let parity_claim = CompositionClaim::ParallelParity {
            context: Box::new(expected.clone()),
            sequential_result: result_hash,
            composed_result: result_hash,
        };
        let parity = ParallelParityEvidence::try_new(
            expected.clone(),
            result_hash,
            result_hash,
            parity_claim
                .commitment::<TestHasher>()
                .unwrap_or_else(|error| panic!("parity claim: {error}")),
        )
        .unwrap_or_else(|error| panic!("parity: {error}"));
        let law_statement = CompositionClaim::ConflictLaw {
            spec_hash: spec
                .commitment::<TestHasher>()
                .unwrap_or_else(|error| panic!("spec hash: {error}")),
            law,
        };
        let evidence = CompositionEvidence::try_new(
            vec![claim_evidence(law_claim, law_statement)],
            vec![],
            Some(parity),
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        assert!(
            verify_deterministic_parallel::<TestHasher, _>(
                &spec,
                &evidence,
                &expected,
                &ExactVerifier
            )
            .is_verified()
        );
    }

    #[test]
    fn raw_equal_result_hashes_without_bound_parity_do_not_promote() {
        let left_id = ComponentId::new(1);
        let right_id = ComponentId::new(2);
        let spec = CompositionSpec::try_new(
            2,
            vec![
                contract(
                    1,
                    Footprint::default(),
                    PathSet::empty(),
                    vec![],
                    vec![],
                    vec![],
                ),
                contract(
                    2,
                    Footprint::default(),
                    PathSet::empty(),
                    vec![],
                    vec![],
                    vec![],
                ),
            ],
            vec![],
            vec![],
            vec![left_id, right_id],
        )
        .unwrap_or_else(|error| panic!("spec: {error}"));
        let expected = context(&spec);
        let evidence = CompositionEvidence::try_new(vec![], vec![], None)
            .unwrap_or_else(|error| panic!("evidence: {error}"));
        assert!(matches!(
            verify_deterministic_parallel::<TestHasher, _>(
                &spec,
                &evidence,
                &expected,
                &ExactVerifier
            )
            .blockers(),
            [CompositionBlocker::MissingSequentialParityEvidence]
        ));
    }

    #[test]
    fn parity_context_mutation_is_rejected() {
        let first = ComponentId::new(1);
        let second = ComponentId::new(2);
        let spec = CompositionSpec::try_new(
            2,
            vec![
                contract(
                    1,
                    Footprint::default(),
                    PathSet::empty(),
                    vec![],
                    vec![],
                    vec![],
                ),
                contract(
                    2,
                    Footprint::default(),
                    PathSet::empty(),
                    vec![],
                    vec![],
                    vec![],
                ),
            ],
            vec![],
            vec![],
            vec![first, second],
        )
        .unwrap_or_else(|error| panic!("spec: {error}"));
        let expected = context(&spec);
        let mutated = ParallelVerificationContext::try_new(
            expected.composition_spec_hash(),
            hash(99),
            hash(72),
            hash(73),
            hash(74),
            hash(75),
            hash(76),
            expected.merge_order().to_vec(),
        )
        .unwrap_or_else(|error| panic!("mutated: {error}"));
        let result_hash = hash(80);
        let claim = CompositionClaim::ParallelParity {
            context: Box::new(mutated.clone()),
            sequential_result: result_hash,
            composed_result: result_hash,
        };
        let parity = ParallelParityEvidence::try_new(
            mutated,
            result_hash,
            result_hash,
            claim
                .commitment::<TestHasher>()
                .unwrap_or_else(|error| panic!("claim: {error}")),
        )
        .unwrap_or_else(|error| panic!("parity: {error}"));
        let evidence = CompositionEvidence::try_new(vec![], vec![], Some(parity))
            .unwrap_or_else(|error| panic!("evidence: {error}"));
        assert!(
            verify_deterministic_parallel::<TestHasher, _>(
                &spec,
                &evidence,
                &expected,
                &ExactVerifier
            )
            .blockers()
            .iter()
            .any(|item| matches!(item, CompositionBlocker::ParallelContextMismatch))
        );
    }
}
