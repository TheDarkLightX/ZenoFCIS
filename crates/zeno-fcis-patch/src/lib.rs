//! Preconditioned canonical patches over the ZenoFCIS reference value algebra.
//!
//! A patch binds its expected pre-root, orders operations by canonical path
//! bytes, rejects duplicate or overlapping paths, and checks each old value
//! before constructing a successor. Application is pure and all-or-nothing.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{
    CanonicalEncode, CommitmentHasher, DecodeError, DecodeLimits, Domain, EncodeError, Hash32,
    commitment, decode_value,
};
use zeno_fcis_value::{Field, MapEntry, Value, ValueError};

const PATH_TAG_FIELD: u8 = 0;
const PATH_TAG_TUPLE: u8 = 1;
const PATH_TAG_VECTOR: u8 = 2;
const PATH_TAG_SUM_PAYLOAD: u8 = 3;
const PATH_TAG_MAP_KEY: u8 = 4;
const PATCH_TAG_INSERT: u8 = 0;
const PATCH_TAG_UPDATE: u8 = 1;
const PATCH_TAG_DELETE: u8 = 2;
const PATCH_MERGE_TAG_STATE_TYPE: u8 = 0;
const PATCH_MERGE_TAG_PRE_ROOT: u8 = 1;
const PATCH_MERGE_TAG_OPERATION: u8 = 2;

/// Explicit resource bounds for strict canonical patch decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatchDecodeLimits {
    /// Maximum bytes in the complete encoded patch.
    pub max_input_bytes: u64,
    /// Maximum operations in the patch.
    pub max_operations: u32,
    /// Maximum segments in any one value path.
    pub max_path_segments: u32,
    /// Maximum bytes in one encoded map-key path segment.
    pub max_map_key_bytes: u64,
    /// Maximum aggregate value nodes decoded across payloads and map keys.
    pub max_value_nodes: u64,
    /// Maximum aggregate byte and text payload bytes decoded across payloads and map keys.
    pub max_value_payload_bytes: u64,
    /// Per-value ZCVE decoding limits.
    pub value: DecodeLimits,
}

impl Default for PatchDecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: DecodeLimits::DEFAULT_MAX_INPUT_BYTES,
            max_operations: 4_096,
            max_path_segments: 256,
            max_map_key_bytes: 65_536,
            max_value_nodes: 1_000_000,
            max_value_payload_bytes: 64 * 1024 * 1024,
            value: DecodeLimits::default(),
        }
    }
}

/// One stable navigation step inside a closed value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PathSegment {
    /// Stable record field identifier.
    Field(u16),
    /// Fixed tuple position.
    TupleIndex(u32),
    /// Vector position.
    VectorIndex(u32),
    /// Payload of the current sum variant.
    SumPayload,
    /// Canonical encoded map key.
    MapKey(Box<[u8]>),
}

/// An immutable nested value path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ValuePath {
    segments: Box<[PathSegment]>,
}

impl ValuePath {
    /// Creates a path. The empty path addresses the complete state value.
    #[must_use]
    pub fn new(segments: Vec<PathSegment>) -> Self {
        Self {
            segments: segments.into_boxed_slice(),
        }
    }

    /// Returns path segments.
    #[must_use]
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }

    /// Returns whether this path is a strict or equal prefix of another path.
    #[must_use]
    pub fn is_prefix_of(&self, other: &Self) -> bool {
        self.segments.len() <= other.segments.len()
            && self
                .segments
                .iter()
                .zip(other.segments.iter())
                .all(|(left, right)| left == right)
    }
}

impl CanonicalEncode for ValuePath {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_length(output, self.segments.len())?;
        for segment in &self.segments {
            match segment {
                PathSegment::Field(id) => {
                    output.push(PATH_TAG_FIELD);
                    output.extend_from_slice(&id.to_be_bytes());
                }
                PathSegment::TupleIndex(index) => {
                    output.push(PATH_TAG_TUPLE);
                    output.extend_from_slice(&index.to_be_bytes());
                }
                PathSegment::VectorIndex(index) => {
                    output.push(PATH_TAG_VECTOR);
                    output.extend_from_slice(&index.to_be_bytes());
                }
                PathSegment::SumPayload => output.push(PATH_TAG_SUM_PAYLOAD),
                PathSegment::MapKey(encoded_key) => {
                    output.push(PATH_TAG_MAP_KEY);
                    put_blob(output, encoded_key)?;
                }
            }
        }
        Ok(())
    }
}

/// One preconditioned state operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatchOp {
    /// Inserts an absent record field or map entry.
    Insert {
        /// Absent destination path.
        path: ValuePath,
        /// Semantic map key when the last path segment is `MapKey`.
        map_key: Option<Value>,
        /// Inserted value.
        value: Value,
    },
    /// Replaces an existing value with an exact old-value hash precondition.
    Update {
        /// Existing destination path.
        path: ValuePath,
        /// Expected old value commitment.
        expected_old_hash: Hash32,
        /// Replacement value.
        value: Value,
    },
    /// Deletes an existing record field or map entry.
    Delete {
        /// Existing destination path.
        path: ValuePath,
        /// Expected old value commitment.
        expected_old_hash: Hash32,
    },
}

impl PatchOp {
    /// Returns the target path.
    #[must_use]
    pub const fn path(&self) -> &ValuePath {
        match self {
            Self::Insert { path, .. } | Self::Update { path, .. } | Self::Delete { path, .. } => {
                path
            }
        }
    }

    fn sort_key(&self) -> Result<Vec<u8>, PatchError> {
        self.path().canonical_bytes().map_err(PatchError::Encode)
    }
}

impl CanonicalEncode for PatchOp {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Insert {
                path,
                map_key,
                value,
            } => {
                output.push(PATCH_TAG_INSERT);
                put_blob(output, &path.canonical_bytes()?)?;
                match map_key {
                    None => output.push(0),
                    Some(key) => {
                        output.push(1);
                        put_blob(output, &key.canonical_bytes()?)?;
                    }
                }
                put_blob(output, &value.canonical_bytes()?)?;
            }
            Self::Update {
                path,
                expected_old_hash,
                value,
            } => {
                output.push(PATCH_TAG_UPDATE);
                put_blob(output, &path.canonical_bytes()?)?;
                output.extend_from_slice(expected_old_hash.as_bytes());
                put_blob(output, &value.canonical_bytes()?)?;
            }
            Self::Delete {
                path,
                expected_old_hash,
            } => {
                output.push(PATCH_TAG_DELETE);
                put_blob(output, &path.canonical_bytes()?)?;
                output.extend_from_slice(expected_old_hash.as_bytes());
            }
        }
        Ok(())
    }
}

/// Exact reason that two canonical patch operations cannot coexist.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PatchOperationConflictKind {
    /// Two different operations target the same exact path.
    SamePathDifferentOperation,
    /// One operation targets an ancestor of the other operation's path.
    AncestorDescendantOverlap,
}

impl PatchOperationConflictKind {
    const fn tag(self) -> u8 {
        match self {
            Self::SamePathDifferentOperation => 0,
            Self::AncestorDescendantOverlap => 1,
        }
    }
}

/// Canonical, operand-order-independent witness for an incompatible patch merge.
///
/// Operation witnesses retain both exact operations in canonical byte order.
/// A verifier can therefore replay the paths, operation identity, and overlap
/// relation without trusting the merge caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatchMergeConflict {
    /// Patches describe different state schemas.
    StateTypeMismatch {
        /// Canonically smaller state type.
        first: u32,
        /// Canonically larger state type.
        second: u32,
    },
    /// Patches are bound to different immutable pre-states.
    ExpectedPreRootMismatch {
        /// Canonically smaller pre-root.
        first: Hash32,
        /// Canonically larger pre-root.
        second: Hash32,
    },
    /// Exact path-level incompatibility.
    Operation {
        /// Conflict relation between the two paths.
        kind: PatchOperationConflictKind,
        /// Longest common path prefix.
        common_prefix: ValuePath,
        /// Canonically first operation.
        first: PatchOp,
        /// Canonically second operation.
        second: PatchOp,
    },
}

impl CanonicalEncode for PatchMergeConflict {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::StateTypeMismatch { first, second } => {
                output.push(PATCH_MERGE_TAG_STATE_TYPE);
                output.extend_from_slice(&first.to_be_bytes());
                output.extend_from_slice(&second.to_be_bytes());
            }
            Self::ExpectedPreRootMismatch { first, second } => {
                output.push(PATCH_MERGE_TAG_PRE_ROOT);
                output.extend_from_slice(first.as_bytes());
                output.extend_from_slice(second.as_bytes());
            }
            Self::Operation {
                kind,
                common_prefix,
                first,
                second,
            } => {
                output.push(PATCH_MERGE_TAG_OPERATION);
                output.push(kind.tag());
                put_blob(output, &common_prefix.canonical_bytes()?)?;
                put_blob(output, &first.canonical_bytes()?)?;
                put_blob(output, &second.canonical_bytes()?)?;
            }
        }
        Ok(())
    }
}

