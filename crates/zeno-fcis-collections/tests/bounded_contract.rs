#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used)]
use std::collections::{BTreeMap, VecDeque};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_collections::bounded::*;
use zeno_fcis_collections::pipe::*;
use zeno_fcis_collections::{BTreeMapBackend, LogicalEntry, PersistentMap};
use zeno_fcis_value::Value;
fn limits(n: u32, item: u64, total: u64) -> CollectionLimits {
    CollectionLimits {
        max_entries: n,
        max_item_bytes: item,
        max_snapshot_bytes: total,
    }
}
fn entry(k: u128, v: u128) -> LogicalEntry {
    let key = Value::unsigned(k);
    LogicalEntry::try_new(key.canonical_bytes().unwrap(), key, Value::unsigned(v)).unwrap()
}
fn key(k: u128) -> Vec<u8> {
    Value::unsigned(k).canonical_bytes().unwrap()
}
fn profile() -> PipeProfile {
    PipeProfile {
        domain: [1; 32],
        schema: [2; 32],
    }
}
fn msg(id: u8, v: u128) -> PipeMessage {
    PipeMessage::new(profile(), [id; 32], Value::unsigned(v))
}
fn oracle_header(kind: u8, l: CollectionLimits, n: u32) -> Vec<u8> {
    let mut out = b"ZBC1".to_vec();
    out.push(kind);
    out.extend(l.max_entries.to_be_bytes());
    out.extend(l.max_item_bytes.to_be_bytes());
    out.extend(l.max_snapshot_bytes.to_be_bytes());
    out.extend(n.to_be_bytes());
    out
}
fn oracle_integer(out: &mut Vec<u8>, value: u128) {
    out.extend(17_u64.to_be_bytes());
    out.push(3);
    out.extend(value.to_be_bytes());
}

