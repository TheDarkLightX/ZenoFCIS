//! A store opened under an application's contract lineage, as typed handles.
//!
//! What a store holds decides what can be done with it. A schema v9 store can
//! only be migrated. A v10 store that runs an earlier version of the lineage
//! can be audited and read, or upgraded to the last version. Only a store
//! that runs the last version commits and delivers. Each transition consumes
//! its handle and returns the next one, all its checks and writes happen in
//! one immediate transaction, and every refusal writes nothing.
//!
//! A program comparison never runs while a write transaction is open. What
//! it establishes depends only on two of the lineage's catalogs, so the
//! lineage keeps each pair's outcome in memory, and an operation establishes
//! the pairs it needs from plain reads before it takes the lock. Inside the
//! transaction the shell only looks a pair up without waiting for the memo;
//! a missing pair or a busy memo ends the attempt with nothing written.
//! Establishing or waiting for the pair happens before the next transaction.

use std::{
    cell::Cell,
    collections::BTreeMap,
    path::Path,
    sync::{
        Mutex, PoisonError, TryLockError,
        atomic::{AtomicUsize, Ordering},
    },
};

use rusqlite::{Connection, TransactionBehavior, params};
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::MemoryDestination;
use zeno_fcis_synthesis::finite::{
    v2_authority::{self as authority, Authority, PublicationOutcome},
    v2_catalog::BoundCatalog,
};

use super::equivalence::DEFAULT_MAX_INPUT_TUPLES;
use super::upgrade::{self, Admission, Facts, Kind, Premise, Premises, Successor};
use super::{
    Anchor, Checkpoint, CrashPoint, Error, Members, Segments, Snapshot, UpgradeReceipt,
    V2SqliteShell, audit_tail, check_schema, check_schema_v9, compact_publication, configure,
    data_version, genesis_anchor, inject, load_upgrades, migration_v9_to_v10, open_existing,
    prepare, provider, settle,
};

/// The complete outcome of establishing the premises for one pair of
/// lineage versions.
#[derive(Clone, Copy, Debug)]
enum Settled {
    /// Every premise held, or the first missing one.
    Established(Result<Premises, Premise>),
    /// A policy could not be encoded: `Error::Range`.
    Unencodable,
}

#[cfg(test)]
mod tests;

impl Settled {
    fn result(self) -> Result<Result<Premises, Premise>, Error> {
        match self {
            Self::Established(established) => Ok(established),
            Self::Unencodable => Err(Error::Range),
        }
    }
}

/// An application's contract lineage, oldest version first and the version
/// it runs now last: each version's checked catalog and the library Authority
/// bound from it, between each version and the next the SHA-256 of the
/// `transform` receipt that adopted the later one, and the cap on the input
/// tuples one comparison of two decision programs may enumerate.
///
/// The receipt digests are provenance, not evidence the shell checks: the
/// shell replays no receipt. It establishes a program succession itself,
/// from the two catalogs, by comparing the two decision programs on every
/// input tuple ([`Successor::establish`]). A generated contract's
/// `with_lineage` passes the digests of receipts that `zeno-fcis generate
/// contract` replayed. A program-successor upgrade records the ones it spans,
/// and an audit requires the recorded digests to be exactly these.
///
/// The lineage also keeps, in memory only, the outcome of establishing the
/// premises for each pair of versions it was asked about, so one lineage
/// value compares two programs at most once. The outcome depends only on
/// the two catalogs it holds immutably; it is never stored in or read from a
/// store, and another lineage value establishes it afresh.
pub struct Lineage<'c, 'p> {
    catalogs: Vec<&'c BoundCatalog<'p>>,
    authorities: Vec<Authority<'p>>,
    receipts: Vec<Hash32>,
    max_input_tuples: u64,
    settled: Mutex<BTreeMap<(usize, usize), Settled>>,
    comparisons: AtomicUsize,
}

