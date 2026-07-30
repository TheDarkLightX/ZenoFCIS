//! Versioned sparse authenticated-state planning behind canonical semantic patches.
//!
//! The semantic ZCVE state and its root remain authoritative. This crate adds
//! an explicitly separate authenticated index root and never silently replaces
//! a profile's existing state-root definition.

#![forbid(unsafe_code)]

use core::fmt;
use std::collections::BTreeMap;

use zeno_fcis_codec::{
    CanonicalEncode, DecodeError, DecodeLimits, Domain, EncodeError, Hash32, commitment,
    decode_value,
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_patch::{CanonicalPatch, PatchError};
use zeno_fcis_value::Value;

/// Maximum logical leaves in the inspectable reference backend.
pub const MAX_REFERENCE_LEAVES: usize = 4_096;
/// Canonical encoding version for authenticated update plans.
pub const AUTHENTICATED_PLAN_ENCODING_VERSION: u16 = 2;
/// Canonical encoding version for sparse membership and absence proofs.
pub const SPARSE_PROOF_ENCODING_VERSION: u16 = 1;
const TREE_DEPTH: usize = 256;

/// Explicit resource bounds for persisted authenticated plans and proofs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedDecodeLimits {
    /// Maximum complete canonical input bytes.
    pub max_input_bytes: u64,
    /// Maximum logical leaf writes in one authenticated plan.
    pub max_writes: u32,
    /// Maximum stale-node candidates in one authenticated plan.
    pub max_stale_nodes: u32,
    /// Limits applied to every nested canonical value.
    pub value: DecodeLimits,
}

impl Default for AuthenticatedDecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 4 * 1024 * 1024,
            max_writes: MAX_REFERENCE_LEAVES as u32,
            max_stale_nodes: MAX_REFERENCE_LEAVES as u32,
            value: DecodeLimits::default(),
        }
    }
}

/// Explicit dual-root authenticated profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedProfile {
    tree_id: Hash32,
    profile_hash: Hash32,
    projector_hash: Hash32,
}

impl AuthenticatedProfile {
    /// Creates a profile with explicit tree, activation, and projector identities.
    pub fn try_new(
        tree_id: Hash32,
        profile_hash: Hash32,
        projector_hash: Hash32,
    ) -> Result<Self, AuthError> {
        if tree_id == Hash32::ZERO || profile_hash == Hash32::ZERO || projector_hash == Hash32::ZERO
        {
            return Err(AuthError::ZeroIdentity);
        }
        Ok(Self {
            tree_id,
            profile_hash,
            projector_hash,
        })
    }

    /// Returns the operational tree identity.
    #[must_use]
    pub const fn tree_id(self) -> Hash32 {
        self.tree_id
    }

    /// Returns the reviewed dual-root profile identity.
    #[must_use]
    pub const fn profile_hash(self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the declared state-projector commitment.
    #[must_use]
    pub const fn projector_hash(self) -> Hash32 {
        self.projector_hash
    }
}

impl CanonicalEncode for AuthenticatedProfile {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.tree_id.as_bytes());
        output.extend_from_slice(self.profile_hash.as_bytes());
        output.extend_from_slice(self.projector_hash.as_bytes());
        Ok(())
    }
}

/// Project-supplied projection expected to cover all authenticated logical leaves.
pub trait StateProjector {
    /// Declares the projector identity commitment.
    ///
    /// This is an identity binding, not implementation attestation. Production
    /// setup must select the concrete projector implementation independently.
    fn declared_projector_hash(&self) -> Hash32;
    /// Returns every logical leaf exactly once.
    fn project(&self, state: &Value) -> Result<Vec<(Hash32, Value)>, AuthError>;
}

/// Read-only authenticated tree boundary.
pub trait TreeReader {
    /// Returns the mounted dual-root profile.
    fn profile(&self) -> AuthenticatedProfile;
    /// Returns the monotonically increasing tree version.
    fn version(&self) -> u64;
    /// Returns the current authenticated root.
    fn root(&self) -> Hash32;
    /// Returns one immutable logical leaf.
    fn get(&self, key: Hash32) -> Option<&Value>;
    /// Builds a membership or absence proof at the current root.
    fn prove(&self, key: Hash32) -> Result<SparseProof, AuthError>;
}

/// Atomic authenticated-tree publication boundary.
pub trait TreeWriter: TreeReader {
    /// Applies a complete expected-root/version plan or leaves the tree unchanged.
    fn apply_plan(&mut self, plan: &PlannedAuthenticatedCommit) -> Result<(), AuthError>;
}

/// One logical leaf mutation in canonical key order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LeafWrite {
    /// Insert or replace a leaf.
    Put {
        /// Canonical 256-bit logical key.
        key: Hash32,
        /// Immutable logical leaf value.
        value: Value,
    },
    /// Delete an existing leaf.
    Delete {
        /// Canonical 256-bit logical key.
        key: Hash32,
    },
}

impl LeafWrite {
    const fn key(&self) -> Hash32 {
        match self {
            Self::Put { key, .. } | Self::Delete { key } => *key,
        }
    }
}

/// Canonically ordered logical node batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeBatch {
    writes: Box<[LeafWrite]>,
}

impl NodeBatch {
    fn from_maps(before: &BTreeMap<Hash32, Value>, after: &BTreeMap<Hash32, Value>) -> Self {
        let mut writes = Vec::new();
        for (key, old) in before {
            match after.get(key) {
                None => writes.push(LeafWrite::Delete { key: *key }),
                Some(new) if new != old => writes.push(LeafWrite::Put {
                    key: *key,
                    value: new.clone(),
                }),
                Some(_) => {}
            }
        }
        for (key, value) in after {
            if !before.contains_key(key) {
                writes.push(LeafWrite::Put {
                    key: *key,
                    value: value.clone(),
                });
            }
        }
        writes.sort_by_key(LeafWrite::key);
        Self {
            writes: writes.into_boxed_slice(),
        }
    }

    /// Returns canonical logical writes.
    #[must_use]
    pub const fn writes(&self) -> &[LeafWrite] {
        &self.writes
    }
}

impl CanonicalEncode for NodeBatch {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_length(output, self.writes.len())?;
        for write in &self.writes {
            match write {
                LeafWrite::Put { key, value } => {
                    output.push(0);
                    output.extend_from_slice(key.as_bytes());
                    put_blob(output, &value.canonical_bytes()?)?;
                }
                LeafWrite::Delete { key } => {
                    output.push(1);
                    output.extend_from_slice(key.as_bytes());
                }
            }
        }
        Ok(())
    }
}

/// Old logical node eligible for pruning only after a committed version advances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaleNodeCandidate {
    /// Version that still owns the node.
    pub stale_since_version: u64,
    /// Logical leaf key.
    pub key: Hash32,
    /// Old leaf commitment.
    pub old_leaf_hash: Hash32,
}

/// Complete authenticated update plan bound to one semantic patch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedAuthenticatedCommit {
    profile: AuthenticatedProfile,
    expected_version: u64,
    next_version: u64,
    semantic_pre_root: Hash32,
    semantic_post_root: Hash32,
    patch_hash: Hash32,
    authenticated_pre_root: Hash32,
    authenticated_post_root: Hash32,
    node_batch: NodeBatch,
    stale_nodes: Box<[StaleNodeCandidate]>,
    post_leaves: BTreeMap<Hash32, Value>,
}

impl PlannedAuthenticatedCommit {
    /// Returns the profile binding.
    #[must_use]
    pub const fn profile(&self) -> AuthenticatedProfile {
        self.profile
    }

    /// Returns the expected tree version.
    #[must_use]
    pub const fn expected_version(&self) -> u64 {
        self.expected_version
    }

    /// Returns the successor tree version.
    #[must_use]
    pub const fn next_version(&self) -> u64 {
        self.next_version
    }

    /// Returns the semantic pre-root fixed by the patch.
    #[must_use]
    pub const fn semantic_pre_root(&self) -> Hash32 {
        self.semantic_pre_root
    }

    /// Returns the applied semantic post-root.
    #[must_use]
    pub const fn semantic_post_root(&self) -> Hash32 {
        self.semantic_post_root
    }

    /// Returns the canonical patch commitment.
    #[must_use]
    pub const fn patch_hash(&self) -> Hash32 {
        self.patch_hash
    }

