//! imbl persistent map backend.
//!
//! Uses `imbl::OrdMap` for structural sharing with ordered keys. Entries
//! are stored by encoded key and materialized in canonical order.

use alloc::vec::Vec;

use super::{LogicalEntry, PersistentMap};
use crate::private::Sealed;

/// A persistent map backed by `imbl::OrdMap`.
///
/// Entries are keyed by encoded key bytes and stored as `(key, value)` pairs.
/// This backend provides structural sharing with O(log n) operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImblBackend {
    entries: imbl::OrdMap<Box<[u8]>, (crate::Value, crate::Value)>,
}

impl Sealed for ImblBackend {}

impl ImblBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: imbl::OrdMap::new(),
        }
    }
}

impl Default for ImblBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentMap for ImblBackend {
    fn empty() -> Self {
        Self::new()
    }

    fn insert(&self, entry: LogicalEntry) -> Self {
        let mut next = self.clone();
        let (encoded_key, key, value) = entry.into_parts();
        next.entries.insert(encoded_key, (key, value));
        next
    }

    fn remove(&self, encoded_key: &[u8]) -> Self {
        let mut next = self.clone();
        next.entries.remove(encoded_key);
        next
    }

    fn get(&self, encoded_key: &[u8]) -> Option<&crate::Value> {
        self.entries.get(encoded_key).map(|(_, v)| v)
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn to_entries(&self) -> Vec<LogicalEntry> {
        self.entries
            .iter()
            .map(|(ek, (k, v))| LogicalEntry::new(ek.to_vec(), k.clone(), v.clone()))
            .collect()
    }
}
