//! SQLite schema v10 stores compact genuine publications under the admitted
//! identities of a contract lineage. Full reconstructed publications bind replay,
//! certificates and delivery IDs. The functional core owns decisions; SQLite,
//! SHA-256 and destination acknowledgements remain trusted shell assumptions.
//! Identity copying is outside the logical byte cap. Private checkpoints
//! require trusted provenance and do not authenticate hostile stores.
//!
//! A store starts under one Authority. A checked upgrade appends a chained
//! record that moves the head to a later contract of the application's
//! lineage with the same state schema: a program successor at any state; a
//! reviewed behaviour change when the new contract's state laws and declared
//! claims hold on the current state; and any other contract only when its
//! genesis laws admit the current state (see
//! [`upgrade`](crate::v2::upgrade)). A declared data migration, admitted by
//! forward simulation, or a declared rename, admitted when only names differ,
//! also moves the head's state to the new schema (see
//! [`migration`](crate::v2::migration)). Each history segment replays under the
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
//!
//! Deliveries follow a typed lifecycle, `Pending` → `Delivered` →
//! acknowledged. [`next_pending`](crate::v2::V2SqliteShell::next_pending)
//! issues the oldest pending delivery as a [`Pending`](crate::v2::Pending)
//! token that holds the handle's exclusive borrow; delivering consumes it into
//! a [`Delivered`](crate::v2::Delivered) token, and acknowledging consumes
//! that. `Pending` states what the types enforce and what stays a run-time
//! check.
//!
//! An external relay reaches the same lifecycle across processes: the
//! [`relay`](crate::v2::relay) module exports the pending deliveries as an
//! ordered stream of lines and acknowledges one delivery by its ID and the
//! SHA-256 of its payload, refusing an unknown, mismatched or already
//! acknowledged acknowledgment with a named error and no write.

use crate::CrashPoint;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use std::{cell::Cell, fmt, path::Path};
use zeno_fcis_codec::{
    CanonicalEncode, CommitmentHasher, DecodeLimits, Domain, EncodeError, Hash32, commitment,
    decode_value,
};
use zeno_fcis_crypto::{RustCryptoSha256, verify_approved_provider};
use zeno_fcis_plan::OutboxEntry;
use zeno_fcis_shell::{CommitStatus, MemoryDestination};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, Publication, PublicationOutcome, WireDelivery},
    v2_composition::{Kind, Raw},
};

/// The behaviour-change admission: the new contract's state laws and
/// declared claims on the store's state.
pub mod behaviour;
mod compact;
mod delivery;
pub mod equivalence;
mod lineage;
/// Data migrations admitted by forward simulation, and the rename tier.
pub mod migration;
pub mod relay;
/// The pure upgrade decision and the lineage assignment.
pub mod upgrade;
pub use compact::{compact_publication, expand_publication};
pub use delivery::{Delivered, Pending};
pub use lineage::{Lineage, Opened, Step, Store, Superseded, V9Store};

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
/// at the head for a genesis admission, the adoption receipt digests for a
/// program successor, or the owner review texts, framed, for a behaviour
/// change. The record's magic names its kind.
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

/// Original inputs of one commit, retained only after its publication and
/// certificate replay successfully. This owned reporting data grants no authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditedCommit {
    /// Commit position, from one.
    pub sequence: u64,
    /// Position of the publishing contract in the supplied lineage, from one.
    pub contract_version: usize,
    /// Exact state after all upgrades at the preceding head.
    pub prestate: Vec<u8>,
    /// Original command envelope.
    pub command: Vec<u8>,
    /// Original context envelope.
    pub context: Vec<u8>,
    /// Original successor state envelope.
    pub poststate: Vec<u8>,
}

/// A full read-only audit with original genesis and committing inputs.
/// All values come from one read transaction; they are reports, not capabilities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditedHistory {
    /// The contract at the checked head, from one.
    pub contract_version: usize,
    /// The coherent head, including upgrades after the final commit.
    pub snapshot: Snapshot,
    /// The contract that published the store's actual genesis, from one.
    pub genesis_contract_version: usize,
    /// Original genesis envelope, before any migration or rename.
    pub genesis: Vec<u8>,
    /// Every checked committing invocation, oldest first.
    pub commits: Vec<AuditedCommit>,
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
    behaviour: Option<upgrade::Behaviour>,
    migration: Option<upgrade::Migrated>,
    record: Vec<u8>,
}
impl UpgradeReceipt {
    /// How the new contract admitted the store's state.
    pub fn kind(&self) -> upgrade::Kind {
        self.kind
    }
    /// For a program successor, the evidence that the shell established all
    /// five premises, including the number of input tuples on which it
    /// compared the two decision programs; `None` for a genesis admission.
    pub fn premises(&self) -> Option<upgrade::Premises> {
        self.premises
    }
    /// Position among the store's upgrades, from 1.
    pub fn ordinal(&self) -> u64 {
        self.ordinal
    }
    /// For a program successor, the SHA-256 of each adoption receipt between
    /// the two versions, oldest first, as the upgrading lineage declares
    /// them; empty for a genesis admission. They are provenance: the shell
    /// replays no receipt, it compares the two programs itself, and an audit
    /// requires these digests to be the auditing lineage's.
    pub fn receipts(&self) -> &[Hash32] {
        &self.receipts
    }
    /// For a behaviour change, the state laws and claims that held on the
    /// state, the laws not evaluated and the owner review digests; `None`
    /// for any other kind.
    pub fn behaviour(&self) -> Option<&upgrade::Behaviour> {
        self.behaviour.as_ref()
    }
    /// For a migration, the migration digest, what the forward simulation
    /// compared and the migrated state's root; `None` for any other kind.
    pub fn migration(&self) -> Option<&upgrade::Migrated> {
        self.migration.as_ref()
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

/// One stored delivery: its place in commit order, rather than hash order,
/// its exact content and its certificate-bound ID. Plain data that grants
/// nothing: only a [`Pending`] token delivers, and only a [`Delivered`] token
/// acknowledges.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delivery {
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
impl Delivery {
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
    /// What a program-successor record is checked against; nothing for a
    /// handle opened with one Authority, whose store has no upgrade.
    /// `missing` receives a pair of versions whose comparison the check
    /// needed and the lineage had not yet established.
    fn declared<'m>(self, missing: &'m Cell<Option<(usize, usize)>>) -> Declared<'m, 'a, 'p> {
        Declared {
            lineage: self.lineage(),
            len: self.authorities().len(),
            missing,
        }
    }
    /// The lineage a handle was opened through, if any.
    fn lineage(self) -> Option<&'a Lineage<'a, 'p>> {
        match self {
            Self::One(_) => None,
            Self::Lineage { lineage, .. } => Some(lineage),
        }
    }
    fn last(self) -> &'a Authority<'p> {
        self.authorities()
            .last()
            .unwrap_or_else(|| unreachable!("a handle is bound to at least one Authority"))
    }
}