    /// Returns the expected authenticated root.
    #[must_use]
    pub const fn authenticated_pre_root(&self) -> Hash32 {
        self.authenticated_pre_root
    }

    /// Returns the successor authenticated root.
    #[must_use]
    pub const fn authenticated_post_root(&self) -> Hash32 {
        self.authenticated_post_root
    }

    /// Returns the canonical logical changes.
    #[must_use]
    pub const fn node_batch(&self) -> &NodeBatch {
        &self.node_batch
    }

    /// Returns nodes that become stale after successful publication.
    #[must_use]
    pub const fn stale_nodes(&self) -> &[StaleNodeCandidate] {
        &self.stale_nodes
    }
}

impl CanonicalEncode for PlannedAuthenticatedCommit {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&AUTHENTICATED_PLAN_ENCODING_VERSION.to_be_bytes());
        output.extend_from_slice(self.profile.tree_id.as_bytes());
        output.extend_from_slice(self.profile.profile_hash.as_bytes());
        output.extend_from_slice(self.profile.projector_hash.as_bytes());
        output.extend_from_slice(&self.expected_version.to_be_bytes());
        output.extend_from_slice(&self.next_version.to_be_bytes());
        for hash in [
            self.semantic_pre_root,
            self.semantic_post_root,
            self.patch_hash,
            self.authenticated_pre_root,
            self.authenticated_post_root,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        put_blob(output, &self.node_batch.canonical_bytes()?)?;
        put_length(output, self.stale_nodes.len())?;
        for stale in &self.stale_nodes {
            output.extend_from_slice(&stale.stale_since_version.to_be_bytes());
            output.extend_from_slice(stale.key.as_bytes());
            output.extend_from_slice(stale.old_leaf_hash.as_bytes());
        }
        Ok(())
    }
}

/// Strictly decoded authenticated plan without reference-backend private state.
///
/// This value is inspectable transport data. It cannot be converted into
/// [`PlannedAuthenticatedCommit`] or applied to a tree. Production authority
/// must independently reconstruct the plan from the exact authorized candidate
/// and require byte equality.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedAuthenticatedPlan {
    profile: AuthenticatedProfile,
    expected_version: u64,
    next_version: u64,
    semantic_pre_root: Hash32,
    semantic_post_root: Hash32,
    patch_hash: Hash32,
    authenticated_pre_root: Hash32,
    authenticated_post_root: Hash32,
    node_batch: NodeBatch,
    stale_nodes: Box<[StaleNodeCandidate]>,
}

impl DecodedAuthenticatedPlan {
    /// Returns the exact profile binding.
    #[must_use]
    pub const fn profile(&self) -> AuthenticatedProfile {
        self.profile
    }

    /// Returns the expected tree version.
    #[must_use]
    pub const fn expected_version(&self) -> u64 {
        self.expected_version
    }

    /// Returns the successor tree version.
    #[must_use]
    pub const fn next_version(&self) -> u64 {
        self.next_version
    }

    /// Returns the semantic pre-state root.
    #[must_use]
    pub const fn semantic_pre_root(&self) -> Hash32 {
        self.semantic_pre_root
    }

    /// Returns the semantic post-state root.
    #[must_use]
    pub const fn semantic_post_root(&self) -> Hash32 {
        self.semantic_post_root
    }

    /// Returns the exact canonical patch commitment.
    #[must_use]
    pub const fn patch_hash(&self) -> Hash32 {
        self.patch_hash
    }

    /// Returns the expected authenticated root.
    #[must_use]
    pub const fn authenticated_pre_root(&self) -> Hash32 {
        self.authenticated_pre_root
    }

    /// Returns the successor authenticated root.
    #[must_use]
    pub const fn authenticated_post_root(&self) -> Hash32 {
        self.authenticated_post_root
    }

    /// Returns canonical logical leaf writes.
    #[must_use]
    pub const fn node_batch(&self) -> &NodeBatch {
        &self.node_batch
    }

    /// Returns canonical stale-node candidates.
    #[must_use]
    pub const fn stale_nodes(&self) -> &[StaleNodeCandidate] {
        &self.stale_nodes
    }
}

impl CanonicalEncode for DecodedAuthenticatedPlan {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&AUTHENTICATED_PLAN_ENCODING_VERSION.to_be_bytes());
        output.extend_from_slice(self.profile.tree_id.as_bytes());
        output.extend_from_slice(self.profile.profile_hash.as_bytes());
        output.extend_from_slice(self.profile.projector_hash.as_bytes());
        output.extend_from_slice(&self.expected_version.to_be_bytes());
        output.extend_from_slice(&self.next_version.to_be_bytes());
        for hash in [
            self.semantic_pre_root,
            self.semantic_post_root,
            self.patch_hash,
            self.authenticated_pre_root,
            self.authenticated_post_root,
        ] {
            output.extend_from_slice(hash.as_bytes());
        }
        put_blob(output, &self.node_batch.canonical_bytes()?)?;
        put_length(output, self.stale_nodes.len())?;
        for stale in &self.stale_nodes {
            output.extend_from_slice(&stale.stale_since_version.to_be_bytes());
            output.extend_from_slice(stale.key.as_bytes());
            output.extend_from_slice(stale.old_leaf_hash.as_bytes());
        }
        Ok(())
    }
}

/// Strictly decodes one canonical authenticated update plan.
pub fn decode_authenticated_plan(
    bytes: &[u8],
    limits: AuthenticatedDecodeLimits,
) -> Result<DecodedAuthenticatedPlan, AuthDecodeError> {
    enforce_authenticated_input_limit(bytes, limits)?;
    let mut cursor = AuthCursor::new(bytes);
    let format = cursor.take_u16()?;
    if format != AUTHENTICATED_PLAN_ENCODING_VERSION {
        return Err(AuthDecodeError::VersionMismatch {
            expected: AUTHENTICATED_PLAN_ENCODING_VERSION,
            actual: format,
        });
    }
    let profile = decode_profile(&mut cursor)?;
    let expected_version = cursor.take_u64()?;
    let next_version = cursor.take_u64()?;
    if expected_version.checked_add(1) != Some(next_version) {
        return Err(AuthDecodeError::NonSuccessorVersion);
    }
    let semantic_pre_root = cursor.take_hash32()?;
    let semantic_post_root = cursor.take_hash32()?;
    let patch_hash = cursor.take_hash32()?;
    let authenticated_pre_root = cursor.take_hash32()?;
    let authenticated_post_root = cursor.take_hash32()?;
    let node_batch = decode_node_batch(cursor.take_blob()?, limits)?;
    let stale_count = cursor.take_u32()?;
    if stale_count > limits.max_stale_nodes {
        return Err(AuthDecodeError::StaleNodeLimit {
            limit: limits.max_stale_nodes,
            actual: stale_count,
        });
    }
    let mut stale_nodes =
        Vec::with_capacity(bounded_capacity(stale_count, cursor.remaining(), 72)?);
    let mut previous_key = None;
    for _ in 0..stale_count {
        let stale_since_version = cursor.take_u64()?;
        if stale_since_version != expected_version {
            return Err(AuthDecodeError::StaleVersionMismatch);
        }
        let key = cursor.take_hash32()?;
        if previous_key.is_some_and(|previous| previous >= key) {
            return Err(AuthDecodeError::NonCanonicalStaleOrder);
        }
        previous_key = Some(key);
        if node_batch
            .writes()
            .binary_search_by_key(&key, LeafWrite::key)
            .is_err()
        {
            return Err(AuthDecodeError::StaleKeyNotWritten);
        }
        stale_nodes.push(StaleNodeCandidate {
            stale_since_version,
            key,
            old_leaf_hash: cursor.take_hash32()?,
        });
    }
    require_complete(&cursor)?;
    let decoded = DecodedAuthenticatedPlan {
        profile,
        expected_version,
        next_version,
        semantic_pre_root,
        semantic_post_root,
        patch_hash,
        authenticated_pre_root,
        authenticated_post_root,
        node_batch,
        stale_nodes: stale_nodes.into_boxed_slice(),
    };
    require_canonical(bytes, &decoded)?;
    Ok(decoded)
}

/// Result of planning both semantic and authenticated successor state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedState {
    semantic_post_state: Value,
    authenticated: PlannedAuthenticatedCommit,
}

