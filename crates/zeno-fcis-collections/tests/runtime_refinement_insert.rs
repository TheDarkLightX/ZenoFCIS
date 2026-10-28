//! Finite implementation differential for one immutable bounded-map operation.
//!
//! The logical view includes limits and refusal outcomes, not only entries.
//! This checks every admitted map and insertion order in the stated integer
//! domain; it is not a proof of arbitrary values, rpds, SQLite or crash recovery.
#![cfg(feature = "ordered-map")]
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_collections::bounded::{BoundedError, BoundedMap, CollectionLimits};
use zeno_fcis_collections::{LogicalEntry, OrderedMap};
use zeno_fcis_value::Value;

type Logical = BTreeMap<u128, u128>;
type Actual = BoundedMap<OrderedMap>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct View {
    limits: CollectionLimits,
    entries: Vec<(Value, Value)>,
    lookups: Vec<Option<Value>>,
    bytes: Vec<u8>,
    length: u64,
    count: usize,
}

fn key(k: u128) -> Vec<u8> {
    Value::unsigned(k).canonical_bytes().unwrap()
}

fn entry(k: u128, v: u128) -> LogicalEntry {
    LogicalEntry::try_new(key(k), Value::unsigned(k), Value::unsigned(v)).unwrap()
}

fn actual_view(map: &Actual) -> View {
    View {
        limits: map.limits(),
        entries: map
            .entries()
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect(),
        lookups: (1..=3).map(|k| map.get(&key(k)).cloned()).collect(),
        bytes: map.snapshot_bytes().unwrap(),
        length: map.snapshot_len(),
        count: map.len(),
    }
}

// Independent logical accounting. The existing canonical Value codec remains
// a premise; no production bounded-map helper or cached byte total is used.
fn logical_view(map: &Logical, limits: CollectionLimits) -> View {
    let mut bytes = b"ZBC1".to_vec();
    bytes.push(1);
    bytes.extend(limits.max_entries.to_be_bytes());
    bytes.extend(limits.max_item_bytes.to_be_bytes());
    bytes.extend(limits.max_snapshot_bytes.to_be_bytes());
    bytes.extend(u32::try_from(map.len()).unwrap().to_be_bytes());
    for (k, v) in map {
        for value in [k, v] {
            let encoded = Value::unsigned(*value).canonical_bytes().unwrap();
            bytes.extend(u64::try_from(encoded.len()).unwrap().to_be_bytes());
            bytes.extend(encoded);
        }
    }
    View {
        limits,
        entries: map
            .iter()
            .map(|(k, v)| (Value::unsigned(*k), Value::unsigned(*v)))
            .collect(),
        lookups: (1..=3)
            .map(|k| map.get(&k).map(|v| Value::unsigned(*v)))
            .collect(),
        length: u64::try_from(bytes.len()).unwrap(),
        bytes,
        count: map.len(),
    }
}

fn oracle_insert(
    map: &Logical,
    limits: CollectionLimits,
    k: u128,
    v: u128,
) -> Result<View, BoundedError> {
    let mut after = map.clone();
    after.insert(k, v);
    let item_bytes = 16 + key(k).len() + Value::unsigned(v).canonical_bytes().unwrap().len();
    let view = logical_view(&after, limits);
    // Order is observable: item size, then count, then complete snapshot size.
    if u64::try_from(item_bytes).unwrap() > limits.max_item_bytes {
        Err(BoundedError::ItemTooLarge)
    } else if after.len() > limits.max_entries as usize {
        Err(BoundedError::Full)
    } else if view.length > limits.max_snapshot_bytes {
        Err(BoundedError::SnapshotTooLarge)
    } else {
        Ok(view)
    }
}

fn equivalent(
    expected: &Result<View, BoundedError>,
    observed: &Result<View, BoundedError>,
    old_expected: &[View],
    old_observed: &[View],
) -> bool {
    expected == observed && old_expected == old_observed
}