impl<'c, 'p> Lineage<'c, 'p> {
    /// Binds every version's catalog into its Authority through the library,
    /// with the default comparison cap of 100,000,000 input tuples, the
    /// default of `zeno-fcis transform check`.
    ///
    /// # Errors
    /// `Error::Lineage` without a version, or unless there is exactly one
    /// receipt between each version and the next; `Error::Authority` when a
    /// catalog does not bind.
    pub fn bind(catalogs: &[&'c BoundCatalog<'p>], receipts: &[[u8; 32]]) -> Result<Self, Error> {
        Self::bind_with_cap(catalogs, receipts, DEFAULT_MAX_INPUT_TUPLES)
    }

    /// [`Lineage::bind`] with another cap on the input tuples one comparison
    /// of two decision programs may enumerate. A program succession whose
    /// domain exceeds the cap is not established: the upgrade takes the
    /// genesis route, and a store that holds such a record does not audit.
    ///
    /// # Errors
    /// As [`Lineage::bind`].
    pub fn bind_with_cap(
        catalogs: &[&'c BoundCatalog<'p>],
        receipts: &[[u8; 32]],
        max_input_tuples: u64,
    ) -> Result<Self, Error> {
        if catalogs.is_empty() || receipts.len() + 1 != catalogs.len() {
            return Err(Error::Lineage);
        }
        let authorities = catalogs
            .iter()
            .map(|catalog| authority::bind(catalog).map_err(|_| Error::Authority))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            catalogs: catalogs.to_vec(),
            authorities,
            receipts: receipts.iter().copied().map(Hash32::new).collect(),
            max_input_tuples,
            settled: Mutex::new(BTreeMap::new()),
            comparisons: AtomicUsize::new(0),
        })
    }

    /// How many times this lineage value enumerated two decision programs:
    /// at most once for each pair of versions.
    pub fn comparisons(&self) -> usize {
        self.comparisons.load(Ordering::Relaxed)
    }

    fn memo(&self) -> std::sync::MutexGuard<'_, BTreeMap<(usize, usize), Settled>> {
        // The map only ever gains complete entries, so a panic elsewhere
        // cannot leave it inconsistent.
        self.settled.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The premises of a program succession from version `from` to version
    /// `to`, positions from 0, established once for this lineage value and
    /// then remembered, failures included. It runs the program comparison,
    /// so the connection must not be inside a transaction: no comparison
    /// runs while a write lock is held.
    ///
    /// # Errors
    /// `Error::Lineage` unless `from < to <= last`, and `Error::Range` when a
    /// policy cannot be encoded.
    pub(super) fn establish(
        &self,
        connection: &Connection,
        from: usize,
        to: usize,
    ) -> Result<Result<Premises, Premise>, Error> {
        debug_assert!(
            connection.is_autocommit(),
            "a program comparison must not run inside a transaction"
        );
        if from >= to || to >= self.versions() {
            return Err(Error::Lineage);
        }
        let mut memo = self.memo();
        if let Some(settled) = memo.get(&(from, to)) {
            return settled.result();
        }
        let settled = match upgrade::premises(
            self.catalogs[from],
            self.catalogs[to],
            self.max_input_tuples,
        ) {
            Ok((established, enumerated)) => {
                if enumerated {
                    self.comparisons.fetch_add(1, Ordering::Relaxed);
                }
                Settled::Established(established)
            }
            Err(Error::Range) => Settled::Unencodable,
            Err(error) => return Err(error),
        };
        memo.insert((from, to), settled);
        settled.result()
    }

    /// The remembered outcome for the pair, if this lineage established it;
    /// never computes one or waits for a comparison. A busy memo is
    /// unavailable, so the caller releases SQLite before establishing it.
    pub(super) fn settled(
        &self,
        from: usize,
        to: usize,
    ) -> Option<Result<Result<Premises, Premise>, Error>> {
        let memo = match self.settled.try_lock() {
            Ok(memo) => memo,
            Err(TryLockError::WouldBlock) => return None,
            // As in memo(), only complete entries can survive a panic.
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        memo.get(&(from, to)).copied().map(Settled::result)
    }

    /// The cap on the input tuples one comparison may enumerate.
    pub fn max_input_tuples(&self) -> u64 {
        self.max_input_tuples
    }

    /// The declared adoption receipt digests, oldest first: one between each
    /// version and the next.
    pub fn receipts(&self) -> &[Hash32] {
        &self.receipts
    }

    /// The number of versions; the last is the one the application runs.
    pub fn versions(&self) -> usize {
        self.authorities.len()
    }

    /// Every version's Authority, oldest first.
    pub fn authorities(&self) -> &[Authority<'p>] {
        &self.authorities
    }

    pub(super) fn catalogs(&self) -> &[&'c BoundCatalog<'p>] {
        &self.catalogs
    }

    /// Opens the store at `path`. A v10 store is fully audited first: every
    /// history segment replays under the version that published it. A v9
    /// store must be exactly the previous shell's; its migration audits it.
    /// A missing file is refused and not created.
    ///
    /// # Errors
    /// `Schema` for a schema other than v9 or v10 or a v9 store that is not
    /// exactly the previous shell's, `Identity` for a segment under no
    /// version or out of order, and `History` for a failed audit.
    pub fn open(&self, path: impl AsRef<Path>) -> Result<Opened<'_, 'p>, Error> {
        let mut connection = open_existing(path)?;
        provider()?;
        configure(&connection)?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 9 {
            // A read transaction: the check writes nothing.
            let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            check_schema_v9(&tx)?;
            tx.commit()?;
            return Ok(Opened::V9(V9Store {
                connection,
                lineage: self,
            }));
        }
        let (shell, position) =
            V2SqliteShell::reopen(connection, self.members(self.versions()), None)?;
        Ok(Opened::V10(self.store(shell, position)))
    }

    /// Opens a v10 store after a privately validated prefix, auditing only
    /// the tail. The checkpoint is a trusted input.
    ///
    /// # Errors
    /// As [`Lineage::open`], `Identity` for another store's checkpoint, and
    /// `Checkpoint` when a later save at its head replaced it.
    pub fn open_at_checkpoint(
        &self,
        path: impl AsRef<Path>,
        checkpoint: &Checkpoint,
    ) -> Result<Store<'_, 'p>, Error> {
        let (shell, position) = V2SqliteShell::reopen(
            open_existing(path)?,
            self.members(self.versions()),
            Some(checkpoint),
        )?;
        Ok(self.store(shell, position))
    }

    fn members(&self, len: usize) -> Members<'_, 'p> {
        Members::Lineage { lineage: self, len }
    }

    /// The handle for an audited store whose current segment runs the
    /// version at `position`. It is current when that version's identity is
    /// the last version's; an identity binds the complete policy and the
    /// evaluator, so a repeated one is the same contract.
    fn store<'a>(&'a self, mut shell: V2SqliteShell<'a, 'p>, position: usize) -> Store<'a, 'p> {
        let last = self.versions() - 1;
        if self.authorities[position].identity() == self.authorities[last].identity() {
            shell.members = self.members(self.versions());
            Store::Current(shell)
        } else {
            shell.members = self.members(position + 1);
            Store::Superseded(Superseded {
                shell,
                lineage: self,
            })
        }
    }
}

