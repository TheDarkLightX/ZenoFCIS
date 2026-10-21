//! Real loopback HTTP, production SQLite effects, and the independent table oracle.
#![allow(dead_code)]
#[path = "../../zeno-fcis-shell-sqlite/tests/sim/database.rs"]
mod database;
#[path = "../../zeno-fcis-shell-sqlite/tests/sim/fixture.rs"]
mod fixture;
#[path = "../../zeno-fcis-shell-sqlite/tests/sim/model.rs"]
mod model;
mod support;

use database::{Files, Receiver};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    path::Path,
    time::Duration,
};
use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};
use zeno_fcis_ingress::{
    Command, Config, Durable, Endpoint, Event, Refusal, Server, Trace, TraceSink, encode_command,
    runtime,
};
use zeno_fcis_shell::effects::{
    driver::Binding,
    handler::{Fault, Handled, Terminal},
    port::{self, Delivery, EpochTick, Operation, Report, Sent, WaitError},
    registry::Port,
    simulation::Script,
};
use zeno_fcis_shell_sqlite::{
    CrashPoint,
    effects::production::{Production, Time, Transport},
    v2::{Lineage, V2SqliteShell},
};
fn ok<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
    value.unwrap_or_else(|e| panic!("unexpected: {e:?}"))
}
fn key(n: u64) -> Hash32 {
    let mut bytes = [0; 32];
    bytes[..8].copy_from_slice(&n.to_be_bytes());
    Hash32::new(bytes)
}
fn payload_hash(bytes: &[u8]) -> Hash32 {
    RustCryptoSha256::hash(bytes)
}
fn principal() -> Hash32 {
    Hash32::new([7; 32])
}
fn scoped(contract: Hash32, retry: Hash32) -> Hash32 {
    let mut out = b"zeno-fcis/ingress-key/1".to_vec();
    out.extend_from_slice(principal().as_bytes());
    out.extend_from_slice(contract.as_bytes());
    out.extend_from_slice(retry.as_bytes());
    payload_hash(&out)
}
fn create(files: &Files, authority: &zeno_fcis_synthesis::finite::v2_authority::Authority<'_>) {
    let initial = model::state(fixture::schema_hash(), [0, 0]);
    drop(ok(V2SqliteShell::create(
        &files.store,
        authority,
        fixture::genesis(authority, &initial),
    )));
}
#[derive(Default)]
struct Sink {
    records: Vec<Trace>,
    file: Option<std::fs::File>,
    abort_reply: bool,
}
impl Sink {
    fn file(path: &Path) -> Self {
        Self {
            file: Some(ok(std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path))),
            ..Self::default()
        }
    }
    fn save(&self, name: &str) {
        if let Ok(dir) = std::env::var("INGRESS_TRACE_DIR") {
            let text = self
                .records
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            ok(std::fs::write(
                Path::new(&dir).join(format!("{name}.trace")),
                text,
            ));
        }
    }
}
impl TraceSink for Sink {
    fn record(&mut self, trace: Trace) {
        self.records.push(trace);
        if let Some(file) = &mut self.file {
            ok(writeln!(file, "{trace}"));
            ok(file.sync_all());
        }
        if self.abort_reply && trace.event == Event::Reply {
            std::process::abort();
        }
    }
}
struct Wire {
    receiver: Receiver,
    mode: u8,
    queued: Vec<Delivery>,
    abort: bool,
}
impl Wire {
    fn receive(&mut self, d: &Delivery) {
        self.receiver.receive(
            &model::Delivery {
                sequence: d.version,
                ordinal: d.ordinal,
                id: d.id,
                payload: d.payload.clone(),
                row: Vec::new(),
                acknowledged: false,
            },
            false,
        );
    }
}
impl Transport for Wire {
    fn send(&mut self, d: &Delivery) -> Sent {
        if self.mode == 2 && self.queued.is_empty() && d.ordinal == 0 {
            self.queued.push(d.clone());
        } else {
            self.receive(d);
            if self.mode == 1 {
                self.receive(d);
            }
            let queued = std::mem::take(&mut self.queued);
            for old in queued {
                self.receive(&old);
            }
        }
        if self.abort {
            std::process::abort();
        }
        Sent::Sent(Report {
            delivery_id: d.id,
            payload_sha256: d.payload_sha256,
        })
    }
}
#[derive(Default)]
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
struct Faults<H> {
    inner: H,
    choices: VecDeque<(Port, u8)>,
}
impl<H: Terminal> Terminal for Faults<H> {
    fn ports(&self) -> &'static [Port] {
        self.inner.ports()
    }
    fn handle(&mut self, binding: &Binding, operation: &Operation) -> Handled {
        if self
            .choices
            .front()
            .is_some_and(|(port, _)| *port == operation.port())
        {
            let (_, choice) = self.choices.pop_front().unwrap_or_else(|| panic!("choice"));
            Fault::new(&mut self.inner, &mut Script::new(vec![choice])).handle(binding, operation)
        } else {
            self.inner.handle(binding, operation)
        }
    }
    fn crash(&mut self, binding: &Binding) {
        self.inner.crash(binding);
    }
}
fn config(contract: Hash32) -> Config {
    ok(Config::new(
        [0xab; 32],
        principal(),
        contract,
        "https://app.example".into(),
        Duration::from_secs(2),
    ))
}
fn open_production<'a, 'p>(
    files: &Files,
    lineage: &'a Lineage<'a, 'p>,
    mode: u8,
) -> Production<'a, 'p, Wire, Ticks> {
    ok(Production::open(
        &files.store,
        lineage,
        1,
        Wire {
            receiver: Receiver::open(&files.receiver),
            mode,
            queued: Vec::new(),
            abort: false,
        },
        Some(Ticks::default()),
    ))
}
fn body(contract: Hash32, variant: u16, retry: u64, expected: u64) -> Vec<u8> {
    encode_command(contract, key(retry), expected, &fixture::command(variant))
}
fn wire(body: &[u8]) -> Vec<u8> {
    let mut bytes=format!("POST /command HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nOrigin: https://app.example\r\nContent-Length: {}\r\n\r\n","ab".repeat(32),body.len()).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
fn client(address: std::net::SocketAddr, request: Vec<u8>, lost: bool) -> Vec<u8> {
    let mut stream = ok(TcpStream::connect(address));
    ok(stream.set_read_timeout(Some(Duration::from_secs(5))));
    ok(stream.write_all(&request));
    let _ = stream.shutdown(Shutdown::Write);
    if lost {
        return Vec::new();
    }
    let mut out = Vec::new();
    let _ = stream.read_to_end(&mut out);
    out
}
fn exchange<B: Endpoint>(server: &mut Server<B, Sink>, body: &[u8], lost: bool) -> Vec<u8> {
    let rt = ok(runtime());
    if std::env::var_os("INGRESS_TEST_MEMORY").is_some() {
        let reply = support::exchange(server, wire(body), false);
        return if lost { Vec::new() } else { reply };
    }
    let listener = ok(rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")));
    let address = ok(listener.local_addr());
    let request = wire(body);
    let thread = std::thread::spawn(move || client(address, request, lost));
    let _ = rt.block_on(server.serve_one(&listener));
    ok(thread.join())
}
fn status(bytes: &[u8]) -> u16 {
    std::str::from_utf8(
        bytes
            .split(|byte| *byte == b'\r')
            .next()
            .unwrap_or_default(),
    )
    .ok()
    .and_then(|s| s.split_whitespace().nth(1))
    .and_then(|s| s.parse().ok())
    .unwrap_or(0)
}
fn packet(bytes: &[u8]) -> &[u8] {
    let i = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("response"));
    &bytes[i + 4..]
}
fn expected(
    authority: &zeno_fcis_synthesis::finite::v2_authority::Authority<'_>,
    contract: Hash32,
    ack: usize,
) -> model::Model {
    let mut model = model::Model::new(fixture::model_genesis(authority));
    model.commit(
        fixture::input(authority, scoped(contract, key(1)), [0, 0], 120),
        0,
        Some(0),
    );
    for d in model.deliveries.clone().iter().take(ack) {
        model.acknowledge(d.id, payload_hash(&d.payload));
    }
    model
}
fn compare(files: &Files, model: &model::Model) {
    ok(database::compare(
        &model.database(),
        &database::snapshot(&files.store),
    ));
    model.invariants();
}
fn final_state(
    files: &Files,
    authority: &zeno_fcis_synthesis::finite::v2_authority::Authority<'_>,
    contract: Hash32,
) {
    let model = expected(authority, contract, 2);
    compare(files, &model);
    let receiver = Receiver::open(&files.receiver);
    for d in &model.deliveries {
        assert_eq!(receiver.effect_count(d.id), 1);
        assert_eq!(receiver.state().get(&d.id), Some(&d.payload));
    }
    receiver.invariants();
}
fn traces(sink: &Sink) {
    let text = sink
        .records
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!text.contains(&"ab".repeat(32)), "secret in trace");
    for event in [
        Event::Request,
        Event::Decision,
        Event::Commit,
        Event::Send,
        Event::Acknowledge,
        Event::Reply,
    ] {
        assert!(
            sink.records.iter().any(|t| t.event == event),
            "missing {event:?}"
        );
    }
    for t in sink
        .records
        .iter()
        .filter(|t| matches!(t.event, Event::Send | Event::Acknowledge))
    {
        assert!(
            t.request.is_some()
                && t.replay.is_some()
                && t.decision.is_some()
                && t.commit.is_some()
                && t.delivery.is_some()
        );
    }
}

