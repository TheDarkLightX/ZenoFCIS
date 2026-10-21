//! One durable store: replay from audited inputs, then checked commit and delivery.
use crate::{Command, Endpoint, Event, Refusal, Trace, TraceSink, digest};
use std::num::NonZeroU64;
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::effects::{
    codec,
    driver::{Binding, Driver, Reply},
    handler::{self, Handled, Terminal},
    port::{Code, CommitMode, OpId, Operation, Outcome, Response, Store},
    program::{
        CommitInvocation, CommitPolicy, CommitProgram, DeliveryInvocation, DeliveryPolicy,
        DeliveryProgram, Program, Step,
    },
    registry::REGISTRY,
};
use zeno_fcis_shell_sqlite::effects::{bind_publication, publication_data};
use zeno_fcis_synthesis::finite::{
    v2_authority::{Authority, PublicationOutcome},
    v2_composition::Raw,
};

/// Serial shell around an immutable Authority, one principal's canonical context
/// and one production terminal. The context must express that principal's rights
/// in the application's checked contract. No body field can select it.
pub struct Durable<'a, 'p, H> {
    authority: &'a Authority<'p>,
    terminal: H,
    store: Store,
    principal: Hash32,
    context: Vec<u8>,
}
impl<'a, 'p, H: Terminal> Durable<'a, 'p, H> {
    /// Bind the terminal's store and copy the principal's immutable core context.
    /// The terminal must cover both checked programs, including bounded backoff.
    pub fn new(
        authority: &'a Authority<'p>,
        terminal: H,
        store: Store,
        principal: Hash32,
        context: &[u8],
    ) -> Result<Self, Refusal> {
        handler::bind(terminal.ports(), CommitProgram::PORTS).map_err(|_| Refusal::Binding)?;
        handler::bind(terminal.ports(), DeliveryProgram::PORTS).map_err(|_| Refusal::Binding)?;
        Ok(Self {
            authority,
            terminal,
            store,
            principal,
            context: context.to_vec(),
        })
    }
    /// Return the exclusively owned shell handler after use.
    pub fn into_terminal(self) -> H {
        self.terminal
    }

    fn call(
        &mut self,
        operation: Operation,
        trace: Trace,
        sink: &mut dyn TraceSink,
    ) -> Result<Response, Refusal> {
        let binding = Binding::of(OpId::first(1), &operation);
        observe(&mut self.terminal, &binding, &operation, trace, sink)
    }
}
impl<H: Terminal> Endpoint for Durable<'_, '_, H> {
    fn execute(&mut self, command: &Command, sink: &mut dyn TraceSink) -> Result<Vec<u8>, Refusal> {
        if command.principal != self.principal
            || command.contract != digest(self.authority.identity())
        {
            return Err(Refusal::Binding);
        }
        let mut trace = Trace::request(command);
        let key = command.replay_key();
        // Audited history is reporting data, never a committing capability.
        let history = match self.call(Operation::ReadHistory(self.store), trace, sink)? {
            Response::ReadHistory(Outcome::Ok(history)) => history,
            _ => return Err(Refusal::Unavailable),
        };
        let original = history.commits.iter().find(|entry| entry.key == key);
        let data = if let Some(entry) = original {
            if entry.publication.command != command.bytes
                || entry.publication.context != self.context
                || Some(entry.version) != command.expected.checked_add(1)
            {
                return Err(Refusal::Replay);
            }
            entry.publication.clone()
        } else {
            let head = match self.call(Operation::ReadState(self.store), trace, sink)? {
                Response::ReadState(Outcome::Ok(head)) => head,
                _ => return Err(Refusal::Unavailable),
            };
            if head.version != command.expected {
                return Err(Refusal::Conflict);
            }
            match self.authority.publish(Raw {
                state: &head.state,
                command: &command.bytes,
                context: &self.context,
            }) {
                PublicationOutcome::Commit(publication) => publication_data(&publication),
                _ => return Err(Refusal::Decision),
            }
        };
        // Rebuild even audited data: it never supplies publication authority.
        let publication = bind_publication(self.authority, &data).map_err(|_| Refusal::Binding)?;
        trace.event = Event::Decision;
        trace.decision = Some(digest(publication.subject()));
        sink.record(trace);
        let mut packet = b"ZFIP1".to_vec();
        codec::hash(&mut packet, command.id());
        codec::hash(&mut packet, key);
        codec::bytes(&mut packet, publication.subject());
        codec::bytes(&mut packet, publication.poststate());
        let invocation = CommitInvocation {
            epoch: 1,
            store: self.store,
            key,
            publication: data,
            mode: CommitMode::At(command.expected),
        };
        let first = if original.is_some() {
            CommitProgram::recover(invocation, CommitPolicy { retries: 2 })
        } else {
            CommitProgram::start(invocation, CommitPolicy { retries: 2 })
        };
        let receipt = match run::<CommitProgram>(&mut self.terminal, first, trace, sink)? {
            Outcome::Ok(receipt) => receipt,
            Outcome::Refused(Code::Replay) => return Err(Refusal::Replay),
            Outcome::Refused(_) => return Err(Refusal::Conflict),
            _ => return Err(Refusal::Unavailable),
        };
        packet.extend_from_slice(&receipt.version.to_be_bytes());
        codec::hash(&mut packet, receipt.chain);
        trace.event = Event::Commit;
        trace.commit = Some(receipt.chain);
        sink.record(trace);
        let delivery = DeliveryProgram::start(
            DeliveryInvocation {
                epoch: 2,
                store: self.store,
            },
            DeliveryPolicy {
                retries: 2,
                backoff: 1,
                jitter: NonZeroU64::MIN,
            },
        );
        // Delivery failure does not erase or replace the already committed packet.
        // Return uncertainty so the caller retries the same original key.
        if !matches!(
            run::<DeliveryProgram>(&mut self.terminal, delivery, trace, sink)?,
            Outcome::Ok(_)
        ) {
            return Err(Refusal::Unavailable);
        }
        Ok(packet)
    }
}

