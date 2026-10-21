//! A deterministic simulated world whose every fault is an explicit choice.
//!
//! The world holds the durable store D (state, history, replay keys,
//! receipts and outbox), each handle's volatile state V, the transport
//! queue, the receiver's durable deduplication R and the epoch clock. Each
//! fault point asks its [`Script`] to choose among a finite set: answer,
//! busy, an ambiguous or lost reply, a crash before or after the write,
//! drop, duplicate, a wake time or interruption, and a jitter draw. Only the
//! simulation chooses; the programs and the driver stay choice-free, so no
//! continuation is ever resumed twice.
//!
//! The simulated application is a one-byte counter: a publication's command
//! byte is added to the state, and every commit carries two deliveries.

use alloc::vec;
use alloc::vec::Vec;
use core::num::NonZeroU64;

use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};

use super::codec;
use super::driver::Binding;
use super::handler::{Handled, Terminal};
use super::port::{
    AuditedHistory, Code, CommitMode, CommitReceipt, Delivery, EpochTick, Fault, HistoryEntry,
    Operation, Outcome, PublicationData, Report, Response, Sent, Snapshot, Store, WaitError,
};
use super::registry::Port;
use crate::CommitStatus;

/// One exploration profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Profile {
    /// The programs' retry budget.
    pub retries: u8,
    /// Transport queue capacity.
    pub queue: usize,
    /// Largest clock tick.
    pub ticks: u64,
    /// Jitter draws are below this bound.
    pub jitter: NonZeroU64,
    /// Backoff ticks before a resend.
    pub backoff: u64,
}

const TWO: NonZeroU64 = match NonZeroU64::new(2) {
    Some(two) => two,
    None => NonZeroU64::MIN,
};

impl Profile {
    /// The frozen profile at one retry budget: queue capacity two, ticks
    /// 0 to 3, jitter below 2 and a backoff of one tick.
    #[must_use]
    pub const fn frozen(retries: u8) -> Self {
        Self {
            retries,
            queue: 2,
            ticks: 3,
            jitter: TWO,
            backoff: 1,
        }
    }

    /// Whether the profile stays within the frozen domains.
    #[must_use]
    pub fn well_formed(&self) -> bool {
        self.retries <= 2
            && (1..=2).contains(&self.queue)
            && self.ticks <= 3
            && self.jitter.get() <= 2
            && self.backoff <= self.ticks
    }
}

/// Identity of the simulated Authority.
pub const IDENTITY: &[u8] = b"simulated-authority";
/// Identity of the simulated store.
pub const STORE: Hash32 = Hash32::new([0x5a; 32]);
/// Number of handles on the store.
pub const HANDLES: usize = 2;
/// Every port the simulation covers.
pub const ALL_PORTS: &[Port] = &Port::ALL;
/// The store ports only, without transport or time.
pub const STORE_PORTS: &[Port] = &[
    Port::ReadState,
    Port::Commit,
    Port::ReadHistory,
    Port::ReadOutbox,
    Port::Acknowledge,
    Port::Reopen,
];

/// A handle on the simulated store, numbered from one.
#[must_use]
pub const fn store(handle: u32) -> Store {
    Store {
        identity: STORE,
        handle,
    }
}

fn sha256(parts: &[&[u8]]) -> Hash32 {
    let mut out = Vec::new();
    for part in parts {
        codec::bytes(&mut out, part);
    }
    RustCryptoSha256::hash(&out)
}

/// The simulated Authority's subject for these inputs.
#[must_use]
pub fn subject(prestate: &[u8], command: &[u8], context: &[u8]) -> Vec<u8> {
    sha256(&[IDENTITY, prestate, command, context])
        .as_bytes()
        .to_vec()
}

/// A genuine publication adding `body` to the counter in `prestate`.
#[must_use]
pub fn publication(prestate: &[u8], body: u8) -> PublicationData {
    PublicationData {
        prestate: prestate.to_vec(),
        command: vec![body],
        context: vec![1],
        identity: IDENTITY.to_vec(),
        subject: subject(prestate, &[body], &[1]),
    }
}