#[test]
fn durable_journey_and_changed_key() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        let files = Files::new();
        create(&files, authority);
        let production = open_production(&files, &lineage, 0);
        let store = production.store();
        let durable = ok(Durable::new(
            authority,
            production,
            store,
            principal(),
            &fixture::context(),
        ));
        let mut server = Server::new(config(contract), durable, Sink::default());
        let first = exchange(&mut server, &body(contract, 120, 1, 0), false);
        assert_eq!(status(&first), 200, "first command");
        final_state(&files, authority, contract);
        let replay = exchange(&mut server, &body(contract, 120, 1, 0), false);
        assert_eq!(status(&replay), 200);
        assert_eq!(packet(&first), packet(&replay), "packet changed on retry");
        let before = database::snapshot(&files.store);
        for changed in [body(contract, 121, 1, 0), body(contract, 120, 1, 1)] {
            let reply = exchange(&mut server, &changed, false);
            assert_eq!(
                status(&reply),
                409,
                "changed command accepted under old key"
            );
            assert_eq!(database::snapshot(&files.store), before);
        }
        assert_eq!(
            status(&exchange(&mut server, &body(contract, 121, 2, 0), false)),
            409,
            "stale expected revision accepted"
        );
        let second = exchange(&mut server, &body(contract, 121, 2, 1), false);
        assert_eq!(status(&second), 200, "second revision");
        let old = exchange(&mut server, &body(contract, 120, 1, 0), false);
        assert_eq!(status(&old), 200);
        assert_eq!(
            packet(&old),
            packet(&first),
            "old packet changed after later commit"
        );
        let mut model = expected(authority, contract, 2);
        model.commit(
            fixture::input(authority, scoped(contract, key(2)), [1, 0], 121),
            0,
            Some(1),
        );
        for d in model.deliveries.clone() {
            if d.sequence == 2 {
                model.acknowledge(d.id, payload_hash(&d.payload));
            }
        }
        compare(&files, &model);
        let (_, sink) = server.into_parts();
        traces(&sink);
        sink.save("journey");
    });
}

