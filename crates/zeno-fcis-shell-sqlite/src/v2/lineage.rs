//! A store opened under an application's contract lineage, as typed handles.
//!
//! What a store holds decides what can be done with it. A schema v9 store can
//! only be migrated. A v10 store that runs an earlier version of the lineage
//! can be audited and read, or upgraded to the last version. Only a store
//! that runs the last version commits and delivers. Each transition consumes
//! its handle and returns the next one, all its checks and writes happen in
//! one immediate transaction, and every refusal writes nothing.
//!
//! The steps between consecutive versions are declared with the lineage:
//! an adoption, whose new version may be admitted as a program successor;
//! a behaviour change, a reviewed rule change admitted when the new
//! version's state laws and declared claims hold on the store's state; a
//! data migration, admitted by forward simulation over the old version's
//! whole declared input domain; or a rename, admitted when only names
//! differ. An upgrade across a behaviour change compares no decision
//! programs at all, and an upgrade across a migration or rename moves the
//! head's state to the new schema, one recorded hop per such step.
//!
//! A program comparison never runs while a write transaction is open. What
//! it establishes depends only on two of the lineage's catalogs, so the
//! lineage keeps each pair's outcome in memory, and an operation establishes
//! the pairs it needs from plain reads before it takes the lock. Inside the
//! transaction the shell only looks a pair up without waiting for the memo;
//! a missing pair or a busy memo ends the attempt with nothing written.
//! Establishing or waiting for the pair happens before the next transaction.
//! A migration's forward simulation is kept and looked up the same way.

use std::{
    cell::Cell,
    collections::BTreeMap,
    path::Path,
    sync::{
        Mutex, PoisonError, TryLockError,
        atomic::{AtomicUsize, Ordering},
    },
};

use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::MemoryDestination;
use zeno_fcis_synthesis::finite::{
    v2_authority::{self as authority, Authority, PublicationOutcome},
    v2_catalog::BoundCatalog,
};

use super::behaviour::{self, Claim};
use super::equivalence::DEFAULT_MAX_INPUT_TUPLES;
use super::migration::{self, Migration, Simulated, Unsimulated};
use super::upgrade::{
    self, Admission, Behaviour, Facts, Kind, Migrated, Premise, Premises, Successor,
};
use super::{
    Anchor, AuditedHistory, Checkpoint, CrashPoint, Error, Members, Segments, Snapshot,
    UpgradeReceipt, V2SqliteShell, audit_tail, audit_tail_with_inputs, check_schema,
    check_schema_v9, compact_publication, configure, data_version, genesis_anchor, hash, inject,
    load_upgrades, migration_v9_to_v10, open_existing, prepare, provider, settle, sha256,
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

/// How one version of a lineage follows the previous one.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Step<'s> {
    /// The later version adopted a new decision program: the SHA-256 of the
    /// `transform` receipt that adopted it.
    Adoption([u8; 32]),
    /// The later version changes the rules, as `zeno-fcis contract evolve`
    /// recorded it after the owner's review.
    BehaviourChange {
        /// The plain-language diff of the change, exactly as the owner
        /// reviewed it; an upgrade binds its SHA-256 and stores the text.
        review: &'s [u8],
        /// The later version's inductive claims, compiled to law programs
        /// over the state; they must hold on the state, as its state laws
        /// must. They also apply to versions that later adoptions add.
        claims: &'s [Claim<'s>],
    },
    /// The later version changes the state's layout, and this migration
    /// maps the earlier version's state to it, as `zeno-fcis contract
    /// evolve --migration` recorded it. An upgrade admits it only by
    /// forward simulation (see [`migration`](crate::v2::migration)) and by
    /// checking the target state laws and claims on the mapped current state.
    Migration {
        /// The declared mapping between state layouts.
        migration: Migration<'s>,
        /// Freshly compiled inductive claims of the target version.
        claims: &'s [Claim<'s>],
        /// Recomputed owner review binding the target claim declarations.
        review: &'s [u8],
    },
    /// The later version renames profile, type, field or variant names and
    /// changes nothing else; an upgrade checks that exactly.
    Rename,
}

