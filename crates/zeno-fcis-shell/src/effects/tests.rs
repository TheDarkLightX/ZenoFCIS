use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Debug;
use core::num::NonZeroU64;

use zeno_fcis_codec::Hash32;

use super::driver::{Binding, Driver, Fatal, Refusal, Reply};
use super::port::{
    AuditedHistory, Code, CommitMode, CommitReceipt, Delivery, EpochTick, Fault, HistoryEntry,
    OpId, Operation, Outcome, PublicationData, Purpose, Report, Response, Sent, Snapshot, Store,
    Val, WaitError,
};
use super::program::{
    CommitInvocation, CommitPolicy, CommitProgram, DeliveryInvocation, DeliveryPolicy,
    DeliveryProgram, Program, Step,
};
use super::registry::{
    self, Answer, Capability, Kind, Law, PROGRAM_SOURCE_SHA256, Port, REGISTRY, Registry,
    RegistryError, SIGNATURES, Signature, TypeError, V2,
};
use crate::CommitStatus;

const EPOCH: u64 = 7;

fn ok<T, E: Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{error:?}"),
    }
}

fn some<T>(value: Option<T>) -> T {
    match value {
        Some(value) => value,
        None => panic!("missing value"),
    }
}

fn hash(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}

fn store(handle: u32) -> Store {
    Store {
        identity: hash(0xa0),
        handle,
    }
}

fn publication(command: u8) -> PublicationData {
    PublicationData {
        prestate: vec![1, 2],
        command: vec![command],
        context: vec![3],
        identity: vec![4; 8],
        subject: vec![command, 5],
    }
}

fn snapshot(version: u64) -> Snapshot {
    Snapshot {
        version,
        identity: vec![4; 8],
        state: vec![version as u8],
        root: hash(0x10),
        chain: hash(0x11),
    }
}

fn receipt(status: CommitStatus) -> CommitReceipt {
    CommitReceipt {
        version: 4,
        chain: hash(0x12),
        status,
    }
}

fn history(entries: &[(Hash32, PublicationData)]) -> AuditedHistory {
    let mut commits = Vec::new();
    for (index, (key, publication)) in entries.iter().enumerate() {
        commits.push(HistoryEntry {
            key: *key,
            version: index as u64 + 1,
            publication: publication.clone(),
        });
    }
    AuditedHistory {
        head: snapshot(entries.len() as u64),
        commits,
    }
}

fn delivery(ordinal: u32) -> Delivery {
    Delivery {
        id: hash(0x30 + ordinal as u8),
        version: 1,
        lane: 1,
        ordinal,
        destination: vec![9],
        payload: vec![ordinal as u8; 4],
        payload_sha256: hash(0x40 + ordinal as u8),
    }
}

fn report(delivery: &Delivery) -> Report {
    Report {
        delivery_id: delivery.id,
        payload_sha256: delivery.payload_sha256,
    }
}

fn invocation(mode: CommitMode) -> CommitInvocation {
    CommitInvocation {
        epoch: EPOCH,
        store: store(1),
        key: hash(0x20),
        publication: publication(120),
        mode,
    }
}

fn commit_driver(retries: u8) -> Driver<CommitProgram> {
    ok(Driver::<CommitProgram>::start(
        REGISTRY,
        EPOCH,
        CommitProgram::start(invocation(CommitMode::At(3)), CommitPolicy { retries }),
    ))
}

fn delivery_policy(retries: u8) -> DeliveryPolicy {
    DeliveryPolicy {
        retries,
        backoff: 3,
        jitter: some(NonZeroU64::new(2)),
    }
}

fn delivery_driver(retries: u8) -> Driver<DeliveryProgram> {
    ok(Driver::<DeliveryProgram>::start(
        REGISTRY,
        EPOCH,
        DeliveryProgram::start(
            DeliveryInvocation {
                epoch: EPOCH,
                store: store(1),
            },
            delivery_policy(retries),
        ),
    ))
}

/// Answers the outstanding request with each response, correctly bound.
fn answer<P: Program>(driver: &mut Driver<P>, responses: Vec<Response>) {
    for response in responses {
        let binding = some(driver.request()).0.clone();
        ok(driver.reply(Reply { binding, response }));
    }
}

/// What a driver shows after its responses: the next request or the outcome.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Eq, PartialEq)]
enum Next<T> {
    Request(Operation),
    Done(Outcome<T>),
}

fn next<P: Program>(driver: &Driver<P>) -> Next<P::Output>
where
    P::Output: Clone,
{
    if let Some((_, operation)) = driver.request() {
        return Next::Request(operation.clone());
    }
    match driver.outcome() {
        Some(outcome) => Next::Done(outcome.clone()),
        None => panic!("driver failed: {:?}", driver.failure()),
    }
}

fn commit_request(mode: CommitMode) -> Next<CommitReceipt> {
    Next::Request(Operation::Commit(
        store(1),
        hash(0x20),
        publication(120),
        mode,
    ))
}