#[test]
fn loopback_stage2_fault_schedules() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        // Every applicable choice of the stage-2 Fault layer, one fault per schedule.
        for port in [
            Port::ReadState,
            Port::ReadHistory,
            Port::ReadOutbox,
            Port::Reopen,
            Port::Commit,
            Port::Acknowledge,
            Port::Send,
            Port::Clock,
            Port::Random,
            Port::Wait,
        ] {
            let options = match port {
                Port::Commit | Port::Acknowledge => 8,
                Port::Send => 7,
                Port::Clock | Port::Random => 4,
                _ => 5,
            };
            for choice in 0..options {
                let files = Files::new();
                create(&files, authority);
                let production = open_production(&files, &lineage, 0);
                let store = production.store();
                let mut choices = VecDeque::new();
                if port == Port::Reopen {
                    choices.push_back((Port::Commit, 3));
                }
                if matches!(port, Port::Clock | Port::Random | Port::Wait) {
                    choices.push_back((Port::Send, 1));
                }
                choices.push_back((port, choice));
                let durable = ok(Durable::new(
                    authority,
                    Faults {
                        inner: production,
                        choices,
                    },
                    store,
                    principal(),
                    &fixture::context(),
                ));
                let mut server = Server::new(config(contract), durable, Sink::default());
                let first = exchange(&mut server, &body(contract, 120, 1, 0), false);
                assert!(
                    [200, 503].contains(&status(&first)),
                    "{port:?} {choice}: {}",
                    status(&first)
                );
                // A first attempt may stop at any atomic prefix; no partial publication.
                let observed = database::snapshot(&files.store);
                let mut allowed = vec![model::Model::new(fixture::model_genesis(authority))];
                for ack in 0..=2 {
                    allowed.push(expected(authority, contract, ack));
                }
                assert!(
                    allowed
                        .iter()
                        .any(|m| database::compare(&m.database(), &observed).is_ok()),
                    "non-atomic fault result {port:?} {choice}"
                );
                let recovered = exchange(&mut server, &body(contract, 120, 1, 0), false);
                assert_eq!(status(&recovered), 200, "{port:?} {choice}");
                final_state(&files, authority, contract);
                let (durable, sink) = server.into_parts();
                assert!(
                    durable.into_terminal().choices.is_empty(),
                    "fault was not reached {port:?} {choice}"
                );
                traces(&sink);
                sink.save(&format!("fault-{port:?}-{choice}"));
            }
        }
    });
}

