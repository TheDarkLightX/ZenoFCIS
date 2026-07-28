//! Crash-atomic SQLite refinement of the pure ZenoFCIS shell model.

#![forbid(unsafe_code)]

use core::fmt;
use core::marker::PhantomData;
use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use zeno_fcis_authority::{
    AuthorizedShellError, BoundInterpreter, CatalogAuthorizedTransition, CatalogCommitAuthority,
    CatalogTransitionProgram,
};
use zeno_fcis_codec::{
    CanonicalEncode, DecodeError, DecodeLimits, Domain, EncodeError, Hash32, commitment,
    decode_value,
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_laws::ProjectLawEngine;
use zeno_fcis_patch::{PatchError, hash_value};
use zeno_fcis_plan::OutboxEntry;
use zeno_fcis_receipt::{CandidateId, SealError};
use zeno_fcis_schema::SchemaAdmittedEnvelope;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_value::Value;

const SQLITE_SCHEMA_VERSION: i64 = 2;

const SCHEMA_V2: &str = "
PRAGMA foreign_keys = ON;
PRAGMA synchronous = FULL;
CREATE TABLE shell_identity (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    policy_id BLOB NOT NULL CHECK (length(policy_id) = 32),
    state_domain_name TEXT NOT NULL,
    state_domain_version INTEGER NOT NULL CHECK (state_domain_version >= 0)
);
CREATE TABLE IF NOT EXISTS semantic_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    state_bytes BLOB NOT NULL,
    semantic_root BLOB NOT NULL CHECK (length(semantic_root) = 32),
    version INTEGER NOT NULL CHECK (version >= 0)
);
CREATE TABLE authorizations (
    authorization_id BLOB PRIMARY KEY CHECK (length(authorization_id) = 32),
    policy_id BLOB NOT NULL CHECK (length(policy_id) = 32),
    invocation_id BLOB NOT NULL CHECK (length(invocation_id) = 32),
    replay_id BLOB NOT NULL UNIQUE CHECK (length(replay_id) = 32),
    candidate_id BLOB NOT NULL UNIQUE CHECK (length(candidate_id) = 32),
    authorization_bytes BLOB NOT NULL,
    bundle_bytes BLOB NOT NULL,
    receipt_bytes BLOB NOT NULL
);
CREATE TABLE bundles (
    candidate_id BLOB PRIMARY KEY CHECK (length(candidate_id) = 32),
    authorization_id BLOB NOT NULL UNIQUE CHECK (length(authorization_id) = 32),
    bundle_bytes BLOB NOT NULL,
    receipt_bytes BLOB NOT NULL,
    FOREIGN KEY(authorization_id) REFERENCES authorizations(authorization_id)
);
CREATE TABLE replay (
    replay_id BLOB PRIMARY KEY CHECK (length(replay_id) = 32),
    authorization_id BLOB NOT NULL UNIQUE CHECK (length(authorization_id) = 32),
    candidate_id BLOB NOT NULL CHECK (length(candidate_id) = 32),
    authorization_bytes BLOB NOT NULL,
    bundle_bytes BLOB NOT NULL,
    FOREIGN KEY(authorization_id) REFERENCES authorizations(authorization_id),
    FOREIGN KEY(candidate_id) REFERENCES bundles(candidate_id)
);
CREATE TABLE outbox (
    delivery_id BLOB PRIMARY KEY CHECK (length(delivery_id) = 32),
    authorization_id BLOB NOT NULL CHECK (length(authorization_id) = 32),
    candidate_id BLOB NOT NULL CHECK (length(candidate_id) = 32),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    channel INTEGER NOT NULL CHECK (channel >= 0),
    destination_bytes BLOB NOT NULL,
    payload_bytes BLOB NOT NULL,
    entry_hash BLOB NOT NULL CHECK (length(entry_hash) = 32),
    acknowledged INTEGER NOT NULL DEFAULT 0 CHECK (acknowledged IN (0, 1)),
    UNIQUE(authorization_id, ordinal),
    FOREIGN KEY(authorization_id) REFERENCES authorizations(authorization_id),
    FOREIGN KEY(candidate_id) REFERENCES bundles(candidate_id)
);
PRAGMA user_version = 2;
";

