//! Immutable bounded collections with versioned, policy-bound snapshots.
use crate::{LogicalEntry, MapError, PersistentMap};
use alloc::vec::Vec;
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_value::{Value, ValueError, ValueLimits};

const HEADER: u64 = 29;

/// Resource policy included in every snapshot. Limits count encoded bytes, not RAM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollectionLimits {
    /// Maximum retained items.
    pub max_entries: u32,
    /// Maximum encoded item, including framing (map: both keys and values).
    pub max_item_bytes: u64,
    /// Maximum complete snapshot, including version, kind, policy and count.
    pub max_snapshot_bytes: u64,
}

/// Deterministic refusal; the original collection is never modified.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundedError {
    /// The empty snapshot cannot fit its policy.
    InvalidLimits,
    /// Incoming value fails the existing default Value structural limits.
    Value(ValueError),
    /// Canonical encoding or materialization failed.
    Map(MapError),
    /// Checked byte/count arithmetic overflowed.
    ArithmeticOverflow,
    /// The item's complete encoded framing exceeds its limit.
    ItemTooLarge,
    /// A new item would exceed the entry limit.
    Full,
    /// The complete candidate snapshot exceeds its byte limit.
    SnapshotTooLarge,
    /// A FIFO has no head.
    Empty,
}
impl From<ValueError> for BoundedError {
    fn from(e: ValueError) -> Self {
        Self::Value(e)
    }
}
impl From<MapError> for BoundedError {
    fn from(e: MapError) -> Self {
        Self::Map(e)
    }
}

pub(crate) fn add(a: u64, b: u64) -> Result<u64, BoundedError> {
    a.checked_add(b).ok_or(BoundedError::ArithmeticOverflow)
}
pub(crate) fn value_size(v: &Value) -> Result<u64, BoundedError> {
    v.validate_limits(ValueLimits::default())?;
    u64::try_from(v.encoded_length()?).map_err(|_| BoundedError::ArithmeticOverflow)
}
pub(crate) fn check(
    l: CollectionLimits,
    count: usize,
    item: u64,
    total: u64,
) -> Result<(), BoundedError> {
    if item > l.max_item_bytes {
        return Err(BoundedError::ItemTooLarge);
    }
    if count > l.max_entries as usize {
        return Err(BoundedError::Full);
    }
    if total > l.max_snapshot_bytes {
        return Err(BoundedError::SnapshotTooLarge);
    }
    Ok(())
}
pub(crate) fn header(kind: u8, l: CollectionLimits, count: usize) -> Result<Vec<u8>, BoundedError> {
    let count = u32::try_from(count).map_err(|_| BoundedError::ArithmeticOverflow)?;
    let mut out = Vec::from(&b"ZBC1"[..]);
    out.push(kind);
    out.extend_from_slice(&l.max_entries.to_be_bytes());
    out.extend_from_slice(&l.max_item_bytes.to_be_bytes());
    out.extend_from_slice(&l.max_snapshot_bytes.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    Ok(out)
}
pub(crate) fn blob(out: &mut Vec<u8>, v: &Value) -> Result<(), BoundedError> {
    let bytes = v.canonical_bytes().map_err(MapError::from)?;
    let n = u64::try_from(bytes.len()).map_err(|_| BoundedError::ArithmeticOverflow)?;
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&bytes);
    Ok(())
}

/// Capacity and byte-bound immutable wrapper over an existing sealed backend.
#[derive(Clone)]
pub struct BoundedMap<M: PersistentMap> {
    map: M,
    limits: CollectionLimits,
    bytes: u64,
}
impl<M: PersistentMap> BoundedMap<M> {
    /// Creates an empty map, refusing policies smaller than the 29-byte header.
    pub fn empty(limits: CollectionLimits) -> Result<Self, BoundedError> {
        if limits.max_snapshot_bytes < HEADER {
            return Err(BoundedError::InvalidLimits);
        }
        Ok(Self {
            map: M::empty(),
            limits,
            bytes: HEADER,
        })
    }
    /// Returns the retained policy.
    pub fn limits(&self) -> CollectionLimits {
        self.limits
    }
    /// Returns the number of logical entries.
    pub fn len(&self) -> usize {
        self.map.len()
    }
    /// Returns whether no entries are retained.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    /// Returns exact snapshot bytes, including policy and framing.
    pub fn snapshot_len(&self) -> u64 {
        self.bytes
    }
    /// Looks up an already canonical encoded key.
    pub fn get(&self, key: &[u8]) -> Option<&Value> {
        self.map.get(key)
    }
    /// Returns logical entries in canonical encoded-key order.
    pub fn entries(&self) -> Vec<LogicalEntry> {
        self.map.to_entries()
    }
    /// Validates the value, then item bytes, capacity and full snapshot bytes.
    /// Replacement at capacity is permitted. Refusal preserves every old snapshot.
    pub fn insert(&self, entry: LogicalEntry) -> Result<Self, BoundedError> {
        let key = u64::try_from(entry.encoded_key().len())
            .map_err(|_| BoundedError::ArithmeticOverflow)?;
        let size = add(add(16, key)?, value_size(entry.value())?)?;
        let old = self.map.get(entry.encoded_key());
        let old_size = match old {
            Some(v) => add(add(16, key)?, value_size(v)?)?,
            None => 0,
        };
        let count = self
            .len()
            .checked_add(usize::from(old.is_none()))
            .ok_or(BoundedError::ArithmeticOverflow)?;
        let bytes = add(
            self.bytes
                .checked_sub(old_size)
                .ok_or(BoundedError::ArithmeticOverflow)?,
            size,
        )?;
        check(self.limits, count, size, bytes)?;
        Ok(Self {
            map: self.map.insert(entry),
            limits: self.limits,
            bytes,
        })
    }
    /// Removes a key; a missing key is an immutable no-op.
    pub fn remove(&self, key: &[u8]) -> Result<Self, BoundedError> {
        let Some(v) = self.map.get(key) else {
            return Ok(self.clone());
        };
        let n = u64::try_from(key.len()).map_err(|_| BoundedError::ArithmeticOverflow)?;
        let size = add(add(16, n)?, value_size(v)?)?;
        Ok(Self {
            map: self.map.remove(key),
            limits: self.limits,
            bytes: self
                .bytes
                .checked_sub(size)
                .ok_or(BoundedError::ArithmeticOverflow)?,
        })
    }
    /// Encodes ZBC1 kind 1, policy, count, then length-prefixed canonical key/value pairs.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, BoundedError> {
        self.encode_kind(1)
    }
    fn encode_kind(&self, kind: u8) -> Result<Vec<u8>, BoundedError> {
        let mut out = header(kind, self.limits, self.len())?;
        for entry in self.entries() {
            blob(&mut out, entry.key())?;
            blob(&mut out, entry.value())?;
        }
        Ok(out)
    }
}