/// The two deliveries of the commit that produced `chain` and `state`.
#[must_use]
pub fn deliveries(version: u64, chain: Hash32, state: &[u8]) -> [Delivery; 2] {
    let delivery = |ordinal: u32| {
        let payload = vec![state.first().copied().unwrap_or(0), ordinal as u8];
        Delivery {
            id: sha256(&[b"delivery", chain.as_bytes(), &ordinal.to_be_bytes()]),
            version,
            lane: 1,
            ordinal,
            destination: b"receiver".to_vec(),
            payload_sha256: RustCryptoSha256::hash(&payload),
            payload,
        }
    };
    [delivery(0), delivery(1)]
}

/// The store's durable state D.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Durable {
    /// Current state envelope.
    pub state: Vec<u8>,
    /// History chain tip.
    pub chain: Hash32,
    /// Every commit, oldest first.
    pub commits: Vec<HistoryEntry>,
    /// The chain after each commit.
    pub receipts: Vec<Hash32>,
    /// Replay keys, one per commit, in commit order.
    pub replays: Vec<Hash32>,
    /// Every delivery record and whether it is acknowledged.
    pub outbox: Vec<(Delivery, bool)>,
}

impl Durable {
    /// The genesis store: counter zero, no commits.
    #[must_use]
    pub fn genesis() -> Self {
        Self {
            state: vec![0],
            chain: sha256(&[b"genesis", IDENTITY]),
            commits: Vec::new(),
            receipts: Vec::new(),
            replays: Vec::new(),
            outbox: Vec::new(),
        }
    }

    /// Number of commits.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.commits.len() as u64
    }

    /// The checked head.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            version: self.version(),
            identity: IDENTITY.to_vec(),
            state: self.state.clone(),
            root: RustCryptoSha256::hash(&self.state),
            chain: self.chain,
        }
    }

    /// The audited history.
    #[must_use]
    pub fn history(&self) -> AuditedHistory {
        AuditedHistory {
            head: self.snapshot(),
            commits: self.commits.clone(),
        }
    }

    /// The pending deliveries, in commit, lane and ordinal order.
    #[must_use]
    pub fn pending(&self) -> Vec<Delivery> {
        let mut pending = Vec::new();
        for (delivery, acknowledged) in &self.outbox {
            if !acknowledged {
                pending.push(delivery.clone());
            }
        }
        pending
    }
}

/// The whole simulated world.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct World {
    /// D.
    pub durable: Durable,
    /// V: each handle's cached version; `None` once dropped by a crash.
    pub handles: [Option<u64>; HANDLES],
    /// Copies in transit, oldest first.
    pub queue: Vec<Delivery>,
    /// R: delivery IDs the receiver has deduplicated, one transaction with
    /// each effect.
    pub dedup: Vec<Hash32>,
    /// R: every effect the receiver applied, by delivery ID.
    pub effects: Vec<Hash32>,
    /// Every delivery ID the transport ever queued.
    pub sent: Vec<Hash32>,
    /// The last tick the clock handed out.
    pub tick: u64,
    /// Answers whose reply was lost, which may still arrive late.
    pub late: Vec<(Binding, Response)>,
}

impl World {
    /// A world around `durable` with fresh handles and an idle transport.
    #[must_use]
    pub fn new(durable: Durable) -> Self {
        let version = durable.version();
        Self {
            durable,
            handles: [Some(version); HANDLES],
            queue: Vec::new(),
            dedup: Vec::new(),
            effects: Vec::new(),
            sent: Vec::new(),
            tick: 0,
            late: Vec::new(),
        }
    }

