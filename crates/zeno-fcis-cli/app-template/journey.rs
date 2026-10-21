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
    exchange_bytes(server, wire_as(path, credential, body))
}
// A browser request: the same, with an Origin header.
fn wire_origin(path: &str, origin: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:8080\r\nAuthorization: Bearer {}\r\nOrigin: {origin}\r\nContent-Length: {}\r\n\r\n",
        "ab".repeat(32),
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
fn exchange_bytes<B: zeno_fcis_ingress::Endpoint>(
    server: &mut Server<B, Sink>,
    bytes: Vec<u8>,
) -> (u16, Vec<u8>) {
    let output = Rc::new(RefCell::new(Vec::new()));
    let runtime = ok(zeno_fcis_ingress::runtime());
    if std::env::var_os("APP_JOURNEY_TCP").is_some() {
        use std::io::{Read, Write};
        let listener = ok(runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")));
        let address = ok(listener.local_addr());
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
            input: bytes,
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
    let fixture: Value = ok(serde_json::from_slice(include_bytes!(
        "../app-checks/journey.json"
    )));
    // A journey of several parties runs on one server in `parties_journey`.
    if fixture["parties"].is_array() {
        return;
    }
    let root = std::env::temp_dir().join(format!("app-journey-{}", std::process::id()));
    ok(std::fs::create_dir(&root));
    let path = root.join("app.sqlite");
    let receiver = root.join("received");
    ok(application::init(&path));
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
    // The server journals every admitted request, so `history` reads the store.
    let accepted = fixture["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["class"] != "Reject")
        .count();
    let history = application::history(&path).map(|history| history.entries.len());
    assert_eq!(
        history,
        Ok(accepted),
        "an admitted request is missing from the journal"
    );
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
    // A view lists state field names; an unknown or repeated name is refused.
    let path = parties_file(&root, &[("a", 0, 0, json!([]))]);
    let mut file: Value = ok(serde_json::from_slice(&ok(std::fs::read(&path))));
    file["parties"][0]["view"] = json!("all");
    ok(std::fs::write(&path, file.to_string()));
    assert!(refusal(application::app::load_parties(&path)).contains("view"));
    let bundle: Value = ok(serde_json::from_slice(include_bytes!("../www/app.json")));
    let first = bundle["description"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["source"] == "state")
        .unwrap()["name"]
        .clone();
    for (view, expected) in [
        (json!(["no_such_field"]), "not a state field"),
        (json!([first, first]), "repeated"),
    ] {
        file["parties"][0]["view"] = view;
        ok(std::fs::write(&path, file.to_string()));
        let parties = ok(application::app::load_parties(&path));
        match application::app::party_views(&parties) {
            Ok(_) => panic!("unknown view field accepted"),
            Err(error) => assert!(error.contains(expected), "{error}"),
        }
    }
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
    // A TLS proxy's public origin, exactly as browsers send it.
    for origin in ["https://app.example.com", "https://app.example.com:8443"] {
        assert_eq!(
            application::app::public_origin(origin).as_deref(),
            Ok(origin)
        );
    }
    for origin in [
        "http://app.example.com",
        "https://App.example.com",
        "https://app.example.com/",
        "https://app.example.com:443",
        "https://user@app.example.com",
        "https://app.example.com:08443",
        "https://",
        "app.example.com",
        "https://app.example.com https://b.example.com",
        "https://app.example.com,https://b.example.com",
        "https://app.example.com?x=1",
        "https://app.example.com#top",
        " https://app.example.com",
        "https://app.example.com\n",
        "",
    ] {
        assert!(application::app::public_origin(origin).is_err(), "{origin}");
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
        (
            vec![
                ("ZENO_APP_SECRET_FILE", &credential),
                ("ZENO_APP_ORIGIN", &credential),
            ],
            "ZENO_APP_ORIGIN",
        ),
    ] {
        let output = ok(std::process::Command::new(env!("CARGO_BIN_EXE_app-server"))
            .env_remove("ZENO_APP_PARTIES")
            .env_remove("ZENO_APP_SECRET_FILE")
            .env_remove("ZENO_APP_PRINCIPAL")
            .env_remove("ZENO_APP_CONTEXT")
            .env_remove("ZENO_APP_ORIGIN")
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
    // A value that is set but empty or not Unicode refuses; it never means unset.
    #[cfg(unix)]
    for value in [
        std::ffi::OsString::new(),
        std::os::unix::ffi::OsStringExt::from_vec(b"https://app.example.com\xff".to_vec()),
    ] {
        let output = ok(std::process::Command::new(env!("CARGO_BIN_EXE_app-server"))
            .env_remove("ZENO_APP_PARTIES")
            .env_remove("ZENO_APP_PRINCIPAL")
            .env_remove("ZENO_APP_CONTEXT")
            .env("ZENO_APP_SECRET_FILE", &credential)
            .env("ZENO_APP_ORIGIN", value)
            .env("ZENO_APP_ADDRESS", "127.0.0.1:0")
            .env("ZENO_APP_STORE", root.join("never.sqlite"))
            .output());
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("ZENO_APP_ORIGIN"));
        assert!(!root.join("never.sqlite").exists());
    }
    // A store or address that is set but empty or not text refuses with its name;
    // it never falls back to the default store or address.
    #[cfg(unix)]
    for (name, value) in [
        ("ZENO_APP_STORE", std::ffi::OsString::new()),
        (
            "ZENO_APP_STORE",
            std::os::unix::ffi::OsStringExt::from_vec(b"store\xff.sqlite".to_vec()),
        ),
        ("ZENO_APP_ADDRESS", std::ffi::OsString::new()),
        (
            "ZENO_APP_ADDRESS",
            std::os::unix::ffi::OsStringExt::from_vec(b"127.0.0.1:\xff".to_vec()),
        ),
    ] {
        let output = ok(std::process::Command::new(env!("CARGO_BIN_EXE_app-server"))
            .current_dir(&root)
            .env_remove("ZENO_APP_PARTIES")
            .env_remove("ZENO_APP_PRINCIPAL")
            .env_remove("ZENO_APP_ORIGIN")
            .env_remove("ZENO_APP_STORE")
            .env_remove("ZENO_APP_ADDRESS")
            .env("ZENO_APP_SECRET_FILE", &credential)
            .env(
                "ZENO_APP_CONTEXT",
                Path::new(env!("CARGO_MANIFEST_DIR")).join("context.json"),
            )
            .env(name, value)
            .output());
        assert_eq!(output.status.code(), Some(1), "{name}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(name),
            "{name}"
        );
        assert!(!root.join("app.sqlite").exists(), "default store opened");
    }
    ok(std::fs::remove_dir_all(root));
}

// A store and its parties for one test: the fixture's parties, their files and,
// for a contract with a server clock, a clock file the test sets.
struct Journey {
    root: std::path::PathBuf,
    path: std::path::PathBuf,
    receiver: std::path::PathBuf,
    parties: Vec<application::app::Party>,
    fixture: Value,
}
impl Journey {
    fn new(name: &str) -> Option<Self> {
        let fixture: Value = ok(serde_json::from_slice(include_bytes!(
            "../app-checks/journey.json"
        )));
        let entries: Vec<_> = fixture["parties"]
            .as_array()?
            .iter()
            .enumerate()
            .map(|(i, p)| (p["name"].as_str().unwrap(), i, i, p["context"].clone()))
            .collect();
        let root = std::env::temp_dir().join(format!("app-{name}-{}", std::process::id()));
        ok(std::fs::create_dir(&root));
        let path = root.join("app.sqlite");
        ok(application::init(&path));
        let file = parties_file(&root, &entries);
        // A fixture party may list the state fields it sees.
        let mut written: Value = ok(serde_json::from_slice(&ok(std::fs::read(&file))));
        for (entry, party) in written["parties"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(fixture["parties"].as_array().unwrap())
        {
            if !party["view"].is_null() {
                entry["view"] = party["view"].clone();
            }
        }
        ok(std::fs::write(&file, written.to_string()));
        let parties = ok(application::app::load_parties(&file));
        Some(Self {
            receiver: root.join("received"),
            path,
            root,
            parties,
            fixture,
        })
    }
    fn clock(&self) -> Option<application::app::ServerClock> {
        ok(application::app::ServerClock::declared(Some(
            self.root.join("clock"),
        )))
    }
    fn set_clock(&self, time: &str) {
        ok(std::fs::write(self.root.join("clock"), time));
    }
    fn credential(&self, name: &str) -> String {
        let index = self.parties.iter().position(|p| p.name == name).unwrap();
        application::app::hex(&[u8::try_from(0x31 + index).unwrap(); 32])
    }
}
impl Drop for Journey {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
// One server, as app-server binds it, over the journey's store.
fn serve<'a, 'p>(
    journey: &Journey,
    lineage: &'a Lineage<'a, 'p>,
) -> Server<Application<'a, 'p, Production<'a, 'p, FileReceiver, Clock>>, Sink> {
    let authority = &lineage.authorities()[0];
    let table = ok(application::app::party_contexts(
        authority,
        &journey.parties,
        journey.clock().as_ref(),
    ));
    let production = ok(Production::open(
        &journey.path,
        lineage,
        1,
        ok(FileReceiver::new(&journey.receiver)),
        Some(Clock::default()),
    ));
    let store = production.store();
    let mut endpoint = ok(Application::with_parties(
        lineage,
        production,
        store,
        &journey.path,
        table,
    ));
    if let Some(clock) = journey.clock() {
        endpoint = ok(endpoint.with_clock(clock));
    }
    let endpoint = endpoint.with_views(ok(application::app::party_views(&journey.parties)));
    let config = ok(application::app::ingress_config(
        &journey.parties,
        RustCryptoSha256::hash(authority.identity()),
        "http://localhost".into(),
    ));
    Server::new(config, endpoint, Sink::default())
}
fn snapshot<B: zeno_fcis_ingress::Endpoint>(
    server: &mut Server<B, Sink>,
    credential: &str,
) -> (u16, Value) {
    let (status, bytes) = exchange_as(server, "/state", credential, &[]);
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
// Preview, then send one step as its party; returns the original request and reply.
fn send<B: zeno_fcis_ingress::Endpoint>(
    server: &mut Server<B, Sink>,
    journey: &Journey,
    name: &str,
    command: &[Value],
    key: u8,
) -> (u16, Value, Vec<u8>, Vec<u8>) {
    let credential = journey.credential(name);
    let (status, state) = snapshot(server, &credential);
    assert_eq!(status, 200, "snapshot for {name}");
    let contract = Hash32::new(ok(<[u8; 32]>::try_from(ok(application::app::unhex(
        state["contract"].as_str().unwrap(),
    )))));
    let revision: u64 = ok(state["revision"].as_str().unwrap().parse());
    let mut input: Vec<Value> = state["values"].as_array().unwrap().clone();
    input.extend(command.iter().cloned());
    input.extend(state["context"].as_array().unwrap().iter().cloned());
    let body = encode_command(
        contract,
        Hash32::new([key; 32]),
        revision,
        &ok(serde_json::to_vec(&input)),
    );
    let (status, bytes) = exchange_as(server, "/preview", &credential, &body);
    assert_eq!(status, 200, "preview for {name}");
    let preview: Value = ok(serde_json::from_slice(&bytes));
    let framed = ok(application::app::unhex(
        preview["command"].as_str().unwrap(),
    ));
    let original = encode_command(contract, Hash32::new([key; 32]), revision, &framed);
    let (status, bytes) = exchange_as(server, "/command", &credential, &original);
    (status, preview["reply"].clone(), original, bytes)
}

#[test]
fn parties_journey() {
    let Some(journey) = Journey::new("parties") else {
        return;
    };
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let catalog = ok(v2_contract::checked_catalog(&descriptor));
    let catalogs = [&catalog];
    let lineage = ok(Lineage::bind(&catalogs, &[]));
    // One server process serves every party's step in turn.
    let mut server = serve(&journey, &lineage);
    let mut accepted = Vec::new();
    let steps = journey.fixture["steps"].as_array().unwrap().clone();
    for (index, step) in steps.iter().enumerate() {
        let name = step["party"].as_str().unwrap();
        if let Some(time) = step["clock"].as_str() {
            journey.set_clock(time);
        }
        let before = ok(application::current(&journey.path));
        let command = step["command"].as_array().unwrap();
        let key = u8::try_from(index + 1).unwrap();
        // A request cannot choose another party's context.
        let (_, own) = snapshot(&mut server, &journey.credential(name));
        for other in journey.parties.iter().filter(|p| p.name != name) {
            let (_, theirs) = snapshot(&mut server, &journey.credential(&other.name));
            if theirs["context"] == own["context"] {
                continue;
            }
            let mut forged: Vec<Value> = own["values"].as_array().unwrap().clone();
            forged.extend(command.iter().cloned());
            forged.extend(theirs["context"].as_array().unwrap().iter().cloned());
            let contract = Hash32::new(ok(<[u8; 32]>::try_from(ok(application::app::unhex(
                own["contract"].as_str().unwrap(),
            )))));
            let body = encode_command(
                contract,
                Hash32::new([key; 32]),
                before.head.commits,
                &ok(serde_json::to_vec(&forged)),
            );
            let (status, _) =
                exchange_as(&mut server, "/preview", &journey.credential(name), &body);
            assert_eq!(status, 403, "a request chose another party's context");
        }
        let (status, reply, original, bytes) = send(&mut server, &journey, name, command, key);
        assert_eq!(reply["class"], step["class"], "step {index}");
        if step["class"] == "Reject" {
            assert_eq!(status, 422, "a party used another party's permission");
            assert_eq!(ok(application::current(&journey.path)), before);
        } else {
            assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
            accepted.push((name.to_owned(), original, bytes));
        }
    }
    let (status, _) = snapshot(&mut server, &"ee".repeat(32));
    assert_eq!(status, 401, "unknown credential admitted");
    for party in &journey.parties {
        let (status, state) = snapshot(&mut server, &journey.credential(&party.name));
        assert_eq!(status, 200);
        let mut context: Vec<Value> = state["context"].as_array().unwrap().clone();
        if let Some(clock) = journey.clock() {
            context.remove(clock.position());
        }
        let expected: Vec<Value> = party.context.iter().map(|v| json!(v.to_string())).collect();
        assert_eq!(context, expected, "snapshot of another party");
        assert_eq!(
            state["principal"],
            application::app::hex(party.principal.as_bytes())
        );
    }
    let history = application::history(&journey.path).map(|history| history.entries.len());
    assert_eq!(
        history,
        Ok(accepted.len()),
        "an admitted request is missing from the journal"
    );
    // A party's view hides state values in its snapshots and replies, chosen by
    // its credential, whatever its request holds.
    let state = ok(application::current(&journey.path)).state;
    let views = ok(application::app::party_views(&journey.parties));
    for (party, (_, view)) in journey.parties.iter().zip(&views) {
        let (_, snapshot) = snapshot(&mut server, &journey.credential(&party.name));
        for (index, seen) in view.iter().enumerate() {
            assert_eq!(
                snapshot["values"][index].is_null(),
                !seen,
                "a hidden field appears in /state"
            );
        }
        if view.iter().all(|seen| *seen) {
            continue;
        }
        let mut input: Vec<Value> = state.iter().map(|v| json!(v.to_string())).collect();
        input.extend(steps[0]["command"].as_array().unwrap().iter().cloned());
        input.extend(snapshot["context"].as_array().unwrap().iter().cloned());
        let contract = Hash32::new(ok(<[u8; 32]>::try_from(ok(application::app::unhex(
            snapshot["contract"].as_str().unwrap(),
        )))));
        let revision: u64 = ok(snapshot["revision"].as_str().unwrap().parse());
        let body = encode_command(
            contract,
            Hash32::new([0xdd; 32]),
            revision,
            &ok(serde_json::to_vec(&input)),
        );
        let (status, bytes) = exchange_as(
            &mut server,
            "/preview",
            &journey.credential(&party.name),
            &body,
        );
        assert_eq!(status, 200);
        let preview: Value = ok(serde_json::from_slice(&bytes));
        for (index, seen) in view.iter().enumerate() {
            assert_eq!(
                preview["reply"]["input"][index].is_null(),
                !seen,
                "the view is chosen from the request"
            );
        }
    }
    drop(server);
    // After a restart the party recovers its original packet; another party cannot.
    let mut server = serve(&journey, &lineage);
    let (name, original, packet) = accepted.last().unwrap();
    let (status, bytes) = exchange_as(&mut server, "/command", &journey.credential(name), original);
    assert_eq!(status, 200);
    assert_eq!(&bytes, packet, "restart changed original packet");
    for other in journey.parties.iter().filter(|p| p.name != *name) {
        let (status, _) = exchange_as(
            &mut server,
            "/command",
            &journey.credential(&other.name),
            original,
        );
        assert_ne!(status, 200, "another party replayed a request");
    }
    if let Some(expected) = journey.fixture["deliveries"].as_u64() {
        assert_eq!(received(&journey.receiver) as u64, expected);
    }
}

#[test]
fn server_clock_journey() {
    let Some(journey) = Journey::new("clock") else {
        return;
    };
    let Some(clock) = journey.clock() else {
        return;
    };
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let catalog = ok(v2_contract::checked_catalog(&descriptor));
    let catalogs = [&catalog];
    let lineage = ok(Lineage::bind(&catalogs, &[]));
    let mut server = serve(&journey, &lineage);
    let steps = journey.fixture["steps"].as_array().unwrap().clone();
    let mut last = None;
    for (index, step) in steps.iter().enumerate() {
        journey.set_clock(step["clock"].as_str().unwrap());
        let name = step["party"].as_str().unwrap();
        let command = step["command"].as_array().unwrap();
        let key = u8::try_from(index + 1).unwrap();
        let (status, reply, original, bytes) = send(&mut server, &journey, name, command, key);
        // Every decision records the server's time, not the request's.
        assert_eq!(
            reply["input"][reply["input"].as_array().unwrap().len()
                - journey.parties[0].context.len()
                - 1
                + clock.position()],
            step["clock"]
        );
        if status == 200 {
            last = Some((name.to_owned(), original, bytes, step["clock"].clone()));
        }
    }
    let (name, original, packet, time) = last.expect("an accepted clocked step");
    let committed: i128 = ok(time.as_str().unwrap().parse());
    // Time moves on: the original request still returns its original decision.
    journey.set_clock(&(committed + 1).to_string());
    let (status, bytes) = exchange_as(
        &mut server,
        "/command",
        &journey.credential(&name),
        &original,
    );
    assert_eq!(status, 200, "clock read again at replay");
    assert_eq!(bytes, packet, "clock read again at replay");
    drop(server);
    // After a restart the clock never goes back before the last committed time.
    journey.set_clock("0");
    let mut server = serve(&journey, &lineage);
    let (status, state) = snapshot(&mut server, &journey.credential(&name));
    assert_eq!(status, 200);
    assert_eq!(
        state["context"][clock.position()],
        json!(committed.to_string()),
        "clock went backward after a restart"
    );
    // A time outside the declared range is refused, never clamped.
    let before = ok(application::current(&journey.path));
    journey.set_clock("1000000");
    let (status, _) = snapshot(&mut server, &journey.credential(&name));
    assert_eq!(status, 503, "out-of-range clock clamped");
    let body = encode_command(
        RustCryptoSha256::hash(ok(application::authority(&descriptor)).identity()),
        Hash32::new([0xcc; 32]),
        before.head.commits,
        &ok(application::app::unhex(
            original_command(&original).as_str(),
        )),
    );
    let (status, bytes) = exchange_as(&mut server, "/command", &journey.credential(&name), &body);
    assert_eq!(status, 503, "out-of-range clock clamped");
    assert_eq!(bytes, b"Clock");
    assert_eq!(ok(application::current(&journey.path)), before);
}
// The framed command of an original request body, as hexadecimal.
fn original_command(body: &[u8]) -> String {
    application::app::hex(&body[73..])
}

#[test]
fn program_connector_acknowledges_only_success() {
    use application::app::ProgramConnector;
    use zeno_fcis_shell::effects::port::{Delivery, Fault, Sent};
    use zeno_fcis_shell_sqlite::effects::production::Transport;
    let root = std::env::temp_dir().join(format!("app-connector-{}", std::process::id()));
    ok(std::fs::create_dir(&root));
    let payload = b"connector delivery".to_vec();
    let delivery = Delivery {
        id: Hash32::new([5; 32]),
        version: 2,
        lane: 1,
        ordinal: 0,
        destination: [&[6, 0, 0, 0, 5][..], b"buyer"].concat(),
        payload_sha256: RustCryptoSha256::hash(&payload),
        payload,
    };
    let connector = |argv: Value, timeout: u64| {
        let file = root.join("connector.json");
        ok(std::fs::write(
            &file,
            json!({"schema":"zeno-fcis/app-connector/1","argv":argv,"timeout_seconds":timeout})
                .to_string(),
        ));
        ProgramConnector::load(&file)
    };
    // The program reads the envelope and payload on standard input.
    let received = root.join("received.json");
    let mut tee = ok(connector(json!(["/usr/bin/tee", received]), 10));
    assert!(matches!(tee.send(&delivery), Sent::Sent(_)));
    let input: Value = ok(serde_json::from_slice(&ok(std::fs::read(&received))));
    assert_eq!(
        input["delivery"],
        application::app::hex(delivery.id.as_bytes()),
        "the delivery ID is missing from the envelope"
    );
    assert_eq!(input["destination_text"], "buyer");
    assert_eq!(input["commit"], 2);
    assert_eq!(input["payload"], application::app::hex(&delivery.payload));
    // Any other exit, or the time limit, leaves the delivery pending.
    let mut fails = ok(connector(json!(["/usr/bin/false"]), 10));
    assert_eq!(
        fails.send(&delivery),
        Sent::Unknown(Fault::Io),
        "a non-zero exit is acknowledged"
    );
    let started = std::time::Instant::now();
    let mut slow = ok(connector(json!(["/bin/sleep", "30"]), 1));
    assert_eq!(slow.send(&delivery), Sent::Unknown(Fault::Io));
    assert!(started.elapsed() < Duration::from_secs(20), "time limit");
    let mut missing = ok(connector(json!(["/no/such/program"]), 1));
    assert_eq!(missing.send(&delivery), Sent::Unknown(Fault::Io));
    for (argv, timeout) in [
        (json!(["tee", "out"]), 10),
        (json!([]), 10),
        (json!(["/usr/bin/tee"]), 0),
        (json!(["/usr/bin/tee"]), 61),
    ] {
        assert!(
            connector(argv, timeout).is_err(),
            "connector configuration accepted"
        );
    }
    ok(std::fs::remove_dir_all(root));
}

#[test]
fn public_origin_is_the_only_origin() {
    let root = std::env::temp_dir().join(format!("app-origin-{}", std::process::id()));
    ok(std::fs::create_dir(&root));
    let path = root.join("app.sqlite");
    ok(application::init(&path));
    let fixture: Value = ok(serde_json::from_slice(include_bytes!(
        "../app-checks/journey.json"
    )));
    let step = &fixture["steps"][0];
    let contract = v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = ok(application::authority(&descriptor));
    let hash = RustCryptoSha256::hash(authority.identity());
    let catalog = ok(v2_contract::checked_catalog(&descriptor));
    let catalogs = [&catalog];
    let lineage = ok(Lineage::bind(&catalogs, &[]));
    let clock = ok(application::app::ServerClock::declared(Some(
        root.join("clock"),
    )));
    ok(std::fs::write(
        root.join("clock"),
        step["clock"].as_str().unwrap_or("0"),
    ));
    // The request's inputs: the current state, the step's command and its context
    // with any server clock in place.
    let mut context: Vec<i128> = step_context(&fixture, step).iter().map(scalar).collect();
    if let Some(clock) = &clock {
        context.insert(clock.position(), scalar(&step["clock"]));
    }
    let before = ok(application::current(&path));
    let mut input = before.state.clone();
    input.extend(step["command"].as_array().unwrap().iter().map(scalar));
    input.extend(&context);
    let frames = ok(application::wire(&descriptor, &input));
    let principal = Hash32::new([7; 32]);
    let preview = encode_command(
        hash,
        Hash32::new([1; 32]),
        before.head.commits,
        &ok(serde_json::to_vec(
            &input.iter().map(ToString::to_string).collect::<Vec<_>>(),
        )),
    );
    let command = encode_command(hash, Hash32::new([1; 32]), before.head.commits, &frames[1]);
    let serve = |origin: String| {
        let production = ok(Production::open(
            &path,
            &lineage,
            1,
            ok(FileReceiver::new(&root.join("received"))),
            Some(Clock::default()),
        ));
        let store = production.store();
        let mut endpoint = ok(Application::new(
            &lineage, production, store, &path, principal, &frames[2],
        ));
        if let Some(clock) =
            application::app::ServerClock::declared(Some(root.join("clock"))).unwrap()
        {
            endpoint = ok(endpoint.with_clock(clock));
        }
        let config = ok(Config::new(
            [0xab; 32],
            principal,
            hash,
            origin,
            Duration::from_secs(10),
        ));
        Server::new(config, endpoint, Sink::default())
    };
    // Unset, the loopback origin is the only one, as before.
    let mut server = serve("http://127.0.0.1:8080".into());
    let (status, _) = exchange_bytes(
        &mut server,
        wire_origin("/preview", "http://127.0.0.1:8080", &preview),
    );
    assert_eq!(status, 200);
    let (status, _) = exchange_bytes(
        &mut server,
        wire_origin("/preview", "https://app.example.com", &preview),
    );
    assert_eq!(status, 403);
    drop(server);
    // With ZENO_APP_ORIGIN in force, only that exact origin is accepted.
    let origin = ok(application::app::public_origin("https://app.example.com"));
    let mut server = serve(origin);
    for route in ["/preview", "/command"] {
        let body = if route == "/preview" {
            &preview
        } else {
            &command
        };
        for other in [
            "http://127.0.0.1:8080",
            "https://evil.example",
            "https://app.example.com.evil.example",
            "https://APP.example.com",
        ] {
            let (status, _) = exchange_bytes(&mut server, wire_origin(route, other, body));
            assert_eq!(status, 403, "configured origin not exact: {other}");
        }
        assert_eq!(ok(application::current(&path)), before);
        let (status, bytes) = exchange_bytes(
            &mut server,
            wire_origin(route, "https://app.example.com", body),
        );
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
    }
    drop(server);
    ok(std::fs::remove_dir_all(root));
}