/// Every response branch of the commit program: responses from the start,
/// then the request or outcome the specification requires.
fn commit_cases() -> Vec<(&'static str, u8, Vec<Response>, Next<CommitReceipt>)> {
    let lost = Response::Commit(Outcome::Unknown(Fault::Io));
    let reopened = Response::Reopen(Outcome::Ok(snapshot(4)));
    let present = Response::ReadHistory(Outcome::Ok(history(&[(hash(0x20), publication(120))])));
    let absent = Response::ReadHistory(Outcome::Ok(history(&[(hash(0x21), publication(120))])));
    let changed = Response::ReadHistory(Outcome::Ok(history(&[(hash(0x20), publication(121))])));
    let replay = receipt(CommitStatus::IdempotentReplay);
    vec![
        ("start", 2, vec![], commit_request(CommitMode::At(3))),
        (
            "committed",
            2,
            vec![Response::Commit(Outcome::Ok(receipt(
                CommitStatus::Committed,
            )))],
            Next::Done(Outcome::Ok(receipt(CommitStatus::Committed))),
        ),
        (
            "refused",
            2,
            vec![Response::Commit(Outcome::Refused(Code::Concurrent))],
            Next::Done(Outcome::Refused(Code::Concurrent)),
        ),
        (
            "not applied retries the original mode",
            2,
            vec![Response::Commit(Outcome::NotApplied(Fault::Busy))],
            commit_request(CommitMode::At(3)),
        ),
        (
            "not applied until the budget is spent",
            2,
            vec![
                Response::Commit(Outcome::NotApplied(Fault::Busy)),
                Response::Commit(Outcome::NotApplied(Fault::Busy)),
                Response::Commit(Outcome::NotApplied(Fault::Full)),
            ],
            Next::Done(Outcome::NotApplied(Fault::Full)),
        ),
        (
            "lost reply reopens",
            2,
            vec![lost.clone()],
            Next::Request(Operation::Reopen(store(1))),
        ),
        (
            "lost reply with no budget is unresolved",
            0,
            vec![Response::Commit(Outcome::Unknown(Fault::Full))],
            Next::Done(Outcome::Unknown(Fault::Full)),
        ),
        (
            "reopened reads history",
            2,
            vec![lost.clone(), reopened.clone()],
            Next::Request(Operation::ReadHistory(store(1))),
        ),
        (
            "reopen refused is unresolved",
            2,
            vec![
                lost.clone(),
                Response::Reopen(Outcome::Refused(Code::History)),
            ],
            Next::Done(Outcome::Unknown(Fault::Io)),
        ),
        (
            "reopen failure retries",
            2,
            vec![
                lost.clone(),
                Response::Reopen(Outcome::NotApplied(Fault::Busy)),
            ],
            Next::Request(Operation::Reopen(store(1))),
        ),
        (
            "reopen failures spend the budget and keep the original fault",
            2,
            vec![
                Response::Commit(Outcome::Unknown(Fault::Full)),
                Response::Reopen(Outcome::Unknown(Fault::Io)),
                Response::Reopen(Outcome::Unknown(Fault::Busy)),
            ],
            Next::Done(Outcome::Unknown(Fault::Full)),
        ),
        (
            "exact key and publication retry unversioned",
            2,
            vec![lost.clone(), reopened.clone(), present.clone()],
            commit_request(CommitMode::Unversioned),
        ),
        (
            "absent key keeps the original guard",
            2,
            vec![lost.clone(), reopened.clone(), absent],
            commit_request(CommitMode::At(3)),
        ),
        (
            "changed publication under the key keeps the original guard",
            2,
            vec![lost.clone(), reopened.clone(), changed],
            commit_request(CommitMode::At(3)),
        ),
        (
            "history failure retries",
            2,
            vec![
                lost.clone(),
                reopened.clone(),
                Response::ReadHistory(Outcome::NotApplied(Fault::Busy)),
            ],
            Next::Request(Operation::ReadHistory(store(1))),
        ),
        (
            "history refused is unresolved",
            2,
            vec![
                lost.clone(),
                reopened.clone(),
                Response::ReadHistory(Outcome::Refused(Code::History)),
            ],
            Next::Done(Outcome::Unknown(Fault::Io)),
        ),
        (
            "recovered replay",
            2,
            vec![
                lost.clone(),
                reopened.clone(),
                present.clone(),
                Response::Commit(Outcome::Ok(replay)),
            ],
            Next::Done(Outcome::Ok(replay)),
        ),
        (
            "busy replay retry keeps unversioned",
            2,
            vec![
                lost.clone(),
                reopened.clone(),
                present.clone(),
                Response::Commit(Outcome::NotApplied(Fault::Busy)),
            ],
            commit_request(CommitMode::Unversioned),
        ),
        (
            "busy replay retry with no budget stays unknown",
            1,
            vec![
                lost,
                reopened,
                present,
                Response::Commit(Outcome::NotApplied(Fault::Busy)),
            ],
            Next::Done(Outcome::Unknown(Fault::Io)),
        ),
    ]
}

