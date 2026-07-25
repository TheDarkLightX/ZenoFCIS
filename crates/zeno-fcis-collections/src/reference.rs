//! Reference BTreeMap backend.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::{LogicalEntry, PersistentMap};

/// A persistent map backed by a standard `BTreeMap`.
///
/// This is the reference backend. It clones the entire map on every
/// modification, so it has no structural sharing. It is always available
/// and serves as the ground truth for differential testing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BTreeMapBackend {
    entries: BTreeMap<Box<[u8]>, (crate::Value, crate::Value)>,
}

impl BTreeMapBackend {
    #[must_use]
    /// Creates an empty BTreeMap backend.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

impl Default for BTreeMapBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentMap for BTreeMapBackend {
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
