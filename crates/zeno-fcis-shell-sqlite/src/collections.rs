//! Keyed collections in SQLite: one row per map entry, an incremental Merkle
//! commitment per map, and a chained journal of decisions.
//!
//! Each map's commitment is the binary radix tree of its 32-byte keys. An
//! empty subtree hashes to `EMPTY`, a subtree holding one key to that key's
//! leaf hash, and a subtree holding two or more keys to `NODE` over its two
//! halves, split at the next key bit, most significant first. So the root
//! depends only on the entries, and leaves sit in sorted key order.
//!
//! The store keeps the hash of every subtree with two or more keys. Writing
//! one key rehashes only the subtrees on its path that hold another key:
//! as many as the bits the key shares with its nearest neighbour in sorted
//! order, about log2 of the entries for unrelated keys and at most 256. The
//! work does not grow with the map's size beyond that.
//!
//! [`CollectionStore::commit`] applies one decision's entry writes in a
//! transaction, after refusing any write to a key the decision did not
//! name, and appends a journal row that chains the decision, the scalar
//! state and every map's root before and after.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;

/// An opaque 32-byte map key.
pub type Key = [u8; 32];
/// A SHA-256 digest.
pub type Digest = [u8; 32];

/// Number of key bits, and so the deepest level of the tree.
const KEY_BITS: usize = 256;

