//! Differential tests for persistent collection backends.
//!
//! Every backend must produce identical logical entries, canonical bytes, and
//! `Value::Map` values for the same operations, regardless of insertion
//! history. These tests verify insertion-history independence, deletion,
//! zero-removal, alias resistance, snapshot retention, and old-version
//! stability.

use super::*;
use alloc::string::String;

fn encoded_key_for(byte: u8) -> Vec<u8> {
    Value::U128(u128::from(byte))
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode key: {e}"))
}

fn encoded_key_for_text(text: &str) -> Vec<u8> {
    Value::text_ascii(String::from(text))
        .unwrap_or_else(|e| panic!("text: {e}"))
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode key: {e}"))
}

fn make_entry(key_byte: u8, value_byte: u8) -> LogicalEntry {
    let key = Value::U128(u128::from(key_byte));
    let encoded_key = key
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode key: {e}"));
    LogicalEntry::try_new(encoded_key, key, Value::U128(u128::from(value_byte)))
        .unwrap_or_else(|error| panic!("logical entry: {error}"))
}

fn make_entry_text(key_text: &str, value_byte: u8) -> LogicalEntry {
    let key = Value::text_ascii(String::from(key_text)).unwrap_or_else(|e| panic!("text: {e}"));
    let encoded_key = key
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode key: {e}"));
    LogicalEntry::try_new(encoded_key, key, Value::U128(u128::from(value_byte)))
        .unwrap_or_else(|error| panic!("logical entry: {error}"))
}

// ---------------------------------------------------------------------------
// BTreeMapBackend: basic operations
// ---------------------------------------------------------------------------

#[test]
fn btreemap_empty_is_empty() {
    let map = BTreeMapBackend::empty();
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
}

#[test]
fn btreemap_insert_and_get() {
    let map = BTreeMapBackend::empty().insert(make_entry(1, 10));
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(&encoded_key_for(1)), Some(&Value::U128(10)));
}

#[test]
fn btreemap_remove_returns_empty() {
    let map = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .remove(&encoded_key_for(1));
    assert!(map.is_empty());
}

#[test]
fn btreemap_update_replaces_value() {
    let map = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(1, 20));
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(&encoded_key_for(1)), Some(&Value::U128(20)));
}

// ---------------------------------------------------------------------------
// Insertion-history independence
// ---------------------------------------------------------------------------

#[test]
fn btreemap_insertion_history_independence() {
    let order_a = BTreeMapBackend::empty()
        .insert(make_entry(3, 30))
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20));
    let order_b = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20))
        .insert(make_entry(3, 30));
    assert_eq!(order_a.to_entries(), order_b.to_entries());
    assert_eq!(order_a.canonical_bytes(), order_b.canonical_bytes());
    assert_eq!(order_a.to_value_map(), order_b.to_value_map());
}

// ---------------------------------------------------------------------------
// Deletion and zero-removal
// ---------------------------------------------------------------------------

#[test]
fn btreemap_remove_nonexistent_is_noop() {
    let map = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .remove(&encoded_key_for(99));
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(&encoded_key_for(1)), Some(&Value::U128(10)));
}

#[test]
fn btreemap_remove_all_yields_empty_entries() {
    let map = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20))
        .remove(&encoded_key_for(1))
        .remove(&encoded_key_for(2));
    assert!(map.is_empty());
    assert!(map.to_entries().is_empty());
}

// ---------------------------------------------------------------------------
// Alias resistance and snapshot retention
// ---------------------------------------------------------------------------

#[test]
fn btreemap_insert_does_not_mutate_original() {
    let original = BTreeMapBackend::empty().insert(make_entry(1, 10));
    let modified = original.insert(make_entry(2, 20));
    assert_eq!(original.len(), 1);
    assert_eq!(modified.len(), 2);
}

#[test]
fn btreemap_remove_does_not_mutate_original() {
    let original = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20));
    let modified = original.remove(&encoded_key_for(1));
    assert_eq!(original.len(), 2);
    assert_eq!(modified.len(), 1);
}

#[test]
fn btreemap_snapshot_retention() {
    let v1 = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20));
    let v2 = v1.insert(make_entry(3, 30));
    let v3 = v2.remove(&encoded_key_for(1));
    assert_eq!(v1.len(), 2);
    assert_eq!(v2.len(), 3);
    assert_eq!(v3.len(), 2);
    assert!(v1.get(&encoded_key_for(3)).is_none());
    assert!(v2.get(&encoded_key_for(3)).is_some());
    assert!(v3.get(&encoded_key_for(1)).is_none());
}

#[test]
fn btreemap_old_version_stability() {
    let v1 = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20));
    let v1_bytes = v1.canonical_bytes();
    let _v2 = v1.insert(make_entry(3, 30));
    assert_eq!(v1.canonical_bytes(), v1_bytes);
}