#[test]
fn loopback_transaction_boundaries() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        for point in points() {
            let files = Files::new();
            create(&files, authority);
            let mut production = open_production(&files, &lineage, 0);
            production.interrupt_next_commit(point);
            let store = production.store();
            let durable = ok(Durable::new(
                authority,
                production,
                store,
                principal(),
                &fixture::context(),
            ));
            let mut server = Server::new(config(contract), durable, Sink::default());
            assert_eq!(
                status(&exchange(&mut server, &body(contract, 120, 1, 0), false)),
                503
            );
            if point == CrashPoint::AfterCommit {
                compare(&files, &expected(authority, contract, 0));
            } else {
                compare(
                    &files,
                    &model::Model::new(fixture::model_genesis(authority)),
                );
            }
            assert_eq!(
                status(&exchange(&mut server, &body(contract, 120, 1, 0), false)),
                200
            );
            final_state(&files, authority, contract);
            let (_, sink) = server.into_parts();
            traces(&sink);
            sink.save(&format!("statement-{point:?}"));
        }
    });
}
fn points() -> [CrashPoint; 8] {
    [
        CrashPoint::BeforeTransaction,
        CrashPoint::AfterValidation,
        CrashPoint::AfterStateWrite,
        CrashPoint::AfterReplayWrite,
        CrashPoint::AfterFirstOutboxWrite,
        CrashPoint::AfterOutboxWrite,
        CrashPoint::BeforeCommit,
        CrashPoint::AfterCommit,
    ]
}

#[test]
fn loopback_transport_duplicate_reorder() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        for mode in [1, 2] {
            let files = Files::new();
            create(&files, authority);
            let production = open_production(&files, &lineage, mode);
            let store = production.store();
            let durable = ok(Durable::new(
                authority,
                production,
                store,
                principal(),
                &fixture::context(),
            ));
            let mut server = Server::new(config(contract), durable, Sink::default());
            assert_eq!(
                status(&exchange(&mut server, &body(contract, 120, 1, 0), false)),
                200
            );
            final_state(&files, authority, contract);
            let (_, sink) = server.into_parts();
            traces(&sink);
            sink.save(&format!("transport-{mode}"));
        }
    });
}

#[test]
fn loopback_real_busy() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        let files = Files::new();
        create(&files, authority);
        let mut production = open_production(&files, &lineage, 0);
        ok(production.set_test_busy_timeout(Duration::ZERO));
        let store = production.store();
        let durable = ok(Durable::new(
            authority,
            production,
            store,
            principal(),
            &fixture::context(),
        ));
        let mut server = Server::new(config(contract), durable, Sink::default());
        let connection = ok(rusqlite::Connection::open(&files.store));
        ok(connection.execute_batch("BEGIN IMMEDIATE"));
        assert_eq!(
            status(&exchange(&mut server, &body(contract, 120, 1, 0), false)),
            503
        );
        ok(connection.execute_batch("ROLLBACK"));
        compare(
            &files,
            &model::Model::new(fixture::model_genesis(authority)),
        );
        assert_eq!(
            status(&exchange(&mut server, &body(contract, 120, 1, 0), false)),
            200
        );
        final_state(&files, authority, contract);
        let (_, sink) = server.into_parts();
        assert!(sink.records.iter().any(|t| t.event == Event::Fault));
        sink.save("real-busy");
    });
}

