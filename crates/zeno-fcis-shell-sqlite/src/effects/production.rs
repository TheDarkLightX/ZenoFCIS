//! The production handler: the shell-effects ports mounted on the V2 SQLite
//! shell. Every port calls the existing V2 API the registry names, so the
//! SQL, its order, the guards and the canonical bytes are unchanged:
//!
//! - ReadState is `snapshot`; Reopen is `open`, `audit` and `snapshot`;
//! - Commit rebuilds the publication through Authority, then calls
//!   `commit`, `commit_at` or `commit_bounded_at` by its mode;
//! - ReadHistory is `Lineage::audit_history_read_only`, with each commit's
//!   publication data rebuilt through the Authority that published it;
//! - ReadOutbox is `export_pending`; Acknowledge is the relay's
//!   `pending_by_id`, `relayed` and `acknowledge`;
//! - Send goes to the caller's [`Transport`]; Clock, Random and Wait go to an
//!   opt-in [`Time`], and without one those ports are not covered.
//!
//! A V2 refusal keeps its code. SQLite busy or full is NotApplied: the
//! transaction rolled back. Any other SQLite failure of a commit or an
//! acknowledgement is Unknown; of a read, NotApplied.

use std::path::{Path, PathBuf};

use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::MemoryDestination;
use zeno_fcis_shell::effects::driver::Binding;
use zeno_fcis_shell::effects::handler::{Handled, Terminal};
use zeno_fcis_shell::effects::port::{
    AuditedHistory, Code, CommitMode, CommitReceipt, Delivery, EpochTick, Fault, HistoryEntry,
    Operation, Outcome, Response, Sent, Snapshot, Store, WaitError,
};
use zeno_fcis_shell::effects::registry::Port;
use zeno_fcis_synthesis::finite::v2_authority::Authority;

use super::{Unbound, audited_publication, bind_publication, store_identity};
use crate::v2::{self, Error, Lineage, V2SqliteShell, relay};

/// Source/configuration/seed of a qualified caller adapter. This is provenance
/// data, not an attestation or authority to publish or acknowledge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterProvenance {
    /// Actual implementation source digest.
    pub source: Hash32,
    /// Initial configuration and future-relevant retained state.
    pub configuration: Vec<u8>,
    /// Explicit PRNG seed, or zero for an adapter without randomness.
    pub seed: u64,
}

/// A transport for Send: an observation, never a proof of receipt.
pub trait Transport {
    /// Non-effecting provenance snapshot; None keeps recording unqualified.
    fn provenance(&self) -> Option<AdapterProvenance> {
        None
    }

    /// Sends one delivery's exact bytes.
    fn send(&mut self, delivery: &Delivery) -> Sent;
}

/// The opt-in time adapter for Clock, Random and Wait.
pub trait Time {
    /// Non-effecting provenance snapshot; None keeps recording unqualified.
    fn provenance(&self) -> Option<AdapterProvenance> {
        None
    }

    /// A monotonic tick of `epoch`.
    fn clock(&mut self, epoch: u64) -> Result<EpochTick, Fault>;
    /// A draw below `bound`, for retry jitter only.
    fn random(&mut self, bound: u64) -> Result<u64, Fault>;
    /// Yields until `deadline`.
    fn wait(&mut self, deadline: EpochTick) -> Result<EpochTick, WaitError>;
}

/// No time adapter: Clock, Random and Wait stay uncovered.
#[derive(Debug, Default)]
pub struct NoTime;

impl Time for NoTime {
    fn clock(&mut self, _: u64) -> Result<EpochTick, Fault> {
        Err(Fault::Io)
    }
    fn random(&mut self, _: u64) -> Result<u64, Fault> {
        Err(Fault::Io)
    }
    fn wait(&mut self, _: EpochTick) -> Result<EpochTick, WaitError> {
        Err(WaitError::Failed(Fault::Io))
    }
}

