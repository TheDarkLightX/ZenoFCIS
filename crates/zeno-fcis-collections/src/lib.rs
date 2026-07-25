//! Persistent collection adapters for ZenoFCIS.
//!
//! All backends produce the same logical entries and canonical bytes. Equality,
//! ordering, encoding, and roots are defined over logical entries, never over a
//! backend's internal shape. No backend is selected as default until explicit
//! performance and assurance promotion criteria pass.
//!
//! This crate is `no_std + alloc` and contains no `unsafe` code in the adapter
//! layer. Optional backend crates may contain their own `unsafe` code, but
//! their types are never exposed in the public adapter API.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;

use zeno_fcis_codec::{CanonicalEncode, EncodeError};
use zeno_fcis_value::{MapEntry, Value, ValueError};

// ---------------------------------------------------------------------------
// Logical entry
// ---------------------------------------------------------------------------

/// A logical map entry carrying the authoritative encoded key ordering.
///
/// This is the sole authority for map equality, ordering, and canonical
/// encoding. Backend-specific internal shapes are never used for protocol
/// meaning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicalEntry {
    encoded_key: Box<[u8]>,
    key: Value,
    value: Value,
}

impl LogicalEntry {
    /// Creates a logical entry.
    ///
    /// This constructor does not validate that `encoded_key` matches the
    /// canonical encoding of `key`. Use [`try_new`](Self::try_new) for
    /// validated construction.
    #[must_use]
    pub fn new(encoded_key: Vec<u8>, key: Value, value: Value) -> Self {
        Self {
            encoded_key: encoded_key.into_boxed_slice(),
            key,
            value,
        }
    }

    /// Creates a validated logical entry, checking that `encoded_key` matches
    /// the canonical encoding of `key`.
    ///
    /// This enforces the CBC invariant that the encoded key is the canonical
    /// encoding of the semantic key, preventing inconsistent entries that
    /// would fail at the codec boundary.
    pub fn try_new(encoded_key: Vec<u8>, key: Value, value: Value) -> Result<Self, MapError> {
        let expected = key.canonical_bytes()?;
        if encoded_key != expected {
            return Err(MapError::KeyEncodingMismatch {
                expected: expected.into_boxed_slice(),
                actual: encoded_key.into_boxed_slice(),
            });
        }
        Ok(Self {
            encoded_key: encoded_key.into_boxed_slice(),
            key,
            value,
        })
    }

    /// Returns the authoritative encoded key bytes.
    #[must_use]
    pub fn encoded_key(&self) -> &[u8] {
        &self.encoded_key
    }

    /// Returns the semantic key.
    #[must_use]
    pub fn key(&self) -> &Value {
        &self.key
    }

    /// Returns the semantic value.
    #[must_use]
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Consumes the entry and returns its parts.
    #[must_use]
    pub fn into_parts(self) -> (Box<[u8]>, Value, Value) {
        (self.encoded_key, self.key, self.value)
    }
}

// ---------------------------------------------------------------------------
// Map error
// ---------------------------------------------------------------------------

/// Map materialization or encoding failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MapError {
    /// Value map construction failed (duplicate or unsorted keys).
    ValueMap(ValueError),
    /// Canonical encoding failed.
    Encoding(EncodeError),
    /// Encoded key does not match the canonical encoding of the semantic key.
    KeyEncodingMismatch {
        /// Expected canonical encoding.
        expected: Box<[u8]>,
        /// Actual encoded key provided.
        actual: Box<[u8]>,
    },
}

impl From<ValueError> for MapError {
    fn from(error: ValueError) -> Self {
        Self::ValueMap(error)
    }
}

impl From<EncodeError> for MapError {
    fn from(error: EncodeError) -> Self {
        Self::Encoding(error)
    }
}