struct Capture<B> {
    inner: B,
    packet: Vec<u8>,
}
impl<B: Endpoint> Endpoint for Capture<B> {
    fn execute(&mut self, command: &Command, sink: &mut dyn TraceSink) -> Result<Vec<u8>, Refusal> {
        let packet = self.inner.execute(command, sink)?;
        self.packet = packet.clone();
        Ok(packet)
    }
}
#[test]
fn lost_reply_reopen_original_packet() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        let files = Files::new();
        create(&files, authority);
        let production = open_production(&files, &lineage, 0);
        let store = production.store();
        let durable = ok(Durable::new(
            authority,
            production,
            store,
            principal(),
            &fixture::context(),
        ));
        let mut server = Server::new(
            config(contract),
            Capture {
                inner: durable,
                packet: Vec::new(),
            },
            Sink::default(),
        );
        assert!(exchange(&mut server, &body(contract, 120, 1, 0), true).is_empty());
        let (capture, sink) = server.into_parts();
        assert!(!capture.packet.is_empty());
        let original = capture.packet;
        drop(capture.inner);
        sink.save("lost-reply");
        let production = open_production(&files, &lineage, 0);
        let store = production.store();
        let durable = ok(Durable::new(
            authority,
            production,
            store,
            principal(),
            &fixture::context(),
        ));
        let mut server = Server::new(config(contract), durable, Sink::default());
        let recovered = exchange(&mut server, &body(contract, 120, 1, 0), false);
        assert_eq!(status(&recovered), 200);
        assert_eq!(packet(&recovered), original);
        final_state(&files, authority, contract);
        let (_, sink) = server.into_parts();
        sink.save("lost-reply-recovered");
    });
}

#[test]
fn ingress_child() {
    let Ok(role) = std::env::var("INGRESS_CHILD") else {
        return;
    };
    let role: usize = ok(role.parse());
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        let store_path = ok(std::env::var("INGRESS_STORE"));
        let receiver_path = ok(std::env::var("INGRESS_RECEIVER"));
        let mut production = ok(Production::open(
            &store_path,
            &lineage,
            1,
            Wire {
                receiver: Receiver::open(Path::new(&receiver_path)),
                mode: 0,
                queued: Vec::new(),
                abort: role == 8,
            },
            Some(Ticks::default()),
        ));
        if role < 8 {
            production.abort_next_commit_at(points()[role]);
        }
        let store = production.store();
        let durable = ok(Durable::new(
            authority,
            production,
            store,
            principal(),
            &fixture::context(),
        ));
        let mut sink = Sink::file(Path::new(&ok(std::env::var("INGRESS_TRACE"))));
        sink.abort_reply = role == 9;
        let mut server = Server::new(config(contract), durable, sink);
        let rt = ok(runtime());
        if std::env::var_os("INGRESS_TEST_MEMORY").is_some() {
            let input = ok(std::fs::read(ok(std::env::var("INGRESS_INPUT"))));
            let _ = support::exchange(&mut server, input, false);
            panic!("child did not abort");
        }
        let listener = ok(rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")));
        ok(std::fs::write(
            ok(std::env::var("INGRESS_READY")),
            ok(listener.local_addr()).to_string(),
        ));
        let _ = rt.block_on(server.serve_one(&listener));
        panic!("child did not abort");
    });
}