/// An application's contract lineage, oldest version first and the version
/// it runs now last: each version's checked catalog and the library Authority
/// bound from it, between each version and the next the [`Step`] that made
/// the later one, and the cap on the input tuples one comparison of two
/// decision programs may enumerate.
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
    steps: Vec<Step<'c>>,
    /// One per step: the adoption's receipt digest, or zeros for any other
    /// step, which [`Lineage::adoption_receipts`] never returns.
    receipts: Vec<Hash32>,
    max_input_tuples: u64,
    settled: Mutex<BTreeMap<(usize, usize), Settled>>,
    comparisons: AtomicUsize,
    /// Each migration step's simulation outcome, by step position.
    simulated: Mutex<BTreeMap<usize, Result<Simulated, Unsimulated>>>,
    simulations: AtomicUsize,
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

    /// Binds a lineage whose steps may include behaviour changes, with the
    /// default comparison cap.
    ///
    /// # Errors
    /// `Error::Lineage` without a version, or unless there is exactly one
    /// step between each version and the next; `Error::Authority` when a
    /// catalog does not bind.
    pub fn bind_steps(
        catalogs: &[&'c BoundCatalog<'p>],
        steps: &[Step<'c>],
    ) -> Result<Self, Error> {
        Self::bind_steps_with_cap(catalogs, steps, DEFAULT_MAX_INPUT_TUPLES)
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
        let steps: Vec<Step<'c>> = receipts.iter().copied().map(Step::Adoption).collect();
        Self::bind_steps_with_cap(catalogs, &steps, max_input_tuples)
    }

    /// [`Lineage::bind_steps`] with another comparison cap.
    ///
    /// # Errors
    /// As [`Lineage::bind_steps`].
    pub fn bind_steps_with_cap(
        catalogs: &[&'c BoundCatalog<'p>],
        steps: &[Step<'c>],
        max_input_tuples: u64,
    ) -> Result<Self, Error> {
        if catalogs.is_empty() || steps.len() + 1 != catalogs.len() {
            return Err(Error::Lineage);
        }
        let authorities = catalogs
            .iter()
            .map(|catalog| authority::bind(catalog).map_err(|_| Error::Authority))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            catalogs: catalogs.to_vec(),
            authorities,
            steps: steps.to_vec(),
            receipts: steps
                .iter()
                .map(|step| match step {
                    Step::Adoption(receipt) => Hash32::new(*receipt),
                    Step::BehaviourChange { .. } | Step::Migration { .. } | Step::Rename => {
                        Hash32::new([0; 32])
                    }
                })
                .collect(),
            max_input_tuples,
            settled: Mutex::new(BTreeMap::new()),
            comparisons: AtomicUsize::new(0),
            simulated: Mutex::new(BTreeMap::new()),
            simulations: AtomicUsize::new(0),
        })
    }

    /// How many times this lineage value ran a migration's forward
    /// simulation: at most once for each migration step.
    pub fn simulations(&self) -> usize {
        self.simulations.load(Ordering::Relaxed)
    }

    /// The forward simulation of the migration at step `step`, from version
    /// `step` to the next, positions from 0, run once for this lineage value
    /// and then remembered, refusals included; `None` when that step is not
    /// a migration. It enumerates the old version's whole input domain, so
    /// the connection must not be inside a transaction.
    pub(super) fn simulate(
        &self,
        connection: &Connection,
        step: usize,
    ) -> Option<Result<Simulated, Unsimulated>> {
        debug_assert!(
            connection.is_autocommit(),
            "a forward simulation must not run inside a transaction"
        );
        let Some(Step::Migration { migration, .. }) = self.steps.get(step) else {
            return None;
        };
        let mut memo = self
            .simulated
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(outcome) = memo.get(&step) {
            return Some(*outcome);
        }
        let outcome = migration::simulate(
            self.catalogs[step],
            &self.authorities[step],
            self.catalogs[step + 1],
            &self.authorities[step + 1],
            migration,
        );
        self.simulations.fetch_add(1, Ordering::Relaxed);
        memo.insert(step, outcome);
        Some(outcome)
    }

    /// The remembered simulation of the migration at step `step`, if this
    /// lineage ran it; never runs one or waits while SQLite is locked.
    pub(super) fn simulated(&self, step: usize) -> Option<Result<Simulated, Unsimulated>> {
        let memo = match self.simulated.try_lock() {
            Ok(memo) => memo,
            Err(TryLockError::WouldBlock) => return None,
            // Only complete simulation outcomes are inserted.
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        memo.get(&step).copied()
    }

    /// Establishes what an operation needs for the versions `from` to `to`
    /// before it takes the lock: the migration's simulation when the step
    /// between them is a migration, and otherwise the premises of a program
    /// succession.
    ///
    /// # Errors
    /// As [`Lineage::establish`].
    pub(super) fn prepare_pair(
        &self,
        connection: &Connection,
        from: usize,
        to: usize,
    ) -> Result<(), Error> {
        if to == from + 1 && self.simulate(connection, from).is_some() {
            return Ok(());
        }
        self.establish(connection, from, to).map(|_| ())
    }

    /// Whether the step from version `step` to the next moves the state to
    /// another schema: a migration or a rename.
    pub(super) fn moves_state(&self, step: usize) -> bool {
        matches!(
            self.steps.get(step),
            Some(Step::Migration { .. } | Step::Rename)
        )
    }

    /// The hops an upgrade from version `from` to version `to` records, in
    /// order: one for each migration or rename step, and one for each run of
    /// other steps between them. A span without either is one hop, exactly
    /// as before migrations existed.
    pub(super) fn hops(&self, from: usize, to: usize) -> Vec<(usize, usize)> {
        if from >= to {
            return vec![(from, to)];
        }
        let mut hops = Vec::new();
        let mut start = from;
        while start < to {
            let mut end = start + 1;
            if !self.moves_state(start) {
                while end < to && !self.moves_state(end) {
                    end += 1;
                }
            }
            hops.push((start, end));
            start = end;
        }
        hops
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

    /// The declared steps, oldest first: one between each version and the
    /// next.
    pub fn steps(&self) -> &[Step<'c>] {
        &self.steps
    }

    /// The adoption receipt digests between versions `from` and `to`,
    /// positions from 0, oldest first, when every step between them is an
    /// adoption; `None` otherwise, or for positions out of order or range.
    pub(super) fn adoption_receipts(&self, from: usize, to: usize) -> Option<&[Hash32]> {
        let steps = self.steps.get(from..to)?;
        steps
            .iter()
            .all(|step| matches!(step, Step::Adoption(_)))
            .then(|| self.receipts.get(from..to))
            .flatten()
    }

    /// The owner reviews of the behaviour changes between versions `from`
    /// and `to`, oldest first; empty when there is none.
    pub(super) fn reviews(&self, from: usize, to: usize) -> Vec<&'c [u8]> {
        self.steps
            .get(from..to)
            .unwrap_or_default()
            .iter()
            .filter_map(|step| match step {
                Step::BehaviourChange { review, .. } => Some(*review),
                Step::Adoption(_) | Step::Migration { .. } | Step::Rename => None,
            })
            .collect()
    }

    /// The claims declared for the version at `position`: those of the last
    /// behaviour change or migration that leads to it, or to a version it
    /// adopted from. Migration claims are compiled against the target schema;
    /// old-schema claim programs are never carried across it.
    pub(super) fn claims(&self, position: usize) -> &'c [Claim<'c>] {
        self.steps
            .get(..position)
            .unwrap_or_default()
            .iter()
            .rev()
            .find_map(|step| match step {
                Step::BehaviourChange { claims, .. } | Step::Migration { claims, .. } => {
                    Some(*claims)
                }
                Step::Rename => Some(&[][..]),
                Step::Adoption(_) => None,
            })
            .unwrap_or_default()
    }

    /// The number of versions; the last is the one the application runs.
    pub fn versions(&self) -> usize {
        self.authorities.len()
    }

    /// Every version's Authority, oldest first.
    pub fn authorities(&self) -> &[Authority<'p>] {
        &self.authorities
    }

    /// Every immutable checked catalog, oldest first, for interpreting reports
    /// under the schema that originally produced them. No new authority is issued.
    pub fn catalogs(&self) -> &[&'c BoundCatalog<'p>] {
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

    /// Audits the store at `path` without writing to it, whichever version
    /// of the lineage it runs: the file is opened read-only, every history
    /// segment replays under the version that published it in one read
    /// transaction, and no checkpoint is saved. Returns the version the store
    /// runs, from 1, and its audited head. A build can therefore check an
    /// older store before it upgrades it. A missing file is refused.
    ///
    /// # Errors
    /// As [`Lineage::open`] for a v10 store, and `Schema` for any other.
    pub fn audit_read_only(&self, path: impl AsRef<Path>) -> Result<(usize, Snapshot), Error> {
        let history = self.audited_history(path, false)?;
        Ok((history.contract_version, history.snapshot))
    }

    /// Replays the complete history in one read-only transaction and returns
    /// its original genesis and inputs. Each commit names its actual publishing
    /// version and pre-state after preceding migrations. Upgrades at the last
    /// head are included in the snapshot even when no subsequent commit exists.
    /// This reporting API does not mint publication or delivery capabilities.
    ///
    /// # Errors
    /// As [`Self::audit_read_only`]. No partial history is returned on refusal.
    pub fn audit_history_read_only(&self, path: impl AsRef<Path>) -> Result<AuditedHistory, Error> {
        self.audited_history(path, true)
    }

    fn audited_history(
        &self,
        path: impl AsRef<Path>,
        collect: bool,
    ) -> Result<AuditedHistory, Error> {
        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        provider()?;
        let members = self.members(self.versions());
        // Comparisons run before the read transaction, as for any audit.
        prepare(&connection, members, false);
        settle(&mut connection, members, |connection, missing| {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            check_schema(&tx)?;
            let authorities = members.authorities();
            let segments = Segments::load(&tx, authorities, load_upgrades(&tx)?)?;
            let (_, initial) = genesis_anchor(&tx, segments.authority(authorities, 0))?;
            let genesis = initial.state.clone();
            let mut commits = Vec::new();
            let anchor = audit_tail_with_inputs(
                &tx,
                authorities,
                members.declared(missing),
                &segments,
                initial,
                0,
                collect.then_some(&mut commits),
            )?;
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
            let snapshot = Snapshot {
                anchor,
                binding: segments.identity(segments.current()).to_vec(),
                bundles: u64::try_from(bundles).map_err(|_| Error::History)?,
                replays: u64::try_from(replays).map_err(|_| Error::History)?,
                pending: u64::try_from(pending).map_err(|_| Error::History)?,
                upgrades: u64::try_from(segments.upgrades.len()).map_err(|_| Error::History)?,
            };
            let position = segments.position(segments.current());
            tx.commit()?;
            Ok(AuditedHistory {
                contract_version: position + 1,
                snapshot,
                genesis_contract_version: segments.position(0) + 1,
                genesis,
                commits,
            })
        })
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
    /// After a complete audit under the whole lineage: when the lineage
    /// declares a behaviour change between the version the store runs and
    /// the last one, the last version's state laws and declared claims must
    /// hold on the current state (see [`behaviour`](crate::v2::behaviour)),
    /// with no program comparison and without its genesis exactness law.
    /// Otherwise the shell tries to
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
    /// refusal names the missing premise, and a behaviour-change refusal the
    /// law or claim that does not hold.
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
        // Plain reads first: every comparison and simulation this upgrade
        // needs runs before the transaction takes the lock.
        prepare(&connection, members, true);
        let (anchor, genesis, data_version, receipt) =
            settle(&mut connection, members, |connection, missing| {
                upgrade_once(connection, lineage, crash, missing)
            })?;
        let shell = V2SqliteShell {
            members,
            connection,
            anchor,
            genesis,
            delivery_interpreter,
            data_version,
        };
        inject(crash, CrashPoint::AfterCommit)?;
        Ok((shell, receipt))
    }
}

