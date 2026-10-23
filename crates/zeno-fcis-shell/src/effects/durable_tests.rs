use super::super::{
    driver::{Driver, Reply},
    handler::{Fault, Handling},
    laws,
    port::{Code, OpId},
    program::{DeliveryInvocation, DeliveryPolicy, DeliveryProgram},
    registry::{Port, REGISTRY},
    simulation::{self, Durable, Profile, Script, Simulation, World},
};
use super::*;
use std::{cell::RefCell, rc::Rc};

pub(super) fn ok<T, E: core::fmt::Debug>(r: Result<T, E>) -> T {
    r.unwrap_or_else(|e| panic!("{e:?}"))
}
pub(super) fn policies() -> Vec<Policy> {
    vec![
        Policy::AtMostOne { capacity: 2 },
        Policy::allowlist(vec![b"receiver".to_vec()]),
    ]
}
pub(super) fn operation(key: u8) -> Operation {
    let mut d = simulation::deliveries(1, Hash32::new([1; 32]), &[1])[0].clone();
    d.id = Hash32::new([key; 32]);
    Operation::Send(simulation::store(1), d)
}
pub(super) fn binding(op: &Operation) -> Binding {
    Binding::of(OpId::first(1), op)
}
pub(super) fn manifest() -> Manifest {
    Manifest::current(
        vec!["record", "simulation"],
        vec![],
        simulation::STORE,
        vec![],
    )
}

#[derive(Clone)]
pub(super) struct Memory(pub Rc<RefCell<Disk>>);
pub(super) struct Disk {
    pub bytes: Vec<u8>,
    pub writes: usize,
    // Each comparison is a process boundary: before atomic commit, after it,
    // or returned write error (including an ambiguous committed write).
    pub stop: Option<(usize, u8)>,
}
impl Memory {
    pub(super) fn new() -> Self {
        Self(Rc::new(RefCell::new(Disk {
            bytes: ok(initial(simulation::STORE, &policies())),
            writes: 0,
            stop: None,
        })))
    }
    pub(super) fn open(&self) -> Relay<Self> {
        ok(Relay::open(
            self.clone(),
            simulation::STORE,
            &policies(),
            manifest(),
        ))
    }
}
impl StateStore for Memory {
    fn load(&mut self) -> Result<Vec<u8>, Error> {
        Ok(self.0.borrow().bytes.clone())
    }
    fn commit(&mut self, expected: &[u8], next: &[u8]) -> Result<(), Error> {
        let mut disk = self.0.borrow_mut();
        disk.writes += 1;
        let fault = disk
            .stop
            .filter(|(n, _)| *n == disk.writes)
            .map(|(_, kind)| kind);
        if fault == Some(0) {
            panic!("injected process loss before commit");
        }
        if fault == Some(2) {
            return Err(Error::Storage);
        }
        if disk.bytes != expected {
            return Err(Error::Conflict);
        }
        disk.bytes = next.to_vec();
        if fault == Some(1) {
            panic!("injected process loss after commit");
        }
        if fault == Some(3) {
            return Err(Error::Storage);
        }
        Ok(())
    }
}

pub(super) struct TerminalProbe {
    pub disk: Memory,
    pub calls: usize,
    pub suspend: bool,
}
impl Terminal for TerminalProbe {
    fn ports(&self) -> &'static [Port] {
        &Port::ALL
    }
    fn handle(&mut self, b: &Binding, op: &Operation) -> Handled {
        self.handle_with_invocation(b, op).outcome
    }
    fn handle_with_invocation(&mut self, _: &Binding, op: &Operation) -> Handling {
        let restored = ok(restore(
            simulation::STORE,
            &policies(),
            &self.disk.0.borrow().bytes,
        ));
        assert_eq!(
            policy_record::admit(&restored, op).1.rejection(),
            Some(Code::PolicyDuplicate),
            "durable_before_forward: terminal entered before durable reservation"
        );
        if self.suspend {
            return Handling {
                outcome: Handled::Suspend,
                invocation: Invocation::NotInvoked,
            };
        }
        self.calls += 1;
        Handling {
            outcome: Handled::Answer(Response::Send(Sent::Unknown(super::super::port::Fault::Io))),
            invocation: Invocation::MayHaveInvoked,
        }
    }
}

#[test]
fn durable_before_forward() {
    let disk = Memory::new();
    let mut relay = disk.open();
    let mut terminal = TerminalProbe {
        disk,
        calls: 0,
        suspend: false,
    };
    let op = operation(1);
    ok(relay.handle(&mut terminal, &binding(&op), &op));
    assert_eq!(terminal.calls, 1);
}