impl PlannedState {
    /// Returns the applied immutable semantic state.
    #[must_use]
    pub const fn semantic_post_state(&self) -> &Value {
        &self.semantic_post_state
    }

    /// Returns the complete authenticated plan.
    #[must_use]
    pub const fn authenticated(&self) -> &PlannedAuthenticatedCommit {
        &self.authenticated
    }
}

/// Configured authenticated-state planner that owns one projector implementation.
///
/// Request-time callers supply states and patches but cannot substitute a
/// different projector. Projector correctness and completeness remain trusted
/// setup obligations unless independently proved.
pub struct AuthenticatedStatePlanner<P> {
    profile: AuthenticatedProfile,
    projector: P,
}

impl<P: StateProjector> AuthenticatedStatePlanner<P> {
    /// Mounts one concrete projector under the expected authenticated profile.
    pub fn try_new(profile: AuthenticatedProfile, projector: P) -> Result<Self, AuthError> {
        if projector.declared_projector_hash() != profile.projector_hash() {
            return Err(AuthError::ProjectorMismatch);
        }
        Ok(Self { profile, projector })
    }

    /// Returns the mounted authenticated profile.
    #[must_use]
    pub const fn profile(&self) -> AuthenticatedProfile {
        self.profile
    }

    /// Plans an authenticated update only after the semantic patch applies exactly.
    pub fn plan(
        &self,
        pre_state: &Value,
        state_domain: Domain<'_>,
        patch: &CanonicalPatch,
        tree: &ReferenceSparseTree,
    ) -> Result<PlannedState, AuthError> {
        if tree.profile != self.profile {
            return Err(AuthError::ProfileMismatch);
        }
        if self.projector.declared_projector_hash() != self.profile.projector_hash() {
            return Err(AuthError::ProjectorMismatch);
        }
        plan_authenticated_update(pre_state, state_domain, patch, tree, &self.projector)
    }
}

fn plan_authenticated_update<P: StateProjector>(
    pre_state: &Value,
    state_domain: Domain<'_>,
    patch: &CanonicalPatch,
    tree: &ReferenceSparseTree,
    projector: &P,
) -> Result<PlannedState, AuthError> {
    let before = projected_map(projector.project(pre_state)?)?;
    if before != tree.leaves {
        return Err(AuthError::ProjectionMismatch);
    }
    if patch.expected_pre_root() == Hash32::ZERO {
        return Err(AuthError::ZeroSemanticRoot);
    }
    let applied = patch
        .apply::<RustCryptoSha256>(pre_state, state_domain)
        .map_err(AuthError::Patch)?;
    let semantic_post_root = applied.post_root();
    let (post_state, _) = applied.into_parts();
    let after = projected_map(projector.project(&post_state)?)?;
    let next_version = tree
        .version
        .checked_add(1)
        .ok_or(AuthError::VersionOverflow)?;
    let rebuilt = ReferenceSparseTree::from_map(tree.profile, next_version, after.clone())?;
    let node_batch = NodeBatch::from_maps(&before, &after);
    let mut stale_nodes = Vec::new();
    for write in node_batch.writes() {
        if let Some(old) = before.get(&write.key()) {
            stale_nodes.push(StaleNodeCandidate {
                stale_since_version: tree.version,
                key: write.key(),
                old_leaf_hash: leaf_hash(write.key(), old)?,
            });
        }
    }
    let patch_hash = hash_canonical("zeno-fcis/auth-patch", patch)?;
    let plan = PlannedAuthenticatedCommit {
        profile: tree.profile,
        expected_version: tree.version,
        next_version,
        semantic_pre_root: patch.expected_pre_root(),
        semantic_post_root,
        patch_hash,
        authenticated_pre_root: tree.root,
        authenticated_post_root: rebuilt.root,
        node_batch,
        stale_nodes: stale_nodes.into_boxed_slice(),
        post_leaves: after,
    };
    Ok(PlannedState {
        semantic_post_state: post_state,
        authenticated: plan,
    })
}

/// Proof payload at one fixed-depth sparse-tree root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProofLeaf {
    /// Exact value present at the key.
    Membership(Value),
    /// Key is absent at this root.
    Absence,
}

/// Fixed-depth membership or absence proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparseProof {
    profile: AuthenticatedProfile,
    version: u64,
    root: Hash32,
    key: Hash32,
    leaf: ProofLeaf,
    siblings: Box<[Hash32]>,
}

impl CanonicalEncode for SparseProof {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&SPARSE_PROOF_ENCODING_VERSION.to_be_bytes());
        output.extend_from_slice(self.profile.tree_id.as_bytes());
        output.extend_from_slice(self.profile.profile_hash.as_bytes());
        output.extend_from_slice(self.profile.projector_hash.as_bytes());
        output.extend_from_slice(&self.version.to_be_bytes());
        output.extend_from_slice(self.root.as_bytes());
        output.extend_from_slice(self.key.as_bytes());
        match &self.leaf {
            ProofLeaf::Absence => output.push(0),
            ProofLeaf::Membership(value) => {
                output.push(1);
                put_blob(output, &value.canonical_bytes()?)?;
            }
        }
        put_length(output, self.siblings.len())?;
        for sibling in &self.siblings {
            output.extend_from_slice(sibling.as_bytes());
        }
        Ok(())
    }
}

/// Strictly decodes one canonical sparse membership or absence proof.
pub fn decode_sparse_proof(
    bytes: &[u8],
    limits: AuthenticatedDecodeLimits,
) -> Result<SparseProof, AuthDecodeError> {
    enforce_authenticated_input_limit(bytes, limits)?;
    let mut cursor = AuthCursor::new(bytes);
    let format = cursor.take_u16()?;
    if format != SPARSE_PROOF_ENCODING_VERSION {
        return Err(AuthDecodeError::VersionMismatch {
            expected: SPARSE_PROOF_ENCODING_VERSION,
            actual: format,
        });
    }
    let profile = decode_profile(&mut cursor)?;
    let version = cursor.take_u64()?;
    let root = cursor.take_hash32()?;
    let key = cursor.take_hash32()?;
    let leaf = match cursor.take_u8()? {
        0 => ProofLeaf::Absence,
        1 => ProofLeaf::Membership(decode_authenticated_value(cursor.take_blob()?, limits)?),
        tag => return Err(AuthDecodeError::UnknownProofLeafTag(tag)),
    };
    let sibling_count = cursor.take_u32()?;
    if sibling_count != TREE_DEPTH as u32 {
        return Err(AuthDecodeError::ProofLength {
            expected: TREE_DEPTH as u32,
            actual: sibling_count,
        });
    }
    let mut siblings = Vec::with_capacity(TREE_DEPTH);
    for _ in 0..sibling_count {
        siblings.push(cursor.take_hash32()?);
    }
    require_complete(&cursor)?;
    let proof = SparseProof {
        profile,
        version,
        root,
        key,
        leaf,
        siblings: siblings.into_boxed_slice(),
    };
    require_canonical(bytes, &proof)?;
    Ok(proof)
}

impl SparseProof {
    /// Returns the complete authenticated profile identity.
    #[must_use]
    pub const fn profile(&self) -> AuthenticatedProfile {
        self.profile
    }

    /// Returns the operational tree identity.
    #[must_use]
    pub const fn tree_id(&self) -> Hash32 {
        self.profile.tree_id()
    }