fn delivery_cases() -> Vec<(&'static str, u8, Vec<Response>, Next<u64>)> {
    let (first, second) = (delivery(0), delivery(1));
    let outbox = Response::ReadOutbox(Outcome::Ok(vec![first.clone(), second.clone()]));
    let sent = Response::Send(Sent::Sent(report(&first)));
    let not_sent = Response::Send(Sent::NotSent(Code::Delivery));
    let clock = Response::Clock(Ok(EpochTick {
        epoch: EPOCH,
        tick: 5,
    }));
    let draw = Response::Random(Ok(1));
    let tick = |tick| EpochTick { epoch: EPOCH, tick };
    vec![
        (
            "start",
            2,
            vec![],
            Next::Request(Operation::ReadOutbox(store(1))),
        ),
        (
            "empty outbox",
            2,
            vec![Response::ReadOutbox(Outcome::Ok(vec![]))],
            Next::Done(Outcome::Ok(0)),
        ),
        (
            "sends the first record's exact bytes",
            2,
            vec![outbox.clone()],
            Next::Request(Operation::Send(store(1), first.clone())),
        ),
        (
            "unordered outbox",
            2,
            vec![Response::ReadOutbox(Outcome::Ok(vec![
                second.clone(),
                first.clone(),
            ]))],
            Next::Done(Outcome::Refused(Code::Protocol)),
        ),
        (
            "read failure retries",
            2,
            vec![Response::ReadOutbox(Outcome::Unknown(Fault::Io))],
            Next::Request(Operation::ReadOutbox(store(1))),
        ),
        (
            "read refused",
            2,
            vec![Response::ReadOutbox(Outcome::Refused(Code::History))],
            Next::Done(Outcome::Refused(Code::History)),
        ),
        (
            "matching report acknowledges",
            2,
            vec![outbox.clone(), sent.clone()],
            Next::Request(Operation::Acknowledge(
                store(1),
                first.id,
                first.payload_sha256,
            )),
        ),
        (
            "mismatched report never acknowledges",
            2,
            vec![
                outbox.clone(),
                Response::Send(Sent::Sent(Report {
                    delivery_id: first.id,
                    payload_sha256: hash(0xee),
                })),
            ],
            Next::Done(Outcome::Unknown(Fault::Io)),
        ),
        (
            "acknowledged sends the next record",
            2,
            vec![
                outbox.clone(),
                sent.clone(),
                Response::Acknowledge(Outcome::Ok(())),
            ],
            Next::Request(Operation::Send(store(1), second.clone())),
        ),
        (
            "already acknowledged counts",
            2,
            vec![
                outbox.clone(),
                sent.clone(),
                Response::Acknowledge(Outcome::Ok(())),
                Response::Send(Sent::Sent(report(&second))),
                Response::Acknowledge(Outcome::Refused(Code::AlreadyAcknowledged)),
            ],
            Next::Done(Outcome::Ok(2)),
        ),
        (
            "acknowledgement refused",
            2,
            vec![
                outbox.clone(),
                sent.clone(),
                Response::Acknowledge(Outcome::Refused(Code::PayloadMismatch)),
            ],
            Next::Done(Outcome::Unknown(Fault::Io)),
        ),
        (
            "lost acknowledgement repeats it",
            2,
            vec![
                outbox.clone(),
                sent.clone(),
                Response::Acknowledge(Outcome::Unknown(Fault::Io)),
            ],
            Next::Request(Operation::Acknowledge(
                store(1),
                first.id,
                first.payload_sha256,
            )),
        ),
        (
            "lost acknowledgement with no budget",
            0,
            vec![
                outbox.clone(),
                sent.clone(),
                Response::Acknowledge(Outcome::Unknown(Fault::Io)),
            ],
            Next::Done(Outcome::Unknown(Fault::Io)),
        ),
        (
            "not sent backs off",
            2,
            vec![outbox.clone(), not_sent.clone()],
            Next::Request(Operation::Clock),
        ),
        (
            "not sent with no budget leaves it pending",
            0,
            vec![outbox.clone(), not_sent.clone()],
            Next::Done(Outcome::Refused(Code::Delivery)),
        ),
        (
            "unknown send with no budget",
            0,
            vec![outbox.clone(), Response::Send(Sent::Unknown(Fault::Io))],
            Next::Done(Outcome::Unknown(Fault::Io)),
        ),
        (
            "clock then jitter",
            2,
            vec![outbox.clone(), not_sent.clone(), clock.clone()],
            Next::Request(Operation::Random(
                Purpose::RetryJitter,
                some(NonZeroU64::new(2)),
            )),
        ),
        (
            "clock of another epoch",
            2,
            vec![
                outbox.clone(),
                not_sent.clone(),
                Response::Clock(Ok(EpochTick { epoch: 8, tick: 5 })),
            ],
            Next::Done(Outcome::Refused(Code::Protocol)),
        ),
        (
            "clock failure",
            2,
            vec![
                outbox.clone(),
                not_sent.clone(),
                Response::Clock(Err(Fault::Busy)),
            ],
            Next::Done(Outcome::NotApplied(Fault::Busy)),
        ),
        (
            "jitter sets the deadline",
            2,
            vec![
                outbox.clone(),
                not_sent.clone(),
                clock.clone(),
                draw.clone(),
            ],
            Next::Request(Operation::Wait(tick(9))),
        ),
        (
            "out-of-range draw",
            2,
            vec![
                outbox.clone(),
                not_sent.clone(),
                clock.clone(),
                Response::Random(Ok(2)),
            ],
            Next::Done(Outcome::Refused(Code::Protocol)),
        ),
        (
            "jitter failure",
            2,
            vec![
                outbox.clone(),
                not_sent.clone(),
                clock.clone(),
                Response::Random(Err(Fault::Io)),
            ],
            Next::Done(Outcome::NotApplied(Fault::Io)),
        ),
        (
            "wake at the deadline resends the exact record",
            2,
            vec![
                outbox.clone(),
                not_sent.clone(),
                clock.clone(),
                draw.clone(),
                Response::Wait(Ok(tick(9))),
            ],
            Next::Request(Operation::Send(store(1), first.clone())),
        ),
        (
            "early wake",
            2,
            vec![
                outbox.clone(),
                not_sent.clone(),
                clock.clone(),
                draw.clone(),
                Response::Wait(Ok(tick(8))),
            ],
            Next::Done(Outcome::Refused(Code::Protocol)),
        ),
        (
            "interrupted wait",
            2,
            vec![
                outbox.clone(),
                not_sent.clone(),
                clock.clone(),
                draw.clone(),
                Response::Wait(Err(WaitError::Interrupted)),
            ],
            Next::Done(Outcome::Refused(Code::Interrupted)),
        ),
        (
            "failed wait",
            2,
            vec![
                outbox,
                not_sent,
                clock,
                draw,
                Response::Wait(Err(WaitError::Failed(Fault::Busy))),
            ],
            Next::Done(Outcome::NotApplied(Fault::Busy)),
        ),
    ]
}

