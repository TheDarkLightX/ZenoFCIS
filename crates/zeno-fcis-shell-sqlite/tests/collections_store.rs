//! The keyed-collection store: incremental Merkle roots, framing and replay.

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
        .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"))
        .rehashed
}

#[test]
fn collection_roots_match_a_rebuild_after_every_write() {
    let mut store = CollectionStore::in_memory()
        .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"));
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
        let record = (!stream.next().is_multiple_of(4)).then(|| step.to_be_bytes().to_vec());
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
            store
                .root(7)
                .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}")),
            rebuild_root(&entries),
            "step {step}"
        );
        assert_eq!(
            store
                .entries(7)
                .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}")),
            entries
        );
    }
    // Another map is untouched and empty.
    assert_eq!(
        store
            .root(8)
            .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}")),
        rebuild_root(&[])
    );
}

#[test]
fn collection_writes_outside_the_named_entries_are_refused() {
    let mut store = CollectionStore::in_memory()
        .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"));
    let (a, b) = ([1u8; 32], [2u8; 32]);
    store
        .commit(&named(7, &[a]), &[(7, a, Some(vec![1]))], b"d1", b"s1")
        .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"));
    let root = store
        .root(7)
        .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"));
    // A decision that names only `a` but writes `b` too.
    let refused = store
        .commit(
            &named(7, &[a]),
            &[(7, a, Some(vec![2])), (7, b, Some(vec![3]))],
            b"d2",
            b"s2",
        )
        .err()
        .unwrap_or_else(|| panic!("unnamed write must fail"));
    assert!(matches!(refused, Error::Framing { map: 7, key } if key == b));
    assert_eq!(
        store
            .root(7)
            .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}")),
        root
    );
    assert_eq!(
        store
            .get(7, &a)
            .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}")),
        Some(vec![1])
    );
    assert_eq!(
        store
            .get(7, &b)
            .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}")),
        None
    );
    assert_eq!(
        store
            .commits()
            .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"))
            .len(),
        1
    );
}

#[test]
fn collection_journal_chains_and_replays_exactly() {
    let mut store = CollectionStore::in_memory()
        .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"));
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
            .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"));
        decisions.push((writes, decision, vec![step as u8], vec![a, b]));
    }
    let journal = store
        .commits()
        .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"));
    assert_eq!(journal.len(), 60);
    // Replaying the decisions into a fresh store gives the same journal.
    let mut replay = CollectionStore::in_memory()
        .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"));
    let mut previous = [0u8; 32];
    for (row, (writes, decision, scalars, keys)) in journal.iter().zip(&decisions) {
        let again = replay
            .commit(&named(3, keys), writes, decision, scalars)
            .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"));
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
    assert_eq!(
        replay
            .root(3)
            .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}")),
        store
            .root(3)
            .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"))
    );
}

#[test]
fn collection_writes_rehash_only_the_shared_prefix() {
    let mut store = CollectionStore::in_memory()
        .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}"));
    let mut stream = Stream(0x5eed);
    // The first key rehashes nothing: it is the root's only leaf.
    assert_eq!(commit(&mut store, &[(1, stream.key(), Some(vec![0]))]), 0);
    let mut deepest = 0;
    for _ in 0..4000 {
        deepest = deepest.max(commit(&mut store, &[(1, stream.key(), Some(vec![1]))]));
    }
    assert_eq!(
        store
            .len(1)
            .unwrap_or_else(|error| panic!("collection fixture failed: {error:?}")),
        4001
    );
    // About log2(4001) = 12 for random keys; far below the 256 key bits.
    assert!(deepest <= 48, "{deepest}");
    // Two keys that differ only in the last bit share 255 bits: the bound.
    let near = [9u8; 32];
    let mut other = near;
    other[31] ^= 1;
    commit(&mut store, &[(2, near, Some(vec![0]))]);
    assert_eq!(commit(&mut store, &[(2, other, Some(vec![0]))]), 256);
}