/// What the lineage a handle is bound to declares for its first `len`
/// versions: each version's checked catalog, the adoption receipt digests
/// between each version and the next, and the premises it established for
/// pairs of versions. An audit takes a program succession from these alone
/// and never takes evidence from the store as evidence about itself. Inside a
/// transaction it only looks an established pair up; a pair not yet
/// established goes to `missing`, and the check ends with `Error::Unsettled`.
#[derive(Clone, Copy)]
struct Declared<'m, 'a, 'p> {
    lineage: Option<&'a Lineage<'a, 'p>>,
    len: usize,
    missing: &'m Cell<Option<(usize, usize)>>,
}

/// Establishes, from plain reads and before any write transaction, the
/// comparisons an operation on the store will need: one for each
/// program-successor record, and with `to_last` the one from the version
/// the store runs to the last. It is only a head start: a store that fails
/// to read here fails the same way inside the transaction, and one changed
/// in between is caught there.
fn prepare(connection: &Connection, members: Members<'_, '_>, to_last: bool) {
    let Some(lineage) = members.lineage() else {
        return;
    };
    let authorities = members.authorities();
    let Ok(segments) = load_upgrades(connection)
        .and_then(|upgrades| Segments::load(connection, authorities, upgrades))
    else {
        return;
    };
    for (index, stored) in segments.upgrades.iter().enumerate() {
        if matches!(
            upgrade::Kind::of_record(&stored.record),
            Some(upgrade::Kind::ProgramSuccessor | upgrade::Kind::Migration)
        ) {
            let (from, to) = (segments.position(index), segments.position(index + 1));
            // The outcome, failures included, is kept for the transaction.
            let _ = lineage.prepare_pair(connection, from, to);
        }
    }
    let (current, last) = (
        segments.position(segments.current()),
        authorities.len().saturating_sub(1),
    );
    if !to_last || authorities[current].identity() == authorities[last].identity() {
        return;
    }
    for (from, to) in lineage.hops(current, last) {
        // An upgrade across a behaviour change compares no programs, and a
        // rename compares none either.
        let moves = to == from + 1 && lineage.moves_state(from);
        if (moves || lineage.reviews(from, to).is_empty())
            && !matches!(lineage.steps().get(from), Some(Step::Rename) if moves)
        {
            let _ = lineage.prepare_pair(connection, from, to);
        }
    }
}