/// A store failure.
#[derive(Debug)]
pub enum Error {
    /// SQLite refused an operation.
    Sqlite(rusqlite::Error),
    /// A decision wrote an entry its command and context did not name.
    Framing {
        /// The map written.
        map: u32,
        /// The key no named entry holds.
        key: Key,
    },
    /// A stored subtree hash is missing, so the store is damaged.
    Corrupt(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "collection store: {error}"),
            Self::Framing { map, key } => write!(
                f,
                "framing: a decision wrote map {map} at an entry it did not name ({})",
                hex(key)
            ),
            Self::Corrupt(what) => write!(f, "collection store damaged: {what}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

/// One journal row: a committed decision and the roots it moved between.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Commit {
    /// One-based position in the journal.
    pub sequence: u64,
    /// The decision's bytes, as the caller bound them.
    pub decision: Vec<u8>,
    /// The scalar state after the decision.
    pub scalars: Vec<u8>,
    /// Each named map's root before the decision.
    pub pre_roots: BTreeMap<u32, Digest>,
    /// Each named map's root after the decision.
    pub post_roots: BTreeMap<u32, Digest>,
    /// The chain over the previous row and all of the above.
    pub chain: Digest,
    /// Subtrees rehashed to apply the writes; work, not journaled, so 0 in
    /// rows read back.
    pub rehashed: u64,
}

/// One write: the new record, or `None` to remove the entry.
pub type Write = (u32, Key, Option<Vec<u8>>);

/// SQLite storage for the maps of one application.
pub struct CollectionStore {
    connection: Connection,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS collection_entries (
    map INTEGER NOT NULL, key BLOB NOT NULL, record BLOB NOT NULL, leaf BLOB NOT NULL,
    PRIMARY KEY (map, key)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS collection_nodes (
    map INTEGER NOT NULL, depth INTEGER NOT NULL, prefix BLOB NOT NULL, hash BLOB NOT NULL,
    PRIMARY KEY (map, depth, prefix)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS collection_commits (
    sequence INTEGER PRIMARY KEY, decision BLOB NOT NULL, scalars BLOB NOT NULL,
    pre_roots BLOB NOT NULL, post_roots BLOB NOT NULL, chain BLOB NOT NULL);
";

impl CollectionStore {
    /// Opens or creates the store at `path`.
    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::with(Connection::open(path)?)
    }

    /// An empty store in memory.
    pub fn in_memory() -> Result<Self, Error> {
        Self::with(Connection::open_in_memory()?)
    }

    fn with(connection: Connection) -> Result<Self, Error> {
        connection.execute_batch(SCHEMA)?;
        Ok(Self { connection })
    }

    /// The record stored at `key`, if any.
    pub fn get(&self, map: u32, key: &Key) -> Result<Option<Vec<u8>>, Error> {
        Ok(self
            .connection
            .query_row(
                "SELECT record FROM collection_entries WHERE map = ?1 AND key = ?2",
                params![map, key.as_slice()],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// The number of entries in a map.
    pub fn len(&self, map: u32) -> Result<u64, Error> {
        let count: i64 = self.connection.query_row(
            "SELECT count(*) FROM collection_entries WHERE map = ?1",
            params![map],
            |row| row.get(0),
        )?;
        Ok(count.unsigned_abs())
    }

    /// Every entry of a map in key order, for audits and replay checks.
    pub fn entries(&self, map: u32) -> Result<Vec<(Key, Vec<u8>)>, Error> {
        let mut statement = self
            .connection
            .prepare("SELECT key, record FROM collection_entries WHERE map = ?1 ORDER BY key")?;
        let rows = statement.query_map(params![map], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (key, record) = row?;
            out.push((key_of(&key)?, record));
        }
        Ok(out)
    }

    /// The map's Merkle root.
    pub fn root(&self, map: u32) -> Result<Digest, Error> {
        subtree(&self.connection, map, 0, &[0; 32])
    }

    /// Applies one decision: refuses any write outside `named`, writes the
    /// entries, and appends a journal row binding the decision, the scalar
    /// state and the roots of every named map before and after.
    pub fn commit(
        &mut self,
        named: &BTreeMap<u32, BTreeSet<Key>>,
        writes: &[Write],
        decision: &[u8],
        scalars: &[u8],
    ) -> Result<Commit, Error> {
        for (map, key, _) in writes {
            if !named.get(map).is_some_and(|keys| keys.contains(key)) {
                return Err(Error::Framing {
                    map: *map,
                    key: *key,
                });
            }
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let roots = |connection: &Connection| -> Result<BTreeMap<u32, Digest>, Error> {
            named
                .keys()
                .map(|map| Ok((*map, subtree(connection, *map, 0, &[0; 32])?)))
                .collect()
        };
        let pre_roots = roots(&transaction)?;
        let mut rehashed = 0;
        for (map, key, record) in writes {
            rehashed += write(&transaction, *map, key, record.as_deref())?;
        }
        let post_roots = roots(&transaction)?;
        let previous: Option<(i64, Vec<u8>)> = transaction
            .query_row(
                "SELECT sequence, chain FROM collection_commits ORDER BY sequence DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (sequence, previous_chain) = match previous {
            Some((sequence, chain)) => (sequence.unsigned_abs() + 1, key_of(&chain)?),
            None => (1, [0; 32]),
        };
        let chain = chain_hash(
            &previous_chain,
            sequence,
            decision,
            scalars,
            &pre_roots,
            &post_roots,
        );
        transaction.execute(
            "INSERT INTO collection_commits VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                i64::try_from(sequence).map_err(|_| Error::Corrupt("sequence".into()))?,
                decision,
                scalars,
                roots_bytes(&pre_roots),
                roots_bytes(&post_roots),
                chain.as_slice()
            ],
        )?;
        transaction.commit()?;
        Ok(Commit {
            sequence,
            decision: decision.to_vec(),
            scalars: scalars.to_vec(),
            pre_roots,
            post_roots,
            chain,
            rehashed,
        })
    }

    /// The journal, oldest first.
    pub fn commits(&self) -> Result<Vec<Commit>, Error> {
        let mut statement = self.connection.prepare(
            "SELECT sequence, decision, scalars, pre_roots, post_roots, chain
             FROM collection_commits ORDER BY sequence",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (sequence, decision, scalars, pre, post, chain) = row?;
            out.push(Commit {
                sequence: sequence.unsigned_abs(),
                decision,
                scalars,
                pre_roots: roots_of(&pre)?,
                post_roots: roots_of(&post)?,
                chain: key_of(&chain)?,
                rehashed: 0,
            });
        }
        Ok(out)
    }
}

/// The root of a key set, recomputed from scratch: the reference for the
/// incremental tree.
pub fn rebuild_root(entries: &[(Key, Vec<u8>)]) -> Digest {
    let mut sorted: Vec<(Key, Digest)> = entries
        .iter()
        .map(|(key, record)| (*key, leaf_hash(key, record)))
        .collect();
    sorted.sort_by_key(|(key, _)| *key);
    sorted.dedup_by_key(|(key, _)| *key);
    rebuild(&sorted, 0)
}

fn rebuild(leaves: &[(Key, Digest)], depth: usize) -> Digest {
    match leaves {
        [] => empty_hash(),
        [(_, leaf)] => *leaf,
        _ => {
            let split = leaves.partition_point(|(key, _)| bit(key, depth) == 0);
            node_hash(
                &rebuild(&leaves[..split], depth + 1),
                &rebuild(&leaves[split..], depth + 1),
            )
        }
    }
}

/// Writes one entry and rehashes the subtrees on its path that hold another
/// key, deepest first; returns how many it rehashed.
fn write(
    connection: &Connection,
    map: u32,
    key: &Key,
    record: Option<&[u8]>,
) -> Result<u64, Error> {
    match record {
        Some(record) => connection.execute(
            "INSERT INTO collection_entries VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (map, key) DO UPDATE SET record = excluded.record, leaf = excluded.leaf",
            params![
                map,
                key.as_slice(),
                record,
                leaf_hash(key, record).as_slice()
            ],
        )?,
        None => connection.execute(
            "DELETE FROM collection_entries WHERE map = ?1 AND key = ?2",
            params![map, key.as_slice()],
        )?,
    };
    // Below the deepest prefix the key shares with another key, its subtree
    // holds it alone and no node row exists.
    let Some(shared) = deepest_shared(connection, map, key)? else {
        return Ok(0);
    };
    let mut child = match record {
        Some(record) => leaf_hash(key, record),
        None => empty_hash(),
    };
    for depth in (0..=shared).rev() {
        let mut sibling = prefix(key, depth + 1);
        flip(&mut sibling, depth);
        let other = subtree(connection, map, depth + 1, &sibling)?;
        let (left, right) = if bit(key, depth) == 0 {
            (child, other)
        } else {
            (other, child)
        };
        let here = prefix(key, depth);
        let under = keys_under(connection, map, depth, &here)?;
        child = match under.as_slice() {
            [] => empty_hash(),
            [only] => leaf_of(connection, map, only)?,
            _ => node_hash(&left, &right),
        };
        let depth_column = i64::try_from(depth).unwrap_or(0);
        if under.len() >= 2 {
            connection.execute(
                "INSERT INTO collection_nodes VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (map, depth, prefix) DO UPDATE SET hash = excluded.hash",
                params![map, depth_column, here.as_slice(), child.as_slice()],
            )?;
        } else {
            connection.execute(
                "DELETE FROM collection_nodes WHERE map = ?1 AND depth = ?2 AND prefix = ?3",
                params![map, depth_column, here.as_slice()],
            )?;
        }
    }
    Ok(u64::try_from(shared + 1).unwrap_or(u64::MAX))
}

/// The deepest level at which the key's subtree holds another key: the bits
/// it shares with its nearest neighbour in sorted order. `None` when the map
/// holds no other key.
fn deepest_shared(connection: &Connection, map: u32, key: &Key) -> Result<Option<usize>, Error> {
    let neighbour = |sql: &str| -> Result<Option<Key>, Error> {
        let found: Option<Vec<u8>> = connection
            .query_row(sql, params![map, key.as_slice()], |row| row.get(0))
            .optional()?;
        found.map(|k| key_of(&k)).transpose()
    };
    let before = neighbour(
        "SELECT key FROM collection_entries WHERE map = ?1 AND key < ?2 ORDER BY key DESC LIMIT 1",
    )?;
    let after = neighbour(
        "SELECT key FROM collection_entries WHERE map = ?1 AND key > ?2 ORDER BY key LIMIT 1",
    )?;
    Ok([before, after]
        .into_iter()
        .flatten()
        .map(|other| shared_bits(key, &other))
        .max())
}

/// The hash of the subtree at `depth` under the first `depth` bits of
/// `prefix`.
fn subtree(connection: &Connection, map: u32, depth: usize, prefix: &Key) -> Result<Digest, Error> {
    match keys_under(connection, map, depth, prefix)?.as_slice() {
        [] => Ok(empty_hash()),
        [only] => leaf_of(connection, map, only),
        _ => {
            let hash: Option<Vec<u8>> = connection
                .query_row(
                    "SELECT hash FROM collection_nodes WHERE map = ?1 AND depth = ?2 AND prefix = ?3",
                    params![map, i64::try_from(depth).unwrap_or(0), prefix.as_slice()],
                    |row| row.get(0),
                )
                .optional()?;
            hash.map(|h| key_of(&h))
                .transpose()?
                .ok_or_else(|| Error::Corrupt(format!("map {map} node at depth {depth}")))
        }
    }
}

/// Up to two keys under a prefix: enough to tell empty, one and many.
fn keys_under(
    connection: &Connection,
    map: u32,
    depth: usize,
    prefix: &Key,
) -> Result<Vec<Key>, Error> {
    let (low, high) = bounds(prefix, depth);
    let mut statement = connection.prepare_cached(
        "SELECT key FROM collection_entries WHERE map = ?1 AND key >= ?2 AND key <= ?3 ORDER BY key LIMIT 2",
    )?;
    let rows = statement.query_map(params![map, low.as_slice(), high.as_slice()], |row| {
        row.get::<_, Vec<u8>>(0)
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(key_of(&row?)?);
    }
    Ok(out)
}

fn leaf_of(connection: &Connection, map: u32, key: &Key) -> Result<Digest, Error> {
    let leaf: Vec<u8> = connection.query_row(
        "SELECT leaf FROM collection_entries WHERE map = ?1 AND key = ?2",
        params![map, key.as_slice()],
        |row| row.get(0),
    )?;
    key_of(&leaf)
}

fn bit(key: &Key, depth: usize) -> u8 {
    (key[depth / 8] >> (7 - depth % 8)) & 1
}

fn flip(key: &mut Key, depth: usize) {
    key[depth / 8] ^= 1 << (7 - depth % 8);
}

/// The first `depth` bits of the key, then zeros.
fn prefix(key: &Key, depth: usize) -> Key {
    let mut out = [0u8; 32];
    for index in 0..depth.min(KEY_BITS) {
        if bit(key, index) == 1 {
            out[index / 8] |= 1 << (7 - index % 8);
        }
    }
    out
}

/// The least and greatest keys with the prefix's first `depth` bits.
fn bounds(prefix: &Key, depth: usize) -> (Key, Key) {
    let low = self::prefix(prefix, depth);
    let mut high = low;
    for index in depth..KEY_BITS {
        high[index / 8] |= 1 << (7 - index % 8);
    }
    (low, high)
}

fn shared_bits(a: &Key, b: &Key) -> usize {
    (0..KEY_BITS)
        .take_while(|i| bit(a, *i) == bit(b, *i))
        .count()
}

fn sha256(bytes: &[u8]) -> Digest {
    *<RustCryptoSha256 as CommitmentHasher>::hash(bytes).as_bytes()
}

fn empty_hash() -> Digest {
    sha256(b"ZFCIS-MAP-EMPTY\0")
}

/// The leaf of one entry: its key and the hash of its record.
pub fn leaf_hash(key: &Key, record: &[u8]) -> Digest {
    let mut framed = b"ZFCIS-MAP-LEAF\0".to_vec();
    framed.extend_from_slice(key);
    framed.extend_from_slice(&sha256(record));
    sha256(&framed)
}

fn node_hash(left: &Digest, right: &Digest) -> Digest {
    let mut framed = b"ZFCIS-MAP-NODE\0".to_vec();
    framed.extend_from_slice(left);
    framed.extend_from_slice(right);
    sha256(&framed)
}

fn roots_bytes(roots: &BTreeMap<u32, Digest>) -> Vec<u8> {
    let mut out = Vec::new();
    for (map, root) in roots {
        out.extend_from_slice(&map.to_be_bytes());
        out.extend_from_slice(root);
    }
    out
}

fn roots_of(bytes: &[u8]) -> Result<BTreeMap<u32, Digest>, Error> {
    if bytes.len() % 36 != 0 {
        return Err(Error::Corrupt("journal roots".into()));
    }
    Ok(bytes
        .chunks(36)
        .map(|chunk| {
            let mut map = [0u8; 4];
            map.copy_from_slice(&chunk[..4]);
            let mut root = [0u8; 32];
            root.copy_from_slice(&chunk[4..]);
            (u32::from_be_bytes(map), root)
        })
        .collect())
}

/// The journal chain over the previous chain, the decision, the scalar
/// state and the roots before and after.
pub fn chain_hash(
    previous: &Digest,
    sequence: u64,
    decision: &[u8],
    scalars: &[u8],
    pre: &BTreeMap<u32, Digest>,
    post: &BTreeMap<u32, Digest>,
) -> Digest {
    let mut framed = b"ZFCIS-COLLECTION-COMMIT\0".to_vec();
    framed.extend_from_slice(previous);
    framed.extend_from_slice(&sequence.to_be_bytes());
    framed.extend_from_slice(&sha256(decision));
    framed.extend_from_slice(&sha256(scalars));
    framed.extend_from_slice(&roots_bytes(pre));
    framed.extend_from_slice(&roots_bytes(post));
    sha256(&framed)
}

fn key_of(bytes: &[u8]) -> Result<Key, Error> {
    bytes
        .try_into()
        .map_err(|_| Error::Corrupt(format!("{}-byte key or hash", bytes.len())))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
