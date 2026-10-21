//! Bounded depth-first and breadth-first exploration of every fault schedule.
//!
//! A schedule is the sequence of simulated choices: which actor steps next,
//! whether a crashed actor restarts, which queued copy the receiver takes,
//! whether a lost reply arrives late, and every fault point. Continuations
//! are never resumed twice, so each node re-runs the system from the start
//! with its choice prefix. States at scheduler choices are fingerprinted and
//! expanded once. Exploration is `complete` only when no frontier remains;
//! a budget stop is `incomplete`, and a malformed profile, choice or
//! program is `refused`. Exhaustion is never a pass.

use alloc::boxed::Box;
use alloc::collections::{BTreeSet, VecDeque};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;

use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};

use super::codec;
use super::driver::{Driver, Fatal, Reply};
use super::handler::{self, Handled, Layer, Stack, Unbound};
use super::laws::{self, LawFailure, Laws};
use super::policy::{self, EffectKey, Monitor, Policy};
use super::port::{CommitMode, CommitReceipt, Operation, Outcome, PublicationData, Response};
use super::program::{
    CommitInvocation, CommitPolicy, CommitProgram, DeliveryInvocation, DeliveryPolicy,
    DeliveryProgram, Program, Step,
};
use super::record::{self, Ended, Manifest, Transcript};
use super::registry::REGISTRY;
use super::simulation::{self, Durable, Profile, Script, Simulation, World};

/// Which protocol a run exercises.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scenario {
    /// Two commit programs on two handles, with two keys and two bodies.
    Commit,
    /// The delivery program and the receiver after one commit with two
    /// deliveries.
    Delivery,
}

/// Frontier order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Order {
    /// Depth first.
    Depth,
    /// Breadth first.
    Breadth,
}

/// The layers in front of the simulation, innermost first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layers {
    /// No layer.
    Bare,
    /// A record layer.
    Record,
    /// A record layer behind an identity forwarder.
    IdentityRecord,
}

/// Counts of one exploration.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stats {
    /// Distinct states at scheduler choices.
    pub states: u64,
    /// Complete schedules run to the end.
    pub schedules: u64,
    /// Runs, one per explored node.
    pub runs: u64,
    /// Nodes cut because their state was already expanded.
    pub pruned: u64,
    /// Longest schedule, in choices.
    pub deepest: usize,
    /// Transcripts replayed handler-free.
    pub replays: u64,
    /// Crashes, restarts after a crash, copies received and late replies
    /// refused, each counted once.
    pub tally: Tally,
    /// Explicit policy refusals witnessed at newly explored transitions.
    pub policy_refusals: u64,
    /// Program outcomes across finished schedules; exploration closure is
    /// separate from whether any individual action completed.
    pub actions: Actions,
}

/// No action silently disappears: success, explicit refusal, or unresolved.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Actions {
    /// Protocol success, such as a matching acknowledgement, not proof of
    /// external execution.
    pub committed: u64,
    /// Explicit refusal.
    pub refused: u64,
    /// Crash, Unknown, or NotApplied; progress remains unresolved.
    pub incomplete: u64,
}

impl Actions {
    fn count<T>(&mut self, outcome: Option<&Outcome<T>>) {
        match outcome {
            Some(Outcome::Ok(_)) => self.committed += 1,
            Some(Outcome::Refused(_)) => self.refused += 1,
            _ => self.incomplete += 1,
        }
    }
}

/// Events of an exploration, each counted in the run that first reaches it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Tally {
    /// Crashes.
    pub crashes: u64,
    /// Restarts after a crash.
    pub restarts: u64,
    /// Copies the receiver took.
    pub received: u64,
    /// Late replies the drivers refused.
    pub late: u64,
}

/// Why an exploration was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// The profile is outside the frozen domains.
    Profile,
    /// A schedule's choice is out of range.
    Choice,
    /// A terminal does not cover a program's port.
    Unbound(Unbound),
    /// The driver stopped a program.
    Fatal(Fatal),
}

/// How an exploration ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exploration {
    /// Every schedule of the profile was explored.
    Complete,
    /// The budget stopped it with this many nodes unexplored.
    Incomplete(usize),
    /// It was refused before or during the run.
    Refused(Refusal),
}

/// The result of one exploration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    /// Source identity of the explorer, including its policy retry path.
    pub source: Hash32,
    /// Bound policy identities, in stack order; empty for the original explorer.
    pub policies: Vec<Hash32>,
    /// Premises and nonclaims for this policy product.
    pub premises: &'static [&'static str],
    /// How it ended.
    pub exploration: Exploration,
    /// Its counts.
    pub stats: Stats,
    /// Law checks and witnesses.
    pub laws: Laws,
    /// Every distinct durable store reached.
    pub stores: Vec<Durable>,
    /// SHA-256 over the sorted fingerprints of every final state.
    pub finals: Hash32,
    /// Owned resumable search state on budget exhaustion; no frontier is discarded.
    pub checkpoint: Option<Checkpoint>,
}

