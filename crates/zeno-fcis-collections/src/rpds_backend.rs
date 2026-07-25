//! rpds persistent vector backend.
//!
//! Uses `rpds::Vector` as a builder with structural sharing. Entries are
//! materialized in canonical (encoded-key-sorted) order on `to_entries`.

use alloc::vec::Vec;

use super::{LogicalEntry, PersistentMap};

/// A persistent map backed by `rpds::Vector`.
///
/// Entries are stored in insertion order and sorted on materialization. This
/// backend provides structural sharing: `insert` and `remove` return new maps
/// that share structure with the original.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RpdsBackend {
    entries: rpds::Vector<LogicalEntry>,
}

impl RpdsBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: rpds::Vector::new(),
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
        let mut next = self.clone();
        let encoded_key = entry.encoded_key().to_vec();
        next.entries = next
            .entries
            .iter()
            .filter(|e| e.encoded_key() != encoded_key.as_slice())
            .cloned()
            .collect();
        next.entries.push_back_mut(entry);
        next
    }

    fn remove(&self, encoded_key: &[u8]) -> Self {
        let mut next = self.clone();
        next.entries = next
            .entries
            .iter()
            .filter(|e| e.encoded_key() != encoded_key)
            .cloned()
            .collect();
        next
    }

    fn get(&self, encoded_key: &[u8]) -> Option<&crate::Value> {
        self.entries
            .iter()
            .find(|e| e.encoded_key() == encoded_key)
            .map(|e| e.value())
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn to_entries(&self) -> Vec<LogicalEntry> {
        let mut entries: Vec<LogicalEntry> = self.entries.iter().cloned().collect();
        entries.sort_by(|a, b| a.encoded_key().cmp(b.encoded_key()));
        entries
    }
}