    /// SHA-256 of the world's canonical bytes.
    #[must_use]
    pub fn digest(&self) -> Hash32 {
        let mut out = Vec::new();
        let durable = &self.durable;
        codec::bytes(&mut out, &durable.state);
        codec::hash(&mut out, durable.chain);
        for entry in &durable.commits {
            codec::hash(&mut out, entry.key);
            out.extend_from_slice(&entry.version.to_be_bytes());
            let publication = &entry.publication;
            for part in [
                &publication.prestate,
                &publication.command,
                &publication.context,
                &publication.identity,
                &publication.subject,
            ] {
                codec::bytes(&mut out, part);
            }
        }
        for hash in durable.receipts.iter().chain(&durable.replays) {
            codec::hash(&mut out, *hash);
        }
        for (delivery, acknowledged) in &durable.outbox {
            delivery_bytes(&mut out, delivery);
            out.push(u8::from(*acknowledged));
        }
        for cached in self.handles {
            out.extend_from_slice(&cached.map_or(u64::MAX, |version| version).to_be_bytes());
        }
        for delivery in &self.queue {
            delivery_bytes(&mut out, delivery);
        }
        for hash in self.dedup.iter().chain(&self.effects).chain(&self.sent) {
            codec::hash(&mut out, *hash);
        }
        out.extend_from_slice(&self.tick.to_be_bytes());
        for (binding, response) in &self.late {
            codec::bytes(&mut out, &codec::binding(binding));
            codec::bytes(&mut out, &codec::response(response));
        }
        RustCryptoSha256::hash(&out)
    }
}

fn delivery_bytes(out: &mut Vec<u8>, delivery: &Delivery) {
    codec::hash(out, delivery.id);
    out.extend_from_slice(&delivery.version.to_be_bytes());
    out.push(delivery.lane);
    out.extend_from_slice(&delivery.ordinal.to_be_bytes());
    codec::bytes(out, &delivery.destination);
    codec::bytes(out, &delivery.payload);
    codec::hash(out, delivery.payload_sha256);
}

/// The choices of one schedule, consumed in order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Script {
    choices: Vec<u8>,
    at: usize,
    fault_free: bool,
    /// The option count of the first choice the schedule does not make.
    pub needed: Option<usize>,
    /// Whether a recorded choice was out of range.
    pub malformed: bool,
}

impl Script {
    /// A schedule that makes exactly these choices.
    #[must_use]
    pub const fn new(choices: Vec<u8>) -> Self {
        Self {
            choices,
            at: 0,
            fault_free: false,
            needed: None,
            malformed: false,
        }
    }

    /// A schedule that always takes the first, fault-free option.
    #[must_use]
    pub const fn fault_free() -> Self {
        Self {
            choices: Vec::new(),
            at: 0,
            fault_free: true,
            needed: None,
            malformed: false,
        }
    }

    /// Chooses one of `options`, or `None` when the schedule ends here.
    pub fn choose(&mut self, options: usize) -> Option<usize> {
        if options == 1 || self.fault_free {
            return Some(0);
        }
        match self.choices.get(self.at) {
            Some(choice) if usize::from(*choice) < options => {
                self.at += 1;
                Some(usize::from(*choice))
            }
            Some(_) => {
                self.malformed = true;
                None
            }
            None => {
                self.needed = Some(options);
                None
            }
        }
    }

    /// The choices made so far.
    #[must_use]
    pub fn made(&self) -> &[u8] {
        &self.choices[..self.at]
    }
}

/// What a commit would do on a fault-free store.
enum Decision {
    Refuse(Code),
    Replay(CommitReceipt),
    Apply(Durable, CommitReceipt),
}