/// Source/profile-bound search state. Its private owned fields cannot be
/// altered through an advisory Report. Resume consumes it; clone explicitly
/// to branch a search. This is an in-memory checkpoint, not a disk format.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checkpoint {
    source: Hash32,
    binding: Hash32,
    scenario: Scenario,
    profile: Profile,
    layers: Layers,
    order: Order,
    policies: Vec<Policy>,
    frontier: VecDeque<Vec<u8>>,
    visited: BTreeSet<Hash32>,
    finals: BTreeSet<Hash32>,
    progress: Box<Report>,
}

fn search_source() -> Hash32 {
    super::registry::source_digest(&[
        ("explore.rs", include_bytes!("explore.rs")),
        ("simulation.rs", include_bytes!("simulation.rs")),
        ("laws.rs", include_bytes!("laws.rs")),
        ("program.rs", include_bytes!("program.rs")),
        ("driver.rs", include_bytes!("driver.rs")),
        ("handler.rs", include_bytes!("handler.rs")),
        ("record.rs", include_bytes!("record.rs")),
        ("policy.rs", include_bytes!("policy.rs")),
        ("registry.rs", include_bytes!("registry.rs")),
        ("port.rs", include_bytes!("port.rs")),
        ("codec.rs", include_bytes!("codec.rs")),
    ])
}

fn search_binding(
    scenario: Scenario,
    profile: Profile,
    layers: Layers,
    order: Order,
    policies: &[Policy],
) -> Hash32 {
    let mut bytes = profile.bytes();
    bytes.extend_from_slice(&[scenario as u8, layers as u8, order as u8]);
    for policy in policies {
        codec::hash(&mut bytes, policy.identity());
    }
    RustCryptoSha256::hash(&bytes)
}

/// A broken law or invariant, with the schedule that reproduces it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Violation {
    /// Bound policy identities, in stack order.
    pub policies: Vec<Hash32>,
    /// Premises and nonclaims of the checked product.
    pub premises: &'static [&'static str],
    /// The choices that reach it.
    pub schedule: Vec<u8>,
    /// The invariant that broke.
    pub what: &'static str,
    /// The law, when a law broke.
    pub law: Option<LawFailure>,
}

#[derive(Clone, Copy, Debug)]
enum Role {
    Commit {
        handle: u32,
        key: Hash32,
        body: u8,
        mode: CommitMode,
    },
    Delivery {
        handle: u32,
    },
}

#[derive(Debug)]
enum Running {
    Commit(Driver<CommitProgram>),
    Delivery(Driver<DeliveryProgram>),
}

#[derive(Debug)]
struct Incarnation {
    transcripts: Vec<Transcript>,
    start: Vec<u8>,
    epoch: u64,
    recover: bool,
    ended: Ended,
}

#[derive(Debug)]
struct Actor {
    role: Role,
    publication: PublicationData,
    running: Option<Running>,
    stack: Stack,
    start: Vec<u8>,
    epoch: u64,
    recover: bool,
    incarnation: u8,
    crashed: bool,
    commit: Option<Outcome<CommitReceipt>>,
    delivery: Option<Outcome<u64>>,
    done: Vec<Incarnation>,
}

enum Failure {
    Violation(&'static str, Option<LawFailure>),
    Refused(Refusal),
}

enum Run {
    Suspended {
        options: usize,
        fingerprint: Option<Hash32>,
    },
    Finished(Hash32),
}

const KEYS: [Hash32; 2] = [Hash32::new([0x11; 32]), Hash32::new([0x22; 32])];
const RESTARTS: u8 = 1;

fn commit_done(outcome: &Outcome<CommitReceipt>) -> Vec<u8> {
    codec::response(&Response::Commit(outcome.clone()))
}

fn delivery_done(outcome: &Outcome<u64>) -> Vec<u8> {
    let count = match outcome {
        Outcome::Ok(count) => Some(*count),
        _ => None,
    };
    codec::outcome(outcome, count)
}

struct System {
    tally: Tally,
    profile: Profile,
    layers: Layers,
    simulation: Simulation,
    actors: Vec<Actor>,
    prefix: Vec<u8>,
    policies: Vec<Policy>,
    // Independent multiset oracle; unlike World.sent this preserves repeats.
    attempts: Vec<EffectKey>,
    policy_refusals: u64,
    manifest: Manifest,
}

impl System {
    fn new(
        scenario: Scenario,
        profile: Profile,
        layers: Layers,
        prefix: &[u8],
        policies: &[Policy],
    ) -> Result<Self, Failure> {
        let manifest = Manifest::current(Vec::new(), Vec::new(), Hash32::ZERO, Vec::new());
        Self::bound(scenario, profile, layers, prefix, policies, &manifest)
    }