// ---------------------------------------------------------------------------
// Canonical encoding consistency
// ---------------------------------------------------------------------------

#[test]
fn btreemap_canonical_bytes_deterministic() {
    let map = BTreeMapBackend::empty()
        .insert(make_entry_text("a", 1))
        .insert(make_entry_text("b", 2))
        .insert(make_entry_text("c", 3));
    let bytes_a = map.canonical_bytes();
    let bytes_b = map.canonical_bytes();
    assert_eq!(bytes_a, bytes_b);
}

#[test]
fn btreemap_entries_are_sorted_by_encoded_key() {
    let map = BTreeMapBackend::empty()
        .insert(make_entry_text("c", 3))
        .insert(make_entry_text("a", 1))
        .insert(make_entry_text("b", 2));
    let entries = map.to_entries();
    assert_eq!(
        entries[0].encoded_key(),
        encoded_key_for_text("a").as_slice()
    );
    assert_eq!(
        entries[1].encoded_key(),
        encoded_key_for_text("b").as_slice()
    );
    assert_eq!(
        entries[2].encoded_key(),
        encoded_key_for_text("c").as_slice()
    );
}

// ---------------------------------------------------------------------------
// Differential tests: BTreeMap vs reference model
// ---------------------------------------------------------------------------

#[test]
fn btreemap_differential_insert_remove_sequence() {
    let mut reference = BTreeMapBackend::empty();
    let mut subject = BTreeMapBackend::empty();
    for i in 1..=10u8 {
        let entry = make_entry(i, i * 2);
        reference = reference.insert(entry.clone());
        subject = subject.insert(entry);
        assert_eq!(reference.to_entries(), subject.to_entries());
    }
    for i in (1..=10u8).step_by(2) {
        reference = reference.remove(&encoded_key_for(i));
        subject = subject.remove(&encoded_key_for(i));
        assert_eq!(reference.to_entries(), subject.to_entries());
    }
    assert_eq!(reference.canonical_bytes(), subject.canonical_bytes());
}

// ---------------------------------------------------------------------------
// Optional rpds backend differential tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "rpds-backend"))]
mod rpds_tests {
    use super::*;

    #[test]
    fn rpds_empty_is_empty() {
        let map = RpdsBackend::empty();
        assert!(map.is_empty());
    }

    #[test]
    fn rpds_insert_and_get() {
        let map = RpdsBackend::empty().insert(make_entry(1, 10));
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&encoded_key_for(1)), Some(&Value::U128(10)));
    }

    #[test]
    fn rpds_insertion_history_independence() {
        let order_a = RpdsBackend::empty()
            .insert(make_entry(3, 30))
            .insert(make_entry(1, 10))
            .insert(make_entry(2, 20));
        let order_b = RpdsBackend::empty()
            .insert(make_entry(1, 10))
            .insert(make_entry(2, 20))
            .insert(make_entry(3, 30));
        assert_eq!(order_a.to_entries(), order_b.to_entries());
        assert_eq!(order_a.canonical_bytes(), order_b.canonical_bytes());
    }

    #[test]
    fn rpds_snapshot_retention() {
        let v1 = RpdsBackend::empty()
            .insert(make_entry(1, 10))
            .insert(make_entry(2, 20));
        let v2 = v1.insert(make_entry(3, 30));
        assert_eq!(v1.len(), 2);
        assert_eq!(v2.len(), 3);
        assert!(v1.get(&encoded_key_for(3)).is_none());
    }

    #[test]
    fn rpds_differential_vs_btreemap() {
        let mut btree = BTreeMapBackend::empty();
        let mut rpds = RpdsBackend::empty();
        for i in 1..=20u8 {
            let entry = make_entry(i, i * 3);
            btree = btree.insert(entry.clone());
            rpds = rpds.insert(entry);
        }
        assert_eq!(btree.to_entries(), rpds.to_entries());
        assert_eq!(btree.canonical_bytes(), rpds.canonical_bytes());
        for i in (1..=20u8).filter(|i| i % 3 == 0) {
            btree = btree.remove(&encoded_key_for(i));
            rpds = rpds.remove(&encoded_key_for(i));
        }
        assert_eq!(btree.to_entries(), rpds.to_entries());
        assert_eq!(btree.canonical_bytes(), rpds.canonical_bytes());
    }
}

// ---------------------------------------------------------------------------
// Optional imbl backend differential tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "imbl-backend"))]
mod imbl_tests {
    use super::*;

    #[test]
    fn imbl_empty_is_empty() {
        let map = ImblBackend::empty();
        assert!(map.is_empty());
    }