const STORE_AND_SEND: &[Port] = &[
    Port::ReadState,
    Port::Commit,
    Port::ReadHistory,
    Port::ReadOutbox,
    Port::Send,
    Port::Acknowledge,
    Port::Reopen,
];

/// The production terminal for one handle on one store.
pub struct Production<'a, 'p, T: Transport, C: Time = NoTime> {
    path: PathBuf,
    lineage: &'a Lineage<'a, 'p>,
    store: Store,
    shell: Option<V2SqliteShell<'a, 'p>>,
    transport: T,
    time: Option<C>,
    exclusive: Option<std::fs::File>,
    invalidated: bool,
    #[cfg(feature = "crash-injection")]
    interrupt: Option<crate::CrashPoint>,
    #[cfg(feature = "crash-injection")]
    abort: bool,
}

impl<'a, 'p, T: Transport, C: Time> Production<'a, 'p, T, C> {
    /// Opens an existing store through a single-version lineage as handle
    /// `handle`. The store's identity is computed from its audited genesis,
    /// the Authority and the delivery interpreter.
    pub fn open(
        path: impl AsRef<Path>,
        lineage: &'a Lineage<'a, 'p>,
        handle: u32,
        transport: T,
        time: Option<C>,
    ) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        let authority = current(lineage)?;
        let history = lineage.audit_history_read_only(&path)?;
        let interpreter = MemoryDestination::interpreter_identity()?;
        let identity = store_identity(&history.genesis, authority.identity(), interpreter);
        let shell = V2SqliteShell::open(&path, authority)?;
        if shell.effects_identity()? != identity {
            return Err(Error::Identity);
        }
        Ok(Self {
            path,
            lineage,
            store: Store { identity, handle },
            shell: Some(shell),
            transport,
            time,
            exclusive: None,
            invalidated: false,
            #[cfg(feature = "crash-injection")]
            interrupt: None,
            #[cfg(feature = "crash-injection")]
            abort: false,
        })
    }

    /// Open a server-local store under an exclusive advisory file lock. Every
    /// operation checks for another SQLite connection's writes and refuses
    /// until a full audit/restart; it never adopts an external tail silently.
    /// Other writers must respect the lock; physical file tampering is outside
    /// SQLite's change-counter guarantee.
    pub fn open_exclusive(
        path: impl AsRef<Path>,
        lineage: &'a Lineage<'a, 'p>,
        handle: u32,
        transport: T,
        time: Option<C>,
    ) -> Result<Self, Error> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.as_ref())
            .map_err(|_| Error::History)?;
        file.try_lock().map_err(|_| Error::Concurrent)?;
        let mut terminal = Self::open(path, lineage, handle, transport, time)?;
        terminal.shell()?.effects_exclusive();
        terminal.exclusive = Some(file);
        Ok(terminal)
    }

    /// Binds the terminal and both caller adapters before private recording.
    /// Unqualified adapters may execute, but cannot yield qualified evidence.
    pub fn manifest(&self, start: Vec<u8>) -> Option<zeno_fcis_shell::effects::record::Manifest> {
        use zeno_fcis_shell::effects::{
            codec,
            record::{Implementation, Manifest},
        };
        let transport = self.transport.provenance()?;
        let time = self.time.as_ref()?.provenance()?;
        if transport.source == Hash32::ZERO || time.source == Hash32::ZERO {
            return None;
        }
        let mut manifest = Manifest::current(
            vec!["record", "production"],
            vec![],
            self.store.identity,
            start,
        );
        manifest.implementations.extend([
            Implementation {
                name: "production",
                source: implementation_source(),
            },
            Implementation {
                name: "transport",
                source: transport.source,
            },
            Implementation {
                name: "time",
                source: time.source,
            },
        ]);
        manifest.configuration = self.shell.as_ref()?.effects_configuration().ok()?;
        manifest
            .configuration
            .extend_from_slice(&self.store.handle.to_be_bytes());
        manifest.configuration.extend_from_slice(&[
            u8::from(self.exclusive.is_some()),
            u8::from(self.invalidated),
        ]);
        for adapter in [transport, time] {
            codec::bytes(&mut manifest.configuration, &adapter.configuration);
            manifest
                .configuration
                .extend_from_slice(&adapter.seed.to_be_bytes());
        }
        #[cfg(feature = "crash-injection")]
        codec::bytes(
            &mut manifest.configuration,
            format!("{:?}", self.interrupt).as_bytes(),
        );
        Some(manifest)
    }

    /// The handle's store: its identity and handle number.
    #[must_use]
    pub const fn store(&self) -> Store {
        self.store
    }

    /// The transport.
    pub fn transport(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Interrupts the next commit at `point`, with the transaction rolled
    /// back in-process: a crash at a statement boundary.
    #[cfg(feature = "crash-injection")]
    pub fn interrupt_next_commit(&mut self, point: crate::CrashPoint) {
        self.interrupt = Some(point);
    }

    /// Abort, without destructors, at the next commit's selected boundary.
    /// Test-only, per-handle and consumed once; supplies no decision authority.
    #[cfg(feature = "crash-injection")]
    pub fn abort_next_commit_at(&mut self, point: crate::CrashPoint) {
        self.interrupt = Some(point);
        self.abort = true;
    }

    /// Sets the handle's SQLite busy timeout, for busy tests.
    #[cfg(feature = "crash-injection")]
    pub fn set_test_busy_timeout(&mut self, timeout: std::time::Duration) -> Result<(), Error> {
        self.shell()?.set_test_busy_timeout(timeout)
    }

    fn shell(&mut self) -> Result<&mut V2SqliteShell<'a, 'p>, Error> {
        if self.shell.is_none() {
            self.shell = Some(self.open_bound()?);
        }
        match self.shell.as_mut() {
            Some(shell) => Ok(shell),
            None => Err(Error::Identity),
        }
    }

    fn commit(
        &mut self,
        key: Hash32,
        data: &zeno_fcis_shell::effects::port::PublicationData,
        mode: CommitMode,
    ) -> Handled {
        let authority = match current(self.lineage) {
            Ok(authority) => authority,
            Err(error) => return answer(Port::Commit, Err(error), true),
        };
        let publication = match bind_publication(authority, data) {
            Ok(publication) => publication,
            Err(Unbound::Identity | Unbound::Subject) => {
                return Handled::Answer(Response::refused(Port::Commit, Code::Identity));
            }
            Err(Unbound::NotCommitting) => {
                return Handled::Answer(Response::refused(Port::Commit, Code::InvocationKind));
            }
        };
        #[cfg(feature = "crash-injection")]
        let interrupt = self.interrupt.take();
        #[cfg(feature = "crash-injection")]
        let abort = std::mem::take(&mut self.abort);
        let shell = match self.shell() {
            Ok(shell) => shell,
            Err(error) => return answer(Port::Commit, Err(error), true),
        };
        #[cfg(feature = "crash-injection")]
        let result = if abort {
            shell.commit_effects_abort(mode, key, publication, interrupt)
        } else {
            match mode {
                CommitMode::Unversioned => shell.commit_with_crash(key, publication, interrupt),
                CommitMode::At(version) => {
                    shell.commit_with_crash_at(version, key, publication, interrupt)
                }
                CommitMode::BoundedAt(version, cap) => {
                    shell.commit_bounded_with_crash_at(version, key, publication, cap, interrupt)
                }
            }
        };
        #[cfg(not(feature = "crash-injection"))]
        let result = match mode {
            CommitMode::Unversioned => shell.commit(key, publication),
            CommitMode::At(version) => shell.commit_at(version, key, publication),
            CommitMode::BoundedAt(version, cap) => {
                shell.commit_bounded_at(version, key, publication, cap)
            }
        };
        let receipt = result.map(|receipt| {
            Response::Commit(Outcome::Ok(CommitReceipt {
                version: receipt.version(),
                chain: receipt.chain(),
                status: receipt.status(),
            }))
        });
        self.settle(Port::Commit, receipt, true)
    }

    /// A crash injected inside V2 drops the handle and gives no answer.
    fn settle(&mut self, port: Port, result: Result<Response, Error>, writes: bool) -> Handled {
        #[cfg(feature = "crash-injection")]
        if let Err(Error::InjectedCrash(_)) = result {
            self.discard_shell();
            return Handled::Crash;
        }
        answer(port, result, writes)
    }

    fn history(&mut self) -> Result<AuditedHistory, Error> {
        let audited = self.lineage.audit_history_read_only(&self.path)?;
        let identity = store_identity(
            &audited.genesis,
            current(self.lineage)?.identity(),
            MemoryDestination::interpreter_identity()?,
        );
        if identity != self.store.identity {
            return Err(Error::Identity);
        }
        let authorities = self.lineage.authorities();
        let mut commits = Vec::with_capacity(audited.commits.len());
        for commit in &audited.commits {
            let Some(authority) = authorities.get(commit.contract_version.wrapping_sub(1)) else {
                return Err(Error::History);
            };
            let publication = audited_publication(
                authority,
                &commit.prestate,
                &commit.command,
                &commit.context,
            )
            .map_err(|_| Error::History)?;
            commits.push(HistoryEntry {
                key: commit.replay_id,
                version: commit.sequence,
                publication,
            });
        }
        Ok(AuditedHistory {
            head: snapshot(&audited.snapshot),
            commits,
        })
    }

    fn open_bound(&self) -> Result<V2SqliteShell<'a, 'p>, Error> {
        let shell = V2SqliteShell::open(&self.path, current(self.lineage)?)?;
        if shell.effects_identity()? != self.store.identity {
            return Err(Error::Identity);
        }
        let mut shell = shell;
        if self.exclusive.is_some() {
            shell.effects_exclusive();
        }
        Ok(shell)
    }

    /// Test projection including adapter state, connection/cache state and
    /// pending injection. Transport Debug must cover its volatile state;
    /// the qualified explorer separately snapshots its receiver database.
    #[cfg(feature = "crash-injection")]
    pub fn test_execution_state(&self) -> Result<Vec<u8>, Error>
    where
        T: core::fmt::Debug,
        C: core::fmt::Debug,
    {
        let mut bytes = format!(
            "{:?}|{:?}|{:?}|{:?}|{}|{}",
            self.store,
            self.time,
            self.transport,
            self.interrupt,
            self.exclusive.is_some(),
            self.invalidated
        )
        .into_bytes();
        match &self.shell {
            Some(shell) => {
                bytes.push(1);
                bytes.extend_from_slice(&shell.effects_configuration()?);
            }
            None => bytes.push(0),
        }
        Ok(bytes)
    }

    fn external_change(&self) -> bool {
        if self.exclusive.is_none() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let same_file = self
                .exclusive
                .as_ref()
                .and_then(|file| file.metadata().ok())
                .zip(std::fs::metadata(&self.path).ok())
                .is_some_and(|(held, path)| held.dev() == path.dev() && held.ino() == path.ino());
            if !same_file {
                return true;
            }
        }
        self.shell
            .as_ref()
            .is_some_and(|shell| shell.effects_unchanged().is_err())
    }

    fn discard_shell(&mut self) {
        // The exclusive server retains the old connection solely as a change
        // monitor across recovery. A reopened handle still audits independently.
        // Dropping that monitor would hide external writes during the gap.
        if self.exclusive.is_none() {
            self.shell = None;
        }
    }

    fn reopen(&mut self) -> Result<Snapshot, Error> {
        let mut shell = self.open_bound()?;
        shell.audit()?;
        let head = snapshot(&shell.snapshot()?);
        if self.external_change() {
            self.invalidated = true;
            return Err(Error::History);
        }
        self.shell = Some(shell);
        Ok(head)
    }
}