    fn bound(
        scenario: Scenario,
        profile: Profile,
        layers: Layers,
        prefix: &[u8],
        policies: &[Policy],
        manifest: &Manifest,
    ) -> Result<Self, Failure> {
        let genesis = Durable::genesis();
        let (durable, roles) = match scenario {
            Scenario::Commit => (
                genesis,
                vec![
                    Role::Commit {
                        handle: 1,
                        key: KEYS[0],
                        body: 1,
                        mode: CommitMode::At(0),
                    },
                    Role::Commit {
                        handle: 2,
                        key: KEYS[1],
                        body: 2,
                        mode: CommitMode::Unversioned,
                    },
                ],
            ),
            Scenario::Delivery => (
                laws::committed(&genesis, KEYS[0], 1),
                vec![Role::Delivery { handle: 1 }],
            ),
        };
        let initial = durable.state.clone();
        let simulation = Simulation::new(
            World::new(durable),
            profile,
            simulation::ALL_PORTS,
            Script::new(prefix.to_vec()),
        );
        let mut system = Self {
            tally: Tally::default(),
            profile,
            layers,
            simulation,
            actors: Vec::new(),
            prefix: prefix.to_vec(),
            policies: policies.to_vec(),
            attempts: Vec::new(),
            policy_refusals: 0,
            manifest: manifest.clone(),
        };
        for role in roles {
            let body = match role {
                Role::Commit { body, .. } => body,
                Role::Delivery { .. } => 0,
            };
            system.actors.push(Actor {
                role,
                publication: simulation::publication(&initial, body),
                running: None,
                stack: Stack::default(),
                start: Vec::new(),
                epoch: 0,
                recover: false,
                incarnation: 0,
                crashed: false,
                commit: None,
                delivery: None,
                done: Vec::new(),
            });
        }
        for index in 0..system.actors.len() {
            system.launch(index, false)?;
        }
        Ok(system)
    }

    fn launch(&mut self, index: usize, recover: bool) -> Result<(), Failure> {
        let retries = self.profile.retries;
        let ports = self.simulation.ports;
        let initial = if self.layers == Layers::Bare {
            Hash32::ZERO
        } else {
            self.simulation.world.digest()
        };
        let actor = &mut self.actors[index];
        let policy_layers = if actor.incarnation == 0 {
            self.policies
                .iter()
                .cloned()
                .map(Monitor::new)
                .map(Layer::Policy)
                .collect()
        } else {
            actor.stack.policies()
        };
        let epoch = (index as u64 + 1) * 16 + u64::from(actor.incarnation);
        let mut start = Vec::new();
        let running = match actor.role {
            Role::Commit {
                handle, key, mode, ..
            } => {
                handler::bind(ports, CommitProgram::PORTS)
                    .map_err(|unbound| Failure::Refused(Refusal::Unbound(unbound)))?;
                let invocation = CommitInvocation {
                    epoch,
                    store: simulation::store(handle),
                    key,
                    publication: actor.publication.clone(),
                    mode,
                };
                start.extend_from_slice(b"commit");
                start.extend_from_slice(&[u8::from(recover), retries]);
                start.extend_from_slice(&codec::operation(&Operation::Commit(
                    invocation.store,
                    key,
                    invocation.publication.clone(),
                    mode,
                )));
                let policy = CommitPolicy { retries };
                let first = if recover {
                    CommitProgram::recover(invocation, policy)
                } else {
                    CommitProgram::start(invocation, policy)
                };
                Running::Commit(
                    Driver::start(REGISTRY, epoch, first)
                        .map_err(|fatal| Failure::Refused(Refusal::Fatal(fatal)))?,
                )
            }
            Role::Delivery { handle } => {
                handler::bind(ports, DeliveryProgram::PORTS)
                    .map_err(|unbound| Failure::Refused(Refusal::Unbound(unbound)))?;
                let invocation = DeliveryInvocation {
                    epoch,
                    store: simulation::store(handle),
                };
                start.extend_from_slice(b"delivery");
                start.extend_from_slice(&[u8::from(recover), retries]);
                start.extend_from_slice(&self.profile.backoff.to_be_bytes());
                start.extend_from_slice(&self.profile.jitter.get().to_be_bytes());
                start
                    .extend_from_slice(&codec::operation(&Operation::ReadOutbox(invocation.store)));
                let first = DeliveryProgram::start(invocation, self.policy());
                Running::Delivery(
                    Driver::start(REGISTRY, epoch, first)
                        .map_err(|fatal| Failure::Refused(Refusal::Fatal(fatal)))?,
                )
            }
        };
        let mut names = match self.layers {
            Layers::Bare => vec!["simulation"],
            Layers::Record => vec!["record", "simulation"],
            Layers::IdentityRecord => vec!["identity", "record", "simulation"],
        };
        if !policy_layers.is_empty() {
            let binding = Stack {
                layers: policy_layers.clone(),
            }
            .policy_bytes();
            codec::bytes(&mut start, b"zeno-fcis/effect-policy-stack/1");
            codec::bytes(&mut start, &binding);
            for _ in &policy_layers {
                names.insert(0, "policy");
            }
        }
        let mut configuration = self.profile.bytes();
        for layer in &policy_layers {
            if let Layer::Policy(monitor) = layer {
                codec::hash(&mut configuration, monitor.identity());
            }
        }
        let record = || {
            let mut manifest = self.manifest.clone();
            manifest.stack = names.clone();
            manifest.schedule = self.prefix.clone();
            manifest.initial = initial;
            manifest.start = start.clone();
            manifest.configuration = configuration.clone();
            Layer::Record(Transcript::new(manifest))
        };
        let mut layers = policy_layers;
        layers.extend(match self.layers {
            Layers::Bare => Vec::new(),
            Layers::Record => vec![record()],
            Layers::IdentityRecord => vec![Layer::Identity, record()],
        });
        let stack = Stack { layers };
        let actor = &mut self.actors[index];
        actor.running = Some(running);
        actor.stack = stack;
        actor.start = start;
        actor.epoch = epoch;
        actor.recover = recover;
        actor.crashed = false;
        Ok(())
    }