/// Patch merge failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatchMergeError {
    /// A canonical witness could not be encoded within the wire format.
    Encode(EncodeError),
    /// Reconstructing the merged canonical patch failed.
    Patch(PatchError),
    /// The two patches are semantically incompatible.
    Conflict(Box<PatchMergeConflict>),
}

impl fmt::Display for PatchMergeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(error) => error.fmt(formatter),
            Self::Patch(error) => error.fmt(formatter),
            Self::Conflict(conflict) => match conflict.as_ref() {
                PatchMergeConflict::StateTypeMismatch { .. } => {
                    formatter.write_str("patch state types differ")
                }
                PatchMergeConflict::ExpectedPreRootMismatch { .. } => {
                    formatter.write_str("patch pre-roots differ")
                }
                PatchMergeConflict::Operation { kind, .. } => match kind {
                    PatchOperationConflictKind::SamePathDifferentOperation => {
                        formatter.write_str("different patch operations target the same path")
                    }
                    PatchOperationConflictKind::AncestorDescendantOverlap => {
                        formatter.write_str("patch operation paths overlap")
                    }
                },
            },
        }
    }
}

impl core::error::Error for PatchMergeError {}

/// A canonical, pre-root-bound, non-overlapping patch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalPatch {
    state_type: u32,
    expected_pre_root: Hash32,
    operations: Box<[PatchOp]>,
}

impl CanonicalPatch {
    /// Normalizes operations into canonical path order and rejects overlap.
    pub fn try_new(
        state_type: u32,
        expected_pre_root: Hash32,
        mut operations: Vec<PatchOp>,
    ) -> Result<Self, PatchError> {
        for operation in &operations {
            validate_insert_shape(operation)?;
        }
        let mut keyed = Vec::with_capacity(operations.len());
        for operation in operations.drain(..) {
            keyed.push((operation.sort_key()?, operation));
        }
        keyed.sort_by(|left, right| left.0.cmp(&right.0));
        {
            let mut paths = keyed
                .iter()
                .map(|(_, operation)| operation.path())
                .collect::<Vec<_>>();
            paths.sort_unstable();
            if paths.windows(2).any(|pair| pair[0].is_prefix_of(pair[1])) {
                return Err(PatchError::OverlappingPaths);
            }
        }
        Ok(Self {
            state_type,
            expected_pre_root,
            operations: keyed
                .into_iter()
                .map(|(_, operation)| operation)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        })
    }

    /// Returns the state type identifier.
    #[must_use]
    pub const fn state_type(&self) -> u32 {
        self.state_type
    }

    /// Returns the required pre-root.
    #[must_use]
    pub const fn expected_pre_root(&self) -> Hash32 {
        self.expected_pre_root
    }

    /// Returns canonical operations.
    #[must_use]
    pub fn operations(&self) -> &[PatchOp] {
        &self.operations
    }

    /// Merges compatible operations under one unchanged pre-state.
    ///
    /// Compatible merge is commutative, idempotent, and associative. A
    /// conflict produces the same canonical witness when operands are
    /// reversed. This function creates data only; it grants no commit or
    /// publication authority.
    pub fn merge(&self, other: &Self) -> Result<Self, PatchMergeError> {
        if self.state_type != other.state_type {
            let (first, second) = ordered_pair(self.state_type, other.state_type);
            return Err(PatchMergeError::Conflict(Box::new(
                PatchMergeConflict::StateTypeMismatch { first, second },
            )));
        }
        if self.expected_pre_root != other.expected_pre_root {
            let (first, second) = ordered_pair(self.expected_pre_root, other.expected_pre_root);
            return Err(PatchMergeError::Conflict(Box::new(
                PatchMergeConflict::ExpectedPreRootMismatch { first, second },
            )));
        }

        let mut conflicts = Vec::new();
        for left in self.operations() {
            for right in other.operations() {
                if left.path() == right.path() {
                    if left != right {
                        conflicts.push(operation_conflict(
                            PatchOperationConflictKind::SamePathDifferentOperation,
                            left,
                            right,
                        )?);
                    }
                } else if left.path().is_prefix_of(right.path())
                    || right.path().is_prefix_of(left.path())
                {
                    conflicts.push(operation_conflict(
                        PatchOperationConflictKind::AncestorDescendantOverlap,
                        left,
                        right,
                    )?);
                }
            }
        }
        if !conflicts.is_empty() {
            conflicts.sort_by(|left, right| left.0.cmp(&right.0));
            let Some((_, conflict)) = conflicts.into_iter().next() else {
                return Err(PatchMergeError::Encode(EncodeError::LengthOverflow));
            };
            return Err(PatchMergeError::Conflict(Box::new(conflict)));
        }

        let mut operations = self.operations.to_vec();
        for operation in other.operations() {
            if !operations.iter().any(|existing| existing == operation) {
                operations.push(operation.clone());
            }
        }
        Self::try_new(self.state_type, self.expected_pre_root, operations)
            .map_err(PatchMergeError::Patch)
    }

    /// Purely applies the complete patch or returns without a successor.
    pub fn apply<H: CommitmentHasher>(
        &self,
        state: &Value,
        state_domain: Domain<'_>,
    ) -> Result<AppliedPatch, PatchError> {
        let actual_pre_root = hash_value::<H>(state_domain, state)?;
        if actual_pre_root != self.expected_pre_root {
            return Err(PatchError::PreRootMismatch {
                expected: self.expected_pre_root,
                actual: actual_pre_root,
            });
        }

        let mut current = state.clone();
        for operation in &self.operations {
            current = match operation {
                PatchOp::Insert {
                    path,
                    map_key,
                    value,
                } => insert_at(&current, path.segments(), map_key.as_ref(), value.clone())?,
                PatchOp::Update {
                    path,
                    expected_old_hash,
                    value,
                } => {
                    let old = lookup(&current, path.segments())?;
                    let actual = hash_precondition_value::<H>(old)?;
                    if actual != *expected_old_hash {
                        return Err(PatchError::OldValueMismatch {
                            path: path.clone(),
                            expected: *expected_old_hash,
                            actual,
                        });
                    }
                    replace_at(&current, path.segments(), value.clone())?
                }
                PatchOp::Delete {
                    path,
                    expected_old_hash,
                } => {
                    let old = lookup(&current, path.segments())?;
                    let actual = hash_precondition_value::<H>(old)?;
                    if actual != *expected_old_hash {
                        return Err(PatchError::OldValueMismatch {
                            path: path.clone(),
                            expected: *expected_old_hash,
                            actual,
                        });
                    }
                    delete_at(&current, path.segments())?
                }
            };
        }

        let post_root = hash_value::<H>(state_domain, &current)?;
        Ok(AppliedPatch {
            state: current,
            post_root,
        })
    }
}

fn ordered_pair<T: Ord>(left: T, right: T) -> (T, T) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn operation_conflict(
    kind: PatchOperationConflictKind,
    left: &PatchOp,
    right: &PatchOp,
) -> Result<(Vec<u8>, PatchMergeConflict), PatchMergeError> {
    let left_bytes = left.canonical_bytes().map_err(PatchMergeError::Encode)?;
    let right_bytes = right.canonical_bytes().map_err(PatchMergeError::Encode)?;
    let (first, second, first_bytes, second_bytes) = if left_bytes <= right_bytes {
        (left.clone(), right.clone(), left_bytes, right_bytes)
    } else {
        (right.clone(), left.clone(), right_bytes, left_bytes)
    };
    let common_prefix = common_path_prefix(first.path(), second.path());
    let mut key = common_prefix
        .canonical_bytes()
        .map_err(PatchMergeError::Encode)?;
    key.push(kind.tag());
    put_blob(&mut key, &first_bytes).map_err(PatchMergeError::Encode)?;
    put_blob(&mut key, &second_bytes).map_err(PatchMergeError::Encode)?;
    Ok((
        key,
        PatchMergeConflict::Operation {
            kind,
            common_prefix,
            first,
            second,
        },
    ))
}

fn common_path_prefix(left: &ValuePath, right: &ValuePath) -> ValuePath {
    let segments = left
        .segments()
        .iter()
        .zip(right.segments())
        .take_while(|(left_segment, right_segment)| left_segment == right_segment)
        .map(|(segment, _)| segment.clone())
        .collect();
    ValuePath::new(segments)
}

