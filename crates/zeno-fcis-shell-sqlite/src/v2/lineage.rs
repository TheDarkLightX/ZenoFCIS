//! A store opened under an application's contract lineage, as typed handles.
//!
//! What a store holds decides what can be done with it. A schema v9 store can
//! only be migrated. A v10 store that runs an earlier version of the lineage
//! can be audited and read, or upgraded to the last version. Only a store
//! that runs the last version commits and delivers. Each transition consumes
//! its handle and returns the next one, all its checks and writes happen in
//! one immediate transaction, and every refusal writes nothing.

use std::path::Path;

use rusqlite::{Connection, TransactionBehavior, params};
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::MemoryDestination;
use zeno_fcis_synthesis::finite::{
    v2_authority::{self as authority, Authority, PublicationOutcome},
    v2_catalog::BoundCatalog,
};

use super::upgrade::{self, Admission, Facts, Kind, Successor};
use super::{
    Anchor, Checkpoint, CrashPoint, Error, Members, Segments, Snapshot, UpgradeReceipt,
    V2SqliteShell, audit_tail, check_schema, check_schema_v9, compact_publication, configure,
    data_version, genesis_anchor, inject, load_upgrades, migration_v9_to_v10, open_existing,
    provider,
};

/// An application's contract lineage, oldest version first and the version
/// it runs now last: each version's checked catalog and the library Authority
/// bound from it, and between each version and the next the SHA-256 of the
/// `transform` receipt that adopted the later one.
///
/// The shell replays no receipt. A generated contract's `with_lineage` passes
/// the digests of receipts that `zeno-fcis generate contract` replayed, and a
/// program-successor upgrade binds the ones it spans.
pub struct Lineage<'c, 'p> {
    catalogs: Vec<&'c BoundCatalog<'p>>,
    authorities: Vec<Authority<'p>>,
    receipts: Vec<Hash32>,
}

impl<'c, 'p> Lineage<'c, 'p> {
    /// Binds every version's catalog into its Authority through the library.
    ///
    /// # Errors
    /// `Error::Lineage` without a version, or unless there is exactly one
    /// receipt between each version and the next; `Error::Authority` when a
    /// catalog does not bind.
    pub fn bind(catalogs: &[&'c BoundCatalog<'p>], receipts: &[[u8; 32]]) -> Result<Self, Error> {
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
        })
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
    /// After a complete audit under the whole lineage, the version the store
    /// runs and the last one are compared: a program successor
    /// ([`upgrade::program_successor`]) is admitted at any state and binds
    /// the receipts of the adoptions between them; any other contract must
    /// admit the current state through its genesis evaluation. The pure
    /// [`upgrade::decide`] then requires equal canonical state schemas and
    /// different identities. The record becomes the next chain link, and
    /// pending deliveries keep their IDs and order.
    ///
    /// # Errors
    /// `Schema` for a store that is no longer v10, `Identity` for a segment
    /// under no version, `History` for a failed audit, `Concurrent` when the
    /// head moved, and `Upgrade` for the decision's refusals.
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
        inject(crash, CrashPoint::BeforeTransaction)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_schema(&tx)?;
        let (authorities, catalogs) = (lineage.authorities(), lineage.catalogs());
        let segments = Segments::load(&tx, authorities, load_upgrades(&tx)?)?;
        let (genesis, initial) = genesis_anchor(&tx, segments.authority(authorities, 0))?;
        let anchor = audit_tail(&tx, authorities, catalogs, &segments, initial, 0)?;
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
        let successor = if same {
            None
        } else {
            Successor::establish(
                catalogs[current],
                catalogs[last],
                &lineage.receipts[current..last],
            )?
        };
        let genesis_outcome =
            (!same && successor.is_none()).then(|| to.publish_genesis(&anchor.state));
        let admission = match (successor, &genesis_outcome) {
            (Some(successor), _) => Admission::Successor(successor),
            (None, Some(PublicationOutcome::Commit(publication))) => {
                Admission::Genesis(upgrade::Genesis {
                    subject: publication.subject(),
                    poststate: publication.poststate(),
                })
            }
            (None, Some(PublicationOutcome::Refused { error, .. })) => {
                Admission::Refused(Some(*error))
            }
            (None, _) => Admission::Refused(None),
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
            _ => return Err(Error::Upgrade(upgrade::Refusal::Genesis(None))),
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
        let shell = V2SqliteShell {
            members: lineage.members(lineage.versions()),
            connection,
            anchor: Anchor {
                chain: plan.chain(),
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
        let anchor = audit_tail(&tx, authorities, members.catalogs(), &segments, initial, 0)?;
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