    fn policy(&self) -> DeliveryPolicy {
        DeliveryPolicy {
            retries: self.profile.retries,
            backoff: self.profile.backoff,
            jitter: self.profile.jitter,
        }
    }

    /// Digest of everything that decides the rest of the run: the world,
    /// each actor's driver, incarnation and outcome, and the pending choice.
    /// Transcripts are left out: they record the past and decide nothing.
    fn fingerprint(&self, pending: &str) -> Hash32 {
        let mut text = String::new();
        let _ = write!(text, "{pending}|{}", self.simulation.world.digest());
        for actor in &self.actors {
            let _ = write!(
                text,
                "|{:?}|{}|{}|{:?}|{:?}",
                actor.running, actor.incarnation, actor.crashed, actor.commit, actor.delivery
            );
            if !self.policies.is_empty() {
                let _ = write!(text, "|{:?}", actor.stack.policy_bytes());
            }
        }
        if !self.policies.is_empty() {
            let _ = write!(text, "|{:?}", self.attempts);
        }
        RustCryptoSha256::hash(text.as_bytes())
    }

    fn finish(&mut self, index: usize, ended: Ended) {
        let actor = &mut self.actors[index];
        let mut transcripts = Vec::new();
        for transcript in actor.stack.transcripts() {
            transcripts.push(transcript.clone());
        }
        actor.done.push(Incarnation {
            transcripts,
            start: actor.start.clone(),
            epoch: actor.epoch,
            recover: actor.recover,
            ended,
        });
    }

    /// Runs one scheduled step of actor `index`.
    fn step(
        &mut self,
        index: usize,
        laws: &mut Laws,
        stores: &mut Vec<Durable>,
    ) -> Result<bool, Failure> {
        let request = match &self.actors[index].running {
            Some(Running::Commit(driver)) => driver.request().map(|(b, o)| (b.clone(), o.clone())),
            Some(Running::Delivery(driver)) => {
                driver.request().map(|(b, o)| (b.clone(), o.clone()))
            }
            None => None,
        };
        let Some((binding, operation)) = request else {
            return Ok(true);
        };
        let check = self.simulation.script.made().len() + 1 >= self.prefix.len();
        let before = if check {
            Some(self.simulation.world.clone())
        } else {
            None
        };
        let handled = self.actors[index]
            .stack
            .handle(&mut self.simulation, &binding, &operation);
        if handled == Handled::Suspend {
            return Ok(false);
        }
        if !self.policies.is_empty() {
            if matches!(&handled, Handled::Answer(response) if policy::refused(response)) {
                if self.fresh() {
                    self.policy_refusals += 1;
                }
            } else if let Operation::Send(store, delivery) = &operation {
                let key = (store.identity, delivery.id);
                for policy in &self.policies {
                    match policy {
                        Policy::AtMostOne { .. } if self.attempts.contains(&key) => {
                            return Err(Failure::Violation("policy: second send of a key", None));
                        }
                        Policy::Allowlist(recipients)
                            if !recipients.contains(&delivery.destination) =>
                        {
                            return Err(Failure::Violation(
                                "policy: recipient not allowlisted",
                                None,
                            ));
                        }
                        _ => {}
                    }
                }
                self.attempts.push(key);
            }
        }
        if let (Some(before), true) = (
            before,
            self.simulation.script.made().len() == self.prefix.len(),
        ) {
            laws.check(
                &self.profile,
                &before,
                &binding,
                &operation,
                &handled,
                &self.simulation.world,
            )
            .map_err(|law| Failure::Violation("law failed", Some(law)))?;
            if !stores.contains(&self.simulation.world.durable) {
                stores.push(self.simulation.world.durable.clone());
            }
        }
        match handled {
            Handled::Answer(response) => {
                let reply = Reply { binding, response };
                let actor = &mut self.actors[index];
                let ended = match &mut actor.running {
                    Some(Running::Commit(driver)) => {
                        driver
                            .reply(reply)
                            .map_err(|_| Failure::Violation("a bound answer was refused", None))?;
                        if let Some(fatal) = driver.failure() {
                            return Err(Failure::Refused(Refusal::Fatal(fatal)));
                        }
                        driver.outcome().map(|outcome| {
                            actor.commit = Some(outcome.clone());
                            commit_done(outcome)
                        })
                    }
                    Some(Running::Delivery(driver)) => {
                        driver
                            .reply(reply)
                            .map_err(|_| Failure::Violation("a bound answer was refused", None))?;
                        if let Some(fatal) = driver.failure() {
                            return Err(Failure::Refused(Refusal::Fatal(fatal)));
                        }
                        driver.outcome().map(|outcome| {
                            actor.delivery = Some(outcome.clone());
                            delivery_done(outcome)
                        })
                    }
                    None => None,
                };
                if let Some(bytes) = ended {
                    self.actors[index].stack.done(&bytes);
                    self.finish(index, Ended::Done(bytes));
                }
            }
            Handled::Crash => {
                if self.fresh() {
                    self.tally.crashes += 1;
                }
                self.finish(index, Ended::Crashed);
                let actor = &mut self.actors[index];
                actor.running = None;
                actor.crashed = true;
            }
            Handled::Suspend => {}
        }
        Ok(true)
    }