/// Immutable bounded set backed by canonical map keys and unit values.
#[derive(Clone)]
pub struct BoundedSet<M: PersistentMap> {
    map: BoundedMap<M>,
}
impl<M: PersistentMap> BoundedSet<M> {
    /// Creates an empty set with the map's framing/resource rules.
    pub fn empty(limits: CollectionLimits) -> Result<Self, BoundedError> {
        Ok(Self {
            map: BoundedMap::empty(limits)?,
        })
    }
    /// Number of retained distinct keys.
    pub fn len(&self) -> usize {
        self.map.len()
    }
    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    /// Inserts a canonical key; duplicates succeed even when full.
    pub fn insert(&self, key: Value) -> Result<Self, BoundedError> {
        let size = add(17, value_size(&key)?)?;
        if size > self.map.limits.max_item_bytes {
            return Err(BoundedError::ItemTooLarge);
        }
        let bytes = key.canonical_bytes().map_err(MapError::from)?;
        let entry = LogicalEntry::try_new(bytes, key, Value::unit())?;
        Ok(Self {
            map: self.map.insert(entry)?,
        })
    }
    /// Removes a canonical encoded key.
    pub fn remove(&self, key: &[u8]) -> Result<Self, BoundedError> {
        Ok(Self {
            map: self.map.remove(key)?,
        })
    }
    /// Returns whether a canonical encoded key is present.
    pub fn contains(&self, key: &[u8]) -> bool {
        self.map.get(key).is_some()
    }
    /// Returns keys in canonical encoded-key order.
    pub fn keys(&self) -> Vec<Value> {
        self.map
            .entries()
            .into_iter()
            .map(|e| e.key().clone())
            .collect()
    }
    /// Encodes ZBC1 kind 2; unit values remain explicit in its map-shaped body.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, BoundedError> {
        self.map.encode_kind(2)
    }
}

/// Immutable FIFO. Ordering is insertion order, never canonical-key sort order.
///
/// Callers cannot forge the retained byte measurement or bypass admission:
/// ```compile_fail
/// use zeno_fcis_collections::bounded::{BoundedFifo, CollectionLimits};
/// let limits = CollectionLimits {
///     max_entries: 0, max_item_bytes: 0, max_snapshot_bytes: 0,
/// };
/// let _ = BoundedFifo { items: vec![], limits, bytes: 0 };
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedFifo {
    items: Vec<Value>,
    limits: CollectionLimits,
    bytes: u64,
}
impl BoundedFifo {
    /// Creates an empty queue.
    pub fn empty(limits: CollectionLimits) -> Result<Self, BoundedError> {
        if limits.max_snapshot_bytes < HEADER {
            return Err(BoundedError::InvalidLimits);
        }
        Ok(Self {
            items: Vec::new(),
            limits,
            bytes: HEADER,
        })
    }
    /// Returns items in FIFO order without mutable aliases.
    pub fn items(&self) -> &[Value] {
        &self.items
    }
    /// Exact complete encoded snapshot length.
    pub fn snapshot_len(&self) -> u64 {
        self.bytes
    }
    /// Returns the head or a typed empty refusal.
    pub fn head(&self) -> Result<&Value, BoundedError> {
        self.items.first().ok_or(BoundedError::Empty)
    }
    /// Appends after structural, item, capacity and snapshot-byte validation.
    pub fn push(&self, value: Value) -> Result<Self, BoundedError> {
        let size = add(8, value_size(&value)?)?;
        let bytes = add(self.bytes, size)?;
        let count = self
            .items
            .len()
            .checked_add(1)
            .ok_or(BoundedError::ArithmeticOverflow)?;
        check(self.limits, count, size, bytes)?;
        let mut items = self.items.clone();
        items.push(value);
        Ok(Self {
            items,
            limits: self.limits,
            bytes,
        })
    }
    /// Returns the original head and next queue; the original stays valid.
    pub fn pop(&self) -> Result<(Value, Self), BoundedError> {
        let head = self.head()?;
        let size = add(8, value_size(head)?)?;
        let bytes = self
            .bytes
            .checked_sub(size)
            .ok_or(BoundedError::ArithmeticOverflow)?;
        Ok((
            head.clone(),
            Self {
                items: self.items[1..].to_vec(),
                limits: self.limits,
                bytes,
            },
        ))
    }
    /// Encodes ZBC1 kind 3, policy, count and length-prefixed FIFO values.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, BoundedError> {
        let mut out = header(3, self.limits, self.items.len())?;
        for value in &self.items {
            blob(&mut out, value)?;
        }
        Ok(out)
    }
}