/// A store, by the schema version it has.
pub enum Opened<'a, 'p> {
    /// Schema v10: it runs the lineage's last version or an earlier one.
    V10(Store<'a, 'p>),
    /// Schema v9, from before upgrades were recorded: migrate it first.
    V9(V9Store<'a, 'p>),
}

/// A schema v10 store, by the lineage version it runs.
pub enum Store<'a, 'p> {
    /// It runs the lineage's last version: every operation.
    Current(V2SqliteShell<'a, 'p>),
    /// It runs an earlier version: it can be audited and read, or upgraded.
    Superseded(Superseded<'a, 'p>),
}

/// A schema v10 store that runs an earlier version of its lineage. It can be
/// audited and read, or upgraded to the lineage's last version; it commits
/// and delivers nothing until it is upgraded.
pub struct Superseded<'a, 'p> {
    /// Bound to the lineage up to and including the store's version.
    shell: V2SqliteShell<'a, 'p>,
    lineage: &'a Lineage<'a, 'p>,
}

impl<'a, 'p> Superseded<'a, 'p> {
    /// The lineage version the store runs, from 1.
    pub fn version(&self) -> usize {
        self.shell.lineage().len()
    }

    /// The contract the store runs.
    pub fn authority(&self) -> &'a Authority<'p> {
        self.shell.authority()
    }