fn decide(durable: &Durable, key: Hash32, data: &PublicationData, mode: CommitMode) -> Decision {
    let version = durable.version();
    // V2 checks the expected version before it looks the replay key up.
    if let CommitMode::At(expected) | CommitMode::BoundedAt(expected, _) = mode
        && expected != version
    {
        return Decision::Refuse(Code::Concurrent);
    }
    if let Some(index) = durable.replays.iter().position(|replay| *replay == key) {
        let Some(entry) = durable.commits.get(index) else {
            return Decision::Refuse(Code::History);
        };
        if entry.publication != *data {
            return Decision::Refuse(Code::Replay);
        }
        return Decision::Replay(CommitReceipt {
            version: entry.version,
            chain: durable.receipts[index],
            status: CommitStatus::IdempotentReplay,
        });
    }
    if data.identity != IDENTITY
        || data.subject != subject(&data.prestate, &data.command, &data.context)
    {
        return Decision::Refuse(Code::Identity);
    }
    if data.prestate != durable.state {
        return Decision::Refuse(Code::PreState);
    }
    let size = data.prestate.len() + data.command.len() + data.context.len();
    if let CommitMode::BoundedAt(_, cap) = mode
        && size > cap
    {
        return Decision::Refuse(Code::Capacity);
    }
    let (Some(count), Some(body)) = (durable.state.first(), data.command.first()) else {
        return Decision::Refuse(Code::Range);
    };
    let Some(next) = count.checked_add(*body) else {
        return Decision::Refuse(Code::Range);
    };
    let mut after = durable.clone();
    after.state = vec![next];
    after.chain = sha256(&[durable.chain.as_bytes(), &data.subject]);
    let version = version + 1;
    after.commits.push(HistoryEntry {
        key,
        version,
        publication: data.clone(),
    });
    after.receipts.push(after.chain);
    after.replays.push(key);
    for delivery in deliveries(version, after.chain, &after.state) {
        after.outbox.push((delivery, false));
    }
    let receipt = CommitReceipt {
        version,
        chain: after.chain,
        status: CommitStatus::Committed,
    };
    Decision::Apply(after, receipt)
}

/// The simulation handler: terminal for its ports over one world.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Simulation {
    /// The world.
    pub world: World,
    /// The profile.
    pub profile: Profile,
    /// The ports this simulation covers.
    pub ports: &'static [Port],
    /// The schedule's choices.
    pub script: Script,
}

fn not_applied(port: Port, fault: Fault) -> Response {
    match port {
        Port::ReadState => Response::ReadState(Outcome::NotApplied(fault)),
        Port::Commit => Response::Commit(Outcome::NotApplied(fault)),
        Port::ReadHistory => Response::ReadHistory(Outcome::NotApplied(fault)),
        Port::ReadOutbox => Response::ReadOutbox(Outcome::NotApplied(fault)),
        Port::Acknowledge => Response::Acknowledge(Outcome::NotApplied(fault)),
        Port::Reopen => Response::Reopen(Outcome::NotApplied(fault)),
        Port::Send => Response::Send(Sent::NotSent(Code::Delivery)),
        Port::Clock => Response::Clock(Err(fault)),
        Port::Random => Response::Random(Err(fault)),
        Port::Wait => Response::Wait(Err(WaitError::Failed(fault))),
    }
}

fn unknown(port: Port) -> Response {
    match port {
        Port::Commit => Response::Commit(Outcome::Unknown(Fault::Io)),
        Port::Acknowledge => Response::Acknowledge(Outcome::Unknown(Fault::Io)),
        other => not_applied(other, Fault::Io),
    }
}

fn refused(port: Port, code: Code) -> Response {
    match port {
        Port::ReadState => Response::ReadState(Outcome::Refused(code)),
        Port::Commit => Response::Commit(Outcome::Refused(code)),
        Port::ReadHistory => Response::ReadHistory(Outcome::Refused(code)),
        Port::ReadOutbox => Response::ReadOutbox(Outcome::Refused(code)),
        Port::Acknowledge => Response::Acknowledge(Outcome::Refused(code)),
        Port::Reopen => Response::Reopen(Outcome::Refused(code)),
        Port::Send => Response::Send(Sent::NotSent(code)),
        other => not_applied(other, Fault::Io),
    }
}

impl Simulation {
    /// A simulation of `world` that covers `ports`.
    #[must_use]
    pub const fn new(
        world: World,
        profile: Profile,
        ports: &'static [Port],
        script: Script,
    ) -> Self {
        Self {
            world,
            profile,
            ports,
            script,
        }
    }

    fn crash(&mut self, handle: usize) -> Handled {
        self.world.handles[handle] = None;
        Handled::Crash
    }

    /// A store operation that writes nothing: answer, busy or crash.
    fn read(&mut self, handle: usize, port: Port, answer: Response) -> Handled {
        match self.script.choose(3) {
            None => Handled::Suspend,
            Some(0) => {
                self.world.handles[handle] = Some(self.world.durable.version());
                Handled::Answer(answer)
            }
            Some(1) => Handled::Answer(not_applied(port, Fault::Busy)),
            Some(_) => self.crash(handle),
        }
    }