    /// Offers every lost reply late, at this point; each driver must refuse
    /// it. A refusal changes nothing, so the replies stay offered at every
    /// later point instead of being scheduled.
    fn offer_late(&mut self) -> Result<(), Failure> {
        let fresh = self.fresh();
        for (binding, response) in &self.simulation.world.late {
            let owner = (binding.id.epoch / 16).checked_sub(1);
            let Some(actor) = owner.and_then(|owner| self.actors.get_mut(owner as usize)) else {
                continue;
            };
            let reply = Reply {
                binding: binding.clone(),
                response: response.clone(),
            };
            let admitted = match &mut actor.running {
                Some(Running::Commit(driver)) => driver.reply(reply).is_ok(),
                Some(Running::Delivery(driver)) => driver.reply(reply).is_ok(),
                None => continue,
            };
            if admitted {
                return Err(Failure::Violation("late reply admitted", None));
            }
            if fresh {
                self.tally.late += 1;
            }
        }
        Ok(())
    }

    /// Whether the run is past every choice its parent node made.
    fn fresh(&self) -> bool {
        self.simulation.script.made().len() == self.prefix.len()
    }

    fn run(&mut self, laws: &mut Laws, stores: &mut Vec<Durable>) -> Result<Run, Failure> {
        loop {
            self.offer_late()?;
            let mut events = Vec::new();
            for (index, actor) in self.actors.iter().enumerate() {
                let waiting = match &actor.running {
                    Some(Running::Commit(driver)) => driver.request().is_some(),
                    Some(Running::Delivery(driver)) => driver.request().is_some(),
                    None => false,
                };
                if waiting {
                    events.push(Event::Step(index));
                }
                if actor.crashed && actor.incarnation < RESTARTS {
                    events.push(Event::Restart(index));
                }
            }
            for index in 0..self.simulation.world.queue.len() {
                events.push(Event::Receive(index));
            }
            if events.is_empty() {
                self.check_end()?;
                self.replay_all()?;
                return Ok(Run::Finished(self.fingerprint("end")));
            }
            let pick = match self.simulation.script.choose(events.len()) {
                Some(pick) => pick,
                None if self.simulation.script.malformed => {
                    return Err(Failure::Refused(Refusal::Choice));
                }
                None => {
                    return Ok(Run::Suspended {
                        options: events.len(),
                        fingerprint: Some(self.fingerprint("schedule")),
                    });
                }
            };
            match events[pick] {
                Event::Step(index) => {
                    if !self.step(index, laws, stores)? {
                        if self.simulation.script.malformed {
                            return Err(Failure::Refused(Refusal::Choice));
                        }
                        // Handlers choose before they change anything, so the
                        // state is still the one before this step.
                        let mut pending = String::new();
                        let _ = write!(pending, "fault {index}");
                        return Ok(Run::Suspended {
                            options: self.simulation.script.needed.unwrap_or(1),
                            fingerprint: Some(self.fingerprint(&pending)),
                        });
                    }
                }
                Event::Restart(index) => {
                    self.actors[index].incarnation += 1;
                    let recover = matches!(self.actors[index].role, Role::Commit { .. });
                    self.launch(index, recover)?;
                    if self.fresh() {
                        self.tally.restarts += 1;
                    }
                }
                Event::Receive(index) => {
                    self.simulation.receive(index);
                    if self.fresh() {
                        self.tally.received += 1;
                    }
                }
            }
        }
    }