impl<T: Transport, C: Time> Terminal for Production<'_, '_, T, C> {
    fn ports(&self) -> &'static [Port] {
        if self.time.is_some() {
            &Port::ALL
        } else {
            STORE_AND_SEND
        }
    }

    fn handle(&mut self, binding: &Binding, operation: &Operation) -> Handled {
        let port = operation.port();
        if !self.ports().contains(&port)
            || operation.store().is_some_and(|store| store != self.store)
        {
            return Handled::Answer(Response::refused(port, Code::Identity));
        }
        if self.invalidated {
            return Handled::Answer(Response::refused(port, Code::History));
        }
        if self.external_change() {
            self.invalidated = true;
            return Handled::Answer(Response::refused(port, Code::History));
        }
        match operation {
            Operation::ReadState(_) => {
                let result = self.shell().and_then(|shell| shell.snapshot());
                let result = result.map(|head| Response::ReadState(Outcome::Ok(snapshot(&head))));
                answer(port, result, false)
            }
            Operation::Commit(_, key, data, mode) => self.commit(*key, data, *mode),
            Operation::ReadHistory(_) => {
                let result = self
                    .history()
                    .map(|history| Response::ReadHistory(Outcome::Ok(history)));
                answer(port, result, false)
            }
            Operation::ReadOutbox(_) => {
                let result = self.shell().and_then(|shell| shell.export_pending());
                let result = result.map(|pending| {
                    Response::ReadOutbox(Outcome::Ok(pending.iter().map(delivery).collect()))
                });
                answer(port, result, false)
            }
            Operation::Send(_, delivery) => {
                Handled::Answer(Response::Send(self.transport.send(delivery)))
            }
            Operation::Acknowledge(_, id, payload_sha256) => {
                let result = self
                    .shell()
                    .and_then(|shell| relay::acknowledge(shell, *id, *payload_sha256))
                    .map(|()| Response::Acknowledge(Outcome::Ok(())));
                answer(port, result, true)
            }
            Operation::Reopen(_) => {
                let result = self
                    .reopen()
                    .map(|head| Response::Reopen(Outcome::Ok(head)));
                answer(port, result, false)
            }
            Operation::Clock => match &mut self.time {
                Some(time) => Handled::Answer(Response::Clock(time.clock(binding.id.epoch))),
                None => Handled::Answer(Response::refused(port, Code::Identity)),
            },
            Operation::Random(_, bound) => match &mut self.time {
                Some(time) => Handled::Answer(Response::Random(time.random(bound.get()))),
                None => Handled::Answer(Response::refused(port, Code::Identity)),
            },
            Operation::Wait(deadline) => match &mut self.time {
                Some(time) => Handled::Answer(Response::Wait(time.wait(*deadline))),
                None => Handled::Answer(Response::refused(port, Code::Identity)),
            },
        }
    }

    fn crash(&mut self, _: &Binding) {
        self.discard_shell();
    }
}

