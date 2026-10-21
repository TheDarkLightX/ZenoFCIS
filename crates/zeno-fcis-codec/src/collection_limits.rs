//! Encoded collection bounds shared by interpreters and storage adapters.

/// Resource policy included in every bounded collection snapshot.
/// Limits count encoded bytes, not memory or execution work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollectionLimits {
    /// Maximum retained items.
    pub max_entries: u32,
    /// Maximum encoded item, including its framing.
    pub max_item_bytes: u64,
    /// Maximum complete snapshot, including version, kind, policy and count.
    pub max_snapshot_bytes: u64,
}
