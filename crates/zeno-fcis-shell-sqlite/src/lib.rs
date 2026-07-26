//! Crash-atomic SQLite refinement of the pure ZenoFCIS shell model.

#![forbid(unsafe_code)]

use core::fmt;
use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use zeno_fcis_codec::{
    CanonicalEncode, DecodeError, DecodeLimits, Domain, EncodeError, Hash32, commitment,
    decode_value,
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_patch::{PatchError, hash_value};
use zeno_fcis_plan::OutboxEntry;
use zeno_fcis_receipt::{CandidateId, CommitBundle, SealError};
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_value::Value;

const SCHEMA: &str = "
PRAGMA foreign_keys = ON;
PRAGMA synchronous = FULL;
CREATE TABLE IF NOT EXISTS semantic_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    state_bytes BLOB NOT NULL,
    semantic_root BLOB NOT NULL CHECK (length(semantic_root) = 32),
    version INTEGER NOT NULL CHECK (version >= 0)
);
CREATE TABLE IF NOT EXISTS bundles (
    candidate_id BLOB PRIMARY KEY CHECK (length(candidate_id) = 32),
    bundle_bytes BLOB NOT NULL,
    receipt_bytes BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS replay (
    replay_id BLOB PRIMARY KEY CHECK (length(replay_id) = 32),
    candidate_id BLOB NOT NULL CHECK (length(candidate_id) = 32),
    bundle_bytes BLOB NOT NULL,
    FOREIGN KEY(candidate_id) REFERENCES bundles(candidate_id)
);
CREATE TABLE IF NOT EXISTS outbox (
    delivery_id BLOB PRIMARY KEY CHECK (length(delivery_id) = 32),
    candidate_id BLOB NOT NULL CHECK (length(candidate_id) = 32),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    channel INTEGER NOT NULL CHECK (channel >= 0),
    destination_bytes BLOB NOT NULL,
    payload_bytes BLOB NOT NULL,
    entry_hash BLOB NOT NULL CHECK (length(entry_hash) = 32),
    acknowledged INTEGER NOT NULL DEFAULT 0 CHECK (acknowledged IN (0, 1)),
    UNIQUE(candidate_id, ordinal),
    FOREIGN KEY(candidate_id) REFERENCES bundles(candidate_id)
);
";

/// Persistent snapshot of the authoritative semantic state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredSnapshot {
    version: u64,
    state: Value,
    root: Hash32,
    bundle_count: u64,
    replay_count: u64,
    pending_outbox: u64,
}

impl StoredSnapshot {
    /// Returns the committed database version.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the immutable semantic state.
    #[must_use]
    pub const fn state(&self) -> &Value {
        &self.state
    }

    /// Returns the semantic state root.
    #[must_use]
    pub const fn root(&self) -> Hash32 {
        self.root
    }

    /// Returns the number of complete committed bundles.
    #[must_use]
    pub const fn bundle_count(&self) -> u64 {
        self.bundle_count
    }

    /// Returns the number of replay bindings.
    #[must_use]
    pub const fn replay_count(&self) -> u64 {
        self.replay_count
    }

    /// Returns the number of pending outbox records.
    #[must_use]
    pub const fn pending_outbox(&self) -> u64 {
        self.pending_outbox
    }
}

/// Fault-injection point in the concrete commit protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrashPoint {
    /// Fail before opening the transaction.
    BeforeTransaction,
    /// Fail after exact bundle validation and state derivation.
    AfterValidation,
    /// Fail after writing semantic state/root/version.
    AfterStateWrite,
    /// Fail after replay, bundle, and receipt publication.
    AfterReplayWrite,
    /// Fail after every outbox row is written.
    AfterOutboxWrite,
    /// Fail immediately before SQLite commit.
    BeforeCommit,
    /// Commit, then simulate process loss before delivery.
    AfterCommit,
}

/// One exact pending external-delivery obligation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingDelivery {
    delivery_id: Hash32,
    entry_hash: Hash32,
    candidate_id: CandidateId,
    entry: OutboxEntry,
}

impl PendingDelivery {
    /// Returns the idempotent delivery identity.
    #[must_use]
    pub const fn delivery_id(&self) -> Hash32 {
        self.delivery_id
    }

