//! Assume-guarantee contracts and deterministic composition evidence.
//!
//! This crate reifies component access, assumptions, guarantees, frame rules,
//! wiring, and proof references as closed immutable values. It does not execute
//! components and does not treat the presence of an evidence hash as a proof;
//! callers must supply an [`EvidenceVerifier`] that validates each artifact.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, EncodeError, Hash32};

const MAX_PATH_ATOMS: usize = 64;
const MAX_PATHS_PER_SET: usize = 4096;
const MAX_COMPONENTS: usize = 4096;
const MAX_CLAIMS: usize = 16_384;

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

/// One navigation atom in a semantic read, write, context, or effect path.
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
    /// Commitment of a canonical map key.
    MapKey(Hash32),
    /// Terminal wildcard matching any descendant of the current path.
    AnyDescendant,
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
    pub fn atoms(&self) -> &[PathAtom] {
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

    /// Returns whether this declared path contains every value designated by
    /// `other`.
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
            match atom {
                PathAtom::Field(id) => {
                    output.push(0);
                    output.extend_from_slice(&id.to_be_bytes());
                }
                PathAtom::TupleIndex(index) => {
                    output.push(1);
                    output.extend_from_slice(&index.to_be_bytes());
                }
                PathAtom::VectorIndex(index) => {
                    output.push(2);
                    output.extend_from_slice(&index.to_be_bytes());
                }
                PathAtom::SumPayload => output.push(3),
                PathAtom::MapKey(hash) => {
                    output.push(4);
                    output.extend_from_slice(hash.as_bytes());
                }
                PathAtom::AnyDescendant => output.push(5),
            }
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
    pub fn paths(&self) -> &[AccessPath] {
        &self.paths
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

/// Complete semantic footprint of one component or task.
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

/// Direction of one deterministic-parallel conflict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictKind {
    /// Both components may write an overlapping value.
    WriteWrite,
    /// The left component may write a value read by the right component.
    LeftWriteRightRead,
    /// The right component may write a value read by the left component.
    RightWriteLeftRead,
}

/// One detected semantic conflict.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Conflict {
    kind: ConflictKind,
}

impl Conflict {
    /// Returns the conflict direction.
    #[must_use]
    pub const fn kind(&self) -> ConflictKind {
        self.kind
    }
}

/// Computes the complete default noninterference conflict set.
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
        allowed_writers.sort();
        if allowed_writers.windows(2).any(|pair| pair[0] == pair[1]) {
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

/// Strict contract for one composable FCIS component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentContract {
    id: ComponentId,
    profile_hash: Hash32,
    footprint: Footprint,
    assumptions: Box<[Assumption]>,
    guarantees: Box<[Guarantee]>,
    frames: Box<[FrameRule]>,
}

