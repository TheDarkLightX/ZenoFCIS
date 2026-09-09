//! Ordered map with immutable, shareable snapshots.
//!
//! Uses `rpds::RedBlackTreeMapSync` for structural sharing with ordered keys. Entries
//! are stored by encoded key and materialized in canonical order.

use alloc::boxed::Box;
use alloc::vec::Vec;

use super::{LogicalEntry, PersistentMap};
use crate::private::Sealed;

/// An ordered map whose immutable snapshots can be shared across threads.
///
/// Uses the pinned `rpds::RedBlackTreeMapSync` internally. Entries are keyed by
/// encoded key bytes and stored as `(key, value)` pairs. Updates share unchanged
/// tree nodes and take O(log n) tree operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderedMap {
    entries: rpds::RedBlackTreeMapSync<Box<[u8]>, (crate::Value, crate::Value)>,
}

impl Sealed for OrderedMap {}

impl OrderedMap {
    #[must_use]
    /// Creates an empty persistent map.
    pub fn new() -> Self {
        Self {
            entries: rpds::RedBlackTreeMap::new_sync(),
        }
    }
}

impl Default for OrderedMap {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentMap for OrderedMap {
    fn empty() -> Self {
        Self::new()
    }

    fn insert(&self, entry: LogicalEntry) -> Self {
        let (encoded_key, key, value) = entry.into_parts();
        Self {
            entries: self.entries.insert(encoded_key, (key, value)),
        }
    }

    fn remove(&self, encoded_key: &[u8]) -> Self {
        Self {
            entries: self.entries.remove(encoded_key),
        }
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
        self.entries
            .iter()
            .map(|(ek, (k, v))| LogicalEntry::from_stored_parts(ek.to_vec(), k.clone(), v.clone()))
            .collect()
    }
}