    #[test]
    fn imbl_insert_and_get() {
        let map = ImblBackend::empty().insert(make_entry(1, 10));
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&encoded_key_for(1)), Some(&Value::U128(10)));
    }

    #[test]
    fn imbl_insertion_history_independence() {
        let order_a = ImblBackend::empty()
            .insert(make_entry(3, 30))
            .insert(make_entry(1, 10))
            .insert(make_entry(2, 20));
        let order_b = ImblBackend::empty()
            .insert(make_entry(1, 10))
            .insert(make_entry(2, 20))
            .insert(make_entry(3, 30));
        assert_eq!(order_a.to_entries(), order_b.to_entries());
        assert_eq!(order_a.canonical_bytes(), order_b.canonical_bytes());
    }

    #[test]
    fn imbl_snapshot_retention() {
        let v1 = ImblBackend::empty()
            .insert(make_entry(1, 10))
            .insert(make_entry(2, 20));
        let v2 = v1.insert(make_entry(3, 30));
        assert_eq!(v1.len(), 2);
        assert_eq!(v2.len(), 3);
        assert!(v1.get(&encoded_key_for(3)).is_none());
    }

    #[test]
    fn imbl_differential_vs_btreemap() {
        let mut btree = BTreeMapBackend::empty();
        let mut imbl = ImblBackend::empty();
        for i in 1..=20u8 {
            let entry = make_entry(i, i * 3);
            btree = btree.insert(entry.clone());
            imbl = imbl.insert(entry);
        }
        assert_eq!(btree.to_entries(), imbl.to_entries());
        assert_eq!(btree.canonical_bytes(), imbl.canonical_bytes());
        for i in (1..=20u8).filter(|i| i % 3 == 0) {
            btree = btree.remove(&encoded_key_for(i));
            imbl = imbl.remove(&encoded_key_for(i));
        }
        assert_eq!(btree.to_entries(), imbl.to_entries());
        assert_eq!(btree.canonical_bytes(), imbl.canonical_bytes());
    }
}

// ---------------------------------------------------------------------------
// Fallible materialization tests
// ---------------------------------------------------------------------------

#[test]
fn btreemap_try_to_value_map_succeeds() {
    let map = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20));
    let result = map.try_to_value_map();
    assert!(result.is_ok());
    let value = result.unwrap_or_else(|e| panic!("value map: {e}"));
    assert_eq!(value.kind(), zeno_fcis_value::ValueKind::Map);
}

#[test]
fn btreemap_try_canonical_bytes_succeeds() {
    let map = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20));
    let result = map.try_canonical_bytes();
    assert!(result.is_ok());
    let bytes = result.unwrap_or_else(|e| panic!("canonical: {e}"));
    assert!(!bytes.is_empty());
}

#[test]
fn btreemap_empty_map_value_is_map() {
    let map = BTreeMapBackend::empty();
    let value = map.to_value_map();
    assert_eq!(value.kind(), zeno_fcis_value::ValueKind::Map);
}

#[derive(Clone)]
struct CorruptStoredKeyBackend;

impl super::private::Sealed for CorruptStoredKeyBackend {}

impl PersistentMap for CorruptStoredKeyBackend {
    fn empty() -> Self {
        Self
    }

    fn insert(&self, _entry: LogicalEntry) -> Self {
        self.clone()
    }

    fn remove(&self, _encoded_key: &[u8]) -> Self {
        self.clone()
    }

    fn get(&self, _encoded_key: &[u8]) -> Option<&Value> {
        None
    }

    fn len(&self) -> usize {
        1
    }

    fn to_entries(&self) -> Vec<LogicalEntry> {
        vec![LogicalEntry::from_stored_parts(
            vec![0xff],
            Value::U128(42),
            Value::Unit,
        )]
    }
}

#[test]
fn materialization_rejects_a_backend_stored_key_mismatch() {
    let result = CorruptStoredKeyBackend.try_to_value_map();
    assert!(matches!(result, Err(MapError::KeyEncodingMismatch { .. })));
}

// ---------------------------------------------------------------------------
// LogicalEntry::try_new validation tests
// ---------------------------------------------------------------------------

#[test]
fn logical_entry_try_new_accepts_matching_encoding() {
    let key = Value::U128(42);
    let encoded = key
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode: {e}"));
    let result = LogicalEntry::try_new(encoded, key.clone(), Value::U128(99));
    assert!(result.is_ok());
    let entry = result.unwrap_or_else(|e| panic!("entry: {e}"));
    assert_eq!(entry.key(), &Value::U128(42));
}

#[test]
fn logical_entry_try_new_rejects_mismatched_encoding() {
    let key = Value::U128(42);
    let wrong_encoded = vec![0_u8, 1, 2, 3];
    let result = LogicalEntry::try_new(wrong_encoded, key, Value::U128(99));
    assert!(matches!(result, Err(MapError::KeyEncodingMismatch { .. })));
}

