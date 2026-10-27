//! One bounded HTTP/1 request per connection, on a current-thread runtime.
use crate::{Asset, Command, Endpoint, Event, Refusal, Trace, TraceSink};
use http_body_util::{BodyExt, Full, Limited};
use hyper::{
    Method, Request, Response, StatusCode,
    body::{Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{cell::RefCell, convert::Infallible, io, time::Duration};
use tokio::{
    net::TcpListener,
    runtime::{Builder, Runtime},
    time::timeout,
};
use zeno_fcis_codec::Hash32;

/// Total HTTP header byte limit, enforced by hyper's framing parser.
pub const MAX_HEADER: usize = 4096;
/// Maximum body bytes, enforced while streaming, including chunked bodies.
pub const MAX_BODY: usize = 65_536;
/// Maximum configured credentials; each names exactly one principal.
pub const MAX_PARTIES: usize = 64;

/// Owned immutable transport policy. Deliberately has no Debug implementation.
pub struct Config {
    parties: Vec<([u8; 32], Hash32)>,
    contract: Hash32,
    origin: String,
    deadline: Duration,
}
impl Config {
    /// Bind one 32-byte credential to a principal, contract and exact browser origin.
    /// TLS must terminate at a trusted proxy protecting this loopback connection.
    pub fn new(
        secret: [u8; 32],
        principal: Hash32,
        contract: Hash32,
        origin: String,
        deadline: Duration,
    ) -> Result<Self, Refusal> {
        Self::with_parties(vec![(secret, principal)], contract, origin, deadline)
    }
    /// Bind 1 to 64 nonzero credentials, each to its own principal, under one
    /// contract, origin and deadline. Credentials and principals must be distinct:
    /// the presented credential alone selects the principal.
    pub fn with_parties(
        parties: Vec<([u8; 32], Hash32)>,
        contract: Hash32,
        origin: String,
        deadline: Duration,
    ) -> Result<Self, Refusal> {
        if parties.is_empty()
            || parties.len() > MAX_PARTIES
            || parties.iter().any(|(secret, _)| *secret == [0; 32])
            || parties.iter().enumerate().any(|(i, (secret, principal))| {
                parties[..i]
                    .iter()
                    .any(|(old, owner)| old == secret || owner == principal)
            })
            || !(origin.starts_with("https://") || origin.starts_with("http://"))
            || !origin.is_ascii()
            || origin.bytes().any(|b| b.is_ascii_whitespace())
            || deadline.is_zero()
            || deadline > Duration::from_secs(60)
        {
            return Err(Refusal::Malformed);
        }
        Ok(Self {
            parties,
            contract,
            origin,
            deadline,
        })
    }
}

/// Hand-built current-thread runtime, with I/O and timers and no macros feature.
pub fn runtime() -> io::Result<Runtime> {
    Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
}

/// Owns the shell and output sink exclusively. Requests run serially; no decision cache.
pub struct Server<B, S> {
    config: Config,
    state: RefCell<(B, S)>,
    assets: Vec<Asset>,
}
impl<B: Endpoint, S: TraceSink> Server<B, S> {
    /// Create a serial server with its configured application and trace sink.
    pub fn new(config: Config, backend: B, sink: S) -> Self {
        Self {
            config,
            state: RefCell::new((backend, sink)),
            assets: Vec::new(),
        }
    }
    /// Install a bounded, closed set of public assets before serving.
    /// Copies are already owned by Asset; no mutable aliases escape.
    pub fn with_assets(mut self, assets: Vec<Asset>) -> Result<Self, Refusal> {
        let total = assets
            .iter()
            .try_fold(0usize, |sum, asset| sum.checked_add(asset.bytes.len()));
        if assets.len() > 128
            || total.is_none_or(|n| n > 32 * 1024 * 1024)
            || assets
                .iter()
                .enumerate()
                .any(|(i, asset)| assets[..i].iter().any(|old| old.route == asset.route))
            || assets
                .iter()
                .any(|asset| ["/state", "/preview", "/command"].contains(&asset.route.as_str()))
        {
            return Err(Refusal::Malformed);
        }
        self.assets = assets;
        Ok(self)
    }
    /// Recover the caller-owned shell and sink after serving.
    pub fn into_parts(self) -> (B, S) {
        self.state.into_inner()
    }
    /// Accept and serve one connection. The client has one total header/body deadline.
    /// A synchronous core or SQLite call runs to completion once entered.
    pub async fn serve_one(&mut self, listener: &TcpListener) -> io::Result<()> {
        let (stream, peer) = listener.accept().await?;
        if !peer.ip().is_loopback() || !listener.local_addr()?.ip().is_loopback() {
            self.state
                .get_mut()
                .1
                .record(Trace::empty(Event::Refused(Refusal::Binding)));
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "loopback required",
            ));
        }
        self.serve_connection(stream).await
    }

    /// Serve one caller-supplied local byte stream with the same authentication,
    /// framing limits and deadlines. The caller owns transport protection and
    /// peer validation; this also supports a Unix-socket TLS proxy.
    pub async fn serve_connection<I>(&mut self, stream: I) -> io::Result<()>
    where
        I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let config = &self.config;
        let state = &self.state;
        let service = service_fn(|request| handle(config, state, &self.assets, request));
        let mut builder = http1::Builder::new();
        builder
            .keep_alive(false)
            .half_close(true)
            .max_headers(24)
            .max_header_size(MAX_HEADER)
            .timer(TokioTimer::new())
            .header_read_timeout(config.deadline);
        match timeout(
            config.deadline,
            builder.serve_connection(TokioIo::new(HeaderGuard::new(stream)), service),
        )
        .await
        {
            Ok(Ok(())) => Ok(()),
            result => {
                let refusal = if result.is_err() {
                    Refusal::Unavailable
                } else {
                    Refusal::Malformed
                };
                self.state
                    .get_mut()
                    .1
                    .record(Trace::empty(Event::Refused(refusal)));
                Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "request incomplete",
                ))
            }
        }
    }
}

