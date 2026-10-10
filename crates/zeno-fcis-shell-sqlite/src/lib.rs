//! Crash-atomic SQLite refinement of the pure ZenoFCIS shell model.

#![forbid(unsafe_code)]

use core::fmt;
use core::marker::PhantomData;
use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use zeno_fcis_authority::{
    AuthorizationDecodeLimits, AuthorizedShellError, BoundDeliveryInterpreter,
    CatalogAuthorizedGenesis, CatalogAuthorizedTransition, CatalogCommitAuthority,
    CatalogTransitionProgram, GenesisId,
};
use zeno_fcis_codec::{
    CanonicalEncode, DecodeError, DecodeLimits, Domain, EncodeError, Hash32, commitment,
    decode_value,
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_laws::ProjectLawEngine;
use zeno_fcis_patch::{PatchError, hash_value};
use zeno_fcis_plan::OutboxEntry;
use zeno_fcis_receipt::{
    BundleDecodeLimits, CandidateId, CommitBundle, ReceiptDecodeLimits, SealError,
    decode_commit_bundle, decode_receipt,
};
use zeno_fcis_schema::{SchemaAdmittedEnvelope, ValidationLimits};
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_value::Value;

const SQLITE_SCHEMA_VERSION: i64 = 5;

const SCHEMA_V5: &str = "
PRAGMA foreign_keys = ON;
PRAGMA synchronous = FULL;
CREATE TABLE shell_identity (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    policy_id BLOB NOT NULL CHECK (length(policy_id) = 32),
    state_domain_name TEXT NOT NULL,
    state_domain_version INTEGER NOT NULL CHECK (state_domain_version >= 0)
);
CREATE TABLE genesis (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    genesis_id BLOB NOT NULL UNIQUE CHECK (length(genesis_id) = 32),
    policy_id BLOB NOT NULL CHECK (length(policy_id) = 32),
    genesis_binding_hash BLOB NOT NULL CHECK (length(genesis_binding_hash) = 32),
    initial_root BLOB NOT NULL CHECK (length(initial_root) = 32),
    law_set_hash BLOB NOT NULL CHECK (length(law_set_hash) = 32),
    law_evaluation_hash BLOB NOT NULL CHECK (length(law_evaluation_hash) = 32),
    initial_state_bytes BLOB NOT NULL,
    authorization_bytes BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS semantic_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    state_bytes BLOB NOT NULL,
    semantic_root BLOB NOT NULL CHECK (length(semantic_root) = 32),
    version INTEGER NOT NULL CHECK (version >= 0)
);
CREATE TABLE authorizations (
    authorization_id BLOB PRIMARY KEY CHECK (length(authorization_id) = 32),
    state_version INTEGER NOT NULL UNIQUE CHECK (state_version > 0),
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
    UNIQUE(candidate_id, ordinal),
    FOREIGN KEY(authorization_id) REFERENCES authorizations(authorization_id),
    FOREIGN KEY(candidate_id) REFERENCES bundles(candidate_id)
);
PRAGMA user_version = 5;
";

type SqliteMarker<P, L, I> = PhantomData<fn() -> (P, L, I)>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ValidatedStoredCandidate {
    state_version: u64,
    authorization_id: Hash32,
    policy_id: Hash32,
    invocation_id: Hash32,
    replay_id: Hash32,
    authorization_bytes: Vec<u8>,
    bundle_bytes: Vec<u8>,
    receipt_bytes: Vec<u8>,
    bundle: CommitBundle,
}

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
    genesis_id: GenesisId,
    state_domain_name: String,
    state_domain_version: u16,
    validated_state_root: Hash32,
    validated_state_version: u64,
    validated_history: BTreeMap<CandidateId, ValidatedStoredCandidate>,
    interpreter: I,
    marker: SqliteMarker<P, L, I>,
}