fn current<'a, 'p>(lineage: &'a Lineage<'a, 'p>) -> Result<&'a Authority<'p>, Error> {
    lineage.authorities().last().ok_or(Error::Lineage)
}

fn snapshot(head: &v2::Snapshot) -> Snapshot {
    Snapshot {
        version: head.version(),
        identity: head.binding().to_vec(),
        state: head.state().to_vec(),
        root: head.root(),
        chain: head.chain(),
    }
}

fn delivery(record: &v2::Delivery) -> Delivery {
    Delivery {
        id: record.delivery_id(),
        version: record.version(),
        lane: record.lane(),
        ordinal: record.ordinal(),
        destination: record.destination().to_vec(),
        payload: record.payload().to_vec(),
        payload_sha256: relay::payload_sha256(record.payload()),
    }
}

/// Maps a V2 result onto the port's outcome, keeping V2's refusal codes.
fn answer(port: Port, result: Result<Response, Error>, writes: bool) -> Handled {
    let error = match result {
        Ok(response) => return Handled::Answer(response),
        Err(error) => error,
    };
    let code = match error {
        Error::Sqlite(rusqlite::Error::SqliteFailure(failure, _)) => {
            return Handled::Answer(match failure.code {
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                    Response::not_applied(port, Fault::Busy)
                }
                rusqlite::ErrorCode::DiskFull => Response::not_applied(port, Fault::Full),
                _ if writes => Response::unknown(port),
                _ => Response::not_applied(port, Fault::Io),
            });
        }
        Error::Sqlite(_) if writes => return Handled::Answer(Response::unknown(port)),
        Error::Sqlite(_) => return Handled::Answer(Response::not_applied(port, Fault::Io)),
        Error::Encoding(_) | Error::Range => Code::Range,
        Error::Provider => Code::Provider,
        Error::Schema(_) => Code::Schema,
        Error::Identity => Code::Identity,
        Error::Authority => Code::Authority,
        Error::Lineage => Code::Lineage,
        Error::Checkpoint => Code::Checkpoint,
        Error::InvocationKind => Code::InvocationKind,
        Error::History => Code::History,
        Error::PreState => Code::PreState,
        Error::Replay => Code::Replay,
        Error::Concurrent => Code::Concurrent,
        Error::Delivery => Code::Delivery,
        Error::Interpreter => Code::Interpreter,
        Error::UnknownDelivery => Code::UnknownDelivery,
        Error::AlreadyAcknowledged => Code::AlreadyAcknowledged,
        Error::PayloadMismatch => Code::PayloadMismatch,
        Error::Capacity { .. } => Code::Capacity,
        Error::Upgrade(_) => Code::Upgrade,
        Error::Succession(_) => Code::Succession,
        Error::Unsettled => Code::Unsettled,
        #[cfg(any(test, feature = "crash-injection"))]
        Error::InjectedCrash(_) => return Handled::Crash,
    };
    Handled::Answer(Response::refused(port, code))
}

