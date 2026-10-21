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
fn wire_as(path: &str, credential: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes=format!("{} {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {credential}\r\nContent-Length: {}\r\n\r\n",if path=="/state"{"GET"}else{"POST"},body.len()).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
fn exchange<B: zeno_fcis_ingress::Endpoint>(
    server: &mut Server<B, Sink>,
    path: &str,
    body: &[u8],
) -> (u16, Vec<u8>) {
    exchange_as(server, path, &"ab".repeat(32), body)
}
fn exchange_as<B: zeno_fcis_ingress::Endpoint>(
    server: &mut Server<B, Sink>,
    path: &str,
    credential: &str,
    body: &[u8],
) -> (u16, Vec<u8>) {
    let output = Rc::new(RefCell::new(Vec::new()));
    let runtime = ok(zeno_fcis_ingress::runtime());
    if std::env::var_os("APP_JOURNEY_TCP").is_some() {
        use std::io::{Read, Write};
        let listener = ok(runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")));
        let address = ok(listener.local_addr());
        let bytes = wire_as(path, credential, body);
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
            input: wire_as(path, credential, body),
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
// A step's context: its own, or that of the party it names.
fn step_context(fixture: &Value, step: &Value) -> Vec<Value> {
    if let Some(context) = step["context"].as_array() {
        return context.clone();
    }
    let parties = fixture["parties"].as_array().unwrap();
    let party = parties.iter().find(|p| p["name"] == step["party"]).unwrap();
    party["context"].as_array().unwrap().clone()
}
// Delivered payload files, each with its receiver envelope beside it.
fn received(receiver: &Path) -> usize {
    let names: Vec<String> = ok(std::fs::read_dir(receiver))
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .collect();
    let payloads: Vec<&String> = names.iter().filter(|n| !n.ends_with(".json")).collect();
    assert_eq!(payloads.len() * 2, names.len(), "payload without envelope");
    for name in payloads {
        let payload = ok(std::fs::read(receiver.join(name)));
        let envelope: Value = ok(serde_json::from_slice(&ok(std::fs::read(
            receiver.join(format!("{name}.json")),
        ))));
        assert_eq!(envelope["delivery"], name.as_str());
        assert_eq!(
            envelope["payload_sha256"],
            application::app::hex(RustCryptoSha256::hash(&payload).as_bytes())
        );
    }
    names.len() / 2
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
        input.extend(step_context(&fixture, step).iter().map(scalar));
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
    assert_eq!(received(&receiver), delivered as usize);
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
        // Canonical text: its tag, a 32-bit big-endian length and the ASCII bytes.
        destination: [&[6, 0, 0, 0, 19][..], b"local-file-receiver"].concat(),
        payload_sha256: RustCryptoSha256::hash(&payload),
        payload,
    };
    let mut receiver = ok(FileReceiver::new(&root));
    assert!(matches!(receiver.send(&delivery), Sent::Sent(_)));
    drop(receiver);
    let mut reopened = ok(FileReceiver::new(&root));
    assert!(matches!(reopened.send(&delivery), Sent::Sent(_)));
    assert_eq!(ok(std::fs::read_dir(&root)).count(), 2);
    let envelope: Value = ok(serde_json::from_slice(&ok(std::fs::read(root.join(
        format!("{}.json", application::app::hex(delivery.id.as_bytes())),
    )))));
    assert_eq!(envelope["destination_text"], "local-file-receiver");
    assert_eq!(envelope["lane"], 1);
    // A payload kept without an envelope gains one on the next send.
    let described = root.join(format!(
        "{}.json",
        application::app::hex(delivery.id.as_bytes())
    ));
    ok(std::fs::remove_file(&described));
    assert!(matches!(reopened.send(&delivery), Sent::Sent(_)));
    assert!(described.is_file());
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

// Write a parties file whose entry i uses credential [i + 0x31; 32] and principal
// [i + 0x51; 32]; each entry is (name, credential index, principal index, context).
fn parties_file(root: &Path, entries: &[(&str, usize, usize, Value)]) -> std::path::PathBuf {
    let mut parties = Vec::new();
    for (index, (name, credential, principal, context)) in entries.iter().enumerate() {
        let secret = application::app::hex(&[u8::try_from(0x31 + credential).unwrap(); 32]);
        ok(std::fs::write(
            root.join(format!("credential-{credential}")),
            secret + "\n",
        ));
        ok(std::fs::write(
            root.join(format!("context-{index}.json")),
            context.to_string(),
        ));
        parties.push(json!({"name":name,"credential":format!("credential-{credential}"),"principal":application::app::hex(&[u8::try_from(0x51 + principal).unwrap(); 32]),"context":format!("context-{index}.json")}));
    }
    let path = root.join("parties.json");
    ok(std::fs::write(
        &path,
        json!({"schema":"zeno-fcis/app-parties/1","parties":parties}).to_string(),
    ));
    path
}
fn refusal(result: Result<Vec<application::app::Party>, String>) -> String {
    match result {
        Ok(_) => panic!("start-up configuration accepted"),
        Err(error) => error,
    }
}

#[test]
fn parties_file_start_up_refusals() {
    let root = std::env::temp_dir().join(format!("app-parties-file-{}", std::process::id()));
    ok(std::fs::create_dir(&root));
    let load = |entries: &[(&str, usize, usize, Value)]| {
        application::app::load_parties(&parties_file(&root, entries))
    };
    let three = ok(load(&[
        ("buyer", 0, 0, json!(["1"])),
        ("seller", 1, 1, json!(["2"])),
        ("operator", 2, 2, json!(["3"])),
    ]));
    assert_eq!(three.len(), 3);
    assert_eq!(three[1].name, "seller");
    assert_eq!(three[1].principal, Hash32::new([0x52; 32]));
    assert_eq!(three[2].context, [3_i128]);
    for (entries, expected) in [
        (
            vec![("a", 0, 0, json!([])), ("a", 1, 1, json!([]))],
            "repeated",
        ),
        (
            vec![("a", 0, 0, json!([])), ("b", 0, 1, json!([]))],
            "share a credential",
        ),
        (
            vec![("a", 0, 0, json!([])), ("b", 1, 0, json!([]))],
            "share a principal",
        ),
        (vec![("a b", 0, 0, json!([]))], "party names"),
        (Vec::new(), "1 to 64"),
    ] {
        assert!(refusal(load(&entries)).contains(expected), "{expected}");
    }
    let many = vec![json!({}); application::app::MAX_PARTIES + 1];
    let path = root.join("many.json");
    ok(std::fs::write(
        &path,
        json!({"schema":"zeno-fcis/app-parties/1","parties":many}).to_string(),
    ));
    assert!(refusal(application::app::load_parties(&path)).contains("1 to 64"));
    // Unreadable, oversized and zero credentials, and unknown keys.
    let path = parties_file(&root, &[("a", 0, 0, json!([]))]);
    ok(std::fs::remove_file(root.join("credential-0")));
    assert!(refusal(application::app::load_parties(&path)).contains("could not be read"));
    ok(std::fs::write(
        root.join("credential-0"),
        "0".repeat(65_537),
    ));
    assert!(refusal(application::app::load_parties(&path)).contains("exceeds limit"));
    ok(std::fs::write(root.join("credential-0"), "00".repeat(32)));
    assert!(refusal(application::app::load_parties(&path)).contains("zero"));
    ok(std::fs::write(&path, " ".repeat(65_537)));
    assert!(refusal(application::app::load_parties(&path)).contains("exceeds limit"));
    ok(std::fs::write(&path, json!({"schema":"zeno-fcis/app-parties/1","parties":[{"name":"a","credential":"c","principal":"p","context":"x","role":"r"}]}).to_string()));
    assert!(refusal(application::app::load_parties(&path)).contains("exactly name"));
    for address in ["0.0.0.0:8080", "192.0.2.10:80", "[::]:8080"] {
        assert!(
            application::app::listen_address(address).is_err(),
            "{address}"
        );
    }
    for address in ["127.0.0.1:8080", "[::1]:8080"] {
        assert!(
            application::app::listen_address(address).is_ok(),
            "{address}"
        );
    }
    // The server refuses these before it opens a store or a socket.
    let parties = parties_file(&root, &[("a", 0, 0, json!([]))]);
    let credential = root.join("credential-0");
    for (variables, expected) in [
        (
            vec![
                ("ZENO_APP_PARTIES", &parties),
                ("ZENO_APP_SECRET_FILE", &credential),
            ],
            "set one form",
        ),
        (
            vec![("ZENO_APP_SECRET_FILE", &credential)],
            "loopback required",
        ),
    ] {
        let output = ok(std::process::Command::new(env!("CARGO_BIN_EXE_app-server"))
            .env_remove("ZENO_APP_PARTIES")
            .env_remove("ZENO_APP_SECRET_FILE")
            .env_remove("ZENO_APP_PRINCIPAL")
            .env_remove("ZENO_APP_CONTEXT")
            .envs(variables)
            .env(
                "ZENO_APP_ADDRESS",
                if expected == "loopback required" {
                    "0.0.0.0:0"
                } else {
                    "127.0.0.1:0"
                },
            )
            .env("ZENO_APP_STORE", root.join("never.sqlite"))
            .output());
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{expected}"
        );
        assert!(!root.join("never.sqlite").exists());
    }
    ok(std::fs::remove_dir_all(root));
}

#[test]
fn parties_journey() {
    let fixture: Value = ok(serde_json::from_slice(include_bytes!(
        "../app-checks/journey.json"
    )));
    let Some(parties) = fixture["parties"].as_array() else {
        return;
    };
    let root = std::env::temp_dir().join(format!("app-parties-{}", std::process::id()));
    ok(std::fs::create_dir(&root));
    let path = root.join("app.sqlite");
    let receiver = root.join("received");
    ok(application::init(&path));
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = ok(application::authority(&descriptor));
    let hash = RustCryptoSha256::hash(authority.identity());
    let entries: Vec<_> = parties
        .iter()
        .enumerate()
        .map(|(i, p)| (p["name"].as_str().unwrap(), i, i, p["context"].clone()))
        .collect();
    let loaded = ok(application::app::load_parties(&parties_file(
        &root, &entries,
    )));
    let credential = |name: &str| {
        let index = parties.iter().position(|p| p["name"] == name).unwrap();
        application::app::hex(&[u8::try_from(0x31 + index).unwrap(); 32])
    };
    let steps = fixture["steps"].as_array().unwrap();
    let first: Vec<i128> = steps[0]["command"]
        .as_array()
        .unwrap()
        .iter()
        .map(scalar)
        .collect();
    // Each party's canonical context, framed as the server frames it.
    let table: Vec<_> = loaded
        .iter()
        .map(|party| {
            let mut input = ok(application::current(&path)).state;
            input.extend(&first);
            input.extend(&party.context);
            (
                party.principal,
                ok(application::wire(&descriptor, &input))[2].clone(),
            )
        })
        .collect();
    let catalog = ok(v2_contract::checked_catalog(&descriptor));
    let catalogs = [&catalog];
    let lineage = ok(Lineage::bind(&catalogs, &[]));
    let serve = |table: Vec<(Hash32, Vec<u8>)>| {
        let production = ok(Production::open(
            &path,
            &lineage,
            1,
            ok(FileReceiver::new(&receiver)),
            Some(Clock::default()),
        ));
        let store = production.store();
        let endpoint = ok(Application::with_parties(
            &lineage, production, store, &path, table,
        ));
        let config = ok(application::app::ingress_config(
            &loaded,
            hash,
            "http://localhost".into(),
        ));
        Server::new(config, endpoint, Sink::default())
    };
    // One server process serves every party's step in turn.
    let mut server = serve(table.clone());
    let mut accepted = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let name = step["party"].as_str().unwrap();
        let party = loaded.iter().find(|p| p.name == name).unwrap();
        let before = ok(application::current(&path));
        let mut input = before.state.clone();
        input.extend(step["command"].as_array().unwrap().iter().map(scalar));
        let command = input.clone();
        input.extend(&party.context);
        let frames = ok(application::wire(&descriptor, &input));
        let key = Hash32::new([u8::try_from(index + 1).unwrap(); 32]);
        let strings: Vec<String> = input.iter().map(ToString::to_string).collect();
        let preview_body = encode_command(
            hash,
            key,
            before.head.commits,
            &ok(serde_json::to_vec(&strings)),
        );
        let (status, bytes) =
            exchange_as(&mut server, "/preview", &credential(name), &preview_body);
        assert_eq!(status, 200);
        let preview: Value = ok(serde_json::from_slice(&bytes));
        assert_eq!(preview["reply"]["class"], step["class"], "step {index}");
        // A request cannot choose another party's context.
        for other in loaded.iter().filter(|p| p.context != party.context) {
            let mut forged: Vec<String> = command.iter().map(ToString::to_string).collect();
            forged.extend(other.context.iter().map(ToString::to_string));
            let body = encode_command(
                hash,
                key,
                before.head.commits,
                &ok(serde_json::to_vec(&forged)),
            );
            let (status, _) = exchange_as(&mut server, "/preview", &credential(name), &body);
            assert_eq!(status, 403, "a request chose another party's context");
        }
        let original = encode_command(hash, key, before.head.commits, &frames[1]);
        let (status, bytes) = exchange_as(&mut server, "/command", &credential(name), &original);
        if step["class"] == "Reject" {
            assert_eq!(status, 422, "a party used another party's permission");
            assert_eq!(ok(application::current(&path)), before);
        } else {
            assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
            accepted.push((name, original, bytes));
        }
    }
    let (status, _) = exchange_as(&mut server, "/state", &"ee".repeat(32), &[]);
    assert_eq!(status, 401, "unknown credential admitted");
    for party in &loaded {
        let (status, bytes) = exchange_as(&mut server, "/state", &credential(&party.name), &[]);
        assert_eq!(status, 200);
        let snapshot: Value = ok(serde_json::from_slice(&bytes));
        let context: Vec<String> = party.context.iter().map(ToString::to_string).collect();
        assert_eq!(
            snapshot["context"],
            json!(context),
            "snapshot of another party"
        );
        assert_eq!(
            snapshot["principal"],
            application::app::hex(party.principal.as_bytes())
        );
    }
    drop(server);
    // After a restart the party recovers its original packet; another party cannot.
    let mut server = serve(table);
    let (name, original, packet) = accepted.last().unwrap();
    let (status, bytes) = exchange_as(&mut server, "/command", &credential(name), original);
    assert_eq!(status, 200);
    assert_eq!(&bytes, packet, "restart changed original packet");
    for other in loaded.iter().filter(|p| p.name != *name) {
        let (status, _) = exchange_as(&mut server, "/command", &credential(&other.name), original);
        assert_ne!(status, 200, "another party replayed a request");
    }
    if let Some(expected) = fixture["deliveries"].as_u64() {
        assert_eq!(received(&receiver) as u64, expected);
    }
    drop(server);
    ok(std::fs::remove_dir_all(root));
}
