//! The production SQLite handler behind the shell-effects ports, checked
//! against the independent protocol model and its complete `v2_*` snapshot
//! comparator: every fault schedule of the frozen profile that applies to
//! SQLite, every statement boundary, a real busy store, and ten real
//! process aborts with recovery by the original key.
#[allow(dead_code, unused_imports)]
mod sim;

use rusqlite::Connection;
use sim::{
    POINTS,
    database::{self, Files, Receiver},
    fixture, key,
    model::{self, Input, Model},
    ok,
};
use std::{collections::BTreeSet, fmt::Write as _, time::Duration};
use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};
use zeno_fcis_shell::effects::{
    driver::{Driver, Reply},
    handler::{self, Fault, Handled, Terminal},
    port::{
        self, Code, CommitMode, CommitReceipt, EpochTick, Operation, Outcome, Report, Response,
        Sent, Store, WaitError,
    },
    program::{
        CommitInvocation, CommitPolicy, CommitProgram, DeliveryInvocation, DeliveryPolicy,
        DeliveryProgram, Program,
    },
    registry::REGISTRY,
    simulation::Script,
};
use zeno_fcis_shell::{CommitStatus, MemoryDestination};
use zeno_fcis_shell_sqlite::{
    CrashMode, CrashPoint,
    effects::{
        self,
        production::{Production, Time, Transport},
    },
    v2::{Lineage, V2SqliteShell},
};
use zeno_fcis_synthesis::finite::v2_authority::Authority;

/// Sends into the receiver that P1's model tracks; it deduplicates durably.
struct Wire {
    receiver: Option<Receiver>,
}
impl std::fmt::Debug for Wire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wire")
            .field("receiver", &self.receiver.is_some())
            .finish()
    }
}
impl Transport for Wire {
    fn send(&mut self, delivery: &port::Delivery) -> Sent {
        if let Some(receiver) = &mut self.receiver {
            receiver.receive(&carried(delivery), false);
        }
        Sent::Sent(Report {
            delivery_id: delivery.id,
            payload_sha256: delivery.payload_sha256,
        })
    }
}
fn carried(delivery: &port::Delivery) -> model::Delivery {
    model::Delivery {
        sequence: delivery.version,
        ordinal: delivery.ordinal,
        id: delivery.id,
        payload: delivery.payload.clone(),
        row: Vec::new(),
        acknowledged: false,
    }
}

/// A clock that never moves on its own: waits return their deadline.
#[derive(Debug, Default)]
struct Ticks(u64);
impl Time for Ticks {
    fn clock(&mut self, epoch: u64) -> Result<EpochTick, port::Fault> {
        Ok(EpochTick {
            epoch,
            tick: self.0,
        })
    }
    fn random(&mut self, _: u64) -> Result<u64, port::Fault> {
        Ok(0)
    }
    fn wait(&mut self, deadline: EpochTick) -> Result<EpochTick, WaitError> {
        self.0 = self.0.max(deadline.tick);
        Ok(EpochTick {
            epoch: deadline.epoch,
            tick: self.0,
        })
    }
}

type Handle<'a, 'p> = Production<'a, 'p, Wire, Ticks>;

fn with_lineage(run: impl FnOnce(&Lineage<'_, '_>)) {
    fixture::with_catalog(|catalog| run(&ok(Lineage::bind(&[catalog], &[]))));
}

fn handle<'a, 'p>(
    files: &Files,
    lineage: &'a Lineage<'a, 'p>,
    number: u32,
    receiver: bool,
) -> Handle<'a, 'p> {
    let receiver = receiver.then(|| Receiver::open(&files.receiver));
    ok(Production::open(
        &files.store,
        lineage,
        number,
        Wire { receiver },
        Some(Ticks::default()),
    ))
}