    /// A store write: answer, busy, an ambiguous reply that did not apply,
    /// a lost reply that did, or a crash before or after it.
    fn write(
        &mut self,
        binding: &Binding,
        handle: usize,
        after: Durable,
        answer: Response,
    ) -> Handled {
        let port = binding.port;
        match self.script.choose(6) {
            None => Handled::Suspend,
            Some(0) => {
                self.world.durable = after;
                self.world.handles[handle] = Some(self.world.durable.version());
                Handled::Answer(answer)
            }
            Some(1) => Handled::Answer(not_applied(port, Fault::Busy)),
            Some(2) => Handled::Answer(unknown(port)),
            Some(3) => {
                self.world.durable = after;
                self.world.late.push((binding.clone(), answer));
                Handled::Answer(unknown(port))
            }
            Some(4) => self.crash(handle),
            Some(_) => {
                self.world.durable = after;
                self.crash(handle)
            }
        }
    }

    fn send(&mut self, binding: &Binding, handle: usize, delivery: &Delivery) -> Handled {
        let room = self.profile.queue.saturating_sub(self.world.queue.len());
        let report = Response::Send(Sent::Sent(Report {
            delivery_id: delivery.id,
            payload_sha256: delivery.payload_sha256,
        }));
        let not_sent = Response::Send(Sent::NotSent(Code::Delivery));
        if room == 0 {
            return match self.script.choose(2) {
                None => Handled::Suspend,
                Some(0) => Handled::Answer(not_sent),
                Some(_) => self.crash(handle),
            };
        }
        let options = if room >= 2 { 7 } else { 6 };
        let lost = Response::Send(Sent::Unknown(Fault::Io));
        match self.script.choose(options) {
            None => Handled::Suspend,
            Some(0) => {
                self.carry(delivery);
                Handled::Answer(report)
            }
            Some(1) => Handled::Answer(not_sent),
            // Dropped in transit.
            Some(2) => Handled::Answer(lost),
            // Carried, with the report lost.
            Some(3) => {
                self.carry(delivery);
                self.world.late.push((binding.clone(), report));
                Handled::Answer(lost)
            }
            Some(4) => self.crash(handle),
            Some(5) => {
                self.carry(delivery);
                self.crash(handle)
            }
            Some(_) => {
                self.carry(delivery);
                self.carry(delivery);
                Handled::Answer(report)
            }
        }
    }

    fn carry(&mut self, delivery: &Delivery) {
        self.world.queue.push(delivery.clone());
        if !self.world.sent.contains(&delivery.id) {
            self.world.sent.push(delivery.id);
        }
    }

    fn acknowledge(
        &mut self,
        binding: &Binding,
        handle: usize,
        id: Hash32,
        payload_sha256: Hash32,
    ) -> Handled {
        let durable = &self.world.durable;
        let Some(index) = durable
            .outbox
            .iter()
            .position(|(delivery, _)| delivery.id == id)
        else {
            return self.read(
                handle,
                Port::Acknowledge,
                Response::Acknowledge(Outcome::Refused(Code::UnknownDelivery)),
            );
        };
        let (delivery, acknowledged) = &durable.outbox[index];
        let code = if delivery.payload_sha256 != payload_sha256 {
            Some(Code::PayloadMismatch)
        } else if *acknowledged {
            Some(Code::AlreadyAcknowledged)
        } else {
            None
        };
        if let Some(code) = code {
            return self.read(
                handle,
                Port::Acknowledge,
                Response::Acknowledge(Outcome::Refused(code)),
            );
        }
        let mut after = durable.clone();
        after.outbox[index].1 = true;
        self.write(
            binding,
            handle,
            after,
            Response::Acknowledge(Outcome::Ok(())),
        )
    }

    /// Receives the queued copy at `index`: the effect and its deduplication
    /// row share one receiver transaction.
    pub fn receive(&mut self, index: usize) {
        if index >= self.world.queue.len() {
            return;
        }
        let delivery = self.world.queue.remove(index);
        if !self.world.dedup.contains(&delivery.id) {
            self.world.dedup.push(delivery.id);
            self.world.effects.push(delivery.id);
        }
    }
}

