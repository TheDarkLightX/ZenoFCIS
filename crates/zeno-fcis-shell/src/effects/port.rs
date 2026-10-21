//! Plain port data: requests, responses and owned views of shell results.
//!
//! These are schema views, not SQLite types: an upper adapter maps the V2
//! shell's values onto them. None of them grants authority.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::num::NonZeroU64;

use zeno_fcis_codec::Hash32;

use super::registry::Port;
use crate::CommitStatus;

/// A configured store handle: the store's 32-byte identity and the handle number.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Store {
    /// Identity of the store the handle is bound to.
    pub identity: Hash32,
    /// Handle number; two handles on one store differ only here.
    pub handle: u32,
}

/// A request's identity: the process epoch and its ordinal within the epoch.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OpId {
    /// Process epoch; requests from different epochs never match.
    pub epoch: u64,
    /// Position of the request in its epoch, from zero.
    pub ordinal: u64,
}

impl OpId {
    /// The first request of an epoch.
    #[must_use]
    pub const fn first(epoch: u64) -> Self {
        Self { epoch, ordinal: 0 }
    }

    /// The next request of the same epoch, or `None` instead of wrapping.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.ordinal.checked_add(1) {
            Some(ordinal) => Some(Self {
                epoch: self.epoch,
                ordinal,
            }),
            None => None,
        }
    }
}

/// Original inputs of one publication, as a program carries them.
///
/// Reporting data only: the trusted driver holds the matching Publication or
/// rebuilds it through Authority, and refuses data whose subject differs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationData {
    /// Exact original prestate envelope.
    pub prestate: Vec<u8>,
    /// Exact original command envelope.
    pub command: Vec<u8>,
    /// Exact original context envelope.
    pub context: Vec<u8>,
    /// Full identity of the publishing Authority.
    pub identity: Vec<u8>,
    /// Publication subject the Authority computed for these inputs.
    pub subject: Vec<u8>,
}

/// The expected-version guard of a commit, one per existing V2 commit API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitMode {
    /// `commit`: no expected version.
    Unversioned,
    /// `commit_at`: the store must be at this version.
    At(u64),
    /// `commit_bounded_at`: a version and a byte cap for the publication.
    BoundedAt(u64, usize),
}

/// One checked head of a store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    /// Number of committed transitions after genesis.
    pub version: u64,
    /// Full identity of the contract the store runs.
    pub identity: Vec<u8>,
    /// Exact state envelope.
    pub state: Vec<u8>,
    /// Commitment to the state envelope.
    pub root: Hash32,
    /// History hash-chain tip.
    pub chain: Hash32,
}

/// Durable certificate of one commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitReceipt {
    /// Original commit position, also returned by an idempotent replay.
    pub version: u64,
    /// Chain link binding the certificate.
    pub chain: Hash32,
    /// Whether this call committed or matched an earlier commit.
    pub status: CommitStatus,
}

/// One audited commit: its original key and publication inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEntry {
    /// Original replay key.
    pub key: Hash32,
    /// Commit position, from one.
    pub version: u64,
    /// Original publication inputs.
    pub publication: PublicationData,
}

/// A read-only audit of a store: its head and every commit, oldest first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditedHistory {
    /// Checked head.
    pub head: Snapshot,
    /// Every audited commit, oldest first.
    pub commits: Vec<HistoryEntry>,
}

/// One pending delivery record with its exact content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delivery {
    /// Certificate-bound delivery ID.
    pub id: Hash32,
    /// Owning commit position.
    pub version: u64,
    /// Zero for effects, one for durable outbox entries.
    pub lane: u8,
    /// Ordinal within the lane.
    pub ordinal: u32,
    /// Exact destination bytes.
    pub destination: Vec<u8>,
    /// Exact payload bytes.
    pub payload: Vec<u8>,
    /// SHA-256 of the payload, as the relay acknowledges it.
    pub payload_sha256: Hash32,
}

/// What a transport reports about one send: no proof of receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Report {
    /// Delivery ID the transport sent.
    pub delivery_id: Hash32,
    /// SHA-256 of the payload the transport sent.
    pub payload_sha256: Hash32,
}