fn data(a: &Authority<'_>, input: &Input) -> port::PublicationData {
    let pre = model::state(fixture::schema_hash(), input.before);
    effects::publication_data(&fixture::publication(
        a,
        &pre,
        &input.command,
        &input.context,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scenario {
    Commit,
    Delivery,
}

#[derive(Debug)]
enum Running {
    Commit(Driver<CommitProgram>),
    Delivery(Driver<DeliveryProgram>),
}

#[derive(Debug)]
struct Actor {
    handle: usize,
    input: Option<Input>,
    mode: CommitMode,
    running: Option<Running>,
    incarnation: u8,
    crashed: bool,
    commit: Option<Outcome<CommitReceipt>>,
    delivery: Option<Outcome<u64>>,
}

enum Run {
    Suspended(usize, Hash32),
    Finished,
}

/// One world: a fresh store, two production handles, P1's model and receiver.
struct World<'a, 'p> {
    authority: &'a Authority<'p>,
    files: Files,
    handles: Vec<Handle<'a, 'p>>,
    model: Model,
    actors: Vec<Actor>,
    script: Script,
    retries: u8,
    interrupted: Option<CrashPoint>,
}

impl<'a, 'p> World<'a, 'p> {
    fn new(lineage: &'a Lineage<'a, 'p>, scenario: Scenario, retries: u8, prefix: &[u8]) -> Self {
        let a = &lineage.authorities()[0];
        let files = Files::memory();
        let mut db = sim::create(&files, a);
        let mut model = Model::new(fixture::model_genesis(a));
        let first = fixture::input(a, key(1), [0, 0], 120);
        let second = fixture::input(a, key(2), [0, 0], 121);
        let actors = match scenario {
            Scenario::Commit => vec![
                actor(0, Some(first), CommitMode::At(0)),
                actor(1, Some(second), CommitMode::Unversioned),
            ],
            Scenario::Delivery => {
                assert!(matches!(
                    sim::commit(&mut db, a, &first, 0),
                    model::Outcome::Committed(1, _)
                ));
                model.commit(first, 0, None);
                vec![actor(0, None, CommitMode::Unversioned)]
            }
        };
        drop(db);
        let handles = vec![
            handle(&files, lineage, 1, true),
            handle(&files, lineage, 2, false),
        ];
        let mut world = Self {
            authority: a,
            files,
            handles,
            model,
            actors,
            script: Script::new(prefix.to_vec()),
            retries,
            interrupted: None,
        };
        world.compare("initial");
        for index in 0..world.actors.len() {
            world.launch(index, false);
        }
        world
    }

    fn launch(&mut self, index: usize, recover: bool) {
        let actor = &self.actors[index];
        let store = self.handles[actor.handle].store();
        let epoch = (index as u64 + 1) * 16 + u64::from(actor.incarnation);
        let running = match &actor.input {
            Some(input) => {
                let invocation = CommitInvocation {
                    epoch,
                    store,
                    key: input.key,
                    publication: data(self.authority, input),
                    mode: actor.mode,
                };
                let policy = CommitPolicy {
                    retries: self.retries,
                };
                let first = if recover {
                    CommitProgram::recover(invocation, policy)
                } else {
                    CommitProgram::start(invocation, policy)
                };
                ok(handler::bind(
                    self.handles[actor.handle].ports(),
                    CommitProgram::PORTS,
                ));
                Running::Commit(ok(Driver::start(REGISTRY, epoch, first)))
            }
            None => {
                let invocation = DeliveryInvocation { epoch, store };
                let policy = DeliveryPolicy {
                    retries: self.retries,
                    backoff: 1,
                    jitter: ok(1_u64.try_into()),
                };
                ok(handler::bind(
                    self.handles[actor.handle].ports(),
                    DeliveryProgram::PORTS,
                ));
                Running::Delivery(ok(Driver::start(
                    REGISTRY,
                    epoch,
                    DeliveryProgram::start(invocation, policy),
                )))
            }
        };
        let actor = &mut self.actors[index];
        actor.running = Some(running);
        actor.crashed = false;
    }

    /// The complete `v2_*` snapshot must equal the model's, and the
    /// receiver must hold exactly the model's effects.
    fn compare(&self, at: &str) {
        if let Err(error) = self.agrees() {
            panic!("{at}: {error}");
        }
    }

    fn agrees(&self) -> Result<(), String> {
        database::compare(
            &self.model.database(),
            &database::snapshot(&self.files.store),
        )?;
        self.model.invariants();
        let receiver = Receiver::open(&self.files.receiver);
        receiver.invariants();
        if receiver.state() != self.model.receiver {
            return Err("I6 receiver state differs from the model".into());
        }
        Ok(())
    }

    /// Digest of everything that decides the rest of the run: the store,
    /// the receiver, the model, each actor's driver and the pending choice.
    fn fingerprint(&self, pending: &str) -> Hash32 {
        let mut text = String::new();
        let _ = write!(text, "{pending}|{:?}", self.actors);
        let mut bytes = text.into_bytes();
        for part in [
            database::snapshot(&self.files.store).canonical_bytes(),
            database::snapshot_matching(&self.files.receiver, "*").canonical_bytes(),
            self.model.database().canonical_bytes(),
        ] {
            bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
            bytes.extend_from_slice(&part);
        }
        for handle in &self.handles {
            let state = ok(handle.test_execution_state());
            bytes.extend_from_slice(&(state.len() as u64).to_be_bytes());
            bytes.extend_from_slice(&state);
        }
        bytes.extend_from_slice(format!("{:?}", self.interrupted).as_bytes());
        RustCryptoSha256::hash(&bytes)
    }

    /// Checks the terminal's own answer to a forwarded request against the
    /// model, and advances the model by what the request did.
    fn expect(&mut self, index: usize, operation: &Operation, handled: &Handled) {
        let answer = match handled {
            Handled::Answer(response) => response,
            // An interruption injected inside V2 at a statement boundary:
            // the model's own crash codes say whether the commit survived.
            Handled::Crash => {
                let (Operation::Commit(_, _, _, mode), Some(point), Some(input)) = (
                    operation,
                    self.interrupted.take(),
                    self.actors[index].input.clone(),
                ) else {
                    panic!("a crash nobody injected");
                };
                let fault = POINTS
                    .iter()
                    .position(|candidate| *candidate == point)
                    .map_or(5, |position| position as u8 + 1);
                let expected = match mode {
                    CommitMode::Unversioned => None,
                    CommitMode::At(version) | CommitMode::BoundedAt(version, _) => Some(*version),
                };
                assert_eq!(
                    self.model.commit(input, fault, expected),
                    model::Outcome::Refused(model::Refusal::Crash(fault))
                );
                return;
            }
            Handled::Suspend => panic!("the production handler suspended"),
        };
        let model = &mut self.model;
        match (operation, answer) {
            (Operation::Commit(_, _, _, mode), Response::Commit(outcome)) => {
                let Some(input) = self.actors[index].input.clone() else {
                    panic!("a delivery actor committed");
                };
                let expected = match mode {
                    CommitMode::Unversioned => None,
                    CommitMode::At(version) | CommitMode::BoundedAt(version, _) => Some(*version),
                };
                let actual = match outcome {
                    Outcome::Ok(receipt) if receipt.status == CommitStatus::Committed => {
                        model::Outcome::Committed(receipt.version, receipt.chain)
                    }
                    Outcome::Ok(receipt) => model::Outcome::Replay(receipt.version, receipt.chain),
                    Outcome::Refused(Code::Concurrent) => {
                        model::Outcome::Refused(model::Refusal::Concurrent)
                    }
                    Outcome::Refused(Code::PreState) => {
                        model::Outcome::Refused(model::Refusal::PreState)
                    }
                    Outcome::Refused(Code::Replay) => {
                        model::Outcome::Refused(model::Refusal::Replay)
                    }
                    other => panic!("unexpected commit answer {other:?}"),
                };
                assert_eq!(model.commit(input, 0, expected), actual, "commit");
            }
            (Operation::Acknowledge(_, id, hash), Response::Acknowledge(outcome)) => {
                let actual = match outcome {
                    Outcome::Ok(()) => model::Outcome::Observed,
                    Outcome::Refused(Code::UnknownDelivery) => {
                        model::Outcome::Refused(model::Refusal::Unknown)
                    }
                    Outcome::Refused(Code::AlreadyAcknowledged) => {
                        model::Outcome::Refused(model::Refusal::AlreadyAcknowledged)
                    }
                    Outcome::Refused(Code::PayloadMismatch) => {
                        model::Outcome::Refused(model::Refusal::PayloadMismatch)
                    }
                    other => panic!("unexpected acknowledgement {other:?}"),
                };
                assert_eq!(model.acknowledge(*id, *hash), actual, "acknowledge");
            }
            (Operation::Reopen(_), Response::Reopen(Outcome::Ok(head)))
            | (Operation::ReadState(_), Response::ReadState(Outcome::Ok(head))) => {
                assert_eq!((head.version, head.chain), (model.sequence(), model.chain));
                if matches!(operation, Operation::Reopen(_)) {
                    // Reopen audits, which saves an explicit checkpoint.
                    model.audit();
                }
            }
            (Operation::ReadHistory(_), Response::ReadHistory(Outcome::Ok(history))) => {
                assert_eq!(history.commits.len() as u64, model.sequence());
                assert_eq!(history.head.chain, model.chain);
            }
            (Operation::ReadOutbox(_), Response::ReadOutbox(Outcome::Ok(pending))) => {
                let ids: Vec<_> = pending.iter().map(|d| d.id).collect();
                let expected: Vec<_> = model
                    .deliveries
                    .iter()
                    .filter(|d| !d.acknowledged)
                    .map(|d| d.id)
                    .collect();
                assert_eq!(ids, expected, "export");
            }
            (Operation::Send(_, delivery), Response::Send(Sent::Sent(_))) => {
                model.receiver.insert(delivery.id, delivery.payload.clone());
            }
            (Operation::Clock | Operation::Random(..) | Operation::Wait(_), _) => {}
            (operation, answer) => panic!("unexpected answer {answer:?} to {operation:?}"),
        }
    }

    fn step(&mut self, index: usize) -> Option<usize> {
        let request = match &self.actors[index].running {
            Some(Running::Commit(driver)) => driver.request().map(|(b, o)| (b.clone(), o.clone())),
            Some(Running::Delivery(driver)) => {
                driver.request().map(|(b, o)| (b.clone(), o.clone()))
            }
            None => None,
        };
        let (binding, operation) = request?;
        let terminal = &mut self.handles[self.actors[index].handle];
        let mut fault = Fault::new(terminal, &mut self.script);
        let handled = fault.handle(&binding, &operation);
        let forwarded = fault.forwarded.take();
        if handled == Handled::Suspend {
            return self.script.needed;
        }
        if let Some(raw) = &forwarded {
            self.expect(index, &operation, raw);
        }
        if let Err(error) = self.agrees() {
            panic!("{:?} after {operation:?}: {error}", self.script.made());
        }
        let actor = &mut self.actors[index];
        match handled {
            Handled::Answer(response) => {
                let reply = Reply { binding, response };
                match &mut actor.running {
                    Some(Running::Commit(driver)) => {
                        ok(driver.reply(reply));
                        actor.commit = driver.outcome().cloned();
                    }
                    Some(Running::Delivery(driver)) => {
                        ok(driver.reply(reply));
                        actor.delivery = driver.outcome().cloned();
                    }
                    None => {}
                }
            }
            _ => {
                actor.running = None;
                actor.crashed = true;
            }
        }
        None
    }

    fn run(&mut self) -> Run {
        loop {
            let mut events = Vec::new();
            for (index, actor) in self.actors.iter().enumerate() {
                let waiting = match &actor.running {
                    Some(Running::Commit(driver)) => driver.request().is_some(),
                    Some(Running::Delivery(driver)) => driver.request().is_some(),
                    None => false,
                };
                if waiting {
                    events.push((index, false));
                }
                if actor.crashed && actor.incarnation == 0 {
                    events.push((index, true));
                }
            }
            if events.is_empty() {
                self.finish();
                return Run::Finished;
            }
            let Some(pick) = self.script.choose(events.len()) else {
                return Run::Suspended(events.len(), self.fingerprint("schedule"));
            };
            let (index, restart) = events[pick];
            if restart {
                self.actors[index].incarnation += 1;
                let recover = self.actors[index].input.is_some();
                self.launch(index, recover);
            } else if let Some(options) = self.step(index) {
                return Run::Suspended(options, self.fingerprint(&format!("fault {index}")));
            }
        }
    }

    /// Recovery by the original key, and complete delivery.
    fn finish(&self) {
        for actor in &self.actors {
            if let (Some(Outcome::Ok(receipt)), Some(input)) = (&actor.commit, &actor.input) {
                let replayed = self.model.clone().commit(input.clone(), 0, None);
                assert_eq!(
                    replayed,
                    model::Outcome::Replay(receipt.version, receipt.chain),
                    "a receipt is not the original key's"
                );
            }
            if matches!(actor.delivery, Some(Outcome::Ok(_))) {
                assert!(self.model.deliveries.iter().all(|d| d.acknowledged));
            }
        }
    }
}

fn actor(handle: usize, input: Option<Input>, mode: CommitMode) -> Actor {
    Actor {
        handle,
        input,
        mode,
        running: None,
        incarnation: 0,
        crashed: false,
        commit: None,
        delivery: None,
    }
}

/// Every fault schedule the fault handler and scheduler can make, depth
/// first, each state expanded once.
fn explore(lineage: &Lineage<'_, '_>, scenario: Scenario, retries: u8) -> (u64, u64, u64) {
    let mut frontier = vec![Vec::new()];
    let mut visited = BTreeSet::new();
    let (mut states, mut schedules, mut runs) = (0, 0, 0);
    while let Some(prefix) = frontier.pop() {
        runs += 1;
        let mut world = World::new(lineage, scenario, retries, &prefix);
        match world.run() {
            Run::Suspended(options, fingerprint) => {
                if !visited.insert(fingerprint) {
                    continue;
                }
                states += 1;
                for choice in (0..options).rev() {
                    let mut child = prefix.clone();
                    child.push(choice as u8);
                    frontier.push(child);
                }
            }
            Run::Finished => schedules += 1,
        }
    }
    (states, schedules, runs)
}

fn agrees(scenario: Scenario, budgets: std::ops::RangeInclusive<u8>) {
    with_lineage(|lineage| {
        for retries in budgets {
            let (states, schedules, runs) = explore(lineage, scenario, retries);
            eprintln!(
                "{scenario:?} retries {retries}: {states} states, {schedules} schedules, {runs} runs"
            );
            assert!(schedules > 0);
        }
    });
}

/// Two commit programs on two handles at retry budget zero, and the
/// delivery program at budgets zero and one; the ignored tests below add
/// the rest of the SQLite scope.
#[test]
fn production_agrees_with_model() {
    agrees(Scenario::Commit, 0..=0);
    agrees(Scenario::Delivery, 0..=1);
}

/// Commit at retry budget one closes on SQLite in about three hours in
/// release. Budget two is checked only in the simulation: each budget
/// multiplied the SQLite runs about seventy-fold (5,498, then 399,718).
#[test]
#[ignore = "commit at retry budget one on SQLite; run it in release"]
fn production_agrees_with_model_commit_budget_one() {
    agrees(Scenario::Commit, 1..=1);
}

#[test]
#[ignore = "delivery at retry budget two on SQLite; run it in release"]
fn production_agrees_with_model_delivery_budget_two() {
    agrees(Scenario::Delivery, 2..=2);
}

#[test]
fn store_identity_binds_genesis_authority_and_interpreter() {
    with_lineage(|lineage| {
        let a = &lineage.authorities()[0];
        let files = Files::memory();
        drop(sim::create(&files, a));
        let one = handle(&files, lineage, 1, false);
        let two = handle(&files, lineage, 2, false);
        assert_eq!(one.store().identity, two.store().identity);
        assert_eq!((one.store().handle, two.store().handle), (1, 2));
        let genesis = model::state(fixture::schema_hash(), [0, 0]);
        let interpreter = ok(MemoryDestination::interpreter_identity());
        assert_eq!(
            one.store().identity,
            effects::store_identity(&genesis, a.identity(), interpreter)
        );
        drop((one, two));
        assert_eq!(
            handle(&files, lineage, 1, false).store().identity,
            effects::store_identity(&genesis, a.identity(), interpreter),
            "reopening keeps the identity"
        );
        // Another genesis, Authority or interpreter is another store.
        let other = model::state(fixture::schema_hash(), [1, 0]);
        let mut identities = BTreeSet::from([
            effects::store_identity(&genesis, a.identity(), interpreter),
            effects::store_identity(&other, a.identity(), interpreter),
            effects::store_identity(&genesis, a.identity(), Hash32::new([7; 32])),
        ]);
        let mut identity = a.identity().to_vec();
        identity[0] ^= 1;
        identities.insert(effects::store_identity(&genesis, &identity, interpreter));
        assert_eq!(identities.len(), 4, "store identity ignores an input");
        // A request for another store or handle is refused untouched.
        let mut one = handle(&files, lineage, 1, false);
        let foreign = Store {
            identity: Hash32::new([9; 32]),
            handle: 1,
        };
        let other_handle = Store {
            handle: 2,
            ..one.store()
        };
        for store in [foreign, other_handle] {
            let read = Operation::ReadState(store);
            let binding =
                zeno_fcis_shell::effects::driver::Binding::of(port::OpId::first(1), &read);
            assert_eq!(
                one.handle(&binding, &read),
                Handled::Answer(Response::ReadState(Outcome::Refused(Code::Identity)))
            );
        }
    });
}

fn recover(world: &mut World<'_, '_>, index: usize) {
    world.actors[index].incarnation += 1;
    world.launch(index, true);
    assert!(matches!(world.run(), Run::Finished));
}

/// A crash at every statement boundary of the commit transaction: the store
/// keeps the pre-state or the complete post-state, and the restarted program
/// recovers by the original key.
#[test]
fn production_statement_boundaries() {
    with_lineage(|lineage| {
        let points = POINTS
            .iter()
            .copied()
            .chain([CrashPoint::AfterFirstOutboxWrite]);
        for point in points {
            // One commit actor and no other choice: the first option everywhere.
            let mut world = World::new(lineage, Scenario::Commit, 0, &[]);
            world.actors.truncate(1);
            world.script = Script::fault_free();
            world.handles[0].interrupt_next_commit(point);
            world.interrupted = Some(point);
            let crash = world.step(0);
            assert_eq!(crash, None);
            assert!(world.actors[0].crashed, "{point:?} did not interrupt");
            assert_eq!(world.interrupted, None);
            recover(&mut world, 0);
            let Some(Outcome::Ok(receipt)) = world.actors[0].commit else {
                panic!("{point:?}: recovery failed");
            };
            let expected = if point == CrashPoint::AfterCommit {
                CommitStatus::IdempotentReplay
            } else {
                CommitStatus::Committed
            };
            assert_eq!(
                (receipt.version, receipt.status),
                (1, expected),
                "{point:?}"
            );
            world.compare(&format!("{point:?} recovered"));
        }
    });
}

/// A real SQLite busy store answers NotApplied(Busy) and changes nothing.
#[test]
fn production_real_busy() {
    with_lineage(|lineage| {
        let mut world = World::new(lineage, Scenario::Commit, 1, &[]);
        world.actors.truncate(1);
        world.script = Script::fault_free();
        ok(world.handles[0].set_test_busy_timeout(Duration::ZERO));
        let lock = ok(Connection::open(&world.files.store));
        ok(lock.execute_batch("BEGIN IMMEDIATE"));
        let (binding, operation) = match &world.actors[0].running {
            Some(Running::Commit(driver)) => {
                let (binding, operation) = driver.request().unwrap_or_else(|| panic!("request"));
                (binding.clone(), operation.clone())
            }
            _ => panic!("commit actor"),
        };
        let busy = world.handles[0].handle(&binding, &operation);
        assert_eq!(
            busy,
            Handled::Answer(Response::Commit(Outcome::NotApplied(port::Fault::Busy))),
            "busy is not applied"
        );
        ok(lock.execute_batch("ROLLBACK"));
        drop(lock);
        world.compare("busy");
        assert!(matches!(world.run(), Run::Finished));
        assert!(matches!(world.actors[0].commit, Some(Outcome::Ok(_))));
    });
}

/// V2's refusals keep their codes through the production handler.
#[test]
fn production_keeps_v2_refusals() {
    with_lineage(|lineage| {
        let a = &lineage.authorities()[0];
        let mut world = World::new(lineage, Scenario::Commit, 0, &[]);
        let store = world.handles[0].store();
        let first = fixture::input(a, key(1), [0, 0], 120);
        let mut forged = data(a, &first);
        forged.subject[0] ^= 1;
        let changed = data(a, &fixture::input(a, key(1), [0, 0], 121));
        let cases = [
            (
                Operation::Commit(store, key(1), data(a, &first), CommitMode::At(1)),
                Code::Concurrent,
            ),
            (
                Operation::Commit(store, key(1), forged, CommitMode::Unversioned),
                Code::Identity,
            ),
            (
                Operation::Commit(store, key(1), data(a, &first), CommitMode::BoundedAt(0, 1)),
                Code::Capacity,
            ),
            (
                Operation::Commit(store, key(1), data(a, &first), CommitMode::Unversioned),
                Code::Replay,
            ),
        ];
        for (index, (operation, code)) in cases.into_iter().enumerate() {
            if index == 3 {
                // Commit key 1 first, then changed bytes under it refuse.
                let commit = Operation::Commit(store, key(1), changed.clone(), CommitMode::At(0));
                let binding =
                    zeno_fcis_shell::effects::driver::Binding::of(port::OpId::first(9), &commit);
                assert!(matches!(
                    world.handles[0].handle(&binding, &commit),
                    Handled::Answer(Response::Commit(Outcome::Ok(_)))
                ));
            }
            let binding =
                zeno_fcis_shell::effects::driver::Binding::of(port::OpId::first(1), &operation);
            assert_eq!(
                world.handles[0].handle(&binding, &operation),
                Handled::Answer(Response::Commit(Outcome::Refused(code))),
                "{operation:?}"
            );
        }
    });
}

// The child is selected by the test harness; it is harmless in normal runs.
#[test]
fn production_child() {
    let Ok(role) = std::env::var("EFFECTS_CHILD") else {
        return;
    };
    let path = ok(std::env::var("EFFECTS_STORE"));
    fixture::with_authority(|a| {
        let mut db = ok(V2SqliteShell::open(&path, a));
        let input = fixture::input(a, key(1), [0, 0], 120);
        let role: u8 = ok(role.parse());
        let pre = model::state(fixture::schema_hash(), input.before);
        if role < 7 || role == 9 {
            let _ = db.commit_with_test_controls(
                input.key,
                fixture::publication(a, &pre, &input.command, &input.context),
                Some(if role == 9 {
                    CrashPoint::AfterFirstOutboxWrite
                } else {
                    POINTS[role as usize]
                }),
                CrashMode::Abort,
                || {},
            );
        } else {
            sim::commit(&mut db, a, &input, 0);
            if role == 8 {
                let mut model = Model::new(fixture::model_genesis(a));
                model.commit(input, 0, None);
                let mut receiver =
                    Receiver::open(std::path::Path::new(&ok(std::env::var("EFFECTS_RECEIVER"))));
                receiver.receive(&model.deliveries[0], false);
            }
            std::process::abort();
        }
        panic!("child did not abort");
    });
}

/// P1's ten real process aborts, recovered by the restarted commit program
/// through the production handler; role 8 also redelivers after recovery.
#[test]
fn production_abort_reopen() {
    with_lineage(|lineage| {
        let a = &lineage.authorities()[0];
        for role in 0..10 {
            let files = Files::new();
            drop(sim::create(&files, a));
            let status = ok(std::process::Command::new(ok(std::env::current_exe()))
                .args(["--exact", "production_child", "--nocapture"])
                .env("EFFECTS_CHILD", role.to_string())
                .env("EFFECTS_STORE", &files.store)
                .env("EFFECTS_RECEIVER", &files.receiver)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status());
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(status.signal(), Some(6), "role {role}: child must abort");
            }
            let input = fixture::input(a, key(1), [0, 0], 120);
            let mut model = Model::new(fixture::model_genesis(a));
            if (6..=8).contains(&role) {
                model.commit(input.clone(), 0, None);
            }
            if role == 8 {
                model
                    .receiver
                    .insert(model.deliveries[0].id, model.deliveries[0].payload.clone());
            }
            ok(database::compare(
                &model.database(),
                &database::snapshot(&files.store),
            ));
            let mut world = World {
                authority: a,
                handles: vec![
                    handle(&files, lineage, 1, true),
                    handle(&files, lineage, 2, false),
                ],
                model,
                actors: vec![actor(0, Some(input), CommitMode::At(0))],
                script: Script::fault_free(),
                retries: 0,
                interrupted: None,
                files,
            };
            recover(&mut world, 0);
            let Some(Outcome::Ok(receipt)) = world.actors[0].commit else {
                panic!("role {role}: recovery failed");
            };
            let replayed = (6..=8).contains(&role);
            assert_eq!(
                (
                    receipt.version,
                    receipt.status == CommitStatus::IdempotentReplay
                ),
                (1, replayed),
                "role {role}"
            );
            // Deliver both records; the receiver deduplicates the redelivery.
            world.actors.push(actor(0, None, CommitMode::Unversioned));
            world.launch(1, false);
            assert!(matches!(world.run(), Run::Finished));
            assert!(matches!(world.actors[1].delivery, Some(Outcome::Ok(2))));
            world.compare(&format!("role {role} delivered"));
        }
    });
}

#[test]
fn production_fingerprint_retains_clock() {
    with_lineage(|lineage| {
        let mut early = World::new(lineage, Scenario::Delivery, 1, &[0, 4]);
        let op = Operation::Clock;
        let binding = zeno_fcis_shell::effects::driver::Binding::of(port::OpId::first(17), &op);
        let mut probe = Script::new(vec![]);
        let has_time_faults =
            Fault::new(&mut early.handles[0], &mut probe).handle(&binding, &op) == Handled::Suspend;
        let prefix = if has_time_faults {
            vec![0, 1, 0, 0, 0, 4]
        } else {
            vec![0, 1, 4]
        };
        let mut waited = World::new(lineage, Scenario::Delivery, 1, &prefix);
        let (Run::Suspended(na, a), Run::Suspended(nb, b)) = (early.run(), waited.run()) else {
            panic!("expected restart fault point")
        };
        assert_eq!(na, nb);
        assert_ne!(a, b, "clock state projected away");
        let op = Operation::Clock;
        let binding = zeno_fcis_shell::effects::driver::Binding::of(port::OpId::first(17), &op);
        let left = early.handles[0].handle(&binding, &op);
        let right = waited.handles[0].handle(&binding, &op);
        assert_ne!(left, right);
        println!(
            "CHECK: early and waited prefixes retain distinct fingerprints {a} and {b}; Clock returns {left:?} vs {right:?}"
        );
        early.script = Script::new(vec![0, 1]);
        waited.script = Script::new(vec![0, 1]);
        let (Run::Suspended(_, next_a), Run::Suspended(_, next_b)) = (early.run(), waited.run())
        else {
            panic!("expected next Send fault")
        };
        assert_ne!(next_a, next_b);
        println!(
            "CHECK: common suffix [0,1] also produces different successor fingerprints {next_a} and {next_b}"
        );
    });
}