#[test]
fn real_kill_and_reopen() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        for role in 0..10 {
            let files = Files::new();
            create(&files, authority);
            let ready = files.store.with_extension("ready");
            let trace = files.store.with_extension("trace");
            let input = files.store.with_extension("request");
            ok(std::fs::write(&input, wire(&body(contract, 120, 1, 0))));
            let mut child = ok(std::process::Command::new(ok(std::env::current_exe()))
                .args(["--exact", "ingress_child", "--nocapture"])
                .env("INGRESS_CHILD", role.to_string())
                .env("INGRESS_STORE", &files.store)
                .env("INGRESS_RECEIVER", &files.receiver)
                .env("INGRESS_READY", &ready)
                .env("INGRESS_TRACE", &trace)
                .env("INGRESS_INPUT", &input)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn());
            if std::env::var_os("INGRESS_TEST_MEMORY").is_none() {
                let start = std::time::Instant::now();
                while !ready.is_file() {
                    if let Some(status) = ok(child.try_wait()) {
                        panic!("child failed before listening: {status}");
                    }
                    if start.elapsed() > Duration::from_secs(10) {
                        let _ = child.kill();
                        panic!("child startup timeout");
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                let address = ok(ok(std::fs::read_to_string(&ready)).parse());
                let response = client(address, wire(&body(contract, 120, 1, 0)), false);
                assert_ne!(status(&response), 200);
            }
            let exit = ok(child.wait());
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(
                exit.signal(),
                Some(6),
                "child did not terminate with SIGABRT: {exit}"
            );
            if role < 7 {
                compare(
                    &files,
                    &model::Model::new(fixture::model_genesis(authority)),
                );
            } else {
                compare(
                    &files,
                    &expected(authority, contract, if role == 9 { 2 } else { 0 }),
                );
            }
            let production = open_production(&files, &lineage, 0);
            let store = production.store();
            let durable = ok(Durable::new(
                authority,
                production,
                store,
                principal(),
                &fixture::context(),
            ));
            let mut server = Server::new(config(contract), durable, Sink::default());
            assert_eq!(
                status(&exchange(&mut server, &body(contract, 120, 1, 0), false)),
                200,
                "abort role {role}"
            );
            final_state(&files, authority, contract);
            let (_, sink) = server.into_parts();
            sink.save(&format!("kill-{role}-recovered"));
            let interrupted = ok(std::fs::read_to_string(&trace));
            assert!(interrupted.contains("Decision"));
            assert!(!interrupted.contains(&"ab".repeat(32)));
            if let Ok(dir) = std::env::var("INGRESS_TRACE_DIR") {
                ok(std::fs::copy(
                    &trace,
                    Path::new(&dir).join(format!("kill-{role}.trace")),
                ));
            }
            let _ = std::fs::remove_file(input);
            let _ = std::fs::remove_file(ready);
            let _ = std::fs::remove_file(trace);
        }
    });
}

#[test]
fn principal_context_and_core_refusals() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        for (bound, configured, allow) in [
            (principal(), key(55), true),
            (principal(), principal(), false),
            (principal(), principal(), true),
        ] {
            let files = Files::new();
            create(&files, authority);
            let production = open_production(&files, &lineage, 0);
            let store = production.store();
            let mut context = model::envelope(
                102,
                fixture::schema_hash(),
                zeno_fcis_value::Value::boolean(allow),
            );
            let durable = ok(Durable::new(authority, production, store, bound, &context));
            context.fill(0);
            let config = ok(Config::new(
                [0xab; 32],
                configured,
                contract,
                "https://app.example".into(),
                Duration::from_secs(2),
            ));
            let mut server = Server::new(config, durable, Sink::default());
            let response = exchange(&mut server, &body(contract, 120, 1, 0), false);
            assert_eq!(
                status(&response),
                if bound != configured {
                    403
                } else if !allow {
                    422
                } else {
                    200
                }
            );
            if bound != configured || !allow {
                compare(
                    &files,
                    &model::Model::new(fixture::model_genesis(authority)),
                );
            } else {
                final_state(&files, authority, contract);
            }
        }
    });
}

