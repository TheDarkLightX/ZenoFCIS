//! The keyed-collection store: incremental Merkle roots, framing and replay.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};

use zeno_fcis_shell_sqlite::collections::{
    CollectionStore, Error, Key, Write, chain_hash, rebuild_root,
};

/// A deterministic xorshift stream.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn key(&mut self) -> Key {
        let mut key = [0u8; 32];
        for chunk in key.chunks_mut(8) {
            chunk.copy_from_slice(&self.next().to_be_bytes());
        }
        key
    }
}

fn named(map: u32, keys: &[Key]) -> BTreeMap<u32, BTreeSet<Key>> {
    BTreeMap::from([(map, keys.iter().copied().collect())])
}

fn commit(store: &mut CollectionStore, writes: &[Write]) -> u64 {
    let mut maps: BTreeMap<u32, BTreeSet<Key>> = BTreeMap::new();
    for (map, key, _) in writes {
        maps.entry(*map).or_default().insert(*key);
    }
    store
        .commit(&maps, writes, b"decision", b"scalars")
        .unwrap()
        .rehashed
}

#[test]
fn collection_roots_match_a_rebuild_after_every_write() {
    let mut store = CollectionStore::in_memory().unwrap();
    let mut stream = Stream(0x9e37_79b9_7f4a_7c15);
    // Random keys, plus keys that share all but their last bits, so that
    // the tree has both shallow and 255-bit-deep branches.
    let mut keys: Vec<Key> = (0..120).map(|_| stream.key()).collect();
    let base = stream.key();
    for last in 0..8u8 {
        let mut near = base;
        near[31] = (near[31] & 0xf8) | last;
        keys.push(near);
    }
    let mut model: BTreeMap<Key, Vec<u8>> = BTreeMap::new();
    for step in 0..900u64 {
        let key = keys[(stream.next() % keys.len() as u64) as usize];
        let record = (stream.next() % 4 != 0).then(|| step.to_be_bytes().to_vec());
        commit(&mut store, &[(7, key, record.clone())]);
        match record {
            Some(record) => {
                model.insert(key, record);
            }
            None => {
                model.remove(&key);
            }
        }
        let entries: Vec<(Key, Vec<u8>)> = model.iter().map(|(k, v)| (*k, v.clone())).collect();
        assert_eq!(
            store.root(7).unwrap(),
            rebuild_root(&entries),
            "step {step}"
        );
        assert_eq!(store.entries(7).unwrap(), entries);
    }
    // Another map is untouched and empty.
    assert_eq!(store.root(8).unwrap(), rebuild_root(&[]));
}

#[test]
fn collection_writes_outside_the_named_entries_are_refused() {
    let mut store = CollectionStore::in_memory().unwrap();
    let (a, b) = ([1u8; 32], [2u8; 32]);
    store
        .commit(&named(7, &[a]), &[(7, a, Some(vec![1]))], b"d1", b"s1")
        .unwrap();
    let root = store.root(7).unwrap();
    // A decision that names only `a` but writes `b` too.
    let refused = store
        .commit(
            &named(7, &[a]),
            &[(7, a, Some(vec![2])), (7, b, Some(vec![3]))],
            b"d2",
            b"s2",
        )
        .unwrap_err();
    assert!(matches!(refused, Error::Framing { map: 7, key } if key == b));
    assert_eq!(store.root(7).unwrap(), root);
    assert_eq!(store.get(7, &a).unwrap(), Some(vec![1]));
    assert_eq!(store.get(7, &b).unwrap(), None);
    assert_eq!(store.commits().unwrap().len(), 1);
}

#[test]
fn collection_journal_chains_and_replays_exactly() {
    let mut store = CollectionStore::in_memory().unwrap();
    let mut stream = Stream(7);
    let keys: Vec<Key> = (0..40).map(|_| stream.key()).collect();
    let mut decisions = Vec::new();
    for step in 0..60u64 {
        let a = keys[(stream.next() % 40) as usize];
        let b = keys[(stream.next() % 40) as usize];
        let writes = vec![(3, a, Some(vec![step as u8])), (3, b, None)];
        let decision = step.to_be_bytes().to_vec();
        store
            .commit(&named(3, &[a, b]), &writes, &decision, &[step as u8])
            .unwrap();
        decisions.push((writes, decision, vec![step as u8], vec![a, b]));
    }
    let journal = store.commits().unwrap();
    assert_eq!(journal.len(), 60);
    // Replaying the decisions into a fresh store gives the same journal.
    let mut replay = CollectionStore::in_memory().unwrap();
    let mut previous = [0u8; 32];
    for (row, (writes, decision, scalars, keys)) in journal.iter().zip(&decisions) {
        let again = replay
            .commit(&named(3, keys), writes, decision, scalars)
            .unwrap();
        assert_eq!(again.chain, row.chain);
        assert_eq!(again.post_roots, row.post_roots);
        assert_eq!(
            row.chain,
            chain_hash(
                &previous,
                row.sequence,
                &row.decision,
                &row.scalars,
                &row.pre_roots,
                &row.post_roots
            )
        );
        previous = row.chain;
    }
    assert_eq!(replay.root(3).unwrap(), store.root(3).unwrap());
}

#[test]
fn collection_writes_rehash_only_the_shared_prefix() {
    let mut store = CollectionStore::in_memory().unwrap();
    let mut stream = Stream(0x5eed);
    // The first key rehashes nothing: it is the root's only leaf.
    assert_eq!(commit(&mut store, &[(1, stream.key(), Some(vec![0]))]), 0);
    let mut deepest = 0;
    for _ in 0..4000 {
        deepest = deepest.max(commit(&mut store, &[(1, stream.key(), Some(vec![1]))]));
    }
    assert_eq!(store.len(1).unwrap(), 4001);
    // About log2(4001) = 12 for random keys; far below the 256 key bits.
    assert!(deepest <= 48, "{deepest}");
    // Two keys that differ only in the last bit share 255 bits: the bound.
    let near = [9u8; 32];
    let mut other = near;
    other[31] ^= 1;
    commit(&mut store, &[(2, near, Some(vec![0]))]);
    assert_eq!(commit(&mut store, &[(2, other, Some(vec![0]))]), 256);
}