#[test]
fn exhaustive_insert_preserves_view_frame_and_retained_versions() {
    // 19 logical maps: each of three keys is absent or has value 1 or 2,
    // excluding the eight three-entry maps. 54 resource policies are explicit.
    // Only states satisfying the independently checked resource invariant are
    // initial representations; all six operations are checked from each one.
    assert_eq!(key(1).len(), 17);
    let mut states = 0;
    let mut counts = [0_usize; 4]; // accepted, Full, SnapshotTooLarge, ItemTooLarge
    let mut replacement_at_capacity = 0;
    for capacity in 0..=2 {
        for item_limit in [49, 50, 51] {
            for snapshot_limit in [29, 78, 79, 128, 129, 130] {
                let limits = CollectionLimits {
                    max_entries: capacity,
                    max_item_bytes: item_limit,
                    max_snapshot_bytes: snapshot_limit,
                };
                for encoding in 0..27 {
                    let digits = [encoding % 3, (encoding / 3) % 3, encoding / 9];
                    let logical: Logical = digits
                        .into_iter()
                        .enumerate()
                        .filter(|(_, v)| *v != 0)
                        .map(|(k, v)| ((k + 1) as u128, v as u128))
                        .collect();
                    let n = logical.len();
                    if n > 2
                        || n > capacity as usize
                        || 29 + 50 * n as u64 > snapshot_limit
                        || (n != 0 && item_limit < 50)
                    {
                        continue;
                    }
                    states += 1;
                    let ascending: Vec<_> = logical.iter().map(|(k, v)| (*k, *v)).collect();
                    let orders = if n == 2 {
                        vec![ascending.clone(), ascending.into_iter().rev().collect()]
                    } else {
                        vec![ascending]
                    };
                    for order in orders {
                        let mut actual = Actual::empty(limits).unwrap();
                        let mut prefix = Logical::new();
                        let mut retained = vec![actual.clone()];
                        let mut expected_old = vec![logical_view(&prefix, limits)];
                        for (k, v) in order {
                            actual = actual.insert(entry(k, v)).unwrap();
                            prefix.insert(k, v);
                            retained.push(actual.clone());
                            expected_old.push(logical_view(&prefix, limits));
                        }
                        assert_eq!(actual_view(&actual), logical_view(&logical, limits));
                        for k in 1..=3 {
                            for v in 1..=2 {
                                let expected = oracle_insert(&logical, limits, k, v);
                                let observed = actual.insert(entry(k, v)).map(|m| actual_view(&m));
                                let old_observed: Vec<_> =
                                    retained.iter().map(actual_view).collect();
                                assert!(
                                    equivalent(&expected, &observed, &expected_old, &old_observed),
                                    "policy={limits:?} map={logical:?} insert=({k},{v}) expected={expected:?} observed={observed:?}"
                                );
                                // Frame and all old versions are part of exact
                                // equality above, even on a capacity refusal.
                                assert_eq!(actual_view(&actual), logical_view(&logical, limits));
                                match expected {
                                    Ok(_) => {
                                        counts[0] += 1;
                                        if n == capacity as usize && logical.contains_key(&k) {
                                            replacement_at_capacity += 1;
                                        }
                                    }
                                    Err(BoundedError::Full) => counts[1] += 1,
                                    Err(BoundedError::SnapshotTooLarge) => counts[2] += 1,
                                    Err(BoundedError::ItemTooLarge) => counts[3] += 1,
                                    Err(other) => panic!("unexpected oracle refusal: {other:?}"),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(states, 198);
    assert_eq!(counts, [768, 456, 144, 108]);
    assert!(replacement_at_capacity > 0);
    println!(
        "bounded insert: 54 policies, 198 admitted logical states, 1476 operations across all initial insertion orders; outcomes={counts:?}"
    );
}

#[test]
fn comparator_rejects_planted_output_frame_version_and_refusal_faults() {
    let limits = CollectionLimits {
        max_entries: 2,
        max_item_bytes: 50,
        max_snapshot_bytes: 129,
    };
    let old = Logical::from([(1, 1)]);
    let saved = vec![logical_view(&old, limits)];
    let expected = oracle_insert(&old, limits, 2, 2);
    assert!(equivalent(&expected, &expected, &saved, &saved));
    let wrong_output = oracle_insert(&old, limits, 2, 1);
    let wrong_frame = oracle_insert(&Logical::from([(1, 2)]), limits, 2, 2);
    let changed_old = vec![logical_view(&Logical::from([(1, 2)]), limits)];
    assert!(!equivalent(&expected, &wrong_output, &saved, &saved));
    assert!(!equivalent(&expected, &wrong_frame, &saved, &saved));
    assert!(!equivalent(&expected, &expected, &saved, &changed_old));
    assert!(!equivalent(
        &expected,
        &Err(BoundedError::Full),
        &saved,
        &saved
    ));
    assert!(!equivalent(
        &Err(BoundedError::Full),
        &expected,
        &saved,
        &saved
    ));
    assert!(!equivalent(
        &Err(BoundedError::ItemTooLarge),
        &Err(BoundedError::Full),
        &saved,
        &saved
    ));
}

#[test]
fn invalid_empty_snapshot_policy_refuses() {
    let invalid = CollectionLimits {
        max_entries: 2,
        max_item_bytes: 50,
        max_snapshot_bytes: 28,
    };
    assert!(matches!(
        Actual::empty(invalid),
        Err(BoundedError::InvalidLimits)
    ));
}