    /// Returns the dual-root profile identity.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.profile.profile_hash()
    }

    /// Returns the declared projector commitment.
    #[must_use]
    pub const fn projector_hash(&self) -> Hash32 {
        self.profile.projector_hash()
    }

    /// Returns the tree version.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the root embedded in the untrusted proof payload.
    #[must_use]
    pub const fn root(&self) -> Hash32 {
        self.root
    }

    /// Returns the key embedded in the untrusted proof payload.
    #[must_use]
    pub const fn key(&self) -> Hash32 {
        self.key
    }

    /// Returns the proof leaf.
    #[must_use]
    pub const fn leaf(&self) -> &ProofLeaf {
        &self.leaf
    }

    /// Recomputes the root without establishing an external trust anchor.
    ///
    /// This method proves only internal consistency. Authorization decisions
    /// should use [`Self::verify_against`], retain its nominal witness, and
    /// separately compare the witness context with authority-owned state.
    pub fn verify_internal_consistency(&self) -> Result<bool, AuthError> {
        Ok(self.recompute_root()? == self.root)
    }

    /// Verifies this proof against the exact supplied expected context.
    pub fn verify_against(
        &self,
        expected: SparseProofContext,
    ) -> Result<ContextVerifiedSparseProof, AuthError> {
        if self.profile != expected.profile {
            return Err(AuthError::ProofProfileMismatch);
        }
        if self.version != expected.version {
            return Err(AuthError::ProofVersionMismatch);
        }
        if self.root != expected.root {
            return Err(AuthError::ProofRootMismatch);
        }
        if self.key != expected.key {
            return Err(AuthError::ProofKeyMismatch);
        }
        if self.recompute_root()? != expected.root {
            return Err(AuthError::InvalidProof);
        }
        Ok(ContextVerifiedSparseProof {
            context: expected,
            leaf: self.leaf.clone(),
        })
    }

    fn recompute_root(&self) -> Result<Hash32, AuthError> {
        if self.siblings.len() != TREE_DEPTH {
            return Err(AuthError::ProofLength);
        }
        let empties = empty_hashes()?;
        let mut current = match &self.leaf {
            ProofLeaf::Membership(value) => leaf_hash(self.key, value)?,
            ProofLeaf::Absence => empties[TREE_DEPTH],
        };
        for depth in (0..TREE_DEPTH).rev() {
            current = if key_bit(self.key, depth) == 0 {
                node_hash(current, self.siblings[depth])?
            } else {
                node_hash(self.siblings[depth], current)?
            };
        }
        Ok(current)
    }
}

/// Caller-supplied expected context required to interpret a sparse proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SparseProofContext {
    profile: AuthenticatedProfile,
    version: u64,
    root: Hash32,
    key: Hash32,
}

impl SparseProofContext {
    /// Creates an exact proof-verification context.
    #[must_use]
    pub const fn new(
        profile: AuthenticatedProfile,
        version: u64,
        root: Hash32,
        key: Hash32,
    ) -> Self {
        Self {
            profile,
            version,
            root,
            key,
        }
    }

    /// Returns the complete authenticated profile identity.
    #[must_use]
    pub const fn profile(self) -> AuthenticatedProfile {
        self.profile
    }

    /// Returns the expected tree version.
    #[must_use]
    pub const fn version(self) -> u64 {
        self.version
    }

    /// Returns the expected authenticated root.
    #[must_use]
    pub const fn root(self) -> Hash32 {
        self.root
    }

    /// Returns the expected logical key.
    #[must_use]
    pub const fn key(self) -> Hash32 {
        self.key
    }
}

/// A sparse proof verified against one exact caller-supplied context.
///
/// This witness has no public constructor. Callers must obtain it through
/// [`SparseProof::verify_against`].
/// The type does not attest that the supplied context came from a production
/// authority; consumers must compare [`Self::context`] with authority-owned
/// state or policy.
///
/// ```compile_fail
/// use zeno_fcis_authenticated::{
///     ContextVerifiedSparseProof, ProofLeaf, SparseProofContext,
/// };
///
/// fn forge(
///     context: SparseProofContext,
///     leaf: ProofLeaf,
/// ) -> ContextVerifiedSparseProof {
///     ContextVerifiedSparseProof { context, leaf }
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextVerifiedSparseProof {
    context: SparseProofContext,
    leaf: ProofLeaf,
}

impl ContextVerifiedSparseProof {
    /// Returns the exact context against which the proof was verified.
    #[must_use]
    pub const fn context(&self) -> SparseProofContext {
        self.context
    }

    /// Returns the verified membership or absence result.
    #[must_use]
    pub const fn leaf(&self) -> &ProofLeaf {
        &self.leaf
    }
}

/// Inspectable fixed-depth sparse-tree reference backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceSparseTree {
    profile: AuthenticatedProfile,
    version: u64,
    root: Hash32,
    leaves: BTreeMap<Hash32, Value>,
}

impl ReferenceSparseTree {
    /// Builds a tree from unordered logical leaves and rejects duplicates.
    pub fn try_new(
        profile: AuthenticatedProfile,
        version: u64,
        leaves: Vec<(Hash32, Value)>,
    ) -> Result<Self, AuthError> {
        Self::from_map(profile, version, projected_map(leaves)?)
    }

    fn from_map(
        profile: AuthenticatedProfile,
        version: u64,
        leaves: BTreeMap<Hash32, Value>,
    ) -> Result<Self, AuthError> {
        if leaves.len() > MAX_REFERENCE_LEAVES {
            return Err(AuthError::TooManyLeaves);
        }
        let hashed = hashed_leaves(&leaves)?;
        let empties = empty_hashes()?;
        let root = subtree_hash(&hashed, 0, &empties)?;
        Ok(Self {
            profile,
            version,
            root,
            leaves,
        })
    }

    /// Returns all logical leaves in canonical key order.
    #[must_use]
    pub fn leaves(&self) -> impl ExactSizeIterator<Item = (&Hash32, &Value)> {
        self.leaves.iter()
    }
}

impl TreeReader for ReferenceSparseTree {
    fn profile(&self) -> AuthenticatedProfile {
        self.profile
    }

    fn version(&self) -> u64 {
        self.version
    }

    fn root(&self) -> Hash32 {
        self.root
    }

    fn get(&self, key: Hash32) -> Option<&Value> {
        self.leaves.get(&key)
    }

    fn prove(&self, key: Hash32) -> Result<SparseProof, AuthError> {
        let hashed = hashed_leaves(&self.leaves)?;
        let empties = empty_hashes()?;
        let mut siblings = Vec::with_capacity(TREE_DEPTH);
        proof_siblings(&hashed, key, 0, &empties, &mut siblings)?;
        Ok(SparseProof {
            profile: self.profile,
            version: self.version,
            root: self.root,
            key,
            leaf: self
                .leaves
                .get(&key)
                .cloned()
                .map_or(ProofLeaf::Absence, ProofLeaf::Membership),
            siblings: siblings.into_boxed_slice(),
        })
    }
}

impl TreeWriter for ReferenceSparseTree {
    fn apply_plan(&mut self, plan: &PlannedAuthenticatedCommit) -> Result<(), AuthError> {
        if self.profile != plan.profile {
            return Err(AuthError::ProfileMismatch);
        }
        if self.version != plan.expected_version {
            return Err(AuthError::VersionConflict);
        }
        if self.root != plan.authenticated_pre_root {
            return Err(AuthError::RootConflict);
        }
        let rebuilt = Self::from_map(plan.profile, plan.next_version, plan.post_leaves.clone())?;
        if rebuilt.root != plan.authenticated_post_root {
            return Err(AuthError::PostRootMismatch);
        }
        *self = rebuilt;
        Ok(())
    }
}

fn projected_map(leaves: Vec<(Hash32, Value)>) -> Result<BTreeMap<Hash32, Value>, AuthError> {
    if leaves.len() > MAX_REFERENCE_LEAVES {
        return Err(AuthError::TooManyLeaves);
    }
    let mut map = BTreeMap::new();
    for (key, value) in leaves {
        if map.insert(key, value).is_some() {
            return Err(AuthError::DuplicateLeafKey);
        }
    }
    Ok(map)
}

fn hashed_leaves(leaves: &BTreeMap<Hash32, Value>) -> Result<Vec<(Hash32, Hash32)>, AuthError> {
    leaves
        .iter()
        .map(|(key, value)| Ok((*key, leaf_hash(*key, value)?)))
        .collect()
}

fn leaf_hash(key: Hash32, value: &Value) -> Result<Hash32, AuthError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(key.as_bytes());
    put_blob(
        &mut bytes,
        &value.canonical_bytes().map_err(AuthError::Encode)?,
    )
    .map_err(AuthError::Encode)?;
    hash_bytes("zeno-fcis/auth-leaf", &bytes)
}

fn node_hash(left: Hash32, right: Hash32) -> Result<Hash32, AuthError> {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(left.as_bytes());
    bytes.extend_from_slice(right.as_bytes());
    hash_bytes("zeno-fcis/auth-node", &bytes)
}

fn empty_hashes() -> Result<[Hash32; TREE_DEPTH + 1], AuthError> {
    let mut hashes = [Hash32::ZERO; TREE_DEPTH + 1];
    hashes[TREE_DEPTH] = hash_bytes("zeno-fcis/auth-empty", b"leaf")?;
    for depth in (0..TREE_DEPTH).rev() {
        hashes[depth] = node_hash(hashes[depth + 1], hashes[depth + 1])?;
    }
    Ok(hashes)
}