/// The commit program restarted after a crash recovers before any retry.
fn recover_cases() -> Vec<(&'static str, Vec<Response>, Next<CommitReceipt>)> {
    let reopened = Response::Reopen(Outcome::Ok(snapshot(4)));
    vec![
        (
            "restart reopens first",
            vec![],
            Next::Request(Operation::Reopen(store(1))),
        ),
        (
            "restart reads history",
            vec![reopened.clone()],
            Next::Request(Operation::ReadHistory(store(1))),
        ),
        (
            "restart retries unversioned on the exact key",
            vec![
                reopened.clone(),
                Response::ReadHistory(Outcome::Ok(history(&[(hash(0x20), publication(120))]))),
            ],
            commit_request(CommitMode::Unversioned),
        ),
        (
            "restart keeps the original guard on absence",
            vec![reopened, Response::ReadHistory(Outcome::Ok(history(&[])))],
            commit_request(CommitMode::At(3)),
        ),
        (
            "restart with a refused reopen stays unknown",
            vec![Response::Reopen(Outcome::Refused(Code::History))],
            Next::Done(Outcome::Unknown(Fault::Io)),
        ),
    ]
}

/// Lists the conformance cases the step functions break.
fn conformance_failures() -> Vec<&'static str> {
    let mut failures = Vec::new();
    for (name, retries, responses, expected) in commit_cases() {
        let mut driver = commit_driver(retries);
        answer(&mut driver, responses);
        if next(&driver) != expected {
            failures.push(name);
        }
    }
    for (name, retries, responses, expected) in delivery_cases() {
        let mut driver = delivery_driver(retries);
        answer(&mut driver, responses);
        if next(&driver) != expected {
            failures.push(name);
        }
    }
    for (name, responses, expected) in recover_cases() {
        let mut driver = ok(Driver::<CommitProgram>::start(
            REGISTRY,
            EPOCH,
            CommitProgram::recover(invocation(CommitMode::At(3)), CommitPolicy { retries: 2 }),
        ));
        answer(&mut driver, responses);
        if next(&driver) != expected {
            failures.push(name);
        }
    }
    failures
}

fn hex(hash: Hash32) -> alloc::string::String {
    let mut out = alloc::string::String::new();
    for byte in hash.as_bytes() {
        out.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        out.push(char::from(b"0123456789abcdef"[usize::from(byte & 15)]));
    }
    out
}

#[test]
fn step_conformance() {
    let computed = hex(registry::program_source());
    let bound = computed == PROGRAM_SOURCE_SHA256;
    let failures = conformance_failures();
    assert!(
        bound && failures.is_empty(),
        "program source bound: {bound} (computed {computed}); conformance failures: {failures:?}"
    );
}

