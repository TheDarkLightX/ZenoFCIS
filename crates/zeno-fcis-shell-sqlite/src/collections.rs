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
use zeno_fcis_codec::collection_merkle::{
    Subtree, bounds, empty_hash, node_hash, prefix, shared_path_length, sibling_prefix, update_path,
};
/// Canonical pure keyed-leaf hash and independent full rebuild, retaining these API names.
pub use zeno_fcis_codec::collection_merkle::{leaf_hash, rebuild_root};
use zeno_fcis_crypto::RustCryptoSha256;

/// An opaque 32-byte map key.
pub type Key = [u8; 32];
/// A SHA-256 digest.
pub type Digest = [u8; 32];

/// A store failure.
#[derive(Debug)]
pub enum Error {
    /// SQLite refused an operation.
    Sqlite(rusqlite::Error),
    /// The expected committed revision has changed.
    Conflict,
    /// A retry key already names a committed request.
    Replay,
    /// A configured map would exceed its declared capacity.
    Capacity {
        /// Map at its resource limit.
        map: u32,
    },
    /// A decision wrote an entry its command and context did not name.
    Framing {
        /// The map written.
        map: u32,
        /// The key no named entry holds.
        key: Key,
    },
    /// Stored records, cached hashes or indexes are inconsistent.
    Corrupt(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "collection store: {error}"),
            Self::Conflict => write!(f, "collection store revision conflict"),
            Self::Replay => write!(f, "collection retry key already committed"),
            Self::Capacity { map } => write!(f, "collection map {map} capacity exceeded"),
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
    /// Subtree rows processed by the update plan, not journaled, so 0 in
    /// rows read back. Excludes the additional pre-update path verification.
    pub rehashed: u64,
}

/// Authenticated request binding, supplied by the transport shell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestBinding {
    /// Principal- and contract-scoped retry key.
    pub key: Digest,
    /// Exact authenticated request digest.
    pub request: Digest,
}

/// One write: the new record, or `None` to remove the entry.
pub type Write = (u32, Key, Option<Vec<u8>>);