impl CanonicalEncode for CanonicalPatch {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.state_type.to_be_bytes());
        output.extend_from_slice(self.expected_pre_root.as_bytes());
        put_length(output, self.operations.len())?;
        for operation in &self.operations {
            put_blob(output, &operation.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Strictly decodes one canonical patch under explicit resource bounds.
///
/// Decoding admits every nested value through the ZCVE/1 decoder, reconstructs
/// the patch through [`CanonicalPatch::try_new`], and requires the reconstructed
/// canonical bytes to equal the complete input. Alternate operation order,
/// overlapping paths, malformed map keys, trailing bytes, and noncanonical
/// nested values therefore fail closed.
pub fn decode_canonical_patch(
    bytes: &[u8],
    limits: PatchDecodeLimits,
) -> Result<CanonicalPatch, PatchDecodeError> {
    let actual_input = u64::try_from(bytes.len()).map_err(|_| PatchDecodeError::LengthOverflow)?;
    if actual_input > limits.max_input_bytes {
        return Err(PatchDecodeError::InputLimit {
            limit: limits.max_input_bytes,
            actual: actual_input,
        });
    }

    let mut cursor = PatchCursor::new(bytes);
    let state_type = cursor.take_u32()?;
    let expected_pre_root = cursor.take_hash32()?;
    let operation_count = cursor.take_u32()?;
    if operation_count > limits.max_operations {
        return Err(PatchDecodeError::OperationLimit {
            limit: limits.max_operations,
            actual: operation_count,
        });
    }
    // Every operation is carried in a u32-length-prefixed blob.
    let mut state = PatchDecodeState::default();
    let mut operations = Vec::with_capacity(initial_collection_capacity(
        operation_count,
        cursor.remaining(),
        4,
    )?);
    for _ in 0..operation_count {
        let encoded = cursor.take_blob()?;
        operations.push(decode_patch_operation(encoded, limits, &mut state)?);
    }
    if cursor.remaining() != 0 {
        return Err(PatchDecodeError::TrailingBytes {
            offset: cursor.offset,
        });
    }

    let patch = CanonicalPatch::try_new(state_type, expected_pre_root, operations)
        .map_err(PatchDecodeError::Patch)?;
    let encoded = patch.canonical_bytes().map_err(PatchDecodeError::Encode)?;
    if encoded.as_slice() != bytes {
        return Err(PatchDecodeError::NonCanonical);
    }
    Ok(patch)
}

fn decode_patch_operation(
    bytes: &[u8],
    limits: PatchDecodeLimits,
    state: &mut PatchDecodeState,
) -> Result<PatchOp, PatchDecodeError> {
    let mut cursor = PatchCursor::new(bytes);
    let tag = cursor.take_u8()?;
    if !matches!(tag, PATCH_TAG_INSERT | PATCH_TAG_UPDATE | PATCH_TAG_DELETE) {
        return Err(PatchDecodeError::UnknownOperationTag(tag));
    }
    let path = decode_value_path(cursor.take_blob()?, limits, state)?;
    let operation = match tag {
        PATCH_TAG_INSERT => {
            let map_key = match cursor.take_u8()? {
                0 => None,
                1 => Some(decode_patch_value(cursor.take_blob()?, limits, state)?),
                flag => return Err(PatchDecodeError::InvalidMapKeyFlag(flag)),
            };
            let value = decode_patch_value(cursor.take_blob()?, limits, state)?;
            PatchOp::Insert {
                path,
                map_key,
                value,
            }
        }
        PATCH_TAG_UPDATE => PatchOp::Update {
            path,
            expected_old_hash: cursor.take_hash32()?,
            value: decode_patch_value(cursor.take_blob()?, limits, state)?,
        },
        PATCH_TAG_DELETE => PatchOp::Delete {
            path,
            expected_old_hash: cursor.take_hash32()?,
        },
        other => return Err(PatchDecodeError::UnknownOperationTag(other)),
    };
    if cursor.remaining() != 0 {
        return Err(PatchDecodeError::TrailingBytes {
            offset: cursor.offset,
        });
    }
    Ok(operation)
}

fn decode_value_path(
    bytes: &[u8],
    limits: PatchDecodeLimits,
    state: &mut PatchDecodeState,
) -> Result<ValuePath, PatchDecodeError> {
    let mut cursor = PatchCursor::new(bytes);
    let segment_count = cursor.take_u32()?;
    if segment_count > limits.max_path_segments {
        return Err(PatchDecodeError::PathSegmentLimit {
            limit: limits.max_path_segments,
            actual: segment_count,
        });
    }
    // Every path segment has at least its one-byte tag.
    let mut segments = Vec::with_capacity(initial_collection_capacity(
        segment_count,
        cursor.remaining(),
        1,
    )?);
    for _ in 0..segment_count {
        let segment = match cursor.take_u8()? {
            PATH_TAG_FIELD => PathSegment::Field(cursor.take_u16()?),
            PATH_TAG_TUPLE => PathSegment::TupleIndex(cursor.take_u32()?),
            PATH_TAG_VECTOR => PathSegment::VectorIndex(cursor.take_u32()?),
            PATH_TAG_SUM_PAYLOAD => PathSegment::SumPayload,
            PATH_TAG_MAP_KEY => {
                let encoded_key = cursor.take_blob()?;
                let actual = u64::try_from(encoded_key.len())
                    .map_err(|_| PatchDecodeError::LengthOverflow)?;
                if actual > limits.max_map_key_bytes {
                    return Err(PatchDecodeError::MapKeyLimit {
                        limit: limits.max_map_key_bytes,
                        actual,
                    });
                }
                let _ = decode_patch_value(encoded_key, limits, state)?;
                PathSegment::MapKey(encoded_key.to_vec().into_boxed_slice())
            }
            other => return Err(PatchDecodeError::UnknownPathTag(other)),
        };
        segments.push(segment);
    }
    if cursor.remaining() != 0 {
        return Err(PatchDecodeError::TrailingBytes {
            offset: cursor.offset,
        });
    }
    Ok(ValuePath::new(segments))
}

fn decode_patch_value(
    bytes: &[u8],
    limits: PatchDecodeLimits,
    state: &mut PatchDecodeState,
) -> Result<Value, PatchDecodeError> {
    let value = decode_value(bytes, limits.value).map_err(PatchDecodeError::Value)?;
    let metrics = value
        .validate_limits(limits.value.value)
        .map_err(|error| PatchDecodeError::Value(DecodeError::InvalidValue(error)))?;
    state.value_nodes = state
        .value_nodes
        .checked_add(metrics.nodes)
        .ok_or(PatchDecodeError::LengthOverflow)?;
    if state.value_nodes > limits.max_value_nodes {
        return Err(PatchDecodeError::ValueNodeLimit {
            limit: limits.max_value_nodes,
            actual: state.value_nodes,
        });
    }
    state.value_payload_bytes = state
        .value_payload_bytes
        .checked_add(metrics.payload_bytes)
        .ok_or(PatchDecodeError::LengthOverflow)?;
    if state.value_payload_bytes > limits.max_value_payload_bytes {
        return Err(PatchDecodeError::ValuePayloadLimit {
            limit: limits.max_value_payload_bytes,
            actual: state.value_payload_bytes,
        });
    }
    Ok(value)
}

#[derive(Default)]
struct PatchDecodeState {
    value_nodes: u64,
    value_payload_bytes: u64,
}

fn initial_collection_capacity(
    count: u32,
    remaining_wire_bytes: usize,
    minimum_wire_bytes_per_item: usize,
) -> Result<usize, PatchDecodeError> {
    let count = usize::try_from(count).map_err(|_| PatchDecodeError::LengthOverflow)?;
    let wire_bound = remaining_wire_bytes
        .checked_div(minimum_wire_bytes_per_item)
        .ok_or(PatchDecodeError::LengthOverflow)?;
    Ok(count.min(wire_bound))
}

struct PatchCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PatchCursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], PatchDecodeError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(PatchDecodeError::LengthOverflow)?;
        let Some(bytes) = self.bytes.get(self.offset..end) else {
            return Err(PatchDecodeError::UnexpectedEnd {
                offset: self.offset,
                requested: count,
            });
        };
        self.offset = end;
        Ok(bytes)
    }

    fn take_u8(&mut self) -> Result<u8, PatchDecodeError> {
        self.take(1)?
            .first()
            .copied()
            .ok_or(PatchDecodeError::UnexpectedEnd {
                offset: self.offset,
                requested: 1,
            })
    }

    fn take_u16(&mut self) -> Result<u16, PatchDecodeError> {
        let mut bytes = [0_u8; 2];
        bytes.copy_from_slice(self.take(2)?);
        Ok(u16::from_be_bytes(bytes))
    }

    fn take_u32(&mut self) -> Result<u32, PatchDecodeError> {
        let mut bytes = [0_u8; 4];
        bytes.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(bytes))
    }

    fn take_hash32(&mut self) -> Result<Hash32, PatchDecodeError> {
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(self.take(32)?);
        Ok(Hash32::new(bytes))
    }

    fn take_blob(&mut self) -> Result<&'a [u8], PatchDecodeError> {
        let length = self.take_u32()?;
        let length = usize::try_from(length).map_err(|_| PatchDecodeError::LengthOverflow)?;
        self.take(length)
    }
}

