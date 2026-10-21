//! HTTP admission and framing checks, with explicit transport selection.
mod support;
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpStream},
    time::Duration,
};
use zeno_fcis_codec::Hash32;
use zeno_fcis_ingress::{
    Command, Config, Endpoint, Event, MAX_BODY, MAX_HEADER, Refusal, Server, Trace, TraceSink,
    encode_command, runtime,
};

#[derive(Default)]
struct Sink(Vec<Trace>);
impl TraceSink for Sink {
    fn record(&mut self, trace: Trace) {
        self.0.push(trace);
    }
}
struct Echo;
impl Endpoint for Echo {
    fn execute(&mut self, command: &Command, _: &mut dyn TraceSink) -> Result<Vec<u8>, Refusal> {
        Ok(command.principal().as_bytes().to_vec())
    }
}
fn ok<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
    value.unwrap_or_else(|e| panic!("unexpected: {e:?}"))
}
fn server() -> Server<Echo, Sink> {
    Server::new(
        ok(Config::new(
            [0xab; 32],
            Hash32::new([7; 32]),
            Hash32::new([8; 32]),
            "https://app.example".into(),
            Duration::from_millis(80),
        )),
        Echo,
        Sink::default(),
    )
}
fn body() -> Vec<u8> {
    encode_command(Hash32::new([8; 32]), Hash32::new([9; 32]), 0, &[3; 64])
}
fn request(body: &[u8], extra: &str) -> Vec<u8> {
    let mut bytes=format!("POST /command HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n{}\r\n", "ab".repeat(32),body.len(),extra).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
fn exchange(server: &mut Server<Echo, Sink>, request: Vec<u8>, delay: bool) -> Vec<u8> {
    let rt = ok(runtime());
    if std::env::var_os("INGRESS_TEST_MEMORY").is_some() {
        return support::exchange(server, request, delay);
    }
    let listener = ok(rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")));
    let address = ok(listener.local_addr());
    let child = std::thread::spawn(move || {
        let mut client = ok(TcpStream::connect(address));
        ok(client.set_read_timeout(Some(Duration::from_secs(3))));
        let _ = client.write_all(&request);
        if delay {
            std::thread::sleep(Duration::from_millis(150));
        }
        let _ = client.shutdown(Shutdown::Write);
        let mut out = Vec::new();
        let _ = client.read_to_end(&mut out);
        out
    });
    let _ = rt.block_on(server.serve_one(&listener));
    ok(child.join())
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
fn payload(bytes: &[u8]) -> &[u8] {
    let index = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("HTTP response"));
    &bytes[index + 4..]
}
#[test]
fn actor_from_credential() {
    let response = exchange(&mut server(), request(&body(), ""), false);
    assert_eq!(status(&response), 200);
    assert_eq!(payload(&response), [7; 32], "actor came from body");
}
#[test]
fn duplicate_headers() {
    for extra in [
        format!("Authorization: Bearer {}\r\n", "ab".repeat(32)),
        "Origin: https://app.example\r\nOrigin: https://app.example\r\n".into(),
        "X-Other: 1\r\nX-Other: 2\r\n".into(),
        "Host: localhost\r\n".into(),
        format!("Content-Length: {}\r\n", body().len()),
    ] {
        let response = exchange(&mut server(), request(&body(), &extra), false);
        assert_ne!(status(&response), 200, "duplicate header accepted");
    }
}
#[test]
fn cross_origin() {
    for origin in ["https://evil.example", "null", "https://app.example.evil"] {
        let response = exchange(
            &mut server(),
            request(&body(), &format!("Origin: {origin}\r\n")),
            false,
        );
        assert_eq!(status(&response), 403, "cross-origin accepted");
    }
    assert_eq!(
        status(&exchange(
            &mut server(),
            request(&body(), "Origin: https://app.example\r\n"),
            false
        )),
        200
    );
}
#[test]
fn oversized_requests() {
    let mut oversized = body();
    oversized.resize(MAX_BODY + 1, 3);
    assert_eq!(
        status(&exchange(&mut server(), request(&oversized, ""), false)),
        413,
        "body size limit missing"
    );
    let mut chunked=format!("POST /command HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n","ab".repeat(32),MAX_BODY+1).into_bytes();
    chunked.extend_from_slice(&oversized);
    chunked.extend_from_slice(b"\r\n0\r\n\r\n");
    assert_eq!(
        status(&exchange(&mut server(), chunked, false)),
        413,
        "chunked body size limit missing"
    );
    assert_ne!(
        status(&exchange(
            &mut server(),
            request(&body(), &format!("X-Large: {}\r\n", "a".repeat(MAX_HEADER))),
            false
        )),
        200,
        "header size limit missing"
    );
}
#[test]
fn malformed_and_credentials() {
    for credential in [
        "",
        "Bearer short",
        &format!("Bearer {}", "AB".repeat(32)),
        &format!("Bearer {}", "ac".repeat(32)),
        &format!("Bearer {}", "ag".repeat(32)),
    ] {
        let wire = String::from_utf8_lossy(&request(&body(), ""))
            .replace(&format!("Bearer {}", "ab".repeat(32)), credential)
            .into_bytes();
        assert_ne!(status(&exchange(&mut server(), wire, false)), 200);
    }
    for change in [
        "GET /command",
        "POST /wrong",
        "POST /command?x=1",
        "POST http://evil.example/command",
    ] {
        let wire = String::from_utf8_lossy(&request(&body(), ""))
            .replace("POST /command", change)
            .into_bytes();
        assert_ne!(status(&exchange(&mut server(), wire, false)), 200);
    }
    assert_eq!(
        status(&exchange(&mut server(), request(&[0; 74], ""), false)),
        400
    );
    let mut wrong = body();
    wrong[1] ^= 1;
    assert_eq!(
        status(&exchange(&mut server(), request(&wrong, ""), false)),
        403
    );
    let raw = b"POST /command HTTP/1.1\r\nHost: local\r\nBad Header: x\r\n\r\n".to_vec();
    assert_ne!(status(&exchange(&mut server(), raw, false)), 200);
}
#[test]
fn slow_clients() {
    for wire in [
        b"POST /command HTTP/1.1\r\nHost:".to_vec(),
        request(&body(), "")[..request(&body(), "").len() - 1].to_vec(),
    ] {
        let started = std::time::Instant::now();
        let response = exchange(&mut server(), wire, true);
        assert_ne!(status(&response), 200);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
#[test]
fn trace_redaction() {
    let mut server = server();
    let _ = exchange(&mut server, request(&body(), ""), false);
    let (_, sink) = server.into_parts();
    let text = sink
        .0
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!text.contains(&"ab".repeat(32)), "secret in trace");
    assert!(!text.contains("Bearer"));
    assert!(
        sink.0
            .iter()
            .any(|t| t.event == Event::Request && t.request.is_some() && t.replay.is_some())
    );
}

#[test]
fn ambiguous_framing() {
    let wire = request(&body(), "Transfer-Encoding: chunked\r\n");
    assert_ne!(
        status(&exchange(&mut server(), wire, false)),
        200,
        "length and transfer encoding accepted"
    );
}

#[test]
fn transfer_codings_other_than_chunked() {
    let chunked = |coding: &str| {
        let mut wire = format!(
            "POST /command HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nTransfer-Encoding: {coding}\r\n\r\n{:x}\r\n",
            "ab".repeat(32),
            body().len()
        )
        .into_bytes();
        wire.extend_from_slice(&body());
        wire.extend_from_slice(b"\r\n0\r\n\r\n");
        wire
    };
    assert_eq!(status(&exchange(&mut server(), chunked("chunked"), false)), 200);
    for coding in ["gzip, chunked", "deflate, chunked", "x-custom, chunked"] {
        assert_eq!(
            status(&exchange(&mut server(), chunked(coding), false)),
            400,
            "transfer coding other than chunked accepted: {coding}"
        );
    }
}

#[test]
fn closed_asset_routes_and_authenticated_views() {
    use zeno_fcis_ingress::Asset;
    let mut assets_server = ok(server().with_assets(vec![ok(Asset::new(
        "/",
        "text/html; charset=utf-8",
        b"checked app",
    ))]));
    for (path, expected) in [
        ("/", 200),
        ("/../Cargo.toml", 400),
        ("/%2e%2e/Cargo.toml", 400),
        ("/unknown", 400),
        ("/?x=1", 400),
    ] {
        let wire = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").into_bytes();
        let reply = exchange(&mut assets_server, wire, false);
        assert_eq!(status(&reply), expected);
        if expected == 200 {
            assert_eq!(payload(&reply), b"checked app");
            assert!(String::from_utf8_lossy(&reply).contains("nosniff"));
        }
    }
    let reply = exchange(
        &mut assets_server,
        b"GET /state HTTP/1.1\r\nHost: localhost\r\n\r\n".to_vec(),
        false,
    );
    assert_eq!(status(&reply), 401);
    let reply = exchange(
        &mut assets_server,
        b"GET / HTTP/1.1\r\nHost: localhost\r\nOrigin: https://evil.example\r\n\r\n".to_vec(),
        false,
    );
    assert_ne!(status(&reply), 200);
    assert!(Asset::new("/../x", "text/html; charset=utf-8", b"x").is_err());
    assert!(
        server()
            .with_assets(vec![ok(Asset::new("/state", "application/json", b"{}"))])
            .is_err()
    );
    assert!(
        server()
            .with_assets(vec![
                ok(Asset::new("/", "application/json", b"{}")),
                ok(Asset::new("/", "application/json", b"{}"))
            ])
            .is_err()
    );
}
fn party_server() -> Server<Echo, Sink> {
    Server::new(
        ok(Config::with_parties(
            vec![
                ([0xab; 32], Hash32::new([7; 32])),
                ([0xcd; 32], Hash32::new([17; 32])),
                ([0xef; 32], Hash32::new([27; 32])),
            ],
            Hash32::new([8; 32]),
            "https://app.example".into(),
            Duration::from_millis(80),
        )),
        Echo,
        Sink::default(),
    )
}
fn party_request(credential: &str) -> Vec<u8> {
    let body = body();
    let mut bytes = format!(
        "POST /command HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {credential}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(&body);
    bytes
}
#[test]
fn party_credentials_select_principals() {
    for (credential, principal) in [("ab", 7), ("cd", 17), ("ef", 27)] {
        let response = exchange(
            &mut party_server(),
            party_request(&credential.repeat(32)),
            false,
        );
        assert_eq!(status(&response), 200);
        assert_eq!(payload(&response), [principal; 32], "wrong principal");
    }
    for credential in ["12".repeat(32), "ab".repeat(31) + "ac", String::new()] {
        let response = exchange(&mut party_server(), party_request(&credential), false);
        assert_eq!(status(&response), 401, "unknown credential admitted");
    }
}
#[test]
fn party_configuration_refusals() {
    let config = |parties: Vec<([u8; 32], Hash32)>| {
        Config::with_parties(
            parties,
            Hash32::new([8; 32]),
            "https://app.example".into(),
            Duration::from_millis(80),
        )
        .is_err()
    };
    let many = (0..=64u8)
        .map(|n| ([n + 1; 32], Hash32::new([n; 32])))
        .collect();
    assert!(config(Vec::new()), "empty party table");
    assert!(config(many), "more than 64 parties");
    assert!(
        config(vec![([0; 32], Hash32::new([1; 32]))]),
        "zero credential"
    );
    assert!(
        config(vec![
            ([1; 32], Hash32::new([1; 32])),
            ([1; 32], Hash32::new([2; 32]))
        ]),
        "duplicate credential"
    );
    assert!(
        config(vec![
            ([1; 32], Hash32::new([1; 32])),
            ([2; 32], Hash32::new([1; 32]))
        ]),
        "duplicate principal"
    );
    assert!(!config(vec![
        ([1; 32], Hash32::new([1; 32])),
        ([2; 32], Hash32::new([2; 32]))
    ]));
}
// A preview-capable endpoint, as an application serves one.
struct Previewing;
impl Endpoint for Previewing {
    fn preview(&mut self, command: &Command) -> Result<Vec<u8>, Refusal> {
        Ok(command.principal().as_bytes().to_vec())
    }
    fn execute(&mut self, command: &Command, _: &mut dyn TraceSink) -> Result<Vec<u8>, Refusal> {
        Ok(command.principal().as_bytes().to_vec())
    }
}
#[test]
fn configured_https_origin_is_exact() {
    // The public origin a TLS proxy serves replaces the loopback origin.
    let server = || {
        Server::new(
            ok(Config::new(
                [0xab; 32],
                Hash32::new([7; 32]),
                Hash32::new([8; 32]),
                "https://app.example.com".into(),
                Duration::from_millis(80),
            )),
            Previewing,
            Sink::default(),
        )
    };
    let send = |path: &str, origin: &str| {
        let body = body();
        let mut bytes = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:8080\r\nAuthorization: Bearer {}\r\nOrigin: {origin}\r\nContent-Length: {}\r\n\r\n",
            "ab".repeat(32),
            body.len()
        )
        .into_bytes();
        bytes.extend_from_slice(&body);
        status(&exchange_with(&mut server(), bytes))
    };
    for path in ["/preview", "/command"] {
        assert_eq!(send(path, "https://app.example.com"), 200, "{path}");
        for other in [
            "http://127.0.0.1:8080",
            "https://evil.example",
            "https://app.example.com.evil.example",
            "https://APP.example.com",
            "https://app.example.co",
            "http://app.example.com",
        ] {
            assert_eq!(
                send(path, other),
                403,
                "configured origin not exact: {other}"
            );
        }
    }
}
fn exchange_with(server: &mut Server<Previewing, Sink>, request: Vec<u8>) -> Vec<u8> {
    let rt = ok(runtime());
    if std::env::var_os("INGRESS_TEST_MEMORY").is_some() {
        return support::exchange(server, request, false);
    }
    let listener = ok(rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")));
    let address = ok(listener.local_addr());
    let child = std::thread::spawn(move || {
        let mut client = ok(TcpStream::connect(address));
        ok(client.set_read_timeout(Some(Duration::from_secs(3))));
        let _ = client.write_all(&request);
        let _ = client.shutdown(Shutdown::Write);
        let mut out = Vec::new();
        let _ = client.read_to_end(&mut out);
        out
    });
    let _ = rt.block_on(server.serve_one(&listener));
    ok(child.join())
}
