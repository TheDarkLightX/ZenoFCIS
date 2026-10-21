//! The pure step functions: commit with recovery, and outbox delivery.
//!
//! A program is `start(invocation, policy) -> Step` and
//! `resume(continuation, response) -> Step`: total functions over explicit
//! data, with no I/O, clock, randomness, closure or macro. A continuation is a
//! private phase plus saved values. It has no `Clone`, so within one process
//! it is resumed at most once; a crash destroys it.

use alloc::vec::Vec;
use core::num::NonZeroU64;

use zeno_fcis_codec::Hash32;

use super::port::{
    AuditedHistory, Code, CommitMode, CommitReceipt, Delivery, EpochTick, Fault, OpId, Operation,
    Outcome, PublicationData, Purpose, Response, Sent, Store, WaitError,
};
use super::registry::{Capability, Port};

/// One step of a program.
// A step is moved once per request; boxing would add an allocation per step
// solely to shrink the request variant.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Eq, PartialEq)]
pub enum Step<K, T> {
    /// The run ended.
    Done(Outcome<T>),
    /// The program requests one operation and waits for its response.
    Request(OpId, Operation, K),
}

/// A program the driver can run.
pub trait Program {
    /// Saved phase and values, consumed by [`Program::resume`].
    type Continuation;
    /// What a successful run yields.
    type Output;
    /// Every port the program can request.
    const PORTS: &'static [Port];
    /// The capabilities the program declares; each port's must be among them.
    const CAPABILITIES: &'static [Capability];
    /// Resumes the program with the response to its outstanding request.
    fn resume(
        continuation: Self::Continuation,
        response: Response,
    ) -> Step<Self::Continuation, Self::Output>;
}

/// Commit one publication, recovering by its original key after a lost reply.
#[derive(Debug)]
pub struct CommitProgram;

/// The retained commit request: the caller keeps it across uncertainty.
#[derive(Debug, Eq, PartialEq)]
pub struct CommitInvocation {
    /// Process epoch of the run.
    pub epoch: u64,
    /// Store handle to commit through.
    pub store: Store,
    /// Original replay key.
    pub key: Hash32,
    /// Original publication inputs.
    pub publication: PublicationData,
    /// Original expected-version guard.
    pub mode: CommitMode,
}

/// The commit program's retry policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitPolicy {
    /// Retries after NotApplied or Unknown, shared by every phase.
    pub retries: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommitPhase {
    Commit,
    Reopen,
    History,
}

/// Saved values of the commit program.
#[derive(Debug, Eq, PartialEq)]
pub struct CommitContinuation {
    phase: CommitPhase,
    id: OpId,
    retries: u8,
    mode: CommitMode,
    unknown: Option<Fault>,
    invocation: CommitInvocation,
}

type CommitStep = Step<CommitContinuation, CommitReceipt>;

impl CommitProgram {
    /// Requests the original commit.
    #[must_use]
    pub fn start(invocation: CommitInvocation, policy: CommitPolicy) -> CommitStep {
        let mode = invocation.mode;
        let id = OpId::first(invocation.epoch);
        let operation = commit_operation(&invocation, mode);
        Step::Request(
            id,
            operation,
            CommitContinuation {
                phase: CommitPhase::Commit,
                id,
                retries: policy.retries,
                mode,
                unknown: None,
                invocation,
            },
        )
    }

    /// Restarts a commit whose run ended without an answer, as after a
    /// crash: the caller's retained request is recovered by reopening the
    /// store and reading its history before any retry.
    #[must_use]
    pub fn recover(invocation: CommitInvocation, policy: CommitPolicy) -> CommitStep {
        let id = OpId::first(invocation.epoch);
        Step::Request(
            id,
            Operation::Reopen(invocation.store),
            CommitContinuation {
                phase: CommitPhase::Reopen,
                id,
                retries: policy.retries,
                mode: invocation.mode,
                unknown: Some(Fault::Io),
                invocation,
            },
        )
    }
}

