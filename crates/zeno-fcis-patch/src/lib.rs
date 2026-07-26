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

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_value::{Field, MapEntry, Value, ValueError};

const PATH_TAG_FIELD: u8 = 0;
const PATH_TAG_TUPLE: u8 = 1;
const PATH_TAG_VECTOR: u8 = 2;
const PATH_TAG_SUM_PAYLOAD: u8 = 3;
const PATH_TAG_MAP_KEY: u8 = 4;
const PATCH_TAG_INSERT: u8 = 0;
const PATCH_TAG_UPDATE: u8 = 1;
const PATCH_TAG_DELETE: u8 = 2;

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
}