#[test]
fn port_registry() {
    assert_eq!(registry::check(&REGISTRY), Ok(()));
    // The registry's identity; any change to its data needs a new pin.
    assert_eq!(
        hex(registry::identity(&REGISTRY)),
        "9a88849d214cb711e5d173b16048baa58d40e5933faeff0df8a7220ff9599b89"
    );
    for port in Port::ALL {
        assert_eq!(some(registry::signature(&REGISTRY, port)).port, port);
    }
    // Registry borrows static data. Construct the mutant fixtures at compile
    // time rather than leaking heap allocations to manufacture that lifetime.
    const fn complete() -> [Signature; 10] {
        let mut result = [SIGNATURES[0]; 10];
        let mut index = 0;
        while index < result.len() {
            result[index] = SIGNATURES[index];
            index += 1;
        }
        result
    }
    const fn without(removed: usize) -> [Signature; 9] {
        let mut result = [SIGNATURES[0]; 9];
        let mut source = 0;
        let mut target = 0;
        while source < SIGNATURES.len() {
            if source != removed {
                result[target] = SIGNATURES[source];
                target += 1;
            }
            source += 1;
        }
        result
    }
    macro_rules! with {
        ($signatures:expr) => {
            Registry {
                signatures: &const { $signatures },
                ..REGISTRY
            }
        };
    }
    assert_eq!(
        registry::check(&with!(without(5))),
        Err(RegistryError::Missing(Port::Acknowledge))
    );
    assert_eq!(
        registry::check(&with!(without(9))),
        Err(RegistryError::Missing(Port::Wait))
    );
    assert_eq!(
        registry::check(&with!({
            let mut reordered = complete();
            reordered.swap(1, 2);
            reordered
        })),
        Err(RegistryError::Missing(Port::Commit))
    );
    assert_eq!(
        registry::check(&with!({
            let mut repeated = complete();
            repeated[1] = SIGNATURES[0];
            repeated
        })),
        Err(RegistryError::Order(Port::ReadState))
    );
    assert_eq!(
        registry::check(&with!({
            let mut extra = [SIGNATURES[0]; 11];
            let mut index = 0;
            while index < SIGNATURES.len() {
                extra[index] = SIGNATURES[index];
                index += 1;
            }
            extra
        })),
        Err(RegistryError::Order(Port::ReadState))
    );
    macro_rules! changed {
        ($index:expr, |$signature:ident| $edit:expr) => {
            registry::check(&with!({
                let mut signatures = complete();
                let $signature = &mut signatures[$index];
                $edit;
                signatures
            }))
        };
    }
    // A read port that writes durable state exceeds its capability.
    assert_eq!(
        changed!(0, |s| s.footprint.writes =
            s.footprint.writes.and(registry::Resources::DURABLE)),
        Err(RegistryError::Footprint(Port::ReadState))
    );
    // A transport that touches the store exceeds its capability.
    assert_eq!(
        changed!(4, |s| s.footprint.reads = registry::Resources::DURABLE),
        Err(RegistryError::Footprint(Port::Send))
    );
    assert_eq!(
        changed!(1, |s| s.laws = &[]),
        Err(RegistryError::Law(Port::Commit))
    );
    assert_eq!(
        changed!(1, |s| s.laws = &[1, 1]),
        Err(RegistryError::Law(Port::Commit))
    );
    assert_eq!(
        changed!(1, |s| s.laws = &[1, 99]),
        Err(RegistryError::Law(Port::Commit))
    );
    assert_eq!(
        changed!(1, |s| s.laws = &[1]),
        Err(RegistryError::UnusedLaw(2))
    );
    assert_eq!(
        changed!(0, |s| s.v2 = V2::OptIn),
        Err(RegistryError::Unmapped(Port::ReadState))
    );
    assert_eq!(
        changed!(5, |s| s.v2 = V2::Api(&[])),
        Err(RegistryError::Unmapped(Port::Acknowledge))
    );
    assert_eq!(
        changed!(0, |s| s.arguments = Kind::Hash),
        Err(RegistryError::Malformed(Port::ReadState, "arguments"))
    );
    assert_eq!(
        changed!(0, |s| s.answer = Answer::Outcome(Kind::Bytes(0))),
        Err(RegistryError::Malformed(Port::ReadState, "byte cap"))
    );
    assert_eq!(
        changed!(0, |s| s.answer = Answer::Outcome(Kind::Int(2, 1))),
        Err(RegistryError::Malformed(Port::ReadState, "range"))
    );
    const TWICE: &[registry::Field] = &[
        registry::Field {
            name: "store",
            kind: Kind::Store,
        },
        registry::Field {
            name: "store",
            kind: Kind::Store,
        },
    ];
    assert_eq!(
        changed!(0, |s| s.arguments = Kind::Record(TWICE)),
        Err(RegistryError::Malformed(Port::ReadState, "name"))
    );
    assert_eq!(
        registry::check(&Registry {
            schema: "zeno-fcis/shell-effects/2",
            ..REGISTRY
        }),
        Err(RegistryError::Schema)
    );
    const UNUSED: &[Law] = &[
        registry::LAWS[0],
        registry::LAWS[1],
        registry::LAWS[2],
        registry::LAWS[3],
        registry::LAWS[4],
        registry::LAWS[5],
        registry::LAWS[6],
        registry::LAWS[7],
        registry::LAWS[8],
        Law {
            id: 10,
            name: "orphan",
            statement: "named by no port",
        },
    ];
    assert_eq!(
        registry::check(&Registry {
            laws: UNUSED,
            ..REGISTRY
        }),
        Err(RegistryError::UnusedLaw(10))
    );
    // The identity covers the data: a changed statement changes it.
    const OTHER_LAWS: [Law; 9] = {
        let mut laws = [registry::LAWS[0]; 9];
        let mut index = 0;
        while index < laws.len() {
            laws[index] = registry::LAWS[index];
            index += 1;
        }
        laws[0].statement = "changed";
        laws
    };
    let other = Registry {
        laws: &OTHER_LAWS,
        ..REGISTRY
    };
    assert_ne!(registry::identity(&other), registry::identity(&REGISTRY));
    assert_eq!(registry::identity(&REGISTRY), registry::identity(&REGISTRY));
}