#[test]
fn restart_keeps_reservations() {
    let disk = Memory::new();
    let op = operation(1);
    struct Counter(usize);
    impl Terminal for Counter {
        fn ports(&self) -> &'static [Port] {
            &Port::ALL
        }
        fn handle(&mut self, _: &Binding, _: &Operation) -> Handled {
            self.0 += 1;
            Handled::Answer(Response::Send(Sent::Unknown(super::super::port::Fault::Io)))
        }
    }
    let mut terminal = Counter(0);
    ok(disk.open().handle(&mut terminal, &binding(&op), &op));
    let handled = ok(disk.open().handle(&mut terminal, &binding(&op), &op));
    assert_eq!(
        handled,
        Handled::Answer(Response::Send(Sent::Refused(Code::PolicyDuplicate))),
        "restart_keeps_reservations: reset allowed another send"
    );
    assert_eq!(terminal.0, 1);
}

#[test]
fn restart_requires_recorded_release() {
    let disk = Memory::new();
    let op = operation(1);
    let (reserved, _) = policy_record::admit(
        &ok(restore(
            simulation::STORE,
            &policies(),
            &disk.0.borrow().bytes,
        )),
        &op,
    );
    disk.0.borrow_mut().bytes = ok(checkpoint(simulation::STORE, &reserved, None));
    let restored = ok(restore(
        simulation::STORE,
        &policies(),
        &disk.0.borrow().bytes,
    ));
    assert_eq!(
        policy_record::admit(&restored, &op).1.rejection(),
        Some(Code::PolicyDuplicate),
        "restart_requires_recorded_release: unrecorded evidence released a key"
    );
    disk.0.borrow_mut().bytes = ok(checkpoint(
        simulation::STORE,
        &reserved,
        Some(Hash32::new([1; 32])),
    ));
    let restored = ok(restore(
        simulation::STORE,
        &policies(),
        &disk.0.borrow().bytes,
    ));
    assert_eq!(policy_record::admit(&restored, &op).1.rejection(), None);
}

#[test]
fn restore_binding() {
    let bytes = ok(initial(simulation::STORE, &policies()));
    let mut other = policies();
    other[0] = Policy::AtMostOne { capacity: 3 };
    assert_eq!(
        restore(simulation::STORE, &other, &bytes),
        Err(Error::Binding),
        "restore_binding: another policy accepted"
    );
    assert_eq!(
        restore(Hash32::ZERO, &policies(), &bytes),
        Err(Error::Binding)
    );
    let mut reordered = policies();
    reordered.reverse();
    assert_eq!(
        restore(simulation::STORE, &reordered, &bytes),
        Err(Error::Binding)
    );
    assert_eq!(
        initial(simulation::STORE, &[Policy::allowlist(vec![])]),
        Err(Error::Binding)
    );
}

#[test]
fn never_sends_on_store_error() {
    for fault in [2, 3] {
        let disk = Memory::new();
        disk.0.borrow_mut().stop = Some((1, fault));
        let mut relay = disk.open();
        let op = operation(1);
        // This probe deliberately does not assume write-ahead so the mutant
        // dies on the no-send assertion, not a prerequisite assertion.
        struct Counter(usize);
        impl Terminal for Counter {
            fn ports(&self) -> &'static [Port] {
                &Port::ALL
            }
            fn handle(&mut self, _: &Binding, _: &Operation) -> Handled {
                self.0 += 1;
                Handled::Crash
            }
        }
        let mut terminal = Counter(0);
        let result = relay.handle(&mut terminal, &binding(&op), &op);
        assert_eq!(
            terminal.0, 0,
            "never_sends_on_store_error: write error still sent"
        );
        assert_eq!(result, Err(Error::Storage));
        assert_eq!(
            relay.handle(&mut terminal, &binding(&op), &op),
            Err(Error::Stopped)
        );
    }
}

#[test]
fn release_is_durable_before_reuse() {
    let op = operation(1);
    for fault in [None, Some(2), Some(3)] {
        let disk = Memory::new();
        disk.0.borrow_mut().stop = fault.map(|f| (2, f));
        let mut terminal = TerminalProbe {
            disk: disk.clone(),
            calls: 0,
            suspend: true,
        };
        let mut relay = disk.open();
        let result = relay.handle(&mut terminal, &binding(&op), &op);
        assert_eq!(result.is_err(), fault.is_some());
        assert_eq!(terminal.calls, 0);
        terminal.suspend = false;
        let retry = ok(disk.open().handle(&mut terminal, &binding(&op), &op));
        assert_eq!(
            matches!(retry, Handled::Answer(Response::Send(Sent::Refused(_)))),
            fault == Some(2)
        );
    }
}