/// Why a program asks for randomness: only retry jitter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Purpose {
    /// Spread retries; never a credential or a replay key.
    RetryJitter,
}

/// A monotonic tick of one process epoch; no wall-clock authority.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EpochTick {
    /// Process epoch; ticks of different epochs are incomparable.
    pub epoch: u64,
    /// Tick within the epoch.
    pub tick: u64,
}

/// A V2 refusal code, in V2's declaration order, then the effects code.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Code {
    /// The sealed SHA-256 provider failed its known-answer check.
    Provider,
    /// A different schema requires an explicit migration.
    Schema,
    /// Another bound Authority or contract.
    Identity,
    /// The library refused to bind a lineage catalog.
    Authority,
    /// A malformed lineage.
    Lineage,
    /// The store no longer holds the checkpoint.
    Checkpoint,
    /// The other verified invocation kind.
    InvocationKind,
    /// Stored history fails validation.
    History,
    /// An integer or length cannot be represented.
    Range,
    /// The publication was produced for a different current state.
    PreState,
    /// The replay key binds different input or publication bytes.
    Replay,
    /// Another connection changed the state first.
    Concurrent,
    /// A delivery obligation or acknowledgement differs.
    Delivery,
    /// The delivery interpreter differs from the library source.
    Interpreter,
    /// The acknowledged delivery ID is unknown.
    UnknownDelivery,
    /// The delivery is already acknowledged.
    AlreadyAcknowledged,
    /// The acknowledged payload hash differs.
    PayloadMismatch,
    /// The publication exceeds the requested byte cap.
    Capacity,
    /// The pure upgrade decision refused.
    Upgrade,
    /// The store holds an unsupported upgrade.
    Succession,
    /// Needed comparisons stayed unavailable.
    Unsettled,
    /// A response broke its port law: an unordered outbox, a mismatched
    /// report, an early wake or an out-of-range draw.
    Protocol,
}

/// Why an operation failed without a refusal code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fault {
    /// The store or resource was busy.
    Busy,
    /// Storage was full.
    Full,
    /// Input/output failed or the reply was lost.
    Io,
}

/// A store operation's result. NotApplied needs rollback or non-execution
/// evidence; an ambiguous commit or a lost reply is Unknown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome<T> {
    /// The operation succeeded.
    Ok(T),
    /// The operation was refused with no durable change.
    Refused(Code),
    /// The operation did not apply.
    NotApplied(Fault),
    /// The operation may or may not have applied.
    Unknown(Fault),
}

/// A transport observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sent {
    /// The transport sent this content.
    Sent(Report),
    /// The transport did not send.
    NotSent(Code),
    /// The transport may or may not have sent.
    Unknown(Code),
}

/// One request a program makes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Read one checked snapshot.
    ReadState(Store),
    /// Publish once, atomically.
    Commit(Store, Hash32, PublicationData, CommitMode),
    /// Read the audited history (reporting only).
    ReadHistory(Store),
    /// Read the exact pending deliveries, ordered.
    ReadOutbox(Store),
    /// Send one delivery's exact bytes.
    Send(Store, Delivery),
    /// Acknowledge one delivery by ID and payload SHA-256.
    Acknowledge(Store, Hash32, Hash32),
    /// Open and audit the store again; never create one.
    Reopen(Store),
    /// Read the epoch clock.
    Clock,
    /// Draw a value below the bound.
    Random(Purpose, NonZeroU64),
    /// Yield until the deadline.
    Wait(EpochTick),
}

/// One response, answering exactly one operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Response {
    /// Answers ReadState.
    ReadState(Outcome<Snapshot>),
    /// Answers Commit.
    Commit(Outcome<CommitReceipt>),
    /// Answers ReadHistory.
    ReadHistory(Outcome<AuditedHistory>),
    /// Answers ReadOutbox.
    ReadOutbox(Outcome<Vec<Delivery>>),
    /// Answers Send.
    Send(Sent),
    /// Answers Acknowledge.
    Acknowledge(Outcome<()>),
    /// Answers Reopen.
    Reopen(Outcome<Snapshot>),
    /// Answers Clock.
    Clock(Result<EpochTick, Fault>),
    /// Answers Random.
    Random(Result<u64, Fault>),
    /// Answers Wait; an error is an explicit interruption.
    Wait(Result<EpochTick, Fault>),
}

