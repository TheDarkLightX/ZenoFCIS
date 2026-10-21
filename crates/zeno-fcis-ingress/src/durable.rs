//! One durable store: replay from audited inputs, then checked commit and delivery.
use crate::{Command, Endpoint, Event, Refusal, Trace, TraceSink, digest};
use std::{collections::BTreeMap, num::NonZeroU64};
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::effects::{
    codec,
    driver::{Binding, Driver, Reply},
    handler::{self, Handled, Terminal},
    port::{Code, CommitMode, HistoryEntry, OpId, Operation, Outcome, Response, Snapshot, Store},
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

/// Derives a new decision's context from the party's configured context.
pub type Admit<'f> = dyn FnMut(&[u8]) -> Result<Vec<u8>, Refusal> + 'f;

/// Serial shell around an immutable Authority, each configured principal's
/// canonical context and one production terminal. Each context must express that
/// principal's rights in the application's checked contract. The authenticated
/// principal alone selects its context; no body field can select one.
pub struct Durable<'a, 'p, H> {
    authority: &'a Authority<'p>,
    terminal: H,
    store: Store,
    parties: Vec<(Hash32, Vec<u8>)>,
    head: Option<Snapshot>,
    commits: BTreeMap<Hash32, HistoryEntry>,
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
        Self::with_parties(
            authority,
            terminal,
            store,
            vec![(principal, context.to_vec())],
        )
    }
    /// Bind 1 to 64 distinct principals, each with its own immutable core context.
    /// Contexts may repeat; replay keys remain scoped to each principal.
    pub fn with_parties(
        authority: &'a Authority<'p>,
        terminal: H,
        store: Store,
        parties: Vec<(Hash32, Vec<u8>)>,
    ) -> Result<Self, Refusal> {
        if parties.is_empty()
            || parties.len() > crate::MAX_PARTIES
            || parties
                .iter()
                .enumerate()
                .any(|(i, (principal, _))| parties[..i].iter().any(|(old, _)| old == principal))
        {
            return Err(Refusal::Malformed);
        }
        handler::bind(terminal.ports(), CommitProgram::PORTS).map_err(|_| Refusal::Binding)?;
        handler::bind(terminal.ports(), DeliveryProgram::PORTS).map_err(|_| Refusal::Binding)?;
        Ok(Self {
            authority,
            terminal,
            store,
            parties,
            head: None,
            commits: BTreeMap::new(),
        })
    }
    /// The canonical context configured for an authenticated principal, if any.
    pub fn context(&self, principal: Hash32) -> Option<&[u8]> {
        self.parties
            .iter()
            .find(|(owner, _)| *owner == principal)
            .map(|(_, context)| context.as_slice())
    }
    /// Return the exclusively owned shell handler after use.
    pub fn into_terminal(self) -> H {
        self.terminal
    }

    // Reporting data only. Each entry owns its bytes; it must still be rebound
    // through Authority before a commit. A failed commit drops the cache so an
    // uncertain result is recovered through the original audited history path.
    fn load(&mut self, trace: Trace, sink: &mut dyn TraceSink) -> Result<(), Refusal> {
        if self.head.is_some() {
            return Ok(());
        }
        let history = match self.call(Operation::ReadHistory(self.store), trace, sink)? {
            Response::ReadHistory(Outcome::Ok(history)) => history,
            _ => return Err(Refusal::Unavailable),
        };
        let mut commits = BTreeMap::new();
        for (index, entry) in history.commits.into_iter().enumerate() {
            if Some(entry.version) != u64::try_from(index).ok().and_then(|n| n.checked_add(1))
                || commits.insert(entry.key, entry).is_some()
            {
                return Err(Refusal::Binding);
            }
        }
        if u64::try_from(commits.len()).ok() != Some(history.head.version) {
            return Err(Refusal::Binding);
        }
        self.commits = commits;
        self.head = Some(history.head);
        Ok(())
    }
    /// Validate the terminal's tip against the retained verified head. This is
    /// reporting state, never a publication capability. Unexpected movement
    /// refuses; an exclusive production terminal also refuses external writes.
    pub fn verified_snapshot(&mut self) -> Result<Snapshot, Refusal> {
        self.verified_head(Trace::empty(Event::Request), &mut Ignore)
    }
    fn verified_head(
        &mut self,
        trace: Trace,
        sink: &mut dyn TraceSink,
    ) -> Result<Snapshot, Refusal> {
        self.load(trace, sink)?;
        let actual = match self.call(Operation::ReadState(self.store), trace, sink)? {
            Response::ReadState(Outcome::Ok(head)) => head,
            _ => return Err(Refusal::Unavailable),
        };
        if self.head.as_ref() != Some(&actual) {
            return Err(Refusal::Unavailable);
        }
        Ok(actual)
    }
    /// Original audited or successfully committed inputs, indexed by scoped key.
    /// The explicit key comparison prevents a misdirected index from serving an
    /// unrelated entry. No mutable alias escapes.
    pub fn committed(&self, key: Hash32) -> Result<Option<&HistoryEntry>, Refusal> {
        match self.commits.get(&key) {
            Some(entry) if entry.key != key => Err(Refusal::Binding),
            entry => Ok(entry),
        }
    }
    /// Retained original inputs for startup clock reconstruction only.
    pub fn commits(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.commits.values()
    }
    /// Checked pending deliveries; work is proportional to pending records.
    pub fn pending(&mut self) -> Result<usize, Refusal> {
        match self.call(
            Operation::ReadOutbox(self.store),
            Trace::empty(Event::Request),
            &mut Ignore,
        )? {
            Response::ReadOutbox(Outcome::Ok(pending)) => Ok(pending.len()),
            _ => Err(Refusal::Unavailable),
        }
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
        self.execute_admitted(
            command,
            &mut |configured| Ok(configured.to_vec()),
            &|configured, stored| configured == stored,
            sink,
        )
    }
}
impl<H: Terminal> Durable<'_, '_, H> {
    /// Execute with a context admitted for this request, such as one carrying the
    /// server clock. The authenticated principal still selects its configured
    /// context: `admit` derives a new decision's context from it, and `replays`
    /// decides whether an original request's stored context belongs to it. A
    /// replay reuses the stored decision and never calls `admit`.
    pub fn execute_admitted(
        &mut self,
        command: &Command,
        admit: &mut Admit<'_>,
        replays: &dyn Fn(&[u8], &[u8]) -> bool,
        sink: &mut dyn TraceSink,
    ) -> Result<Vec<u8>, Refusal> {
        let configured = match self.context(command.principal) {
            Some(context) if command.contract == digest(self.authority.identity()) => {
                context.to_vec()
            }
            _ => return Err(Refusal::Binding),
        };
        let mut trace = Trace::request(command);
        let key = command.replay_key();
        let head = self.verified_head(trace, sink)?;
        let original = self.committed(key)?.cloned();
        let data = if let Some(entry) = &original {
            if entry.publication.command != command.bytes
                || !replays(&configured, &entry.publication.context)
                || Some(entry.version) != command.expected.checked_add(1)
            {
                return Err(Refusal::Replay);
            }
            entry.publication.clone()
        } else {
            if head.version != command.expected {
                return Err(Refusal::Conflict);
            }
            let context = admit(&configured)?;
            match self.authority.publish(Raw {
                state: &head.state,
                command: &command.bytes,
                context: &context,
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
            publication: data.clone(),
            mode: if original.is_some() {
                CommitMode::Unversioned
            } else {
                CommitMode::At(command.expected)
            },
        };
        // A known replay has already matched the original key and all inputs.
        // SQLite independently checks that exact publication again. Unknown
        // commit outcomes retain CommitProgram's reopen/history recovery path.
        let first = CommitProgram::start(invocation, CommitPolicy { retries: 2 });
        let committed = run::<CommitProgram>(&mut self.terminal, first, trace, sink);
        let receipt = match committed {
            Ok(Outcome::Ok(receipt)) => receipt,
            other => {
                self.head = None;
                return Err(match other {
                    Ok(Outcome::Refused(Code::Replay)) => Refusal::Replay,
                    Ok(Outcome::Refused(_)) => Refusal::Conflict,
                    _ => Refusal::Unavailable,
                });
            }
        };
        let actual = match self.call(Operation::ReadState(self.store), trace, sink) {
            Ok(Response::ReadState(Outcome::Ok(head))) => head,
            _ => {
                self.head = None;
                return Err(Refusal::Unavailable);
            }
        };
        if original.is_none() {
            if Some(actual.version) != head.version.checked_add(1)
                || actual.version != receipt.version
                || actual.chain != receipt.chain
                || actual.state != publication.poststate()
            {
                self.head = None;
                return Err(Refusal::Unavailable);
            }
            self.commits.insert(
                key,
                HistoryEntry {
                    key,
                    version: receipt.version,
                    publication: data,
                },
            );
        } else if actual != head {
            return Err(Refusal::Unavailable);
        }
        self.head = Some(actual);
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

struct Ignore;
impl TraceSink for Ignore {
    fn record(&mut self, _: Trace) {}
}