impl Program for CommitProgram {
    type Continuation = CommitContinuation;
    type Output = CommitReceipt;
    const PORTS: &'static [Port] = &[Port::Commit, Port::Reopen, Port::ReadHistory];
    const CAPABILITIES: &'static [Capability] = &[
        Capability::WriteStore,
        Capability::OpenStore,
        Capability::ReadStore,
    ];

    fn resume(continuation: CommitContinuation, response: Response) -> CommitStep {
        match (continuation.phase, response) {
            (CommitPhase::Commit, Response::Commit(outcome)) => committed(continuation, outcome),
            (CommitPhase::Reopen, Response::Reopen(outcome)) => match outcome {
                Outcome::Ok(_) => ask_commit_phase(continuation, CommitPhase::History),
                Outcome::Refused(_) => unresolved(&continuation, Fault::Io),
                Outcome::NotApplied(fault) | Outcome::Unknown(fault) => {
                    retry_commit_phase(continuation, CommitPhase::Reopen, fault)
                }
            },
            (CommitPhase::History, Response::ReadHistory(outcome)) => match outcome {
                Outcome::Ok(history) => {
                    // V2 checks the expected version before the replay lookup, so
                    // only the exact original key and publication may retry
                    // unversioned. Absence keeps the original guard.
                    let mode = if contains(&history, &continuation.invocation) {
                        CommitMode::Unversioned
                    } else {
                        continuation.invocation.mode
                    };
                    ask_commit(continuation, mode)
                }
                Outcome::Refused(_) => unresolved(&continuation, Fault::Io),
                Outcome::NotApplied(fault) | Outcome::Unknown(fault) => {
                    retry_commit_phase(continuation, CommitPhase::History, fault)
                }
            },
            // Unreachable through the driver, which admits only the requested
            // port's response; never reported as success or refusal.
            (_, _) => unresolved(&continuation, Fault::Io),
        }
    }
}

fn committed(continuation: CommitContinuation, outcome: Outcome<CommitReceipt>) -> CommitStep {
    match outcome {
        Outcome::Ok(receipt) => Step::Done(Outcome::Ok(receipt)),
        Outcome::Refused(code) => Step::Done(Outcome::Refused(code)),
        Outcome::NotApplied(fault) => {
            if continuation.retries == 0 {
                return match continuation.unknown {
                    Some(original) => Step::Done(Outcome::Unknown(original)),
                    None => Step::Done(Outcome::NotApplied(fault)),
                };
            }
            let mode = continuation.mode;
            ask_commit(spend(continuation), mode)
        }
        Outcome::Unknown(fault) => {
            if continuation.retries == 0 {
                return unresolved(&continuation, fault);
            }
            let mut continuation = spend(continuation);
            if continuation.unknown.is_none() {
                continuation.unknown = Some(fault);
            }
            ask_commit_phase(continuation, CommitPhase::Reopen)
        }
    }
}

fn retry_commit_phase(
    continuation: CommitContinuation,
    phase: CommitPhase,
    fault: Fault,
) -> CommitStep {
    if continuation.retries == 0 {
        return unresolved(&continuation, fault);
    }
    ask_commit_phase(spend(continuation), phase)
}

fn spend(mut continuation: CommitContinuation) -> CommitContinuation {
    continuation.retries -= 1;
    continuation
}

fn unresolved(continuation: &CommitContinuation, fault: Fault) -> CommitStep {
    match continuation.unknown {
        Some(original) => Step::Done(Outcome::Unknown(original)),
        None => Step::Done(Outcome::Unknown(fault)),
    }
}

fn ask_commit(mut continuation: CommitContinuation, mode: CommitMode) -> CommitStep {
    continuation.mode = mode;
    ask_commit_phase(continuation, CommitPhase::Commit)
}