    /// Returns the exact committed entry hash.
    #[must_use]
    pub const fn entry_hash(&self) -> Hash32 {
        self.entry_hash
    }

    /// Returns the candidate that owns the obligation.
    #[must_use]
    pub const fn candidate_id(&self) -> CandidateId {
        self.candidate_id
    }

    /// Returns the exact committed outbox entry.
    #[must_use]
    pub const fn entry(&self) -> &OutboxEntry {
        &self.entry
    }
}

/// Idempotent destination boundary.
pub trait IdempotentDestination {
    /// Destination-specific failure type.
    type Error: fmt::Display;

    /// Delivers once by identity and returns the observed exact entry hash.
    fn deliver(
        &mut self,
        delivery_id: Hash32,
        entry_hash: Hash32,
        entry: &OutboxEntry,
    ) -> Result<Hash32, Self::Error>;
}

/// Deterministic destination stub that rejects identity/content collisions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemoryDestination {
    delivered: BTreeMap<Hash32, Hash32>,
}

impl MemoryDestination {
    /// Returns the exact number of distinct delivered identities.
    #[must_use]
    pub fn delivered_count(&self) -> usize {
        self.delivered.len()
    }
}

/// Memory-destination collision failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryCollision;

impl fmt::Display for DeliveryCollision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("delivery identity already binds different entry content")
    }
}

impl std::error::Error for DeliveryCollision {}

impl IdempotentDestination for MemoryDestination {
    type Error = DeliveryCollision;

    fn deliver(
        &mut self,
        delivery_id: Hash32,
        entry_hash: Hash32,
        _: &OutboxEntry,
    ) -> Result<Hash32, Self::Error> {
        match self.delivered.get(&delivery_id) {
            Some(existing) if *existing != entry_hash => Err(DeliveryCollision),
            Some(existing) => Ok(*existing),
            None => {
                self.delivered.insert(delivery_id, entry_hash);
                Ok(entry_hash)
            }
        }
    }
}

/// Concrete SQLite shell with one explicit semantic-state domain.
pub struct SqliteShell {
    connection: Connection,
    state_domain_name: String,
    state_domain_version: u16,
}

impl SqliteShell {
    /// Opens or initializes a file-backed shell.
    pub fn open(
        path: impl AsRef<Path>,
        initial_state: &Value,
        state_domain_name: impl Into<String>,
        state_domain_version: u16,
    ) -> Result<Self, SqliteShellError> {
        let connection = Connection::open(path).map_err(SqliteShellError::Sqlite)?;
        Self::from_connection(
            connection,
            initial_state,
            state_domain_name.into(),
            state_domain_version,
        )
    }

    /// Opens an in-memory shell for deterministic refinement and fault tests.
    pub fn open_in_memory(
        initial_state: &Value,
        state_domain_name: impl Into<String>,
        state_domain_version: u16,
    ) -> Result<Self, SqliteShellError> {
        let connection = Connection::open_in_memory().map_err(SqliteShellError::Sqlite)?;
        Self::from_connection(
            connection,
            initial_state,
            state_domain_name.into(),
            state_domain_version,
        )
    }

    fn from_connection(
        connection: Connection,
        initial_state: &Value,
        state_domain_name: String,
        state_domain_version: u16,
    ) -> Result<Self, SqliteShellError> {
        Domain::new(&state_domain_name, state_domain_version).map_err(SqliteShellError::Encode)?;
        connection
            .execute_batch(SCHEMA)
            .map_err(SqliteShellError::Sqlite)?;
        let mut shell = Self {
            connection,
            state_domain_name,
            state_domain_version,
        };
        shell.initialize_or_validate(initial_state)?;
        Ok(shell)
    }