/// One decided hop of an upgrade: its plan, the bytes its row stores in the
/// publication column, the receipt digests it binds, the head before it and
/// the head after it.
struct Hop {
    ordinal: u64,
    plan: upgrade::Plan,
    admitted: Vec<u8>,
    receipts: Vec<Hash32>,
    before: Anchor,
    after: Anchor,
    from_identity: Vec<u8>,
    identity: Vec<u8>,
}

/// One attempt at the upgrade, in one immediate transaction. A comparison or
/// simulation it needs that is not yet established ends it with
/// `Error::Unsettled` and the pair in `missing`, having written nothing.
/// Every hop is decided before anything is written. Returns the head after
/// the last hop.
fn upgrade_once(
    connection: &mut Connection,
    lineage: &Lineage<'_, '_>,
    crash: Option<CrashPoint>,
    missing: &Cell<Option<(usize, usize)>>,
) -> Result<(Anchor, Hash32, i64, UpgradeReceipt), Error> {
    inject(crash, CrashPoint::BeforeTransaction)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_schema(&tx)?;
    let authorities = lineage.authorities();
    let segments = Segments::load(&tx, authorities, load_upgrades(&tx)?)?;
    let (genesis, initial) = genesis_anchor(&tx, segments.authority(authorities, 0))?;
    let declared = lineage.members(lineage.versions()).declared(missing);
    let anchor = audit_tail(&tx, authorities, declared, &segments, initial, 0)?;
    let (current, last) = (
        segments.position(segments.current()),
        lineage.versions() - 1,
    );
    let mut ordinal = u64::try_from(segments.upgrades.len())
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or(Error::Range)?;
    let mut head = anchor.clone();
    let mut hops = Vec::new();
    for (from, to) in lineage.hops(current, last) {
        let hop = decide_hop(lineage, &head, ordinal, from, to, missing)?;
        head = hop.after.clone();
        hops.push(hop);
        ordinal = ordinal.checked_add(1).ok_or(Error::Range)?;
    }
    let (data_version, receipt) = record(tx, crash, &anchor, &head, &hops)?;
    Ok((head, genesis, data_version, receipt))
}