    /// The protocol invariants of a final state.
    fn check_end(&self) -> Result<(), Failure> {
        let durable = &self.simulation.world.durable;
        let world = &self.simulation.world;
        // No partial commit: refold the store from genesis.
        let Some(fold) = laws::history_fold(&durable.commits) else {
            return Err(Failure::Violation("invalid history inputs", None));
        };
        let acked: Vec<bool> = durable.outbox.iter().map(|(_, acked)| *acked).collect();
        let mut expected = fold;
        for (record, acked) in expected.outbox.iter_mut().zip(&acked) {
            record.1 = *acked;
        }
        if expected != *durable {
            return Err(Failure::Violation(
                "the store is not the fold of its history",
                None,
            ));
        }
        // At most one application per key.
        for (index, key) in durable.replays.iter().enumerate() {
            if durable.replays[..index].contains(key) {
                return Err(Failure::Violation("a key applied twice", None));
            }
        }
        for actor in &self.actors {
            match actor.role {
                Role::Commit { key, .. } => {
                    let entry = durable.commits.iter().find(|entry| entry.key == key);
                    if entry.is_some_and(|entry| entry.publication != actor.publication) {
                        return Err(Failure::Violation("a key committed other bytes", None));
                    }
                    // Recovery by the original key: a receipt is that key's
                    // original commit, and a refusal leaves the key absent.
                    match (&actor.commit, entry) {
                        (Some(Outcome::Ok(receipt)), Some(entry))
                            if receipt.version == entry.version
                                && Some(&receipt.chain)
                                    == durable.receipts.get(entry.version as usize - 1) => {}
                        (Some(Outcome::Ok(_)), _) => {
                            return Err(Failure::Violation(
                                "a receipt is not the original key's",
                                None,
                            ));
                        }
                        (Some(Outcome::Refused(_) | Outcome::NotApplied(_)), Some(_)) => {
                            return Err(Failure::Violation("a refused key was applied", None));
                        }
                        _ => {}
                    }
                }
                Role::Delivery { .. } => {
                    if matches!(actor.delivery, Some(Outcome::Ok(_)))
                        && !actor.crashed
                        && durable.outbox.iter().any(|(_, acked)| !acked)
                    {
                        return Err(Failure::Violation(
                            "delivered, yet a record is pending",
                            None,
                        ));
                    }
                }
            }
        }
        // At least once, never more than once past durable deduplication.
        for (index, id) in world.effects.iter().enumerate() {
            if world.effects[..index].contains(id) {
                return Err(Failure::Violation("a delivery took effect twice", None));
            }
        }
        let committed = |id: &Hash32| {
            durable
                .outbox
                .iter()
                .any(|(delivery, _)| delivery.id == *id)
        };
        if !world.sent.iter().all(committed) || !world.effects.iter().all(committed) {
            return Err(Failure::Violation(
                "a delivery was sent before it was committed",
                None,
            ));
        }
        for (delivery, acked) in &durable.outbox {
            if *acked && !world.sent.contains(&delivery.id) {
                return Err(Failure::Violation("a record was acknowledged unsent", None));
            }
        }
        Ok(())
    }