    fn initialize_or_validate(&mut self, initial_state: &Value) -> Result<(), SqliteShellError> {
        let domain = Domain::new(&self.state_domain_name, self.state_domain_version)
            .map_err(SqliteShellError::Encode)?;
        let expected_root = hash_value::<RustCryptoSha256>(domain, initial_state)
            .map_err(SqliteShellError::Patch)?;
        let initial_bytes = initial_state
            .canonical_bytes()
            .map_err(SqliteShellError::Encode)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(SqliteShellError::Sqlite)?;
        let existing = transaction
            .query_row(
                "SELECT state_bytes, semantic_root, version FROM semantic_state WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(SqliteShellError::Sqlite)?;
        match existing {
            None => {
                transaction
                    .execute(
                        "INSERT INTO semantic_state(singleton, state_bytes, semantic_root, version) VALUES (1, ?1, ?2, 0)",
                        params![initial_bytes, expected_root.as_bytes().as_slice()],
                    )
                    .map_err(SqliteShellError::Sqlite)?;
            }
            Some((state_bytes, root_bytes, version)) => {
                let state = decode_canonical_value(&state_bytes)?;
                let actual_root = hash_value::<RustCryptoSha256>(domain, &state)
                    .map_err(SqliteShellError::Patch)?;
                let stored_root = parse_hash(&root_bytes)?;
                if actual_root != stored_root || version < 0 {
                    return Err(SqliteShellError::CorruptState);
                }
            }
        }
        transaction.commit().map_err(SqliteShellError::Sqlite)
    }

    /// Returns a fully validated read snapshot and row counts.
    pub fn snapshot(&self) -> Result<StoredSnapshot, SqliteShellError> {
        let (state_bytes, root_bytes, version) = self
            .connection
            .query_row(
                "SELECT state_bytes, semantic_root, version FROM semantic_state WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(SqliteShellError::Sqlite)?;
        let state = decode_canonical_value(&state_bytes)?;
        let root = parse_hash(&root_bytes)?;
        let domain = Domain::new(&self.state_domain_name, self.state_domain_version)
            .map_err(SqliteShellError::Encode)?;
        if hash_value::<RustCryptoSha256>(domain, &state).map_err(SqliteShellError::Patch)? != root
        {
            return Err(SqliteShellError::CorruptState);
        }
        Ok(StoredSnapshot {
            version: nonnegative_u64(version)?,
            state,
            root,
            bundle_count: count_rows(&self.connection, "bundles")?,
            replay_count: count_rows(&self.connection, "replay")?,
            pending_outbox: count_pending(&self.connection)?,
        })
    }

    /// Atomically publishes one exact complete bundle.
    pub fn commit(
        &mut self,
        replay_id: Hash32,
        bundle: &CommitBundle,
    ) -> Result<CommitStatus, SqliteShellError> {
        self.commit_with_crash_point(replay_id, bundle, None)
    }

    /// Runs the commit protocol with one deterministic injected crash point.
    pub fn commit_with_crash_point(
        &mut self,
        replay_id: Hash32,
        bundle: &CommitBundle,
        crash: Option<CrashPoint>,
    ) -> Result<CommitStatus, SqliteShellError> {
        if crash == Some(CrashPoint::BeforeTransaction) {
            return Err(SqliteShellError::InjectedCrash(
                CrashPoint::BeforeTransaction,
            ));
        }
        let domain = Domain::new(&self.state_domain_name, self.state_domain_version)
            .map_err(SqliteShellError::Encode)?;
        let bundle_bytes = bundle.canonical_bytes().map_err(SqliteShellError::Encode)?;
        let candidate = bundle.candidate_id();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(SqliteShellError::Sqlite)?;

        if let Some((candidate_bytes, existing_bundle)) = replay_row(&transaction, replay_id)? {
            let existing_candidate = CandidateId::new(parse_hash(&candidate_bytes)?);
            if existing_candidate != candidate || existing_bundle != bundle_bytes {
                return Err(SqliteShellError::ReplayConflict);
            }
            transaction.commit().map_err(SqliteShellError::Sqlite)?;
            return Ok(CommitStatus::IdempotentReplay);
        }

        let (state_bytes, root_bytes, version) = transaction
            .query_row(
                "SELECT state_bytes, semantic_root, version FROM semantic_state WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(SqliteShellError::Sqlite)?;
        let stored_root = parse_hash(&root_bytes)?;
        if stored_root != bundle.body().pre_root() {
            return Err(SqliteShellError::RootConflict {
                expected: bundle.body().pre_root(),
                actual: stored_root,
            });
        }
        let pre_state = decode_canonical_value(&state_bytes)?;
        let applied = bundle
            .validate_and_apply::<RustCryptoSha256>(&pre_state, domain)
            .map_err(SqliteShellError::Bundle)?;
        let (post_state, post_root) = applied.into_parts();
        let post_bytes = post_state
            .canonical_bytes()
            .map_err(SqliteShellError::Encode)?;
        let next_version = nonnegative_u64(version)?
            .checked_add(1)
            .ok_or(SqliteShellError::VersionOverflow)?;
        if crash == Some(CrashPoint::AfterValidation) {
            return Err(SqliteShellError::InjectedCrash(CrashPoint::AfterValidation));
        }

        let updated = transaction
            .execute(
                "UPDATE semantic_state SET state_bytes = ?1, semantic_root = ?2, version = ?3 WHERE singleton = 1 AND semantic_root = ?4 AND version = ?5",
                params![
                    post_bytes,
                    post_root.as_bytes().as_slice(),
                    sqlite_i64(next_version)?,
                    stored_root.as_bytes().as_slice(),
                    version,
                ],
            )
            .map_err(SqliteShellError::Sqlite)?;
        if updated != 1 {
            return Err(SqliteShellError::ConcurrentConflict);
        }
        if crash == Some(CrashPoint::AfterStateWrite) {
            return Err(SqliteShellError::InjectedCrash(CrashPoint::AfterStateWrite));
        }

        let receipt_bytes = bundle
            .receipt()
            .canonical_bytes()
            .map_err(SqliteShellError::Encode)?;
        transaction
            .execute(
                "INSERT INTO bundles(candidate_id, bundle_bytes, receipt_bytes) VALUES (?1, ?2, ?3)",
                params![
                    candidate.hash().as_bytes().as_slice(),
                    bundle_bytes.as_slice(),
                    receipt_bytes,
                ],
            )
            .map_err(map_constraint)?;
        transaction
            .execute(
                "INSERT INTO replay(replay_id, candidate_id, bundle_bytes) VALUES (?1, ?2, ?3)",
                params![
                    replay_id.as_bytes().as_slice(),
                    candidate.hash().as_bytes().as_slice(),
                    bundle_bytes.as_slice(),
                ],
            )
            .map_err(map_constraint)?;
        if crash == Some(CrashPoint::AfterReplayWrite) {
            return Err(SqliteShellError::InjectedCrash(
                CrashPoint::AfterReplayWrite,
            ));
        }

        for entry in bundle.outbox_plan().entries() {
            insert_outbox(&transaction, candidate, entry)?;
        }
        if crash == Some(CrashPoint::AfterOutboxWrite) {
            return Err(SqliteShellError::InjectedCrash(
                CrashPoint::AfterOutboxWrite,
            ));
        }
        if crash == Some(CrashPoint::BeforeCommit) {
            return Err(SqliteShellError::InjectedCrash(CrashPoint::BeforeCommit));
        }
        transaction.commit().map_err(SqliteShellError::Sqlite)?;
        if crash == Some(CrashPoint::AfterCommit) {
            return Err(SqliteShellError::InjectedCrash(CrashPoint::AfterCommit));
        }
        Ok(CommitStatus::Committed)
    }

    /// Returns the first pending outbox record in candidate/ordinal order.
    pub fn next_pending(&self) -> Result<Option<PendingDelivery>, SqliteShellError> {
        let row = self
            .connection
            .query_row(
                "SELECT delivery_id, entry_hash, candidate_id, ordinal, channel, destination_bytes, payload_bytes FROM outbox WHERE acknowledged = 0 ORDER BY candidate_id, ordinal LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                        row.get::<_, Vec<u8>>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(SqliteShellError::Sqlite)?;
        let Some((delivery, entry_hash, candidate, ordinal, channel, destination, payload)) = row
        else {
            return Ok(None);
        };
        let entry = OutboxEntry::new(
            nonnegative_u32(ordinal)?,
            nonnegative_u32(channel)?,
            decode_canonical_value(&destination)?,
            decode_canonical_value(&payload)?,
        );
        let pending = PendingDelivery {
            delivery_id: parse_hash(&delivery)?,
            entry_hash: parse_hash(&entry_hash)?,
            candidate_id: CandidateId::new(parse_hash(&candidate)?),
            entry,
        };
        if hash_outbox_entry(&pending.entry)? != pending.entry_hash
            || pending
                .entry
                .delivery_id::<RustCryptoSha256>(pending.candidate_id.hash())
                .map_err(SqliteShellError::Encode)?
                != pending.delivery_id
        {
            return Err(SqliteShellError::CorruptOutbox);
        }
        Ok(Some(pending))
    }

    /// Records an exact, idempotent acknowledgement.
    pub fn acknowledge(
        &mut self,
        delivery_id: Hash32,
        observed_entry_hash: Hash32,
    ) -> Result<(), SqliteShellError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(SqliteShellError::Sqlite)?;
        let row = transaction
            .query_row(
                "SELECT entry_hash, acknowledged FROM outbox WHERE delivery_id = ?1",
                [delivery_id.as_bytes().as_slice()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(SqliteShellError::Sqlite)?
            .ok_or(SqliteShellError::UnknownDelivery(delivery_id))?;
        let expected = parse_hash(&row.0)?;
        if expected != observed_entry_hash {
            return Err(SqliteShellError::AcknowledgementMismatch {
                expected,
                observed: observed_entry_hash,
            });
        }
        if row.1 == 0 {
            transaction
                .execute(
                    "UPDATE outbox SET acknowledged = 1 WHERE delivery_id = ?1",
                    [delivery_id.as_bytes().as_slice()],
                )
                .map_err(SqliteShellError::Sqlite)?;
        } else if row.1 != 1 {
            return Err(SqliteShellError::CorruptOutbox);
        }
        transaction.commit().map_err(SqliteShellError::Sqlite)
    }

    /// Delivers and acknowledges the first pending obligation.
    pub fn deliver_next<D: IdempotentDestination>(
        &mut self,
        destination: &mut D,
    ) -> Result<bool, SqliteShellError> {
        let Some(pending) = self.next_pending()? else {
            return Ok(false);
        };
        let observed = destination
            .deliver(pending.delivery_id, pending.entry_hash, &pending.entry)
            .map_err(|error| SqliteShellError::Destination(error.to_string()))?;
        self.acknowledge(pending.delivery_id, observed)?;
        Ok(true)
    }
}

type ReplayRow = (Vec<u8>, Vec<u8>);

fn replay_row(
    transaction: &Transaction<'_>,
    replay_id: Hash32,
) -> Result<Option<ReplayRow>, SqliteShellError> {
    transaction
        .query_row(
            "SELECT candidate_id, bundle_bytes FROM replay WHERE replay_id = ?1",
            [replay_id.as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(SqliteShellError::Sqlite)
}

fn insert_outbox(
    transaction: &Transaction<'_>,
    candidate: CandidateId,
    entry: &OutboxEntry,
) -> Result<(), SqliteShellError> {
    let delivery_id = entry
        .delivery_id::<RustCryptoSha256>(candidate.hash())
        .map_err(SqliteShellError::Encode)?;
    let entry_hash = hash_outbox_entry(entry)?;
    let destination = entry
        .destination()
        .canonical_bytes()
        .map_err(SqliteShellError::Encode)?;
    let payload = entry
        .payload()
        .canonical_bytes()
        .map_err(SqliteShellError::Encode)?;
    transaction
        .execute(
            "INSERT INTO outbox(delivery_id, candidate_id, ordinal, channel, destination_bytes, payload_bytes, entry_hash, acknowledged) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            params![
                delivery_id.as_bytes().as_slice(),
                candidate.hash().as_bytes().as_slice(),
                i64::from(entry.ordinal()),
                i64::from(entry.channel()),
                destination,
                payload,
                entry_hash.as_bytes().as_slice(),
            ],
        )
        .map_err(map_constraint)?;
    Ok(())
}

fn decode_canonical_value(bytes: &[u8]) -> Result<Value, SqliteShellError> {
    let value = decode_value(bytes, DecodeLimits::default()).map_err(SqliteShellError::Decode)?;
    if value
        .canonical_bytes()
        .map_err(SqliteShellError::Encode)?
        .as_slice()
        != bytes
    {
        return Err(SqliteShellError::CorruptState);
    }
    Ok(value)
}

fn hash_outbox_entry(entry: &OutboxEntry) -> Result<Hash32, SqliteShellError> {
    let domain = Domain::new("zeno-fcis/outbox-entry", 1).map_err(SqliteShellError::Encode)?;
    let bytes = entry.canonical_bytes().map_err(SqliteShellError::Encode)?;
    commitment::<RustCryptoSha256>(domain, &bytes).map_err(SqliteShellError::Encode)
}

fn parse_hash(bytes: &[u8]) -> Result<Hash32, SqliteShellError> {
    let exact: [u8; 32] = bytes
        .try_into()
        .map_err(|_| SqliteShellError::InvalidHashLength)?;
    Ok(Hash32::new(exact))
}

fn nonnegative_u64(value: i64) -> Result<u64, SqliteShellError> {
    u64::try_from(value).map_err(|_| SqliteShellError::IntegerRange)
}

fn nonnegative_u32(value: i64) -> Result<u32, SqliteShellError> {
    u32::try_from(value).map_err(|_| SqliteShellError::IntegerRange)
}

fn sqlite_i64(value: u64) -> Result<i64, SqliteShellError> {
    i64::try_from(value).map_err(|_| SqliteShellError::IntegerRange)
}

fn count_rows(connection: &Connection, table: &str) -> Result<u64, SqliteShellError> {
    let query = match table {
        "bundles" => "SELECT COUNT(*) FROM bundles",
        "replay" => "SELECT COUNT(*) FROM replay",
        _ => return Err(SqliteShellError::CorruptState),
    };
    let count = connection
        .query_row(query, [], |row| row.get::<_, i64>(0))
        .map_err(SqliteShellError::Sqlite)?;
    nonnegative_u64(count)
}

fn count_pending(connection: &Connection) -> Result<u64, SqliteShellError> {
    let count = connection
        .query_row(
            "SELECT COUNT(*) FROM outbox WHERE acknowledged = 0",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(SqliteShellError::Sqlite)?;
    nonnegative_u64(count)
}

fn map_constraint(error: rusqlite::Error) -> SqliteShellError {
    match error {
        rusqlite::Error::SqliteFailure(ref inner, _)
            if inner.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            SqliteShellError::UniquenessConflict
        }
        other => SqliteShellError::Sqlite(other),
    }
}

/// Concrete shell or database failure.
#[derive(Debug)]
pub enum SqliteShellError {
    /// SQLite operation failed.
    Sqlite(rusqlite::Error),
    /// Canonical encoding failed.
    Encode(EncodeError),
    /// Canonical value decoding failed.
    Decode(DecodeError),
    /// Semantic patch/root computation failed.
    Patch(PatchError),
    /// Candidate bundle validation failed.
    Bundle(SealError),
    /// Stored hash is not exactly 32 bytes.
    InvalidHashLength,
    /// Stored integer cannot be represented by the protocol type.
    IntegerRange,
    /// Version cannot advance.
    VersionOverflow,
    /// Stored semantic state or root is inconsistent.
    CorruptState,
    /// Stored outbox content or binding is inconsistent.
    CorruptOutbox,
    /// Candidate expected a different current root.
    RootConflict {
        /// Bundle expected root.
        expected: Hash32,
        /// Database current root.
        actual: Hash32,
    },
    /// Replay ID already binds different candidate or bundle bytes.
    ReplayConflict,
    /// Database uniqueness constraint rejected the candidate or outbox row.
    UniquenessConflict,
    /// Compare-and-swap state update lost a concurrent race.
    ConcurrentConflict,
    /// Delivery identity is not present.
    UnknownDelivery(Hash32),
    /// Destination acknowledged different content.
    AcknowledgementMismatch {
        /// Committed entry hash.
        expected: Hash32,
        /// Destination-observed hash.
        observed: Hash32,
    },
    /// Destination adapter failed.
    Destination(String),
    /// Deterministic crash injection interrupted the protocol.
    InjectedCrash(CrashPoint),
}

impl fmt::Display for SqliteShellError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(formatter, "SQLite failure: {error}"),
            Self::Encode(error) => write!(formatter, "canonical encoding failed: {error}"),
            Self::Decode(error) => write!(formatter, "canonical decoding failed: {error}"),
            Self::Patch(error) => write!(formatter, "semantic patch failed: {error}"),
            Self::Bundle(error) => write!(formatter, "bundle validation failed: {error}"),
            Self::InvalidHashLength => formatter.write_str("stored hash length is invalid"),
            Self::IntegerRange => formatter.write_str("stored integer is out of range"),
            Self::VersionOverflow => formatter.write_str("database version overflow"),
            Self::CorruptState => formatter.write_str("stored semantic state is inconsistent"),
            Self::CorruptOutbox => formatter.write_str("stored outbox row is inconsistent"),
            Self::RootConflict { expected, actual } => {
                write!(formatter, "expected root {expected}, current root {actual}")
            }
            Self::ReplayConflict => formatter.write_str("replay identity conflict"),
            Self::UniquenessConflict => formatter.write_str("database identity conflict"),
            Self::ConcurrentConflict => formatter.write_str("concurrent commit conflict"),
            Self::UnknownDelivery(id) => write!(formatter, "unknown delivery {id}"),
            Self::AcknowledgementMismatch { expected, observed } => write!(
                formatter,
                "acknowledgement expected {expected}, observed {observed}"
            ),
            Self::Destination(error) => write!(formatter, "destination failed: {error}"),
            Self::InjectedCrash(point) => write!(formatter, "injected crash at {point:?}"),
        }
    }
}

impl std::error::Error for SqliteShellError {}

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_core::DecisionKind;
    use zeno_fcis_patch::{CanonicalPatch, PatchOp, PathSegment, ValuePath};
    use zeno_fcis_plan::{CommitPlan, OutboxPlan};
    use zeno_fcis_receipt::{CandidateBindings, CandidateBuilder};
    use zeno_fcis_shell::{ShellState, commit as reference_commit};
    use zeno_fcis_value::Field;

    fn state_domain() -> Domain<'static> {
        Domain::new("test/sqlite-state", 1).unwrap_or_else(|error| panic!("domain: {error}"))
    }

    fn initial_state() -> Value {
        Value::Record(Vec::<Field>::new().into_boxed_slice())
    }

    fn bundle(state: &Value) -> CommitBundle {
        let root = hash_value::<RustCryptoSha256>(state_domain(), state)
            .unwrap_or_else(|error| panic!("root: {error}"));
        let patch = CanonicalPatch::try_new(
            1,
            root,
            vec![PatchOp::Insert {
                path: ValuePath::new(vec![PathSegment::Field(1)]),
                map_key: None,
                value: Value::U128(11),
            }],
        )
        .unwrap_or_else(|error| panic!("patch: {error}"));
        let outbox = OutboxPlan::try_new(vec![OutboxEntry::new(
            0,
            7,
            Value::Text("destination".into()),
            Value::U128(11),
        )])
        .unwrap_or_else(|error| panic!("outbox: {error}"));
        CandidateBuilder::seal::<RustCryptoSha256>(
            state,
            state_domain(),
            DecisionKind::Accept,
            None,
            CandidateBindings {
                profile_hash: Hash32::new([1; 32]),
                command_hash: Hash32::new([2; 32]),
                context_hash: Hash32::new([3; 32]),
                precedence_hash: Hash32::new([4; 32]),
                algorithm_hash: Hash32::new([5; 32]),
                budget_hash: Hash32::new([6; 32]),
            },
            patch,
            CommitPlan::empty(),
            outbox,
        )
        .unwrap_or_else(|error| panic!("bundle: {error}"))
    }

    fn shell(state: &Value) -> SqliteShell {
        SqliteShell::open_in_memory(state, "test/sqlite-state", 1)
            .unwrap_or_else(|error| panic!("shell: {error}"))
    }

    #[test]
    fn sqlite_trace_matches_reference_commit() {
        let state = initial_state();
        let bundle = bundle(&state);
        let replay = Hash32::new([9; 32]);
        let reference = ShellState::new::<RustCryptoSha256>(state.clone(), state_domain())
            .unwrap_or_else(|error| panic!("reference: {error}"));
        let expected =
            reference_commit::<RustCryptoSha256>(&reference, state_domain(), replay, &bundle)
                .unwrap_or_else(|error| panic!("reference commit: {error}"));
        let mut database = shell(&state);
        assert_eq!(
            database
                .commit(replay, &bundle)
                .unwrap_or_else(|error| panic!("commit: {error}")),
            CommitStatus::Committed
        );
        let snapshot = database
            .snapshot()
            .unwrap_or_else(|error| panic!("snapshot: {error}"));
        assert_eq!(snapshot.state(), expected.state().state());
        assert_eq!(snapshot.root(), expected.state().root());
        assert_eq!(snapshot.bundle_count(), 1);
        assert_eq!(snapshot.pending_outbox(), 1);
        assert_eq!(
            database
                .commit(replay, &bundle)
                .unwrap_or_else(|error| panic!("replay: {error}")),
            CommitStatus::IdempotentReplay
        );
    }

    #[test]
    fn every_precommit_crash_rolls_back_complete_set() {
        let state = initial_state();
        let bundle = bundle(&state);
        for point in [
            CrashPoint::BeforeTransaction,
            CrashPoint::AfterValidation,
            CrashPoint::AfterStateWrite,
            CrashPoint::AfterReplayWrite,
            CrashPoint::AfterOutboxWrite,
            CrashPoint::BeforeCommit,
        ] {
            let mut database = shell(&state);
            let before = database
                .snapshot()
                .unwrap_or_else(|error| panic!("before: {error}"));
            assert!(matches!(
                database.commit_with_crash_point(Hash32::new([8; 32]), &bundle, Some(point)),
                Err(SqliteShellError::InjectedCrash(observed)) if observed == point
            ));
            let after = database
                .snapshot()
                .unwrap_or_else(|error| panic!("after: {error}"));
            assert_eq!(after, before);
        }
    }

    #[test]
    fn postcommit_crash_recovers_by_replay_and_pending_delivery() {
        let state = initial_state();
        let bundle = bundle(&state);
        let replay = Hash32::new([7; 32]);
        let mut database = shell(&state);
        assert!(matches!(
            database.commit_with_crash_point(replay, &bundle, Some(CrashPoint::AfterCommit)),
            Err(SqliteShellError::InjectedCrash(CrashPoint::AfterCommit))
        ));
        assert_eq!(
            database
                .commit(replay, &bundle)
                .unwrap_or_else(|error| panic!("retry: {error}")),
            CommitStatus::IdempotentReplay
        );
        let mut destination = MemoryDestination::default();
        assert!(
            database
                .deliver_next(&mut destination)
                .unwrap_or_else(|error| panic!("deliver: {error}"))
        );
        assert_eq!(destination.delivered_count(), 1);
        assert!(
            !database
                .deliver_next(&mut destination)
                .unwrap_or_else(|error| panic!("redeliver: {error}"))
        );
        assert_eq!(
            database
                .snapshot()
                .unwrap_or_else(|error| panic!("snapshot: {error}"))
                .pending_outbox(),
            0
        );
    }

    #[test]
    fn acknowledgement_binds_exact_entry_hash() {
        let state = initial_state();
        let bundle = bundle(&state);
        let mut database = shell(&state);
        database
            .commit(Hash32::new([6; 32]), &bundle)
            .unwrap_or_else(|error| panic!("commit: {error}"));
        let pending = database
            .next_pending()
            .unwrap_or_else(|error| panic!("pending: {error}"))
            .unwrap_or_else(|| panic!("missing pending"));
        assert!(matches!(
            database.acknowledge(pending.delivery_id(), Hash32::ZERO),
            Err(SqliteShellError::AcknowledgementMismatch { .. })
        ));
        database
            .acknowledge(pending.delivery_id(), pending.entry_hash())
            .unwrap_or_else(|error| panic!("ack: {error}"));
    }
}