impl core::fmt::Display for MapError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ValueMap(error) => write!(formatter, "value map error: {error}"),
            Self::Encoding(error) => write!(formatter, "encoding error: {error}"),
            Self::KeyEncodingMismatch { expected, actual } => write!(
                formatter,
                "encoded key does not match canonical encoding of semantic key (expected {} bytes, got {} bytes)",
                expected.len(),
                actual.len()
            ),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MapError {}

// ---------------------------------------------------------------------------
// Persistent map trait
// ---------------------------------------------------------------------------

/// Sealed module preventing external implementations of [`PersistentMap`].
///
/// Only vetted backends within this crate can implement the trait, ensuring
/// that all backends pass differential testing and meet the authority
/// boundary requirements.
mod private {
    /// Sealed trait marker.
    pub trait Sealed {}
}

/// A persistent map with structural sharing.
///
/// All implementations must produce identical `Value::Map` values for the same
/// logical entries, regardless of insertion history. The canonical encoding
/// is defined over the materialized entries, not the backend's internal shape.
///
/// This trait is sealed: only vetted backends within this crate can implement
/// it, preventing untested external implementations from entering the
/// protocol boundary.
pub trait PersistentMap: Clone + private::Sealed {
    /// Creates an empty map.
    fn empty() -> Self;

    /// Returns a new map with the entry inserted or updated.
    fn insert(&self, entry: LogicalEntry) -> Self;

    /// Returns a new map with the entry removed, if present.
    fn remove(&self, encoded_key: &[u8]) -> Self;

    /// Returns the value for a key, if present.
    fn get(&self, encoded_key: &[u8]) -> Option<&Value>;

    /// Returns the number of entries.
    fn len(&self) -> usize;

    /// Returns true if the map is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Materializes entries in canonical (encoded-key-sorted) order.
    fn to_entries(&self) -> Vec<LogicalEntry>;

    /// Materializes a `Value::Map` in canonical order.
    ///
    /// Returns an error if entries are not in strict sorted order or have
    /// duplicate keys. Backends sort entries in `to_entries()`, so this only
    /// fails if the backend's sort is broken.
    fn try_to_value_map(&self) -> Result<Value, MapError> {
        let entries: Vec<MapEntry> = self
            .to_entries()
            .into_iter()
            .map(|e| MapEntry::new(e.encoded_key.to_vec(), e.key, e.value))
            .collect();
        Value::map_canonical(entries).map_err(MapError::from)
    }

    /// Materializes a `Value::Map` in canonical order.
    ///
    /// # Panics
    /// Panics if entries are not in strict sorted order or have duplicate keys.
    /// Use `try_to_value_map` for fallible construction.
    fn to_value_map(&self) -> Value {
        self.try_to_value_map()
            .unwrap_or_else(|e| panic!("materialize map: {e}"))
    }

    /// Returns canonical bytes for the materialized map.
    ///
    /// Returns an error if the map cannot be encoded.
    fn try_canonical_bytes(&self) -> Result<Vec<u8>, MapError> {
        self.try_to_value_map()?
            .canonical_bytes()
            .map_err(MapError::from)
    }

    /// Returns canonical bytes for the materialized map.
    ///
    /// # Panics
    /// Panics if the map cannot be encoded. Use `try_canonical_bytes` for
    /// fallible construction.
    fn canonical_bytes(&self) -> Vec<u8> {
        self.try_canonical_bytes()
            .unwrap_or_else(|e| panic!("canonical encode: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Reference BTreeMap backend
// ---------------------------------------------------------------------------

mod reference;

pub use reference::BTreeMapBackend;

// ---------------------------------------------------------------------------
// Optional backends
// ---------------------------------------------------------------------------

#[cfg(feature = "rpds-backend")]
mod rpds_backend;

#[cfg(feature = "rpds-backend")]
pub use rpds_backend::RpdsBackend;

#[cfg(feature = "imbl-backend")]
mod imbl_backend;

#[cfg(feature = "imbl-backend")]
pub use imbl_backend::ImblBackend;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(missing_docs)]
mod tests;