/// Successful pure patch application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedPatch {
    state: Value,
    post_root: Hash32,
}

impl AppliedPatch {
    /// Returns the successor state.
    #[must_use]
    pub const fn state(&self) -> &Value {
        &self.state
    }

    /// Returns the successor root.
    #[must_use]
    pub const fn post_root(&self) -> Hash32 {
        self.post_root
    }

    /// Consumes the result.
    #[must_use]
    pub fn into_parts(self) -> (Value, Hash32) {
        (self.state, self.post_root)
    }
}

/// Computes the canonical value commitment under an explicit state domain.
pub fn hash_value<H: CommitmentHasher>(
    domain: Domain<'_>,
    value: &Value,
) -> Result<Hash32, PatchError> {
    let bytes = value.canonical_bytes().map_err(PatchError::Encode)?;
    commitment::<H>(domain, &bytes).map_err(PatchError::Encode)
}

/// Computes the protocol-defined old-value commitment used by update and deletion preconditions.
pub fn hash_precondition_value<H: CommitmentHasher>(value: &Value) -> Result<Hash32, PatchError> {
    let domain = Domain::new("zeno-fcis/value", 1).map_err(PatchError::Encode)?;
    hash_value::<H>(domain, value)
}

/// Resolves one immutable value path without applying a patch.
///
/// The empty path returns the complete supplied value. Map navigation uses the
/// exact encoded key bytes already carried by [`ValuePath`].
pub fn value_at<'a>(value: &'a Value, path: &ValuePath) -> Result<&'a Value, PatchError> {
    lookup(value, path.segments())
}

fn validate_insert_shape(operation: &PatchOp) -> Result<(), PatchError> {
    let PatchOp::Insert { path, map_key, .. } = operation else {
        return Ok(());
    };
    let Some(last) = path.segments().last() else {
        return Err(PatchError::InsertAtRoot);
    };
    match last {
        PathSegment::Field(_) => {
            if map_key.is_some() {
                return Err(PatchError::UnexpectedMapKey);
            }
        }
        PathSegment::MapKey(encoded_key) => {
            let Some(key) = map_key else {
                return Err(PatchError::MissingMapKey);
            };
            let actual = key.canonical_bytes().map_err(PatchError::Encode)?;
            if actual.as_slice() != encoded_key.as_ref() {
                return Err(PatchError::MapKeyMismatch);
            }
        }
        PathSegment::TupleIndex(_) | PathSegment::VectorIndex(_) | PathSegment::SumPayload => {
            return Err(PatchError::UnsupportedInsertTarget);
        }
    }
    Ok(())
}

fn lookup<'a>(value: &'a Value, segments: &[PathSegment]) -> Result<&'a Value, PatchError> {
    let Some((first, rest)) = segments.split_first() else {
        return Ok(value);
    };
    match (first, value) {
        (PathSegment::Field(id), Value::Record(fields)) => {
            let index = fields
                .binary_search_by_key(id, Field::id)
                .map_err(|_| PatchError::PathNotFound)?;
            lookup(fields[index].value(), rest)
        }
        (PathSegment::TupleIndex(index), Value::Tuple(items))
        | (PathSegment::VectorIndex(index), Value::Vector(items)) => {
            let index = usize::try_from(*index).map_err(|_| PatchError::IndexOverflow)?;
            let child = items.get(index).ok_or(PatchError::PathNotFound)?;
            lookup(child, rest)
        }
        (PathSegment::SumPayload, Value::Sum { payload, .. }) => {
            let child = payload.as_deref().ok_or(PatchError::PathNotFound)?;
            lookup(child, rest)
        }
        (PathSegment::MapKey(encoded_key), Value::Map(entries)) => {
            let index = entries
                .binary_search_by(|entry| entry.encoded_key().cmp(encoded_key.as_ref()))
                .map_err(|_| PatchError::PathNotFound)?;
            lookup(entries[index].value(), rest)
        }
        _ => Err(PatchError::PathTypeMismatch),
    }
}

fn replace_at(
    value: &Value,
    segments: &[PathSegment],
    replacement: Value,
) -> Result<Value, PatchError> {
    let Some((first, rest)) = segments.split_first() else {
        return Ok(replacement);
    };
    match (first, value) {
        (PathSegment::Field(id), Value::Record(fields)) => {
            let index = fields
                .binary_search_by_key(id, Field::id)
                .map_err(|_| PatchError::PathNotFound)?;
            let mut next = fields.to_vec();
            let child = replace_at(next[index].value(), rest, replacement)?;
            next[index] = Field::new(*id, child);
            Value::record_canonical(next).map_err(PatchError::InvalidValue)
        }
        (PathSegment::TupleIndex(index), Value::Tuple(items)) => {
            let index = usize::try_from(*index).map_err(|_| PatchError::IndexOverflow)?;
            let mut next = items.to_vec();
            let current = next.get(index).ok_or(PatchError::PathNotFound)?;
            let child = replace_at(current, rest, replacement)?;
            next[index] = child;
            Ok(Value::Tuple(next.into_boxed_slice()))
        }
        (PathSegment::VectorIndex(index), Value::Vector(items)) => {
            let index = usize::try_from(*index).map_err(|_| PatchError::IndexOverflow)?;
            let mut next = items.to_vec();
            let current = next.get(index).ok_or(PatchError::PathNotFound)?;
            let child = replace_at(current, rest, replacement)?;
            next[index] = child;
            Ok(Value::Vector(next.into_boxed_slice()))
        }
        (
            PathSegment::SumPayload,
            Value::Sum {
                type_id,
                variant,
                payload,
            },
        ) => {
            let current = payload.as_deref().ok_or(PatchError::PathNotFound)?;
            let child = replace_at(current, rest, replacement)?;
            Ok(Value::Sum {
                type_id: *type_id,
                variant: *variant,
                payload: Some(Box::new(child)),
            })
        }
        (PathSegment::MapKey(encoded_key), Value::Map(entries)) => {
            let index = entries
                .binary_search_by(|entry| entry.encoded_key().cmp(encoded_key.as_ref()))
                .map_err(|_| PatchError::PathNotFound)?;
            let mut next = entries.to_vec();
            let key = next[index].key().clone();
            let child = replace_at(next[index].value(), rest, replacement)?;
            next[index] = MapEntry::try_new(key, child).map_err(PatchError::InvalidValue)?;
            Value::map_canonical(next).map_err(PatchError::InvalidValue)
        }
        _ => Err(PatchError::PathTypeMismatch),
    }
}