impl Terminal for Simulation {
    fn ports(&self) -> &'static [Port] {
        self.ports
    }

    fn handle(&mut self, binding: &Binding, operation: &Operation) -> Handled {
        let port = operation.port();
        if !self.ports.contains(&port) {
            return Handled::Answer(refused(port, Code::Identity));
        }
        let handle = match operation.store() {
            Some(store) if store.identity == STORE && (1..=2).contains(&store.handle) => {
                Some(store.handle as usize - 1)
            }
            Some(_) => return Handled::Answer(refused(port, Code::Identity)),
            None => None,
        };
        let durable = &self.world.durable;
        match (operation, handle) {
            (Operation::ReadState(_), Some(handle)) => {
                let answer = Response::ReadState(Outcome::Ok(durable.snapshot()));
                self.read(handle, port, answer)
            }
            (Operation::ReadHistory(_), Some(handle)) => {
                let answer = Response::ReadHistory(Outcome::Ok(durable.history()));
                self.read(handle, port, answer)
            }
            (Operation::ReadOutbox(_), Some(handle)) => {
                let answer = Response::ReadOutbox(Outcome::Ok(durable.pending()));
                self.read(handle, port, answer)
            }
            (Operation::Reopen(_), Some(handle)) => {
                let answer = Response::Reopen(Outcome::Ok(durable.snapshot()));
                self.read(handle, port, answer)
            }
            (Operation::Commit(_, key, data, mode), Some(handle)) => {
                match decide(durable, *key, data, *mode) {
                    Decision::Refuse(code) => {
                        self.read(handle, port, Response::Commit(Outcome::Refused(code)))
                    }
                    Decision::Replay(receipt) => {
                        self.read(handle, port, Response::Commit(Outcome::Ok(receipt)))
                    }
                    Decision::Apply(after, receipt) => self.write(
                        binding,
                        handle,
                        after,
                        Response::Commit(Outcome::Ok(receipt)),
                    ),
                }
            }
            (Operation::Send(_, delivery), Some(handle)) => self.send(binding, handle, delivery),
            (Operation::Acknowledge(_, id, payload_sha256), Some(handle)) => {
                self.acknowledge(binding, handle, *id, *payload_sha256)
            }
            (Operation::Clock, _) => {
                let first = self.world.tick;
                let Some(offset) = self.script.choose(choices(first, self.profile.ticks)) else {
                    return Handled::Suspend;
                };
                self.world.tick = first + offset as u64;
                Handled::Answer(Response::Clock(Ok(EpochTick {
                    epoch: binding.id.epoch,
                    tick: self.world.tick,
                })))
            }
            (Operation::Random(_, bound), _) => {
                let options = usize::try_from(bound.get()).unwrap_or(usize::MAX).min(256);
                match self.script.choose(options) {
                    None => Handled::Suspend,
                    Some(draw) => Handled::Answer(Response::Random(Ok(draw as u64))),
                }
            }
            (Operation::Wait(deadline), _) => {
                let first = deadline.tick.max(self.world.tick);
                let wakes = if deadline.epoch == binding.id.epoch && first <= self.profile.ticks {
                    choices(first, self.profile.ticks)
                } else {
                    0
                };
                // The last option is an explicit interruption.
                match self.script.choose(wakes + 1) {
                    None => Handled::Suspend,
                    Some(offset) if offset < wakes => {
                        self.world.tick = first + offset as u64;
                        Handled::Answer(Response::Wait(Ok(EpochTick {
                            epoch: binding.id.epoch,
                            tick: self.world.tick,
                        })))
                    }
                    Some(_) => Handled::Answer(Response::Wait(Err(WaitError::Interrupted))),
                }
            }
            _ => Handled::Answer(refused(port, Code::Identity)),
        }
    }
}

/// The number of ticks from `first` to `last`, inclusive.
fn choices(first: u64, last: u64) -> usize {
    usize::try_from(last.saturating_sub(first).saturating_add(1)).unwrap_or(usize::MAX)
}