    /// Revalidates all history under the versions up to the store's, and
    /// saves a checkpoint at its head.
    ///
    /// # Errors
    /// As [`V2SqliteShell::audit`].
    pub fn audit(&mut self) -> Result<Checkpoint, Error> {
        self.shell.audit()
    }

    /// The checked head and durable counts; see [`V2SqliteShell::snapshot`].
    ///
    /// # Errors
    /// As [`V2SqliteShell::snapshot`].
    pub fn snapshot(&mut self) -> Result<Snapshot, Error> {
        self.shell.snapshot()
    }

    /// Records the checked upgrade to the lineage's last version and returns
    /// the store's handle under the whole lineage, with the receipt.
    ///
    /// After a complete audit under the whole lineage, the shell tries to
    /// establish every premise of a program succession from the version the
    /// store runs to the last one ([`Successor::establish`]), including an
    /// exhaustive comparison of the two decision programs under the lineage's
    /// cap. A program successor is admitted at any state and records the
    /// number of tuples compared and the lineage's receipt digests of the
    /// adoptions between them. When any premise is missing, the new contract
    /// must admit the current state through its genesis evaluation. The pure
    /// [`upgrade::decide`] then requires equal canonical state schemas and
    /// different identities. The record becomes the next chain link, and
    /// pending deliveries keep their IDs and order.
    ///
    /// # Errors
    /// `Schema` for a store that is no longer v10, `Identity` for a segment
    /// under no version, `History` for a failed audit, `Concurrent` when the
    /// head moved, and `Upgrade` for the decision's refusals; a genesis
    /// refusal names the missing premise.
    pub fn upgrade(self) -> Result<(V2SqliteShell<'a, 'p>, UpgradeReceipt), Error> {
        self.upgrade_with_crash(None)
    }

    /// [`Superseded::upgrade`] with a deterministic injected interruption for
    /// crash tests: `BeforeTransaction`, `AfterValidation` (decided),
    /// `AfterStateWrite` (the chain tip), `AfterReplayWrite` (the record row),
    /// `BeforeCommit` and `AfterCommit`.
    ///
    /// # Errors
    /// As [`Superseded::upgrade`], and `InjectedCrash`.
    pub fn upgrade_with_crash(
        self,
        crash: Option<CrashPoint>,
    ) -> Result<(V2SqliteShell<'a, 'p>, UpgradeReceipt), Error> {
        let Self { shell, lineage } = self;
        let V2SqliteShell {
            mut connection,
            delivery_interpreter,
            ..
        } = shell;
        let members = lineage.members(lineage.versions());
        // Plain reads first: every comparison this upgrade needs runs before
        // the transaction takes the lock.
        prepare(&connection, members, true);
        let (anchor, genesis, data_version, receipt) =
            settle(&mut connection, members, |connection, missing| {
                upgrade_once(connection, lineage, crash, missing)
            })?;
        let shell = V2SqliteShell {
            members,
            connection,
            anchor: Anchor {
                chain: receipt.chain,
                ..anchor
            },
            genesis,
            delivery_interpreter,
            data_version,
        };
        inject(crash, CrashPoint::AfterCommit)?;
        Ok((shell, receipt))
    }
}