/// Requests the next operation of `phase`.
fn ask_commit_phase(mut continuation: CommitContinuation, phase: CommitPhase) -> CommitStep {
    let Some(id) = continuation.id.next() else {
        return match continuation.unknown {
            Some(original) => Step::Done(Outcome::Unknown(original)),
            None => Step::Done(Outcome::Refused(Code::Range)),
        };
    };
    let store = continuation.invocation.store;
    let operation = match phase {
        CommitPhase::Commit => commit_operation(&continuation.invocation, continuation.mode),
        CommitPhase::Reopen => Operation::Reopen(store),
        CommitPhase::History => Operation::ReadHistory(store),
    };
    continuation.phase = phase;
    continuation.id = id;
    Step::Request(id, operation, continuation)
}

fn commit_operation(invocation: &CommitInvocation, mode: CommitMode) -> Operation {
    Operation::Commit(
        invocation.store,
        invocation.key,
        invocation.publication.clone(),
        mode,
    )
}

fn contains(history: &AuditedHistory, invocation: &CommitInvocation) -> bool {
    for entry in &history.commits {
        if entry.key == invocation.key && entry.publication == invocation.publication {
            return true;
        }
    }
    false
}

/// Deliver every pending record: send its exact bytes and acknowledge only a
/// matching report. NotSent and Unknown leave the record pending.
#[derive(Debug)]
pub struct DeliveryProgram;

/// One delivery run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryInvocation {
    /// Process epoch of the run.
    pub epoch: u64,
    /// Store handle to deliver from.
    pub store: Store,
}

/// The delivery program's retry, backoff and jitter policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryPolicy {
    /// Retries after a failed read, send or acknowledgement, shared by the run.
    pub retries: u8,
    /// Ticks to wait before a resend, before jitter.
    pub backoff: u64,
    /// Jitter is drawn below this bound.
    pub jitter: NonZeroU64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeliveryPhase {
    Read,
    Send,
    Acknowledge,
    Clock,
    Jitter,
    Wait,
}

/// Saved values of the delivery program.
#[derive(Debug, Eq, PartialEq)]
pub struct DeliveryContinuation {
    phase: DeliveryPhase,
    id: OpId,
    retries: u8,
    invocation: DeliveryInvocation,
    policy: DeliveryPolicy,
    pending: Vec<Delivery>,
    current: usize,
    acknowledged: u64,
    may_have_sent: bool,
    tick: EpochTick,
}

type DeliveryStep = Step<DeliveryContinuation, u64>;

impl DeliveryProgram {
    /// Requests the exact pending records.
    #[must_use]
    pub fn start(invocation: DeliveryInvocation, policy: DeliveryPolicy) -> DeliveryStep {
        let id = OpId::first(invocation.epoch);
        Step::Request(
            id,
            Operation::ReadOutbox(invocation.store),
            DeliveryContinuation {
                phase: DeliveryPhase::Read,
                id,
                retries: policy.retries,
                invocation,
                policy,
                pending: Vec::new(),
                current: 0,
                acknowledged: 0,
                may_have_sent: false,
                tick: EpochTick {
                    epoch: invocation.epoch,
                    tick: 0,
                },
            },
        )
    }
}