/// A program that requests a port it does not list.
struct Undeclared;
impl Program for Undeclared {
    type Continuation = ();
    type Output = ();
    const PORTS: &'static [Port] = &[Port::ReadState];
    const CAPABILITIES: &'static [Capability] = &[Capability::ReadStore];
    fn resume(_: (), _: Response) -> Step<(), ()> {
        Step::Done(Outcome::Ok(()))
    }
}

/// A program that lists a port without declaring its capability.
struct Overreaching;
impl Program for Overreaching {
    type Continuation = ();
    type Output = ();
    const PORTS: &'static [Port] = &[Port::ReadState, Port::Commit];
    const CAPABILITIES: &'static [Capability] = &[Capability::ReadStore];
    fn resume(_: (), _: Response) -> Step<(), ()> {
        Step::Done(Outcome::Ok(()))
    }
}

#[test]
fn capability_footprint() {
    // Every port each program lists has its capability declared.
    for (ports, capabilities) in [
        (CommitProgram::PORTS, CommitProgram::CAPABILITIES),
        (DeliveryProgram::PORTS, DeliveryProgram::CAPABILITIES),
    ] {
        for port in ports {
            let capability = some(registry::signature(&REGISTRY, *port)).capability;
            assert!(capabilities.contains(&capability), "{port:?}");
        }
    }
    let first = |operation| Step::Request(OpId::first(EPOCH), operation, ());
    assert_eq!(
        Driver::<Overreaching>::start(REGISTRY, EPOCH, first(Operation::ReadState(store(1)))).err(),
        Some(Fatal::Capability(Port::Commit))
    );
    assert_eq!(
        Driver::<Undeclared>::start(REGISTRY, EPOCH, first(Operation::Clock)).err(),
        Some(Fatal::Capability(Port::Clock))
    );
    assert_eq!(
        Driver::<Undeclared>::start(
            REGISTRY,
            EPOCH,
            Step::Request(
                OpId {
                    epoch: EPOCH,
                    ordinal: 1
                },
                Operation::ReadState(store(1)),
                ()
            ),
        )
        .err(),
        Some(Fatal::Sequence)
    );
    let broken = Registry {
        signatures: &SIGNATURES[..9],
        ..REGISTRY
    };
    assert_eq!(
        Driver::<Undeclared>::start(broken, EPOCH, first(Operation::ReadState(store(1)))).err(),
        Some(Fatal::Registry(RegistryError::Missing(Port::Wait)))
    );
}