/// Decides the hop from version `from` to version `to` at `head`, the
/// store's head after every earlier hop of this upgrade.
fn decide_hop(
    lineage: &Lineage<'_, '_>,
    head: &Anchor,
    ordinal: u64,
    from: usize,
    to: usize,
    missing: &Cell<Option<(usize, usize)>>,
) -> Result<Hop, Error> {
    let (authorities, catalogs) = (lineage.authorities(), lineage.catalogs());
    let (old, new) = (&authorities[from], &authorities[to]);
    let sequence = u64::try_from(head.sequence).map_err(|_| Error::Range)?;
    let facts = |admission| Facts {
        ordinal,
        sequence,
        root: head.root,
        previous_chain: head.chain,
        from_identity: old.identity(),
        from_schema: catalogs[from].original_schema(),
        identity: new.identity(),
        schema: catalogs[to].original_schema(),
        state: &head.state,
        admission,
    };
    let hop = |plan: upgrade::Plan, admitted, receipts, state: Option<Vec<u8>>| {
        let after = match state {
            Some(state) => Anchor {
                sequence: head.sequence,
                root: hash(zeno_fcis_codec::domains::V2_STATE, &state)?,
                state,
                chain: plan.chain(),
            },
            None => Anchor {
                chain: plan.chain(),
                ..head.clone()
            },
        };
        Ok::<_, Error>(Hop {
            ordinal,
            plan,
            admitted,
            receipts,
            before: head.clone(),
            after,
            from_identity: old.identity().to_vec(),
            identity: new.identity().to_vec(),
        })
    };
    // Another connection may have upgraded the store since it was
    // opened; the decision then refuses the same contract first.
    let same = old.identity() == new.identity();
    if !same && to == from + 1 && lineage.moves_state(from) {
        // A migration or rename moves the state; nothing else is compared.
        let mut migrated = None;
        let (description, moved) = match lineage.steps()[from] {
            Step::Migration {
                migration, review, ..
            } => {
                let Some(simulated) = lineage.simulated(from) else {
                    missing.set(Some((from, to)));
                    return Err(Error::Unsettled);
                };
                let encoding = migration.encode().map_err(|_| Error::Range)?;
                let moved = simulated.and_then(|simulated| {
                    migration::migrate_state(catalogs[from], catalogs[to], &migration, &head.state)
                        .map(|state| (simulated, state))
                });
                match moved {
                    Ok((simulated, state)) => {
                        let root = hash(zeno_fcis_codec::domains::V2_STATE, &state)?;
                        let checked = behaviour::check(catalogs[to], lineage.claims(to), &state)
                            .map_err(|unmet| {
                                Error::Upgrade(upgrade::Refusal::MigrationState(unmet))
                            })?;
                        migrated = Some(Migrated::new(
                            sha256(&encoding),
                            simulated,
                            root,
                            checked,
                            sha256(review),
                        ));
                        (encoding, Ok(state))
                    }
                    Err(unsimulated) => (encoding, Err(Some(unsimulated))),
                }
            }
            _ => {
                let exact = migration::rename_exact(catalogs[from], catalogs[to])
                    .map_err(|_| Error::Range)?;
                let state = exact
                    .then(|| migration::reframe(catalogs[from], catalogs[to], &head.state))
                    .flatten();
                (Vec::new(), state.ok_or(None))
            }
        };
        let admission = match (&migrated, &moved) {
            (Some(migrated), _) => Admission::Migration(migrated),
            (None, Err(Some(unsimulated))) => Admission::Unsimulated(*unsimulated),
            (None, Ok(state)) => {
                Admission::Rename(hash(zeno_fcis_codec::domains::V2_STATE, state)?)
            }
            (None, Err(None)) => Admission::NotRenamed,
        };
        let plan = upgrade::decide(&facts(admission))?;
        // The decision refused every admission without a moved state.
        let state = moved.map_err(|_| Error::Range)?;
        let admitted = upgrade::framed_move(&description, &state)?;
        return hop(plan, admitted, Vec::new(), Some(state));
    }
    // A declared behaviour change is admitted by the new version's laws on
    // the state alone: no comparison is looked up, and none is needed.
    let reviews = lineage.reviews(from, to);
    if !same && !reviews.is_empty() {
        let digests = reviews.iter().map(|text| sha256(text)).collect();
        let checked = behaviour::check(catalogs[to], lineage.claims(to), &head.state)
            .map(|checked| Behaviour::new(checked, digests));
        let admission = match &checked {
            Ok(behaviour) => Admission::Behaviour(behaviour),
            Err(unmet) => Admission::Unmet(*unmet),
        };
        let plan = upgrade::decide(&facts(admission))?;
        let admitted = upgrade::framed_reviews(&reviews)?;
        return hop(plan, admitted, Vec::new(), None);
    }
    // The same contract establishes nothing: the decision refuses it first.
    let established = if same {
        Err(Premise::Policy)
    } else {
        let Some(settled) = lineage.settled(from, to) else {
            missing.set(Some((from, to)));
            return Err(Error::Unsettled);
        };
        let receipts = lineage.adoption_receipts(from, to).ok_or(Error::Lineage)?;
        settled?.map(|premises| Successor::held(receipts, premises))
    };
    let successor = established.ok();
    let genesis_outcome = (!same && successor.is_none()).then(|| new.publish_genesis(&head.state));
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
    let plan = upgrade::decide(&facts(admission))?;
    let admitted = match (plan.kind(), &genesis_outcome) {
        (Kind::ProgramSuccessor, _) => plan.evidence().to_vec(),
        (Kind::GenesisAdmission, Some(PublicationOutcome::Commit(publication))) => {
            compact_publication(new.identity(), publication.subject())?
        }
        _ => {
            return Err(Error::Upgrade(upgrade::Refusal::Genesis {
                missing: Premise::Policy,
                refusal: None,
            }));
        }
    };
    let receipts = successor.map_or_else(Vec::new, |successor| successor.receipts().to_vec());
    hop(plan, admitted, receipts, None)
}