    /// Replays every finished incarnation's transcripts with no handler.
    fn replay_all(&self) -> Result<(), Failure> {
        for actor in &self.actors {
            for done in &actor.done {
                for transcript in &done.transcripts {
                    let ended = match actor.role {
                        Role::Commit {
                            handle, key, mode, ..
                        } => {
                            let invocation = CommitInvocation {
                                epoch: done.epoch,
                                store: simulation::store(handle),
                                key,
                                publication: actor.publication.clone(),
                                mode,
                            };
                            let policy = CommitPolicy {
                                retries: self.profile.retries,
                            };
                            let first = if done.recover {
                                CommitProgram::recover(invocation, policy)
                            } else {
                                CommitProgram::start(invocation, policy)
                            };
                            replay::<CommitProgram>(transcript, done, first, commit_done)
                        }
                        Role::Delivery { handle } => {
                            let invocation = DeliveryInvocation {
                                epoch: done.epoch,
                                store: simulation::store(handle),
                            };
                            let first = DeliveryProgram::start(invocation, self.policy());
                            replay::<DeliveryProgram>(transcript, done, first, delivery_done)
                        }
                    };
                    if ended? != done.ended {
                        return Err(Failure::Violation("replay ended differently", None));
                    }
                }
            }
        }
        Ok(())
    }
}

fn replay<P: Program>(
    transcript: &Transcript,
    done: &Incarnation,
    first: Step<P::Continuation, P::Output>,
    outcome: fn(&Outcome<P::Output>) -> Vec<u8>,
) -> Result<Ended, Failure> {
    record::replay::<P>(transcript, &done.start, done.epoch, first, outcome)
        .map_err(|_| Failure::Violation("replay is not exact", None))
}

#[derive(Clone, Copy, Debug)]
enum Event {
    Step(usize),
    Restart(usize),
    Receive(usize),
}

/// Explores every schedule of `scenario` under `profile`, up to `budget` runs.
pub fn explore(
    scenario: Scenario,
    profile: Profile,
    layers: Layers,
    order: Order,
    budget: u64,
) -> Result<Report, Violation> {
    explore_with_policies(scenario, profile, layers, order, budget, &[])
}

/// Explores program × world × policy, including policy state at every
/// fingerprint and every retry/restart. Refused actions remain recorded and
/// pending; an exhausted search is incomplete, never a pass.
pub fn explore_with_policies(
    scenario: Scenario,
    profile: Profile,
    layers: Layers,
    order: Order,
    budget: u64,
    policies: &[Policy],
) -> Result<Report, Violation> {
    let premises = if policies.is_empty() {
        &[][..]
    } else {
        policy::PREMISES
    };
    let mut report = Report {
        source: search_source(),
        policies: policies.iter().map(Policy::identity).collect(),
        premises,
        exploration: Exploration::Complete,
        stats: Stats::default(),
        laws: Laws::default(),
        stores: Vec::new(),
        finals: Hash32::ZERO,
        checkpoint: None,
    };
    if !profile.well_formed() {
        report.exploration = Exploration::Refused(Refusal::Profile);
        return Ok(report);
    }
    let mut frontier = VecDeque::new();
    frontier.push_back(Vec::new());
    resume(
        Checkpoint {
            source: report.source,
            binding: search_binding(scenario, profile, layers, order, policies),
            scenario,
            profile,
            layers,
            order,
            policies: policies.to_vec(),
            frontier,
            visited: BTreeSet::new(),
            finals: BTreeSet::new(),
            progress: Box::new(report),
        },
        budget,
    )
}

/// Continues exactly the retained search for at most `budget` additional runs.
/// A changed implementation or profile binding refuses before doing work.
pub fn resume(checkpoint: Checkpoint, budget: u64) -> Result<Report, Violation> {
    if checkpoint.source != search_source()
        || checkpoint.binding
            != search_binding(
                checkpoint.scenario,
                checkpoint.profile,
                checkpoint.layers,
                checkpoint.order,
                &checkpoint.policies,
            )
    {
        return Err(Violation {
            policies: checkpoint.policies.iter().map(Policy::identity).collect(),
            premises: checkpoint.progress.premises,
            schedule: Vec::new(),
            what: "checkpoint source/profile mismatch",
            law: None,
        });
    }
    let Checkpoint {
        source,
        binding,
        scenario,
        profile,
        layers,
        order,
        policies: policy_data,
        mut frontier,
        mut visited,
        mut finals,
        progress,
    } = checkpoint;
    let policies = policy_data.as_slice();
    let mut report = *progress;
    let premises = report.premises;
    report.exploration = Exploration::Complete;
    let limit = report.stats.runs.saturating_add(budget);
    // Immutable source identities are calculated once per resumed invocation,
    // then copied into each prefix. There is no global or retained cache.
    let manifest = Manifest::current(Vec::new(), Vec::new(), Hash32::ZERO, Vec::new());
    loop {
        let next = match order {
            Order::Depth => frontier.pop_back(),
            Order::Breadth => frontier.pop_front(),
        };
        let Some(prefix) = next else {
            break;
        };
        if report.stats.runs >= limit {
            // Put the popped prefix back at exactly its traversal position.
            match order {
                Order::Depth => frontier.push_back(prefix),
                Order::Breadth => frontier.push_front(prefix),
            }
            report.exploration = Exploration::Incomplete(frontier.len());
            report.checkpoint = Some(Checkpoint {
                source,
                binding,
                scenario,
                profile,
                layers,
                order,
                policies: policy_data.clone(),
                frontier,
                visited,
                finals: finals.clone(),
                progress: Box::new(report.clone()),
            });
            break;
        }
        report.stats.runs += 1;
        report.stats.deepest = report.stats.deepest.max(prefix.len());
        let outcome = System::bound(scenario, profile, layers, &prefix, policies, &manifest)
            .and_then(|mut system| {
                let run = system.run(&mut report.laws, &mut report.stores);
                let tally = &mut report.stats.tally;
                tally.crashes += system.tally.crashes;
                tally.restarts += system.tally.restarts;
                tally.received += system.tally.received;
                tally.late += system.tally.late;
                report.stats.policy_refusals += system.policy_refusals;
                let run = run?;
                if matches!(run, Run::Finished(_)) {
                    for actor in &system.actors {
                        match actor.role {
                            Role::Commit { .. } => {
                                report.stats.actions.count(actor.commit.as_ref())
                            }
                            Role::Delivery { .. } => {
                                report.stats.actions.count(actor.delivery.as_ref())
                            }
                        }
                    }
                }
                if matches!(run, Run::Finished(_)) && layers != Layers::Bare {
                    let mut count = 0;
                    for actor in &system.actors {
                        count += actor.done.len() as u64;
                    }
                    report.stats.replays += count;
                }
                Ok(run)
            });
        match outcome {
            Ok(Run::Suspended {
                options,
                fingerprint,
            }) => {
                if let Some(fingerprint) = fingerprint {
                    if !visited.insert(fingerprint) {
                        report.stats.pruned += 1;
                        continue;
                    }
                    report.stats.states += 1;
                }
                let child = |choice: usize| {
                    let mut child = prefix.clone();
                    child.push(choice as u8);
                    child
                };
                match order {
                    Order::Depth => {
                        for choice in (0..options).rev() {
                            frontier.push_back(child(choice));
                        }
                    }
                    Order::Breadth => {
                        for choice in 0..options {
                            frontier.push_back(child(choice));
                        }
                    }
                }
            }
            Ok(Run::Finished(fingerprint)) => {
                report.stats.schedules += 1;
                finals.insert(fingerprint);
            }
            Err(Failure::Violation(what, law)) => {
                return Err(Violation {
                    policies: report.policies.clone(),
                    premises,
                    schedule: prefix,
                    what,
                    law,
                });
            }
            Err(Failure::Refused(refusal)) => {
                report.exploration = Exploration::Refused(refusal);
                break;
            }
        }
    }
    let mut bytes = Vec::new();
    for fingerprint in &finals {
        codec::hash(&mut bytes, *fingerprint);
    }
    report.finals = RustCryptoSha256::hash(&bytes);
    Ok(report)
}

/// One finished run's transcript and what replaying it needs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Recorded {
    /// Premises and nonclaims, including for refused or crashed actions.
    pub premises: &'static [&'static str],
    /// The transcript.
    pub transcript: Transcript,
    /// The manifest's start bytes.
    pub start: Vec<u8>,
    /// The run's epoch.
    pub epoch: u64,
    /// Whether the run was a restart.
    pub recover: bool,
    /// How the run ended.
    pub ended: Ended,
}