/// Actual source closure of the qualified SQLite terminal, excluding tests.
#[must_use]
pub fn implementation_source() -> Hash32 {
    zeno_fcis_shell::effects::registry::source_digest(&[
        ("effects/production.rs", include_bytes!("production.rs")),
        ("lib.rs", include_bytes!("../lib.rs")),
        (
            "shell/lib.rs",
            include_bytes!("../../../zeno-fcis-shell/src/lib.rs"),
        ),
        ("effects.rs", include_bytes!("../effects.rs")),
        ("v2.rs", include_bytes!("../v2.rs")),
        ("v2/behaviour.rs", include_bytes!("../v2/behaviour.rs")),
        ("v2/compact.rs", include_bytes!("../v2/compact.rs")),
        ("v2/delivery.rs", include_bytes!("../v2/delivery.rs")),
        (
            "v2/equivalence/checker.rs",
            include_bytes!("../v2/equivalence/checker.rs"),
        ),
        (
            "v2/equivalence/checker_api.rs",
            include_bytes!("../v2/equivalence/checker_api.rs"),
        ),
        (
            "v2/equivalence/checker_equality.rs",
            include_bytes!("../v2/equivalence/checker_equality.rs"),
        ),
        (
            "v2/equivalence/checker_range_spec.rs",
            include_bytes!("../v2/equivalence/checker_range_spec.rs"),
        ),
        (
            "v2/equivalence/checker_ranges.rs",
            include_bytes!("../v2/equivalence/checker_ranges.rs"),
        ),
        (
            "v2/equivalence/checker_spec.rs",
            include_bytes!("../v2/equivalence/checker_spec.rs"),
        ),
        ("v2/equivalence.rs", include_bytes!("../v2/equivalence.rs")),
        ("v2/history.rs", include_bytes!("../v2/history.rs")),
        ("v2/lineage.rs", include_bytes!("../v2/lineage.rs")),
        (
            "v2/migration/payload.rs",
            include_bytes!("../v2/migration/payload.rs"),
        ),
        ("v2/migration.rs", include_bytes!("../v2/migration.rs")),
        ("v2/relay.rs", include_bytes!("../v2/relay.rs")),
        ("v2/upgrade.rs", include_bytes!("../v2/upgrade.rs")),
    ])
}
