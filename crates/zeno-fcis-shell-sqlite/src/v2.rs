//! SQLite schema v10 stores compact genuine publications under the admitted
//! identities of a contract lineage. Full reconstructed publications bind replay,
//! certificates and delivery IDs. The functional core owns decisions; SQLite,
//! SHA-256 and destination acknowledgements remain trusted shell assumptions.
//! Identity copying is outside the logical byte cap. Private checkpoints
//! require trusted provenance and do not authenticate hostile stores.
//!
//! A store starts under one Authority. A checked upgrade appends a chained
//! record that moves the head to a later contract of the application's
//! lineage with the same state schema: a program successor at any state, and
//! any other contract only when its genesis laws admit the current state (see
//! [`upgrade`](crate::v2::upgrade)). Each history segment replays under the
//! Authority it was published with, so a store with upgrades is opened through
//! its [`Lineage`](crate::v2::Lineage), oldest version first, which yields a
//! typed handle.
//!
//! The chain shows that every segment is valid under the lineage version it
//! names. It is not keyed: anyone who can write the file can append a
//! well-formed record or roll the store back to an earlier valid head, and
//! detecting that needs a tip held outside the file, such as an
//! [`UpgradeReceipt`](crate::v2::UpgradeReceipt) or a private
//! [`Checkpoint`](crate::v2::Checkpoint).

use crate::CrashPoint;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use std::{fmt, path::Path};
use zeno_fcis_codec::{
    CanonicalEncode, DecodeLimits, Domain, EncodeError, Hash32, commitment, decode_value,
};
use zeno_fcis_crypto::{RustCryptoSha256, verify_approved_provider};
use zeno_fcis_plan::OutboxEntry;
use zeno_fcis_shell::{CommitStatus, IdempotentDestination, MemoryDestination};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, Publication, PublicationOutcome, WireDelivery},
    v2_catalog::BoundCatalog,
    v2_composition::{Kind, Raw},
};

mod compact;
mod lineage;
/// The pure upgrade decision and the lineage assignment.
pub mod upgrade;
pub use compact::{compact_publication, expand_publication};
pub use lineage::{Lineage, Opened, Store, Superseded, V9Store};

/// The tables of schema v9, which v10 keeps unchanged.
const HISTORY: &str = "
CREATE TABLE v2_genesis (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 identity BLOB NOT NULL, initial BLOB NOT NULL, publication BLOB NOT NULL,
 delivery_interpreter BLOB NOT NULL CHECK(length(delivery_interpreter)=32),
 chain BLOB NOT NULL CHECK(length(chain)=32)
);
CREATE TABLE v2_state (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 sequence INTEGER NOT NULL CHECK(sequence>=0), state BLOB NOT NULL,
 root BLOB NOT NULL CHECK(length(root)=32), chain BLOB NOT NULL CHECK(length(chain)=32)
);
CREATE TABLE v2_commits (
 sequence INTEGER PRIMARY KEY CHECK(sequence>0),
 replay_id BLOB NOT NULL UNIQUE CHECK(length(replay_id)=32),
 pre_root BLOB NOT NULL CHECK(length(pre_root)=32),
 post_root BLOB NOT NULL CHECK(length(post_root)=32),
 previous_chain BLOB NOT NULL CHECK(length(previous_chain)=32),
 chain BLOB NOT NULL UNIQUE CHECK(length(chain)=32),
 command BLOB NOT NULL, context BLOB NOT NULL, post BLOB NOT NULL,
 publication BLOB NOT NULL, certificate BLOB NOT NULL
);
CREATE TABLE v2_deliveries (
 sequence INTEGER NOT NULL REFERENCES v2_commits(sequence),
 lane INTEGER NOT NULL CHECK(lane IN (0,1)),
 ordinal INTEGER NOT NULL CHECK(ordinal>=0),
 channel INTEGER NOT NULL CHECK(channel>=0),
 destination_root INTEGER NOT NULL CHECK(destination_root>=0),
 payload_root INTEGER NOT NULL CHECK(payload_root>=0),
 destination BLOB NOT NULL, payload BLOB NOT NULL, marker BLOB NOT NULL,
 delivery_id BLOB NOT NULL UNIQUE CHECK(length(delivery_id)=32),
 entry_hash BLOB NOT NULL CHECK(length(entry_hash)=32),
 acknowledged INTEGER NOT NULL CHECK(acknowledged IN (0,1)),
 PRIMARY KEY(sequence,lane,ordinal)
);
CREATE INDEX v2_pending ON v2_deliveries(acknowledged,sequence,lane,ordinal);
CREATE TABLE v2_checkpoints (
 sequence INTEGER PRIMARY KEY CHECK(sequence>=0),
 root BLOB NOT NULL CHECK(length(root)=32), chain BLOB NOT NULL CHECK(length(chain)=32)
);
";
/// Schema v10 adds the chained upgrade records. `ordinal` orders them;
/// `sequence` is the head each one was recorded at; `identity` is the full
/// identity of the contract upgraded to; `publication` is the admission the
/// record binds: that contract's compact genesis publication over the state
/// at the head for a genesis admission, or the adoption receipt digests for a
/// program successor. The record's magic names its kind.
const UPGRADES: &str = "
CREATE TABLE v2_upgrades (
 ordinal INTEGER PRIMARY KEY CHECK(ordinal>0),
 sequence INTEGER NOT NULL CHECK(sequence>=0),
 identity BLOB NOT NULL, publication BLOB NOT NULL,
 root BLOB NOT NULL CHECK(length(root)=32),
 previous_chain BLOB NOT NULL CHECK(length(previous_chain)=32),
 record BLOB NOT NULL, chain BLOB NOT NULL UNIQUE CHECK(length(chain)=32)
);
";
const SCHEMA_VERSION: i64 = 10;

/// The complete current schema a new store receives.
fn schema_v10() -> String {
    format!("{HISTORY}{UPGRADES}PRAGMA user_version = {SCHEMA_VERSION};\n")
}
/// Schema v9 exactly as the previous shell created it: the history tables alone.
fn schema_v9() -> String {
    format!("{HISTORY}PRAGMA user_version = 9;\n")
}
/// The one explicit migration: the upgrade table and the new version, so a
/// migrated store's catalog equals a new store's.
fn migration_v9_to_v10() -> String {
    format!("{UPGRADES}PRAGMA user_version = {SCHEMA_VERSION};\n")
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Anchor {
    sequence: i64,
    state: Vec<u8>,
    root: Hash32,
    chain: Hash32,
}

/// A validated prefix capability. Its private fields cannot be imported from DB bytes.
#[derive(Clone, Debug)]
pub struct Checkpoint {
    identity: Vec<u8>,
    genesis: Hash32,
    anchor: Anchor,
}

/// Coherent checked head with the exact original state and actual persisted identity.
/// Owning/cloning H adds O(H) work outside the invocation meter and transition cap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    anchor: Anchor,
    binding: Vec<u8>,
    bundles: u64,
    replays: u64,
    pending: u64,
    upgrades: u64,
}
impl Snapshot {
    /// Number of committed transitions after genesis.
    pub fn version(&self) -> u64 {
        self.anchor.sequence as u64
    }
    /// Exact checked original ZFCISV1 envelope.
    pub fn state(&self) -> &[u8] {
        &self.anchor.state
    }
    /// Actual full stored identity of the current contract, admitted in the
    /// same transaction as this head.
    pub fn binding(&self) -> &[u8] {
        &self.binding
    }
    /// Actual number of persisted committing records.
    pub fn bundle_count(&self) -> u64 {
        self.bundles
    }
    /// Actual number of distinct persisted replay keys.
    pub fn replay_count(&self) -> u64 {
        self.replays
    }
    /// Library-computed commitment to the complete envelope.
    pub fn root(&self) -> Hash32 {
        self.anchor.root
    }
    /// Complete validated history hash chain tip.
    pub fn chain(&self) -> Hash32 {
        self.anchor.chain
    }
    /// Number of unacknowledged obligations in both delivery lanes.
    pub fn pending(&self) -> u64 {
        self.pending
    }
    /// Number of recorded contract upgrades; the history has one more segment.
    pub fn upgrades(&self) -> u64 {
        self.upgrades
    }
}

/// Durable certificate bound to the actual pre-state root and core publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitReceipt {
    sequence: u64,
    chain: Hash32,
    status: CommitStatus,
}
impl CommitReceipt {
    /// Original commit position, also returned by an idempotent replay.
    pub fn version(&self) -> u64 {
        self.sequence
    }
    /// Hash chain link binding this certificate.
    pub fn chain(&self) -> Hash32 {
        self.chain
    }
    /// Whether this call committed or matched a previously committed invocation.
    pub fn status(&self) -> CommitStatus {
        self.status
    }
}

