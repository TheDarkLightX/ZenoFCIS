//! The emitted application's HTTP, SQLite, restart and delivery journey.
#![cfg(feature = "app")]
use application::{
    app::{Application, Clock, FileReceiver},
    v2_contract,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io,
    path::Path,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};
use zeno_fcis_ingress::{Config, Event, Server, Trace, TraceSink, encode_command};
use zeno_fcis_shell_sqlite::{effects::production::Production, v2::Lineage};
#[derive(Default)]
struct Sink(Vec<Trace>);
impl TraceSink for Sink {
    fn record(&mut self, trace: Trace) {
        self.0.push(trace);
    }
}
struct Memory {
    input: Vec<u8>,
    at: usize,
    output: Rc<RefCell<Vec<u8>>>,
}
impl AsyncRead for Memory {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let n = out.remaining().min(self.input.len() - self.at);
        out.put_slice(&self.input[self.at..self.at + n]);
        self.at += n;
        Poll::Ready(Ok(()))
    }
}
impl AsyncWrite for Memory {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.output.borrow_mut().extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
fn ok<T, E: std::fmt::Debug>(v: Result<T, E>) -> T {
    v.unwrap_or_else(|e| panic!("{e:?}"))
}
fn wire(path: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes=format!("{} {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n\r\n",if path=="/state"{"GET"}else{"POST"},"ab".repeat(32),body.len()).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
fn exchange<B: zeno_fcis_ingress::Endpoint>(
    server: &mut Server<B, Sink>,
    path: &str,
    body: &[u8],
) -> (u16, Vec<u8>) {
    let output = Rc::new(RefCell::new(Vec::new()));
    let runtime = ok(zeno_fcis_ingress::runtime());
    if std::env::var_os("APP_JOURNEY_TCP").is_some() {
        use std::io::{Read, Write};
        let listener = ok(runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")));
        let address = ok(listener.local_addr());
        let bytes = wire(path, body);
        let client = std::thread::spawn(move || {
            let mut stream = ok(std::net::TcpStream::connect(address));
            ok(stream.set_read_timeout(Some(Duration::from_secs(15))));
            ok(stream.write_all(&bytes));
            ok(stream.shutdown(std::net::Shutdown::Write));
            let mut reply = Vec::new();
            ok(stream.read_to_end(&mut reply));
            reply
        });
        ok(runtime.block_on(server.serve_one(&listener)));
        *output.borrow_mut() = ok(client.join());
    } else {
        let stream = Memory {
            input: wire(path, body),
            at: 0,
            output: Rc::clone(&output),
        };
        ok(runtime.block_on(server.serve_connection(stream)));
    }
    let response = output.borrow();
    let status = std::str::from_utf8(&response)
        .ok()
        .and_then(|v| v.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let begin = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    (status, response[begin..].to_vec())
}
fn scalar(v: &Value) -> i128 {
    v.as_str().unwrap().parse().unwrap()
}
#[test]
fn app_factory_journey() {
    let root = std::env::temp_dir().join(format!("app-journey-{}", std::process::id()));
    ok(std::fs::create_dir(&root));
    let path = root.join("app.sqlite");
    let receiver = root.join("received");
    ok(application::init(&path));
    let fixture: Value = ok(serde_json::from_slice(include_bytes!(
        "../app-checks/journey.json"
    )));
    let bundle: Value = ok(serde_json::from_slice(include_bytes!("../www/app.json")));
    let mut observations = Vec::new();
    let mut all_traces = Vec::new();
    let mut delivered = 0;
    for (index, step) in fixture["steps"].as_array().unwrap().iter().enumerate() {
        let contract = v2_contract::Contract::new();
        let descriptor = contract.descriptor();
        let authority = ok(application::authority(&descriptor));
        let before = ok(application::current(&path));
        let mut input = before.state.clone();
        input.extend(step["command"].as_array().unwrap().iter().map(scalar));
        input.extend(step["context"].as_array().unwrap().iter().map(scalar));
        let frames = ok(application::wire(&descriptor, &input));
        let input_strings: Vec<String> = input.iter().map(ToString::to_string).collect();
        let hash = RustCryptoSha256::hash(authority.identity());
        let principal = Hash32::new([7; 32]);
        let key = Hash32::new([u8::try_from(index + 1).unwrap(); 32]);
        let original = encode_command(hash, key, before.head.commits, &frames[1]);
        let preview_body = encode_command(
            hash,
            key,
            before.head.commits,
            &ok(serde_json::to_vec(&input_strings)),
        );
        let mut first_packet = Vec::new();
        let mut preview = Value::Null;
        // Two independent opens: lose the first reply, then replay the exact request.
        for attempt in 0..2 {
            let catalog = ok(v2_contract::checked_catalog(&descriptor));
            let catalogs = [&catalog];
            let lineage = ok(Lineage::bind(&catalogs, &[]));
            let production = ok(Production::open(
                &path,
                &lineage,
                1,
                ok(FileReceiver::new(&receiver)),
                Some(Clock::default()),
            ));
            let store = production.store();
            let endpoint = ok(Application::new(
                &lineage, production, store, &path, principal, &frames[2],
            ));
            let config = ok(Config::new(
                [0xab; 32],
                principal,
                hash,
                "http://localhost".into(),
                Duration::from_secs(10),
            ));
            let mut server = Server::new(config, endpoint, Sink::default());
            if attempt == 0 {
                let (status, bytes) = exchange(&mut server, "/preview", &preview_body);
                assert_eq!(status, 200);
                preview = ok(serde_json::from_slice::<Value>(&bytes));
                assert_eq!(preview["reply"]["class"], step["class"]);
                assert_eq!(preview["command"], application::app::hex(&frames[1]));
            }
            let (status, bytes) = exchange(&mut server, "/command", &original);
            if step["class"] == "Reject" {
                assert_eq!(status, 422);
                assert_eq!(ok(application::current(&path)), before);
            } else {
                assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
                let result: Value = ok(serde_json::from_slice(&bytes));
                assert_eq!(result["reply"], preview["reply"]);
                if attempt == 0 {
                    first_packet = bytes.clone();
                } else {
                    assert_eq!(bytes, first_packet, "restart changed original packet");
                }
                let (status, snapshot) = exchange(&mut server, "/state", &[]);
                assert_eq!(status, 200);
                let snapshot: Value = ok(serde_json::from_slice(&snapshot));
                assert_eq!(snapshot["values"], result["reply"]["post"]);
                assert_eq!(snapshot["pending"], "0");
                if attempt == 0 {
                    observations.push(json!({"before":input_strings,"preview":preview["reply"],"result":result["reply"],"response":result,"command":preview["command"],"key":application::app::hex(key.as_bytes()),"revision":before.head.commits.to_string(),"principal":application::app::hex(principal.as_bytes()),"contract":application::app::hex(hash.as_bytes())}));
                }
            }
            let (_, sink) = server.into_parts();
            for t in &sink.0 {
                if t.event == Event::Acknowledge {
                    delivered += 1;
                    assert!(t.delivery.is_some() && t.commit.is_some() && t.decision.is_some());
                }
                if matches!(
                    t.event,
                    Event::Request
                        | Event::Decision
                        | Event::Commit
                        | Event::Send
                        | Event::Acknowledge
                        | Event::Reply
                ) {
                    assert!(t.request.is_some() && t.replay.is_some());
                }
            }
            all_traces.extend(sink.0);
        }
    }
    if let Some(expected) = fixture["deliveries"].as_u64() {
        assert_eq!(delivered, expected);
    }
    let files = ok(std::fs::read_dir(&receiver))
        .filter_map(Result::ok)
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .count();
    assert_eq!(files, delivered as usize);
    assert!(
        !all_traces
            .iter()
            .map(ToString::to_string)
            .collect::<String>()
            .contains(&"ab".repeat(32))
    );
    if let Some(out) = std::env::var_os("APP_JOURNEY_OUT") {
        ok(std::fs::write(
            Path::new(&out),
            ok(serde_json::to_vec_pretty(
                &json!({"bundle":bundle,"observations":observations,"delivered":delivered,"trace":all_traces.iter().map(ToString::to_string).collect::<Vec<_>>()}),
            )),
        ));
    }
    ok(std::fs::remove_dir_all(root));
}

#[test]
fn file_receiver_deduplicates_across_reopen_and_keeps_collisions_unknown() {
    use zeno_fcis_shell::effects::port::{Delivery, Fault, Sent};
    use zeno_fcis_shell_sqlite::effects::production::Transport;
    let root = std::env::temp_dir().join(format!("app-receiver-{}", std::process::id()));
    ok(std::fs::create_dir(&root));
    let payload = b"retained delivery".to_vec();
    let mut delivery = Delivery {
        id: Hash32::new([3; 32]),
        version: 1,
        lane: 1,
        ordinal: 0,
        destination: b"local-file-receiver".to_vec(),
        payload_sha256: RustCryptoSha256::hash(&payload),
        payload,
    };
    let mut receiver = ok(FileReceiver::new(&root));
    assert!(matches!(receiver.send(&delivery), Sent::Sent(_)));
    drop(receiver);
    let mut reopened = ok(FileReceiver::new(&root));
    assert!(matches!(reopened.send(&delivery), Sent::Sent(_)));
    assert_eq!(ok(std::fs::read_dir(&root)).count(), 1);
    delivery.payload = b"changed bytes under the same ID".to_vec();
    delivery.payload_sha256 = RustCryptoSha256::hash(&delivery.payload);
    assert_eq!(reopened.send(&delivery), Sent::Unknown(Fault::Io));
    assert_eq!(
        ok(std::fs::read(
            root.join(application::app::hex(delivery.id.as_bytes()))
        )),
        b"retained delivery"
    );
    ok(std::fs::remove_dir_all(root));
}
