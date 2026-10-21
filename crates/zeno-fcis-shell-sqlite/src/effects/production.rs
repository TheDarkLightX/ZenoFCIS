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

/// A transport for Send: an observation, never a proof of receipt.
pub trait Transport {
    /// Sends one delivery's exact bytes.
    fn send(&mut self, delivery: &Delivery) -> Sent;
}

/// The opt-in time adapter for Clock, Random and Wait.
pub trait Time {
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
    #[cfg(feature = "crash-injection")]
    interrupt: Option<crate::CrashPoint>,
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
        Ok(Self {
            path,
            lineage,
            store: Store { identity, handle },
            shell: Some(shell),
            transport,
            time,
            #[cfg(feature = "crash-injection")]
            interrupt: None,
        })
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

    /// Sets the handle's SQLite busy timeout, for busy tests.
    #[cfg(feature = "crash-injection")]
    pub fn set_test_busy_timeout(&mut self, timeout: std::time::Duration) -> Result<(), Error> {
        self.shell()?.set_test_busy_timeout(timeout)
    }

    fn shell(&mut self) -> Result<&mut V2SqliteShell<'a, 'p>, Error> {
        if self.shell.is_none() {
            self.shell = Some(V2SqliteShell::open(&self.path, current(self.lineage)?)?);
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
        let shell = match self.shell() {
            Ok(shell) => shell,
            Err(error) => return answer(Port::Commit, Err(error), true),
        };
        #[cfg(feature = "crash-injection")]
        let result = match mode {
            CommitMode::Unversioned => shell.commit_with_crash(key, publication, interrupt),
            CommitMode::At(version) => {
                shell.commit_with_crash_at(version, key, publication, interrupt)
            }
            CommitMode::BoundedAt(version, cap) => {
                shell.commit_bounded_with_crash_at(version, key, publication, cap, interrupt)
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
            self.shell = None;
            return Handled::Crash;
        }
        answer(port, result, writes)
    }

    fn history(&mut self) -> Result<AuditedHistory, Error> {
        let audited = self.lineage.audit_history_read_only(&self.path)?;
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

    fn reopen(&mut self) -> Result<Snapshot, Error> {
        self.shell = None;
        let mut shell = V2SqliteShell::open(&self.path, current(self.lineage)?)?;
        shell.audit()?;
        let head = snapshot(&shell.snapshot()?);
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
        self.shell = None;
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