/// A recorded contract upgrade: the chained record and what it binds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeReceipt {
    kind: upgrade::Kind,
    premises: Option<upgrade::Premises>,
    ordinal: u64,
    sequence: u64,
    chain: Hash32,
    from_identity: Vec<u8>,
    identity: Vec<u8>,
    receipts: Vec<Hash32>,
    record: Vec<u8>,
}
impl UpgradeReceipt {
    /// How the new contract admitted the store's state.
    pub fn kind(&self) -> upgrade::Kind {
        self.kind
    }
    /// For a program successor, the premises the shell checked and the
    /// record binds; `None` for a genesis admission.
    pub fn premises(&self) -> Option<upgrade::Premises> {
        self.premises
    }
    /// Position among the store's upgrades, from 1.
    pub fn ordinal(&self) -> u64 {
        self.ordinal
    }
    /// For a program successor, the SHA-256 of each adoption receipt between
    /// the two versions, oldest first, as the upgrading lineage carried them;
    /// empty for a genesis admission. They name the evidence an auditor
    /// replays; the shell itself replays no receipt.
    pub fn receipts(&self) -> &[Hash32] {
        &self.receipts
    }
    /// The commit position the upgrade was recorded at; later commits run
    /// under the new contract.
    pub fn version(&self) -> u64 {
        self.sequence
    }
    /// The hash chain tip after the upgrade.
    pub fn chain(&self) -> Hash32 {
        self.chain
    }
    /// Full identity of the contract the store ran before.
    pub fn from_identity(&self) -> &[u8] {
        &self.from_identity
    }
    /// Full identity of the contract the store runs now.
    pub fn identity(&self) -> &[u8] {
        &self.identity
    }
    /// The canonical record whose `V2_CHAIN` hash is the new chain tip.
    pub fn record(&self) -> &[u8] {
        &self.record
    }
}

/// Exact pending data, retained in commit order rather than hash order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pending {
    sequence: u64,
    lane: u8,
    ordinal: u32,
    channel: u32,
    destination_root: u32,
    payload_root: u32,
    destination: Vec<u8>,
    payload: Vec<u8>,
    marker: Vec<u8>,
    delivery_id: Hash32,
    entry_hash: Hash32,
}
impl Pending {
    /// Owning commit position.
    pub fn version(&self) -> u64 {
        self.sequence
    }
    /// Zero for effects, one for durable outbox entries.
    pub fn lane(&self) -> u8 {
        self.lane
    }
    /// Ordinal checked by the functional core.
    pub fn ordinal(&self) -> u32 {
        self.ordinal
    }
    /// Declared and checked channel.
    pub fn channel(&self) -> u32 {
        self.channel
    }
    /// Original declared destination type root.
    pub fn destination_root(&self) -> u32 {
        self.destination_root
    }
    /// Original declared payload type root.
    pub fn payload_root(&self) -> u32 {
        self.payload_root
    }
    /// Complete bare canonical destination bytes.
    pub fn destination(&self) -> &[u8] {
        &self.destination
    }
    /// Complete bare canonical payload bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
    /// Declared marker, which is not by itself a globally unique delivery ID.
    pub fn marker(&self) -> &[u8] {
        &self.marker
    }
    /// ID computed from the genuine publication and exact original entry bytes.
    pub fn delivery_id(&self) -> Hash32 {
        self.delivery_id
    }
    /// Commitment to the complete original OutboxEntry encoding.
    pub fn entry_hash(&self) -> Hash32 {
        self.entry_hash
    }
}

/// The contract versions a handle replays with, oldest first; the last one is
/// the contract the store runs now.
#[derive(Clone, Copy)]
enum Members<'a, 'p> {
    /// Created or opened with one Authority: a store without upgrades.
    One(&'a Authority<'p>),
    /// Opened through a lineage, bound to its first `len` versions.
    Lineage {
        lineage: &'a Lineage<'a, 'p>,
        len: usize,
    },
}
impl<'a, 'p> Members<'a, 'p> {
    fn authorities(self) -> &'a [Authority<'p>] {
        match self {
            Self::One(authority) => std::slice::from_ref(authority),
            Self::Lineage { lineage, len } => &lineage.authorities()[..len],
        }
    }
    /// Each version's checked catalog, which a program-successor record needs;
    /// none for a handle opened with one Authority, whose store has no upgrade.
    fn catalogs(self) -> &'a [&'a BoundCatalog<'p>] {
        match self {
            Self::One(_) => &[],
            Self::Lineage { lineage, len } => &lineage.catalogs()[..len],
        }
    }
    fn last(self) -> &'a Authority<'p> {
        self.authorities()
            .last()
            .unwrap_or_else(|| unreachable!("a handle is bound to at least one Authority"))
    }
}

/// SQLite adapter bound to immutable library Authorities: one, or a prefix of
/// a contract lineage, oldest first. The last one is the contract the store
/// runs now. There is no application evaluator.
pub struct V2SqliteShell<'a, 'p> {
    members: Members<'a, 'p>,
    connection: Connection,
    anchor: Anchor,
    genesis: Hash32,
    delivery_interpreter: Hash32,
    data_version: i64,
}