impl<P, L, I> SqliteShell<P, L, I>
where
    P: CatalogTransitionProgram<RustCryptoSha256>,
    L: ProjectLawEngine,
{
    /// Creates a new file-backed shell from one nominal genesis authorization.
    pub fn create(
        path: impl AsRef<Path>,
        authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, I>,
        genesis: CatalogAuthorizedGenesis<RustCryptoSha256, P, L, I>,
        interpreter: BoundDeliveryInterpreter<RustCryptoSha256, P, L, I>,
    ) -> Result<Self, SqliteShellError> {
        let connection = Connection::open(path).map_err(SqliteShellError::Sqlite)?;
        Self::from_new_connection(connection, authority, genesis, interpreter)
    }

    /// Creates a new in-memory shell for deterministic refinement and fault tests.
    pub fn create_in_memory(
        authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, I>,
        genesis: CatalogAuthorizedGenesis<RustCryptoSha256, P, L, I>,
        interpreter: BoundDeliveryInterpreter<RustCryptoSha256, P, L, I>,
    ) -> Result<Self, SqliteShellError> {
        let connection = Connection::open_in_memory().map_err(SqliteShellError::Sqlite)?;
        Self::from_new_connection(connection, authority, genesis, interpreter)
    }

    /// Reopens an existing file-backed shell without accepting replacement genesis state.
    pub fn open_existing(
        path: impl AsRef<Path>,
        authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, I>,
        interpreter: BoundDeliveryInterpreter<RustCryptoSha256, P, L, I>,
    ) -> Result<Self, SqliteShellError> {
        let connection = Connection::open(path).map_err(SqliteShellError::Sqlite)?;
        Self::from_existing_connection(connection, authority, interpreter)
    }

    fn from_new_connection(
        connection: Connection,
        authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, I>,
        genesis: CatalogAuthorizedGenesis<RustCryptoSha256, P, L, I>,
        interpreter: BoundDeliveryInterpreter<RustCryptoSha256, P, L, I>,
    ) -> Result<Self, SqliteShellError> {
        initialize_schema_for_create(&connection)?;
        let policy = authority.policy();
        let (interpreter_policy, interpreter) = interpreter.into_parts();
        if interpreter_policy != policy.policy_id() {
            return Err(SqliteShellError::PolicyMismatch);
        }
        let genesis_id = genesis.genesis_id();
        let genesis_bytes = genesis
            .canonical_bytes()
            .map_err(SqliteShellError::Encode)?;
        let body = genesis.body().clone();
        let initial_state = genesis.initial_state().value().value().clone();
        let initial_state_bytes = initial_state
            .canonical_bytes()
            .map_err(SqliteShellError::Encode)?;
        zeno_fcis_authority::AuthorizedShellState::new(authority, genesis)
            .map_err(SqliteShellError::Authority)?;
        let state_domain_name = policy.state_domain().name().to_owned();
        let state_domain_version = policy.state_domain().version();
        let mut shell = Self {
            connection,
            policy_id: policy.policy_id(),
            genesis_id,
            state_domain_name,
            state_domain_version,
            validated_state_root: body.initial_root(),
            validated_state_version: 0,
            validated_history: BTreeMap::new(),
            interpreter,
            marker: PhantomData,
        };
        shell.initialize_new(&body, &initial_state_bytes, &genesis_bytes)?;
        Ok(shell)
    }

    fn from_existing_connection(
        connection: Connection,
        authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, I>,
        interpreter: BoundDeliveryInterpreter<RustCryptoSha256, P, L, I>,
    ) -> Result<Self, SqliteShellError> {
        validate_existing_schema(&connection)?;
        let policy = authority.policy();
        let (interpreter_policy, interpreter) = interpreter.into_parts();
        if interpreter_policy != policy.policy_id() {
            return Err(SqliteShellError::PolicyMismatch);
        }
        validate_shell_identity(
            &connection,
            policy.policy_id(),
            policy.state_domain().name(),
            policy.state_domain().version(),
        )?;
        let (genesis_id, initial_state) = validate_persisted_genesis(&connection, authority)?;
        let validated_history = validate_persisted_history(&connection, authority, initial_state)?;
        let validated_state_version =
            u64::try_from(validated_history.len()).map_err(|_| SqliteShellError::IntegerRange)?;
        let validated_state_root = validated_history
            .values()
            .max_by_key(|record| record.state_version)
            .map_or(policy.genesis().expected_initial_root(), |record| {
                record.bundle.body().post_root()
            });
        let shell = Self {
            connection,
            policy_id: policy.policy_id(),
            genesis_id,
            state_domain_name: policy.state_domain().name().to_owned(),
            state_domain_version: policy.state_domain().version(),
            validated_state_root,
            validated_state_version,
            validated_history,
            interpreter,
            marker: PhantomData,
        };
        shell.snapshot()?;
        Ok(shell)
    }

    fn initialize_new(
        &mut self,
        body: &zeno_fcis_authority::GenesisAuthorizationBody,
        initial_state_bytes: &[u8],
        genesis_bytes: &[u8],
    ) -> Result<(), SqliteShellError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(SqliteShellError::Sqlite)?;
        let existing: i64 = transaction
            .query_row(
                "SELECT (SELECT COUNT(*) FROM shell_identity) + (SELECT COUNT(*) FROM genesis) + (SELECT COUNT(*) FROM semantic_state)",
                [],
                |row| row.get(0),
            )
            .map_err(SqliteShellError::Sqlite)?;
        if existing != 0 {
            return Err(SqliteShellError::AlreadyInitialized);
        }
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
        transaction
            .execute(
                "INSERT INTO genesis(singleton, genesis_id, policy_id, genesis_binding_hash, initial_root, law_set_hash, law_evaluation_hash, initial_state_bytes, authorization_bytes) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    self.genesis_id.hash().as_bytes().as_slice(),
                    self.policy_id.as_bytes().as_slice(),
                    body.genesis_binding_hash().as_bytes().as_slice(),
                    body.initial_root().as_bytes().as_slice(),
                    body.law_set_hash().as_bytes().as_slice(),
                    body.law_evaluation_hash().as_bytes().as_slice(),
                    initial_state_bytes,
                    genesis_bytes,
                ],
            )
            .map_err(SqliteShellError::Sqlite)?;
        transaction
            .execute(
                "INSERT INTO semantic_state(singleton, state_bytes, semantic_root, version) VALUES (1, ?1, ?2, 0)",
                params![initial_state_bytes, body.initial_root().as_bytes().as_slice()],
            )
            .map_err(SqliteShellError::Sqlite)?;
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
        let version = nonnegative_u64(version)?;
        if root != self.validated_state_root || version != self.validated_state_version {
            return Err(SqliteShellError::CorruptHistory);
        }
        validate_cached_history(&self.connection, &self.validated_history)?;
        Ok(StoredSnapshot {
            version,
            state,
            root,
            bundle_count: count_rows(&self.connection, "bundles")?,
            replay_count: count_rows(&self.connection, "replay")?,
            pending_outbox: count_pending(&self.connection)?,
        })
    }

    /// Returns the exact policy-bound outbox-delivery interpreter.
    #[must_use]
    pub const fn delivery_interpreter(&self) -> &I {
        &self.interpreter
    }

    /// Returns the content-addressed genesis authorization persisted by this shell.
    #[must_use]
    pub const fn genesis_id(&self) -> GenesisId {
        self.genesis_id
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
        validate_cached_history(&transaction, &self.validated_history)?;

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
            let record = self
                .validated_history
                .get(&existing_candidate)
                .ok_or(SqliteShellError::CorruptHistory)?;
            validate_stored_candidate(&transaction, record)?;
            validate_history_counts(&transaction, &self.validated_history)?;
            transaction.commit().map_err(SqliteShellError::Sqlite)?;
            return Ok(CommitStatus::IdempotentReplay);
        }

        if self.validated_history.contains_key(&candidate) {
            return Err(SqliteShellError::CorruptHistory);
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
        let stored_version = nonnegative_u64(version)?;
        if stored_root != self.validated_state_root
            || stored_version != self.validated_state_version
        {
            return Err(SqliteShellError::CorruptHistory);
        }
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
        let next_version = stored_version
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
                "INSERT INTO authorizations(authorization_id, state_version, policy_id, invocation_id, replay_id, candidate_id, authorization_bytes, bundle_bytes, receipt_bytes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    authorization_id.as_bytes().as_slice(),
                    sqlite_i64(next_version)?,
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
        let record = ValidatedStoredCandidate {
            state_version: next_version,
            authorization_id,
            policy_id: self.policy_id,
            invocation_id,
            replay_id,
            authorization_bytes,
            bundle_bytes,
            receipt_bytes,
            bundle: bundle.clone(),
        };
        if let Some(previous) = self.validated_history.insert(candidate, record) {
            self.validated_history.insert(candidate, previous);
            return Err(SqliteShellError::CorruptHistory);
        }
        if let Err(error) = transaction.commit() {
            self.validated_history.remove(&candidate);
            return Err(SqliteShellError::Sqlite(error));
        }
        self.validated_state_root = post_root;
        self.validated_state_version = next_version;
        if crash == Some(CrashPoint::AfterCommit) {
            return Err(SqliteShellError::InjectedCrash(CrashPoint::AfterCommit));
        }
        Ok(CommitStatus::Committed)
    }

    /// Returns the first pending outbox record in candidate/ordinal order.
    pub fn next_pending(&self) -> Result<Option<PendingDelivery>, SqliteShellError> {
        validate_shell_identity(
            &self.connection,
            self.policy_id,
            &self.state_domain_name,
            self.state_domain_version,
        )?;
        validate_cached_history(&self.connection, &self.validated_history)?;
        let row = self
            .connection
            .query_row(
                "SELECT delivery_id, entry_hash, authorization_id, candidate_id, ordinal, channel, destination_bytes, payload_bytes FROM outbox WHERE acknowledged = 0 ORDER BY candidate_id, ordinal LIMIT 1",
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
        let record = self
            .validated_history
            .get(&pending.candidate_id)
            .ok_or(SqliteShellError::CorruptHistory)?;
        validate_stored_candidate(&self.connection, record)?;
        validate_authorization_mapping(
            &self.connection,
            self.policy_id,
            pending.authorization_id,
            pending.candidate_id,
        )?;
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
        validate_shell_identity(
            &transaction,
            self.policy_id,
            &self.state_domain_name,
            self.state_domain_version,
        )?;
        validate_cached_history(&transaction, &self.validated_history)?;
        let row = transaction
            .query_row(
                "SELECT entry_hash, acknowledged, candidate_id FROM outbox WHERE delivery_id = ?1",
                [delivery_id.as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(SqliteShellError::Sqlite)?
            .ok_or(SqliteShellError::UnknownDelivery(delivery_id))?;
        let candidate = CandidateId::new(parse_hash(&row.2)?);
        let record = self
            .validated_history
            .get(&candidate)
            .ok_or(SqliteShellError::CorruptHistory)?;
        validate_stored_candidate(&transaction, record)?;
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

fn schema_version_and_table_count(connection: &Connection) -> Result<(i64, i64), SqliteShellError> {
    let version = connection
        .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
        .map_err(SqliteShellError::Sqlite)?;
    let existing_tables = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('shell_identity', 'genesis', 'semantic_state', 'authorizations', 'bundles', 'replay', 'outbox')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(SqliteShellError::Sqlite)?;
    Ok((version, existing_tables))
}

fn initialize_schema_for_create(connection: &Connection) -> Result<(), SqliteShellError> {
    let (version, existing_tables) = schema_version_and_table_count(connection)?;
    match version {
        0 if existing_tables == 0 => connection
            .execute_batch(SCHEMA_V5)
            .map_err(SqliteShellError::Sqlite),
        0 => Err(SqliteShellError::LegacySchema),
        SQLITE_SCHEMA_VERSION if existing_tables == 7 => connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")
            .map_err(SqliteShellError::Sqlite),
        SQLITE_SCHEMA_VERSION => Err(SqliteShellError::CorruptSchema),
        other => Err(SqliteShellError::UnsupportedSchemaVersion(other)),
    }
}

fn validate_existing_schema(connection: &Connection) -> Result<(), SqliteShellError> {
    let (version, existing_tables) = schema_version_and_table_count(connection)?;
    match version {
        0 if existing_tables == 0 => Err(SqliteShellError::UninitializedStore),
        0 => Err(SqliteShellError::LegacySchema),
        SQLITE_SCHEMA_VERSION if existing_tables == 7 => connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")
            .map_err(SqliteShellError::Sqlite),
        SQLITE_SCHEMA_VERSION => Err(SqliteShellError::CorruptSchema),
        other => Err(SqliteShellError::UnsupportedSchemaVersion(other)),
    }
}

fn validate_persisted_genesis<P, L, I>(
    connection: &Connection,
    authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, I>,
) -> Result<(GenesisId, Value), SqliteShellError>
where
    P: CatalogTransitionProgram<RustCryptoSha256>,
    L: ProjectLawEngine,
{
    type GenesisRow = (
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
    );
    let row = connection
        .query_row(
            "SELECT genesis_id, policy_id, genesis_binding_hash, initial_root, law_set_hash, law_evaluation_hash, initial_state_bytes, authorization_bytes FROM genesis WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()
        .map_err(SqliteShellError::Sqlite)?;
    let Some((
        stored_id,
        stored_policy,
        stored_binding,
        stored_root,
        stored_law_set,
        stored_evaluation,
        initial_state_bytes,
        stored_authorization,
    )): Option<GenesisRow> = row
    else {
        return Err(SqliteShellError::CorruptGenesis);
    };
    let value = decode_canonical_value(&initial_state_bytes)
        .map_err(|_| SqliteShellError::CorruptGenesis)?;
    let envelope = SchemaAdmittedEnvelope::try_new::<RustCryptoSha256>(
        authority.policy().catalog().schema(),
        value.clone(),
        ValidationLimits::default(),
    )
    .map_err(|_| SqliteShellError::CorruptGenesis)?;
    let expected = authority
        .authorize_genesis(envelope)
        .map_err(|_| SqliteShellError::CorruptGenesis)?;
    let expected_bytes = expected
        .canonical_bytes()
        .map_err(SqliteShellError::Encode)?;
    let body = expected.body();
    if parse_hash(&stored_id)? != expected.genesis_id().hash()
        || parse_hash(&stored_policy)? != body.policy_id()
        || parse_hash(&stored_binding)? != body.genesis_binding_hash()
        || parse_hash(&stored_root)? != body.initial_root()
        || parse_hash(&stored_law_set)? != body.law_set_hash()
        || parse_hash(&stored_evaluation)? != body.law_evaluation_hash()
        || stored_authorization != expected_bytes
    {
        return Err(SqliteShellError::CorruptGenesis);
    }
    let (current_state, current_root, current_version) = connection
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
    if nonnegative_u64(current_version)? == 0
        && (current_state != initial_state_bytes
            || parse_hash(&current_root)? != body.initial_root())
    {
        return Err(SqliteShellError::CorruptGenesis);
    }
    Ok((expected.genesis_id(), value))
}

#[derive(Debug)]
struct StoredAuthorizationRow {
    state_version: i64,
    authorization_id: Vec<u8>,
    policy_id: Vec<u8>,
    invocation_id: Vec<u8>,
    replay_id: Vec<u8>,
    candidate_id: Vec<u8>,
    authorization_bytes: Vec<u8>,
    bundle_bytes: Vec<u8>,
    receipt_bytes: Vec<u8>,
}

fn validate_persisted_history<P, L, I>(
    connection: &Connection,
    authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, I>,
    mut state: Value,
) -> Result<BTreeMap<CandidateId, ValidatedStoredCandidate>, SqliteShellError>
where
    P: CatalogTransitionProgram<RustCryptoSha256>,
    L: ProjectLawEngine,
{
    let rows = load_authorization_rows(connection)?;
    let domain = authority
        .policy()
        .state_domain()
        .domain()
        .map_err(|_| SqliteShellError::CorruptAuthorization)?;
    let mut history = BTreeMap::new();
    for (index, row) in rows.into_iter().enumerate() {
        let expected_version = u64::try_from(index)
            .map_err(|_| SqliteShellError::IntegerRange)?
            .checked_add(1)
            .ok_or(SqliteShellError::VersionOverflow)?;
        let state_version = nonnegative_u64(row.state_version)?;
        if state_version != expected_version {
            return Err(SqliteShellError::CorruptHistory);
        }
        let authorized = authority
            .reauthorize_canonical_transition(
                &row.authorization_bytes,
                AuthorizationDecodeLimits::default(),
            )
            .map_err(|_| SqliteShellError::CorruptAuthorization)?;
        let bundle = decode_commit_bundle::<RustCryptoSha256>(
            &row.bundle_bytes,
            &state,
            domain,
            BundleDecodeLimits::default(),
        )
        .map_err(|_| SqliteShellError::CorruptHistory)?;
        let receipt =
            decode_receipt::<RustCryptoSha256>(&row.receipt_bytes, ReceiptDecodeLimits::default())
                .map_err(|_| SqliteShellError::CorruptHistory)?;
        let candidate = bundle.candidate_id();
        if parse_hash(&row.authorization_id)? != authorized.authorization_id().hash()
            || parse_hash(&row.policy_id)? != authority.policy().policy_id()
            || parse_hash(&row.invocation_id)? != authorized.body().invocation_id()
            || parse_hash(&row.replay_id)? != authorized.replay_id()
            || CandidateId::new(parse_hash(&row.candidate_id)?) != candidate
            || authorized.bundle() != &bundle
            || bundle.receipt() != &receipt
        {
            return Err(SqliteShellError::CorruptHistory);
        }
        let record = ValidatedStoredCandidate {
            state_version,
            authorization_id: authorized.authorization_id().hash(),
            policy_id: authority.policy().policy_id(),
            invocation_id: authorized.body().invocation_id(),
            replay_id: authorized.replay_id(),
            authorization_bytes: row.authorization_bytes,
            bundle_bytes: row.bundle_bytes,
            receipt_bytes: row.receipt_bytes,
            bundle,
        };
        validate_stored_candidate(connection, &record)?;
        let applied = record
            .bundle
            .validate_and_apply::<RustCryptoSha256>(&state, domain)
            .map_err(|_| SqliteShellError::CorruptHistory)?;
        state = applied.into_parts().0;
        if history.insert(candidate, record).is_some() {
            return Err(SqliteShellError::CorruptHistory);
        }
    }
    validate_history_counts(connection, &history)?;
    validate_reconstructed_state(connection, &state, history.len(), domain)?;
    Ok(history)
}

fn load_authorization_rows(
    connection: &Connection,
) -> Result<Vec<StoredAuthorizationRow>, SqliteShellError> {
    let mut statement = connection
        .prepare(
            "SELECT state_version, authorization_id, policy_id, invocation_id, replay_id, candidate_id, authorization_bytes, bundle_bytes, receipt_bytes FROM authorizations ORDER BY state_version",
        )
        .map_err(SqliteShellError::Sqlite)?;
    let mapped = statement
        .query_map([], |row| {
            Ok(StoredAuthorizationRow {
                state_version: row.get(0)?,
                authorization_id: row.get(1)?,
                policy_id: row.get(2)?,
                invocation_id: row.get(3)?,
                replay_id: row.get(4)?,
                candidate_id: row.get(5)?,
                authorization_bytes: row.get(6)?,
                bundle_bytes: row.get(7)?,
                receipt_bytes: row.get(8)?,
            })
        })
        .map_err(SqliteShellError::Sqlite)?;
    mapped
        .collect::<Result<Vec<_>, _>>()
        .map_err(SqliteShellError::Sqlite)
}

fn validate_reconstructed_state(
    connection: &Connection,
    state: &Value,
    history_length: usize,
    domain: Domain<'_>,
) -> Result<(), SqliteShellError> {
    let (state_bytes, root_bytes, version) = connection
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
    let expected_bytes = state.canonical_bytes().map_err(SqliteShellError::Encode)?;
    let expected_root =
        hash_value::<RustCryptoSha256>(domain, state).map_err(SqliteShellError::Patch)?;
    let expected_version =
        u64::try_from(history_length).map_err(|_| SqliteShellError::IntegerRange)?;
    if state_bytes != expected_bytes
        || parse_hash(&root_bytes)? != expected_root
        || nonnegative_u64(version)? != expected_version
    {
        return Err(SqliteShellError::CorruptHistory);
    }
    Ok(())
}

fn validate_history_counts(
    connection: &Connection,
    history: &BTreeMap<CandidateId, ValidatedStoredCandidate>,
) -> Result<(), SqliteShellError> {
    let expected = u64::try_from(history.len()).map_err(|_| SqliteShellError::IntegerRange)?;
    if count_rows(connection, "authorizations")? != expected
        || count_rows(connection, "bundles")? != expected
        || count_rows(connection, "replay")? != expected
    {
        return Err(SqliteShellError::CorruptHistory);
    }
    let expected_outbox = history.values().try_fold(0_u64, |count, record| {
        let entries = u64::try_from(record.bundle.outbox_plan().entries().len())
            .map_err(|_| SqliteShellError::IntegerRange)?;
        count
            .checked_add(entries)
            .ok_or(SqliteShellError::IntegerRange)
    })?;
    if count_rows(connection, "outbox")? != expected_outbox {
        return Err(SqliteShellError::CorruptHistory);
    }
    Ok(())
}

fn validate_cached_history(
    connection: &Connection,
    history: &BTreeMap<CandidateId, ValidatedStoredCandidate>,
) -> Result<(), SqliteShellError> {
    for record in history.values() {
        validate_stored_candidate(connection, record)?;
    }
    validate_history_counts(connection, history)
}

fn validate_stored_candidate(
    connection: &Connection,
    record: &ValidatedStoredCandidate,
) -> Result<(), SqliteShellError> {
    let candidate = record.bundle.candidate_id();
    let authorization = connection
        .query_row(
            "SELECT state_version, policy_id, invocation_id, replay_id, authorization_bytes, bundle_bytes, receipt_bytes FROM authorizations WHERE authorization_id = ?1 AND candidate_id = ?2",
            params![
                record.authorization_id.as_bytes().as_slice(),
                candidate.hash().as_bytes().as_slice(),
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                ))
            },
        )
        .optional()
        .map_err(SqliteShellError::Sqlite)?
        .ok_or(SqliteShellError::CorruptHistory)?;
    if nonnegative_u64(authorization.0)? != record.state_version
        || parse_hash(&authorization.1)? != record.policy_id
        || parse_hash(&authorization.2)? != record.invocation_id
        || parse_hash(&authorization.3)? != record.replay_id
        || authorization.4 != record.authorization_bytes
        || authorization.5 != record.bundle_bytes
        || authorization.6 != record.receipt_bytes
    {
        return Err(SqliteShellError::CorruptHistory);
    }
    let bundle = connection
        .query_row(
            "SELECT authorization_id, bundle_bytes, receipt_bytes FROM bundles WHERE candidate_id = ?1",
            [candidate.hash().as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(SqliteShellError::Sqlite)?
        .ok_or(SqliteShellError::CorruptHistory)?;
    if parse_hash(&bundle.0)? != record.authorization_id
        || bundle.1 != record.bundle_bytes
        || bundle.2 != record.receipt_bytes
    {
        return Err(SqliteShellError::CorruptHistory);
    }
    let replay = connection
        .query_row(
            "SELECT authorization_id, candidate_id, authorization_bytes, bundle_bytes FROM replay WHERE replay_id = ?1",
            [record.replay_id.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(SqliteShellError::Sqlite)?
        .ok_or(SqliteShellError::CorruptHistory)?;
    if parse_hash(&replay.0)? != record.authorization_id
        || CandidateId::new(parse_hash(&replay.1)?) != candidate
        || replay.2 != record.authorization_bytes
        || replay.3 != record.bundle_bytes
    {
        return Err(SqliteShellError::CorruptHistory);
    }
    validate_exact_outbox_rows(connection, record)
}

type StoredOutboxRow = (
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    i64,
    i64,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    i64,
);

fn validate_exact_outbox_rows(
    connection: &Connection,
    record: &ValidatedStoredCandidate,
) -> Result<(), SqliteShellError> {
    let candidate = record.bundle.candidate_id();
    let mut statement = connection
        .prepare(
            "SELECT delivery_id, entry_hash, authorization_id, ordinal, channel, destination_bytes, payload_bytes, candidate_id, acknowledged FROM outbox WHERE candidate_id = ?1 ORDER BY ordinal",
        )
        .map_err(SqliteShellError::Sqlite)?;
    let mapped = statement
        .query_map([candidate.hash().as_bytes().as_slice()], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        })
        .map_err(SqliteShellError::Sqlite)?;
    let rows = mapped
        .collect::<Result<Vec<StoredOutboxRow>, _>>()
        .map_err(SqliteShellError::Sqlite)?;
    if rows.len() != record.bundle.outbox_plan().entries().len() {
        return Err(SqliteShellError::CorruptOutbox);
    }
    for (row, entry) in rows.iter().zip(record.bundle.outbox_plan().entries()) {
        let destination = entry
            .destination()
            .canonical_bytes()
            .map_err(SqliteShellError::Encode)?;
        let payload = entry
            .payload()
            .canonical_bytes()
            .map_err(SqliteShellError::Encode)?;
        let entry_hash = hash_outbox_entry(entry)?;
        let delivery_id = entry
            .delivery_id::<RustCryptoSha256>(candidate.hash())
            .map_err(SqliteShellError::Encode)?;
        if parse_hash(&row.0)? != delivery_id
            || parse_hash(&row.1)? != entry_hash
            || parse_hash(&row.2)? != record.authorization_id
            || nonnegative_u32(row.3)? != entry.ordinal()
            || nonnegative_u32(row.4)? != entry.channel()
            || row.5 != destination
            || row.6 != payload
            || CandidateId::new(parse_hash(&row.7)?) != candidate
            || !matches!(row.8, 0 | 1)
        {
            return Err(SqliteShellError::CorruptOutbox);
        }
    }
    Ok(())
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
        "authorizations" => "SELECT COUNT(*) FROM authorizations",
        "bundles" => "SELECT COUNT(*) FROM bundles",
        "replay" => "SELECT COUNT(*) FROM replay",
        "outbox" => "SELECT COUNT(*) FROM outbox",
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
    /// Creation was attempted over an already initialized authority store.
    AlreadyInitialized,
    /// Reopen was attempted on an empty database with no authorized genesis.
    UninitializedStore,
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
    /// Stored genesis authority cannot be reconstructed under the supplied policy.
    CorruptGenesis,
    /// Stored transition authorization cannot be re-admitted and re-executed exactly.
    CorruptAuthorization,
    /// Stored authorization, bundle, receipt, replay, state, or sequence rows differ.
    CorruptHistory,
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
            Self::AlreadyInitialized => formatter.write_str("SQLite shell is already initialized"),
            Self::UninitializedStore => {
                formatter.write_str("SQLite shell has no authorized genesis")
            }
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
            Self::CorruptGenesis => {
                formatter.write_str("stored genesis authorization is inconsistent")
            }
            Self::CorruptAuthorization => {
                formatter.write_str("stored transition authorization is inconsistent")
            }
            Self::CorruptHistory => {
                formatter.write_str("stored authorized history is inconsistent")
            }
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
        CatalogAuthorizationDecision, ExecutionBinding, GenesisPolicyBinding,
        ReviewedTransitionInput, StateDomainBinding,
    };
    use zeno_fcis_catalog::{
        CatalogLimits, CatalogManifest, ChannelDefinition, EffectDefinition, HashRequirement,
        OperationSemantics, ProjectCatalog,
    };
    use zeno_fcis_core::{BudgetUsed, Decision, DecisionKind};
    use zeno_fcis_crypto::verify_approved_provider;
    use zeno_fcis_evidence::EvidenceEnvelope;
    use zeno_fcis_laws::{
        DecisionScope, GenesisApplicability, GenesisLawCheckInput, LawCheckInput, LawDefinition,
        LawEvidenceRequirement, LawEvidenceVerifier, LawFamilyPolicy, LawKind, LawLimits,
        LawObservation, LawProofDecision, LawProofSubject, LawStatus, VerifiedProjectLaws,
        verify_project_laws,
    };
    use zeno_fcis_patch::ValuePath;
    use zeno_fcis_plan::Effect;
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
                GenesisApplicability::Required,
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
                GenesisApplicability::NotApplicable {
                    rationale_hash: hash(121),
                },
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
                GenesisApplicability::NotApplicable {
                    rationale_hash: hash(122),
                },
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
            OperationSemantics::non_value(hash(107))
                .unwrap_or_else(|error| panic!("semantics: {error}")),
            hash(7),
        )
        .unwrap_or_else(|error| panic!("channel: {error}"));
        let effect = EffectDefinition::try_new(
            id(8),
            name("decision-evidence"),
            TypeId::new(1),
            HashRequirement::Present,
            HashRequirement::Absent,
            OperationSemantics::non_value(hash(108))
                .unwrap_or_else(|error| panic!("semantics: {error}")),
            hash(8),
        )
        .unwrap_or_else(|error| panic!("effect: {error}"));
        let manifest =
            CatalogManifest::try_new::<RustCryptoSha256>(Vec::new(), vec![effect], vec![channel])
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
        fn evaluate_genesis(
            &self,
            _: &GenesisLawCheckInput<'_>,
            _: LawLimits,
        ) -> Result<Vec<LawObservation>, zeno_fcis_laws::LawEngineFailure> {
            Ok(vec![
                LawObservation::try_new(id(1_001), LawStatus::Satisfied, hash(91))
                    .unwrap_or_else(|error| panic!("genesis law observation: {error}")),
            ])
        }

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
            builder.emit(Effect::new(0, 8, hash(80), Hash32::ZERO, Value::U128(11)))?;
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
        let state = initial_state(catalog);
        let initial_root = hash_value::<RustCryptoSha256>(state_domain(), state.value().value())
            .unwrap_or_else(|error| panic!("initial root: {error}"));
        CatalogCommitAuthority::try_new(
            catalog,
            StateDomainBinding::try_new("test/sqlite-state", 1)
                .unwrap_or_else(|error| panic!("state domain: {error}")),
            ExecutionBinding::try_new(hash(50), hash(51), hash(52), hash(deployment), hash(54))
                .unwrap_or_else(|error| panic!("execution: {error}")),
            GenesisPolicyBinding::try_new(
                initial_root,
                hash(70),
                hash(71),
                hash(72),
                hash(deployment),
            )
            .unwrap_or_else(|error| panic!("genesis policy: {error}")),
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

    fn shell(authority: &TestAuthority, catalog: &ProjectCatalog) -> TestShell {
        let genesis = authority
            .authorize_genesis(initial_state(catalog))
            .unwrap_or_else(|error| panic!("genesis: {error}"));
        SqliteShell::create_in_memory(
            authority,
            genesis,
            authority.bind_delivery_interpreter(MemoryDestination::default()),
        )
        .unwrap_or_else(|error| panic!("shell: {error}"))
    }

    fn reopen(database: TestShell, authority: &TestAuthority) -> TestShell {
        let SqliteShell { connection, .. } = database;
        TestShell::from_existing_connection(
            connection,
            authority,
            authority.bind_delivery_interpreter(MemoryDestination::default()),
        )
        .unwrap_or_else(|error| panic!("reopen: {error}"))
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
        let expected_pending = expected
            .state()
            .next_pending()
            .unwrap_or_else(|| panic!("reference pending delivery"));
        let authorization_id = expected_authorization.authorization_id().hash();
        let mut database = shell(&authority, &catalog);
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
        let actual_pending = database
            .next_pending()
            .unwrap_or_else(|error| panic!("database pending: {error}"))
            .unwrap_or_else(|| panic!("database pending delivery"));
        assert_eq!(
            actual_pending.candidate_id(),
            expected_pending.candidate_id()
        );
        assert_eq!(actual_pending.entry(), expected_pending.entry());
        assert_eq!(actual_pending.entry_hash(), expected_pending.entry_hash());
        assert_eq!(actual_pending.delivery_id(), expected_pending.delivery_id());
        assert_eq!(
            actual_pending.delivery_id(),
            actual_pending
                .entry()
                .delivery_id::<RustCryptoSha256>(actual_pending.candidate_id().hash())
                .unwrap_or_else(|error| panic!("candidate delivery identity: {error}"))
        );
        assert_ne!(
            actual_pending.delivery_id(),
            actual_pending
                .entry()
                .delivery_id::<RustCryptoSha256>(authorization_id)
                .unwrap_or_else(|error| panic!("authorization substitution: {error}"))
        );
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
        for point in [
            CrashPoint::BeforeTransaction,
            CrashPoint::AfterValidation,
            CrashPoint::AfterStateWrite,
            CrashPoint::AfterReplayWrite,
            CrashPoint::AfterOutboxWrite,
            CrashPoint::BeforeCommit,
        ] {
            let mut database = shell(&authority, &catalog);
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
        let mut database = shell(&authority, &catalog);
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
        assert_eq!(database.delivery_interpreter().delivered_count(), 1);
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
    fn commit_evidence_is_persisted_without_delivery() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let authorized = authorized(&authority, &catalog, 19);
        assert_eq!(authorized.bundle().commit_plan().effects().len(), 1);
        let mut database = shell(&authority, &catalog);
        assert_eq!(
            database
                .commit(authorized)
                .unwrap_or_else(|error| panic!("commit: {error}")),
            CommitStatus::Committed
        );
        assert_eq!(database.delivery_interpreter().delivered_count(), 0);
        assert_eq!(
            database
                .snapshot()
                .unwrap_or_else(|error| panic!("snapshot: {error}"))
                .pending_outbox(),
            1
        );
        assert!(
            database
                .deliver_next()
                .unwrap_or_else(|error| panic!("deliver: {error}"))
        );
        assert_eq!(database.delivery_interpreter().delivered_count(), 1);
    }

    #[test]
    fn acknowledgement_binds_exact_entry_hash() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let mut database = shell(&authority, &catalog);
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
    fn delivery_rejects_payload_rewrite_even_with_recomputed_row_hashes() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let authorization = authorized(&authority, &catalog, 6);
        let candidate = authorization.bundle().candidate_id();
        let authorization_id = authorization.authorization_id().hash();
        let mut database = shell(&authority, &catalog);
        database
            .commit(authorization)
            .unwrap_or_else(|error| panic!("commit: {error}"));
        let replacement = OutboxEntry::new(
            0,
            7,
            Value::text_ascii(String::from("attacker"))
                .unwrap_or_else(|error| panic!("destination: {error}")),
            Value::U128(99),
        );
        let destination = replacement
            .destination()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("destination bytes: {error}"));
        let payload = replacement
            .payload()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("payload bytes: {error}"));
        let entry_hash =
            hash_outbox_entry(&replacement).unwrap_or_else(|error| panic!("entry hash: {error}"));
        let delivery_id = replacement
            .delivery_id::<RustCryptoSha256>(candidate.hash())
            .unwrap_or_else(|error| panic!("delivery id: {error}"));
        database
            .connection
            .execute(
                "UPDATE outbox SET delivery_id = ?1, authorization_id = ?2, channel = ?3, destination_bytes = ?4, payload_bytes = ?5, entry_hash = ?6 WHERE candidate_id = ?7 AND ordinal = 0",
                params![
                    delivery_id.as_bytes().as_slice(),
                    authorization_id.as_bytes().as_slice(),
                    i64::from(replacement.channel()),
                    destination,
                    payload,
                    entry_hash.as_bytes().as_slice(),
                    candidate.hash().as_bytes().as_slice(),
                ],
            )
            .unwrap_or_else(|error| panic!("rewrite outbox: {error}"));
        assert!(matches!(
            database.next_pending(),
            Err(SqliteShellError::CorruptOutbox)
        ));
    }

    #[test]
    fn replay_and_snapshot_reject_missing_or_extra_outbox_rows() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let authorization = authorized(&authority, &catalog, 6);
        let candidate = authorization.bundle().candidate_id();
        let mut missing = shell(&authority, &catalog);
        missing
            .commit(authorization)
            .unwrap_or_else(|error| panic!("commit missing fixture: {error}"));
        missing
            .connection
            .execute(
                "DELETE FROM outbox WHERE candidate_id = ?1",
                [candidate.hash().as_bytes().as_slice()],
            )
            .unwrap_or_else(|error| panic!("delete outbox: {error}"));
        assert!(matches!(
            missing.next_pending(),
            Err(SqliteShellError::CorruptOutbox) | Err(SqliteShellError::CorruptHistory)
        ));
        assert!(matches!(
            missing.commit(authorized(&authority, &catalog, 6)),
            Err(SqliteShellError::CorruptOutbox) | Err(SqliteShellError::CorruptHistory)
        ));

        let authorization = authorized(&authority, &catalog, 7);
        let candidate = authorization.bundle().candidate_id();
        let authorization_id = authorization.authorization_id().hash();
        let mut extra = shell(&authority, &catalog);
        extra
            .commit(authorization)
            .unwrap_or_else(|error| panic!("commit extra fixture: {error}"));
        let entry = OutboxEntry::new(
            1,
            7,
            Value::text_ascii(String::from("extra"))
                .unwrap_or_else(|error| panic!("extra destination: {error}")),
            Value::U128(1),
        );
        let destination = entry
            .destination()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("extra destination bytes: {error}"));
        let payload = entry
            .payload()
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("extra payload bytes: {error}"));
        let entry_hash =
            hash_outbox_entry(&entry).unwrap_or_else(|error| panic!("extra entry hash: {error}"));
        let delivery_id = entry
            .delivery_id::<RustCryptoSha256>(candidate.hash())
            .unwrap_or_else(|error| panic!("extra delivery id: {error}"));
        extra
            .connection
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
            .unwrap_or_else(|error| panic!("insert extra row: {error}"));
        assert!(matches!(
            extra.snapshot(),
            Err(SqliteShellError::CorruptOutbox) | Err(SqliteShellError::CorruptHistory)
        ));
    }

    #[test]
    fn reopen_rejects_redundant_bundle_bytes_and_noncontiguous_versions() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let mut bytes_tamper = shell(&authority, &catalog);
        bytes_tamper
            .commit(authorized(&authority, &catalog, 6))
            .unwrap_or_else(|error| panic!("commit: {error}"));
        bytes_tamper
            .connection
            .execute("UPDATE bundles SET bundle_bytes = X'00'", [])
            .unwrap_or_else(|error| panic!("tamper bundle bytes: {error}"));
        let SqliteShell { connection, .. } = bytes_tamper;
        assert!(matches!(
            TestShell::from_existing_connection(
                connection,
                &authority,
                authority.bind_delivery_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::CorruptHistory)
        ));

        let mut version_tamper = shell(&authority, &catalog);
        version_tamper
            .commit(authorized(&authority, &catalog, 7))
            .unwrap_or_else(|error| panic!("commit: {error}"));
        version_tamper
            .connection
            .execute("UPDATE authorizations SET state_version = 2", [])
            .unwrap_or_else(|error| panic!("tamper state version: {error}"));
        let SqliteShell { connection, .. } = version_tamper;
        assert!(matches!(
            TestShell::from_existing_connection(
                connection,
                &authority,
                authority.bind_delivery_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::CorruptHistory)
        ));
    }

    #[test]
    fn database_rejects_authorization_from_another_deployment_policy() {
        let catalog = catalog();
        let first_authority = authority(&catalog, 53);
        let other_authority = authority(&catalog, 55);
        let mut database = shell(&first_authority, &catalog);
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
        let genesis = first_authority
            .authorize_genesis(initial_state(&catalog))
            .unwrap_or_else(|error| panic!("genesis: {error}"));
        assert!(matches!(
            SqliteShell::create_in_memory(
                &first_authority,
                genesis,
                other_authority.bind_delivery_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::PolicyMismatch)
        ));
    }

    #[test]
    fn persisted_policy_corruption_is_detected_before_read_or_commit() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let mut database = shell(&authority, &catalog);
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
    fn reopen_revalidates_persisted_genesis_without_caller_state() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let mut database = shell(&authority, &catalog);
        let expected_genesis = database.genesis_id();
        database
            .commit(authorized(&authority, &catalog, 6))
            .unwrap_or_else(|error| panic!("commit before reopen: {error}"));
        let expected_snapshot = database
            .snapshot()
            .unwrap_or_else(|error| panic!("snapshot: {error}"));

        let reopened = reopen(database, &authority);

        assert_eq!(reopened.genesis_id(), expected_genesis);
        assert_eq!(
            reopened
                .snapshot()
                .unwrap_or_else(|error| panic!("reopened snapshot: {error}")),
            expected_snapshot
        );
    }

    #[test]
    fn reopen_rejects_version_zero_state_substitution() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let database = shell(&authority, &catalog);
        let replacement = Value::U128(1);
        let replacement_bytes = replacement
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("replacement bytes: {error}"));
        let replacement_root = hash_value::<RustCryptoSha256>(state_domain(), &replacement)
            .unwrap_or_else(|error| panic!("replacement root: {error}"));
        database
            .connection
            .execute(
                "UPDATE semantic_state SET state_bytes = ?1, semantic_root = ?2 WHERE singleton = 1",
                params![replacement_bytes, replacement_root.as_bytes().as_slice()],
            )
            .unwrap_or_else(|error| panic!("substitute version-zero state: {error}"));
        let SqliteShell { connection, .. } = database;

        assert!(matches!(
            TestShell::from_existing_connection(
                connection,
                &authority,
                authority.bind_delivery_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::CorruptGenesis)
        ));
    }

    #[test]
    fn live_snapshot_rejects_committed_state_substitution() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let mut database = shell(&authority, &catalog);
        database
            .commit(authorized(&authority, &catalog, 6))
            .unwrap_or_else(|error| panic!("commit: {error}"));

        let replacement = Value::U128(12);
        let replacement_bytes = replacement
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("replacement bytes: {error}"));
        let replacement_root = hash_value::<RustCryptoSha256>(state_domain(), &replacement)
            .unwrap_or_else(|error| panic!("replacement root: {error}"));
        database
            .connection
            .execute(
                "UPDATE semantic_state SET state_bytes = ?1, semantic_root = ?2 WHERE singleton = 1",
                params![replacement_bytes, replacement_root.as_bytes().as_slice()],
            )
            .unwrap_or_else(|error| panic!("substitute committed state: {error}"));

        assert!(matches!(
            database.snapshot(),
            Err(SqliteShellError::CorruptHistory)
        ));
    }

    #[test]
    fn reopen_rejects_tampered_genesis_authorization() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let database = shell(&authority, &catalog);
        database
            .connection
            .execute(
                "UPDATE genesis SET authorization_bytes = ?1 WHERE singleton = 1",
                [vec![0_u8]],
            )
            .unwrap_or_else(|error| panic!("tamper genesis: {error}"));
        let SqliteShell { connection, .. } = database;

        assert!(matches!(
            TestShell::from_existing_connection(
                connection,
                &authority,
                authority.bind_delivery_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::CorruptGenesis)
        ));
    }

    #[test]
    fn authorized_genesis_cannot_initialize_an_existing_store_twice() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let database = shell(&authority, &catalog);
        let SqliteShell { connection, .. } = database;
        let genesis = authority
            .authorize_genesis(initial_state(&catalog))
            .unwrap_or_else(|error| panic!("genesis: {error}"));

        assert!(matches!(
            TestShell::from_new_connection(
                connection,
                &authority,
                genesis,
                authority.bind_delivery_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::AlreadyInitialized)
        ));
    }

    #[test]
    fn reopen_rejects_a_different_deployment_policy() {
        let catalog = catalog();
        let first_authority = authority(&catalog, 53);
        let other_authority = authority(&catalog, 55);
        let database = shell(&first_authority, &catalog);
        let SqliteShell { connection, .. } = database;

        assert!(matches!(
            TestShell::from_existing_connection(
                connection,
                &other_authority,
                other_authority.bind_delivery_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::PolicyMismatch)
        ));
    }

    #[test]
    fn reopen_rejects_an_uninitialized_store() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let connection = Connection::open_in_memory()
            .unwrap_or_else(|error| panic!("uninitialized connection: {error}"));

        assert!(matches!(
            TestShell::from_existing_connection(
                connection,
                &authority,
                authority.bind_delivery_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::UninitializedStore)
        ));
    }

    #[test]
    fn legacy_unversioned_database_fails_closed() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let genesis = authority
            .authorize_genesis(initial_state(&catalog))
            .unwrap_or_else(|error| panic!("genesis: {error}"));
        let connection = Connection::open_in_memory()
            .unwrap_or_else(|error| panic!("legacy connection: {error}"));
        connection
            .execute_batch(
                "CREATE TABLE semantic_state(singleton INTEGER PRIMARY KEY, state_bytes BLOB, semantic_root BLOB, version INTEGER);",
            )
            .unwrap_or_else(|error| panic!("legacy schema: {error}"));
        assert!(matches!(
            SqliteShell::<SqliteProgram, SqliteLawEngine, MemoryDestination>::from_new_connection(
                connection,
                &authority,
                genesis,
                authority.bind_delivery_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::LegacySchema)
        ));
    }

    #[test]
    fn schema_v3_requires_explicit_genesis_migration() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let genesis = authority
            .authorize_genesis(initial_state(&catalog))
            .unwrap_or_else(|error| panic!("genesis: {error}"));
        let connection =
            Connection::open_in_memory().unwrap_or_else(|error| panic!("v3 connection: {error}"));
        connection
            .pragma_update(None, "user_version", 3)
            .unwrap_or_else(|error| panic!("v3 schema version: {error}"));
        assert!(matches!(
            SqliteShell::<SqliteProgram, SqliteLawEngine, MemoryDestination>::from_new_connection(
                connection,
                &authority,
                genesis,
                authority.bind_delivery_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::UnsupportedSchemaVersion(3))
        ));
    }

    #[test]
    fn schema_v4_requires_explicit_history_migration() {
        let catalog = catalog();
        let authority = authority(&catalog, 53);
        let genesis = authority
            .authorize_genesis(initial_state(&catalog))
            .unwrap_or_else(|error| panic!("genesis: {error}"));
        let connection =
            Connection::open_in_memory().unwrap_or_else(|error| panic!("v4 connection: {error}"));
        connection
            .pragma_update(None, "user_version", 4)
            .unwrap_or_else(|error| panic!("v4 schema version: {error}"));
        assert!(matches!(
            SqliteShell::<SqliteProgram, SqliteLawEngine, MemoryDestination>::from_new_connection(
                connection,
                &authority,
                genesis,
                authority.bind_delivery_interpreter(MemoryDestination::default()),
            ),
            Err(SqliteShellError::UnsupportedSchemaVersion(4))
        ));
    }
}