fn insert_at(
    value: &Value,
    segments: &[PathSegment],
    map_key: Option<&Value>,
    inserted: Value,
) -> Result<Value, PatchError> {
    let Some((first, rest)) = segments.split_first() else {
        return Err(PatchError::InsertAtRoot);
    };
    if rest.is_empty() {
        return match (first, value) {
            (PathSegment::Field(id), Value::Record(fields)) => {
                let mut next = fields.to_vec();
                match next.binary_search_by_key(id, Field::id) {
                    Ok(_) => Err(PatchError::ExpectedAbsent),
                    Err(index) => {
                        next.insert(index, Field::new(*id, inserted));
                        Value::record_canonical(next).map_err(PatchError::InvalidValue)
                    }
                }
            }
            (PathSegment::MapKey(encoded_key), Value::Map(entries)) => {
                let key = map_key.ok_or(PatchError::MissingMapKey)?;
                let mut next = entries.to_vec();
                match next.binary_search_by(|entry| entry.encoded_key().cmp(encoded_key.as_ref())) {
                    Ok(_) => Err(PatchError::ExpectedAbsent),
                    Err(index) => {
                        let entry = MapEntry::try_new(key.clone(), inserted)
                            .map_err(PatchError::InvalidValue)?;
                        if entry.encoded_key() != encoded_key.as_ref() {
                            return Err(PatchError::MapKeyMismatch);
                        }
                        next.insert(index, entry);
                        Value::map_canonical(next).map_err(PatchError::InvalidValue)
                    }
                }
            }
            _ => Err(PatchError::UnsupportedInsertTarget),
        };
    }

    match (first, value) {
        (PathSegment::Field(id), Value::Record(fields)) => {
            let index = fields
                .binary_search_by_key(id, Field::id)
                .map_err(|_| PatchError::PathNotFound)?;
            let mut next = fields.to_vec();
            let child = insert_at(next[index].value(), rest, map_key, inserted)?;
            next[index] = Field::new(*id, child);
            Value::record_canonical(next).map_err(PatchError::InvalidValue)
        }
        (PathSegment::TupleIndex(index), Value::Tuple(items)) => {
            let index = usize::try_from(*index).map_err(|_| PatchError::IndexOverflow)?;
            let mut next = items.to_vec();
            let current = next.get(index).ok_or(PatchError::PathNotFound)?;
            let child = insert_at(current, rest, map_key, inserted)?;
            next[index] = child;
            Ok(Value::Tuple(next.into_boxed_slice()))
        }
        (PathSegment::VectorIndex(index), Value::Vector(items)) => {
            let index = usize::try_from(*index).map_err(|_| PatchError::IndexOverflow)?;
            let mut next = items.to_vec();
            let current = next.get(index).ok_or(PatchError::PathNotFound)?;
            let child = insert_at(current, rest, map_key, inserted)?;
            next[index] = child;
            Ok(Value::Vector(next.into_boxed_slice()))
        }
        (
            PathSegment::SumPayload,
            Value::Sum {
                type_id,
                variant,
                payload,
            },
        ) => {
            let current = payload.as_deref().ok_or(PatchError::PathNotFound)?;
            let child = insert_at(current, rest, map_key, inserted)?;
            Ok(Value::Sum {
                type_id: *type_id,
                variant: *variant,
                payload: Some(Box::new(child)),
            })
        }
        (PathSegment::MapKey(encoded_key), Value::Map(entries)) => {
            let index = entries
                .binary_search_by(|entry| entry.encoded_key().cmp(encoded_key.as_ref()))
                .map_err(|_| PatchError::PathNotFound)?;
            let mut next = entries.to_vec();
            let key = next[index].key().clone();
            let child = insert_at(next[index].value(), rest, map_key, inserted)?;
            next[index] = MapEntry::try_new(key, child).map_err(PatchError::InvalidValue)?;
            Value::map_canonical(next).map_err(PatchError::InvalidValue)
        }
        _ => Err(PatchError::PathTypeMismatch),
    }
}

fn delete_at(value: &Value, segments: &[PathSegment]) -> Result<Value, PatchError> {
    let Some((first, rest)) = segments.split_first() else {
        return Err(PatchError::DeleteRoot);
    };
    if rest.is_empty() {
        return match (first, value) {
            (PathSegment::Field(id), Value::Record(fields)) => {
                let mut next = fields.to_vec();
                let index = next
                    .binary_search_by_key(id, Field::id)
                    .map_err(|_| PatchError::PathNotFound)?;
                next.remove(index);
                Value::record_canonical(next).map_err(PatchError::InvalidValue)
            }
            (PathSegment::MapKey(encoded_key), Value::Map(entries)) => {
                let mut next = entries.to_vec();
                let index = next
                    .binary_search_by(|entry| entry.encoded_key().cmp(encoded_key.as_ref()))
                    .map_err(|_| PatchError::PathNotFound)?;
                next.remove(index);
                Value::map_canonical(next).map_err(PatchError::InvalidValue)
            }
            _ => Err(PatchError::UnsupportedDeleteTarget),
        };
    }

    match (first, value) {
        (PathSegment::Field(id), Value::Record(fields)) => {
            let index = fields
                .binary_search_by_key(id, Field::id)
                .map_err(|_| PatchError::PathNotFound)?;
            let mut next = fields.to_vec();
            let child = delete_at(next[index].value(), rest)?;
            next[index] = Field::new(*id, child);
            Value::record_canonical(next).map_err(PatchError::InvalidValue)
        }
        (PathSegment::TupleIndex(index), Value::Tuple(items)) => {
            let index = usize::try_from(*index).map_err(|_| PatchError::IndexOverflow)?;
            let mut next = items.to_vec();
            let current = next.get(index).ok_or(PatchError::PathNotFound)?;
            let child = delete_at(current, rest)?;
            next[index] = child;
            Ok(Value::Tuple(next.into_boxed_slice()))
        }
        (PathSegment::VectorIndex(index), Value::Vector(items)) => {
            let index = usize::try_from(*index).map_err(|_| PatchError::IndexOverflow)?;
            let mut next = items.to_vec();
            let current = next.get(index).ok_or(PatchError::PathNotFound)?;
            let child = delete_at(current, rest)?;
            next[index] = child;
            Ok(Value::Vector(next.into_boxed_slice()))
        }
        (
            PathSegment::SumPayload,
            Value::Sum {
                type_id,
                variant,
                payload,
            },
        ) => {
            let current = payload.as_deref().ok_or(PatchError::PathNotFound)?;
            let child = delete_at(current, rest)?;
            Ok(Value::Sum {
                type_id: *type_id,
                variant: *variant,
                payload: Some(Box::new(child)),
            })
        }
        (PathSegment::MapKey(encoded_key), Value::Map(entries)) => {
            let index = entries
                .binary_search_by(|entry| entry.encoded_key().cmp(encoded_key.as_ref()))
                .map_err(|_| PatchError::PathNotFound)?;
            let mut next = entries.to_vec();
            let key = next[index].key().clone();
            let child = delete_at(next[index].value(), rest)?;
            next[index] = MapEntry::try_new(key, child).map_err(PatchError::InvalidValue)?;
            Value::map_canonical(next).map_err(PatchError::InvalidValue)
        }
        _ => Err(PatchError::PathTypeMismatch),
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

/// Strict canonical patch decoding failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatchDecodeError {
    /// Complete input exceeds the declared byte limit.
    InputLimit {
        /// Configured limit.
        limit: u64,
        /// Actual input bytes.
        actual: u64,
    },
    /// The operation count exceeds its limit.
    OperationLimit {
        /// Configured limit.
        limit: u32,
        /// Declared operation count.
        actual: u32,
    },
    /// A path segment count exceeds its limit.
    PathSegmentLimit {
        /// Configured limit.
        limit: u32,
        /// Declared segment count.
        actual: u32,
    },
    /// An encoded map key exceeds its byte limit.
    MapKeyLimit {
        /// Configured limit.
        limit: u64,
        /// Actual encoded bytes.
        actual: u64,
    },
    /// Aggregate decoded value nodes exceed their limit.
    ValueNodeLimit {
        /// Configured limit.
        limit: u64,
        /// Attempted aggregate nodes.
        actual: u64,
    },
    /// Aggregate decoded value payload bytes exceed their limit.
    ValuePayloadLimit {
        /// Configured limit.
        limit: u64,
        /// Attempted aggregate payload bytes.
        actual: u64,
    },
    /// A length conversion or counter overflowed.
    LengthOverflow,
    /// Input ended before the declared item was complete.
    UnexpectedEnd {
        /// Byte offset where decoding stopped.
        offset: usize,
        /// Requested byte count.
        requested: usize,
    },
    /// Bytes remained after a complete item.
    TrailingBytes {
        /// First trailing byte offset within the current item.
        offset: usize,
    },
    /// A path segment tag is not defined by the patch format.
    UnknownPathTag(u8),
    /// A patch operation tag is not defined by the patch format.
    UnknownOperationTag(u8),
    /// An insert map-key flag is not zero or one.
    InvalidMapKeyFlag(u8),
    /// A nested ZCVE value failed strict decoding.
    Value(DecodeError),
    /// Reconstructed patch invariants failed.
    Patch(PatchError),
    /// Canonical re-encoding failed.
    Encode(EncodeError),
    /// Reconstructed canonical bytes differ from the complete input.
    NonCanonical,
}

impl fmt::Display for PatchDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputLimit { limit, actual } => {
                write!(
                    formatter,
                    "patch input bytes {actual} exceeds limit {limit}"
                )
            }
            Self::OperationLimit { limit, actual } => {
                write!(formatter, "patch operations {actual} exceeds limit {limit}")
            }
            Self::PathSegmentLimit { limit, actual } => {
                write!(formatter, "path segments {actual} exceeds limit {limit}")
            }
            Self::MapKeyLimit { limit, actual } => {
                write!(
                    formatter,
                    "encoded map-key bytes {actual} exceeds limit {limit}"
                )
            }
            Self::ValueNodeLimit { limit, actual } => {
                write!(
                    formatter,
                    "decoded value nodes {actual} exceeds limit {limit}"
                )
            }
            Self::ValuePayloadLimit { limit, actual } => write!(
                formatter,
                "decoded value payload bytes {actual} exceeds limit {limit}"
            ),
            Self::LengthOverflow => formatter.write_str("patch decode length overflow"),
            Self::UnexpectedEnd { offset, requested } => write!(
                formatter,
                "patch input ended at offset {offset} before {requested} bytes"
            ),
            Self::TrailingBytes { offset } => {
                write!(formatter, "trailing patch bytes at offset {offset}")
            }
            Self::UnknownPathTag(tag) => write!(formatter, "unknown path tag {tag}"),
            Self::UnknownOperationTag(tag) => {
                write!(formatter, "unknown patch operation tag {tag}")
            }
            Self::InvalidMapKeyFlag(flag) => write!(formatter, "invalid map-key flag {flag}"),
            Self::Value(error) => error.fmt(formatter),
            Self::Patch(error) => error.fmt(formatter),
            Self::Encode(error) => error.fmt(formatter),
            Self::NonCanonical => formatter.write_str("noncanonical patch encoding"),
        }
    }
}

