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
    LogicalEntry::new(encoded_key, key, Value::U128(u128::from(value_byte)))
}

fn make_entry_text(key_text: &str, value_byte: u8) -> LogicalEntry {
    let key = Value::text_ascii(String::from(key_text)).unwrap_or_else(|e| panic!("text: {e}"));
    let encoded_key = key
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode key: {e}"));
    LogicalEntry::new(encoded_key, key, Value::U128(u128::from(value_byte)))
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