async fn handle<B: Endpoint, S: TraceSink>(
    config: &Config,
    state: &RefCell<(B, S)>,
    assets: &[Asset],
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = request.uri().path().to_owned();
    if request.method() == Method::GET && path != "/state" {
        let result = headers(config, &request).and_then(|()| empty_body(&request));
        if result.is_ok()
            && request.uri().query().is_none()
            && request.uri().authority().is_none()
            && let Some(asset) = assets.iter().find(|asset| asset.route == path)
        {
            return Ok(response(
                StatusCode::OK,
                asset.bytes.clone(),
                asset.content_type,
            ));
        }
        return Ok(response(
            StatusCode::BAD_REQUEST,
            b"Malformed".to_vec(),
            "application/octet-stream",
        ));
    }
    if request.method() == Method::GET && path == "/state" {
        let admission = headers(config, &request)
            .and_then(|()| authenticate(config, &request))
            .and_then(|principal| empty_body(&request).map(|()| principal));
        let (backend, sink) = &mut *state.borrow_mut();
        let result = admission.and_then(|principal| backend.snapshot(principal));
        if let Err(error) = result {
            sink.record(Trace::empty(Event::Refused(error)));
        }
        return Ok(result_response(result, "application/json"));
    }
    let result = admit(config, request).await;
    let (backend, sink) = &mut *state.borrow_mut();
    let result = match result {
        Ok(command) => {
            let trace = Trace::request(&command);
            sink.record(trace);
            let result = if path == "/preview" {
                backend.preview(&command)
            } else {
                backend.execute(&command, sink)
            };
            sink.record(Trace {
                event: match result {
                    Ok(_) => Event::Reply,
                    Err(error) => Event::Refused(error),
                },
                ..trace
            });
            result
        }
        Err(error) => {
            sink.record(Trace::empty(Event::Refused(error)));
            Err(error)
        }
    };
    Ok(result_response(result, "application/octet-stream"))
}

fn result_response(
    result: Result<Vec<u8>, Refusal>,
    content_type: &'static str,
) -> Response<Full<Bytes>> {
    let (status, bytes) = match result {
        Ok(bytes) => (StatusCode::OK, bytes),
        Err(error) => (
            match error {
                Refusal::Credential => StatusCode::UNAUTHORIZED,
                Refusal::Origin | Refusal::Binding => StatusCode::FORBIDDEN,
                Refusal::Size => StatusCode::PAYLOAD_TOO_LARGE,
                Refusal::Replay | Refusal::Conflict => StatusCode::CONFLICT,
                Refusal::Decision => StatusCode::UNPROCESSABLE_ENTITY,
                Refusal::Unavailable | Refusal::Clock => StatusCode::SERVICE_UNAVAILABLE,
                Refusal::Malformed => StatusCode::BAD_REQUEST,
            },
            format!("{error:?}").into_bytes(),
        ),
    };
    response(status, bytes, content_type)
}
fn response(
    status: StatusCode,
    bytes: Vec<u8>,
    content_type: &'static str,
) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(bytes)));
    *response.status_mut() = status;
    for (name, value) in [
        ("content-type", content_type),
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; worker-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        ),
    ] {
        response
            .headers_mut()
            .insert(name, hyper::header::HeaderValue::from_static(value));
    }
    response
}