/// One attempt at the upgrade, in one immediate transaction. A comparison it
/// needs that is not yet established ends it with `Error::Unsettled` and the
/// pair in `missing`, having written nothing.
fn upgrade_once(
    connection: &mut Connection,
    lineage: &Lineage<'_, '_>,
    crash: Option<CrashPoint>,
    missing: &Cell<Option<(usize, usize)>>,
) -> Result<(Anchor, Hash32, i64, UpgradeReceipt), Error> {
    inject(crash, CrashPoint::BeforeTransaction)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_schema(&tx)?;
    let (authorities, catalogs) = (lineage.authorities(), lineage.catalogs());
    let segments = Segments::load(&tx, authorities, load_upgrades(&tx)?)?;
    let (genesis, initial) = genesis_anchor(&tx, segments.authority(authorities, 0))?;
    let declared = lineage.members(lineage.versions()).declared(missing);
    let anchor = audit_tail(&tx, authorities, declared, &segments, initial, 0)?;
    let (current, last) = (
        segments.position(segments.current()),
        lineage.versions() - 1,
    );
    let (from, to) = (&authorities[current], &authorities[last]);
    let ordinal = u64::try_from(segments.upgrades.len())
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or(Error::Range)?;
    let sequence = u64::try_from(anchor.sequence).map_err(|_| Error::Range)?;
    // Another connection may have upgraded the store since it was
    // opened; the decision then refuses the same contract first.
    let same = from.identity() == to.identity();
    // The same contract establishes nothing: the decision refuses it first.
    let established = if same {
        Err(Premise::Policy)
    } else {
        let Some(settled) = lineage.settled(current, last) else {
            missing.set(Some((current, last)));
            return Err(Error::Unsettled);
        };
        settled?.map(|premises| Successor::held(&lineage.receipts[current..last], premises))
    };
    let successor = established.ok();
    let genesis_outcome = (!same && successor.is_none()).then(|| to.publish_genesis(&anchor.state));
    let admission = match (established, &genesis_outcome) {
        (Ok(successor), _) => Admission::Successor(successor),
        (Err(missing), Some(PublicationOutcome::Commit(publication))) => Admission::Genesis(
            missing,
            upgrade::Genesis {
                subject: publication.subject(),
                poststate: publication.poststate(),
            },
        ),
        (Err(missing), Some(PublicationOutcome::Refused { error, .. })) => {
            Admission::Refused(missing, Some(*error))
        }
        (Err(missing), _) => Admission::Refused(missing, None),
    };
    let plan = upgrade::decide(&Facts {
        ordinal,
        sequence,
        root: anchor.root,
        previous_chain: anchor.chain,
        from_identity: from.identity(),
        from_schema: catalogs[current].original_schema(),
        identity: to.identity(),
        schema: catalogs[last].original_schema(),
        state: &anchor.state,
        admission,
    })?;
    let admitted = match (plan.kind(), &genesis_outcome) {
        (Kind::ProgramSuccessor, _) => plan.evidence().to_vec(),
        (Kind::GenesisAdmission, Some(PublicationOutcome::Commit(publication))) => {
            compact_publication(to.identity(), publication.subject())?
        }
        _ => {
            return Err(Error::Upgrade(upgrade::Refusal::Genesis {
                missing: Premise::Policy,
                refusal: None,
            }));
        }
    };
    inject(crash, CrashPoint::AfterValidation)?;
    // The head keeps its sequence, state and root; only the chain tip moves.
    let changed = tx.execute(
        "UPDATE v2_state SET chain=?1 WHERE singleton=1 AND sequence=?2 AND root=?3 AND chain=?4",
        params![
            plan.chain().as_bytes().as_slice(),
            anchor.sequence,
            anchor.root.as_bytes().as_slice(),
            anchor.chain.as_bytes().as_slice()
        ],
    )?;
    if changed != 1 {
        return Err(Error::Concurrent);
    }
    inject(crash, CrashPoint::AfterStateWrite)?;
    tx.execute(
        "INSERT INTO v2_upgrades VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            i64::try_from(ordinal).map_err(|_| Error::Range)?,
            anchor.sequence,
            to.identity(),
            admitted,
            anchor.root.as_bytes().as_slice(),
            anchor.chain.as_bytes().as_slice(),
            plan.record(),
            plan.chain().as_bytes().as_slice()
        ],
    )?;
    inject(crash, CrashPoint::AfterReplayWrite)?;
    inject(crash, CrashPoint::BeforeCommit)?;
    let data_version = data_version(&tx)?;
    tx.commit()?;
    let receipt = UpgradeReceipt {
        kind: plan.kind(),
        premises: plan.premises(),
        ordinal,
        sequence,
        chain: plan.chain(),
        from_identity: from.identity().to_vec(),
        identity: to.identity().to_vec(),
        receipts: successor.map_or_else(Vec::new, |successor| successor.receipts().to_vec()),
        record: plan.record().to_vec(),
    };
    Ok((anchor, genesis, data_version, receipt))
}