impl Program for DeliveryProgram {
    type Continuation = DeliveryContinuation;
    type Output = u64;
    const PORTS: &'static [Port] = &[
        Port::ReadOutbox,
        Port::Send,
        Port::Acknowledge,
        Port::Clock,
        Port::Random,
        Port::Wait,
    ];
    const CAPABILITIES: &'static [Capability] = &[
        Capability::ReadStore,
        Capability::Transport,
        Capability::Relay,
        Capability::Clock,
        Capability::Jitter,
    ];

    fn resume(continuation: DeliveryContinuation, response: Response) -> DeliveryStep {
        match (continuation.phase, response) {
            (DeliveryPhase::Read, Response::ReadOutbox(outcome)) => read(continuation, outcome),
            (DeliveryPhase::Send, Response::Send(sent)) => sent_one(continuation, sent),
            (DeliveryPhase::Acknowledge, Response::Acknowledge(outcome)) => {
                acknowledged(continuation, outcome)
            }
            (DeliveryPhase::Clock, Response::Clock(reading)) => match reading {
                Ok(now) if now.epoch == continuation.invocation.epoch => {
                    let mut continuation = continuation;
                    continuation.tick = now;
                    let operation =
                        Operation::Random(Purpose::RetryJitter, continuation.policy.jitter);
                    ask(continuation, DeliveryPhase::Jitter, operation)
                }
                Ok(_) => delivery_done(&continuation, Outcome::Refused(Code::Protocol)),
                Err(fault) => delivery_done(&continuation, Outcome::NotApplied(fault)),
            },
            (DeliveryPhase::Jitter, Response::Random(draw)) => match draw {
                Ok(draw) if draw < continuation.policy.jitter.get() => {
                    let Some(deadline) = continuation
                        .tick
                        .tick
                        .checked_add(continuation.policy.backoff)
                    else {
                        return delivery_done(&continuation, Outcome::Refused(Code::Range));
                    };
                    let Some(deadline) = deadline.checked_add(draw) else {
                        return delivery_done(&continuation, Outcome::Refused(Code::Range));
                    };
                    let mut continuation = continuation;
                    continuation.tick.tick = deadline;
                    let operation = Operation::Wait(continuation.tick);
                    ask(continuation, DeliveryPhase::Wait, operation)
                }
                Ok(_) => delivery_done(&continuation, Outcome::Refused(Code::Protocol)),
                Err(fault) => delivery_done(&continuation, Outcome::NotApplied(fault)),
            },
            (DeliveryPhase::Wait, Response::Wait(woke)) => match woke {
                Ok(now)
                    if now.epoch == continuation.tick.epoch
                        && now.tick >= continuation.tick.tick =>
                {
                    send(continuation)
                }
                Ok(_) => delivery_done(&continuation, Outcome::Refused(Code::Protocol)),
                Err(WaitError::Interrupted) => {
                    delivery_done(&continuation, Outcome::Refused(Code::Interrupted))
                }
                Err(WaitError::Failed(fault)) => {
                    delivery_done(&continuation, Outcome::NotApplied(fault))
                }
            },
            // Unreachable through the driver, which admits only the requested
            // port's response.
            (_, _) => delivery_done(&continuation, Outcome::Refused(Code::Protocol)),
        }
    }
}

fn read(continuation: DeliveryContinuation, outcome: Outcome<Vec<Delivery>>) -> DeliveryStep {
    match outcome {
        Outcome::Ok(pending) => {
            if !ordered(&pending) {
                return delivery_done(&continuation, Outcome::Refused(Code::Protocol));
            }
            let mut continuation = continuation;
            continuation.pending = pending;
            continuation.current = 0;
            send(continuation)
        }
        Outcome::Refused(code) => delivery_done(&continuation, Outcome::Refused(code)),
        // A read changes nothing durable, so a failed read did not apply.
        Outcome::NotApplied(fault) | Outcome::Unknown(fault) => {
            if continuation.retries == 0 {
                return delivery_done(&continuation, Outcome::NotApplied(fault));
            }
            let mut continuation = continuation;
            continuation.retries -= 1;
            let operation = Operation::ReadOutbox(continuation.invocation.store);
            ask(continuation, DeliveryPhase::Read, operation)
        }
    }
}

fn sent_one(mut continuation: DeliveryContinuation, sent: Sent) -> DeliveryStep {
    match &sent {
        Sent::Sent(_) | Sent::Unknown(_) => continuation.may_have_sent = true,
        Sent::NotSent(_) | Sent::Refused(_) => {}
    }
    let Some(delivery) = continuation.pending.get(continuation.current) else {
        return delivery_done(&continuation, Outcome::Refused(Code::Protocol));
    };
    match sent {
        Sent::Sent(report)
            if report.delivery_id == delivery.id
                && report.payload_sha256 == delivery.payload_sha256 =>
        {
            let operation = Operation::Acknowledge(
                continuation.invocation.store,
                delivery.id,
                delivery.payload_sha256,
            );
            ask(continuation, DeliveryPhase::Acknowledge, operation)
        }
        Sent::Sent(_) => delivery_done(&continuation, Outcome::Refused(Code::Protocol)),
        Sent::Refused(code) => delivery_done(&continuation, Outcome::Refused(code)),
        Sent::NotSent(code) => {
            if continuation.retries == 0 {
                return delivery_done(&continuation, Outcome::Refused(code));
            }
            back_off(continuation)
        }
        Sent::Unknown(fault) => {
            if continuation.retries == 0 {
                return delivery_done(&continuation, Outcome::Unknown(fault));
            }
            back_off(continuation)
        }
    }
}