async fn admit(config: &Config, request: Request<Incoming>) -> Result<Command, Refusal> {
    if request.method() != Method::POST
        || !["/command", "/preview"].contains(&request.uri().path())
        || request.uri().query().is_some()
        || request.uri().authority().is_some()
    {
        return Err(Refusal::Malformed);
    }
    headers(config, &request)?;
    let authenticated = authenticate(config, &request)?;
    let body = Limited::new(request.into_body(), MAX_BODY)
        .collect()
        .await
        .map_err(|error| {
            if error.is::<http_body_util::LengthLimitError>() {
                Refusal::Size
            } else {
                Refusal::Malformed
            }
        })?
        .to_bytes();
    if body.len() < 74 || body[0] != 1 {
        return Err(Refusal::Malformed);
    }
    let array = |range: std::ops::Range<usize>| {
        <[u8; 32]>::try_from(&body[range]).map_err(|_| Refusal::Malformed)
    };
    let contract = Hash32::new(array(1..33)?);
    if contract != config.contract {
        return Err(Refusal::Binding);
    }
    let key = Hash32::new(array(33..65)?);
    let expected = u64::from_be_bytes(body[65..73].try_into().map_err(|_| Refusal::Malformed)?);
    Ok(Command {
        principal: authenticated,
        contract,
        key,
        expected,
        bytes: body[73..].to_vec(),
    })
}

fn empty_body(request: &Request<Incoming>) -> Result<(), Refusal> {
    if request.headers().contains_key("transfer-encoding")
        || request
            .headers()
            .get("content-length")
            .is_some_and(|v| v.as_bytes() != b"0")
    {
        return Err(Refusal::Malformed);
    }
    Ok(())
}

fn headers(config: &Config, request: &Request<Incoming>) -> Result<(), Refusal> {
    if request.uri().query().is_some() || request.uri().authority().is_some() {
        return Err(Refusal::Malformed);
    }
    let headers = request.headers();
    for name in headers.keys() {
        if headers.get_all(name).iter().count() != 1 {
            return Err(Refusal::Malformed);
        }
        if headers
            .get(name)
            .is_none_or(|value| value.to_str().is_err())
        {
            return Err(Refusal::Malformed);
        }
    }
    if headers.get("host").is_none() {
        return Err(Refusal::Malformed);
    }
    // Only chunked framing is read. hyper accepts another coding before it, such
    // as `gzip, chunked`, and would hand over the still-coded bytes as the body.
    if headers
        .get("transfer-encoding")
        .is_some_and(|value| !value.as_bytes().eq_ignore_ascii_case(b"chunked"))
    {
        return Err(Refusal::Malformed);
    }
    if headers
        .get("origin")
        .is_some_and(|origin| origin.as_bytes() != config.origin.as_bytes())
    {
        return Err(Refusal::Origin);
    }
    if headers
        .get("sec-fetch-site")
        .is_some_and(|value| value.as_bytes() == b"cross-site")
    {
        return Err(Refusal::Origin);
    }
    Ok(())
}

// The presented credential alone selects the principal; no body field or other
// header can choose one.
fn authenticate(config: &Config, request: &Request<Incoming>) -> Result<Hash32, Refusal> {
    let credential = request
        .headers()
        .get("authorization")
        .ok_or(Refusal::Credential)?
        .to_str()
        .map_err(|_| Refusal::Credential)?;
    let hex = credential
        .strip_prefix("Bearer ")
        .filter(|hex| hex.len() == 64)
        .ok_or(Refusal::Credential)?;
    let mut candidate = [0; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        let digit = |byte| match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            _ => Err(Refusal::Credential),
        };
        candidate[index] = digit(pair[0])? * 16 + digit(pair[1])?;
    }
    matched_principal(&candidate, &config.parties).ok_or(Refusal::Credential)
}