fn subtree_hash(
    leaves: &[(Hash32, Hash32)],
    depth: usize,
    empties: &[Hash32; TREE_DEPTH + 1],
) -> Result<Hash32, AuthError> {
    if leaves.is_empty() {
        return Ok(empties[depth]);
    }
    if depth == TREE_DEPTH {
        return if leaves.len() == 1 {
            Ok(leaves[0].1)
        } else {
            Err(AuthError::DuplicateLeafKey)
        };
    }
    let split = leaves.partition_point(|(key, _)| key_bit(*key, depth) == 0);
    let left = subtree_hash(&leaves[..split], depth + 1, empties)?;
    let right = subtree_hash(&leaves[split..], depth + 1, empties)?;
    node_hash(left, right)
}

fn proof_siblings(
    leaves: &[(Hash32, Hash32)],
    key: Hash32,
    depth: usize,
    empties: &[Hash32; TREE_DEPTH + 1],
    siblings: &mut Vec<Hash32>,
) -> Result<(), AuthError> {
    if depth == TREE_DEPTH {
        return Ok(());
    }
    let split = leaves.partition_point(|(candidate, _)| key_bit(*candidate, depth) == 0);
    let (left, right) = leaves.split_at(split);
    if key_bit(key, depth) == 0 {
        siblings.push(subtree_hash(right, depth + 1, empties)?);
        proof_siblings(left, key, depth + 1, empties, siblings)
    } else {
        siblings.push(subtree_hash(left, depth + 1, empties)?);
        proof_siblings(right, key, depth + 1, empties, siblings)
    }
}

const fn key_bit(key: Hash32, depth: usize) -> u8 {
    let byte = key.as_bytes()[depth / 8];
    (byte >> (7 - (depth % 8))) & 1
}

fn hash_canonical(domain: &'static str, value: &impl CanonicalEncode) -> Result<Hash32, AuthError> {
    let bytes = value.canonical_bytes().map_err(AuthError::Encode)?;
    hash_bytes(domain, &bytes)
}

fn hash_bytes(domain: &'static str, bytes: &[u8]) -> Result<Hash32, AuthError> {
    let domain = Domain::new(domain, 1).map_err(AuthError::Encode)?;
    commitment::<RustCryptoSha256>(domain, bytes).map_err(AuthError::Encode)
}

fn decode_profile(cursor: &mut AuthCursor<'_>) -> Result<AuthenticatedProfile, AuthDecodeError> {
    AuthenticatedProfile::try_new(
        cursor.take_hash32()?,
        cursor.take_hash32()?,
        cursor.take_hash32()?,
    )
    .map_err(|_| AuthDecodeError::ZeroProfileIdentity)
}

fn decode_node_batch(
    bytes: &[u8],
    limits: AuthenticatedDecodeLimits,
) -> Result<NodeBatch, AuthDecodeError> {
    let mut cursor = AuthCursor::new(bytes);
    let count = cursor.take_u32()?;
    if count > limits.max_writes {
        return Err(AuthDecodeError::WriteLimit {
            limit: limits.max_writes,
            actual: count,
        });
    }
    let mut writes = Vec::with_capacity(bounded_capacity(count, cursor.remaining(), 33)?);
    let mut previous_key = None;
    for _ in 0..count {
        let tag = cursor.take_u8()?;
        let key = cursor.take_hash32()?;
        if previous_key.is_some_and(|previous| previous >= key) {
            return Err(AuthDecodeError::NonCanonicalWriteOrder);
        }
        previous_key = Some(key);
        writes.push(match tag {
            0 => LeafWrite::Put {
                key,
                value: decode_authenticated_value(cursor.take_blob()?, limits)?,
            },
            1 => LeafWrite::Delete { key },
            other => return Err(AuthDecodeError::UnknownWriteTag(other)),
        });
    }
    require_complete(&cursor)?;
    let batch = NodeBatch {
        writes: writes.into_boxed_slice(),
    };
    require_canonical(bytes, &batch)?;
    Ok(batch)
}

fn decode_authenticated_value(
    bytes: &[u8],
    limits: AuthenticatedDecodeLimits,
) -> Result<Value, AuthDecodeError> {
    decode_value(bytes, limits.value).map_err(AuthDecodeError::Value)
}

fn enforce_authenticated_input_limit(
    bytes: &[u8],
    limits: AuthenticatedDecodeLimits,
) -> Result<(), AuthDecodeError> {
    let actual = u64::try_from(bytes.len()).map_err(|_| AuthDecodeError::LengthOverflow)?;
    if actual > limits.max_input_bytes {
        return Err(AuthDecodeError::InputLimit {
            limit: limits.max_input_bytes,
            actual,
        });
    }
    Ok(())
}

fn bounded_capacity(
    count: u32,
    remaining_wire_bytes: usize,
    minimum_wire_bytes_per_item: usize,
) -> Result<usize, AuthDecodeError> {
    let count = usize::try_from(count).map_err(|_| AuthDecodeError::LengthOverflow)?;
    let wire_bound = remaining_wire_bytes
        .checked_div(minimum_wire_bytes_per_item)
        .ok_or(AuthDecodeError::LengthOverflow)?;
    Ok(count.min(wire_bound))
}

fn require_complete(cursor: &AuthCursor<'_>) -> Result<(), AuthDecodeError> {
    if cursor.remaining() == 0 {
        Ok(())
    } else {
        Err(AuthDecodeError::TrailingBytes {
            offset: cursor.offset,
        })
    }
}

fn require_canonical(bytes: &[u8], value: &impl CanonicalEncode) -> Result<(), AuthDecodeError> {
    let canonical = value.canonical_bytes().map_err(AuthDecodeError::Encode)?;
    if canonical.as_slice() == bytes {
        Ok(())
    } else {
        Err(AuthDecodeError::NonCanonical)
    }
}

struct AuthCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> AuthCursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    const fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], AuthDecodeError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(AuthDecodeError::LengthOverflow)?;
        let Some(bytes) = self.bytes.get(self.offset..end) else {
            return Err(AuthDecodeError::UnexpectedEnd {
                offset: self.offset,
                requested: count,
            });
        };
        self.offset = end;
        Ok(bytes)
    }

    fn take_u8(&mut self) -> Result<u8, AuthDecodeError> {
        Ok(self.take(1)?[0])
    }

    fn take_u16(&mut self) -> Result<u16, AuthDecodeError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn take_u32(&mut self) -> Result<u32, AuthDecodeError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn take_u64(&mut self) -> Result<u64, AuthDecodeError> {
        let bytes = self.take(8)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn take_hash32(&mut self) -> Result<Hash32, AuthDecodeError> {
        let bytes = self.take(32)?;
        let mut hash = [0_u8; 32];
        hash.copy_from_slice(bytes);
        Ok(Hash32::new(hash))
    }

    fn take_blob(&mut self) -> Result<&'a [u8], AuthDecodeError> {
        let length =
            usize::try_from(self.take_u32()?).map_err(|_| AuthDecodeError::LengthOverflow)?;
        self.take(length)
    }
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

/// Authenticated planning or proof failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthError {
    /// Profile or tree identity is the forbidden zero sentinel.
    ZeroIdentity,
    /// Semantic patch root is the forbidden zero sentinel.
    ZeroSemanticRoot,
    /// Projector returned a duplicate logical key.
    DuplicateLeafKey,
    /// Projector exceeded the fixed reference bound.
    TooManyLeaves,
    /// Projected pre-state differs from the mounted tree snapshot.
    ProjectionMismatch,
    /// Supplied projector identity differs from the authenticated profile.
    ProjectorMismatch,
    /// Tree profile does not match the plan.
    ProfileMismatch,
    /// Tree version is stale.
    VersionConflict,
    /// Tree root is stale.
    RootConflict,
    /// Rebuilt successor does not match the plan.
    PostRootMismatch,
    /// Version cannot advance.
    VersionOverflow,
    /// Sparse proof does not have exactly 256 siblings.
    ProofLength,
    /// Sparse proof profile, tree, or projector identity differs from the expected context.
    ProofProfileMismatch,
    /// Sparse proof version differs from the expected context.
    ProofVersionMismatch,
    /// Sparse proof root differs from the expected context.
    ProofRootMismatch,
    /// Sparse proof key differs from the expected context.
    ProofKeyMismatch,
    /// Sparse proof does not recompute the root supplied in the verification context.
    InvalidProof,
    /// Semantic patch application failed.
    Patch(PatchError),
    /// Canonical encoding or hashing failed.
    Encode(EncodeError),
}

