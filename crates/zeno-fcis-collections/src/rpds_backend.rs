//! rpds persistent map backend.
//!
//! Uses `rpds::HashTrieMap` for O(log n) structural sharing. Entries are
//! keyed by encoded key bytes and materialized in canonical order.

use alloc::vec::Vec;

use super::{LogicalEntry, PersistentMap};
use crate::private::Sealed;

/// A persistent map backed by `rpds::HashTrieMap`.
///
/// Entries are keyed by encoded key bytes. This backend provides structural
/// sharing with O(log n) insert and remove operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RpdsBackend {
    entries: rpds::HashTrieMap<Vec<u8>, (crate::Value, crate::Value)>,
}

impl Sealed for RpdsBackend {}

impl RpdsBackend {
    /// Creates an empty rpds backend.
    pub fn new() -> Self {
        Self {
            entries: rpds::HashTrieMap::new(),
        }
    }
}

impl Default for RpdsBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentMap for RpdsBackend {
    fn empty() -> Self {
        Self::new()
    }

    fn insert(&self, entry: LogicalEntry) -> Self {
        let (encoded_key, key, value) = entry.into_parts();
        let mut next = self.clone();
        next.entries = next.entries.insert(encoded_key.to_vec(), (key, value));
        next
    }

    fn remove(&self, encoded_key: &[u8]) -> Self {
        let mut next = self.clone();
        next.entries = next.entries.remove(encoded_key);
        next
    }

    fn get(&self, encoded_key: &[u8]) -> Option<&crate::Value> {
        self.entries.get(encoded_key).map(|(_, v)| v)
    }

    fn len(&self) -> usize {
        self.entries.size()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn to_entries(&self) -> Vec<LogicalEntry> {
        let mut entries: Vec<LogicalEntry> = self
            .entries
            .iter()
            .map(|(ek, (k, v))| LogicalEntry::new(ek.clone(), k.clone(), v.clone()))
            .collect();
        entries.sort_by(|a, b| a.encoded_key().cmp(b.encoded_key()));
        entries
    }
}