fn run<P: Program>(
    terminal: &mut dyn Terminal,
    first: Step<P::Continuation, P::Output>,
    trace: Trace,
    sink: &mut dyn TraceSink,
) -> Result<Outcome<P::Output>, Refusal>
where
    P::Output: Clone,
{
    let epoch = match &first {
        Step::Request(id, _, _) => id.epoch,
        Step::Done(_) => 0,
    };
    let mut driver = Driver::<P>::start(REGISTRY, epoch, first).map_err(|_| Refusal::Binding)?;
    for _ in 0..8192 {
        if let Some(outcome) = driver.outcome() {
            return Ok(outcome.clone());
        }
        let (binding, operation) = driver.request().ok_or(Refusal::Binding)?;
        let binding = binding.clone();
        let response = observe(terminal, &binding, operation, trace, sink)?;
        driver
            .reply(Reply { binding, response })
            .map_err(|_| Refusal::Binding)?;
    }
    Err(Refusal::Unavailable)
}

fn observe(
    terminal: &mut dyn Terminal,
    binding: &Binding,
    operation: &Operation,
    trace: Trace,
    sink: &mut dyn TraceSink,
) -> Result<Response, Refusal> {
    let delivery = match operation {
        Operation::Send(_, record) => Some(record.id),
        Operation::Acknowledge(_, id, _) => Some(*id),
        _ => None,
    };
    if matches!(operation, Operation::Send(..)) {
        sink.record(Trace {
            event: Event::Send,
            delivery,
            ..trace
        });
    }
    match terminal.handle(binding, operation) {
        Handled::Answer(response) => {
            if matches!(response, Response::Acknowledge(Outcome::Ok(()))) {
                sink.record(Trace {
                    event: Event::Acknowledge,
                    delivery,
                    ..trace
                });
            }
            if response_failed(&response) {
                sink.record(Trace {
                    event: Event::Fault,
                    delivery,
                    ..trace
                });
            }
            Ok(response)
        }
        Handled::Crash | Handled::Suspend => {
            sink.record(Trace {
                event: Event::Fault,
                delivery,
                ..trace
            });
            Err(Refusal::Unavailable)
        }
    }
}
fn response_failed(response: &Response) -> bool {
    match response {
        Response::ReadState(v) | Response::Reopen(v) => !matches!(v, Outcome::Ok(_)),
        Response::Commit(v) => !matches!(v, Outcome::Ok(_)),
        Response::ReadHistory(v) => !matches!(v, Outcome::Ok(_)),
        Response::ReadOutbox(v) => !matches!(v, Outcome::Ok(_)),
        Response::Acknowledge(v) => !matches!(v, Outcome::Ok(_)),
        Response::Send(v) => !matches!(v, zeno_fcis_shell::effects::port::Sent::Sent(_)),
        Response::Clock(v) => v.is_err(),
        Response::Random(v) => v.is_err(),
        Response::Wait(v) => v.is_err(),
    }
}