// SQLite cache faults are storage failures, never new decision authority.
fn cache_fixture(
    four_keys: bool,
) -> Result<(CollectionStore, std::path::PathBuf), Box<dyn std::error::Error>> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("collection-cache-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory)?;
    let path = directory.join("store.sqlite");
    let mut store = CollectionStore::open(&path)?;
    // The genuine token journey: mint3, transfer1, then an aliased no-op.
    let record = |amount: i128| {
        let mut bytes = vec![0];
        bytes.extend_from_slice(&amount.to_be_bytes());
        bytes
    };
    store.commit(
        &named(110, &[[0x11; 32]]),
        &[(110, [0x11; 32], Some(record(3)))],
        b"mint3",
        b"supply3",
    )?;
    store.commit(
        &named(110, &[[0x11; 32], [0x22; 32]]),
        &[
            (110, [0x11; 32], Some(record(2))),
            (110, [0x22; 32], Some(record(1))),
        ],
        b"transfer1",
        b"supply3",
    )?;
    store.commit(
        &named(110, &[[0x11; 32]]),
        &[],
        b"self-transfer1",
        b"supply3",
    )?;
    if four_keys {
        // A sibling subtree has two entries, so the writer must verify a
        // cached branch as well as the recomputed singleton leaf hashes.
        store.commit(
            &named(110, &[[0x81; 32], [0x82; 32]]),
            &[
                (110, [0x81; 32], Some(record(1))),
                (110, [0x82; 32], Some(record(1))),
            ],
            b"two-more",
            b"supply5",
        )?;
    }
    Ok((store, path))
}
#[test]
fn cached_leaf_corruption_refuses_startup() -> Result<(), Box<dyn std::error::Error>> {
    let (store, path) = cache_fixture(false)?;
    drop(store);
    let db = rusqlite::Connection::open(&path)?;
    assert_eq!(
        db.execute(
            "UPDATE collection_entries SET leaf=zeroblob(32) WHERE key=?1",
            rusqlite::params![[0x11u8; 32].as_slice()]
        )?,
        1
    );
    drop(db);
    assert!(
        matches!(CollectionStore::open(&path), Err(Error::Corrupt(_))),
        "startup accepted the corrupted token-journey leaf cache"
    );
    std::fs::remove_dir_all(path.parent().ok_or("parent")?)?;
    Ok(())
}
#[test]
fn cached_leaf_corruption_refuses_write_without_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut store, path) = cache_fixture(false)?;
    let before = (store.entries(110)?, store.commits()?, store.root(110)?);
    let db = rusqlite::Connection::open(&path)?;
    db.execute(
        "UPDATE collection_entries SET leaf=zeroblob(32) WHERE key=?1",
        rusqlite::params![[0x11u8; 32].as_slice()],
    )?;
    drop(db);
    let result = store.commit(
        &named(110, &[[0x22; 32]]),
        &[(110, [0x22; 32], Some(vec![2]))],
        b"mint1",
        b"supply4",
    );
    assert!(
        matches!(result, Err(Error::Corrupt(_))),
        "write accepted the corrupted cached sibling leaf: {result:?}"
    );
    assert_eq!(
        (store.entries(110)?, store.commits()?, store.root(110)?),
        before,
        "cache refusal published entries, roots or journal"
    );
    assert!(store.audit_hashes().is_err());
    drop(store);
    std::fs::remove_dir_all(path.parent().ok_or("parent")?)?;
    Ok(())
}
#[test]
fn cached_branch_corruption_refuses_write_without_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut store, path) = cache_fixture(true)?;
    let before = (store.entries(110)?, store.commits()?, store.root(110)?);
    let db = rusqlite::Connection::open(&path)?;
    let mut right = [0u8; 32];
    right[0] = 0x80;
    assert_eq!(
        db.execute(
            "UPDATE collection_nodes SET hash=zeroblob(32) WHERE map=110 AND depth=1 AND prefix=?1",
            rusqlite::params![right.as_slice()]
        )?,
        1
    );
    drop(db);
    let result = store.commit(
        &named(110, &[[0x11; 32]]),
        &[(110, [0x11; 32], Some(vec![2]))],
        b"change",
        b"supply5",
    );
    assert!(
        matches!(result, Err(Error::Corrupt(_))),
        "write accepted a cached branch not authenticated by the old root: {result:?}"
    );
    assert_eq!(
        (store.entries(110)?, store.commits()?, store.root(110)?),
        before
    );
    drop(store);
    std::fs::remove_dir_all(path.parent().ok_or("parent")?)?;
    Ok(())
}
#[test]
fn startup_checks_every_branch_including_missing_and_surplus_rows()
-> Result<(), Box<dyn std::error::Error>> {
    for fault in ["changed", "missing", "surplus"] {
        let (store, path) = cache_fixture(true)?;
        store.audit_hashes()?;
        drop(store);
        let db = rusqlite::Connection::open(&path)?;
        match fault {
            "changed" => {
                db.execute(
                    "UPDATE collection_nodes SET hash=zeroblob(32) WHERE depth=1",
                    [],
                )?;
            }
            "missing" => {
                db.execute("DELETE FROM collection_nodes WHERE depth=1", [])?;
            }
            _ => {
                db.execute(
                    "INSERT INTO collection_nodes VALUES(999,0,zeroblob(32),zeroblob(32))",
                    [],
                )?;
            }
        }
        drop(db);
        assert!(
            matches!(CollectionStore::open(&path), Err(Error::Corrupt(_))),
            "startup accepted {fault} branch cache"
        );
        std::fs::remove_dir_all(path.parent().ok_or("parent")?)?;
    }
    Ok(())
}

#[test]
fn coordinated_cache_changes_cannot_replace_the_checked_root()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut store, path) = cache_fixture(true)?;
    let entries = store.entries(110)?;
    let commits = store.commits()?;
    let db = rusqlite::Connection::open(&path)?;
    let left: Vec<u8> = db.query_row(
        "SELECT hash FROM collection_nodes WHERE map=110 AND depth=1 AND prefix=zeroblob(32)",
        [],
        |row| row.get(0),
    )?;
    let left: [u8; 32] = left.try_into().map_err(|_| "left hash")?;
    let forged = zeno_fcis_codec::collection_merkle::node_hash(&left, &[0; 32]);
    let mut right = [0u8; 32];
    right[0] = 0x80;
    db.execute(
        "UPDATE collection_nodes SET hash=zeroblob(32) WHERE map=110 AND depth=1 AND prefix=?1",
        rusqlite::params![right.as_slice()],
    )?;
    db.execute(
        "UPDATE collection_nodes SET hash=?1 WHERE map=110 AND depth=0",
        rusqlite::params![forged.as_slice()],
    )?;
    drop(db);
    let result = store.commit(
        &named(110, &[[0x11; 32]]),
        &[(110, [0x11; 32], Some(vec![2]))],
        b"change",
        b"supply5",
    );
    assert!(
        matches!(result, Err(Error::Corrupt(_))),
        "coordinated cache changes replaced the checked root: {result:?}"
    );
    assert_eq!(store.entries(110)?, entries);
    assert_eq!(store.commits()?, commits);
    drop(store);
    std::fs::remove_dir_all(path.parent().ok_or("parent")?)?;
    Ok(())
}