/// SQLite storage for keyed records.
/// Contract execution first binds its declared capacities with `bind`.
/// Unbound commits are schema-free storage utilities, not contract admission.
pub struct CollectionStore {
    connection: Connection,
    // Verified on open, advanced only after our transaction commits. This
    // storage anchor is not decision authority and cannot be borrowed mutably.
    checked_roots: BTreeMap<u32, Digest>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS collection_entries (
    map INTEGER NOT NULL, key BLOB NOT NULL, record BLOB NOT NULL, leaf BLOB NOT NULL,
    PRIMARY KEY (map, key)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS collection_nodes (
    map INTEGER NOT NULL, depth INTEGER NOT NULL, prefix BLOB NOT NULL, hash BLOB NOT NULL,
    PRIMARY KEY (map, depth, prefix)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS collection_binding (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1), identity BLOB NOT NULL, genesis BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS collection_limits (
    map INTEGER PRIMARY KEY, capacity INTEGER NOT NULL, entries INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS collection_requests (
    key BLOB PRIMARY KEY, request BLOB NOT NULL, sequence INTEGER NOT NULL UNIQUE) WITHOUT ROWID;
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
        let snapshot = connection.unchecked_transaction()?;
        let checked_roots = audit_hashes(&snapshot)?;
        snapshot.commit()?;
        Ok(Self {
            connection,
            checked_roots,
        })
    }

    /// Bind an empty application store, or verify its existing exact identity and limits.
    /// This storage preparation handle grants no decision authority.
    pub fn bind(
        &mut self,
        identity: &Digest,
        genesis: &[u8],
        capacities: &BTreeMap<u32, u64>,
    ) -> Result<(), Error> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let binding: Option<(Vec<u8>, Vec<u8>)> = tx
            .query_row(
                "SELECT identity,genesis FROM collection_binding WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((bound, initial)) = binding {
            let mut stmt = tx.prepare("SELECT map,capacity FROM collection_limits ORDER BY map")?;
            let actual = stmt
                .query_map([], |row| Ok((row.get::<_, u32>(0)?, row.get::<_, i64>(1)?)))?
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            if bound != identity
                || initial != genesis
                || (actual.len() != capacities.len()
                    || actual.iter().any(|(map, capacity)| {
                        u64::try_from(*capacity).ok() != capacities.get(map).copied()
                    }))
            {
                return Err(Error::Corrupt("application binding".into()));
            }
        } else {
            let used: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM collection_entries) OR EXISTS(SELECT 1 FROM collection_commits) OR EXISTS(SELECT 1 FROM collection_requests)", [], |row|row.get(0))?;
            if used {
                return Err(Error::Corrupt(
                    "cannot bind an existing unbound store".into(),
                ));
            }
            tx.execute(
                "INSERT INTO collection_binding VALUES (1,?1,?2)",
                params![identity.as_slice(), genesis],
            )?;
            for (map, capacity) in capacities {
                let capacity =
                    i64::try_from(*capacity).map_err(|_| Error::Corrupt("map capacity".into()))?;
                tx.execute(
                    "INSERT INTO collection_limits VALUES (?1,?2,0)",
                    params![map, capacity],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Indexed committed revision; no journal scan or map enumeration.
    pub fn version(&self) -> Result<u64, Error> {
        let n: i64 = self.connection.query_row(
            "SELECT coalesce(max(sequence),0) FROM collection_commits",
            [],
            |row| row.get(0),
        )?;
        u64::try_from(n).map_err(|_| Error::Corrupt("negative sequence".into()))
    }

    /// Scalar state at the indexed tip, or the bound scalar genesis.
    pub fn scalar_state(&self) -> Result<Vec<u8>, Error> {
        let latest: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT scalars FROM collection_commits ORDER BY sequence DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        match latest {
            Some(scalars) => Ok(scalars),
            None => Ok(self.connection.query_row(
                "SELECT genesis FROM collection_binding WHERE singleton=1",
                [],
                |row| row.get(0),
            )?),
        }
    }

    /// Indexed original request and commit for exact retry recovery.
    pub fn request(&self, key: &Digest) -> Result<Option<(Digest, Commit)>, Error> {
        let row: Option<(Vec<u8>, i64)> = self
            .connection
            .query_row(
                "SELECT request,sequence FROM collection_requests WHERE key=?1",
                params![key.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        row.map(|(request,sequence)| {
            let (decision,scalars,pre,post,chain) = self.connection.query_row("SELECT decision,scalars,pre_roots,post_roots,chain FROM collection_commits WHERE sequence=?1",params![sequence],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,Vec<u8>>(2)?,r.get::<_,Vec<u8>>(3)?,r.get::<_,Vec<u8>>(4)?)))?;
            Ok((key_of(&request)?,Commit{sequence:u64::try_from(sequence).map_err(|_|Error::Corrupt("sequence".into()))?,decision,scalars,pre_roots:roots_of(&pre)?,post_roots:roots_of(&post)?,chain:key_of(&chain)?,rehashed:0}))
        }).transpose()
    }

    /// Atomically guard the revision and publish the submission with its scalar/map writes.
    /// The caller must derive writes from its checked decision; this is a storage utility.
    pub fn commit_request(
        &mut self,
        expected: u64,
        named: &BTreeMap<u32, BTreeSet<Key>>,
        writes: &[Write],
        decision: &[u8],
        scalars: &[u8],
        binding: RequestBinding,
    ) -> Result<Commit, Error> {
        self.commit_inner(named, writes, decision, scalars, Some((expected, binding)))
    }

    /// Validate every cached leaf and branch against stored records in one
    /// read snapshot. Missing and surplus node rows also refuse. This full
    /// recovery check is outside the bounded request path.
    pub fn audit_hashes(&self) -> Result<(), Error> {
        let snapshot = self.connection.unchecked_transaction()?;
        let actual = audit_hashes(&snapshot)?;
        for map in actual.keys().chain(self.checked_roots.keys()) {
            if actual.get(map).copied().unwrap_or_else(empty_hash)
                != self
                    .checked_roots
                    .get(map)
                    .copied()
                    .unwrap_or_else(empty_hash)
            {
                return Err(Error::Corrupt(
                    "records changed outside the checked commits".into(),
                ));
            }
        }
        snapshot.commit()?;
        Ok(())
    }

    /// Audit the bound application's cardinality counters and one retry row per commit.
    /// This complete recovery check is deliberately outside the request path.
    pub fn audit_application(&self) -> Result<(), Error> {
        self.audit_hashes()?;
        let (requests,commits):(i64,i64)=self.connection.query_row("SELECT (SELECT count(*) FROM collection_requests),(SELECT count(*) FROM collection_commits)",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if requests != commits {
            return Err(Error::Corrupt("application request cardinality".into()));
        }
        let mut statement = self
            .connection
            .prepare("SELECT map,capacity,entries FROM collection_limits")?;
        let rows = statement.query_map([], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (map, capacity, entries) = row?;
            if entries < 0
                || entries > capacity
                || u64::try_from(entries).ok() != Some(self.len(map)?)
            {
                return Err(Error::Corrupt("application map cardinality".into()));
            }
        }
        let unknown:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM collection_entries e LEFT JOIN collection_limits l ON l.map=e.map WHERE l.map IS NULL)",[],|r|r.get(0))?;
        if unknown {
            return Err(Error::Corrupt("application has an undeclared map".into()));
        }
        Ok(())
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
        self.commit_inner(named, writes, decision, scalars, None)
    }

    fn commit_inner(
        &mut self,
        named: &BTreeMap<u32, BTreeSet<Key>>,
        writes: &[Write],
        decision: &[u8],
        scalars: &[u8],
        request: Option<(u64, RequestBinding)>,
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
        if let Some((expected, binding)) = request {
            let actual: i64 = transaction.query_row(
                "SELECT coalesce(max(sequence),0) FROM collection_commits",
                [],
                |row| row.get(0),
            )?;
            if u64::try_from(actual).ok() != Some(expected) {
                return Err(Error::Conflict);
            }
            let used: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM collection_requests WHERE key=?1)",
                params![binding.key.as_slice()],
                |row| row.get(0),
            )?;
            if used {
                return Err(Error::Replay);
            }
        }
        let bound: bool =
            transaction.query_row("SELECT EXISTS(SELECT 1 FROM collection_binding)", [], |r| {
                r.get(0)
            })?;
        if bound {
            let mut deltas: BTreeMap<u32, i64> = BTreeMap::new();
            let mut unique = BTreeSet::new();
            for (map, key, record) in writes {
                if !unique.insert((*map, *key)) {
                    return Err(Error::Corrupt("duplicate application write".into()));
                }
                let present: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM collection_entries WHERE map=?1 AND key=?2)",
                    params![map, key.as_slice()],
                    |r| r.get(0),
                )?;
                *deltas.entry(*map).or_default() +=
                    i64::from(record.is_some()) - i64::from(present);
            }
            for (map, delta) in deltas {
                let changed = transaction.execute("UPDATE collection_limits SET entries=entries+?2 WHERE map=?1 AND entries+?2 BETWEEN 0 AND capacity",params![map,delta])?;
                if changed != 1 {
                    return Err(Error::Capacity { map });
                }
            }
        }
        let roots = |connection: &Connection| -> Result<BTreeMap<u32, Digest>, Error> {
            named
                .keys()
                .map(|map| Ok((*map, subtree(connection, *map, 0, &[0; 32])?)))
                .collect()
        };
        let pre_roots = roots(&transaction)?;
        for (map, root) in &pre_roots {
            if self
                .checked_roots
                .get(map)
                .copied()
                .unwrap_or_else(empty_hash)
                != *root
            {
                return Err(Error::Corrupt(
                    "cached root changed since checked snapshot".into(),
                ));
            }
        }
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
        if let Some((_, binding)) = request {
            transaction.execute(
                "INSERT INTO collection_requests VALUES (?1,?2,?3)",
                params![
                    binding.key.as_slice(),
                    binding.request.as_slice(),
                    i64::try_from(sequence).map_err(|_| Error::Corrupt("sequence".into()))?
                ],
            )?;
        }
        // Prepare all allocation before publication. A refusal retains the
        // old anchors; after SQLite commits, replacing this owned map cannot fail.
        let mut checked_roots = self.checked_roots.clone();
        checked_roots.extend(post_roots.iter().map(|(map, root)| (*map, *root)));
        transaction.commit()?;
        self.checked_roots = checked_roots;
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

/// Rebuild all cache rows independently from record bytes.
fn audit_hashes(connection: &Connection) -> Result<BTreeMap<u32, Digest>, Error> {
    type Node = (u32, usize, Key);
    fn branches(
        map: u32,
        leaves: &[(Key, Digest)],
        depth: usize,
        expected: &mut BTreeMap<Node, Digest>,
    ) -> Digest {
        match leaves {
            [] => empty_hash(),
            [(_, leaf)] => *leaf,
            _ => {
                let split =
                    leaves.partition_point(|(key, _)| (key[depth / 8] >> (7 - depth % 8)) & 1 == 0);
                let hash = node_hash(
                    &branches(map, &leaves[..split], depth + 1, expected),
                    &branches(map, &leaves[split..], depth + 1, expected),
                );
                expected.insert((map, depth, prefix(&leaves[0].0, depth)), hash);
                hash
            }
        }
    }
    let mut statement = connection
        .prepare("SELECT map,key,record,leaf FROM collection_entries ORDER BY map,key")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, u32>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, Vec<u8>>(2)?,
            row.get::<_, Vec<u8>>(3)?,
        ))
    })?;
    let mut maps: BTreeMap<u32, Vec<(Key, Digest)>> = BTreeMap::new();
    for row in rows {
        let (map, key, record, cached) = row?;
        let key = key_of(&key)?;
        let actual = leaf_hash(&key, &record);
        if key_of(&cached)? != actual {
            return Err(Error::Corrupt("cached leaf does not match record".into()));
        }
        maps.entry(map).or_default().push((key, actual));
    }
    let mut expected = BTreeMap::new();
    let mut roots = BTreeMap::new();
    for (map, leaves) in maps {
        roots.insert(map, branches(map, &leaves, 0, &mut expected));
    }
    let mut statement = connection.prepare("SELECT map,depth,prefix,hash FROM collection_nodes")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, u32>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Vec<u8>>(2)?,
            row.get::<_, Vec<u8>>(3)?,
        ))
    })?;
    for row in rows {
        let (map, depth, prefix, hash) = row?;
        if expected.remove(&(
            map,
            usize::try_from(depth).map_err(|_| Error::Corrupt("node depth".into()))?,
            key_of(&prefix)?,
        )) != Some(key_of(&hash)?)
        {
            return Err(Error::Corrupt(
                "cached branch does not match records".into(),
            ));
        }
    }
    if !expected.is_empty() {
        return Err(Error::Corrupt("missing cached branch".into()));
    }
    Ok(roots)
}

