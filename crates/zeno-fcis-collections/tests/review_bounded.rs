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
fn map_model<M: PersistentMap>() {
    let l = limits(7, 100, 330);
    let mut map = BoundedMap::<M>::empty(l).unwrap();
    let mut model: BTreeMap<Vec<u8>, (Value, Value)> = BTreeMap::new();
    let mut seed = 0x12345678_u64;
    for step in 0..1000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let key = Value::bytes(vec![(seed >> 32) as u8 % 12; (seed % 3) as usize]).unwrap();
        let encoded_key = key.canonical_bytes().unwrap();
        let saved = map.clone();
        let before = saved.snapshot_bytes().unwrap();
        if step % 5 == 0 {
            map = map.remove(&encoded_key).unwrap();
            model.remove(&encoded_key);
        } else {
            let value = Value::bytes(vec![step as u8; ((seed >> 24) % 100) as usize]).unwrap();
            let item =
                16 + encoded_key.len() as u64 + value.canonical_bytes().unwrap().len() as u64;
            let mut candidate = model.clone();
            candidate.insert(encoded_key.clone(), (key.clone(), value.clone()));
            let total = 29
                + candidate
                    .iter()
                    .map(|(k, (_, v))| {
                        16 + k.len() as u64 + v.canonical_bytes().unwrap().len() as u64
                    })
                    .sum::<u64>();
            let expected = if item > l.max_item_bytes {
                Some(BoundedError::ItemTooLarge)
            } else if candidate.len() > l.max_entries as usize {
                Some(BoundedError::Full)
            } else if total > l.max_snapshot_bytes {
                Some(BoundedError::SnapshotTooLarge)
            } else {
                None
            };
            let result = map.insert(LogicalEntry::try_new(encoded_key, key, value).unwrap());
            match expected {
                Some(error) => assert!(matches!(result, Err(e) if e == error)),
                None => {
                    map = result.unwrap();
                    model = candidate;
                }
            }
        }
        let bytes = map.snapshot_bytes().unwrap();
        let mut body = Vec::new();
        for (k, (_, v)) in &model {
            body.extend_from_slice(&(k.len() as u64).to_be_bytes());
            body.extend_from_slice(k);
            let v = v.canonical_bytes().unwrap();
            body.extend_from_slice(&(v.len() as u64).to_be_bytes());
            body.extend_from_slice(&v);
        }
        assert_eq!(&bytes[29..], body);
        assert_eq!(map.snapshot_len(), bytes.len() as u64);
        assert_eq!(saved.snapshot_bytes().unwrap(), before);
    }
}
#[test]
fn variable_length_reference_model() {
    map_model::<BTreeMapBackend>();
}
#[cfg(feature = "rpds-backend")]
#[test]
fn variable_length_rpds_model() {
    map_model::<zeno_fcis_collections::RpdsBackend>();
}
#[cfg(feature = "ordered-map")]
#[test]
fn variable_length_ordered_model() {
    map_model::<zeno_fcis_collections::OrderedMap>();
}

#[test]
fn pipe_generated_mixed_transitions_preserve_pending_model() {
    let profile = PipeProfile {
        domain: [1; 32],
        schema: [2; 32],
    };
    let l = limits(5, 100, 340);
    let mut pipe = ProfileBoundPipe::empty(profile, l).unwrap();
    let mut model: VecDeque<PipeMessage> = VecDeque::new();
    for step in 0..1000_u64 {
        let id = ((step * 11 + step / 3) % 9) as u8;
        let message = PipeMessage::new(
            profile,
            [id; 32],
            Value::bytes(vec![id; ((step * 7) % 70) as usize]).unwrap(),
        );
        let old = pipe.clone();
        let before = old.snapshot_bytes().unwrap();
        match step % 6 {
            0 => {
                if let Some(head) = model.pop_front() {
                    pipe = pipe.acknowledge(&head).unwrap();
                }
            }
            1 => {
                if let Some(head) = model.front() {
                    assert_eq!(pipe.enqueue(head.clone()).unwrap(), pipe);
                }
            }
            2 => {
                if model.len() > 1 {
                    assert_eq!(pipe.acknowledge(&model[1]), Err(PipeError::HeadMismatch));
                }
            }
            _ => {
                let item = 40 + message.payload().canonical_bytes().unwrap().len() as u64;
                let total = 93
                    + model
                        .iter()
                        .map(|m| 40 + m.payload().canonical_bytes().unwrap().len() as u64)
                        .sum::<u64>()
                    + item;
                let refusal = if let Some(previous) = model.iter().find(|m| m.id() == message.id())
                {
                    if previous == &message {
                        None
                    } else {
                        Some(PipeError::IdConflict)
                    }
                } else if item > l.max_item_bytes {
                    Some(PipeError::Bounds(BoundedError::ItemTooLarge))
                } else if model.len() == l.max_entries as usize {
                    Some(PipeError::Bounds(BoundedError::Full))
                } else if total > l.max_snapshot_bytes {
                    Some(PipeError::Bounds(BoundedError::SnapshotTooLarge))
                } else {
                    None
                };
                let result = pipe.enqueue(message.clone());
                match refusal {
                    Some(e) => assert_eq!(result, Err(e)),
                    None => {
                        pipe = result.unwrap();
                        if !model.iter().any(|m| m.id() == message.id()) {
                            model.push_back(message);
                        }
                    }
                }
            }
        }
        assert_eq!(pipe.pending(), model.make_contiguous());
        assert_eq!(
            pipe.snapshot_len(),
            pipe.snapshot_bytes().unwrap().len() as u64
        );
        assert_eq!(old.snapshot_bytes().unwrap(), before);
    }
}