/// Strict authenticated plan or proof decoding failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthDecodeError {
    /// Complete input exceeds the configured byte limit.
    InputLimit {
        /// Configured limit.
        limit: u64,
        /// Actual input length.
        actual: u64,
    },
    /// A plan declares too many logical writes.
    WriteLimit {
        /// Configured limit.
        limit: u32,
        /// Declared count.
        actual: u32,
    },
    /// A plan declares too many stale-node candidates.
    StaleNodeLimit {
        /// Configured limit.
        limit: u32,
        /// Declared count.
        actual: u32,
    },
    /// A length conversion or offset addition overflowed.
    LengthOverflow,
    /// Input ended before a declared field was complete.
    UnexpectedEnd {
        /// Failed byte offset.
        offset: usize,
        /// Requested byte count.
        requested: usize,
    },
    /// Bytes remain after the complete canonical value.
    TrailingBytes {
        /// First trailing byte offset.
        offset: usize,
    },
    /// The canonical format version differs.
    VersionMismatch {
        /// Expected version.
        expected: u16,
        /// Decoded version.
        actual: u16,
    },
    /// One profile identity is the forbidden zero sentinel.
    ZeroProfileIdentity,
    /// The next version is not exactly the expected version plus one.
    NonSuccessorVersion,
    /// A stale candidate does not belong to the expected version.
    StaleVersionMismatch,
    /// A logical write tag is unknown.
    UnknownWriteTag(u8),
    /// A proof leaf tag is unknown.
    UnknownProofLeafTag(u8),
    /// Logical writes are not in unique increasing key order.
    NonCanonicalWriteOrder,
    /// Stale candidates are not in unique increasing key order.
    NonCanonicalStaleOrder,
    /// A stale candidate does not correspond to a logical write in the plan.
    StaleKeyNotWritten,
    /// A sparse proof has the wrong fixed sibling count.
    ProofLength {
        /// Required sibling count.
        expected: u32,
        /// Decoded sibling count.
        actual: u32,
    },
    /// A nested canonical value failed decoding.
    Value(DecodeError),
    /// Canonical reconstruction failed.
    Encode(EncodeError),
    /// Reconstructed canonical bytes differ from the complete input.
    NonCanonical,
}

impl fmt::Display for AuthDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputLimit { limit, actual } => {
                write!(
                    formatter,
                    "authenticated input bytes {actual} exceeds limit {limit}"
                )
            }
            Self::WriteLimit { limit, actual } => {
                write!(
                    formatter,
                    "authenticated writes {actual} exceeds limit {limit}"
                )
            }
            Self::StaleNodeLimit { limit, actual } => write!(
                formatter,
                "authenticated stale nodes {actual} exceeds limit {limit}"
            ),
            Self::LengthOverflow => formatter.write_str("authenticated decode length overflow"),
            Self::UnexpectedEnd { offset, requested } => write!(
                formatter,
                "authenticated input ended at offset {offset} before {requested} bytes"
            ),
            Self::TrailingBytes { offset } => {
                write!(formatter, "trailing authenticated bytes at offset {offset}")
            }
            Self::VersionMismatch { expected, actual } => write!(
                formatter,
                "authenticated format version differs: expected {expected}, got {actual}"
            ),
            Self::ZeroProfileIdentity => {
                formatter.write_str("authenticated profile contains a zero identity")
            }
            Self::NonSuccessorVersion => {
                formatter.write_str("authenticated plan version does not advance exactly once")
            }
            Self::StaleVersionMismatch => {
                formatter.write_str("stale candidate version differs from expected version")
            }
            Self::UnknownWriteTag(tag) => write!(formatter, "unknown leaf-write tag {tag}"),
            Self::UnknownProofLeafTag(tag) => write!(formatter, "unknown proof-leaf tag {tag}"),
            Self::NonCanonicalWriteOrder => {
                formatter.write_str("leaf writes are not in unique canonical order")
            }
            Self::NonCanonicalStaleOrder => {
                formatter.write_str("stale candidates are not in unique canonical order")
            }
            Self::StaleKeyNotWritten => {
                formatter.write_str("stale candidate key is absent from the logical writes")
            }
            Self::ProofLength { expected, actual } => write!(
                formatter,
                "sparse proof siblings differ: expected {expected}, got {actual}"
            ),
            Self::Value(error) => error.fmt(formatter),
            Self::Encode(error) => error.fmt(formatter),
            Self::NonCanonical => formatter.write_str("noncanonical authenticated encoding"),
        }
    }
}

impl std::error::Error for AuthDecodeError {}

impl fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroIdentity => formatter.write_str("authenticated profile identity is zero"),
            Self::ZeroSemanticRoot => formatter.write_str("semantic pre-root is zero"),
            Self::DuplicateLeafKey => formatter.write_str("duplicate projected leaf key"),
            Self::TooManyLeaves => formatter.write_str("reference leaf bound exceeded"),
            Self::ProjectionMismatch => formatter.write_str("projected state does not match tree"),
            Self::ProjectorMismatch => formatter.write_str("authenticated projector mismatch"),
            Self::ProfileMismatch => formatter.write_str("authenticated profile mismatch"),
            Self::VersionConflict => formatter.write_str("authenticated version conflict"),
            Self::RootConflict => formatter.write_str("authenticated root conflict"),
            Self::PostRootMismatch => formatter.write_str("authenticated post-root mismatch"),
            Self::VersionOverflow => formatter.write_str("authenticated version overflow"),
            Self::ProofLength => formatter.write_str("sparse proof must contain 256 siblings"),
            Self::ProofProfileMismatch => {
                formatter.write_str("sparse proof profile context mismatch")
            }
            Self::ProofVersionMismatch => {
                formatter.write_str("sparse proof version context mismatch")
            }
            Self::ProofRootMismatch => formatter.write_str("sparse proof root context mismatch"),
            Self::ProofKeyMismatch => formatter.write_str("sparse proof key context mismatch"),
            Self::InvalidProof => {
                formatter.write_str("sparse proof does not match the expected root")
            }
            Self::Patch(error) => write!(formatter, "semantic patch failed: {error}"),
            Self::Encode(error) => write!(formatter, "authenticated encoding failed: {error}"),
        }
    }
}