#[test]
fn map_boundaries_replacement_and_refusal_snapshot() {
    let l = limits(1, 50, 79);
    let empty = BoundedMap::<BTreeMapBackend>::empty(l).unwrap();
    let one = empty.insert(entry(4, 9)).unwrap();
    assert_eq!(one.snapshot_len(), 79);
    assert_eq!(one.snapshot_bytes().unwrap().len(), 79);
    let replaced = one.insert(entry(4, 10)).unwrap();
    assert_eq!(replaced.get(&key(4)), Some(&Value::unsigned(10)));
    assert_eq!(one.get(&key(4)), Some(&Value::unsigned(9)));
    assert!(empty.is_empty());
    assert!(matches!(one.insert(entry(5, 10)), Err(BoundedError::Full)));
    assert_eq!(
        one.snapshot_bytes().unwrap(),
        one.remove(&key(100)).unwrap().snapshot_bytes().unwrap()
    );
    assert_eq!(one.remove(&key(4)).unwrap().snapshot_len(), 29);
    assert!(matches!(
        BoundedMap::<BTreeMapBackend>::empty(limits(1, 50, 78))
            .unwrap()
            .insert(entry(4, 9)),
        Err(BoundedError::SnapshotTooLarge)
    ));
    assert!(matches!(
        BoundedMap::<BTreeMapBackend>::empty(limits(0, 49, 29))
            .unwrap()
            .insert(entry(4, 9)),
        Err(BoundedError::ItemTooLarge)
    ));
    assert!(matches!(
        BoundedMap::<BTreeMapBackend>::empty(limits(0, 50, 29))
            .unwrap()
            .insert(entry(4, 9)),
        Err(BoundedError::Full)
    ));
    assert!(matches!(
        BoundedMap::<BTreeMapBackend>::empty(limits(0, 0, 28)),
        Err(BoundedError::InvalidLimits)
    ));
}
fn differential<M: PersistentMap>() {
    let l = limits(4, 50, 229);
    let mut actual = BoundedMap::<M>::empty(l).unwrap();
    let mut model = BTreeMap::new();
    let mut snapshots = Vec::new();
    for step in 0_u128..180 {
        let k = (step * 7 + step / 4) % 7;
        if step % 3 == 0 {
            actual = actual.remove(&key(k)).unwrap();
            model.remove(&k);
        } else {
            let result = actual.insert(entry(k, step));
            if model.contains_key(&k) || model.len() < 4 {
                model.insert(k, step);
                actual = result.unwrap();
            } else {
                assert!(matches!(result, Err(BoundedError::Full)));
            }
        }
        let mut expected = oracle_header(1, l, model.len() as u32);
        for (k, v) in &model {
            oracle_integer(&mut expected, *k);
            oracle_integer(&mut expected, *v);
        }
        assert_eq!(actual.snapshot_bytes().unwrap(), expected);
        assert_eq!(actual.snapshot_len(), expected.len() as u64);
        for (snapshot, bytes) in &snapshots {
            let saved: &BoundedMap<M> = snapshot;
            assert_eq!(saved.snapshot_bytes().unwrap(), *bytes);
        }
        snapshots.push((actual.clone(), expected));
    }
}
#[test]
fn reference_model() {
    differential::<BTreeMapBackend>();
}
#[cfg(feature = "rpds-backend")]
#[test]
fn hash_trie_model() {
    differential::<zeno_fcis_collections::RpdsBackend>();
}
#[cfg(feature = "ordered-map")]
#[test]
fn ordered_tree_model() {
    differential::<zeno_fcis_collections::OrderedMap>();
}
#[test]
fn map_history_set_identity_and_policy_binding() {
    let l = limits(3, 100, 500);
    let mut a = BoundedMap::<BTreeMapBackend>::empty(l).unwrap();
    let mut b = a.clone();
    for k in 0..3 {
        a = a.insert(entry(k, k)).unwrap();
    }
    for k in (0..3).rev() {
        b = b.insert(entry(k, k)).unwrap();
    }
    assert_eq!(a.snapshot_bytes().unwrap(), b.snapshot_bytes().unwrap());
    let set = BoundedSet::<BTreeMapBackend>::empty(limits(1, 34, 63))
        .unwrap()
        .insert(Value::unsigned(3))
        .unwrap();
    assert_eq!(set.len(), 1);
    assert_eq!(
        set.snapshot_bytes().unwrap(),
        set.insert(Value::unsigned(3))
            .unwrap()
            .snapshot_bytes()
            .unwrap()
    );
    assert!(matches!(
        set.insert(Value::unsigned(4)),
        Err(BoundedError::Full)
    ));
    let map = BoundedMap::<BTreeMapBackend>::empty(limits(1, 34, 63))
        .unwrap()
        .insert(LogicalEntry::try_new(key(3), Value::unsigned(3), Value::unit()).unwrap())
        .unwrap();
    assert_ne!(map.snapshot_bytes().unwrap(), set.snapshot_bytes().unwrap());
    assert_ne!(
        BoundedFifo::empty(l).unwrap().snapshot_bytes().unwrap(),
        BoundedFifo::empty(limits(4, 100, 500))
            .unwrap()
            .snapshot_bytes()
            .unwrap()
    );
}
#[test]
fn fifo_independent_queue_model_and_order() {
    let l = limits(3, 25, 104);
    let mut q = BoundedFifo::empty(l).unwrap();
    let mut model = VecDeque::new();
    for step in 0_u128..100 {
        let old = q.clone();
        let before = q.snapshot_bytes().unwrap();
        if step % 3 == 0 {
            match model.pop_front() {
                Some(v) => {
                    let (got, next) = q.pop().unwrap();
                    assert_eq!(got, Value::unsigned(v));
                    q = next
                }
                None => assert_eq!(q.pop(), Err(BoundedError::Empty)),
            }
        } else if model.len() < 3 {
            q = q.push(Value::unsigned(step)).unwrap();
            model.push_back(step);
        } else {
            assert_eq!(q.push(Value::unsigned(step)), Err(BoundedError::Full));
        }
        let mut expected = oracle_header(3, l, model.len() as u32);
        for v in &model {
            oracle_integer(&mut expected, *v);
        }
        assert_eq!(q.snapshot_bytes().unwrap(), expected);
        assert_eq!(q.snapshot_len(), expected.len() as u64);
        assert_eq!(old.snapshot_bytes().unwrap(), before);
    }
    let a = BoundedFifo::empty(l)
        .unwrap()
        .push(Value::unsigned(1))
        .unwrap()
        .push(Value::unsigned(2))
        .unwrap();
    let b = BoundedFifo::empty(l)
        .unwrap()
        .push(Value::unsigned(2))
        .unwrap()
        .push(Value::unsigned(1))
        .unwrap();
    assert_ne!(a.snapshot_bytes().unwrap(), b.snapshot_bytes().unwrap());
    assert_eq!(
        BoundedFifo::empty(limits(1, 25, 53))
            .unwrap()
            .push(Value::unsigned(0)),
        Err(BoundedError::SnapshotTooLarge)
    );
}
#[test]
fn pipe_retry_conflict_ack_and_post_ack_replay() {
    let l = limits(2, 57, 207);
    let empty = ProfileBoundPipe::empty(profile(), l).unwrap();
    let a = msg(3, 11);
    let b = msg(4, 12);
    let one = empty.enqueue(a.clone()).unwrap();
    let full = one.enqueue(b.clone()).unwrap();
    let before = full.snapshot_bytes().unwrap();
    assert_eq!(full.snapshot_len(), 207);
    assert_eq!(full.head().unwrap(), &a);
    assert_eq!(full.head().unwrap(), &a);
    assert_eq!(full.enqueue(a.clone()).unwrap(), full);
    assert_eq!(full.enqueue(msg(3, 99)), Err(PipeError::IdConflict));
    assert_eq!(
        full.enqueue(msg(5, 99)),
        Err(PipeError::Bounds(BoundedError::Full))
    );
    assert_eq!(full.acknowledge(&b), Err(PipeError::HeadMismatch));
    assert_eq!(full.acknowledge(&msg(3, 99)), Err(PipeError::HeadMismatch));
    let rest = full.acknowledge(&a).unwrap();
    assert_eq!(rest.head().unwrap(), &b);
    assert_eq!(
        rest.enqueue(a.clone()).unwrap().pending(),
        &[b.clone(), a.clone()]
    );
    assert_eq!(full.snapshot_bytes().unwrap(), before);
    assert!(empty.pending().is_empty());
    let mut expected = oracle_header(4, l, 2);
    expected.extend([1; 32]);
    expected.extend([2; 32]);
    for (id, v) in [(3, 11), (4, 12)] {
        expected.extend([id; 32]);
        oracle_integer(&mut expected, v);
    }
    assert_eq!(before, expected);
    let wrong = PipeMessage::new(
        PipeProfile {
            domain: [9; 32],
            schema: [2; 32],
        },
        [3; 32],
        Value::unsigned(11),
    );
    assert_eq!(full.enqueue(wrong.clone()), Err(PipeError::ProfileMismatch));
    assert_eq!(empty.acknowledge(&wrong), Err(PipeError::ProfileMismatch));
    assert_eq!(
        empty.acknowledge(&a),
        Err(PipeError::Bounds(BoundedError::Empty))
    );
    assert_eq!(
        ProfileBoundPipe::empty(profile(), limits(1, 57, 149))
            .unwrap()
            .enqueue(a.clone()),
        Err(PipeError::Bounds(BoundedError::SnapshotTooLarge))
    );
    assert_eq!(
        ProfileBoundPipe::empty(profile(), limits(0, 56, 93))
            .unwrap()
            .enqueue(a),
        Err(PipeError::Bounds(BoundedError::ItemTooLarge))
    );
    assert_eq!(
        ProfileBoundPipe::empty(profile(), limits(0, 0, 92)),
        Err(PipeError::Bounds(BoundedError::InvalidLimits))
    );
}
#[test]
fn deeply_nested_values_refuse_before_retention() {
    let mut value = Value::unit();
    for _ in 0..65 {
        value = Value::vector(vec![value]).unwrap();
    }
    let q = BoundedFifo::empty(limits(2, 10000, 20000)).unwrap();
    assert!(matches!(q.push(value.clone()), Err(BoundedError::Value(_))));
    let p = ProfileBoundPipe::empty(profile(), limits(2, 10000, 20000)).unwrap();
    assert!(matches!(
        p.enqueue(PipeMessage::new(profile(), [1; 32], value)),
        Err(PipeError::Bounds(BoundedError::Value(_)))
    ));
}