#[test]
fn logical_entry_try_new_rejects_empty_encoding() {
    let key = Value::U128(42);
    let result = LogicalEntry::try_new(vec![], key, Value::U128(99));
    assert!(matches!(result, Err(MapError::KeyEncodingMismatch { .. })));
}

// ---------------------------------------------------------------------------
// Canonical round-trip test
// ---------------------------------------------------------------------------

#[test]
fn btreemap_canonical_bytes_round_trip_is_stable() {
    let map = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20))
        .insert(make_entry(3, 30));
    let bytes_v1 = map.canonical_bytes();
    let bytes_v2 = map.canonical_bytes();
    assert_eq!(bytes_v1, bytes_v2, "canonical bytes must be deterministic");
    assert!(
        bytes_v1.len() > 10,
        "canonical bytes must contain map header and entries"
    );
}

#[test]
fn btreemap_canonical_bytes_differ_for_different_maps() {
    let map_a = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20));
    let map_b = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 99));
    assert_ne!(
        map_a.canonical_bytes(),
        map_b.canonical_bytes(),
        "different values must produce different canonical bytes"
    );
}

// ---------------------------------------------------------------------------
// Property-based insertion-history independence tests
// ---------------------------------------------------------------------------

#[test]
fn btreemap_insertion_order_independence_all_permutations() {
    let entries = vec![
        make_entry(1, 10),
        make_entry(2, 20),
        make_entry(3, 30),
        make_entry(4, 40),
        make_entry(5, 50),
    ];
    let permutations = [
        [0_usize, 1, 2, 3, 4],
        [4, 3, 2, 1, 0],
        [2, 0, 4, 1, 3],
        [3, 1, 4, 2, 0],
        [1, 3, 0, 2, 4],
        [4, 0, 3, 1, 2],
        [0, 4, 1, 3, 2],
        [2, 4, 0, 3, 1],
    ];
    let reference = {
        let mut map = BTreeMapBackend::empty();
        for entry in &entries {
            map = map.insert(entry.clone());
        }
        map.canonical_bytes()
    };
    for perm in &permutations {
        let mut map = BTreeMapBackend::empty();
        for &idx in perm {
            map = map.insert(entries[idx].clone());
        }
        assert_eq!(
            map.canonical_bytes(),
            reference,
            "insertion order {:?} produced different canonical bytes",
            perm
        );
    }
}

#[test]
fn btreemap_remove_then_reinsert_is_identity() {
    let original = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20))
        .insert(make_entry(3, 30));
    let original_bytes = original.canonical_bytes();
    let key = encoded_key_for(2);
    let after_remove = original.remove(&key);
    let after_reinsert = after_remove.insert(make_entry(2, 20));
    assert_eq!(
        after_reinsert.canonical_bytes(),
        original_bytes,
        "remove then reinsert must produce identical canonical bytes"
    );
}

#[test]
fn btreemap_update_value_changes_bytes_but_key_set_stable() {
    let original = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20));
    let updated = original.insert(make_entry(2, 99));
    assert_ne!(
        original.canonical_bytes(),
        updated.canonical_bytes(),
        "updating a value must change canonical bytes"
    );
    assert_eq!(
        original.len(),
        updated.len(),
        "updating a value must not change entry count"
    );
    let original_entries = original.to_entries();
    let original_keys: Vec<Vec<u8>> = original_entries
        .iter()
        .map(|e| e.encoded_key().to_vec())
        .collect();
    let updated_entries = updated.to_entries();
    let updated_keys: Vec<Vec<u8>> = updated_entries
        .iter()
        .map(|e| e.encoded_key().to_vec())
        .collect();
    assert_eq!(
        original_keys, updated_keys,
        "key set must be stable after update"
    );
}

#[test]
fn btreemap_multiple_removes_are_cumulative() {
    let map = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20))
        .insert(make_entry(3, 30))
        .insert(make_entry(4, 40));
    let after_one = map.remove(&encoded_key_for(1));
    assert_eq!(after_one.len(), 3);
    let after_two = after_one.remove(&encoded_key_for(3));
    assert_eq!(after_two.len(), 2);
    let after_three = after_two.remove(&encoded_key_for(2));
    assert_eq!(after_three.len(), 1);
    let after_four = after_three.remove(&encoded_key_for(4));
    assert_eq!(after_four.len(), 0);
    assert!(after_four.is_empty());
}

#[test]
fn btreemap_get_after_remove_returns_none() {
    let map = BTreeMapBackend::empty()
        .insert(make_entry(1, 10))
        .insert(make_entry(2, 20));
    let key = encoded_key_for(1);
    assert!(map.get(&key).is_some());
    let after_remove = map.remove(&key);
    assert!(after_remove.get(&key).is_none());
    assert!(map.get(&key).is_some(), "original must be unchanged");
}