impl ComponentContract {
    /// Creates a contract and rejects duplicate claim identities.
    pub fn try_new(
        id: ComponentId,
        profile_hash: Hash32,
        footprint: Footprint,
        mut assumptions: Vec<Assumption>,
        mut guarantees: Vec<Guarantee>,
        mut frames: Vec<FrameRule>,
    ) -> Result<Self, ContractError> {
        if assumptions.len() + guarantees.len() + frames.len() > MAX_CLAIMS {
            return Err(ContractError::TooManyClaims);
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

    /// Returns the complete semantic footprint.
    #[must_use]
    pub const fn footprint(&self) -> &Footprint {
        &self.footprint
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

/// Canonical composition specification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionSpec {
    version: u16,
    components: Box<[ComponentContract]>,
    wirings: Box<[Wiring]>,
    coupling_claims: Box<[Hash32]>,
    merge_order: Box<[ComponentId]>,
}

impl CompositionSpec {
    /// Creates a bounded specification and validates all component references.
    pub fn try_new(
        version: u16,
        mut components: Vec<ComponentContract>,
        mut wirings: Vec<Wiring>,
        mut coupling_claims: Vec<Hash32>,
        merge_order: Vec<ComponentId>,
    ) -> Result<Self, ContractError> {
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
            if components
                .binary_search_by_key(&wiring.source_component, ComponentContract::id)
                .is_err()
                || components
                    .binary_search_by_key(&wiring.destination_component, ComponentContract::id)
                    .is_err()
            {
                return Err(ContractError::UnknownComponent);
            }
        }
        coupling_claims.sort();
        if coupling_claims.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ContractError::DuplicateClaim);
        }
        Ok(Self {
            version,
            components: components.into_boxed_slice(),
            wirings: wirings.into_boxed_slice(),
            coupling_claims: coupling_claims.into_boxed_slice(),
            merge_order: merge_order.into_boxed_slice(),
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

    fn component(&self, id: ComponentId) -> Option<&ComponentContract> {
        self.components
            .binary_search_by_key(&id, ComponentContract::id)
            .ok()
            .map(|index| &self.components[index])
    }
}

/// Evidence binding one claim to one external proof artifact.
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

/// Evidence that peer guarantees and wiring establish one assumption.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssumptionDischarge {
    component: ComponentId,
    assumption: Hash32,
    provider_guarantees: Box<[Hash32]>,
    artifact: Hash32,
}

impl AssumptionDischarge {
    /// Creates an assumption discharge and canonicalizes provider claims.
    pub fn try_new(
        component: ComponentId,
        assumption: Hash32,
        mut provider_guarantees: Vec<Hash32>,
        artifact: Hash32,
    ) -> Result<Self, ContractError> {
        provider_guarantees.sort();
        if provider_guarantees.is_empty()
            || provider_guarantees
                .windows(2)
                .any(|pair| pair[0] == pair[1])
        {
            return Err(ContractError::InvalidAssumptionDischarge);
        }
        Ok(Self {
            component,
            assumption,
            provider_guarantees: provider_guarantees.into_boxed_slice(),
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

    /// Returns provider-guarantee commitments.
    #[must_use]
    pub const fn provider_guarantees(&self) -> &[Hash32] {
        &self.provider_guarantees
    }

    /// Returns the assumption-closure artifact.
    #[must_use]
    pub const fn artifact(&self) -> Hash32 {
        self.artifact
    }
}

/// Complete bounded evidence index for a composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionEvidence {
    claim_evidence: Box<[ClaimEvidence]>,
    assumption_discharges: Box<[AssumptionDischarge]>,
    sequential_commitment: Hash32,
    composed_commitment: Hash32,
}

impl CompositionEvidence {
    /// Creates an evidence index and rejects duplicate claim/discharge keys.
    pub fn try_new(
        mut claim_evidence: Vec<ClaimEvidence>,
        mut assumption_discharges: Vec<AssumptionDischarge>,
        sequential_commitment: Hash32,
        composed_commitment: Hash32,
    ) -> Result<Self, ContractError> {
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
            sequential_commitment,
            composed_commitment,
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

    /// Returns the normative sequential result commitment.
    #[must_use]
    pub const fn sequential_commitment(&self) -> Hash32 {
        self.sequential_commitment
    }

    /// Returns the composed result commitment.
    #[must_use]
    pub const fn composed_commitment(&self) -> Hash32 {
        self.composed_commitment
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

/// External verifier for a content-addressed proof artifact.
pub trait EvidenceVerifier {
    /// Returns true only when the artifact proves the exact claim under the
    /// verifier's pinned semantics and toolchain.
    fn verify(&self, claim: Hash32, artifact: Hash32) -> bool;
}

/// One fail-closed composition blocker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompositionBlocker {
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
    /// An assumption lacks an exact discharge.
    MissingAssumptionDischarge {
        /// Component making the assumption.
        component: ComponentId,
        /// Assumption claim.
        claim: Hash32,
    },
    /// A discharge references a guarantee absent from every component contract.
    UnknownProviderGuarantee {
        /// Assumption being discharged.
        assumption: Hash32,
        /// Unknown guarantee claim.
        guarantee: Hash32,
    },
    /// A wiring reads an effect absent from the source component's declared
    /// effect footprint.
    UndeclaredWiringSourceEffect {
        /// Source component.
        source: ComponentId,
        /// Effect path consumed by the wiring.
        effect: AccessPath,
    },
    /// Wiring writes a destination without an applicable frame permission.
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
        /// Conflict direction.
        kind: ConflictKind,
    },
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

/// Checks guarantee proofs, assumption closure, frame permissions, and coupling theorems.
#[must_use]
pub fn verify_assume_guarantee<V: EvidenceVerifier>(
    spec: &CompositionSpec,
    evidence: &CompositionEvidence,
    verifier: &V,
) -> CompositionReport {
    let mut blockers = Vec::new();
    for component in spec.components() {
        for guarantee in component.guarantees() {
            if !accepted_claim(evidence, verifier, guarantee.claim()) {
                blockers.push(CompositionBlocker::MissingGuaranteeEvidence {
                    component: component.id(),
                    claim: guarantee.claim(),
                });
            }
        }
        for frame in component.frames() {
            if !accepted_claim(evidence, verifier, frame.proof_claim()) {
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
            if !verifier.verify(assumption.claim(), discharge.artifact()) {
                blockers.push(CompositionBlocker::MissingAssumptionDischarge {
                    component: component.id(),
                    claim: assumption.claim(),
                });
            }
            for provider in discharge.provider_guarantees() {
                if !spec.components().iter().any(|candidate| {
                    candidate
                        .guarantees()
                        .iter()
                        .any(|guarantee| guarantee.claim() == *provider)
                }) {
                    blockers.push(CompositionBlocker::UnknownProviderGuarantee {
                        assumption: assumption.claim(),
                        guarantee: *provider,
                    });
                }
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
                    frame.protected().overlaps(wiring.destination_path())
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

    for claim in spec.coupling_claims() {
        if !accepted_claim(evidence, verifier, *claim) {
            blockers.push(CompositionBlocker::MissingCouplingEvidence { claim: *claim });
        }
    }
    CompositionReport {
        blockers: blockers.into_boxed_slice(),
    }
}

/// Checks assume-guarantee obligations plus default noninterference and sequential parity.
#[must_use]
pub fn verify_deterministic_parallel<V: EvidenceVerifier>(
    spec: &CompositionSpec,
    evidence: &CompositionEvidence,
    verifier: &V,
) -> CompositionReport {
    let mut blockers = verify_assume_guarantee(spec, evidence, verifier)
        .blockers
        .into_vec();
    for left_index in 0..spec.components().len() {
        for right_index in left_index + 1..spec.components().len() {
            let left = &spec.components()[left_index];
            let right = &spec.components()[right_index];
            for conflict in conflicts(left.footprint(), right.footprint()) {
                blockers.push(CompositionBlocker::ParallelConflict {
                    left: left.id(),
                    right: right.id(),
                    kind: conflict.kind(),
                });
            }
        }
    }
    if evidence.sequential_commitment() != evidence.composed_commitment() {
        blockers.push(CompositionBlocker::SequentialParityMismatch);
    }
    CompositionReport {
        blockers: blockers.into_boxed_slice(),
    }
}

fn accepted_claim<V: EvidenceVerifier>(
    evidence: &CompositionEvidence,
    verifier: &V,
    claim: Hash32,
) -> bool {
    evidence
        .claim(claim)
        .is_some_and(|item| verifier.verify(item.claim(), item.artifact()))
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractError {
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
    /// Merge order is not a permutation of component identifiers.
    InvalidMergeOrder,
    /// Wiring is duplicated.
    DuplicateWiring,
    /// Wiring references an unknown component.
    UnknownComponent,
    /// Assumption discharge has no providers or duplicate providers.
    InvalidAssumptionDischarge,
    /// Evidence is duplicated.
    DuplicateEvidence,
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PathTooDeep => "access path exceeds depth bound",
            Self::NonTerminalWildcard => "access-path wildcard must be terminal",
            Self::PathSetTooLarge => "path set exceeds cardinality bound",
            Self::DuplicatePath => "path set contains a duplicate",
            Self::ComponentCardinality => "component set is empty or too large",
            Self::DuplicateComponent => "component identifier is duplicated",
            Self::DuplicateClaim => "claim identity is duplicated",
            Self::TooManyClaims => "component contract exceeds claim bound",
            Self::InvalidMergeOrder => "merge order is not an exact component permutation",
            Self::DuplicateWiring => "wiring is duplicated",
            Self::UnknownComponent => "wiring references an unknown component",
            Self::InvalidAssumptionDischarge => "assumption discharge providers are invalid",
            Self::DuplicateEvidence => "evidence binding is duplicated",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    struct ExactVerifier;

    impl EvidenceVerifier for ExactVerifier {
        fn verify(&self, claim: Hash32, artifact: Hash32) -> bool {
            claim == artifact
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn path(namespace: u32, field: u16) -> AccessPath {
        AccessPath::try_new(namespace, vec![PathAtom::Field(field)])
            .unwrap_or_else(|error| panic!("path: {error}"))
    }

    #[test]
    fn read_write_conflict_is_detected_in_both_directions() {
        let left = Footprint::new(
            PathSet::try_new(vec![path(1, 1)]).unwrap_or_default(),
            PathSet::try_new(vec![path(1, 2)]).unwrap_or_default(),
            PathSet::empty(),
            PathSet::empty(),
        );
        let right = Footprint::new(
            PathSet::try_new(vec![path(1, 2)]).unwrap_or_default(),
            PathSet::try_new(vec![path(1, 1)]).unwrap_or_default(),
            PathSet::empty(),
            PathSet::empty(),
        );
        let found = conflicts(&left, &right);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn wildcard_and_prefix_paths_overlap() {
        let broad = AccessPath::try_new(1, vec![PathAtom::Field(7), PathAtom::AnyDescendant])
            .unwrap_or_else(|error| panic!("path: {error}"));
        let narrow = AccessPath::try_new(
            1,
            vec![
                PathAtom::Field(7),
                PathAtom::Field(2),
                PathAtom::TupleIndex(0),
            ],
        )
        .unwrap_or_else(|error| panic!("path: {error}"));
        assert!(broad.overlaps(&narrow));
        assert!(broad.covers(&narrow));
        assert!(!narrow.covers(&broad));
    }

    #[test]
    fn complete_assume_guarantee_evidence_verifies() {
        let guarantee = Guarantee::new(hash(1), PathSet::empty());
        let frame = FrameRule::try_new(path(1, 9), vec![ComponentId::new(1)], hash(2))
            .unwrap_or_else(|error| panic!("frame: {error}"));
        let producer = ComponentContract::try_new(
            ComponentId::new(1),
            hash(10),
            Footprint::new(
                PathSet::empty(),
                PathSet::empty(),
                PathSet::empty(),
                PathSet::try_new(vec![path(2, 1)]).unwrap_or_default(),
            ),
            Vec::new(),
            vec![guarantee],
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("producer: {error}"));
        let consumer = ComponentContract::try_new(
            ComponentId::new(2),
            hash(11),
            Footprint::default(),
            vec![Assumption::new(hash(3), PathSet::empty())],
            Vec::new(),
            vec![frame],
        )
        .unwrap_or_else(|error| panic!("consumer: {error}"));
        let wiring = Wiring::new(
            ComponentId::new(1),
            path(2, 1),
            ComponentId::new(2),
            path(1, 9),
            hash(12),
        );
        let spec = CompositionSpec::try_new(
            1,
            vec![producer, consumer],
            vec![wiring],
            vec![hash(4)],
            vec![ComponentId::new(1), ComponentId::new(2)],
        )
        .unwrap_or_else(|error| panic!("spec: {error}"));
        let discharge =
            AssumptionDischarge::try_new(ComponentId::new(2), hash(3), vec![hash(1)], hash(3))
                .unwrap_or_else(|error| panic!("discharge: {error}"));
        let evidence = CompositionEvidence::try_new(
            vec![
                ClaimEvidence::new(hash(1), hash(1)),
                ClaimEvidence::new(hash(2), hash(2)),
                ClaimEvidence::new(hash(4), hash(4)),
            ],
            vec![discharge],
            hash(9),
            hash(9),
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        assert!(verify_assume_guarantee(&spec, &evidence, &ExactVerifier).is_verified());
    }

    #[test]
    fn wiring_source_must_be_declared_as_an_effect() {
        let frame = FrameRule::try_new(path(1, 9), vec![ComponentId::new(1)], hash(2))
            .unwrap_or_else(|error| panic!("frame: {error}"));
        let producer = ComponentContract::try_new(
            ComponentId::new(1),
            hash(10),
            Footprint::default(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("producer: {error}"));
        let consumer = ComponentContract::try_new(
            ComponentId::new(2),
            hash(11),
            Footprint::default(),
            Vec::new(),
            Vec::new(),
            vec![frame],
        )
        .unwrap_or_else(|error| panic!("consumer: {error}"));
        let wiring = Wiring::new(
            ComponentId::new(1),
            path(2, 1),
            ComponentId::new(2),
            path(1, 9),
            hash(12),
        );
        let spec = CompositionSpec::try_new(
            1,
            vec![producer, consumer],
            vec![wiring],
            Vec::new(),
            vec![ComponentId::new(1), ComponentId::new(2)],
        )
        .unwrap_or_else(|error| panic!("spec: {error}"));
        let evidence = CompositionEvidence::try_new(
            vec![ClaimEvidence::new(hash(2), hash(2))],
            Vec::new(),
            hash(9),
            hash(9),
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        assert!(matches!(
            verify_assume_guarantee(&spec, &evidence, &ExactVerifier).blockers(),
            [CompositionBlocker::UndeclaredWiringSourceEffect {
                source,
                ..
            }] if *source == ComponentId::new(1)
        ));
    }

    #[test]
    fn parallel_promotion_requires_sequential_parity() {
        let first = ComponentContract::try_new(
            ComponentId::new(1),
            hash(1),
            Footprint::default(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("first: {error}"));
        let second = ComponentContract::try_new(
            ComponentId::new(2),
            hash(2),
            Footprint::default(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap_or_else(|error| panic!("second: {error}"));
        let spec = CompositionSpec::try_new(
            1,
            vec![first, second],
            Vec::new(),
            Vec::new(),
            vec![ComponentId::new(1), ComponentId::new(2)],
        )
        .unwrap_or_else(|error| panic!("spec: {error}"));
        let evidence = CompositionEvidence::try_new(Vec::new(), Vec::new(), hash(1), hash(2))
            .unwrap_or_else(|error| panic!("evidence: {error}"));
        assert!(matches!(
            verify_deterministic_parallel(&spec, &evidence, &ExactVerifier).blockers(),
            [CompositionBlocker::SequentialParityMismatch]
        ));
    }
}