impl Operation {
    /// The port this operation requests.
    #[must_use]
    pub const fn port(&self) -> Port {
        match self {
            Self::ReadState(_) => Port::ReadState,
            Self::Commit(..) => Port::Commit,
            Self::ReadHistory(_) => Port::ReadHistory,
            Self::ReadOutbox(_) => Port::ReadOutbox,
            Self::Send(..) => Port::Send,
            Self::Acknowledge(..) => Port::Acknowledge,
            Self::Reopen(_) => Port::Reopen,
            Self::Clock => Port::Clock,
            Self::Random(..) => Port::Random,
            Self::Wait(_) => Port::Wait,
        }
    }

    /// The store handle this operation addresses, if any.
    #[must_use]
    pub const fn store(&self) -> Option<Store> {
        match self {
            Self::ReadState(store)
            | Self::Commit(store, ..)
            | Self::ReadHistory(store)
            | Self::ReadOutbox(store)
            | Self::Send(store, _)
            | Self::Acknowledge(store, ..)
            | Self::Reopen(store) => Some(*store),
            Self::Clock | Self::Random(..) | Self::Wait(_) => None,
        }
    }

    /// What the operation is about: the publication subject of a commit,
    /// the delivery ID of a send or acknowledgement, and nothing otherwise.
    #[must_use]
    pub fn subject(&self) -> &[u8] {
        match self {
            Self::Commit(_, _, publication, _) => &publication.subject,
            Self::Send(_, delivery) => delivery.id.as_bytes(),
            Self::Acknowledge(_, id, _) => id.as_bytes(),
            _ => &[],
        }
    }

    /// The argument record, for the registry's type check.
    #[must_use]
    pub fn value(&self) -> Val<'_> {
        match self {
            Self::ReadState(store)
            | Self::ReadHistory(store)
            | Self::ReadOutbox(store)
            | Self::Reopen(store) => Val::Record(vec![Val::Store(*store)]),
            Self::Commit(store, key, publication, mode) => Val::Record(vec![
                Val::Store(*store),
                Val::Hash(*key),
                publication_value(publication),
                mode_value(*mode),
            ]),
            Self::Send(store, delivery) => {
                Val::Record(vec![Val::Store(*store), delivery_value(delivery)])
            }
            Self::Acknowledge(store, id, payload) => Val::Record(vec![
                Val::Store(*store),
                Val::Hash(*id),
                Val::Hash(*payload),
            ]),
            Self::Clock => Val::Record(Vec::new()),
            Self::Random(Purpose::RetryJitter, bound) => Val::Record(vec![
                Val::Choice(0, Box::new(Val::Unit)),
                Val::Int(bound.get()),
            ]),
            Self::Wait(deadline) => Val::Record(vec![tick_value(*deadline)]),
        }
    }
}

impl Response {
    /// The port this response answers.
    #[must_use]
    pub const fn port(&self) -> Port {
        match self {
            Self::ReadState(_) => Port::ReadState,
            Self::Commit(_) => Port::Commit,
            Self::ReadHistory(_) => Port::ReadHistory,
            Self::ReadOutbox(_) => Port::ReadOutbox,
            Self::Send(_) => Port::Send,
            Self::Acknowledge(_) => Port::Acknowledge,
            Self::Reopen(_) => Port::Reopen,
            Self::Clock(_) => Port::Clock,
            Self::Random(_) => Port::Random,
            Self::Wait(_) => Port::Wait,
        }
    }