type SqliteMarker<P, L, I> = PhantomData<fn() -> (P, L, I)>;

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
    authorization_id: Hash32,
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

    /// Returns the deployment-specific authorization that owns the obligation.
    #[must_use]
    pub const fn authorization_id(&self) -> Hash32 {
        self.authorization_id
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

/// Concrete SQLite shell pinned to one exact production authorization policy.
pub struct SqliteShell<P, L, I>
where
    P: CatalogTransitionProgram<RustCryptoSha256>,
    L: ProjectLawEngine,
{
    connection: Connection,
    policy_id: Hash32,
    state_domain_name: String,
    state_domain_version: u16,
    interpreter: I,
    marker: SqliteMarker<P, L, I>,
}

impl<P, L, I> SqliteShell<P, L, I>
where
    P: CatalogTransitionProgram<RustCryptoSha256>,
    L: ProjectLawEngine,
{
    /// Opens or initializes a file-backed shell.
    pub fn open(
        path: impl AsRef<Path>,
        authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, I>,
        initial_state: &SchemaAdmittedEnvelope,
        interpreter: BoundInterpreter<RustCryptoSha256, P, L, I>,
    ) -> Result<Self, SqliteShellError> {
        let connection = Connection::open(path).map_err(SqliteShellError::Sqlite)?;
        Self::from_connection(connection, authority, initial_state, interpreter)
    }

    /// Opens an in-memory shell for deterministic refinement and fault tests.
    pub fn open_in_memory(
        authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, I>,
        initial_state: &SchemaAdmittedEnvelope,
        interpreter: BoundInterpreter<RustCryptoSha256, P, L, I>,
    ) -> Result<Self, SqliteShellError> {
        let connection = Connection::open_in_memory().map_err(SqliteShellError::Sqlite)?;
        Self::from_connection(connection, authority, initial_state, interpreter)
    }

    fn from_connection(
        connection: Connection,
        authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, I>,
        initial_state: &SchemaAdmittedEnvelope,
        interpreter: BoundInterpreter<RustCryptoSha256, P, L, I>,
    ) -> Result<Self, SqliteShellError> {
        zeno_fcis_authority::AuthorizedShellState::new(authority, initial_state)
            .map_err(SqliteShellError::Authority)?;
        initialize_schema(&connection)?;
        let policy = authority.policy();
        let (interpreter_policy, interpreter) = interpreter.into_parts();
        if interpreter_policy != policy.policy_id() {
            return Err(SqliteShellError::PolicyMismatch);
        }
        let state_domain_name = policy.state_domain().name().to_owned();
        let state_domain_version = policy.state_domain().version();
        let mut shell = Self {
            connection,
            policy_id: policy.policy_id(),
            state_domain_name,
            state_domain_version,
            interpreter,
            marker: PhantomData,
        };
        shell.initialize_or_validate(initial_state.value().value())?;
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
        let identity = transaction
            .query_row(
                "SELECT policy_id, state_domain_name, state_domain_version FROM shell_identity WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(SqliteShellError::Sqlite)?;
        match identity {
            None => {
                transaction
                    .execute(
                        "INSERT INTO shell_identity(singleton, policy_id, state_domain_name, state_domain_version) VALUES (1, ?1, ?2, ?3)",
                        params![
                            self.policy_id.as_bytes().as_slice(),
                            self.state_domain_name.as_str(),
                            i64::from(self.state_domain_version),
                        ],
                    )
                    .map_err(SqliteShellError::Sqlite)?;
            }
            Some((policy, domain_name, domain_version)) => {
                if parse_hash(&policy)? != self.policy_id
                    || domain_name != self.state_domain_name
                    || nonnegative_u32(domain_version)? != u32::from(self.state_domain_version)
                {
                    return Err(SqliteShellError::PolicyMismatch);
                }
            }
        }
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
        validate_shell_identity(
            &self.connection,
            self.policy_id,
            &self.state_domain_name,
            self.state_domain_version,
        )?;
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

    /// Returns the exact policy-bound interpreter instance owned by this shell.
    #[must_use]
    pub const fn interpreter(&self) -> &I {
        &self.interpreter
    }

    /// Atomically publishes one exact nominally authorized transition.
    pub fn commit(
        &mut self,
        authorized: CatalogAuthorizedTransition<RustCryptoSha256, P, L, I>,
    ) -> Result<CommitStatus, SqliteShellError> {
        self.commit_with_crash_point(authorized, None)
    }

    /// Runs the commit protocol with one deterministic injected crash point.
    pub fn commit_with_crash_point(
        &mut self,
        authorized: CatalogAuthorizedTransition<RustCryptoSha256, P, L, I>,
        crash: Option<CrashPoint>,
    ) -> Result<CommitStatus, SqliteShellError> {
        if crash == Some(CrashPoint::BeforeTransaction) {
            return Err(SqliteShellError::InjectedCrash(
                CrashPoint::BeforeTransaction,
            ));
        }
        let domain = Domain::new(&self.state_domain_name, self.state_domain_version)
            .map_err(SqliteShellError::Encode)?;
        if authorized.body().policy_id() != self.policy_id {
            return Err(SqliteShellError::PolicyMismatch);
        }
        let authorization_id = authorized.authorization_id().hash();
        let authorization_bytes = authorized
            .canonical_bytes()
            .map_err(SqliteShellError::Encode)?;
        let invocation_id = authorized.body().invocation_id();
        let replay_id = authorized.replay_id();
        let bundle = authorized.bundle();
        let bundle_bytes = bundle.canonical_bytes().map_err(SqliteShellError::Encode)?;
        let candidate = bundle.candidate_id();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(SqliteShellError::Sqlite)?;
        validate_shell_identity(
            &transaction,
            self.policy_id,
            &self.state_domain_name,
            self.state_domain_version,
        )?;

        if let Some((
            existing_authorization,
            candidate_bytes,
            existing_authorization_bytes,
            existing_bundle,
        )) = replay_row(&transaction, replay_id)?
        {
            let existing_candidate = CandidateId::new(parse_hash(&candidate_bytes)?);
            if parse_hash(&existing_authorization)? != authorization_id
                || existing_candidate != candidate
                || existing_authorization_bytes != authorization_bytes
                || existing_bundle != bundle_bytes
            {
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
                "INSERT INTO authorizations(authorization_id, policy_id, invocation_id, replay_id, candidate_id, authorization_bytes, bundle_bytes, receipt_bytes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    authorization_id.as_bytes().as_slice(),
                    self.policy_id.as_bytes().as_slice(),
                    invocation_id.as_bytes().as_slice(),
                    replay_id.as_bytes().as_slice(),
                    candidate.hash().as_bytes().as_slice(),
                    authorization_bytes.as_slice(),
                    bundle_bytes.as_slice(),
                    receipt_bytes.as_slice(),
                ],
            )
            .map_err(map_constraint)?;
        transaction
            .execute(
                "INSERT INTO bundles(candidate_id, authorization_id, bundle_bytes, receipt_bytes) VALUES (?1, ?2, ?3, ?4)",
                params![
                    candidate.hash().as_bytes().as_slice(),
                    authorization_id.as_bytes().as_slice(),
                    bundle_bytes.as_slice(),
                    receipt_bytes,
                ],
            )
            .map_err(map_constraint)?;
        transaction
            .execute(
                "INSERT INTO replay(replay_id, authorization_id, candidate_id, authorization_bytes, bundle_bytes) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    replay_id.as_bytes().as_slice(),
                    authorization_id.as_bytes().as_slice(),
                    candidate.hash().as_bytes().as_slice(),
                    authorization_bytes.as_slice(),
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
            insert_outbox(&transaction, authorization_id, candidate, entry)?;
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

    /// Returns the first pending outbox record in authorization/ordinal order.
    pub fn next_pending(&self) -> Result<Option<PendingDelivery>, SqliteShellError> {
        validate_shell_identity(
            &self.connection,
            self.policy_id,
            &self.state_domain_name,
            self.state_domain_version,
        )?;
        let row = self
            .connection
            .query_row(
                "SELECT delivery_id, entry_hash, authorization_id, candidate_id, ordinal, channel, destination_bytes, payload_bytes FROM outbox WHERE acknowledged = 0 ORDER BY authorization_id, ordinal LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Vec<u8>>(6)?,
                        row.get::<_, Vec<u8>>(7)?,
                    ))
                },
            )
            .optional()
            .map_err(SqliteShellError::Sqlite)?;
        let Some((
            delivery,
            entry_hash,
            authorization,
            candidate,
            ordinal,
            channel,
            destination,
            payload,
        )) = row
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
            authorization_id: parse_hash(&authorization)?,
            candidate_id: CandidateId::new(parse_hash(&candidate)?),
            entry,
        };
        validate_authorization_mapping(
            &self.connection,
            self.policy_id,
            pending.authorization_id,
            pending.candidate_id,
        )?;
        if hash_outbox_entry(&pending.entry)? != pending.entry_hash
            || pending
                .entry
                .delivery_id::<RustCryptoSha256>(pending.authorization_id)
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
        validate_shell_identity(
            &transaction,
            self.policy_id,
            &self.state_domain_name,
            self.state_domain_version,
        )?;
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
    pub fn deliver_next(&mut self) -> Result<bool, SqliteShellError>
    where
        I: IdempotentDestination,
    {
        let Some(pending) = self.next_pending()? else {
            return Ok(false);
        };
        let observed = self
            .interpreter
            .deliver(pending.delivery_id, pending.entry_hash, &pending.entry)
            .map_err(|error| SqliteShellError::Destination(error.to_string()))?;
        self.acknowledge(pending.delivery_id, observed)?;
        Ok(true)
    }
}

fn initialize_schema(connection: &Connection) -> Result<(), SqliteShellError> {
    let version = connection
        .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
        .map_err(SqliteShellError::Sqlite)?;
    let existing_tables = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('shell_identity', 'semantic_state', 'authorizations', 'bundles', 'replay', 'outbox')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(SqliteShellError::Sqlite)?;
    match version {
        0 if existing_tables == 0 => connection
            .execute_batch(SCHEMA_V2)
            .map_err(SqliteShellError::Sqlite),
        0 => Err(SqliteShellError::LegacySchema),
        SQLITE_SCHEMA_VERSION if existing_tables == 6 => connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")
            .map_err(SqliteShellError::Sqlite),
        SQLITE_SCHEMA_VERSION => Err(SqliteShellError::CorruptSchema),
        other => Err(SqliteShellError::UnsupportedSchemaVersion(other)),
    }
}

fn validate_shell_identity(
    connection: &Connection,
    policy_id: Hash32,
    state_domain_name: &str,
    state_domain_version: u16,
) -> Result<(), SqliteShellError> {
    let identity = connection
        .query_row(
            "SELECT policy_id, state_domain_name, state_domain_version FROM shell_identity WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(SqliteShellError::Sqlite)?;
    let Some((stored_policy, stored_name, stored_version)) = identity else {
        return Err(SqliteShellError::PolicyMismatch);
    };
    if parse_hash(&stored_policy)? != policy_id
        || stored_name != state_domain_name
        || nonnegative_u32(stored_version)? != u32::from(state_domain_version)
    {
        return Err(SqliteShellError::PolicyMismatch);
    }
    Ok(())
}

fn validate_authorization_mapping(
    connection: &Connection,
    policy_id: Hash32,
    authorization_id: Hash32,
    candidate_id: CandidateId,
) -> Result<(), SqliteShellError> {
    let row = connection
        .query_row(
            "SELECT policy_id, candidate_id FROM authorizations WHERE authorization_id = ?1",
            [authorization_id.as_bytes().as_slice()],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()
        .map_err(SqliteShellError::Sqlite)?;
    let Some((stored_policy, stored_candidate)) = row else {
        return Err(SqliteShellError::CorruptOutbox);
    };
    if parse_hash(&stored_policy)? != policy_id
        || CandidateId::new(parse_hash(&stored_candidate)?) != candidate_id
    {
        return Err(SqliteShellError::CorruptOutbox);
    }
    Ok(())
}

type ReplayRow = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

fn replay_row(
    transaction: &Transaction<'_>,
    replay_id: Hash32,
) -> Result<Option<ReplayRow>, SqliteShellError> {
    transaction
        .query_row(
            "SELECT authorization_id, candidate_id, authorization_bytes, bundle_bytes FROM replay WHERE replay_id = ?1",
            [replay_id.as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(SqliteShellError::Sqlite)
}

fn insert_outbox(
    transaction: &Transaction<'_>,
    authorization_id: Hash32,
    candidate: CandidateId,
    entry: &OutboxEntry,
) -> Result<(), SqliteShellError> {
    let delivery_id = entry
        .delivery_id::<RustCryptoSha256>(authorization_id)
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
            "INSERT INTO outbox(delivery_id, authorization_id, candidate_id, ordinal, channel, destination_bytes, payload_bytes, entry_hash, acknowledged) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
            params![
                delivery_id.as_bytes().as_slice(),
                authorization_id.as_bytes().as_slice(),
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
    /// Nominal authority validation failed before opening the database.
    Authority(AuthorizedShellError),
    /// A legacy unversioned schema requires an explicit reviewed migration.
    LegacySchema,
    /// The database schema version is not supported by this adapter.
    UnsupportedSchemaVersion(i64),
    /// The declared schema version is missing required tables.
    CorruptSchema,
    /// The database belongs to another authorization policy or state domain.
    PolicyMismatch,
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
            Self::Authority(error) => write!(formatter, "authorization failed: {error}"),
            Self::LegacySchema => formatter.write_str(
                "legacy unversioned SQLite schema requires an explicit reviewed migration",
            ),
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "unsupported SQLite schema version {version}")
            }
            Self::CorruptSchema => formatter.write_str("SQLite schema is incomplete"),
            Self::PolicyMismatch => {
                formatter.write_str("SQLite shell authorization policy mismatch")
            }
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
    use zeno_fcis_authority::{
        CatalogAuthorizationDecision, ExecutionBinding, ReviewedTransitionInput, StateDomainBinding,
    };
    use zeno_fcis_catalog::{CatalogLimits, CatalogManifest, ChannelDefinition, ProjectCatalog};
    use zeno_fcis_core::{BudgetUsed, Decision, DecisionKind};
    use zeno_fcis_crypto::verify_approved_provider;
    use zeno_fcis_evidence::EvidenceEnvelope;
    use zeno_fcis_laws::{
        DecisionScope, LawCheckInput, LawDefinition, LawEvidenceRequirement, LawEvidenceVerifier,
        LawFamilyPolicy, LawKind, LawLimits, LawObservation, LawProofDecision, LawProofSubject,
        LawStatus, VerifiedProjectLaws, verify_project_laws,
    };
    use zeno_fcis_patch::ValuePath;
    use zeno_fcis_project::{
        DomainPrefix, ProfileBindings, ProjectProfile, RegistryEntry, RegistryKind, SemanticId,
        StableName,
    };
    use zeno_fcis_schema::{
        Schema, SchemaAdmittedTypeEnvelope, SchemaLimits, TypeDef, TypeId, TypeKind,
        ValidationLimits,
    };
    use zeno_fcis_shell::{ShellState, apply_reference_bundle as reference_commit};
    use zeno_fcis_transition::{
        CataloguedTransitionBuilder, TransitionDecision, TransitionError, TransitionLimits,
    };

    fn state_domain() -> Domain<'static> {
        Domain::new("test/sqlite-state", 1).unwrap_or_else(|error| panic!("domain: {error}"))
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn name(value: &str) -> StableName {
        StableName::try_new(value).unwrap_or_else(|error| panic!("name: {error}"))
    }

    fn id(value: u32) -> SemanticId {
        SemanticId::try_new(value).unwrap_or_else(|error| panic!("id: {error}"))
    }

    fn type_def(raw_id: u32, label: &str, kind: TypeKind) -> TypeDef {
        TypeDef::try_new(TypeId::new(raw_id), label, kind, SchemaLimits::default())
            .unwrap_or_else(|error| panic!("type: {error}"))
    }

    fn registry_entry(kind: RegistryKind, raw_id: u32, label: &str) -> RegistryEntry {
        RegistryEntry::try_new(kind, id(raw_id), name(label), hash(raw_id as u8))
            .unwrap_or_else(|error| panic!("registry entry: {error}"))
    }

    fn law_manifest() -> zeno_fcis_laws::LawManifest {
        let families = LawKind::ALL
            .into_iter()
            .map(|kind| {
                if matches!(
                    kind,
                    LawKind::StateInvariant
                        | LawKind::RejectNoAuthority
                        | LawKind::CommittedFailureEffects
                ) {
                    LawFamilyPolicy::required(kind)
                } else {
                    LawFamilyPolicy::not_applicable(kind, hash(94))
                        .unwrap_or_else(|error| panic!("law family: {error}"))
                }
            })
            .collect();
        let definitions = vec![
            LawDefinition::try_new(
                id(1_001),
                name("state-invariant"),
                LawKind::StateInvariant,
                DecisionScope::Committing,
                hash(101),
                hash(111),
                LawEvidenceRequirement::RuntimeOnly,
            )
            .unwrap_or_else(|error| panic!("state law: {error}")),
            LawDefinition::try_new(
                id(1_002),
                name("reject-no-authority"),
                LawKind::RejectNoAuthority,
                DecisionScope::Reject,
                hash(102),
                hash(112),
                LawEvidenceRequirement::RuntimeOnly,
            )
            .unwrap_or_else(|error| panic!("reject law: {error}")),
            LawDefinition::try_new(
                id(1_003),
                name("committed-failure-effects"),
                LawKind::CommittedFailureEffects,
                DecisionScope::CommittedFailure,
                hash(103),
                hash(113),
                LawEvidenceRequirement::RuntimeOnly,
            )
            .unwrap_or_else(|error| panic!("failure law: {error}")),
        ];
        zeno_fcis_laws::LawManifest::try_new(families, definitions)
            .unwrap_or_else(|error| panic!("law manifest: {error}"))
    }

    fn catalog() -> ProjectCatalog {
        let schema = Schema::try_new(
            "SqliteAuthorityFixture",
            1,
            TypeId::new(1),
            vec![
                type_def(1, "State", TypeKind::U128 { min: 0, max: 100 }),
                type_def(2, "Command", TypeKind::Bool),
                type_def(3, "Context", TypeKind::Bool),
                type_def(
                    4,
                    "Destination",
                    TypeKind::Text {
                        min_len: 1,
                        max_len: 32,
                    },
                ),
            ],
            SchemaLimits::default(),
        )
        .unwrap_or_else(|error| panic!("schema: {error}"));
        let channel = ChannelDefinition::try_new(
            id(7),
            name("delivery"),
            TypeId::new(4),
            TypeId::new(1),
            hash(7),
        )
        .unwrap_or_else(|error| panic!("channel: {error}"));
        let manifest =
            CatalogManifest::try_new::<RustCryptoSha256>(Vec::new(), Vec::new(), vec![channel])
                .unwrap_or_else(|error| panic!("manifest: {error}"));
        let laws = law_manifest();
        let mut entries = vec![
            registry_entry(RegistryKind::StateType, 1, "state"),
            registry_entry(RegistryKind::CommandType, 2, "command"),
            registry_entry(RegistryKind::ContextType, 3, "context"),
        ];
        entries.extend_from_slice(manifest.registry_entries());
        entries.extend(
            laws.registry_entries::<RustCryptoSha256>()
                .unwrap_or_else(|error| panic!("law registry: {error}")),
        );
        let profile = ProjectProfile::try_new(
            name("sqlite-fixture"),
            name("core"),
            id(100),
            1,
            id(1),
            id(2),
            id(3),
            DomainPrefix::try_new("sqlite/fixture")
                .unwrap_or_else(|error| panic!("domain prefix: {error}")),
            ProfileBindings {
                schema_hash: schema
                    .schema_hash::<RustCryptoSha256>()
                    .unwrap_or_else(|error| panic!("schema hash: {error}")),
                precedence_hash: manifest.precedence_hash(),
                algorithm_hash: hash(40),
                codec_hash: hash(41),
                effect_registry_hash: manifest.effect_registry_hash(),
                channel_registry_hash: manifest.channel_registry_hash(),
                policy_hash: laws
                    .commitment::<RustCryptoSha256>()
                    .unwrap_or_else(|error| panic!("law commitment: {error}")),
            },
            entries,
        )
        .unwrap_or_else(|error| panic!("profile: {error}"));
        ProjectCatalog::try_new::<RustCryptoSha256>(
            profile,
            schema,
            manifest,
            CatalogLimits::default(),
        )
        .unwrap_or_else(|error| panic!("catalog: {error}"))
    }

    #[derive(Clone, Copy, Debug)]
    struct SqliteLawEngine;

    impl ProjectLawEngine for SqliteLawEngine {
        fn evaluate(
            &self,
            input: &LawCheckInput<'_>,
            _: LawLimits,
        ) -> Result<Vec<LawObservation>, zeno_fcis_laws::LawEngineFailure> {
            match input.decision().kind() {
                DecisionKind::Accept => Ok(vec![
                    LawObservation::try_new(id(1_001), LawStatus::Satisfied, hash(91))
                        .unwrap_or_else(|error| panic!("law observation: {error}")),
                ]),
                DecisionKind::Reject => Ok(Vec::new()),
                DecisionKind::CommittedFailure => Ok(vec![
                    LawObservation::try_new(id(1_001), LawStatus::Satisfied, hash(91))
                        .unwrap_or_else(|error| panic!("law observation: {error}")),
                    LawObservation::try_new(id(1_003), LawStatus::Satisfied, hash(91))
                        .unwrap_or_else(|error| panic!("law observation: {error}")),
                ]),
            }
        }
    }

    struct SqliteEvidenceVerifier;

    impl LawEvidenceVerifier for SqliteEvidenceVerifier {
        fn verifier_identity(&self) -> Hash32 {
            hash(92)
        }

        fn verify(&self, _: &LawProofSubject, _: &EvidenceEnvelope, _: &[u8]) -> LawProofDecision {
            LawProofDecision::Attested {
                verification_claim: hash(93),
            }
        }
    }

    fn verified_laws(
        catalog: &ProjectCatalog,
    ) -> VerifiedProjectLaws<RustCryptoSha256, SqliteLawEngine> {
        verify_project_laws::<RustCryptoSha256, _, _>(
            catalog,
            law_manifest(),
            hash(90),
            Vec::new(),
            LawLimits::default(),
            hash(91),
            SqliteLawEngine,
            &SqliteEvidenceVerifier,
        )
        .unwrap_or_else(|error| panic!("verified laws: {error}"))
    }

    #[derive(Clone, Copy, Debug)]
    struct SqliteProgram;

    impl CatalogTransitionProgram<RustCryptoSha256> for SqliteProgram {
        type Error = TransitionError;

        fn transition_build_hash(&self) -> Hash32 {
            hash(50)
        }

        fn execute(
            &self,
            input: ReviewedTransitionInput<'_>,
        ) -> Result<TransitionDecision, Self::Error> {
            let expected = input.expected_bindings();
            let mut builder = CataloguedTransitionBuilder::<RustCryptoSha256>::try_new(
                input.catalog(),
                input.pre_state().value().value(),
                input.state_domain(),
                expected.command_hash(),
                expected.context_hash(),
                BudgetUsed::default(),
                input.limits(),
            )?;
            builder.update(ValuePath::new(Vec::new()), Value::U128(11))?;
            builder.enqueue(OutboxEntry::new(
                0,
                7,
                Value::text_ascii(String::from("destination"))
                    .unwrap_or_else(|error| panic!("destination: {error}")),
                Value::U128(11),
            ))?;
            builder.seal()
        }
    }

    type TestAuthority =
        CatalogCommitAuthority<RustCryptoSha256, SqliteProgram, SqliteLawEngine, MemoryDestination>;
    type TestShell = SqliteShell<SqliteProgram, SqliteLawEngine, MemoryDestination>;

    fn limits() -> TransitionLimits {
        TransitionLimits::try_new(4, 4, 4, 64, 8, 64)
            .unwrap_or_else(|error| panic!("limits: {error}"))
    }

    fn authority(catalog: &ProjectCatalog, deployment: u8) -> TestAuthority {
        let provider = verify_approved_provider::<RustCryptoSha256>()
            .unwrap_or_else(|error| panic!("provider: {error}"));
        CatalogCommitAuthority::try_new(
            catalog,
            StateDomainBinding::try_new("test/sqlite-state", 1)
                .unwrap_or_else(|error| panic!("state domain: {error}")),
            ExecutionBinding::try_new(hash(50), hash(51), hash(52), hash(deployment), hash(54))
                .unwrap_or_else(|error| panic!("execution: {error}")),
            limits(),
            &provider,
            verified_laws(catalog),
            SqliteProgram,
        )
        .unwrap_or_else(|error| panic!("authority: {error}"))
    }

    fn initial_state(catalog: &ProjectCatalog) -> SchemaAdmittedEnvelope {
        SchemaAdmittedEnvelope::try_new::<RustCryptoSha256>(
            catalog.schema(),
            Value::U128(0),
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("initial state: {error}"))
    }

    fn command(catalog: &ProjectCatalog) -> SchemaAdmittedTypeEnvelope {
        SchemaAdmittedTypeEnvelope::try_new::<RustCryptoSha256>(
            catalog.schema(),
            TypeId::new(2),
            Value::Bool(true),
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("command: {error}"))
    }

    fn context(catalog: &ProjectCatalog) -> SchemaAdmittedTypeEnvelope {
        SchemaAdmittedTypeEnvelope::try_new::<RustCryptoSha256>(
            catalog.schema(),
            TypeId::new(3),
            Value::Bool(true),
            ValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("context: {error}"))
    }

    fn authorized(
        authority: &TestAuthority,
        catalog: &ProjectCatalog,
        replay: u8,
    ) -> CatalogAuthorizedTransition<
        RustCryptoSha256,
        SqliteProgram,
        SqliteLawEngine,
        MemoryDestination,
    > {
        let invocation = authority
            .admit_invocation(
                initial_state(catalog),
                command(catalog),
                context(catalog),
                hash(60),
                hash(61),
                hash(replay),
            )
            .unwrap_or_else(|error| panic!("invocation: {error}"));
        let decision: CatalogAuthorizationDecision<_, _, _, _> = authority
            .execute(invocation)
            .unwrap_or_else(|error| panic!("execute: {error}"));
        match decision {
            Decision::Accept(accepted) => accepted.into_candidate(),
            Decision::Reject(_) | Decision::CommittedFailure(_) => {
                panic!("fixture program must accept")
            }
        }
    }

    fn shell(authority: &TestAuthority, initial: &SchemaAdmittedEnvelope) -> TestShell {
        SqliteShell::open_in_memory(
            authority,
            initial,
            authority.bind_interpreter(MemoryDestination::default()),
        )
        .unwrap_or_else(|error| panic!("shell: {error}"))
    }

    #[test]
    fn sqlite_trace_matches_reference_commit() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let initial = initial_state(&catalog);
        let replay = hash(9);
        let reference =
            ShellState::new::<RustCryptoSha256>(initial.value().value().clone(), state_domain())
                .unwrap_or_else(|error| panic!("reference: {error}"));
        let expected_authorization = authorized(&authority, &catalog, 9);
        let expected = reference_commit::<RustCryptoSha256>(
            &reference,
            state_domain(),
            replay,
            expected_authorization.bundle(),
        )
        .unwrap_or_else(|error| panic!("reference commit: {error}"));
        let mut database = shell(&authority, &initial);
        assert_eq!(
            database
                .commit(authorized(&authority, &catalog, 9))
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
                .commit(authorized(&authority, &catalog, 9))
                .unwrap_or_else(|error| panic!("replay: {error}")),
            CommitStatus::IdempotentReplay
        );
    }

    #[test]
    fn every_precommit_crash_rolls_back_complete_set() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let initial = initial_state(&catalog);
        for point in [
            CrashPoint::BeforeTransaction,
            CrashPoint::AfterValidation,
            CrashPoint::AfterStateWrite,
            CrashPoint::AfterReplayWrite,
            CrashPoint::AfterOutboxWrite,
            CrashPoint::BeforeCommit,
        ] {
            let mut database = shell(&authority, &initial);
            let before = database
                .snapshot()
                .unwrap_or_else(|error| panic!("before: {error}"));
            assert!(matches!(
                database.commit_with_crash_point(
                    authorized(&authority, &catalog, 8),
                    Some(point),
                ),
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
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let initial = initial_state(&catalog);
        let mut database = shell(&authority, &initial);
        assert!(matches!(
            database.commit_with_crash_point(
                authorized(&authority, &catalog, 7),
                Some(CrashPoint::AfterCommit),
            ),
            Err(SqliteShellError::InjectedCrash(CrashPoint::AfterCommit))
        ));
        assert_eq!(
            database
                .commit(authorized(&authority, &catalog, 7))
                .unwrap_or_else(|error| panic!("retry: {error}")),
            CommitStatus::IdempotentReplay
        );
        assert!(
            database
                .deliver_next()
                .unwrap_or_else(|error| panic!("deliver: {error}"))
        );
        assert_eq!(database.interpreter().delivered_count(), 1);
        assert!(
            !database
                .deliver_next()
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
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let initial = initial_state(&catalog);
        let mut database = shell(&authority, &initial);
        database
            .commit(authorized(&authority, &catalog, 6))
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

    #[test]
    fn database_rejects_authorization_from_another_deployment_policy() {
        let catalog = catalog();
        let first_authority = authority(&catalog, 53);
        let other_authority = authority(&catalog, 55);
        let initial = initial_state(&catalog);
        let mut database = shell(&first_authority, &initial);
        assert!(matches!(
            database.commit(authorized(&other_authority, &catalog, 6)),
            Err(SqliteShellError::PolicyMismatch)
        ));
    }

    #[test]
    fn database_rejects_interpreter_bound_by_another_policy() {
        let catalog = catalog();
        let first_authority = authority(&catalog, 53);
        let other_authority = authority(&catalog, 55);
        let initial = initial_state(&catalog);
        assert!(matches!(
            SqliteShell::open_in_memory(
                &first_authority,
                &initial,
                other_authority.bind_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::PolicyMismatch)
        ));
    }

    #[test]
    fn persisted_policy_corruption_is_detected_before_read_or_commit() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let initial = initial_state(&catalog);
        let mut database = shell(&authority, &initial);
        database
            .connection
            .execute(
                "UPDATE shell_identity SET policy_id = ?1 WHERE singleton = 1",
                [hash(99).as_bytes().as_slice()],
            )
            .unwrap_or_else(|error| panic!("corrupt policy: {error}"));
        assert!(matches!(
            database.snapshot(),
            Err(SqliteShellError::PolicyMismatch)
        ));
        assert!(matches!(
            database.commit(authorized(&authority, &catalog, 6)),
            Err(SqliteShellError::PolicyMismatch)
        ));
    }

    #[test]
    fn legacy_unversioned_database_fails_closed() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let initial = initial_state(&catalog);
        let connection = Connection::open_in_memory()
            .unwrap_or_else(|error| panic!("legacy connection: {error}"));
        connection
            .execute_batch(
                "CREATE TABLE semantic_state(singleton INTEGER PRIMARY KEY, state_bytes BLOB, semantic_root BLOB, version INTEGER);",
            )
            .unwrap_or_else(|error| panic!("legacy schema: {error}"));
        assert!(matches!(
            SqliteShell::<SqliteProgram, SqliteLawEngine, MemoryDestination>::from_connection(
                connection,
                &authority,
                &initial,
                authority.bind_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::LegacySchema)
        ));
    }
}