/// Acquire an immutable path, compute its pure update, then persist only that plan.
fn write(
    connection: &Connection,
    map: u32,
    key: &Key,
    record: Option<&[u8]>,
) -> Result<u64, Error> {
    let neighbours = neighbours(connection, map, key)?;
    let length = shared_path_length(key, &neighbours);
    let mut siblings = Vec::with_capacity(length);
    for depth in 0..length {
        let other =
            sibling_prefix(key, depth).ok_or_else(|| Error::Corrupt("sibling depth".into()))?;
        siblings.push(subtree_summary(connection, map, depth + 1, &other)?);
    }
    // Authenticate every sibling used below against the checked pre-state
    // root by reconstructing the old path from the actual stored record.
    // This catches cache changes after startup without scanning the map.
    let old_root = subtree(connection, map, 0, &[0; 32])?;
    let old_record: Option<Vec<u8>> = connection
        .query_row(
            "SELECT record FROM collection_entries WHERE map=?1 AND key=?2",
            params![map, key.as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    let verified = update_path(key, old_record.as_deref(), &siblings)
        .ok_or_else(|| Error::Corrupt("verification path depth".into()))?;
    if verified.root != old_root {
        return Err(Error::Corrupt(
            "cached update path does not match root".into(),
        ));
    }
    let plan = update_path(key, record, &siblings)
        .ok_or_else(|| Error::Corrupt("update path depth".into()))?;
    let work = u64::try_from(plan.nodes.len()).map_err(|_| Error::Corrupt("path work".into()))?;
    match record {
        Some(record) => {
            let leaf = plan
                .leaf
                .ok_or_else(|| Error::Corrupt("missing update leaf".into()))?;
            connection.execute("INSERT INTO collection_entries VALUES (?1,?2,?3,?4) ON CONFLICT (map,key) DO UPDATE SET record=excluded.record,leaf=excluded.leaf",params![map,key.as_slice(),record,leaf.as_slice()])?;
        }
        None => {
            connection.execute(
                "DELETE FROM collection_entries WHERE map=?1 AND key=?2",
                params![map, key.as_slice()],
            )?;
        }
    }
    for node in plan.nodes {
        let depth = i64::try_from(node.depth).map_err(|_| Error::Corrupt("node depth".into()))?;
        match node.hash {
            Some(hash) => {
                connection.execute("INSERT INTO collection_nodes VALUES (?1,?2,?3,?4) ON CONFLICT (map,depth,prefix) DO UPDATE SET hash=excluded.hash",params![map,depth,node.prefix.as_slice(),hash.as_slice()])?;
            }
            None => {
                connection.execute(
                    "DELETE FROM collection_nodes WHERE map=?1 AND depth=?2 AND prefix=?3",
                    params![map, depth, node.prefix.as_slice()],
                )?;
            }
        }
    }
    Ok(work)
}

/// Acquire the indexed predecessor and successor. Pure code computes their shared depth.
fn neighbours(connection: &Connection, map: u32, key: &Key) -> Result<Vec<Key>, Error> {
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
    Ok([before, after].into_iter().flatten().collect())
}

/// The hash of the subtree at `depth` under the first `depth` bits of
/// `prefix`.
fn subtree(connection: &Connection, map: u32, depth: usize, prefix: &Key) -> Result<Digest, Error> {
    Ok(subtree_summary(connection, map, depth, prefix)?.digest())
}
fn subtree_summary(
    connection: &Connection,
    map: u32,
    depth: usize,
    prefix: &Key,
) -> Result<Subtree, Error> {
    match keys_under(connection, map, depth, prefix)?.as_slice() {
        [] => Ok(Subtree::Empty),
        [only] => Ok(Subtree::Leaf(leaf_of(connection, map, only)?)),
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
                .map(Subtree::Branch)
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
    let (record, leaf): (Vec<u8>, Vec<u8>) = connection.query_row(
        "SELECT record,leaf FROM collection_entries WHERE map = ?1 AND key = ?2",
        params![map, key.as_slice()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let actual = leaf_hash(key, &record);
    if key_of(&leaf)? != actual {
        return Err(Error::Corrupt("cached leaf does not match record".into()));
    }
    Ok(actual)
}

fn sha256(bytes: &[u8]) -> Digest {
    *<RustCryptoSha256 as CommitmentHasher>::hash(bytes).as_bytes()
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
    if !bytes.len().is_multiple_of(36) {
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

/// Exact compiled storage source for application binding, not a publication capability.
pub fn source_digest() -> Digest {
    sha256(include_bytes!("collections.rs"))
}