#[test]
fn type_check() {
    let mut big = publication(120);
    big.command = vec![0; registry::MAX_BYTES as usize + 1];
    let operations = [
        (Operation::ReadState(store(1)), Ok(())),
        (
            Operation::Commit(store(1), hash(1), publication(120), CommitMode::Unversioned),
            Ok(()),
        ),
        (
            Operation::Commit(
                store(1),
                hash(1),
                publication(120),
                CommitMode::At(i64::MAX as u64),
            ),
            Ok(()),
        ),
        (
            Operation::Commit(
                store(1),
                hash(1),
                publication(120),
                CommitMode::BoundedAt(2, 4096),
            ),
            Ok(()),
        ),
        (
            Operation::Commit(
                store(1),
                hash(1),
                publication(120),
                CommitMode::At(u64::MAX),
            ),
            Err(TypeError::Range("at")),
        ),
        (
            Operation::Commit(store(1), hash(1), big, CommitMode::Unversioned),
            Err(TypeError::TooLong("command")),
        ),
        (Operation::ReadHistory(store(1)), Ok(())),
        (Operation::ReadOutbox(store(1)), Ok(())),
        (Operation::Send(store(1), delivery(0)), Ok(())),
        (
            Operation::Send(
                store(1),
                Delivery {
                    lane: 2,
                    ..delivery(0)
                },
            ),
            Err(TypeError::Range("lane")),
        ),
        (
            Operation::Send(
                store(1),
                Delivery {
                    version: 0,
                    ..delivery(0)
                },
            ),
            Err(TypeError::Range("version")),
        ),
        (Operation::Acknowledge(store(1), hash(1), hash(2)), Ok(())),
        (Operation::Reopen(store(1)), Ok(())),
        (Operation::Clock, Ok(())),
        (
            Operation::Random(Purpose::RetryJitter, some(NonZeroU64::new(5))),
            Ok(()),
        ),
        (Operation::Wait(EpochTick { epoch: 1, tick: 2 }), Ok(())),
    ];
    for (operation, expected) in operations {
        let signature = some(registry::signature(&REGISTRY, operation.port()));
        assert_eq!(
            registry::conforms(signature.arguments, &operation.value(), "arguments"),
            expected,
            "{operation:?}"
        );
    }
    let responses = [
        (Response::ReadState(Outcome::Ok(snapshot(1))), Ok(())),
        (
            Response::ReadState(Outcome::Ok(Snapshot {
                version: u64::MAX,
                ..snapshot(1)
            })),
            Err(TypeError::Range("version")),
        ),
        (
            Response::Commit(Outcome::Ok(receipt(CommitStatus::IdempotentReplay))),
            Ok(()),
        ),
        (
            Response::ReadHistory(Outcome::Ok(history(&[(hash(1), publication(1))]))),
            Ok(()),
        ),
        (
            Response::ReadOutbox(Outcome::Ok(vec![delivery(0), delivery(1)])),
            Ok(()),
        ),
        (Response::Send(Sent::Sent(report(&delivery(0)))), Ok(())),
        (Response::Acknowledge(Outcome::Ok(())), Ok(())),
        (Response::Reopen(Outcome::Ok(snapshot(2))), Ok(())),
        (Response::Clock(Ok(EpochTick { epoch: 1, tick: 3 })), Ok(())),
        (Response::Random(Ok(4)), Ok(())),
        (
            Response::Random(Ok(u64::MAX)),
            Err(TypeError::Range("result")),
        ),
        (Response::Wait(Ok(EpochTick { epoch: 1, tick: 3 })), Ok(())),
    ];
    for (response, expected) in responses {
        let signature = some(registry::signature(&REGISTRY, response.port()));
        let value = some(response.value());
        assert_eq!(
            registry::conforms(signature.answer.payload(), &value, "result"),
            expected,
            "{response:?}"
        );
    }
    // Failures carry no sized data; a shape mismatch is named.
    assert_eq!(
        Response::Commit(Outcome::Refused(Code::Replay)).value(),
        None
    );
    assert_eq!(
        registry::conforms(Kind::Hash, &Val::Int(1), "key"),
        Err(TypeError::Shape("key"))
    );
}

/// Replies to the first request of a fresh commit run with `binding` edited;
/// returns the refusal and asserts that the program did not advance.
fn refuse(edit: fn(&mut Binding), response: Response) -> Result<(), Refusal> {
    let mut driver = commit_driver(2);
    let binding = some(driver.request()).0.clone();
    let mut wrong = binding.clone();
    edit(&mut wrong);
    let refusal = driver.reply(Reply {
        binding: wrong,
        response,
    });
    // A refused response leaves the same request outstanding.
    let unchanged = some(driver.request()).0 == &binding;
    assert!(
        unchanged,
        "a refused response advanced the program: {refusal:?}"
    );
    refusal
}

fn lost() -> Response {
    Response::Commit(Outcome::Unknown(Fault::Io))
}

#[test]
fn wrong_store_response() {
    assert_eq!(
        refuse(
            |b| b.store = Some(Store {
                identity: hash(0xa1),
                handle: 1
            }),
            lost()
        ),
        Err(Refusal::Store)
    );
    assert_eq!(
        refuse(|b| b.store = Some(store(2)), lost()),
        Err(Refusal::Handle)
    );
    assert_eq!(refuse(|b| b.store = None, lost()), Err(Refusal::Store));
}