/// Runs one complete schedule and returns every run's transcripts, or
/// `None` if the schedule stops short. `layers` must record.
pub fn transcripts(
    scenario: Scenario,
    profile: Profile,
    layers: Layers,
    schedule: &[u8],
) -> Option<Vec<Recorded>> {
    transcripts_with_policies(scenario, profile, layers, schedule, &[])
}

/// Reproduces a schedule with the same policy product, retaining its bound
/// manifests and premises. A short or invalid schedule yields `None`.
pub fn transcripts_with_policies(
    scenario: Scenario,
    profile: Profile,
    layers: Layers,
    schedule: &[u8],
    policies: &[Policy],
) -> Option<Vec<Recorded>> {
    if !profile.well_formed() {
        return None;
    }
    let mut system = System::new(scenario, profile, layers, schedule, policies).ok()?;
    let mut laws = Laws::default();
    let mut stores = Vec::new();
    match system.run(&mut laws, &mut stores).ok()? {
        Run::Finished(_) => {}
        Run::Suspended { .. } => return None,
    }
    let mut all = Vec::new();
    for actor in &system.actors {
        for done in &actor.done {
            for transcript in &done.transcripts {
                all.push(Recorded {
                    premises: if policies.is_empty() {
                        &[]
                    } else {
                        policy::PREMISES
                    },
                    transcript: transcript.clone(),
                    start: done.start.clone(),
                    epoch: done.epoch,
                    recover: done.recover,
                    ended: done.ended.clone(),
                });
            }
        }
    }
    Some(all)
}

/// The option count of the next choice after `prefix`, or `None` when the
/// schedule is complete.
pub fn next_choice(
    scenario: Scenario,
    profile: Profile,
    layers: Layers,
    prefix: &[u8],
) -> Result<Option<usize>, Violation> {
    next_choice_with_policies(scenario, profile, layers, prefix, &[])
}

/// Reproduces a policy counterexample or offers its next schedule choice.
pub fn next_choice_with_policies(
    scenario: Scenario,
    profile: Profile,
    layers: Layers,
    prefix: &[u8],
    policies: &[Policy],
) -> Result<Option<usize>, Violation> {
    let violation = |what, law| Violation {
        policies: policies.iter().map(Policy::identity).collect(),
        premises: if policies.is_empty() {
            &[]
        } else {
            policy::PREMISES
        },
        schedule: prefix.to_vec(),
        what,
        law,
    };
    if !profile.well_formed() {
        return Err(violation("refused profile", None));
    }
    let mut laws = Laws::default();
    let mut stores = Vec::new();
    let run = System::new(scenario, profile, layers, prefix, policies)
        .and_then(|mut system| system.run(&mut laws, &mut stores));
    match run {
        Ok(Run::Suspended { options, .. }) => Ok(Some(options)),
        Ok(Run::Finished(_)) => Ok(None),
        Err(Failure::Violation(what, law)) => Err(violation(what, law)),
        Err(Failure::Refused(_)) => Err(violation("refused", None)),
    }
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;

    #[test]
    fn checkpoint_refuses_source_or_profile_substitution() {
        let original = explore(
            Scenario::Commit,
            Profile::frozen(0),
            Layers::Record,
            Order::Depth,
            0,
        )
        .unwrap_or_else(|error| panic!("{error:?}"))
        .checkpoint
        .unwrap_or_else(|| panic!("missing checkpoint"));
        for changed_source in [true, false] {
            let mut checkpoint = original.clone();
            if changed_source {
                checkpoint.source = Hash32::ZERO;
            } else {
                checkpoint.profile.ticks = 2;
            }
            assert_eq!(
                resume(checkpoint, 10)
                    .err()
                    .unwrap_or_else(|| panic!("substituted checkpoint accepted"))
                    .what,
                "checkpoint source/profile mismatch"
            );
        }
    }
}