fn party_wire(credential: u8, body: &[u8]) -> Vec<u8> {
    let mut bytes=format!("POST /command HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nOrigin: https://app.example\r\nContent-Length: {}\r\n\r\n",format!("{credential:02x}").repeat(32),body.len()).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
fn party_exchange<B: Endpoint>(
    server: &mut Server<B, Sink>,
    credential: u8,
    body: &[u8],
    lost: bool,
) -> Vec<u8> {
    let rt = ok(runtime());
    if std::env::var_os("INGRESS_TEST_MEMORY").is_some() {
        let reply = support::exchange(server, party_wire(credential, body), false);
        return if lost { Vec::new() } else { reply };
    }
    let listener = ok(rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")));
    let address = ok(listener.local_addr());
    let request = party_wire(credential, body);
    let thread = std::thread::spawn(move || client(address, request, lost));
    let _ = rt.block_on(server.serve_one(&listener));
    ok(thread.join())
}
fn party(n: u8) -> Hash32 {
    Hash32::new([n; 32])
}
fn party_config(contract: Hash32, parties: &[(u8, u8)]) -> Config {
    ok(Config::with_parties(
        parties
            .iter()
            .map(|(credential, principal)| ([*credential; 32], party(*principal)))
            .collect(),
        contract,
        "https://app.example".into(),
        Duration::from_secs(2),
    ))
}
fn permission(allow: bool) -> Vec<u8> {
    model::envelope(
        102,
        fixture::schema_hash(),
        zeno_fcis_value::Value::boolean(allow),
    )
}

#[test]
fn parties_use_their_own_context() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        let files = Files::new();
        create(&files, authority);
        let production = open_production(&files, &lineage, 0);
        let store = production.store();
        // The permitted party is first, so taking the first entry's context would
        // grant the second party's command.
        let durable = ok(Durable::with_parties(
            authority,
            production,
            store,
            vec![
                (party(31), permission(true)),
                (party(32), permission(false)),
            ],
        ));
        let mut server = Server::new(
            party_config(contract, &[(0xa1, 31), (0xa2, 32)]),
            durable,
            Sink::default(),
        );
        let genesis = database::snapshot(&files.store);
        let refused = party_exchange(&mut server, 0xa2, &body(contract, 120, 1, 0), false);
        assert_eq!(status(&refused), 422, "context came from another party");
        assert_eq!(database::snapshot(&files.store), genesis);
        let unknown = party_exchange(&mut server, 0xa3, &body(contract, 120, 1, 0), false);
        assert_eq!(status(&unknown), 401, "unknown credential admitted");
        assert_eq!(database::snapshot(&files.store), genesis);
        let accepted = party_exchange(&mut server, 0xa1, &body(contract, 120, 1, 0), false);
        assert_eq!(status(&accepted), 200, "permitted party refused");
        let (durable, _) = server.into_parts();
        assert_eq!(
            durable.context(party(32)),
            Some(permission(false).as_slice())
        );
        assert_eq!(durable.context(party(33)), None);
    });
}

#[test]
fn parties_cannot_replay_each_other() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        let files = Files::new();
        create(&files, authority);
        let production = open_production(&files, &lineage, 0);
        let store = production.store();
        // Equal contexts: only the principal-scoped replay key separates them.
        let durable = ok(Durable::with_parties(
            authority,
            production,
            store,
            vec![(party(41), permission(true)), (party(42), permission(true))],
        ));
        let mut server = Server::new(
            party_config(contract, &[(0xb1, 41), (0xb2, 42)]),
            Capture {
                inner: durable,
                packet: Vec::new(),
            },
            Sink::default(),
        );
        // The first party's reply is lost; its request is unresolved.
        assert!(party_exchange(&mut server, 0xb1, &body(contract, 120, 1, 0), true).is_empty());
        let original = server.into_parts().0;
        let lost = original.packet.clone();
        assert!(!lost.is_empty());
        let mut server = Server::new(
            party_config(contract, &[(0xb1, 41), (0xb2, 42)]),
            original,
            Sink::default(),
        );
        let after = database::snapshot(&files.store);
        let other = party_exchange(&mut server, 0xb2, &body(contract, 120, 1, 0), false);
        assert_ne!(status(&other), 200, "another party replayed a request");
        assert_ne!(
            packet(&other),
            lost.as_slice(),
            "another party read a packet"
        );
        assert_eq!(database::snapshot(&files.store), after);
        let recovered = party_exchange(&mut server, 0xb1, &body(contract, 120, 1, 0), false);
        assert_eq!(status(&recovered), 200);
        assert_eq!(
            packet(&recovered),
            lost.as_slice(),
            "original packet changed"
        );
        // The same caller key is the second party's own new request.
        let own = party_exchange(&mut server, 0xb2, &body(contract, 120, 1, 1), false);
        assert_eq!(status(&own), 200, "caller keys are not principal-scoped");
    });
}

#[test]
fn party_tables_refuse_ambiguity() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let files = Files::new();
        create(&files, authority);
        let too_many: Vec<_> = (0..=64u8).map(|n| (party(n), permission(true))).collect();
        for parties in [
            Vec::new(),
            vec![(party(1), permission(true)), (party(1), permission(false))],
            too_many,
        ] {
            let production = open_production(&files, &lineage, 0);
            let store = production.store();
            assert!(matches!(
                Durable::with_parties(authority, production, store, parties),
                Err(Refusal::Malformed)
            ));
        }
    });
}