impl core::error::Error for PatchDecodeError {}

/// Patch construction or application failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatchError {
    /// Canonical encoding failed.
    Encode(EncodeError),
    /// A closed value failed validation.
    InvalidValue(ValueError),
    /// Operations target the same path or an ancestor/descendant pair.
    OverlappingPaths,
    /// A root insert is not meaningful.
    InsertAtRoot,
    /// A root delete is not meaningful.
    DeleteRoot,
    /// A map insert omitted its semantic key.
    MissingMapKey,
    /// A non-map insert unexpectedly carried a map key.
    UnexpectedMapKey,
    /// The semantic map key does not match the path's encoded key.
    MapKeyMismatch,
    /// The selected insert destination is unsupported.
    UnsupportedInsertTarget,
    /// The selected delete destination is unsupported.
    UnsupportedDeleteTarget,
    /// A path segment does not match the encountered value kind.
    PathTypeMismatch,
    /// The path does not exist.
    PathNotFound,
    /// An index could not be represented on this platform.
    IndexOverflow,
    /// Insert expected an absent destination but found a value.
    ExpectedAbsent,
    /// The state root does not match the patch precondition.
    PreRootMismatch {
        /// Expected root.
        expected: Hash32,
        /// Actual root.
        actual: Hash32,
    },
    /// An existing value does not match the operation precondition.
    OldValueMismatch {
        /// Target path.
        path: ValuePath,
        /// Expected old value commitment.
        expected: Hash32,
        /// Actual old value commitment.
        actual: Hash32,
    },
}

