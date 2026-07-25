//! Versioned sparse authenticated-state planning behind canonical semantic patches.
//!
//! The semantic ZCVE state and its root remain authoritative. This crate adds
//! an explicitly separate authenticated index root and never silently replaces
//! a profile's existing state-root definition.

#![forbid(unsafe_code)]

use core::fmt;
use std::collections::BTreeMap;

use zeno_fcis_codec::{CanonicalEncode, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_patch::{CanonicalPatch, PatchError};
use zeno_fcis_value::Value;

/// Maximum logical leaves in the inspectable reference backend.
pub const MAX_REFERENCE_LEAVES: usize = 4_096;
const TREE_DEPTH: usize = 256;

/// Explicit dual-root authenticated profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedProfile {
    tree_id: Hash32,
    profile_hash: Hash32,
}

impl AuthenticatedProfile {
    /// Creates a profile with explicit tree and activation identities.
    pub fn try_new(tree_id: Hash32, profile_hash: Hash32) -> Result<Self, AuthError> {
        if tree_id == Hash32::ZERO || profile_hash == Hash32::ZERO {
            return Err(AuthError::ZeroIdentity);
        }
        Ok(Self {
            tree_id,
            profile_hash,
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
}

/// Reviewed total projection from semantic state to authenticated logical leaves.
pub trait StateProjector {
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
        output.extend_from_slice(self.profile.tree_id.as_bytes());
        output.extend_from_slice(self.profile.profile_hash.as_bytes());
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

/// Plans an authenticated update only after the semantic patch applies exactly.
pub fn plan_authenticated_update<P: StateProjector>(
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
    let rebuilt = ReferenceSparseTree::from_map(tree.profile, tree.version + 1, after.clone())?;
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
        next_version: tree
            .version
            .checked_add(1)
            .ok_or(AuthError::VersionOverflow)?,
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
    profile_hash: Hash32,
    version: u64,
    root: Hash32,
    key: Hash32,
    leaf: ProofLeaf,
    siblings: Box<[Hash32]>,
}

impl SparseProof {
    /// Returns the dual-root profile identity.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the tree version.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns whether the proof recomputes its bound root.
    pub fn verify(&self) -> Result<bool, AuthError> {
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
        Ok(current == self.root)
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
            profile_hash: self.profile.profile_hash,
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
    /// Semantic patch application failed.
    Patch(PatchError),
    /// Canonical encoding or hashing failed.
    Encode(EncodeError),
}

impl fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroIdentity => formatter.write_str("authenticated profile identity is zero"),
            Self::ZeroSemanticRoot => formatter.write_str("semantic pre-root is zero"),
            Self::DuplicateLeafKey => formatter.write_str("duplicate projected leaf key"),
            Self::TooManyLeaves => formatter.write_str("reference leaf bound exceeded"),
            Self::ProjectionMismatch => formatter.write_str("projected state does not match tree"),
            Self::ProfileMismatch => formatter.write_str("authenticated profile mismatch"),
            Self::VersionConflict => formatter.write_str("authenticated version conflict"),
            Self::RootConflict => formatter.write_str("authenticated root conflict"),
            Self::PostRootMismatch => formatter.write_str("authenticated post-root mismatch"),
            Self::VersionOverflow => formatter.write_str("authenticated version overflow"),
            Self::ProofLength => formatter.write_str("sparse proof must contain 256 siblings"),
            Self::Patch(error) => write!(formatter, "semantic patch failed: {error}"),
            Self::Encode(error) => write!(formatter, "authenticated encoding failed: {error}"),
        }
    }
}

impl std::error::Error for AuthError {}

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_patch::{PatchOp, ValuePath, hash_value};

    struct RootProjector;

    impl StateProjector for RootProjector {
        fn project(&self, state: &Value) -> Result<Vec<(Hash32, Value)>, AuthError> {
            Ok(vec![(hash(9), state.clone())])
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn profile() -> AuthenticatedProfile {
        AuthenticatedProfile::try_new(hash(1), hash(2))
            .unwrap_or_else(|error| panic!("profile: {error}"))
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
    fn membership_and_absence_proofs_verify() {
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
        assert!(
            member
                .verify()
                .unwrap_or_else(|error| panic!("verify: {error}"))
        );
        assert!(
            absent
                .verify()
                .unwrap_or_else(|error| panic!("verify: {error}"))
        );
        assert!(matches!(absent.leaf, ProofLeaf::Absence));
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
        let planned = plan_authenticated_update(&pre, domain(), &patch, &tree, &RootProjector)
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
}