fn raw_exchange<B: Endpoint>(server: &mut Server<B, Sink>, request: Vec<u8>) -> Vec<u8> {
    let rt = ok(runtime());
    if std::env::var_os("INGRESS_TEST_MEMORY").is_some() {
        return support::exchange(server, request, false);
    }
    let listener = ok(rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")));
    let address = ok(listener.local_addr());
    let thread = std::thread::spawn(move || client(address, request, false));
    let _ = rt.block_on(server.serve_one(&listener));
    ok(thread.join())
}
fn party_http(method: &str, path: &str, credential: u8, origin: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nOrigin: {origin}\r\nContent-Length: {}\r\n\r\n",
        format!("{credential:02x}").repeat(32),
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
// An application's authenticated view and preview around the durable shell.
struct Viewing<B>(B);
impl<B: Endpoint> Endpoint for Viewing<B> {
    fn snapshot(&mut self, principal: Hash32) -> Result<Vec<u8>, Refusal> {
        Ok(principal.as_bytes().to_vec())
    }
    fn preview(&mut self, command: &Command) -> Result<Vec<u8>, Refusal> {
        Ok(command.id().as_bytes().to_vec())
    }
    fn execute(&mut self, command: &Command, sink: &mut dyn TraceSink) -> Result<Vec<u8>, Refusal> {
        self.0.execute(command, sink)
    }
}

#[test]
fn party_traces_hold_no_credential() {
    fixture::with_catalog(|catalog| {
        let lineage = ok(Lineage::bind(&[catalog], &[]));
        let authority = &lineage.authorities()[0];
        let contract = payload_hash(authority.identity());
        let files = Files::new();
        create(&files, authority);
        let production = open_production(&files, &lineage, 0);
        let store = production.store();
        let parties = [(0xc1, 51, true), (0xc2, 52, false), (0xc3, 53, true)];
        let durable = ok(Durable::with_parties(
            authority,
            production,
            store,
            parties
                .iter()
                .map(|(_, principal, allow)| (party(*principal), permission(*allow)))
                .collect(),
        ));
        let credentials: Vec<(u8, u8)> = parties.iter().map(|(c, p, _)| (*c, *p)).collect();
        let mut server = Server::new(
            party_config(contract, &credentials),
            Viewing(durable),
            Sink::default(),
        );
        let origin = "https://app.example";
        let mut revision = 0;
        // Each party's view, preview, command and refusals.
        for (index, (credential, _, allow)) in parties.iter().enumerate() {
            let retry = 20 + index as u64;
            let request = body(contract, 120, retry, revision);
            let call = |server: &mut Server<_, Sink>, method, path, origin, body: &[u8]| {
                status(&raw_exchange(
                    server,
                    party_http(method, path, *credential, origin, body),
                ))
            };
            assert_eq!(call(&mut server, "GET", "/state", origin, &[]), 200);
            assert_eq!(call(&mut server, "POST", "/preview", origin, &request), 200);
            let decided = call(&mut server, "POST", "/command", origin, &request);
            assert_eq!(decided, if *allow { 200 } else { 422 });
            if *allow {
                revision += 1;
            }
            let stale = body(contract, 121, retry, revision + 5);
            assert_eq!(
                call(
                    &mut server,
                    "POST",
                    "/command",
                    "https://evil.example",
                    &request
                ),
                403
            );
            assert_eq!(call(&mut server, "POST", "/command", origin, &stale), 409);
            assert_eq!(
                call(&mut server, "POST", "/preview", origin, &[1, 2, 3]),
                400
            );
        }
        let unknown = raw_exchange(&mut server, party_http("GET", "/state", 0xc4, origin, &[]));
        assert_eq!(status(&unknown), 401);
        let (_, sink) = server.into_parts();
        assert!(sink.records.iter().any(|t| t.event == Event::Acknowledge));
        let text = sink
            .records
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        // No configured credential appears in any trace, as hexadecimal or bytes.
        for (credential, _) in credentials.iter().chain([&(0xc4, 0)]) {
            let hex = format!("{credential:02x}").repeat(32);
            assert!(
                !text.contains(&hex) && !text.contains(&hex.to_uppercase()),
                "party credential in trace"
            );
            for trace in &sink.records {
                for id in [
                    trace.request,
                    trace.replay,
                    trace.decision,
                    trace.commit,
                    trace.delivery,
                ]
                .into_iter()
                .flatten()
                {
                    assert_ne!(
                        id.as_bytes(),
                        &[*credential; 32],
                        "party credential in trace"
                    );
                }
            }
        }
        sink.save("party-traces");
    });
}