fn back_off(mut continuation: DeliveryContinuation) -> DeliveryStep {
    continuation.retries -= 1;
    ask(continuation, DeliveryPhase::Clock, Operation::Clock)
}

fn acknowledged(continuation: DeliveryContinuation, outcome: Outcome<()>) -> DeliveryStep {
    match outcome {
        Outcome::Ok(()) | Outcome::Refused(Code::AlreadyAcknowledged) => {
            let Some(count) = continuation.acknowledged.checked_add(1) else {
                return delivery_done(&continuation, Outcome::Refused(Code::Range));
            };
            let mut continuation = continuation;
            continuation.acknowledged = count;
            continuation.current += 1;
            send(continuation)
        }
        Outcome::Refused(code) => delivery_done(&continuation, Outcome::Refused(code)),
        Outcome::NotApplied(fault) => retry_acknowledge(continuation, Outcome::NotApplied(fault)),
        Outcome::Unknown(fault) => retry_acknowledge(continuation, Outcome::Unknown(fault)),
    }
}

/// Repeats an acknowledgement that failed; a repeat of one that applied
/// answers AlreadyAcknowledged.
fn retry_acknowledge(mut continuation: DeliveryContinuation, failed: Outcome<u64>) -> DeliveryStep {
    if continuation.retries == 0 {
        return delivery_done(&continuation, failed);
    }
    let Some(delivery) = continuation.pending.get(continuation.current) else {
        return delivery_done(&continuation, Outcome::Refused(Code::Protocol));
    };
    let operation = Operation::Acknowledge(
        continuation.invocation.store,
        delivery.id,
        delivery.payload_sha256,
    );
    continuation.retries -= 1;
    ask(continuation, DeliveryPhase::Acknowledge, operation)
}

/// Sends the current record's exact bytes, or ends when none is left.
fn send(continuation: DeliveryContinuation) -> DeliveryStep {
    let Some(delivery) = continuation.pending.get(continuation.current) else {
        return delivery_done(&continuation, Outcome::Ok(continuation.acknowledged));
    };
    let operation = Operation::Send(continuation.invocation.store, delivery.clone());
    ask(continuation, DeliveryPhase::Send, operation)
}

fn ask(
    mut continuation: DeliveryContinuation,
    phase: DeliveryPhase,
    operation: Operation,
) -> DeliveryStep {
    let Some(id) = continuation.id.next() else {
        return delivery_done(&continuation, Outcome::Refused(Code::Range));
    };
    continuation.phase = phase;
    continuation.id = id;
    Step::Request(id, operation, continuation)
}

/// Whether records are strictly ascending by version, lane and ordinal.
fn ordered(pending: &[Delivery]) -> bool {
    let mut index = 1;
    while index < pending.len() {
        let (before, after) = (&pending[index - 1], &pending[index]);
        if (before.version, before.lane, before.ordinal)
            >= (after.version, after.lane, after.ordinal)
        {
            return false;
        }
        index += 1;
    }
    true
}

// A run-level non-execution result is valid only before any possible send.
// This flag stays set across acknowledgements and subsequent records: a later
// refusal cannot erase an earlier external effect or partial progress.
fn delivery_done(continuation: &DeliveryContinuation, outcome: Outcome<u64>) -> DeliveryStep {
    let outcome = if continuation.may_have_sent {
        match outcome {
            Outcome::Ok(count) => Outcome::Ok(count),
            Outcome::NotApplied(fault) | Outcome::Unknown(fault) => Outcome::Unknown(fault),
            Outcome::Refused(_) => Outcome::Unknown(Fault::Io),
        }
    } else {
        outcome
    };
    Step::Done(outcome)
}