impl<'a, 'p> V2SqliteShell<'a, 'p> {
    /// Atomically initialize a new file from a genuine checked genesis capability.
    pub fn create(
        path: impl AsRef<Path>,
        authority: &'a Authority<'p>,
        genesis: Publication<'_>,
    ) -> Result<Self, Error> {
        Self::initialize(Connection::open(path)?, authority, genesis)
    }
    /// Initialize an isolated SQLite memory store from genuine genesis.
    pub fn create_in_memory(
        authority: &'a Authority<'p>,
        genesis: Publication<'_>,
    ) -> Result<Self, Error> {
        Self::initialize(Connection::open_in_memory()?, authority, genesis)
    }
    fn initialize(
        mut connection: Connection,
        authority: &'a Authority<'p>,
        genesis: Publication<'_>,
    ) -> Result<Self, Error> {
        provider()?;
        let delivery_interpreter = MemoryDestination::interpreter_identity()?;
        configure(&connection)?;
        if authority.identity() != genesis.identity() {
            return Err(Error::Identity);
        }
        if genesis.evaluation().kind() != Kind::Genesis {
            return Err(Error::InvocationKind);
        }
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Every schema object counts. A `LIKE 'sqlite_%'` filter would also hide
        // user objects named e.g. `sqlitex` (`_` is a wildcard, case ignored).
        let count: i64 = tx.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))?;
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if count != 0 || version != 0 {
            return Err(Error::Schema(version));
        }
        let compact = compact_publication(genesis.identity(), genesis.subject())?;
        let root = hash(zeno_fcis_codec::domains::V2_STATE, genesis.poststate())?;
        let chain = framed_hash(
            zeno_fcis_codec::domains::V2_GENESIS,
            &[
                genesis.identity(),
                genesis.subject(),
                root.as_bytes(),
                delivery_interpreter.as_bytes(),
            ],
        )?;
        tx.execute_batch(&schema_v10())?;
        check_schema(&tx)?;
        tx.execute(
            "INSERT INTO v2_genesis VALUES(1,?1,?2,?3,?4,?5)",
            params![
                genesis.identity(),
                genesis.poststate(),
                compact,
                delivery_interpreter.as_bytes().as_slice(),
                chain.as_bytes().as_slice()
            ],
        )?;
        tx.execute(
            "INSERT INTO v2_state VALUES(1,0,?1,?2,?3)",
            params![
                genesis.poststate(),
                root.as_bytes().as_slice(),
                chain.as_bytes().as_slice()
            ],
        )?;
        tx.execute(
            "INSERT INTO v2_checkpoints VALUES(0,?1,?2)",
            params![root.as_bytes().as_slice(), chain.as_bytes().as_slice()],
        )?;
        let data_version = data_version(&tx)?;
        tx.commit()?;
        let anchor = Anchor {
            sequence: 0,
            state: genesis.poststate().to_vec(),
            root,
            chain,
        };
        Ok(Self {
            members: Members::One(authority),
            connection,
            anchor,
            genesis: chain,
            delivery_interpreter,
            data_version,
        })
    }
    /// Re-admit genesis and recompute every complete decision and delivery on
    /// reopening a store that never upgraded. A store with upgrades opens
    /// through its [`Lineage`]. A missing file is refused, never created.
    pub fn open(path: impl AsRef<Path>, authority: &'a Authority<'p>) -> Result<Self, Error> {
        Self::reopen(open_existing(path)?, Members::One(authority), None).map(|(shell, _)| shell)
    }
    /// Check the tail after a privately validated prefix. The checkpoint is a trusted input.
    pub fn open_at_checkpoint(
        path: impl AsRef<Path>,
        authority: &'a Authority<'p>,
        checkpoint: &Checkpoint,
    ) -> Result<Self, Error> {
        Self::reopen(
            open_existing(path)?,
            Members::One(authority),
            Some(checkpoint),
        )
        .map(|(shell, _)| shell)
    }
    /// Audit a schema v10 store under `members`: every history segment
    /// replays under the member that published it. Returns the handle and the
    /// position of the store's current segment among the members.
    fn reopen(
        mut connection: Connection,
        members: Members<'a, 'p>,
        checkpoint: Option<&Checkpoint>,
    ) -> Result<(Self, usize), Error> {
        provider()?;
        configure(&connection)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_schema(&tx)?;
        let authorities = members.authorities();
        let segments = Segments::load(&tx, authorities, load_upgrades(&tx)?)?;
        let (genesis, initial) = genesis_anchor(&tx, segments.authority(authorities, 0))?;
        let delivery_interpreter = MemoryDestination::interpreter_identity()?;
        let (start, consumed) = match checkpoint {
            None => (initial, 0),
            Some(c) => {
                if c.genesis != genesis {
                    return Err(Error::Identity);
                }
                let consumed = check_checkpoint(&tx, &segments, c)?;
                if c.identity != segments.authority(authorities, consumed).identity() {
                    return Err(Error::Identity);
                }
                (c.anchor.clone(), consumed)
            }
        };
        let anchor = audit_tail(
            &tx,
            authorities,
            members.catalogs(),
            &segments,
            start,
            consumed,
        )?;
        let position = segments.position(segments.current());
        let data_version = data_version(&tx)?;
        tx.commit()?;
        let shell = Self {
            members,
            connection,
            anchor,
            genesis,
            delivery_interpreter,
            data_version,
        };
        Ok((shell, position))
    }
    /// The contract the store runs now: the last member this handle is bound to.
    pub fn authority(&self) -> &'a Authority<'p> {
        self.current()
    }
    /// Every contract version this handle replays with, oldest first.
    pub fn lineage(&self) -> &'a [Authority<'p>] {
        self.members.authorities()
    }
    fn current(&self) -> &'a Authority<'p> {
        self.members.last()
    }
    /// Revalidate all history explicitly, including acknowledged delivery data.
    pub fn audit(&mut self) -> Result<Checkpoint, Error> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_schema(&tx)?;
        let authorities = self.members.authorities();
        let segments = Segments::load(&tx, authorities, load_upgrades(&tx)?)?;
        segments.require_current(authorities)?;
        let (genesis, initial) = genesis_anchor(&tx, segments.authority(authorities, 0))?;
        let anchor = audit_tail(
            &tx,
            authorities,
            self.members.catalogs(),
            &segments,
            initial,
            0,
        )?;
        if genesis != self.genesis {
            return Err(Error::History);
        }
        save_checkpoint(&tx, &anchor)?;
        let data_version = data_version(&tx)?;
        tx.commit()?;
        self.anchor = anchor;
        self.data_version = data_version;
        Ok(self.checkpoint_value())
    }
    // Normal reads validate the cached tip, not the entire history. A write by
    // another connection invalidates this cache and forces a coherent full audit.
    fn synchronize(&mut self) -> Result<(), Error> {
        if data_version(&self.connection)? != self.data_version {
            self.audit()?;
        }
        Ok(())
    }
    fn checkpoint_value(&self) -> Checkpoint {
        Checkpoint {
            identity: self.current().identity().to_vec(),
            genesis: self.genesis,
            anchor: self.anchor.clone(),
        }
    }
    /// Persist a validated prefix marker and return its private checkpoint capability.
    pub fn checkpoint(&mut self) -> Result<Checkpoint, Error> {
        self.synchronize()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_cached_tip(
            &tx,
            &self.anchor,
            self.data_version,
            self.members.authorities(),
        )?;
        save_checkpoint(&tx, &self.anchor)?;
        tx.commit()?;
        Ok(self.checkpoint_value())
    }
    /// Capture actual admitted identity, head and durable counts in one checked transaction.
    /// Reading/copying H is O(H) outside the original invocation logical meter.
    pub fn snapshot(&mut self) -> Result<Snapshot, Error> {
        self.synchronize()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let segments = check_cached_tip(
            &tx,
            &self.anchor,
            self.data_version,
            self.members.authorities(),
        )?;
        let binding = segments.identity(segments.current()).to_vec();
        let (bundles, replays): (i64, i64) = tx.query_row(
            "SELECT count(*),count(DISTINCT replay_id) FROM v2_commits",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let pending: i64 = tx.query_row(
            "SELECT count(*) FROM v2_deliveries WHERE acknowledged=0",
            [],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(Snapshot {
            anchor: self.anchor.clone(),
            binding,
            bundles: u64::try_from(bundles).map_err(|_| Error::History)?,
            replays: u64::try_from(replays).map_err(|_| Error::History)?,
            pending: u64::try_from(pending).map_err(|_| Error::History)?,
            upgrades: u64::try_from(segments.upgrades.len()).map_err(|_| Error::History)?,
        })
    }
    /// Atomically publish a private capability. Reusing a key requires exact original bytes.
    pub fn commit(
        &mut self,
        replay_id: Hash32,
        publication: Publication<'_>,
    ) -> Result<CommitReceipt, Error> {
        self.commit_with_crash(replay_id, publication, None)
    }
    /// Commit against the exact expected sequence, retaining ABA freshness through CAS.
    pub fn commit_at(
        &mut self,
        expected_version: u64,
        replay_id: Hash32,
        publication: Publication<'_>,
    ) -> Result<CommitReceipt, Error> {
        self.commit_with_crash_at(expected_version, replay_id, publication, None)
    }
    /// Expected-version commit with the same seven original crash boundaries.
    pub fn commit_with_crash_at(
        &mut self,
        expected_version: u64,
        replay_id: Hash32,
        publication: Publication<'_>,
        crash: Option<CrashPoint>,
    ) -> Result<CommitReceipt, Error> {
        self.commit_inner(replay_id, publication, crash, Some(expected_version), None)
    }
    /// Bound the original four-component aggregate before any transactional writes.
    /// The limit is a shell resource request. Sizes come from complete genuine
    /// artifacts, never from caller-reported lengths. Applications retain their
    /// own declared maximum in addition to this requested limit.
    pub fn commit_bounded_at(
        &mut self,
        expected_version: u64,
        replay_id: Hash32,
        publication: Publication<'_>,
        maximum_bytes: usize,
        crash: Option<CrashPoint>,
    ) -> Result<CommitReceipt, Error> {
        self.commit_inner(
            replay_id,
            publication,
            crash,
            Some(expected_version),
            Some(maximum_bytes),
        )
    }
    /// Commit with a deterministic injected interruption for crash-refinement tests.
    pub fn commit_with_crash(
        &mut self,
        replay_id: Hash32,
        publication: Publication<'_>,
        crash: Option<CrashPoint>,
    ) -> Result<CommitReceipt, Error> {
        self.commit_inner(replay_id, publication, crash, None, None)
    }
    fn commit_inner(
        &mut self,
        replay_id: Hash32,
        publication: Publication<'_>,
        crash: Option<CrashPoint>,
        expected_version: Option<u64>,
        byte_limit: Option<usize>,
    ) -> Result<CommitReceipt, Error> {
        if publication.identity() != self.current().identity() {
            return Err(Error::Identity);
        }
        if publication.evaluation().kind() != Kind::Transition {
            return Err(Error::InvocationKind);
        }
        let compact = compact_publication(publication.identity(), publication.subject())?;
        self.synchronize()?;
        inject(crash, CrashPoint::BeforeTransaction)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let segments = check_cached_tip(
            &tx,
            &self.anchor,
            self.data_version,
            self.members.authorities(),
        )?;
        if expected_version.is_some_and(|expected| expected != self.anchor.sequence as u64) {
            return Err(Error::Concurrent);
        }
        let previous: Option<i64> = tx
            .query_row(
                "SELECT sequence FROM v2_commits WHERE replay_id=?1",
                [replay_id.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(sequence) = previous {
            let row = load_commit(&tx, sequence)?;
            check_commit(
                &tx,
                self.members.authorities(),
                &segments,
                &row,
                publication.evaluation().raw().state,
            )?;
            if row.publication != compact
                || row.command != publication.evaluation().raw().command
                || row.context != publication.evaluation().raw().context
            {
                return Err(Error::Replay);
            }
            return Ok(CommitReceipt {
                sequence: sequence as u64,
                chain: row.chain,
                status: CommitStatus::IdempotentReplay,
            });
        }
        let raw = publication.evaluation().raw();
        if raw.state != self.anchor.state {
            return Err(Error::PreState);
        }
        if let Some(declared) = byte_limit {
            let required = complete_bundle(&self.anchor, replay_id, &publication)?
                .sizes()
                .total()?;
            if required > declared {
                return Err(Error::Capacity { required, declared });
            }
        }
        let sequence = self.anchor.sequence.checked_add(1).ok_or(Error::Range)?;
        let post_root = hash(zeno_fcis_codec::domains::V2_STATE, publication.poststate())?;
        let certificate = certificate_bytes(
            sequence,
            replay_id,
            self.anchor.root,
            post_root,
            self.anchor.chain,
            publication.subject(),
        )?;
        let chain = hash(zeno_fcis_codec::domains::V2_CHAIN, &certificate)?;
        inject(crash, CrashPoint::AfterValidation)?;
        let changed = tx.execute("UPDATE v2_state SET sequence=?1,state=?2,root=?3,chain=?4 WHERE singleton=1 AND sequence=?5 AND root=?6 AND chain=?7", params![sequence, publication.poststate(), post_root.as_bytes().as_slice(), chain.as_bytes().as_slice(), self.anchor.sequence, self.anchor.root.as_bytes().as_slice(), self.anchor.chain.as_bytes().as_slice()])?;
        if changed != 1 {
            return Err(Error::Concurrent);
        }
        inject(crash, CrashPoint::AfterStateWrite)?;
        tx.execute(
            "INSERT INTO v2_commits VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                sequence,
                replay_id.as_bytes().as_slice(),
                self.anchor.root.as_bytes().as_slice(),
                post_root.as_bytes().as_slice(),
                self.anchor.chain.as_bytes().as_slice(),
                chain.as_bytes().as_slice(),
                raw.command,
                raw.context,
                publication.poststate(),
                compact,
                certificate
            ],
        )?;
        inject(crash, CrashPoint::AfterReplayWrite)?;
        insert_deliveries(&tx, sequence, &publication, &certificate)?;
        inject(crash, CrashPoint::AfterOutboxWrite)?;
        inject(crash, CrashPoint::BeforeCommit)?;
        let data_version = data_version(&tx)?;
        tx.commit()?;
        self.anchor = Anchor {
            sequence,
            state: publication.poststate().to_vec(),
            root: post_root,
            chain,
        };
        self.data_version = data_version;
        inject(crash, CrashPoint::AfterCommit)?;
        Ok(CommitReceipt {
            sequence: sequence as u64,
            chain,
            status: CommitStatus::Committed,
        })
    }
    /// Read the actual persisted immutable identity of the current contract and
    /// re-admit it while holding a transaction.
    /// This copies and compares O(H) metadata outside the invocation logical meter.
    pub fn binding(&mut self) -> Result<Vec<u8>, Error> {
        self.synchronize()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let segments = check_cached_tip(
            &tx,
            &self.anchor,
            self.data_version,
            self.members.authorities(),
        )?;
        let binding = segments.identity(segments.current()).to_vec();
        tx.commit()?;
        Ok(binding)
    }
    /// Canonically encode every persisted per-transition row and copied field.
    /// Authorization and this complete bundle are separate aggregate components;
    /// the bundle includes the certificate/receipt, duplicate compact authorization, state row and
    /// ordered effect/outbox rows with every locator, marker, ID and count.
    pub fn publication_bundle(
        &mut self,
        expected_version: u64,
        replay_id: Hash32,
        publication: &Publication<'_>,
    ) -> Result<PublicationBundle, Error> {
        // Refuse a wrong kind before snapshot() can refresh and save a checkpoint;
        // a foreign identity still reports Identity first.
        if publication.evaluation().kind() != Kind::Transition {
            if publication.identity() != self.current().identity() {
                return Err(Error::Identity);
            }
            return Err(Error::InvocationKind);
        }
        let snapshot = self.snapshot()?;
        if snapshot.version() != expected_version {
            return Err(Error::Concurrent);
        }
        if publication.identity() != self.current().identity() {
            return Err(Error::Identity);
        }
        if publication.evaluation().raw().state != snapshot.state() {
            return Err(Error::PreState);
        }
        complete_bundle(&self.anchor, replay_id, publication)
    }
    /// Recompute the owning publication before returning the oldest pending obligation.
    pub fn next_pending(&mut self) -> Result<Option<Pending>, Error> {
        self.synchronize()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let segments = check_cached_tip(
            &tx,
            &self.anchor,
            self.data_version,
            self.members.authorities(),
        )?;
        let position: Option<(i64,i64,i64)> = tx.query_row("SELECT sequence,lane,ordinal FROM v2_deliveries WHERE acknowledged=0 ORDER BY sequence,lane,ordinal LIMIT 1", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let result = match position {
            None => None,
            Some((sequence, lane, ordinal)) => {
                let row = load_commit(&tx, sequence)?;
                let state = previous_state(&tx, sequence)?;
                check_commit(&tx, self.members.authorities(), &segments, &row, &state)?;
                Some(load_pending(&tx, sequence, lane, ordinal)?)
            }
        };
        tx.commit()?;
        Ok(result)
    }
    /// Acknowledge only the exact validated content. The host's observation remains trusted.
    pub fn acknowledge(&mut self, delivery_id: Hash32, observed: Hash32) -> Result<(), Error> {
        self.synchronize()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let segments = check_cached_tip(
            &tx,
            &self.anchor,
            self.data_version,
            self.members.authorities(),
        )?;
        let sequence: i64 = tx
            .query_row(
                "SELECT sequence FROM v2_deliveries WHERE delivery_id=?1",
                [delivery_id.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(Error::Delivery)?;
        let row = load_commit(&tx, sequence)?;
        let state = previous_state(&tx, sequence)?;
        check_commit(&tx, self.members.authorities(), &segments, &row, &state)?;
        let expected: Vec<u8> = tx.query_row(
            "SELECT entry_hash FROM v2_deliveries WHERE delivery_id=?1",
            [delivery_id.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        if parse_hash(&expected)? != observed {
            return Err(Error::Delivery);
        }
        tx.execute(
            "UPDATE v2_deliveries SET acknowledged=1 WHERE delivery_id=?1",
            [delivery_id.as_bytes().as_slice()],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// Deliver through the concrete library memory interpreter, then acknowledge.
    /// A crash between those steps is safe to retry with the same content-bound ID.
    pub fn deliver_next_memory(
        &mut self,
        destination: &mut MemoryDestination,
    ) -> Result<bool, Error> {
        let Some((delivery_id, observed)) = self.deliver_next_memory_unacknowledged(destination)?
        else {
            return Ok(false);
        };
        self.acknowledge(delivery_id, observed)?;
        Ok(true)
    }
    /// Deliver through the concrete interpreter while retaining the pending DB entry.
    /// The returned exact identity/content pair can be acknowledged after a restart.
    pub fn deliver_next_memory_unacknowledged(
        &mut self,
        destination: &mut MemoryDestination,
    ) -> Result<Option<(Hash32, Hash32)>, Error> {
        let interpreter = BoundMemoryInterpreter::bind(destination, self.delivery_interpreter)?;
        let Some(pending) = self.next_pending()? else {
            return Ok(None);
        };
        let entry = OutboxEntry::new(
            pending.ordinal,
            pending.channel,
            decode_value(&pending.destination, DecodeLimits::default())
                .map_err(|_| Error::Delivery)?,
            decode_value(&pending.payload, DecodeLimits::default()).map_err(|_| Error::Delivery)?,
        );
        let observed = interpreter
            .destination
            .deliver(pending.delivery_id, pending.entry_hash, &entry)
            .map_err(|_| Error::Delivery)?;
        Ok(Some((pending.delivery_id, observed)))
    }
}

// A binding reaches only the concrete library interpreter. No callback, trait
// implementation supplied by the caller, or caller-reported identity is admitted.
struct BoundMemoryInterpreter<'a> {
    destination: &'a mut MemoryDestination,
}
impl<'a> BoundMemoryInterpreter<'a> {
    fn bind(destination: &'a mut MemoryDestination, expected: Hash32) -> Result<Self, Error> {
        if MemoryDestination::interpreter_identity()? != expected {
            return Err(Error::Interpreter);
        }
        Ok(Self { destination })
    }
}

/// One stored upgrade row, read in ordinal order.
#[derive(Debug)]
struct StoredUpgrade {
    ordinal: i64,
    sequence: i64,
    identity: Vec<u8>,
    publication: Vec<u8>,
    root: Hash32,
    previous_chain: Hash32,
    record: Vec<u8>,
    chain: Hash32,
}
fn load_upgrades(connection: &Connection) -> Result<Vec<StoredUpgrade>, Error> {
    let mut statement = connection.prepare("SELECT ordinal,sequence,identity,publication,root,previous_chain,record,chain FROM v2_upgrades ORDER BY ordinal")?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, Vec<u8>>(4)?,
                r.get::<_, Vec<u8>>(5)?,
                r.get::<_, Vec<u8>>(6)?,
                r.get::<_, Vec<u8>>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut upgrades = Vec::with_capacity(rows.len());
    for (index, row) in rows.into_iter().enumerate() {
        let upgrade = StoredUpgrade {
            ordinal: row.0,
            sequence: row.1,
            identity: row.2,
            publication: row.3,
            root: parse_hash(&row.4)?,
            previous_chain: parse_hash(&row.5)?,
            record: row.6,
            chain: parse_hash(&row.7)?,
        };
        // Ordinals are consecutive from one and heads never move backwards.
        let expected = i64::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or(Error::Range)?;
        if upgrade.ordinal != expected
            || upgrade.sequence < 0
            || upgrades
                .last()
                .is_some_and(|previous: &StoredUpgrade| previous.sequence > upgrade.sequence)
        {
            return Err(Error::History);
        }
        upgrades.push(upgrade);
    }
    Ok(upgrades)
}

/// The store's history segments: the genesis contract, then one more per
/// upgrade, each mapped to the lineage member that published it.
struct Segments {
    genesis_identity: Vec<u8>,
    upgrades: Vec<StoredUpgrade>,
    positions: Vec<usize>,
}
impl Segments {
    fn load(
        connection: &Connection,
        lineage: &[Authority<'_>],
        upgrades: Vec<StoredUpgrade>,
    ) -> Result<Self, Error> {
        let genesis_identity: Vec<u8> = connection.query_row(
            "SELECT identity FROM v2_genesis WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let identities: Vec<&[u8]> = std::iter::once(genesis_identity.as_slice())
            .chain(upgrades.iter().map(|upgrade| upgrade.identity.as_slice()))
            .collect();
        let members: Vec<&[u8]> = lineage.iter().map(|member| member.identity()).collect();
        let positions = upgrade::assign(&members, &identities)?;
        Ok(Self {
            genesis_identity,
            upgrades,
            positions,
        })
    }
    /// Index of the segment the head is in.
    fn current(&self) -> usize {
        self.upgrades.len()
    }
    fn identity(&self, segment: usize) -> &[u8] {
        match segment.checked_sub(1) {
            None => &self.genesis_identity,
            Some(index) => &self.upgrades[index].identity,
        }
    }
    /// The lineage position of a segment's contract.
    fn position(&self, segment: usize) -> usize {
        self.positions[segment]
    }
    fn authority<'x, 'p>(&self, lineage: &'x [Authority<'p>], segment: usize) -> &'x Authority<'p> {
        &lineage[self.position(segment)]
    }
    /// The segment a commit belongs to: after every upgrade recorded at an
    /// earlier head.
    fn segment_of(&self, sequence: i64) -> usize {
        self.upgrades
            .iter()
            .take_while(|upgrade| upgrade.sequence < sequence)
            .count()
    }
    /// The store runs the lineage's last member. Identities, not positions,
    /// decide: an identity binds the complete policy and the evaluator, so a
    /// lineage that repeats one still opens a store that runs it.
    fn require_current(&self, lineage: &[Authority<'_>]) -> Result<(), Error> {
        match lineage.last() {
            Some(last) if last.identity() == self.identity(self.current()) => Ok(()),
            _ => Err(Error::Identity),
        }
    }
    /// How many upgrades a validated anchor already includes: all those at
    /// earlier heads, and those at its own head up to the one whose chain
    /// link it carries.
    fn consumed_at(&self, connection: &Connection, anchor: &Anchor) -> Result<usize, Error> {
        let before = self.segment_of(anchor.sequence);
        if anchor.chain == base_chain(connection, anchor.sequence)? {
            return Ok(before);
        }
        for (offset, upgrade) in self.upgrades[before..].iter().enumerate() {
            if upgrade.sequence != anchor.sequence {
                break;
            }
            if upgrade.chain == anchor.chain {
                return Ok(before + offset + 1);
            }
        }
        Err(Error::History)
    }
    /// The chain tip a commit at `sequence` extends: the last upgrade recorded
    /// at the previous head, else that head's own link.
    fn chain_before(&self, connection: &Connection, sequence: i64) -> Result<Hash32, Error> {
        let previous = sequence.checked_sub(1).ok_or(Error::History)?;
        self.tip_chain(connection, previous)
    }
    /// The chain tip after every event at `sequence`.
    fn tip_chain(&self, connection: &Connection, sequence: i64) -> Result<Hash32, Error> {
        match self
            .upgrades
            .iter()
            .rev()
            .find(|upgrade| upgrade.sequence == sequence)
        {
            Some(upgrade) => Ok(upgrade.chain),
            None => base_chain(connection, sequence),
        }
    }
}
/// The link of the commit at `sequence`, or the genesis link at zero.
fn base_chain(connection: &Connection, sequence: i64) -> Result<Hash32, Error> {
    let chain: Vec<u8> = if sequence == 0 {
        connection.query_row("SELECT chain FROM v2_genesis WHERE singleton=1", [], |r| {
            r.get(0)
        })?
    } else {
        connection.query_row(
            "SELECT chain FROM v2_commits WHERE sequence=?1",
            [sequence],
            |r| r.get(0),
        )?
    };
    parse_hash(&chain)
}

#[derive(Debug)]
struct StoredCommit {
    sequence: i64,
    replay_id: Hash32,
    pre_root: Hash32,
    post_root: Hash32,
    previous_chain: Hash32,
    chain: Hash32,
    command: Vec<u8>,
    context: Vec<u8>,
    post: Vec<u8>,
    publication: Vec<u8>,
    certificate: Vec<u8>,
}
fn load_commit(connection: &Connection, sequence: i64) -> Result<StoredCommit, Error> {
    let row = connection.query_row("SELECT replay_id,pre_root,post_root,previous_chain,chain,command,context,post,publication,certificate FROM v2_commits WHERE sequence=?1", [sequence], |r| Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,Vec<u8>>(2)?,r.get::<_,Vec<u8>>(3)?,r.get::<_,Vec<u8>>(4)?,r.get::<_,Vec<u8>>(5)?,r.get::<_,Vec<u8>>(6)?,r.get::<_,Vec<u8>>(7)?,r.get::<_,Vec<u8>>(8)?,r.get::<_,Vec<u8>>(9)?)))?;
    Ok(StoredCommit {
        sequence,
        replay_id: parse_hash(&row.0)?,
        pre_root: parse_hash(&row.1)?,
        post_root: parse_hash(&row.2)?,
        previous_chain: parse_hash(&row.3)?,
        chain: parse_hash(&row.4)?,
        command: row.5,
        context: row.6,
        post: row.7,
        publication: row.8,
        certificate: row.9,
    })
}
/// Replay one commit under the Authority of its own segment.
fn check_commit(
    connection: &Connection,
    lineage: &[Authority<'_>],
    segments: &Segments,
    row: &StoredCommit,
    pre: &[u8],
) -> Result<(), Error> {
    if row.sequence <= 0
        || hash(zeno_fcis_codec::domains::V2_STATE, pre)? != row.pre_root
        || hash(zeno_fcis_codec::domains::V2_STATE, &row.post)? != row.post_root
    {
        return Err(Error::History);
    }
    let raw = Raw {
        state: pre,
        command: &row.command,
        context: &row.context,
    };
    let segment = segments.segment_of(row.sequence);
    let authority = segments.authority(lineage, segment);
    let binding = segments.identity(segment);
    if authority.identity() != binding {
        return Err(Error::Identity);
    }
    let full = expand_publication(binding, &row.publication)?;
    let PublicationOutcome::Commit(p) = authority.replay_publication(raw, &full) else {
        return Err(Error::History);
    };
    if p.poststate() != row.post {
        return Err(Error::History);
    }
    let certificate = certificate_bytes(
        row.sequence,
        row.replay_id,
        row.pre_root,
        row.post_root,
        row.previous_chain,
        &full,
    )?;
    if row.certificate != certificate
        || hash(zeno_fcis_codec::domains::V2_CHAIN, &certificate)? != row.chain
        || row.previous_chain != segments.chain_before(connection, row.sequence)?
    {
        return Err(Error::History);
    }
    check_deliveries(connection, row.sequence, &p, &certificate)
}
/// Recompute one upgrade record at the head it was recorded at, re-checking
/// the admission its kind claims: a genesis admission replays the stored
/// genesis publication over the state, and a program successor re-derives
/// the policy comparison from the two versions' catalogs.
fn check_upgrade(
    connection: &Connection,
    lineage: &[Authority<'_>],
    catalogs: &[&BoundCatalog<'_>],
    segments: &Segments,
    index: usize,
    anchor: &Anchor,
) -> Result<(), Error> {
    let upgrade = &segments.upgrades[index];
    let ordinal = u64::try_from(index)
        .ok()
        .and_then(|index| index.checked_add(1))
        .ok_or(Error::Range)?;
    if u64::try_from(upgrade.ordinal) != Ok(ordinal)
        || upgrade.sequence != anchor.sequence
        || upgrade.root != anchor.root
        || upgrade.previous_chain != anchor.chain
    {
        return Err(Error::History);
    }
    let from = segments.identity(index);
    let (old, new) = (segments.position(index), segments.position(index + 1));
    let authority = &lineage[new];
    if authority.identity() != upgrade.identity || from == upgrade.identity {
        return Err(Error::Identity);
    }
    let sequence = u64::try_from(anchor.sequence).map_err(|_| Error::Range)?;
    let kind = upgrade::Kind::of_record(&upgrade.record).ok_or(Error::History)?;
    let admission = match kind {
        upgrade::Kind::GenesisAdmission => {
            let full = expand_publication(&upgrade.identity, &upgrade.publication)?;
            let PublicationOutcome::Commit(p) =
                authority.replay_genesis_publication(&anchor.state, &full)
            else {
                return Err(Error::History);
            };
            if p.poststate() != anchor.state {
                return Err(Error::History);
            }
            full
        }
        upgrade::Kind::ProgramSuccessor => {
            // A handle opened with one Authority holds no catalog, and its
            // store cannot hold an upgrade.
            let (Some(from_catalog), Some(to_catalog)) = (catalogs.get(old), catalogs.get(new))
            else {
                return Err(Error::Identity);
            };
            let receipts = upgrade::evidence_receipts(&upgrade.publication)?;
            // The premises are re-derived from the two versions and must be
            // the ones the record states.
            match upgrade::Successor::establish(from_catalog, to_catalog, &receipts)? {
                Some(successor) if receipts.len() == new - old => {
                    upgrade::successor_admission(successor.premises(), &upgrade.publication)
                }
                _ => return Err(Error::History),
            }
        }
    };
    let record = upgrade::record_bytes(
        kind,
        ordinal,
        sequence,
        from,
        &upgrade.identity,
        &admission,
        anchor.root,
        anchor.chain,
    )?;
    if upgrade.record != record
        || hash(zeno_fcis_codec::domains::V2_CHAIN, &record)? != upgrade.chain
        || base_chain(connection, anchor.sequence)? == upgrade.chain
    {
        return Err(Error::History);
    }
    Ok(())
}
/// Check every upgrade recorded at the anchor's head and advance its chain.
fn apply_upgrades(
    connection: &Connection,
    lineage: &[Authority<'_>],
    catalogs: &[&BoundCatalog<'_>],
    segments: &Segments,
    anchor: &mut Anchor,
    mut consumed: usize,
) -> Result<usize, Error> {
    while let Some(upgrade) = segments.upgrades.get(consumed) {
        if upgrade.sequence > anchor.sequence {
            break;
        }
        check_upgrade(connection, lineage, catalogs, segments, consumed, anchor)?;
        anchor.chain = upgrade.chain;
        consumed += 1;
    }
    Ok(consumed)
}
/// Replay every commit after the anchor under its segment's Authority, and
/// every upgrade at the heads passed, up to the stored head.
fn audit_tail(
    connection: &Connection,
    lineage: &[Authority<'_>],
    catalogs: &[&BoundCatalog<'_>],
    segments: &Segments,
    mut anchor: Anchor,
    mut consumed: usize,
) -> Result<Anchor, Error> {
    let mut statement = connection
        .prepare("SELECT sequence FROM v2_commits WHERE sequence>?1 ORDER BY sequence")?;
    let positions = statement
        .query_map([anchor.sequence], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for sequence in positions {
        consumed = apply_upgrades(
            connection,
            lineage,
            catalogs,
            segments,
            &mut anchor,
            consumed,
        )?;
        if sequence != anchor.sequence.checked_add(1).ok_or(Error::Range)? {
            return Err(Error::History);
        }
        let row = load_commit(connection, sequence)?;
        if row.previous_chain != anchor.chain
            || row.pre_root != anchor.root
            || segments.segment_of(sequence) != consumed
        {
            return Err(Error::History);
        }
        check_commit(connection, lineage, segments, &row, &anchor.state)?;
        anchor = Anchor {
            sequence,
            state: row.post,
            root: row.post_root,
            chain: row.chain,
        };
    }
    consumed = apply_upgrades(
        connection,
        lineage,
        catalogs,
        segments,
        &mut anchor,
        consumed,
    )?;
    // An upgrade recorded beyond the head, or one skipped, is not history.
    if consumed != segments.upgrades.len() || read_anchor(connection)? != anchor {
        return Err(Error::History);
    }
    let orphan: i64 = connection.query_row("SELECT count(*) FROM v2_deliveries d LEFT JOIN v2_commits c ON d.sequence=c.sequence WHERE c.sequence IS NULL", [], |r| r.get(0))?;
    if orphan != 0 {
        return Err(Error::History);
    }
    Ok(anchor)
}
fn genesis_anchor(
    connection: &Connection,
    authority: &Authority<'_>,
) -> Result<(Hash32, Anchor), Error> {
    let (identity, initial, publication, interpreter, chain) = connection.query_row(
        "SELECT identity,initial,publication,delivery_interpreter,chain FROM v2_genesis WHERE singleton=1",
        [],
        |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, Vec<u8>>(4)?,
            ))
        },
    )?;
    if identity != authority.identity() {
        return Err(Error::Identity);
    }
    if parse_hash(&interpreter)? != MemoryDestination::interpreter_identity()? {
        return Err(Error::Interpreter);
    }
    let full = expand_publication(&identity, &publication)?;
    if !matches!(
        authority.replay_genesis_publication(&initial, &full),
        PublicationOutcome::Commit(_)
    ) {
        return Err(Error::History);
    }
    let root = hash(zeno_fcis_codec::domains::V2_STATE, &initial)?;
    let expected = framed_hash(
        zeno_fcis_codec::domains::V2_GENESIS,
        &[&identity, &full, root.as_bytes(), &interpreter],
    )?;
    if parse_hash(&chain)? != expected {
        return Err(Error::History);
    }
    Ok((
        expected,
        Anchor {
            sequence: 0,
            state: initial,
            root,
            chain: expected,
        },
    ))
}
fn read_anchor(connection: &Connection) -> Result<Anchor, Error> {
    let (sequence, state, root, chain) = connection.query_row(
        "SELECT sequence,state,root,chain FROM v2_state WHERE singleton=1",
        [],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
            ))
        },
    )?;
    if sequence < 0 || hash(zeno_fcis_codec::domains::V2_STATE, &state)? != parse_hash(&root)? {
        return Err(Error::History);
    }
    Ok(Anchor {
        sequence,
        state,
        root: parse_hash(&root)?,
        chain: parse_hash(&chain)?,
    })
}
fn previous_state(connection: &Connection, sequence: i64) -> Result<Vec<u8>, Error> {
    if sequence <= 0 {
        return Err(Error::History);
    }
    if sequence == 1 {
        Ok(connection.query_row(
            "SELECT initial FROM v2_genesis WHERE singleton=1",
            [],
            |r| r.get(0),
        )?)
    } else {
        Ok(connection.query_row(
            "SELECT post FROM v2_commits WHERE sequence=?1",
            [sequence - 1],
            |r| r.get(0),
        )?)
    }
}
fn check_tip(connection: &Connection, segments: &Segments, anchor: &Anchor) -> Result<(), Error> {
    let (count, max): (i64, i64) = connection.query_row(
        "SELECT count(*),coalesce(max(sequence),0) FROM v2_commits",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if count != anchor.sequence || max != anchor.sequence {
        return Err(Error::History);
    }
    if anchor.sequence > 0 {
        let row = load_commit(connection, anchor.sequence)?;
        if row.post != anchor.state || row.post_root != anchor.root {
            return Err(Error::History);
        }
    }
    if segments.tip_chain(connection, anchor.sequence)? != anchor.chain
        || segments
            .upgrades
            .last()
            .is_some_and(|upgrade| upgrade.sequence > anchor.sequence)
    {
        return Err(Error::History);
    }
    Ok(())
}
/// Check a trusted checkpoint against the store and return how many upgrades
/// its anchor already includes. Its chain link must be one the head holds;
/// the stored marker is keyed by head alone, so a later save at the same head
/// after an upgrade replaces it, which is `Error::Checkpoint`, not tampering.
fn check_checkpoint(
    connection: &Connection,
    segments: &Segments,
    c: &Checkpoint,
) -> Result<usize, Error> {
    if c.anchor.sequence < 0
        || hash(zeno_fcis_codec::domains::V2_STATE, &c.anchor.state)? != c.anchor.root
    {
        return Err(Error::History);
    }
    if c.anchor.sequence > 0 {
        let row = load_commit(connection, c.anchor.sequence)?;
        if row.post_root != c.anchor.root || row.post != c.anchor.state {
            return Err(Error::History);
        }
    }
    let consumed = segments.consumed_at(connection, &c.anchor)?;
    let (root, chain): (Vec<u8>, Vec<u8>) = connection.query_row(
        "SELECT root,chain FROM v2_checkpoints WHERE sequence=?1",
        [c.anchor.sequence],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if parse_hash(&root)? != c.anchor.root {
        return Err(Error::History);
    }
    if parse_hash(&chain)? != c.anchor.chain {
        return Err(Error::Checkpoint);
    }
    Ok(consumed)
}
/// Opens an existing database file. A missing one is refused and not
/// created, so a refused operation on a mistyped path leaves nothing behind.
fn open_existing(path: impl AsRef<Path>) -> Result<Connection, Error> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?)
}
fn configure(connection: &Connection) -> Result<(), Error> {
    // These connection settings must be established outside a transaction.
    connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")?;
    Ok(())
}
fn check_schema(connection: &Connection) -> Result<(), Error> {
    check_exact_schema(connection, SCHEMA_VERSION, &schema_v10())
}
/// A v9 store, exactly as the previous shell created it.
fn check_schema_v9(connection: &Connection) -> Result<(), Error> {
    check_exact_schema(connection, 9, &schema_v9())
}
fn check_exact_schema(connection: &Connection, expected: i64, schema: &str) -> Result<(), Error> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version != expected {
        return Err(Error::Schema(version));
    }
    let reference = Connection::open_in_memory()?;
    reference.execute_batch(schema)?;
    if schema_rows(connection)? != schema_rows(&reference)? {
        return Err(Error::Schema(version));
    }
    Ok(())
}
/// Called only after BEGIN IMMEDIATE. Returns the store's segments, for the
/// operation that follows. If another connection wrote between synchronize()
/// and this transaction, refuse rather than bless stale data.
fn check_cached_tip(
    connection: &Connection,
    anchor: &Anchor,
    version: i64,
    lineage: &[Authority<'_>],
) -> Result<Segments, Error> {
    check_schema(connection)?;
    let segments = Segments::load(connection, lineage, load_upgrades(connection)?)?;
    segments.require_current(lineage)?;
    if data_version(connection)? != version || read_anchor(connection)? != *anchor {
        return Err(Error::Concurrent);
    }
    check_tip(connection, &segments, anchor)?;
    Ok(segments)
}
fn save_checkpoint(connection: &Connection, anchor: &Anchor) -> Result<(), Error> {
    connection.execute(
        "INSERT OR REPLACE INTO v2_checkpoints VALUES(?1,?2,?3)",
        params![
            anchor.sequence,
            anchor.root.as_bytes().as_slice(),
            anchor.chain.as_bytes().as_slice()
        ],
    )?;
    Ok(())
}
/// One `sqlite_master` row: type, name, table name and defining SQL.
type SchemaRow = (String, String, String, Option<String>);

/// The complete schema, with no name filter: the reference is built from the
/// same DDL, so SQLite's own deterministic objects (autoindexes) compare
/// equal, and any other object, whatever its name, is a difference.
fn schema_rows(connection: &Connection) -> Result<Vec<SchemaRow>, Error> {
    let mut statement = connection
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name")?;
    Ok(statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?)
}
fn certificate_bytes(
    sequence: i64,
    replay_id: Hash32,
    pre: Hash32,
    post: Hash32,
    previous: Hash32,
    publication: &[u8],
) -> Result<Vec<u8>, Error> {
    if sequence <= 0 {
        return Err(Error::Range);
    }
    let mut bytes = b"ZFCISV2-CERT\0".to_vec();
    bytes.extend_from_slice(&(sequence as u64).to_be_bytes());
    for h in [
        replay_id,
        pre,
        post,
        previous,
        hash(zeno_fcis_codec::domains::V2_PUBLICATION, publication)?,
    ] {
        bytes.extend_from_slice(h.as_bytes());
    }
    Ok(bytes)
}
fn original_entry(d: &WireDelivery) -> Result<Vec<u8>, Error> {
    let entry = OutboxEntry::new(
        d.ordinal(),
        d.channel(),
        decode_value(d.destination(), DecodeLimits::default()).map_err(|_| Error::Delivery)?,
        decode_value(d.payload(), DecodeLimits::default()).map_err(|_| Error::Delivery)?,
    );
    Ok(entry.canonical_bytes()?)
}
fn delivery_commitments(
    publication: &Publication<'_>,
    lane: u8,
    d: &WireDelivery,
    certificate: &[u8],
) -> Result<(Hash32, Hash32), Error> {
    let entry = original_entry(d)?;
    let candidate = framed_hash(
        zeno_fcis_codec::domains::V2_PUBLICATION,
        &[publication.subject(), certificate],
    )?;
    let mut preimage = candidate.as_bytes().to_vec();
    preimage.extend_from_slice(&entry);
    let domain = if lane == 1 {
        zeno_fcis_codec::domains::DELIVERY
    } else {
        zeno_fcis_codec::domains::V2_EFFECT_DELIVERY
    };
    Ok((
        hash(domain, &preimage)?,
        hash(zeno_fcis_codec::domains::OUTBOX_ENTRY, &entry)?,
    ))
}
fn insert_deliveries(
    connection: &Connection,
    sequence: i64,
    p: &Publication<'_>,
    certificate: &[u8],
) -> Result<(), Error> {
    for (lane, ds) in [(0_u8, p.effects()), (1_u8, p.outbox())] {
        for d in ds {
            let (id, entry) = delivery_commitments(p, lane, d, certificate)?;
            connection.execute(
                "INSERT INTO v2_deliveries VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,0)",
                params![
                    sequence,
                    lane,
                    d.ordinal(),
                    d.channel(),
                    d.destination_root(),
                    d.payload_root(),
                    d.destination(),
                    d.payload(),
                    d.idempotency(),
                    id.as_bytes().as_slice(),
                    entry.as_bytes().as_slice()
                ],
            )?;
        }
    }
    Ok(())
}
fn load_pending(
    connection: &Connection,
    sequence: i64,
    lane: i64,
    ordinal: i64,
) -> Result<Pending, Error> {
    let row = connection.query_row("SELECT channel,destination_root,payload_root,destination,payload,marker,delivery_id,entry_hash,acknowledged FROM v2_deliveries WHERE sequence=?1 AND lane=?2 AND ordinal=?3", params![sequence,lane,ordinal], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,Vec<u8>>(3)?,r.get::<_,Vec<u8>>(4)?,r.get::<_,Vec<u8>>(5)?,r.get::<_,Vec<u8>>(6)?,r.get::<_,Vec<u8>>(7)?,r.get::<_,i64>(8)?)))?;
    if !(0..=1).contains(&row.8) || !(0..=1).contains(&lane) {
        return Err(Error::Delivery);
    }
    Ok(Pending {
        sequence: u64::try_from(sequence).map_err(|_| Error::Range)?,
        lane: lane as u8,
        ordinal: u32::try_from(ordinal).map_err(|_| Error::Range)?,
        channel: u32::try_from(row.0).map_err(|_| Error::Range)?,
        destination_root: u32::try_from(row.1).map_err(|_| Error::Range)?,
        payload_root: u32::try_from(row.2).map_err(|_| Error::Range)?,
        destination: row.3,
        payload: row.4,
        marker: row.5,
        delivery_id: parse_hash(&row.6)?,
        entry_hash: parse_hash(&row.7)?,
    })
}
fn check_deliveries(
    connection: &Connection,
    sequence: i64,
    p: &Publication<'_>,
    certificate: &[u8],
) -> Result<(), Error> {
    let count: i64 = connection.query_row(
        "SELECT count(*) FROM v2_deliveries WHERE sequence=?1",
        [sequence],
        |r| r.get(0),
    )?;
    if count as u128 != p.effects().len() as u128 + p.outbox().len() as u128 {
        return Err(Error::Delivery);
    }
    for (lane, ds) in [(0_u8, p.effects()), (1_u8, p.outbox())] {
        for d in ds {
            let actual = load_pending(connection, sequence, lane as i64, d.ordinal() as i64)?;
            let (id, entry) = delivery_commitments(p, lane, d, certificate)?;
            if actual.channel != d.channel()
                || actual.destination_root != d.destination_root()
                || actual.payload_root != d.payload_root()
                || actual.destination != d.destination()
                || actual.payload != d.payload()
                || actual.marker != d.idempotency()
                || actual.delivery_id != id
                || actual.entry_hash != entry
            {
                return Err(Error::Delivery);
            }
        }
    }
    Ok(())
}

/// The unchanged aggregate definition, including required duplicate artifacts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationSizes {
    /// Canonical bare original command Value.
    pub command: usize,
    /// Canonical bare original successor Value.
    pub state: usize,
    /// Compact checked authorization.
    pub authorization: usize,
    /// Complete independently encoded durable bundle including duplicate compact authorization.
    pub bundle: usize,
    /// Actual certificate/receipt bytes included in bundle.
    pub receipt: usize,
    /// Complete ordered durable outbox rows included in bundle.
    pub outbox: usize,
}
impl PublicationSizes {
    /// Sum only the original four components. Receipt and outbox are subcomponents.
    pub fn total(self) -> Result<usize, Error> {
        self.command
            .checked_add(self.state)
            .and_then(|n| n.checked_add(self.authorization))
            .and_then(|n| n.checked_add(self.bundle))
            .ok_or(Error::Range)
    }
}
/// Complete durable bytes and included receipt/outbox components for inspection.
#[derive(Debug)]
pub struct PublicationBundle {
    bytes: Vec<u8>,
    receipt: Vec<u8>,
    outbox: Vec<u8>,
    sizes: PublicationSizes,
}
impl PublicationBundle {
    /// Canonical full bundle bytes; this is descriptive data, not a capability.
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Exact persisted certificate, in its actual original ordering.
    pub fn receipt(&self) -> &[u8] {
        &self.receipt
    }
    /// Every durable outbox row and count in original order.
    pub fn outbox(&self) -> &[u8] {
        &self.outbox
    }
    /// Original aggregate component lengths, including all framed copies.
    pub fn sizes(&self) -> PublicationSizes {
        self.sizes
    }
}
fn append_parts(bytes: &mut Vec<u8>, parts: &[&[u8]]) -> Result<(), Error> {
    for part in parts {
        bytes.extend_from_slice(
            &u64::try_from(part.len())
                .map_err(|_| Error::Range)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(part);
    }
    Ok(())
}
fn lane_bytes(
    sequence: i64,
    publication: &Publication<'_>,
    lane: u8,
    deliveries: &[WireDelivery],
    certificate: &[u8],
) -> Result<Vec<u8>, Error> {
    let mut bytes = u64::try_from(deliveries.len())
        .map_err(|_| Error::Range)?
        .to_be_bytes()
        .to_vec();
    for d in deliveries {
        let (id, entry_hash) = delivery_commitments(publication, lane, d, certificate)?;
        let mut row = Vec::new();
        append_parts(
            &mut row,
            &[
                &sequence.to_be_bytes(),
                &[lane],
                &d.ordinal().to_be_bytes(),
                &d.channel().to_be_bytes(),
                &d.destination_root().to_be_bytes(),
                &d.payload_root().to_be_bytes(),
                d.destination(),
                d.payload(),
                d.idempotency(),
                id.as_bytes(),
                entry_hash.as_bytes(),
                &[0],
            ],
        )?;
        append_parts(&mut bytes, &[&row])?;
    }
    Ok(bytes)
}
fn complete_bundle(
    anchor: &Anchor,
    replay_id: Hash32,
    publication: &Publication<'_>,
) -> Result<PublicationBundle, Error> {
    let sequence = anchor.sequence.checked_add(1).ok_or(Error::Range)?;
    let post_root = hash(zeno_fcis_codec::domains::V2_STATE, publication.poststate())?;
    let receipt = certificate_bytes(
        sequence,
        replay_id,
        anchor.root,
        post_root,
        anchor.chain,
        publication.subject(),
    )?;
    let chain = hash(zeno_fcis_codec::domains::V2_CHAIN, &receipt)?;
    let raw = publication.evaluation().raw();
    let compact = compact_publication(publication.identity(), publication.subject())?;
    let mut commit = Vec::new();
    append_parts(
        &mut commit,
        &[
            &sequence.to_be_bytes(),
            replay_id.as_bytes(),
            anchor.root.as_bytes(),
            post_root.as_bytes(),
            anchor.chain.as_bytes(),
            chain.as_bytes(),
            raw.command,
            raw.context,
            publication.poststate(),
            &compact,
            &receipt,
        ],
    )?;
    let mut state = Vec::new();
    append_parts(
        &mut state,
        &[
            &1_i64.to_be_bytes(),
            &sequence.to_be_bytes(),
            publication.poststate(),
            post_root.as_bytes(),
            chain.as_bytes(),
        ],
    )?;
    let effects = lane_bytes(sequence, publication, 0, publication.effects(), &receipt)?;
    let outbox = lane_bytes(sequence, publication, 1, publication.outbox(), &receipt)?;
    // The byte after the NUL is the schema version the rows belong to.
    let mut bytes = b"ZFCIS-SQL-PUBLICATION-BUNDLE\0\x0a".to_vec();
    append_parts(&mut bytes, &[&commit, &state, &effects, &outbox])?;
    let command = zeno_fcis_codec::decode_envelope(raw.command, DecodeLimits::default())
        .map_err(|_| Error::History)?
        .into_value()
        .canonical_bytes()?
        .len();
    let post = zeno_fcis_codec::decode_envelope(publication.poststate(), DecodeLimits::default())
        .map_err(|_| Error::History)?
        .into_value()
        .canonical_bytes()?
        .len();
    let sizes = PublicationSizes {
        command,
        state: post,
        authorization: compact.len(),
        bundle: bytes.len(),
        receipt: receipt.len(),
        outbox: outbox.len(),
    };
    sizes.total()?;
    Ok(PublicationBundle {
        bytes,
        receipt,
        outbox,
        sizes,
    })
}

fn data_version(connection: &Connection) -> Result<i64, Error> {
    Ok(connection.query_row("PRAGMA data_version", [], |r| r.get(0))?)
}
fn parse_hash(bytes: &[u8]) -> Result<Hash32, Error> {
    Ok(Hash32::new(bytes.try_into().map_err(|_| Error::History)?))
}
fn provider() -> Result<(), Error> {
    verify_approved_provider::<RustCryptoSha256>().map_err(|_| Error::Provider)?;
    Ok(())
}
fn hash(domain: Domain<'static>, bytes: &[u8]) -> Result<Hash32, Error> {
    Ok(commitment::<RustCryptoSha256>(domain, bytes)?)
}
fn framed_hash(domain: Domain<'static>, parts: &[&[u8]]) -> Result<Hash32, Error> {
    let mut bytes = Vec::new();
    for part in parts {
        bytes.extend_from_slice(
            &u64::try_from(part.len())
                .map_err(|_| Error::Range)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(part);
    }
    hash(domain, &bytes)
}
fn inject(selected: Option<CrashPoint>, point: CrashPoint) -> Result<(), Error> {
    if selected == Some(point) {
        Err(Error::InjectedCrash(point))
    } else {
        Ok(())
    }
}

/// Failure of the trusted SQLite refinement; none grants a publication capability.
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// SQLite refused an operation.
    Sqlite(rusqlite::Error),
    /// Commitment preimage encoding failed.
    Encoding(EncodeError),
    /// The pinned sealed SHA-256 provider failed its known-answer check.
    Provider,
    /// A different schema requires an explicit reviewed migration, never implicit replay.
    Schema(i64),
    /// The private capability or checkpoint belongs to another bound Authority, or a
    /// history segment was published under no lineage member in order, or the
    /// store does not run the contract the operation needs.
    Identity,
    /// The library refused to bind a lineage catalog into an Authority.
    Authority,
    /// A lineage needs at least one version and exactly one adoption receipt
    /// between each version and the next.
    Lineage,
    /// The store no longer holds this checkpoint: its head was saved again
    /// with a later chain link, after an upgrade recorded at the same head.
    /// Open from a newer checkpoint or with a full audit.
    Checkpoint,
    /// The genuine publication has the other verified invocation kind: genesis only
    /// initializes a store, and only a transition commits or forms a publication bundle.
    InvocationKind,
    /// Stored history fails exact replay, root, certificate, upgrade record or chain validation.
    History,
    /// Integer or canonical entry length cannot be represented.
    Range,
    /// The genuine publication was produced for a different current state.
    PreState,
    /// Reusing the replay key would bind different actual input or publication bytes.
    Replay,
    /// Another connection changed the state before the atomic update.
    Concurrent,
    /// A delivery obligation or destination acknowledgement differs.
    Delivery,
    /// The stored or bound delivery interpreter differs from the actual library source.
    Interpreter,
    /// Complete command/state/authorization/bundle bytes exceed the requested limit.
    Capacity {
        /// Actual complete four-component aggregate.
        required: usize,
        /// Requested limit, prior to any publication.
        declared: usize,
    },
    /// The pure upgrade decision refused; see [`upgrade::Refusal`].
    Upgrade(upgrade::Refusal),
    /// An explicitly requested interruption was injected.
    InjectedCrash(CrashPoint),
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}
impl From<EncodeError> for Error {
    fn from(e: EncodeError) -> Self {
        Self::Encoding(e)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "V2 SQLite refinement refused: {self:?}")
    }
}
impl std::error::Error for Error {}

#[cfg(test)]
mod interpreter_tests {
    use super::*;

    #[test]
    fn binding_refuses_a_different_interpreter_before_reaching_the_destination() {
        let mut destination = MemoryDestination::default();
        let actual = MemoryDestination::interpreter_identity()
            .unwrap_or_else(|error| panic!("interpreter identity: {error}"));
        let mut different = *actual.as_bytes();
        different[0] ^= 1;
        assert!(matches!(
            BoundMemoryInterpreter::bind(&mut destination, Hash32::new(different)),
            Err(Error::Interpreter)
        ));
        assert_eq!(destination.delivered_count(), 0);
        assert!(BoundMemoryInterpreter::bind(&mut destination, actual).is_ok());
    }

    #[test]
    fn the_v10_schema_is_v9_with_the_upgrade_table_and_migration_adds_exactly_that() {
        let v9 = schema_v9();
        let history = v9
            .strip_suffix("PRAGMA user_version = 9;\n")
            .unwrap_or_else(|| panic!("v9 ends with its version"));
        assert_eq!(schema_v10(), format!("{history}{}", migration_v9_to_v10()));
        assert!(schema_v10().ends_with("PRAGMA user_version = 10;\n"));
        let fresh = Connection::open_in_memory().unwrap_or_else(|error| panic!("{error}"));
        fresh
            .execute_batch(&schema_v10())
            .unwrap_or_else(|error| panic!("{error}"));
        let migrated = Connection::open_in_memory().unwrap_or_else(|error| panic!("{error}"));
        migrated
            .execute_batch(&schema_v9())
            .unwrap_or_else(|error| panic!("{error}"));
        migrated
            .execute_batch(&migration_v9_to_v10())
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            schema_rows(&fresh).unwrap_or_else(|error| panic!("{error}")),
            schema_rows(&migrated).unwrap_or_else(|error| panic!("{error}"))
        );
        assert!(check_schema(&migrated).is_ok());
        assert!(matches!(check_schema_v9(&migrated), Err(Error::Schema(10))));
    }
}
