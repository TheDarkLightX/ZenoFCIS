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

use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_value::{MapEntry, Value};

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
    #[must_use]
    pub fn new(encoded_key: Vec<u8>, key: Value, value: Value) -> Self {
        Self {
            encoded_key: encoded_key.into_boxed_slice(),
            key,
            value,
        }
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
// Persistent map trait
// ---------------------------------------------------------------------------

/// A persistent map with structural sharing.
///
/// All implementations must produce identical `Value::Map` values for the same
/// logical entries, regardless of insertion history. The canonical encoding
/// is defined over the materialized entries, not the backend's internal shape.
pub trait PersistentMap: Clone {
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
    fn to_value_map(&self) -> Value {
        let entries: Vec<MapEntry> = self
            .to_entries()
            .into_iter()
            .map(|e| MapEntry::new(e.encoded_key.to_vec(), e.key, e.value))
            .collect();
        Value::map_canonical(entries).unwrap_or_else(|e| panic!("materialize map: {e}"))
    }

    /// Returns canonical bytes for the materialized map.
    fn canonical_bytes(&self) -> Vec<u8> {
        self.to_value_map()
            .canonical_bytes()
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