impl fmt::Display for PatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(error) => error.fmt(formatter),
            Self::InvalidValue(error) => error.fmt(formatter),
            Self::OverlappingPaths => formatter.write_str("patch paths overlap"),
            Self::InsertAtRoot => formatter.write_str("cannot insert the root"),
            Self::DeleteRoot => formatter.write_str("cannot delete the root"),
            Self::MissingMapKey => formatter.write_str("map insert is missing its semantic key"),
            Self::UnexpectedMapKey => formatter.write_str("non-map insert carries a map key"),
            Self::MapKeyMismatch => {
                formatter.write_str("semantic map key does not match encoded key")
            }
            Self::UnsupportedInsertTarget => formatter.write_str("unsupported insert target"),
            Self::UnsupportedDeleteTarget => formatter.write_str("unsupported delete target"),
            Self::PathTypeMismatch => formatter.write_str("path segment does not match value kind"),
            Self::PathNotFound => formatter.write_str("patch path not found"),
            Self::IndexOverflow => formatter.write_str("path index overflow"),
            Self::ExpectedAbsent => formatter.write_str("insert destination is not absent"),
            Self::PreRootMismatch { expected, actual } => {
                write!(
                    formatter,
                    "pre-root mismatch: expected {expected}, actual {actual}"
                )
            }
            Self::OldValueMismatch {
                expected, actual, ..
            } => write!(
                formatter,
                "old-value mismatch: expected {expected}, actual {actual}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    struct TestHasher;

    impl CommitmentHasher for TestHasher {
        const ALGORITHM_ID: &'static str = "test/fold/v1";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().copied().enumerate() {
                let slot = index % 32;
                output[slot] = output[slot].wrapping_add(byte);
            }
            Hash32::new(output)
        }
    }

    fn state_domain() -> Domain<'static> {
        match Domain::new("test/state", 1) {
            Ok(domain) => domain,
            Err(error) => panic!("invalid test domain: {error}"),
        }
    }

    fn field_update(field: u16, value: Value) -> PatchOp {
        PatchOp::Update {
            path: ValuePath::new(vec![PathSegment::Field(field)]),
            expected_old_hash: Hash32::ZERO,
            value,
        }
    }

    fn encode_patch_unchecked(operations: &[PatchOp]) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend_from_slice(&1_u32.to_be_bytes());
        output.extend_from_slice(Hash32::ZERO.as_bytes());
        if let Err(error) = put_length(&mut output, operations.len()) {
            panic!("operation count encoding failed: {error}");
        }
        for operation in operations {
            let encoded = match operation.canonical_bytes() {
                Ok(encoded) => encoded,
                Err(error) => panic!("operation encoding failed: {error}"),
            };
            if let Err(error) = put_blob(&mut output, &encoded) {
                panic!("operation blob encoding failed: {error}");
            }
        }
        output
    }

    fn canonical_patch_bytes(operations: Vec<PatchOp>) -> (CanonicalPatch, Vec<u8>) {
        let patch = match CanonicalPatch::try_new(1, Hash32::ZERO, operations) {
            Ok(patch) => patch,
            Err(error) => panic!("patch construction failed: {error}"),
        };
        let bytes = match patch.canonical_bytes() {
            Ok(bytes) => bytes,
            Err(error) => panic!("patch encoding failed: {error}"),
        };
        (patch, bytes)
    }

    #[test]
    fn update_checks_pre_root_and_old_value() {
        let state = match Value::record_canonical(vec![Field::new(1, Value::U128(7))]) {
            Ok(state) => state,
            Err(error) => panic!("invalid state: {error}"),
        };
        let pre_root = match hash_value::<TestHasher>(state_domain(), &state) {
            Ok(root) => root,
            Err(error) => panic!("hash failed: {error}"),
        };
        let old_hash = match hash_precondition_value::<TestHasher>(&Value::U128(7)) {
            Ok(root) => root,
            Err(error) => panic!("hash failed: {error}"),
        };
        let patch = CanonicalPatch::try_new(
            1,
            pre_root,
            vec![PatchOp::Update {
                path: ValuePath::new(vec![PathSegment::Field(1)]),
                expected_old_hash: old_hash,
                value: Value::U128(8),
            }],
        );
        assert!(patch.is_ok());
        let applied = patch.and_then(|value| value.apply::<TestHasher>(&state, state_domain()));
        assert!(applied.is_ok());
    }

    #[test]
    fn value_at_resolves_nested_paths_without_mutation() {
        let state = Value::record_canonical(vec![Field::new(
            1,
            Value::Tuple(vec![Value::Bool(false), Value::U128(7)].into_boxed_slice()),
        )])
        .unwrap_or_else(|error| panic!("invalid state: {error}"));
        let original = state.clone();
        let path = ValuePath::new(vec![PathSegment::Field(1), PathSegment::TupleIndex(1)]);

        assert_eq!(value_at(&state, &path), Ok(&Value::U128(7)));
        assert_eq!(state, original);
    }

    #[test]
    fn overlapping_paths_are_rejected() {
        let patch = CanonicalPatch::try_new(
            1,
            Hash32::ZERO,
            vec![
                PatchOp::Update {
                    path: ValuePath::new(vec![PathSegment::Field(1)]),
                    expected_old_hash: Hash32::ZERO,
                    value: Value::U128(1),
                },
                PatchOp::Update {
                    path: ValuePath::new(vec![PathSegment::Field(1), PathSegment::Field(2)]),
                    expected_old_hash: Hash32::ZERO,
                    value: Value::U128(2),
                },
            ],
        );
        assert_eq!(patch, Err(PatchError::OverlappingPaths));
    }

    #[test]
    fn compatible_merge_is_commutative_idempotent_and_associative() {
        let patch = |field, value| {
            CanonicalPatch::try_new(
                1,
                Hash32::ZERO,
                vec![field_update(field, Value::U128(value))],
            )
            .unwrap_or_else(|error| panic!("patch construction failed: {error}"))
        };
        let first = patch(1, 11);
        let second = patch(2, 22);
        let third = patch(3, 33);

        let first_second = first
            .merge(&second)
            .unwrap_or_else(|error| panic!("compatible merge failed: {error}"));
        let second_first = second
            .merge(&first)
            .unwrap_or_else(|error| panic!("compatible merge failed: {error}"));
        assert_eq!(first_second, second_first);
        assert_eq!(
            first
                .merge(&first)
                .unwrap_or_else(|error| panic!("idempotent merge failed: {error}")),
            first
        );

        let left = first_second
            .merge(&third)
            .unwrap_or_else(|error| panic!("compatible merge failed: {error}"));
        let second_third = second
            .merge(&third)
            .unwrap_or_else(|error| panic!("compatible merge failed: {error}"));
        let right = first
            .merge(&second_third)
            .unwrap_or_else(|error| panic!("compatible merge failed: {error}"));
        assert_eq!(left, right);
    }

    #[test]
    fn merged_patch_checks_one_prestate_and_applies_both_operations() {
        let state = Value::record_canonical(vec![
            Field::new(1, Value::U128(10)),
            Field::new(2, Value::U128(20)),
        ])
        .unwrap_or_else(|error| panic!("invalid state: {error}"));
        let original = state.clone();
        let pre_root = hash_value::<TestHasher>(state_domain(), &state)
            .unwrap_or_else(|error| panic!("state hash failed: {error}"));
        let first_old = hash_precondition_value::<TestHasher>(&Value::U128(10))
            .unwrap_or_else(|error| panic!("value hash failed: {error}"));
        let second_old = hash_precondition_value::<TestHasher>(&Value::U128(20))
            .unwrap_or_else(|error| panic!("value hash failed: {error}"));
        let first = CanonicalPatch::try_new(
            1,
            pre_root,
            vec![PatchOp::Update {
                path: ValuePath::new(vec![PathSegment::Field(1)]),
                expected_old_hash: first_old,
                value: Value::U128(11),
            }],
        )
        .unwrap_or_else(|error| panic!("patch construction failed: {error}"));
        let second = CanonicalPatch::try_new(
            1,
            pre_root,
            vec![PatchOp::Update {
                path: ValuePath::new(vec![PathSegment::Field(2)]),
                expected_old_hash: second_old,
                value: Value::U128(22),
            }],
        )
        .unwrap_or_else(|error| panic!("patch construction failed: {error}"));

        let merged = first
            .merge(&second)
            .unwrap_or_else(|error| panic!("compatible merge failed: {error}"));
        let applied = merged
            .apply::<TestHasher>(&state, state_domain())
            .unwrap_or_else(|error| panic!("merged application failed: {error}"));
        let expected = Value::record_canonical(vec![
            Field::new(1, Value::U128(11)),
            Field::new(2, Value::U128(22)),
        ])
        .unwrap_or_else(|error| panic!("invalid expected state: {error}"));
        assert_eq!(applied.state(), &expected);
        assert_eq!(state, original);
    }

    #[test]
    fn same_path_conflict_witness_is_operand_order_independent() {
        let first = CanonicalPatch::try_new(1, Hash32::ZERO, vec![field_update(1, Value::U128(1))])
            .unwrap_or_else(|error| panic!("patch construction failed: {error}"));
        let second =
            CanonicalPatch::try_new(1, Hash32::ZERO, vec![field_update(1, Value::U128(2))])
                .unwrap_or_else(|error| panic!("patch construction failed: {error}"));

        let left = first.merge(&second);
        let right = second.merge(&first);
        assert_eq!(left, right);
        let Err(PatchMergeError::Conflict(conflict)) = left else {
            panic!("expected a same-path conflict");
        };
        assert!(matches!(
            conflict.as_ref(),
            PatchMergeConflict::Operation {
                kind: PatchOperationConflictKind::SamePathDifferentOperation,
                ..
            }
        ));
    }

    #[test]
    fn ancestor_conflict_witness_retains_exact_common_prefix() {
        let ancestor =
            CanonicalPatch::try_new(1, Hash32::ZERO, vec![field_update(1, Value::U128(1))])
                .unwrap_or_else(|error| panic!("patch construction failed: {error}"));
        let descendant = CanonicalPatch::try_new(
            1,
            Hash32::ZERO,
            vec![PatchOp::Update {
                path: ValuePath::new(vec![PathSegment::Field(1), PathSegment::Field(2)]),
                expected_old_hash: Hash32::ZERO,
                value: Value::U128(2),
            }],
        )
        .unwrap_or_else(|error| panic!("patch construction failed: {error}"));

        let left = ancestor.merge(&descendant);
        let right = descendant.merge(&ancestor);
        assert_eq!(left, right);
        let Err(PatchMergeError::Conflict(conflict)) = left else {
            panic!("expected an ancestor/descendant conflict");
        };
        let PatchMergeConflict::Operation {
            kind,
            common_prefix,
            ..
        } = conflict.as_ref()
        else {
            panic!("expected an operation conflict");
        };
        assert_eq!(*kind, PatchOperationConflictKind::AncestorDescendantOverlap);
        assert_eq!(*common_prefix, ValuePath::new(vec![PathSegment::Field(1)]));
    }

    #[test]
    fn metadata_conflicts_are_canonical_under_operand_reversal() {
        let operation = vec![field_update(1, Value::U128(1))];
        let first = CanonicalPatch::try_new(3, Hash32::new([2; 32]), operation.clone())
            .unwrap_or_else(|error| panic!("patch construction failed: {error}"));
        let different_type = CanonicalPatch::try_new(1, Hash32::new([2; 32]), operation.clone())
            .unwrap_or_else(|error| panic!("patch construction failed: {error}"));
        let different_root = CanonicalPatch::try_new(3, Hash32::new([1; 32]), operation)
            .unwrap_or_else(|error| panic!("patch construction failed: {error}"));

        assert_eq!(first.merge(&different_type), different_type.merge(&first));
        assert_eq!(first.merge(&different_root), different_root.merge(&first));
        assert_eq!(
            first.merge(&different_type),
            Err(PatchMergeError::Conflict(Box::new(
                PatchMergeConflict::StateTypeMismatch {
                    first: 1,
                    second: 3,
                }
            )))
        );
        assert_eq!(
            first.merge(&different_root),
            Err(PatchMergeError::Conflict(Box::new(
                PatchMergeConflict::ExpectedPreRootMismatch {
                    first: Hash32::new([1; 32]),
                    second: Hash32::new([2; 32]),
                }
            )))
        );
    }

    #[test]
    fn non_adjacent_overlapping_paths_are_rejected() {
        let patch = CanonicalPatch::try_new(
            1,
            Hash32::ZERO,
            vec![
                PatchOp::Update {
                    path: ValuePath::new(vec![PathSegment::Field(1)]),
                    expected_old_hash: Hash32::ZERO,
                    value: Value::U128(1),
                },
                PatchOp::Update {
                    path: ValuePath::new(vec![PathSegment::Field(0), PathSegment::Field(0)]),
                    expected_old_hash: Hash32::ZERO,
                    value: Value::U128(2),
                },
                PatchOp::Update {
                    path: ValuePath::new(vec![PathSegment::Field(1), PathSegment::Field(0)]),
                    expected_old_hash: Hash32::ZERO,
                    value: Value::U128(3),
                },
            ],
        );
        assert_eq!(patch, Err(PatchError::OverlappingPaths));
    }

    #[test]
    fn record_insert_and_delete_are_pure() {
        let state = Value::Record(Vec::<Field>::new().into_boxed_slice());
        let pre_root = hash_value::<TestHasher>(state_domain(), &state).unwrap_or(Hash32::ZERO);
        let insert = CanonicalPatch::try_new(
            1,
            pre_root,
            vec![PatchOp::Insert {
                path: ValuePath::new(vec![PathSegment::Field(1)]),
                map_key: None,
                value: Value::U128(9),
            }],
        );
        assert!(insert.is_ok());
        let inserted = insert.and_then(|value| value.apply::<TestHasher>(&state, state_domain()));
        assert!(inserted.is_ok());
        assert_eq!(state, Value::Record(Vec::<Field>::new().into_boxed_slice()));
    }

    #[test]
    fn strict_decoder_round_trips_and_accepts_exact_outer_limits() {
        let (patch, bytes) = canonical_patch_bytes(vec![field_update(1, Value::U128(8))]);
        let byte_count = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let limits = PatchDecodeLimits {
            max_input_bytes: byte_count,
            max_operations: 1,
            max_path_segments: 1,
            max_value_nodes: 1,
            ..PatchDecodeLimits::default()
        };

        let decoded = decode_canonical_patch(&bytes, limits);
        assert_eq!(decoded, Ok(patch.clone()));
        let reencoded =
            decoded.and_then(|value| value.canonical_bytes().map_err(PatchDecodeError::Encode));
        assert_eq!(reencoded, Ok(bytes.clone()));

        assert_eq!(
            decode_canonical_patch(
                &bytes,
                PatchDecodeLimits {
                    max_input_bytes: byte_count.saturating_sub(1),
                    ..limits
                },
            ),
            Err(PatchDecodeError::InputLimit {
                limit: byte_count.saturating_sub(1),
                actual: byte_count,
            })
        );
        assert_eq!(
            decode_canonical_patch(
                &bytes,
                PatchDecodeLimits {
                    max_operations: 0,
                    ..limits
                },
            ),
            Err(PatchDecodeError::OperationLimit {
                limit: 0,
                actual: 1,
            })
        );
    }

    #[test]
    fn collection_reservation_is_bounded_by_remaining_wire_bytes() {
        assert_eq!(initial_collection_capacity(4_096, 0, 4), Ok(0));
        assert_eq!(initial_collection_capacity(4_096, 15, 4), Ok(3));
        assert_eq!(initial_collection_capacity(64, 7, 1), Ok(7));
        assert_eq!(initial_collection_capacity(2, 100, 4), Ok(2));
        assert_eq!(
            initial_collection_capacity(1, 1, 0),
            Err(PatchDecodeError::LengthOverflow)
        );
    }

    #[test]
    fn truncated_large_operation_declaration_is_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(Hash32::ZERO.as_bytes());
        bytes.extend_from_slice(&4_096_u32.to_be_bytes());

        assert!(matches!(
            decode_canonical_patch(&bytes, PatchDecodeLimits::default()),
            Err(PatchDecodeError::UnexpectedEnd { .. })
        ));
    }

    #[test]
    fn strict_decoder_enforces_path_and_aggregate_value_limits() {
        let operation = PatchOp::Update {
            path: ValuePath::new(vec![PathSegment::Field(1), PathSegment::TupleIndex(0)]),
            expected_old_hash: Hash32::ZERO,
            value: Value::Bytes(vec![1_u8, 2].into_boxed_slice()),
        };
        let (_, bytes) = canonical_patch_bytes(vec![operation]);
        let limits = PatchDecodeLimits {
            max_path_segments: 2,
            max_value_nodes: 1,
            max_value_payload_bytes: 2,
            ..PatchDecodeLimits::default()
        };
        assert!(decode_canonical_patch(&bytes, limits).is_ok());

        assert_eq!(
            decode_canonical_patch(
                &bytes,
                PatchDecodeLimits {
                    max_path_segments: 1,
                    ..limits
                },
            ),
            Err(PatchDecodeError::PathSegmentLimit {
                limit: 1,
                actual: 2,
            })
        );
        assert_eq!(
            decode_canonical_patch(
                &bytes,
                PatchDecodeLimits {
                    max_value_nodes: 0,
                    ..limits
                },
            ),
            Err(PatchDecodeError::ValueNodeLimit {
                limit: 0,
                actual: 1,
            })
        );
        assert_eq!(
            decode_canonical_patch(
                &bytes,
                PatchDecodeLimits {
                    max_value_payload_bytes: 1,
                    ..limits
                },
            ),
            Err(PatchDecodeError::ValuePayloadLimit {
                limit: 1,
                actual: 2,
            })
        );
    }

    #[test]
    fn strict_decoder_validates_canonical_map_keys_and_their_exact_bound() {
        let key = Value::U128(7);
        let encoded_key = match key.canonical_bytes() {
            Ok(bytes) => bytes,
            Err(error) => panic!("key encoding failed: {error}"),
        };
        let operation = PatchOp::Insert {
            path: ValuePath::new(vec![PathSegment::MapKey(
                encoded_key.clone().into_boxed_slice(),
            )]),
            map_key: Some(key),
            value: Value::Bool(true),
        };
        let (_, bytes) = canonical_patch_bytes(vec![operation]);
        let key_bytes = u64::try_from(encoded_key.len()).unwrap_or(u64::MAX);
        let limits = PatchDecodeLimits {
            max_map_key_bytes: key_bytes,
            max_value_nodes: 3,
            ..PatchDecodeLimits::default()
        };
        assert!(decode_canonical_patch(&bytes, limits).is_ok());
        assert_eq!(
            decode_canonical_patch(
                &bytes,
                PatchDecodeLimits {
                    max_map_key_bytes: key_bytes.saturating_sub(1),
                    ..limits
                },
            ),
            Err(PatchDecodeError::MapKeyLimit {
                limit: key_bytes.saturating_sub(1),
                actual: key_bytes,
            })
        );
    }

    #[test]
    fn strict_decoder_rejects_noncanonical_order_overlap_and_key_mismatch() {
        let reversed = encode_patch_unchecked(&[
            field_update(2, Value::U128(2)),
            field_update(1, Value::U128(1)),
        ]);
        assert_eq!(
            decode_canonical_patch(&reversed, PatchDecodeLimits::default()),
            Err(PatchDecodeError::NonCanonical)
        );

        let overlapping = encode_patch_unchecked(&[
            field_update(1, Value::U128(1)),
            PatchOp::Update {
                path: ValuePath::new(vec![PathSegment::Field(1), PathSegment::Field(2)]),
                expected_old_hash: Hash32::ZERO,
                value: Value::U128(2),
            },
        ]);
        assert_eq!(
            decode_canonical_patch(&overlapping, PatchDecodeLimits::default()),
            Err(PatchDecodeError::Patch(PatchError::OverlappingPaths))
        );

        let encoded_path_key = match Value::U128(1).canonical_bytes() {
            Ok(bytes) => bytes,
            Err(error) => panic!("key encoding failed: {error}"),
        };
        let mismatched = encode_patch_unchecked(&[PatchOp::Insert {
            path: ValuePath::new(vec![PathSegment::MapKey(
                encoded_path_key.into_boxed_slice(),
            )]),
            map_key: Some(Value::U128(2)),
            value: Value::Bool(true),
        }]);
        assert_eq!(
            decode_canonical_patch(&mismatched, PatchDecodeLimits::default()),
            Err(PatchDecodeError::Patch(PatchError::MapKeyMismatch))
        );
    }

    #[test]
    fn strict_decoder_propagates_nested_zcve_limits() {
        let (_, bytes) = canonical_patch_bytes(vec![field_update(
            1,
            Value::Vector(vec![Value::Unit, Value::Unit].into_boxed_slice()),
        )]);
        let mut limits = PatchDecodeLimits::default();
        limits.value.value.max_collection_len = 1;

        assert_eq!(
            decode_canonical_patch(&bytes, limits),
            Err(PatchDecodeError::Value(DecodeError::CollectionLimit {
                limit: 1,
                attempted: 2,
            }))
        );
    }

    #[test]
    fn strict_decoder_rejects_malformed_tags_flags_trailing_bytes_and_truncation() {
        let (_, update_bytes) = canonical_patch_bytes(vec![field_update(1, Value::U128(8))]);

        let mut unknown_operation = update_bytes.clone();
        let Some(operation_tag) = unknown_operation.get_mut(44) else {
            panic!("missing operation tag");
        };
        *operation_tag = u8::MAX;
        assert_eq!(
            decode_canonical_patch(&unknown_operation, PatchDecodeLimits::default()),
            Err(PatchDecodeError::UnknownOperationTag(u8::MAX))
        );

        let mut unknown_path = update_bytes.clone();
        let Some(path_tag) = unknown_path.get_mut(53) else {
            panic!("missing path tag");
        };
        *path_tag = u8::MAX;
        assert_eq!(
            decode_canonical_patch(&unknown_path, PatchDecodeLimits::default()),
            Err(PatchDecodeError::UnknownPathTag(u8::MAX))
        );

        let insert_path = ValuePath::new(vec![PathSegment::Field(1)]);
        let encoded_path = match insert_path.canonical_bytes() {
            Ok(bytes) => bytes,
            Err(error) => panic!("path encoding failed: {error}"),
        };
        let insert = PatchOp::Insert {
            path: insert_path,
            map_key: None,
            value: Value::Bool(true),
        };
        let (_, mut invalid_flag) = canonical_patch_bytes(vec![insert]);
        let flag_offset = 44_usize
            .saturating_add(1)
            .saturating_add(4)
            .saturating_add(encoded_path.len());
        let Some(flag) = invalid_flag.get_mut(flag_offset) else {
            panic!("missing map-key flag");
        };
        *flag = 2;
        assert_eq!(
            decode_canonical_patch(&invalid_flag, PatchDecodeLimits::default()),
            Err(PatchDecodeError::InvalidMapKeyFlag(2))
        );

        let mut trailing = update_bytes.clone();
        trailing.push(0);
        assert!(matches!(
            decode_canonical_patch(&trailing, PatchDecodeLimits::default()),
            Err(PatchDecodeError::TrailingBytes { .. })
        ));

        let mut truncated = update_bytes;
        let _ = truncated.pop();
        assert!(matches!(
            decode_canonical_patch(&truncated, PatchDecodeLimits::default()),
            Err(PatchDecodeError::UnexpectedEnd { .. })
        ));
    }
}