/// Runs `attempt`, which holds its write transaction only while it runs,
/// until no comparison it needs is missing. After an attempt that ended
/// with `Error::Unsettled`, and so wrote nothing, the missing pair is
/// established with no transaction open and the attempt runs again. A store
/// whose pairs remain unavailable, because other connections change it or
/// comparisons keep the shared memo busy, ends when the bounded retries
/// are exhausted with `Error::Unsettled`.
fn settle<T>(
    connection: &mut Connection,
    members: Members<'_, '_>,
    mut attempt: impl FnMut(&mut Connection, &Cell<Option<(usize, usize)>>) -> Result<T, Error>,
) -> Result<T, Error> {
    let missing = Cell::new(None);
    for _ in 0..=members.authorities().len() {
        match attempt(connection, &missing) {
            Err(Error::Unsettled) => match (members.lineage(), missing.take()) {
                (Some(lineage), Some((from, to))) => {
                    // The outcome, a missing premise or a refused
                    // simulation included, is kept for the next attempt;
                    // only an error ends here.
                    lineage.prepare_pair(connection, from, to)?;
                }
                _ => return Err(Error::Unsettled),
            },
            other => return other,
        }
    }
    Err(Error::Unsettled)
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
        // A full open establishes its comparisons before the lock; a
        // checkpoint open needs only the tail's, which the retry finds.
        if checkpoint.is_none() {
            prepare(&connection, members, false);
        }
        let (anchor, genesis, delivery_interpreter, position, data_version) =
            settle(&mut connection, members, |connection, missing| {
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
                    members.declared(missing),
                    &segments,
                    start,
                    consumed,
                )?;
                let position = segments.position(segments.current());
                let data_version = data_version(&tx)?;
                tx.commit()?;
                Ok((
                    anchor,
                    genesis,
                    delivery_interpreter,
                    position,
                    data_version,
                ))
            })?;
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
        let (members, expected_genesis) = (self.members, self.genesis);
        prepare(&self.connection, members, false);
        let (anchor, data_version) =
            settle(&mut self.connection, members, |connection, missing| {
                let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                check_schema(&tx)?;
                let authorities = members.authorities();
                let segments = Segments::load(&tx, authorities, load_upgrades(&tx)?)?;
                segments.require_current(authorities)?;
                let (genesis, initial) = genesis_anchor(&tx, segments.authority(authorities, 0))?;
                let anchor = audit_tail(
                    &tx,
                    authorities,
                    members.declared(missing),
                    &segments,
                    initial,
                    0,
                )?;
                if genesis != expected_genesis {
                    return Err(Error::History);
                }
                save_checkpoint(&tx, &anchor)?;
                let data_version = data_version(&tx)?;
                tx.commit()?;
                Ok((anchor, data_version))
            })?;
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
/// genesis publication over the state; a program successor establishes
/// all five premises again from the two versions' catalogs, exactly as the
/// upgrade did, and requires the stored receipt digests to be the ones the
/// lineage declares for the versions the record spans; a behaviour
/// change evaluates the new version's state laws and declared claims on the
/// recorded state again and requires the stored review texts to be the ones
/// the lineage declares; a migration takes the lineage's own forward
/// simulation of the step it declares, migrates the recorded state again
/// and requires the stored migration and state to be exactly those; and a
/// rename checks the two catalogs' exactness and frames the state again.
/// Returns the state a migration or rename moved the head to.
fn check_upgrade(
    connection: &Connection,
    lineage: &[Authority<'_>],
    declared: Declared<'_, '_, '_>,
    segments: &Segments,
    index: usize,
    anchor: &Anchor,
) -> Result<Option<Vec<u8>>, Error> {
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
    let mut moved = None;
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
            let (Some(declarer), true) = (declared.lineage, new < declared.len) else {
                return Err(Error::Identity);
            };
            let Some(receipts) = declarer.adoption_receipts(old, new) else {
                // The lineage declares another step than an adoption there.
                return Err(Error::Succession(upgrade::Unsupported::Receipts));
            };
            // Every premise, established from the lineage's own catalogs
            // before this transaction; the record must bind the same tuple
            // count.
            let Some(settled) = declarer.settled(old, new) else {
                declared.missing.set(Some((old, new)));
                return Err(Error::Unsettled);
            };
            let premises = match settled? {
                Ok(premises) => premises,
                Err(premise) => {
                    return Err(Error::Succession(upgrade::Unsupported::Premise(premise)));
                }
            };
            // The stored digests must be the lineage's, value for value.
            let evidence = upgrade::evidence_bytes(receipts);
            if upgrade.publication != evidence {
                // A record that is exact under the digests it stores was made
                // for another declaration of this lineage, and the store may
                // be intact. Any other record is damaged.
                let stored = upgrade::successor_admission(premises, &upgrade.publication);
                let record = upgrade::record_bytes(
                    kind,
                    ordinal,
                    sequence,
                    from,
                    &upgrade.identity,
                    &stored,
                    anchor.root,
                    anchor.chain,
                )?;
                let exact = upgrade.publication.len() == evidence.len()
                    && upgrade.record == record
                    && hash(zeno_fcis_codec::domains::V2_CHAIN, &record)? == upgrade.chain;
                return Err(if exact {
                    Error::Succession(upgrade::Unsupported::Receipts)
                } else {
                    Error::History
                });
            }
            upgrade::successor_admission(premises, &evidence)
        }
        upgrade::Kind::BehaviourChange => {
            let (Some(declarer), true) = (declared.lineage, new < declared.len) else {
                return Err(Error::Identity);
            };
            let reviews = declarer.reviews(old, new);
            let declared_texts = upgrade::framed_reviews(&reviews)?;
            // The admission is derived again: the new version's laws and
            // claims, as this lineage declares them, at the recorded state.
            let checked = behaviour::check(
                declarer.catalogs()[new],
                declarer.claims(new),
                &anchor.state,
            )
            .map_err(|_| Error::History)?;
            if reviews.is_empty() || upgrade.publication != declared_texts {
                // A record exact under the reviews stored beside it was made
                // with another declaration of this lineage; any other is
                // damaged.
                let exact = upgrade::parse_reviews(&upgrade.publication)
                    .filter(|texts| !texts.is_empty())
                    .map(|texts| {
                        let stored = upgrade::Behaviour::new(
                            checked.clone(),
                            texts.iter().map(|text| sha256(text)).collect(),
                        );
                        let record = upgrade::record_bytes(
                            kind,
                            ordinal,
                            sequence,
                            from,
                            &upgrade.identity,
                            &stored.admission()?,
                            anchor.root,
                            anchor.chain,
                        )?;
                        Ok::<_, Error>(
                            upgrade.record == record
                                && hash(zeno_fcis_codec::domains::V2_CHAIN, &record)?
                                    == upgrade.chain,
                        )
                    })
                    .transpose()?
                    .unwrap_or(false);
                return Err(if exact {
                    Error::Succession(upgrade::Unsupported::Reviews)
                } else {
                    Error::History
                });
            }
            upgrade::Behaviour::new(checked, reviews.iter().map(|text| sha256(text)).collect())
                .admission()?
        }
        upgrade::Kind::Migration | upgrade::Kind::Rename => {
            let (Some(declarer), true) = (declared.lineage, new < declared.len) else {
                return Err(Error::Identity);
            };
            let (description, stored) =
                upgrade::parse_move(&upgrade.publication).ok_or(Error::History)?;
            let catalogs = declarer.catalogs();
            // The lineage must declare exactly this step between the two
            // consecutive versions the record spans.
            let step = (new == old + 1)
                .then(|| declarer.steps().get(old).copied())
                .flatten();
            let (admission, state) = match (kind, step) {
                (
                    upgrade::Kind::Migration,
                    Some(lineage::Step::Migration {
                        migration, review, ..
                    }),
                ) => {
                    let Some(simulated) = declarer.simulated(old) else {
                        declared.missing.set(Some((old, new)));
                        return Err(Error::Unsettled);
                    };
                    let encoding = migration.encode().map_err(|_| Error::Range)?;
                    if description != encoding {
                        // A record exact under the migration stored beside
                        // it was made with another declaration of this
                        // lineage; any other is damaged.
                        let digest = sha256(description);
                        let exact = hash(zeno_fcis_codec::domains::V2_CHAIN, &upgrade.record)?
                            == upgrade.chain
                            && upgrade
                                .record
                                .windows(32)
                                .any(|window| window == digest.as_bytes());
                        return Err(if exact {
                            Error::Succession(upgrade::Unsupported::Migration)
                        } else {
                            Error::History
                        });
                    }
                    let simulated = simulated
                        .map_err(|_| Error::Succession(upgrade::Unsupported::Migration))?;
                    let state = migration::migrate_state(
                        catalogs[old],
                        catalogs[new],
                        &migration,
                        &anchor.state,
                    )
                    .map_err(|_| Error::History)?;
                    let root = hash(zeno_fcis_codec::domains::V2_STATE, &state)?;
                    (
                        upgrade::Migrated::new(
                            sha256(&encoding),
                            simulated,
                            root,
                            behaviour::check(catalogs[new], declarer.claims(new), &state).map_err(
                                |unmet| Error::Upgrade(upgrade::Refusal::MigrationState(unmet)),
                            )?,
                            sha256(review),
                        )
                        .admission()?,
                        state,
                    )
                }
                (upgrade::Kind::Rename, Some(lineage::Step::Rename)) => {
                    if !description.is_empty() {
                        return Err(Error::History);
                    }
                    if !migration::rename_exact(catalogs[old], catalogs[new])
                        .map_err(|_| Error::Range)?
                    {
                        return Err(Error::Succession(upgrade::Unsupported::Migration));
                    }
                    let state = migration::reframe(catalogs[old], catalogs[new], &anchor.state)
                        .ok_or(Error::History)?;
                    let root = hash(zeno_fcis_codec::domains::V2_STATE, &state)?;
                    (upgrade::rename_admission(root), state)
                }
                _ => return Err(Error::Succession(upgrade::Unsupported::Migration)),
            };
            if state != stored {
                return Err(Error::History);
            }
            moved = Some(state);
            admission
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
    Ok(moved)
}
/// Check every upgrade recorded at the anchor's head and advance its chain.
fn apply_upgrades(
    connection: &Connection,
    lineage: &[Authority<'_>],
    declared: Declared<'_, '_, '_>,
    segments: &Segments,
    anchor: &mut Anchor,
    mut consumed: usize,
) -> Result<usize, Error> {
    while let Some(upgrade) = segments.upgrades.get(consumed) {
        if upgrade.sequence > anchor.sequence {
            break;
        }
        if let Some(state) =
            check_upgrade(connection, lineage, declared, segments, consumed, anchor)?
        {
            anchor.root = hash(zeno_fcis_codec::domains::V2_STATE, &state)?;
            anchor.state = state;
        }
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
    declared: Declared<'_, '_, '_>,
    segments: &Segments,
    anchor: Anchor,
    consumed: usize,
) -> Result<Anchor, Error> {
    audit_tail_with_inputs(
        connection, lineage, declared, segments, anchor, consumed, None,
    )
}

fn audit_tail_with_inputs(
    connection: &Connection,
    lineage: &[Authority<'_>],
    declared: Declared<'_, '_, '_>,
    segments: &Segments,
    mut anchor: Anchor,
    mut consumed: usize,
    mut commits: Option<&mut Vec<AuditedCommit>>,
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
            declared,
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
        if let Some(commits) = commits.as_mut() {
            commits.push(AuditedCommit {
                sequence: u64::try_from(sequence).map_err(|_| Error::History)?,
                contract_version: segments.position(consumed) + 1,
                prestate: anchor.state.clone(),
                command: row.command.clone(),
                context: row.context.clone(),
                poststate: row.post.clone(),
            });
        }
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
        declared,
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
/// The state the last migration or rename recorded at head `sequence` moved
/// the head to, among the upgrades with ordinal at most `upto` when given;
/// `None` when none of them moved the state. The audit checks every stored
/// state against its record before any operation relies on it.
fn moved_state(
    connection: &Connection,
    sequence: i64,
    upto: Option<usize>,
) -> Result<Option<Vec<u8>>, Error> {
    let upto = match upto {
        Some(upto) => i64::try_from(upto).map_err(|_| Error::Range)?,
        None => i64::MAX,
    };
    let mut statement = connection.prepare(
        "SELECT record,publication FROM v2_upgrades WHERE sequence=?1 AND ordinal<=?2 ORDER BY ordinal DESC",
    )?;
    let rows = statement
        .query_map(params![sequence, upto], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (record, publication) in rows {
        if upgrade::Kind::of_record(&record).is_some_and(upgrade::Kind::moves_state) {
            let (_, state) = upgrade::parse_move(&publication).ok_or(Error::History)?;
            return Ok(Some(state.to_vec()));
        }
    }
    Ok(None)
}
/// The state a commit at `sequence` started from: the state after every
/// event at the previous head, a migration or rename included.
fn previous_state(connection: &Connection, sequence: i64) -> Result<Vec<u8>, Error> {
    if sequence <= 0 {
        return Err(Error::History);
    }
    if let Some(state) = moved_state(connection, sequence - 1, None)? {
        return Ok(state);
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
    match moved_state(connection, anchor.sequence, None)? {
        Some(state) => {
            if state != anchor.state {
                return Err(Error::History);
            }
        }
        None if anchor.sequence > 0 => {
            let row = load_commit(connection, anchor.sequence)?;
            if row.post != anchor.state || row.post_root != anchor.root {
                return Err(Error::History);
            }
        }
        None => {}
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
    let consumed = segments.consumed_at(connection, &c.anchor)?;
    match moved_state(connection, c.anchor.sequence, Some(consumed))? {
        Some(state) => {
            if state != c.anchor.state {
                return Err(Error::History);
            }
        }
        None if c.anchor.sequence > 0 => {
            let row = load_commit(connection, c.anchor.sequence)?;
            if row.post_root != c.anchor.root || row.post != c.anchor.state {
                return Err(Error::History);
            }
        }
        None => {}
    }
    let (root, chain): (Vec<u8>, Vec<u8>) = connection.query_row(
        "SELECT root,chain FROM v2_checkpoints WHERE sequence=?1",
        [c.anchor.sequence],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    // A later save at the same head, after an upgrade, replaces the marker;
    // after a migration or rename it holds another root too.
    if parse_hash(&chain)? != c.anchor.chain {
        return Err(Error::Checkpoint);
    }
    if parse_hash(&root)? != c.anchor.root {
        return Err(Error::History);
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
fn load_delivery(
    connection: &Connection,
    sequence: i64,
    lane: i64,
    ordinal: i64,
) -> Result<Delivery, Error> {
    let row = connection.query_row("SELECT channel,destination_root,payload_root,destination,payload,marker,delivery_id,entry_hash,acknowledged FROM v2_deliveries WHERE sequence=?1 AND lane=?2 AND ordinal=?3", params![sequence,lane,ordinal], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,Vec<u8>>(3)?,r.get::<_,Vec<u8>>(4)?,r.get::<_,Vec<u8>>(5)?,r.get::<_,Vec<u8>>(6)?,r.get::<_,Vec<u8>>(7)?,r.get::<_,i64>(8)?)))?;
    if !(0..=1).contains(&row.8) || !(0..=1).contains(&lane) {
        return Err(Error::Delivery);
    }
    Ok(Delivery {
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
            let actual = load_delivery(connection, sequence, lane as i64, d.ordinal() as i64)?;
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
/// Plain SHA-256 with no domain, as `sha256sum` and the command line's file
/// digests compute it: what an owner review's digest is.
fn sha256(bytes: &[u8]) -> Hash32 {
    <RustCryptoSha256 as CommitmentHasher>::hash(bytes)
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
    /// A lineage needs at least one version and exactly one step, an
    /// adoption receipt, a behaviour change, a migration or a rename,
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
    /// A relay acknowledgment names a delivery ID the store does not hold.
    /// Nothing was written.
    UnknownDelivery,
    /// A relay acknowledgment names a delivery the store already holds as
    /// acknowledged. Nothing was written.
    AlreadyAcknowledged,
    /// A relay acknowledgment carries a payload hash other than the SHA-256
    /// of the stored payload. Nothing was written.
    PayloadMismatch,
    /// Complete command/state/authorization/bundle bytes exceed the requested limit.
    Capacity {
        /// Actual complete four-component aggregate.
        required: usize,
        /// Requested limit, prior to any publication.
        declared: usize,
    },
    /// The pure upgrade decision refused; see [`upgrade::Refusal`].
    Upgrade(upgrade::Refusal),
    /// The store holds a program-successor, behaviour-change, migration or
    /// rename upgrade that the lineage it was opened with does not support; see
    /// [`upgrade::Unsupported`]. The store may be intact. Nothing was written.
    Succession(upgrade::Unsupported),
    /// Needed program comparisons or migration simulations stayed unavailable
    /// across the bounded transaction retries: another connection changed the
    /// store, or a shared memo stayed busy. Nothing was written.
    Unsettled,
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
/// One line: what happened, then what to do. The variant name stays in the
/// `Debug` form, for logs and machine output.
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(
                f,
                "SQLite refused the operation ({error}), so nothing was committed: check that the database file exists and is readable and writable, and that no other program locks it"
            ),
            Self::Encoding(error) => write!(
                f,
                "a value could not be canonically encoded ({error}), so nothing was written: keep values within the canonical encoding's limits"
            ),
            Self::Provider => f.write_str(
                "the pinned SHA-256 provider failed its known-answer check, so nothing can be hashed safely: rebuild with the pinned dependencies",
            ),
            Self::Schema(9) => f.write_str(
                "the database holds schema version 9, from before upgrades were recorded, or tables that differ from it: migrate an unaltered version 9 store to version 10 before any other operation, and create a new store only in a new, empty file",
            ),
            Self::Schema(10) => f.write_str(
                "the database already holds schema version 10, the current one, or tables that differ from it: create a new store only in a new, empty file; a version 10 store needs no migration, and an altered one cannot be opened",
            ),
            Self::Schema(version) => write!(
                f,
                "the database has schema version {version}, which is not a store this shell opens: open a store this shell created, or create a new store in a new, empty file"
            ),
            Self::Identity => f.write_str(
                "this database, or a checkpoint or publication used with it, belongs to a different contract: audit the database with the build that created it, or upgrade it with a build that adopted its contract",
            ),
            Self::Authority => f.write_str(
                "the library refused to bind a contract version of the lineage: regenerate the contract and rebuild the application with the same ZenoFCIS version",
            ),
            Self::Lineage => f.write_str(
                "the contract lineage is malformed: it needs at least one version and exactly one step, an adoption receipt, a behaviour change, a migration or a rename, between each version and the next; regenerate the contract",
            ),
            Self::Checkpoint => f.write_str(
                "the store no longer holds this checkpoint, because its head was saved again after an upgrade at the same head: open it from a newer checkpoint or with a full audit",
            ),
            Self::InvocationKind => f.write_str(
                "the publication has the wrong invocation kind: create a store only from a genesis publication, and commit or bundle only a transition",
            ),
            Self::History => f.write_str(
                "the stored history fails exact replay, root, certificate, upgrade record or chain validation: the database was changed outside this shell or damaged; restore it from a trusted copy before using it",
            ),
            Self::Range => f.write_str(
                "an integer or entry length cannot be represented, so nothing was written: the store or a value is beyond this shell's limits",
            ),
            Self::PreState => f.write_str(
                "the publication was made for a different state than the store's current one: decide again from the current state",
            ),
            Self::Replay => f.write_str(
                "this replay key was already used with different input or publication bytes: use a new replay key for a new command, or resend exactly the original bytes",
            ),
            Self::Concurrent => f.write_str(
                "another connection changed the store during this operation, so nothing was written: reopen the store and try again",
            ),
            Self::Delivery => f.write_str(
                "a delivery or its acknowledgement differs from what the store holds: acknowledge exactly the entry the store delivered; a malformed stored entry means the database was damaged",
            ),
            Self::Interpreter => f.write_str(
                "the delivery interpreter differs from the one the store was created with: deliver with the ZenoFCIS version that created the store",
            ),
            Self::UnknownDelivery => f.write_str(
                "the acknowledgment names a delivery ID this store does not hold, so nothing was written: acknowledge only IDs this store exported",
            ),
            Self::AlreadyAcknowledged => f.write_str(
                "the acknowledgment names a delivery this store already holds as acknowledged, so nothing was written: the delivery needs no further acknowledgment",
            ),
            Self::PayloadMismatch => f.write_str(
                "the acknowledgment's payload hash differs from the SHA-256 of the stored payload, so nothing was written and the delivery stays pending: acknowledge with the hash of the payload the store exported",
            ),
            Self::Capacity { required, declared } => write!(
                f,
                "the complete publication needs {required} bytes, more than the declared limit of {declared}, so nothing was written: raise the limit or publish a smaller decision"
            ),
            Self::Upgrade(upgrade::Refusal::StateSchema) => f.write_str(
                "upgrade refused: the new contract's state schema differs from the store's, and the application declares no migration or rename between them; declare a migration with `zeno-fcis contract evolve --migration`, keep the state schema, or start a new application",
            ),
            Self::Upgrade(upgrade::Refusal::SameContract) => f.write_str(
                "upgrade refused: the store already runs this contract version, so there is nothing to upgrade",
            ),
            Self::Upgrade(upgrade::Refusal::Genesis { missing, refusal }) => {
                write!(
                    f,
                    "upgrade refused: the new contract is not a program successor of the store's ({missing}), and its genesis laws do not admit the store's current state",
                )?;
                if let Some(refusal) = refusal {
                    write!(f, " (the library refused: {refusal:?})")?;
                }
                f.write_str(
                    "; a generated contract admits only its declared genesis state, so adopt the change as a program successor, or upgrade a store still at genesis",
                )
            }
            Self::Upgrade(upgrade::Refusal::Migration(unsimulated)) => write!(
                f,
                "upgrade refused: the declared data migration is not admitted, because {unsimulated}; nothing was written: declare a migration that keeps every observation, or take the change as a reviewed behaviour change"
            ),
            Self::Upgrade(upgrade::Refusal::Rename) => f.write_str(
                "upgrade refused: the new contract differs from the store's in more than names, so it is not a rename; nothing was written: regenerate the application, whose lineage then declares the change it is",
            ),
            Self::Upgrade(upgrade::Refusal::MigrationState(unmet)) => write!(
                f, "migration refused: the mapped state does not satisfy the target contract, because {unmet}; nothing was written: use a state satisfying the target laws and claims or revise the target contract"
            ),
            Self::Upgrade(upgrade::Refusal::Behaviour(unmet)) => write!(
                f,
                "upgrade refused: the reviewed rule change does not admit this store, because {unmet}; nothing was written: change the store's state through the old contract until the new rules hold, or change the new rules"
            ),
            Self::Succession(upgrade::Unsupported::Migration) => f.write_str(
                "the store holds a migration or rename upgrade that this application does not support: it does not declare, or does not admit, that step between those versions; nothing was written: open the store with the build that upgraded it, and restore it from a trusted copy if that build refuses it too",
            ),
            Self::Succession(upgrade::Unsupported::Reviews) => f.write_str(
                "the store holds a behaviour-change upgrade that this application does not support: it is a behaviour change with other owner reviews than this application declares for those versions; nothing was written: open the store with the build that upgraded it, and restore it from a trusted copy if that build refuses it too",
            ),
            Self::Succession(unsupported) => write!(
                f,
                "the store holds a program-successor upgrade that this application does not support: {unsupported}; nothing was written: open the store with the build and comparison cap that upgraded it, and restore it from a trusted copy if that build refuses it too"
            ),
            Self::Unsettled => f.write_str(
                "the store or its shared comparison or simulation cache stayed busy while the shell checked the decision programs or migrations it needs, so nothing was written: try again when other connections, comparisons and simulations are idle",
            ),
            Self::InjectedCrash(point) => write!(
                f,
                "an interruption was injected at {point:?} for a crash test: reopen the store, which holds its last committed state"
            ),
        }
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

#[cfg(test)]
mod message_tests {
    use super::*;
    use zeno_fcis_synthesis::finite::{
        v2_authority::Refusal as LibraryRefusal, v2_composition::Failure as CoreFailure,
        v2_laws::Failure as LawFailure,
    };

    /// The variant's name. No wildcard: a new variant does not compile until
    /// it has a message below.
    fn variant(error: &Error) -> &'static str {
        match error {
            Error::Sqlite(_) => "Sqlite",
            Error::Encoding(_) => "Encoding",
            Error::Provider => "Provider",
            Error::Schema(_) => "Schema",
            Error::Identity => "Identity",
            Error::Authority => "Authority",
            Error::Lineage => "Lineage",
            Error::Checkpoint => "Checkpoint",
            Error::InvocationKind => "InvocationKind",
            Error::History => "History",
            Error::Range => "Range",
            Error::PreState => "PreState",
            Error::Replay => "Replay",
            Error::Concurrent => "Concurrent",
            Error::Delivery => "Delivery",
            Error::Interpreter => "Interpreter",
            Error::UnknownDelivery => "UnknownDelivery",
            Error::AlreadyAcknowledged => "AlreadyAcknowledged",
            Error::PayloadMismatch => "PayloadMismatch",
            Error::Capacity { .. } => "Capacity",
            Error::Upgrade(upgrade::Refusal::StateSchema) => "Upgrade(StateSchema)",
            Error::Upgrade(upgrade::Refusal::SameContract) => "Upgrade(SameContract)",
            Error::Upgrade(upgrade::Refusal::Genesis { .. }) => "Upgrade(Genesis)",
            Error::Upgrade(upgrade::Refusal::Behaviour(_)) => "Upgrade(Behaviour)",
            Error::Upgrade(upgrade::Refusal::Migration(_)) => "Upgrade(Migration)",
            Error::Upgrade(upgrade::Refusal::MigrationState(_)) => "Upgrade(MigrationState)",
            Error::Upgrade(upgrade::Refusal::Rename) => "Upgrade(Rename)",
            Error::Succession(upgrade::Unsupported::Receipts) => "Succession(Receipts)",
            Error::Succession(upgrade::Unsupported::Reviews) => "Succession(Reviews)",
            Error::Succession(upgrade::Unsupported::Premise(_)) => "Succession(Premise)",
            Error::Succession(upgrade::Unsupported::Migration) => "Succession(Migration)",
            Error::Unsettled => "Unsettled",
            Error::InjectedCrash(_) => "InjectedCrash",
        }
    }

    #[test]
    fn every_store_error_reads_as_one_line_saying_what_happened_and_what_to_do() {
        let cases = [
            (
                Error::Sqlite(rusqlite::Error::QueryReturnedNoRows),
                "SQLite refused the operation (Query returned no rows), so nothing was committed: check that the database file exists and is readable and writable, and that no other program locks it",
            ),
            (
                Error::Encoding(EncodeError::LengthOverflow),
                "a value could not be canonically encoded (canonical length overflow), so nothing was written: keep values within the canonical encoding's limits",
            ),
            (
                Error::Provider,
                "the pinned SHA-256 provider failed its known-answer check, so nothing can be hashed safely: rebuild with the pinned dependencies",
            ),
            (
                Error::Schema(9),
                "the database holds schema version 9, from before upgrades were recorded, or tables that differ from it: migrate an unaltered version 9 store to version 10 before any other operation, and create a new store only in a new, empty file",
            ),
            (
                Error::Schema(10),
                "the database already holds schema version 10, the current one, or tables that differ from it: create a new store only in a new, empty file; a version 10 store needs no migration, and an altered one cannot be opened",
            ),
            (
                Error::Schema(0),
                "the database has schema version 0, which is not a store this shell opens: open a store this shell created, or create a new store in a new, empty file",
            ),
            (
                Error::Identity,
                "this database, or a checkpoint or publication used with it, belongs to a different contract: audit the database with the build that created it, or upgrade it with a build that adopted its contract",
            ),
            (
                Error::Authority,
                "the library refused to bind a contract version of the lineage: regenerate the contract and rebuild the application with the same ZenoFCIS version",
            ),
            (
                Error::Lineage,
                "the contract lineage is malformed: it needs at least one version and exactly one step, an adoption receipt, a behaviour change, a migration or a rename, between each version and the next; regenerate the contract",
            ),
            (
                Error::Checkpoint,
                "the store no longer holds this checkpoint, because its head was saved again after an upgrade at the same head: open it from a newer checkpoint or with a full audit",
            ),
            (
                Error::InvocationKind,
                "the publication has the wrong invocation kind: create a store only from a genesis publication, and commit or bundle only a transition",
            ),
            (
                Error::History,
                "the stored history fails exact replay, root, certificate, upgrade record or chain validation: the database was changed outside this shell or damaged; restore it from a trusted copy before using it",
            ),
            (
                Error::Range,
                "an integer or entry length cannot be represented, so nothing was written: the store or a value is beyond this shell's limits",
            ),
            (
                Error::PreState,
                "the publication was made for a different state than the store's current one: decide again from the current state",
            ),
            (
                Error::Replay,
                "this replay key was already used with different input or publication bytes: use a new replay key for a new command, or resend exactly the original bytes",
            ),
            (
                Error::Concurrent,
                "another connection changed the store during this operation, so nothing was written: reopen the store and try again",
            ),
            (
                Error::Delivery,
                "a delivery or its acknowledgement differs from what the store holds: acknowledge exactly the entry the store delivered; a malformed stored entry means the database was damaged",
            ),
            (
                Error::Interpreter,
                "the delivery interpreter differs from the one the store was created with: deliver with the ZenoFCIS version that created the store",
            ),
            (
                Error::UnknownDelivery,
                "the acknowledgment names a delivery ID this store does not hold, so nothing was written: acknowledge only IDs this store exported",
            ),
            (
                Error::AlreadyAcknowledged,
                "the acknowledgment names a delivery this store already holds as acknowledged, so nothing was written: the delivery needs no further acknowledgment",
            ),
            (
                Error::PayloadMismatch,
                "the acknowledgment's payload hash differs from the SHA-256 of the stored payload, so nothing was written and the delivery stays pending: acknowledge with the hash of the payload the store exported",
            ),
            (
                Error::Capacity {
                    required: 70_000,
                    declared: 65_536,
                },
                "the complete publication needs 70000 bytes, more than the declared limit of 65536, so nothing was written: raise the limit or publish a smaller decision",
            ),
            (
                Error::Upgrade(upgrade::Refusal::StateSchema),
                "upgrade refused: the new contract's state schema differs from the store's, and the application declares no migration or rename between them; declare a migration with `zeno-fcis contract evolve --migration`, keep the state schema, or start a new application",
            ),
            (
                Error::Upgrade(upgrade::Refusal::SameContract),
                "upgrade refused: the store already runs this contract version, so there is nothing to upgrade",
            ),
            (
                Error::Upgrade(upgrade::Refusal::Genesis {
                    missing: upgrade::Premise::Policy,
                    refusal: None,
                }),
                "upgrade refused: the new contract is not a program successor of the store's (premise 1: its policy differs from the store's contract in more than the decision program and its Step limit), and its genesis laws do not admit the store's current state; a generated contract admits only its declared genesis state, so adopt the change as a program successor, or upgrade a store still at genesis",
            ),
            (
                Error::Upgrade(upgrade::Refusal::Genesis {
                    missing: upgrade::Premise::Equivalence(
                        equivalence::Unestablished::Counterexample { ordinal: 41 },
                    ),
                    refusal: Some(LibraryRefusal::Core(CoreFailure::Law(LawFailure::Violated))),
                }),
                "upgrade refused: the new contract is not a program successor of the store's (premise 3: the two decision programs differ on input tuple 41 of their declared domain, counting from 0 in enumeration order), and its genesis laws do not admit the store's current state (the library refused: Core(Law(Violated))); a generated contract admits only its declared genesis state, so adopt the change as a program successor, or upgrade a store still at genesis",
            ),
            (
                Error::Upgrade(upgrade::Refusal::MigrationState(
                    behaviour::Unmet::Unevaluable,
                )),
                "migration refused: the mapped state does not satisfy the target contract, because the new contract's laws and claims cannot be evaluated on the store's current state; nothing was written: use a state satisfying the target laws and claims or revise the target contract",
            ),
            (
                Error::Upgrade(upgrade::Refusal::Behaviour(behaviour::Unmet::Law {
                    id: 500,
                    failure: LawFailure::Violated,
                })),
                "upgrade refused: the reviewed rule change does not admit this store, because state law 500 of the new contract is false on the store's current state; nothing was written: change the store's state through the old contract until the new rules hold, or change the new rules",
            ),
            (
                Error::Upgrade(upgrade::Refusal::Behaviour(behaviour::Unmet::Claim {
                    id: 600,
                    failure: LawFailure::Undefined,
                })),
                "upgrade refused: the reviewed rule change does not admit this store, because inductive claim 600 of the new contract has no value on the store's current state; nothing was written: change the store's state through the old contract until the new rules hold, or change the new rules",
            ),
            (
                Error::Upgrade(upgrade::Refusal::Migration(
                    migration::Unsimulated::Differs {
                        observation: migration::Observation::Deliveries,
                        ordinal: 7,
                    },
                )),
                "upgrade refused: the declared data migration is not admitted, because the observation `deliveries` differs between the old and the new contract at input tuple 7, so this is a behaviour change, not a migration; nothing was written: declare a migration that keeps every observation, or take the change as a reviewed behaviour change",
            ),
            (
                Error::Upgrade(upgrade::Refusal::Rename),
                "upgrade refused: the new contract differs from the store's in more than names, so it is not a rename; nothing was written: regenerate the application, whose lineage then declares the change it is",
            ),
            (
                Error::Succession(upgrade::Unsupported::Migration),
                "the store holds a migration or rename upgrade that this application does not support: it does not declare, or does not admit, that step between those versions; nothing was written: open the store with the build that upgraded it, and restore it from a trusted copy if that build refuses it too",
            ),
            (
                Error::Succession(upgrade::Unsupported::Reviews),
                "the store holds a behaviour-change upgrade that this application does not support: it is a behaviour change with other owner reviews than this application declares for those versions; nothing was written: open the store with the build that upgraded it, and restore it from a trusted copy if that build refuses it too",
            ),
            (
                Error::Succession(upgrade::Unsupported::Receipts),
                "the store holds a program-successor upgrade that this application does not support: it names other adoption receipts than this application declares for those versions; nothing was written: open the store with the build and comparison cap that upgraded it, and restore it from a trusted copy if that build refuses it too",
            ),
            (
                Error::Succession(upgrade::Unsupported::Premise(
                    upgrade::Premise::Equivalence(equivalence::Unestablished::DomainTooLarge {
                        size: Some(1_296_000),
                        cap: 1_000,
                    }),
                )),
                "the store holds a program-successor upgrade that this application does not support: this application's contracts do not establish it (premise 3: the decision programs' input domain has 1296000 tuples, more than the lineage's comparison cap of 1000); nothing was written: open the store with the build and comparison cap that upgraded it, and restore it from a trusted copy if that build refuses it too",
            ),
            (
                Error::Unsettled,
                "the store or its shared comparison or simulation cache stayed busy while the shell checked the decision programs or migrations it needs, so nothing was written: try again when other connections, comparisons and simulations are idle",
            ),
            (
                Error::InjectedCrash(CrashPoint::AfterCommit),
                "an interruption was injected at AfterCommit for a crash test: reopen the store, which holds its last committed state",
            ),
        ];
        let mut covered = std::collections::BTreeSet::new();
        for (error, message) in &cases {
            assert_eq!(error.to_string(), *message, "{error:?}");
            assert!(!message.contains('\n'), "{error:?}");
            // The variant name stays in the Debug form.
            let name = variant(error);
            let stem = name.split('(').next().unwrap_or(name);
            assert!(format!("{error:?}").starts_with(stem), "{error:?}");
            covered.insert(name);
        }
        assert_eq!(covered.len(), 33, "{covered:?}");
    }
}