impl std::error::Error for AuthError {}

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_codec::CommitmentHasher;
    use zeno_fcis_patch::{PatchOp, ValuePath, hash_value};

    struct RootProjector;

    struct WrongIdentityProjector;

    struct SameIdentityWrongSemanticsProjector;

    impl StateProjector for RootProjector {
        fn declared_projector_hash(&self) -> Hash32 {
            hash(6)
        }

        fn project(&self, state: &Value) -> Result<Vec<(Hash32, Value)>, AuthError> {
            Ok(vec![(hash(9), state.clone())])
        }
    }

    impl StateProjector for WrongIdentityProjector {
        fn declared_projector_hash(&self) -> Hash32 {
            hash(7)
        }

        fn project(&self, state: &Value) -> Result<Vec<(Hash32, Value)>, AuthError> {
            RootProjector.project(state)
        }
    }

    impl StateProjector for SameIdentityWrongSemanticsProjector {
        fn declared_projector_hash(&self) -> Hash32 {
            hash(6)
        }

        fn project(&self, _state: &Value) -> Result<Vec<(Hash32, Value)>, AuthError> {
            Ok(Vec::new())
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn profile() -> AuthenticatedProfile {
        AuthenticatedProfile::try_new(hash(1), hash(2), hash(6))
            .unwrap_or_else(|error| panic!("profile: {error}"))
    }

    fn planner() -> AuthenticatedStatePlanner<RootProjector> {
        AuthenticatedStatePlanner::try_new(profile(), RootProjector)
            .unwrap_or_else(|error| panic!("planner: {error}"))
    }

    #[test]
    fn profile_rejects_zero_projector_identity() {
        assert_eq!(
            AuthenticatedProfile::try_new(hash(1), hash(2), Hash32::ZERO),
            Err(AuthError::ZeroIdentity)
        );
    }

    fn domain() -> Domain<'static> {
        Domain::new("test/state", 1).unwrap_or_else(|error| panic!("domain: {error}"))
    }

    #[test]
    fn insertion_history_does_not_change_root() {
        let left = ReferenceSparseTree::try_new(
            profile(),
            0,
            vec![(hash(3), Value::U128(3)), (hash(4), Value::U128(4))],
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        let right = ReferenceSparseTree::try_new(
            profile(),
            0,
            vec![(hash(4), Value::U128(4)), (hash(3), Value::U128(3))],
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        assert_eq!(left.root(), right.root());
    }

    #[test]
    fn membership_and_absence_proofs_verify_against_exact_context() {
        let tree = ReferenceSparseTree::try_new(
            profile(),
            7,
            vec![(hash(3), Value::U128(3)), (hash(4), Value::U128(4))],
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        let member = tree
            .prove(hash(3))
            .unwrap_or_else(|error| panic!("proof: {error}"));
        let absent = tree
            .prove(hash(5))
            .unwrap_or_else(|error| panic!("proof: {error}"));
        let member_context =
            SparseProofContext::new(tree.profile(), tree.version(), tree.root(), hash(3));
        let absent_context =
            SparseProofContext::new(tree.profile(), tree.version(), tree.root(), hash(5));
        let verified_member = member
            .verify_against(member_context)
            .unwrap_or_else(|error| panic!("verify: {error}"));
        let verified_absent = absent
            .verify_against(absent_context)
            .unwrap_or_else(|error| panic!("verify: {error}"));
        assert_eq!(verified_member.context(), member_context);
        assert!(matches!(
            verified_member.leaf(),
            ProofLeaf::Membership(Value::U128(3))
        ));
        assert!(matches!(verified_absent.leaf(), ProofLeaf::Absence));
    }

    #[test]
    fn proof_context_substitution_fails_closed() {
        let tree = ReferenceSparseTree::try_new(
            profile(),
            7,
            vec![(hash(3), Value::U128(3)), (hash(4), Value::U128(4))],
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        let proof = tree
            .prove(hash(3))
            .unwrap_or_else(|error| panic!("proof: {error}"));
        let exact = SparseProofContext::new(tree.profile(), tree.version(), tree.root(), hash(3));
        let wrong_tree = AuthenticatedProfile::try_new(hash(8), hash(2), hash(6))
            .unwrap_or_else(|error| panic!("profile: {error}"));
        let wrong_profile = AuthenticatedProfile::try_new(hash(1), hash(8), hash(6))
            .unwrap_or_else(|error| panic!("profile: {error}"));
        let wrong_projector = AuthenticatedProfile::try_new(hash(1), hash(2), hash(8))
            .unwrap_or_else(|error| panic!("profile: {error}"));

        for profile in [wrong_tree, wrong_profile, wrong_projector] {
            assert_eq!(
                proof.verify_against(SparseProofContext::new(
                    profile,
                    exact.version(),
                    exact.root(),
                    exact.key(),
                )),
                Err(AuthError::ProofProfileMismatch)
            );
        }
        assert_eq!(
            proof.verify_against(SparseProofContext::new(
                exact.profile(),
                exact.version() + 1,
                exact.root(),
                exact.key(),
            )),
            Err(AuthError::ProofVersionMismatch)
        );
        assert_eq!(
            proof.verify_against(SparseProofContext::new(
                exact.profile(),
                exact.version(),
                hash(8),
                exact.key(),
            )),
            Err(AuthError::ProofRootMismatch)
        );
        assert_eq!(
            proof.verify_against(SparseProofContext::new(
                exact.profile(),
                exact.version(),
                exact.root(),
                hash(8),
            )),
            Err(AuthError::ProofKeyMismatch)
        );
    }

    #[test]
    fn mutated_proof_fails_against_external_root() {
        let tree = ReferenceSparseTree::try_new(
            profile(),
            7,
            vec![(hash(3), Value::U128(3)), (hash(4), Value::U128(4))],
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        let proof = tree
            .prove(hash(3))
            .unwrap_or_else(|error| panic!("proof: {error}"));
        let context = SparseProofContext::new(tree.profile(), tree.version(), tree.root(), hash(3));

        let mut wrong_leaf = proof.clone();
        wrong_leaf.leaf = ProofLeaf::Membership(Value::U128(9));
        assert_eq!(
            wrong_leaf.verify_against(context),
            Err(AuthError::InvalidProof)
        );

        let mut wrong_sibling = proof;
        wrong_sibling.siblings[0] = hash(9);
        assert_eq!(
            wrong_sibling.verify_against(context),
            Err(AuthError::InvalidProof)
        );
    }

    #[test]
    fn sparse_proofs_strictly_round_trip_and_reject_transport_mutations() {
        let tree = ReferenceSparseTree::try_new(
            profile(),
            7,
            vec![(hash(3), Value::U128(3)), (hash(4), Value::U128(4))],
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        let proof = tree
            .prove(hash(3))
            .unwrap_or_else(|error| panic!("proof: {error}"));
        let bytes = proof
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("proof bytes: {error}"));
        let decoded = decode_sparse_proof(&bytes, AuthenticatedDecodeLimits::default())
            .unwrap_or_else(|error| panic!("decode proof: {error}"));
        assert_eq!(decoded, proof);
        assert!(
            decoded
                .verify_against(SparseProofContext::new(
                    tree.profile(),
                    tree.version(),
                    tree.root(),
                    hash(3),
                ))
                .is_ok()
        );

        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            decode_sparse_proof(&trailing, AuthenticatedDecodeLimits::default()),
            Err(AuthDecodeError::TrailingBytes { .. })
        ));

        let mut unknown_leaf = bytes.clone();
        // version + profile + tree version + root + key
        unknown_leaf[2 + 96 + 8 + 32 + 32] = 2;
        assert_eq!(
            decode_sparse_proof(&unknown_leaf, AuthenticatedDecodeLimits::default()),
            Err(AuthDecodeError::UnknownProofLeafTag(2))
        );

        let short_limit = AuthenticatedDecodeLimits {
            max_input_bytes: u64::try_from(bytes.len() - 1)
                .unwrap_or_else(|error| panic!("input length: {error}")),
            ..AuthenticatedDecodeLimits::default()
        };
        assert!(matches!(
            decode_sparse_proof(&bytes, short_limit),
            Err(AuthDecodeError::InputLimit { .. })
        ));
    }

    #[test]
    fn planned_update_matches_full_rebuild() {
        let pre = Value::U128(7);
        let pre_root = hash_value::<RustCryptoSha256>(domain(), &pre)
            .unwrap_or_else(|error| panic!("pre root: {error}"));
        let old_hash = hash_value::<RustCryptoSha256>(
            Domain::new("zeno-fcis/value", 1).unwrap_or_else(|error| panic!("domain: {error}")),
            &pre,
        )
        .unwrap_or_else(|error| panic!("old hash: {error}"));
        let patch = CanonicalPatch::try_new(
            1,
            pre_root,
            vec![PatchOp::Update {
                path: ValuePath::new(Vec::new()),
                expected_old_hash: old_hash,
                value: Value::U128(8),
            }],
        )
        .unwrap_or_else(|error| panic!("patch: {error}"));
        let mut tree = ReferenceSparseTree::try_new(
            profile(),
            0,
            RootProjector
                .project(&pre)
                .unwrap_or_else(|error| panic!("project: {error}")),
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        let planned = planner()
            .plan(&pre, domain(), &patch, &tree)
            .unwrap_or_else(|error| panic!("plan: {error}"));
        let expected = ReferenceSparseTree::try_new(
            profile(),
            1,
            RootProjector
                .project(planned.semantic_post_state())
                .unwrap_or_else(|error| panic!("project: {error}")),
        )
        .unwrap_or_else(|error| panic!("rebuild: {error}"));
        tree.apply_plan(planned.authenticated())
            .unwrap_or_else(|error| panic!("apply: {error}"));
        assert_eq!(tree.root(), expected.root());
        assert_eq!(tree.version(), 1);
        assert_eq!(planned.authenticated().node_batch().writes().len(), 1);
    }

    #[test]
    fn authenticated_plans_strictly_round_trip_as_non_authoritative_transport() {
        let pre = Value::U128(7);
        let pre_root = hash_value::<RustCryptoSha256>(domain(), &pre)
            .unwrap_or_else(|error| panic!("pre root: {error}"));
        let old_hash = hash_value::<RustCryptoSha256>(
            Domain::new("zeno-fcis/value", 1).unwrap_or_else(|error| panic!("domain: {error}")),
            &pre,
        )
        .unwrap_or_else(|error| panic!("old hash: {error}"));
        let patch = CanonicalPatch::try_new(
            1,
            pre_root,
            vec![PatchOp::Update {
                path: ValuePath::new(Vec::new()),
                expected_old_hash: old_hash,
                value: Value::U128(8),
            }],
        )
        .unwrap_or_else(|error| panic!("patch: {error}"));
        let tree = ReferenceSparseTree::try_new(
            profile(),
            0,
            RootProjector
                .project(&pre)
                .unwrap_or_else(|error| panic!("project: {error}")),
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        let planned = planner()
            .plan(&pre, domain(), &patch, &tree)
            .unwrap_or_else(|error| panic!("plan: {error}"));
        let bytes = planned
            .authenticated()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("plan bytes: {error}"));
        let decoded = decode_authenticated_plan(&bytes, AuthenticatedDecodeLimits::default())
            .unwrap_or_else(|error| panic!("decode plan: {error}"));
        assert_eq!(decoded.profile(), planned.authenticated().profile());
        assert_eq!(
            decoded.authenticated_post_root(),
            planned.authenticated().authenticated_post_root()
        );
        assert_eq!(
            decoded
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("decoded bytes: {error}")),
            bytes
        );

        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            decode_authenticated_plan(&trailing, AuthenticatedDecodeLimits::default()),
            Err(AuthDecodeError::TrailingBytes { .. })
        ));

        let mut wrong_version = bytes.clone();
        wrong_version[2 + 96 + 8..2 + 96 + 16].copy_from_slice(&2_u64.to_be_bytes());
        assert_eq!(
            decode_authenticated_plan(&wrong_version, AuthenticatedDecodeLimits::default()),
            Err(AuthDecodeError::NonSuccessorVersion)
        );

        let no_writes = AuthenticatedDecodeLimits {
            max_writes: 0,
            ..AuthenticatedDecodeLimits::default()
        };
        assert_eq!(
            decode_authenticated_plan(&bytes, no_writes),
            Err(AuthDecodeError::WriteLimit {
                limit: 0,
                actual: 1,
            })
        );
    }

    #[test]
    fn authenticated_collection_reservation_is_wire_bounded() {
        assert_eq!(bounded_capacity(4_096, 0, 33), Ok(0));
        assert_eq!(bounded_capacity(4_096, 65, 33), Ok(1));
        assert_eq!(bounded_capacity(2, 100, 33), Ok(2));
        assert_eq!(
            bounded_capacity(1, 1, 0),
            Err(AuthDecodeError::LengthOverflow)
        );
    }

    #[test]
    fn planning_rejects_projector_identity_substitution() {
        assert!(matches!(
            AuthenticatedStatePlanner::try_new(profile(), WrongIdentityProjector),
            Err(AuthError::ProjectorMismatch)
        ));
    }

    #[test]
    fn matching_declared_identity_is_not_semantic_attestation() {
        assert!(
            AuthenticatedStatePlanner::try_new(profile(), SameIdentityWrongSemanticsProjector)
                .is_ok()
        );
    }

    #[test]
    fn authenticated_plan_v2_binds_projector_commitment() {
        let plan = PlannedAuthenticatedCommit {
            profile: profile(),
            expected_version: 0,
            next_version: 1,
            semantic_pre_root: hash(3),
            semantic_post_root: hash(4),
            patch_hash: hash(5),
            authenticated_pre_root: hash(6),
            authenticated_post_root: hash(7),
            node_batch: NodeBatch {
                writes: Box::new([]),
            },
            stale_nodes: Box::new([]),
            post_leaves: BTreeMap::new(),
        };
        let bytes = plan
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("encode: {error}"));
        let version_bytes = AUTHENTICATED_PLAN_ENCODING_VERSION.to_be_bytes();

        assert_eq!(bytes.get(..2), Some(version_bytes.as_slice()));
        assert_eq!(bytes.len(), 286);
        assert_eq!(
            RustCryptoSha256::hash(&bytes),
            Hash32::new([
                50, 213, 59, 177, 111, 205, 1, 42, 66, 131, 9, 224, 145, 110, 142, 46, 84, 195, 52,
                193, 94, 38, 13, 199, 172, 57, 33, 157, 237, 18, 133, 175,
            ])
        );

        for changed_profile in [
            AuthenticatedProfile::try_new(hash(8), hash(2), hash(6)),
            AuthenticatedProfile::try_new(hash(1), hash(8), hash(6)),
            AuthenticatedProfile::try_new(hash(1), hash(2), hash(8)),
        ] {
            let mut other = plan.clone();
            other.profile = changed_profile.unwrap_or_else(|error| panic!("profile: {error}"));
            let other_bytes = other
                .canonical_bytes()
                .unwrap_or_else(|error| panic!("encode: {error}"));
            assert_ne!(bytes, other_bytes);
        }

        let mut populated = plan;
        populated.node_batch = NodeBatch {
            writes: vec![
                LeafWrite::Put {
                    key: hash(8),
                    value: Value::U128(9),
                },
                LeafWrite::Delete { key: hash(9) },
            ]
            .into_boxed_slice(),
        };
        populated.stale_nodes = vec![StaleNodeCandidate {
            stale_since_version: 10,
            key: hash(11),
            old_leaf_hash: hash(12),
        }]
        .into_boxed_slice();
        let populated_bytes = populated
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("encode populated plan: {error}"));
        assert_eq!(populated_bytes.len(), 445);
        assert_eq!(
            RustCryptoSha256::hash(&populated_bytes),
            Hash32::new([
                30, 144, 42, 116, 117, 242, 134, 23, 106, 168, 174, 98, 128, 173, 53, 65, 116, 231,
                186, 240, 114, 190, 160, 226, 124, 10, 155, 25, 90, 39, 136, 143,
            ])
        );
    }

    #[test]
    fn stale_version_rejects_without_mutation() {
        let mut tree = ReferenceSparseTree::try_new(profile(), 1, vec![])
            .unwrap_or_else(|error| panic!("tree: {error}"));
        let before = tree.clone();
        let plan = PlannedAuthenticatedCommit {
            profile: profile(),
            expected_version: 0,
            next_version: 1,
            semantic_pre_root: hash(3),
            semantic_post_root: hash(4),
            patch_hash: hash(5),
            authenticated_pre_root: tree.root(),
            authenticated_post_root: tree.root(),
            node_batch: NodeBatch {
                writes: Box::new([]),
            },
            stale_nodes: Box::new([]),
            post_leaves: BTreeMap::new(),
        };
        assert_eq!(tree.apply_plan(&plan), Err(AuthError::VersionConflict));
        assert_eq!(tree, before);
    }

    #[test]
    fn planning_at_maximum_version_returns_overflow() {
        let pre = Value::U128(7);
        let pre_root = hash_value::<RustCryptoSha256>(domain(), &pre)
            .unwrap_or_else(|error| panic!("pre root: {error}"));
        let old_hash = hash_value::<RustCryptoSha256>(
            Domain::new("zeno-fcis/value", 1).unwrap_or_else(|error| panic!("domain: {error}")),
            &pre,
        )
        .unwrap_or_else(|error| panic!("old hash: {error}"));
        let patch = CanonicalPatch::try_new(
            1,
            pre_root,
            vec![PatchOp::Update {
                path: ValuePath::new(Vec::new()),
                expected_old_hash: old_hash,
                value: Value::U128(8),
            }],
        )
        .unwrap_or_else(|error| panic!("patch: {error}"));
        let tree = ReferenceSparseTree::try_new(
            profile(),
            u64::MAX,
            RootProjector
                .project(&pre)
                .unwrap_or_else(|error| panic!("project: {error}")),
        )
        .unwrap_or_else(|error| panic!("tree: {error}"));
        assert_eq!(
            planner().plan(&pre, domain(), &patch, &tree),
            Err(AuthError::VersionOverflow)
        );
    }
}