// Source-checked all-entry scan. Every configured credential is compared at full
// width, with no early exit, and the matched principal is selected by mask.
// Configuration refuses duplicate credentials, so at most one entry matches.
// A reviewed source rule, not a hardware side-channel proof.
fn matched_principal(candidate: &[u8; 32], parties: &[([u8; 32], Hash32)]) -> Option<Hash32> {
    let mut found = 0u8;
    let mut principal = [0u8; 32];
    for (secret, owner) in parties {
        #[cfg(test)]
        tests::VISITS.with(|visits| visits.set(visits.get() + 1));
        let mask = 0u8.wrapping_sub(u8::from(std::hint::black_box(credential_matches(
            candidate, secret,
        ))));
        found |= mask;
        for (byte, owned) in principal.iter_mut().zip(owner.as_bytes()) {
            *byte |= owned & mask;
        }
    }
    (found != 0).then_some(Hash32::new(principal))
}

// Source-checked full-width comparison. black_box prevents short-circuit folding;
// this is a reviewed source rule, not a hardware side-channel proof.
fn credential_matches(candidate: &[u8; 32], configured: &[u8; 32]) -> bool {
    let mut difference = 0u8;
    for index in 0..32 {
        difference |= std::hint::black_box(candidate[index] ^ configured[index]);
    }
    difference == 0
}

// hyper normalizes equal duplicate Content-Length fields and removes that field
// when Transfer-Encoding is present. Refuse those raw forms before normalization.
// This bounded prefix guard does not interpret HTTP body framing.
struct HeaderGuard<I> {
    inner: I,
    prefix: Vec<u8>,
    position: usize,
    checked: bool,
}
impl<I> HeaderGuard<I> {
    fn new(inner: I) -> Self {
        Self {
            inner,
            prefix: Vec::new(),
            position: 0,
            checked: false,
        }
    }
}
fn ambiguous_lengths(header: &[u8]) -> bool {
    let mut lengths = 0;
    let mut transfer = false;
    for line in header.split(|byte| *byte == b'\n').skip(1) {
        if let Some(colon) = line.iter().position(|byte| *byte == b':') {
            let name = &line[..colon];
            if name.eq_ignore_ascii_case(b"content-length") {
                lengths += 1;
            }
            if name.eq_ignore_ascii_case(b"transfer-encoding") {
                transfer = true;
            }
        }
    }
    lengths > 1 || (lengths != 0 && transfer)
}
impl<I: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for HeaderGuard<I> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        output: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        use std::{pin::Pin, task::Poll};
        let this = self.get_mut();
        while !this.checked {
            if let Some(end) = this
                .prefix
                .windows(4)
                .position(|bytes| bytes == b"\r\n\r\n")
            {
                if end + 4 > MAX_HEADER || ambiguous_lengths(&this.prefix[..end + 4]) {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "refused headers",
                    )));
                }
                this.checked = true;
                break;
            }
            if this.prefix.len() >= MAX_HEADER {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "header limit",
                )));
            }
            let mut scratch = [0; 1024];
            let available = scratch.len().min(MAX_HEADER - this.prefix.len());
            let mut buffer = tokio::io::ReadBuf::new(&mut scratch[..available]);
            match Pin::new(&mut this.inner).poll_read(cx, &mut buffer) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) if buffer.filled().is_empty() => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "incomplete headers",
                    )));
                }
                Poll::Ready(Ok(())) => this.prefix.extend_from_slice(buffer.filled()),
            }
        }
        if this.position < this.prefix.len() {
            let count = output.remaining().min(this.prefix.len() - this.position);
            output.put_slice(&this.prefix[this.position..this.position + count]);
            this.position += count;
            if this.position == this.prefix.len() {
                this.prefix.clear();
                this.position = 0;
            }
            Poll::Ready(Ok(()))
        } else {
            Pin::new(&mut this.inner).poll_read(cx, output)
        }
    }
}
impl<I: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for HeaderGuard<I> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_write(cx, bytes)
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    thread_local! {
        // Test-only count of configured entries the credential scan compares.
        pub(super) static VISITS: Cell<usize> = const { Cell::new(0) };
    }

    #[test]
    fn credential_scan_visits_every_entry() {
        let parties: Vec<_> = (1..=5u8)
            .map(|n| ([n; 32], Hash32::new([n + 100; 32])))
            .collect();
        for (candidate, expected) in [
            ([1; 32], Some(Hash32::new([101; 32]))),
            ([3; 32], Some(Hash32::new([103; 32]))),
            ([5; 32], Some(Hash32::new([105; 32]))),
            ([9; 32], None),
        ] {
            VISITS.with(|visits| visits.set(0));
            assert_eq!(
                matched_principal(&candidate, &parties),
                expected,
                "wrong party selected"
            );
            assert_eq!(
                VISITS.with(Cell::get),
                parties.len(),
                "credential scan stopped early"
            );
        }
    }
}