    /// The success payload, for the registry's type check; `None` for a
    /// refusal, fault or failed send, which carry no sized data.
    #[must_use]
    pub fn value(&self) -> Option<Val<'_>> {
        match self {
            Self::ReadState(Outcome::Ok(snapshot)) | Self::Reopen(Outcome::Ok(snapshot)) => {
                Some(snapshot_value(snapshot))
            }
            Self::Commit(Outcome::Ok(receipt)) => Some(Val::Record(vec![
                Val::Int(receipt.version),
                Val::Hash(receipt.chain),
                Val::Choice(
                    match receipt.status {
                        CommitStatus::Committed => 0,
                        CommitStatus::IdempotentReplay => 1,
                    },
                    Box::new(Val::Unit),
                ),
            ])),
            Self::ReadHistory(Outcome::Ok(history)) => {
                let mut commits = Vec::with_capacity(history.commits.len());
                for entry in &history.commits {
                    commits.push(Val::Record(vec![
                        Val::Hash(entry.key),
                        Val::Int(entry.version),
                        publication_value(&entry.publication),
                    ]));
                }
                Some(Val::Record(vec![
                    snapshot_value(&history.head),
                    Val::List(commits),
                ]))
            }
            Self::ReadOutbox(Outcome::Ok(deliveries)) => {
                let mut items = Vec::with_capacity(deliveries.len());
                for delivery in deliveries {
                    items.push(delivery_value(delivery));
                }
                Some(Val::List(items))
            }
            Self::Send(Sent::Sent(report)) => Some(Val::Record(vec![
                Val::Hash(report.delivery_id),
                Val::Hash(report.payload_sha256),
            ])),
            Self::Acknowledge(Outcome::Ok(())) => Some(Val::Unit),
            Self::Clock(Ok(tick)) | Self::Wait(Ok(tick)) => Some(tick_value(*tick)),
            Self::Random(Ok(draw)) => Some(Val::Int(*draw)),
            _ => None,
        }
    }
}

/// A port value viewed against its registry schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Val<'a> {
    /// No data.
    Unit,
    /// A store handle.
    Store(Store),
    /// A 32-byte hash.
    Hash(Hash32),
    /// Exact bytes.
    Bytes(&'a [u8]),
    /// An unsigned integer.
    Int(u64),
    /// Fields in declaration order.
    Record(Vec<Val<'a>>),
    /// The chosen alternative's index and payload.
    Choice(usize, Box<Val<'a>>),
    /// Items in order.
    List(Vec<Val<'a>>),
}

fn publication_value(publication: &PublicationData) -> Val<'_> {
    Val::Record(vec![
        Val::Bytes(&publication.prestate),
        Val::Bytes(&publication.command),
        Val::Bytes(&publication.context),
        Val::Bytes(&publication.identity),
        Val::Bytes(&publication.subject),
    ])
}

fn mode_value(mode: CommitMode) -> Val<'static> {
    match mode {
        CommitMode::Unversioned => Val::Choice(0, Box::new(Val::Unit)),
        CommitMode::At(version) => Val::Choice(1, Box::new(Val::Int(version))),
        CommitMode::BoundedAt(version, cap) => Val::Choice(
            2,
            Box::new(Val::Record(vec![
                Val::Int(version),
                // usize fits u64 on every supported target.
                Val::Int(u64::try_from(cap).unwrap_or(u64::MAX)),
            ])),
        ),
    }
}

fn snapshot_value(snapshot: &Snapshot) -> Val<'_> {
    Val::Record(vec![
        Val::Int(snapshot.version),
        Val::Bytes(&snapshot.identity),
        Val::Bytes(&snapshot.state),
        Val::Hash(snapshot.root),
        Val::Hash(snapshot.chain),
    ])
}

fn delivery_value(delivery: &Delivery) -> Val<'_> {
    Val::Record(vec![
        Val::Hash(delivery.id),
        Val::Int(delivery.version),
        Val::Int(u64::from(delivery.lane)),
        Val::Int(u64::from(delivery.ordinal)),
        Val::Bytes(&delivery.destination),
        Val::Bytes(&delivery.payload),
        Val::Hash(delivery.payload_sha256),
    ])
}

fn tick_value(tick: EpochTick) -> Val<'static> {
    Val::Record(vec![Val::Int(tick.epoch), Val::Int(tick.tick)])
}