#[test]
fn duplicate_response() {
    // A repeated answer to a retried commit has the same port, store and
    // subject as the retry; only its ordinal shows it is a duplicate.
    let mut driver = commit_driver(2);
    let first = some(driver.request()).0.clone();
    let busy = Response::Commit(Outcome::NotApplied(Fault::Busy));
    ok(driver.reply(Reply {
        binding: first.clone(),
        response: busy.clone(),
    }));
    let retry = some(driver.request()).0.clone();
    assert_eq!(
        (retry.port, retry.store, &retry.subject),
        (first.port, first.store, &first.subject)
    );
    assert_eq!(
        driver.reply(Reply {
            binding: first,
            response: busy,
        }),
        Err(Refusal::Duplicate),
        "a duplicated response was admitted"
    );
    assert_eq!(some(driver.request()).0, &retry);
    // After two answers, a repeat of the first is stale and of the second a
    // duplicate.
    let mut driver = commit_driver(2);
    let first = some(driver.request()).0.clone();
    ok(driver.reply(Reply {
        binding: first.clone(),
        response: lost(),
    }));
    let reopen = some(driver.request()).0.clone();
    assert_eq!(reopen.id.ordinal, 1);
    let reopened = Response::Reopen(Outcome::Ok(snapshot(4)));
    ok(driver.reply(Reply {
        binding: reopen.clone(),
        response: reopened.clone(),
    }));
    assert_eq!(
        driver.reply(Reply {
            binding: first,
            response: lost(),
        }),
        Err(Refusal::Stale)
    );
    assert_eq!(
        driver.reply(Reply {
            binding: reopen,
            response: reopened,
        }),
        Err(Refusal::Duplicate)
    );
    assert_eq!(
        next(&driver),
        Next::Request(Operation::ReadHistory(store(1)))
    );
    // A finished run admits nothing; its last answer is still a duplicate.
    let mut driver = commit_driver(0);
    let first = some(driver.request()).0.clone();
    let done = Response::Commit(Outcome::Ok(receipt(CommitStatus::Committed)));
    ok(driver.reply(Reply {
        binding: first.clone(),
        response: done.clone(),
    }));
    assert_eq!(
        driver.reply(Reply {
            binding: first.clone(),
            response: done.clone(),
        }),
        Err(Refusal::Duplicate)
    );
    assert_eq!(
        driver.reply(Reply {
            binding: Binding {
                id: OpId {
                    epoch: EPOCH,
                    ordinal: 5
                },
                ..first
            },
            response: done,
        }),
        Err(Refusal::Idle)
    );
}

#[test]
fn response_binding() {
    let driver = commit_driver(2);
    let (binding, operation) = some(driver.request());
    assert_eq!(*binding, Binding::of(OpId::first(EPOCH), operation));
    assert_eq!(binding.subject, publication(120).subject);
    assert_eq!(
        refuse(|b| b.id.epoch = EPOCH + 1, lost()),
        Err(Refusal::Epoch)
    );
    assert_eq!(refuse(|b| b.id.ordinal = 1, lost()), Err(Refusal::Ordinal));
    assert_eq!(refuse(|b| b.port = Port::Reopen, lost()), Err(Refusal::Tag));
    assert_eq!(
        refuse(|_| {}, Response::Reopen(Outcome::Ok(snapshot(1)))),
        Err(Refusal::Tag)
    );
    assert_eq!(refuse(|b| b.subject[0] ^= 1, lost()), Err(Refusal::Subject));
    assert_eq!(
        refuse(
            |_| {},
            Response::Commit(Outcome::Ok(CommitReceipt {
                version: u64::MAX,
                ..receipt(CommitStatus::Committed)
            }))
        ),
        Err(Refusal::Type(TypeError::Range("version")))
    );
    // The bound response is admitted.
    assert_eq!(
        admitted_first_answer(),
        Next::Request(Operation::Reopen(store(1)))
    );
    // Delivery requests bind the delivery ID.
    let mut driver = delivery_driver(1);
    answer(
        &mut driver,
        vec![Response::ReadOutbox(Outcome::Ok(vec![delivery(0)]))],
    );
    let send = some(driver.request()).0.clone();
    assert_eq!(send.subject, delivery(0).id.as_bytes().to_vec());
    assert_eq!(
        driver.reply(Reply {
            binding: Binding {
                subject: delivery(1).id.as_bytes().to_vec(),
                ..send
            },
            response: Response::Send(Sent::Sent(report(&delivery(0)))),
        }),
        Err(Refusal::Subject)
    );
}

/// The correctly bound answer to the first request advances the program.
fn admitted_first_answer() -> Next<CommitReceipt> {
    let mut driver = commit_driver(2);
    let binding = some(driver.request()).0.clone();
    ok(driver.reply(Reply {
        binding,
        response: lost(),
    }));
    next(&driver)
}

#[test]
fn ordinals_never_wrap() {
    let last = OpId {
        epoch: EPOCH,
        ordinal: u64::MAX,
    };
    assert_eq!(last.next(), None);
    assert_eq!(
        OpId::first(EPOCH).next(),
        Some(OpId {
            epoch: EPOCH,
            ordinal: 1
        })
    );
}

#[test]
fn delivery_runs_to_completion() {
    let mut driver = delivery_driver(2);
    let (first, second) = (delivery(0), delivery(1));
    answer(
        &mut driver,
        vec![
            Response::ReadOutbox(Outcome::Ok(vec![first.clone(), second.clone()])),
            Response::Send(Sent::Unknown(Fault::Io)),
            Response::Clock(Ok(EpochTick {
                epoch: EPOCH,
                tick: 0,
            })),
            Response::Random(Ok(0)),
            Response::Wait(Ok(EpochTick {
                epoch: EPOCH,
                tick: 3,
            })),
            Response::Send(Sent::Sent(report(&first))),
            Response::Acknowledge(Outcome::Ok(())),
            Response::Send(Sent::Sent(report(&second))),
            Response::Acknowledge(Outcome::Ok(())),
        ],
    );
    assert_eq!(next(&driver), Next::Done(Outcome::Ok(2)));
}