#[test]
fn restore_refuses_corruption_and_invalid_release() {
    let disk = Memory::new();
    let op = operation(1);
    let (reserved, _) = policy_record::admit(
        &ok(restore(
            simulation::STORE,
            &policies(),
            &disk.0.borrow().bytes,
        )),
        &op,
    );
    let bytes = ok(checkpoint(simulation::STORE, &reserved, None));
    for i in 0..bytes.len() {
        let mut flipped = bytes.clone();
        flipped[i] ^= 1;
        assert!(
            restore(simulation::STORE, &policies(), &flipped).is_err(),
            "corruption byte {i}"
        );
        assert!(
            restore(simulation::STORE, &policies(), &bytes[..i]).is_err(),
            "truncation {i}"
        );
    }
    assert!(restore(simulation::STORE, &policies(), &[]).is_err());
    let bad = ok(checkpoint(simulation::STORE, &reserved, Some(Hash32::ZERO)));
    assert_eq!(
        restore(simulation::STORE, &policies(), &bad),
        Err(Error::Corrupt)
    );
    let empty: Vec<_> = policies().into_iter().map(Monitor::new).collect();
    assert_eq!(
        restore(
            simulation::STORE,
            &policies(),
            &ok(checkpoint(simulation::STORE, &empty, Some(Hash32::ZERO)))
        ),
        Err(Error::Corrupt)
    );
}

#[test]
fn historical_composed_suspend_retains_durable_reservation() {
    let disk = Memory::new();
    let op = operation(1);
    let mut simulation = Simulation::new(
        World::new(Durable::genesis()),
        Profile::frozen(0),
        &Port::ALL,
        Script::new(vec![0]),
    );
    let mut choices = Script::new(vec![6]);
    let mut fault = Fault::new(&mut simulation, &mut choices);
    let mut relay = disk.open();
    assert_eq!(
        ok(relay.handle(&mut fault, &binding(&op), &op)),
        Handled::Suspend
    );
    assert_eq!(simulation.world.queue.len(), 1);
    assert_eq!(
        ok(disk.open().handle(&mut simulation, &binding(&op), &op)),
        Handled::Answer(Response::Send(Sent::Refused(Code::PolicyDuplicate)))
    );
    assert_eq!(simulation.world.queue.len(), 1);
}

fn first() -> Step<super::super::program::DeliveryContinuation, u64> {
    DeliveryProgram::start(
        DeliveryInvocation {
            epoch: 1,
            store: simulation::store(1),
        },
        DeliveryPolicy {
            retries: 0,
            backoff: 1,
            jitter: core::num::NonZeroU64::MIN,
        },
    )
}
fn encode(outcome: &Outcome<u64>) -> Vec<u8> {
    codec::outcome(
        outcome,
        if let Outcome::Ok(n) = outcome {
            Some(*n)
        } else {
            None
        },
    )
}
fn replay_run(t: &Transcript) -> Result<Ended, ReplayError> {
    replay::<DeliveryProgram>(t, (simulation::STORE, &policies()), &[], 1, first(), encode)
}
fn rechain(t: Transcript) -> Transcript {
    let mut result = Transcript::new(t.manifest);
    for link in t.links {
        result.append(link.entry);
    }
    result
}

#[test]
fn restored_transcript_recomputes_evidence() {
    let disk = Memory::new();
    let durable = laws::committed(&Durable::genesis(), Hash32::new([1; 32]), 1);
    let op = Operation::Send(simulation::store(1), durable.outbox[0].0.clone());
    let mut simulation = Simulation::new(
        World::new(durable),
        Profile::frozen(0),
        &Port::ALL,
        Script::fault_free(),
    );
    ok(disk.open().handle(&mut simulation, &binding(&op), &op));
    let mut relay = disk.open();
    let mut driver = ok(Driver::<DeliveryProgram>::start(REGISTRY, 1, first()));
    while let Some((b, op)) = driver.request() {
        let b = b.clone();
        let Handled::Answer(response) = ok(relay.handle(&mut simulation, &b, op)) else {
            panic!("expected answer")
        };
        ok(driver.reply(Reply {
            binding: b,
            response,
        }));
    }
    relay.done(&encode(
        driver
            .outcome()
            .unwrap_or_else(|| panic!("missing outcome")),
    ));
    let original = relay.transcript().clone();
    assert!(matches!(replay_run(&original), Ok(Ended::Done(_))));
    let mut changed = original.clone();
    let n = changed.manifest.configuration.len();
    changed.manifest.configuration[n - 1] ^= 1;
    assert!(
        replay_run(&rechain(changed)).is_err(),
        "changed restore bytes replayed"
    );
    let mut changed = original.clone();
    changed.manifest.policies = policies().into_iter().map(Monitor::new).collect();
    assert!(
        replay_run(&rechain(changed)).is_err(),
        "invented restored monitors replayed"
    );
    let mut changed = original;
    for link in &mut changed.links {
        if let Entry::Policy(_, check) = &mut link.entry {
            check.decisions[0].proposed.push(0);
            break;
        }
    }
    assert!(
        replay_run(&rechain(changed)).is_err(),
        "altered admission evidence replayed"
    );
}