/// A schema v9 store, created before upgrades were recorded. Its one
/// operation is the explicit migration.
pub struct V9Store<'a, 'p> {
    connection: Connection,
    lineage: &'a Lineage<'a, 'p>,
}

impl<'a, 'p> V9Store<'a, 'p> {
    /// Converts the store to schema v10 in one transaction, after a complete
    /// audit under the version it was created with, and returns it by the
    /// version it runs. Nothing else is migrated.
    ///
    /// # Errors
    /// `Schema` for a store that is no longer exactly v9, `Identity` for a
    /// store under no version of the lineage, and `History` for a failed
    /// audit.
    pub fn migrate(self) -> Result<Store<'a, 'p>, Error> {
        self.migrate_with_crash(None)
    }

    /// [`V9Store::migrate`] with a deterministic injected interruption for
    /// crash tests: `BeforeTransaction`, `AfterValidation` (audited),
    /// `AfterStateWrite` (the new table and version), `BeforeCommit` and
    /// `AfterCommit`.
    ///
    /// # Errors
    /// As [`V9Store::migrate`], and `InjectedCrash`.
    pub fn migrate_with_crash(self, crash: Option<CrashPoint>) -> Result<Store<'a, 'p>, Error> {
        let Self {
            mut connection,
            lineage,
        } = self;
        inject(crash, CrashPoint::BeforeTransaction)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_schema_v9(&tx)?;
        let identity: Vec<u8> = tx.query_row(
            "SELECT identity FROM v2_genesis WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        // The earliest version with the store's identity, as `assign` takes it.
        let position = lineage
            .authorities()
            .iter()
            .position(|member| member.identity() == identity)
            .ok_or(Error::Identity)?;
        let members = lineage.members(position + 1);
        let authorities = members.authorities();
        let segments = Segments::load(&tx, authorities, Vec::new())?;
        let (genesis, initial) = genesis_anchor(&tx, segments.authority(authorities, 0))?;
        // A v9 store holds no upgrade, so its audit compares no programs.
        let none = Cell::new(None);
        let anchor = audit_tail(
            &tx,
            authorities,
            members.declared(&none),
            &segments,
            initial,
            0,
        )?;
        let delivery_interpreter = MemoryDestination::interpreter_identity()?;
        inject(crash, CrashPoint::AfterValidation)?;
        tx.execute_batch(&migration_v9_to_v10())?;
        check_schema(&tx)?;
        inject(crash, CrashPoint::AfterStateWrite)?;
        inject(crash, CrashPoint::BeforeCommit)?;
        let data_version = data_version(&tx)?;
        tx.commit()?;
        let shell = V2SqliteShell {
            members,
            connection,
            anchor,
            genesis,
            delivery_interpreter,
            data_version,
        };
        inject(crash, CrashPoint::AfterCommit)?;
        Ok(lineage.store(shell, position))
    }
}