/// Writes the decided hops inside their transaction and commits: the head's
/// chain tip, and its state and root when a hop moved the state, then one
/// record row per hop with its admission in the publication column. The
/// receipt describes the last hop.
fn record(
    tx: rusqlite::Transaction<'_>,
    crash: Option<CrashPoint>,
    anchor: &Anchor,
    head: &Anchor,
    hops: &[Hop],
) -> Result<(i64, UpgradeReceipt), Error> {
    inject(crash, CrashPoint::AfterValidation)?;
    // The head keeps its sequence; only a migration or rename moves its
    // state and root, and every hop moves the chain tip.
    let changed = tx.execute(
        "UPDATE v2_state SET state=?1,root=?2,chain=?3 WHERE singleton=1 AND sequence=?4 AND root=?5 AND chain=?6",
        params![
            head.state,
            head.root.as_bytes().as_slice(),
            head.chain.as_bytes().as_slice(),
            anchor.sequence,
            anchor.root.as_bytes().as_slice(),
            anchor.chain.as_bytes().as_slice()
        ],
    )?;
    if changed != 1 {
        return Err(Error::Concurrent);
    }
    inject(crash, CrashPoint::AfterStateWrite)?;
    for hop in hops {
        tx.execute(
            "INSERT INTO v2_upgrades VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                i64::try_from(hop.ordinal).map_err(|_| Error::Range)?,
                hop.before.sequence,
                hop.identity,
                hop.admitted,
                hop.before.root.as_bytes().as_slice(),
                hop.before.chain.as_bytes().as_slice(),
                hop.plan.record(),
                hop.plan.chain().as_bytes().as_slice()
            ],
        )?;
    }
    inject(crash, CrashPoint::AfterReplayWrite)?;
    inject(crash, CrashPoint::BeforeCommit)?;
    let data_version = data_version(&tx)?;
    tx.commit()?;
    let hop = hops.last().ok_or(Error::Lineage)?;
    let receipt = UpgradeReceipt {
        kind: hop.plan.kind(),
        premises: hop.plan.premises(),
        ordinal: hop.ordinal,
        sequence: u64::try_from(hop.before.sequence).map_err(|_| Error::Range)?,
        chain: hop.plan.chain(),
        from_identity: hop.from_identity.clone(),
        identity: hop.identity.clone(),
        receipts: hop.receipts.clone(),
        behaviour: hop.plan.behaviour().cloned(),
        migration: hop.plan.migration().cloned(),
        record: hop.plan.record().to_vec(),
    };
    Ok((data_version, receipt))
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
