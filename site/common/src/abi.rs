//! Bounded, scalar browser API. See `site/ABI.md` for version 2's protocol.
//!
//! All buffers stay inside Rust, and the published Wasm module keeps its
//! linear memory private. Calls exchange lengths and little-endian words;
//! no caller owns an allocation or supplies an address.

use crate::application::Application;
use crate::demo::{Demo, DemoInstance, Refusal, Stage};
use serde_json::Value as Json;
use std::io::{self, Write};
use std::sync::Mutex;

/// Maximum UTF-8 request size in bytes.
pub const MAX_REQUEST_BYTES: usize = 4_096;
/// Maximum serialized response size in bytes.
pub const MAX_RESPONSE_BYTES: usize = 131_072;
/// Maximum complete UTF-8 requests delivered to a demo between resets.
pub const MAX_REQUESTS: u32 = 64;

const REPORT_TOO_LARGE: &[u8] = br#"{"error":"report capacity exceeded; a decision may have executed; reset required","stage":"transport"}"#;

struct Response {
    bytes: [u8; MAX_RESPONSE_BYTES],
    len: usize,
}

impl Write for Response {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self.bytes.len() - self.len;
        if bytes.len() > remaining {
            return Err(io::Error::other("response capacity exceeded"));
        }
        let end = self.len + bytes.len();
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Boundary {
    input: [u8; MAX_REQUEST_BYTES],
    expected: Option<usize>,
    written: usize,
    response: Response,
    requests: u32,
    demo: Option<Box<dyn DemoInstance + Send>>,
}

impl Boundary {
    // Production uses this only to initialize static storage at compile time.
    // Tests instantiate a local boundary so they never share mutable sessions.
    #[allow(clippy::large_stack_arrays)]
    const fn new() -> Self {
        Self {
            input: [0; MAX_REQUEST_BYTES],
            expected: None,
            written: 0,
            response: Response {
                bytes: [0; MAX_RESPONSE_BYTES],
                len: 0,
            },
            requests: 0,
            demo: None,
        }
    }

    fn restart(&mut self) {
        self.expected = None;
        self.written = 0;
        self.response.len = 0;
        self.requests = 0;
        self.demo = None;
    }

    fn begin(&mut self, length: u32) -> u32 {
        self.expected = None;
        self.written = 0;
        self.response.len = 0;
        let Ok(length) = usize::try_from(length) else {
            return 0;
        };
        if length == 0 || length > self.input.len() {
            return 0;
        }
        self.expected = Some(length);
        1
    }

    fn write(&mut self, word: u32) -> u32 {
        let Some(expected) = self.expected else {
            return 0;
        };
        if self.written >= expected {
            self.expected = None;
            return 0;
        }
        let count = (expected - self.written).min(4);
        self.input[self.written..self.written + count]
            .copy_from_slice(&word.to_le_bytes()[..count]);
        self.written += count;
        1
    }

    fn respond(&mut self, result: Result<Json, Refusal>) -> u32 {
        self.response.len = 0;
        let value = result.unwrap_or_else(|refusal| refusal.to_json());
        if serde_json::to_writer(&mut self.response, &value).is_err() {
            // A decision may already have run. Retire the session and report
            // a transport error, never an input refusal or a claimed rollback.
            self.demo = None;
            self.expected = None;
            self.response.len = 0;
            if self.response.write_all(REPORT_TOO_LARGE).is_err() {
                return 0;
            }
        }
        u32::try_from(self.response.len).unwrap_or(0)
    }

    fn step(&mut self) -> u32 {
        let result = self.decide();
        self.respond(result)
    }

    fn decide(&mut self) -> Result<Json, Refusal> {
        let length = self
            .expected
            .take()
            .filter(|length| *length == self.written)
            .ok_or_else(|| input_refusal("provide one complete request before stepping"))?;
        let input = std::str::from_utf8(&self.input[..length])
            .map_err(|_| input_refusal("input is not UTF-8"))?;
        let demo = self.demo.as_mut().ok_or_else(not_built)?;
        if self.requests >= MAX_REQUESTS {
            return Err(input_refusal(
                "session limit reached; reset after 64 requests",
            ));
        }
        self.requests += 1;
        demo.step(input)
    }

    fn state(&mut self) -> u32 {
        self.expected = None;
        let result = self
            .demo
            .as_ref()
            .ok_or_else(not_built)
            .and_then(|demo| demo.state());
        self.respond(result)
    }

    fn read(&self, offset: u32) -> u32 {
        let Ok(start) = usize::try_from(offset) else {
            return 0;
        };
        if start >= self.response.len {
            return 0;
        }
        let count = (self.response.len - start).min(4);
        let mut word = [0; 4];
        word[..count].copy_from_slice(&self.response.bytes[start..start + count]);
        u32::from_le_bytes(word)
    }
}

static BOUNDARY: Mutex<Boundary> = Mutex::new(Boundary::new());

fn with_boundary(action: impl FnOnce(&mut Boundary) -> u32) -> u32 {
    // Reentry or poison must not block or resume a possibly partial session.
    BOUNDARY
        .try_lock()
        .map_or(0, |mut boundary| action(&mut boundary))
}

fn input_refusal(message: &str) -> Refusal {
    Refusal::new(Stage::Input, message)
}

fn not_built() -> Refusal {
    input_refusal("call demo_reset first")
}

/// Drops the old session, builds `A` at its exact genesis, and returns the
/// state's JSON byte length. Each module's `demo_reset` calls this.
#[must_use]
pub fn reset<A: Application>() -> u32
where
    Demo<A>: Send,
{
    with_boundary(|boundary| {
        boundary.restart();
        let result = Demo::<A>::new().and_then(|demo| {
            let state = demo.state()?;
            boundary.demo = Some(Box::new(demo));
            Ok(state)
        });
        boundary.respond(result)
    })
}

/// Returns the scalar API's version.
// SAFETY: the demo_ prefix reserves unique exported symbols in each module.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn demo_abi_version() -> u32 {
    2
}

/// Starts a request of 1 to 4,096 bytes; returns 1 on success, otherwise 0.
// SAFETY: the demo_ prefix reserves unique exported symbols in each module.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn demo_begin(length: u32) -> u32 {
    with_boundary(|boundary| boundary.begin(length))
}

/// Appends up to four little-endian bytes; returns 1 on success, otherwise 0.
// SAFETY: the demo_ prefix reserves unique exported symbols in each module.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn demo_write(word: u32) -> u32 {
    with_boundary(|boundary| boundary.write(word))
}

/// Consumes the completed request and returns the reply's JSON byte length.
// SAFETY: the demo_ prefix reserves unique exported symbols in each module.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn demo_step() -> u32 {
    with_boundary(Boundary::step)
}

/// Discards pending input and returns the current state's JSON byte length.
// SAFETY: the demo_ prefix reserves unique exported symbols in each module.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn demo_state() -> u32 {
    with_boundary(Boundary::state)
}

/// Reads up to four little-endian response bytes, padded with zeros.
/// An offset outside the latest reply returns zero.
// SAFETY: the demo_ prefix reserves unique exported symbols in each module.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn demo_read(offset: u32) -> u32 {
    with_boundary(|boundary| boundary.read(offset))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Echo {
        steps: u32,
    }

    impl DemoInstance for Echo {
        fn step(&mut self, input: &str) -> Result<Json, Refusal> {
            self.steps += 1;
            Ok(json!({ "input": input, "steps": self.steps }))
        }
        fn state(&self) -> Result<Json, Refusal> {
            Ok(json!({ "steps": self.steps }))
        }
    }

    fn fresh() -> Boundary {
        let mut boundary = Boundary::new();
        boundary.demo = Some(Box::new(Echo { steps: 0 }));
        boundary
    }

    fn put(boundary: &mut Boundary, bytes: &[u8]) {
        assert_eq!(boundary.begin(u32::try_from(bytes.len()).unwrap()), 1);
        for chunk in bytes.chunks(4) {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            assert_eq!(boundary.write(u32::from_le_bytes(word)), 1);
        }
    }

    fn take(boundary: &Boundary, length: u32) -> Json {
        let bytes: Vec<_> = (0..length)
            .step_by(4)
            .flat_map(|offset| boundary.read(offset).to_le_bytes())
            .take(length as usize)
            .collect();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn requests_are_complete_consumed_once_and_independent_of_previous_bytes() {
        let mut boundary = fresh();
        for text in ["abcd", "x", "é", "hello"] {
            put(&mut boundary, text.as_bytes());
            let length = boundary.step();
            assert_eq!(take(&boundary, length)["input"], text);
            let length = boundary.step();
            assert!(take(&boundary, length)["error"].is_string());
        }
        put(&mut boundary, b"incomplete");
        assert_eq!(boundary.begin(5), 1);
        assert_eq!(boundary.write(0), 1);
        let length = boundary.step();
        assert!(take(&boundary, length)["error"].is_string());
        let length = boundary.state();
        assert_eq!(take(&boundary, length)["steps"], 4);
    }

    #[test]
    fn lengths_words_and_utf8_are_checked_before_execution() {
        let mut boundary = fresh();
        for length in [0, 4_097, u32::MAX] {
            assert_eq!(boundary.begin(length), 0);
            assert_eq!(boundary.write(0), 0);
        }
        put(&mut boundary, b"a");
        assert_eq!(boundary.write(0), 0);
        let length = boundary.step();
        assert!(take(&boundary, length)["error"].is_string());
        put(&mut boundary, &[0xff]);
        let length = boundary.step();
        assert_eq!(take(&boundary, length)["error"], "input is not UTF-8");
        put(&mut boundary, b"discard on state");
        let length = boundary.state();
        assert_eq!(take(&boundary, length)["steps"], 0);
        let length = boundary.step();
        assert!(take(&boundary, length)["error"].is_string());
        put(&mut boundary, &[b'a'; MAX_REQUEST_BYTES]);
        let length = boundary.step();
        assert_eq!(
            take(&boundary, length)["input"].as_str().unwrap().len(),
            MAX_REQUEST_BYTES
        );
        assert_eq!(boundary.read(length), 0);
        assert_eq!(boundary.read(u32::MAX), 0);
    }

    #[test]
    fn session_capacity_refuses_before_execution_and_reset_reclaims_it() {
        let mut boundary = fresh();
        for _ in 0..MAX_REQUESTS {
            put(&mut boundary, b"x");
            boundary.step();
        }
        put(&mut boundary, b"x");
        let length = boundary.step();
        assert!(
            take(&boundary, length)["error"]
                .as_str()
                .unwrap()
                .contains("session limit")
        );
        let length = boundary.state();
        assert_eq!(take(&boundary, length)["steps"], MAX_REQUESTS);
        boundary.restart();
        assert_eq!(boundary.requests, 0);
        assert_eq!(boundary.read(0), 0);
        let length = boundary.state();
        assert_eq!(take(&boundary, length)["error"], "call demo_reset first");
        boundary.demo = Some(Box::new(Echo { steps: 0 }));
        put(&mut boundary, b"x");
        let length = boundary.step();
        assert_eq!(take(&boundary, length)["steps"], 1);
    }

    #[test]
    fn response_capacity_retires_the_session_without_claiming_rollback() {
        let mut boundary = fresh();
        let length = boundary.respond(Ok(json!("x".repeat(MAX_RESPONSE_BYTES))));
        let report = take(&boundary, length);
        assert_eq!(report["stage"], "transport");
        assert!(
            report["error"]
                .as_str()
                .unwrap()
                .contains("may have executed")
        );
        assert!(boundary.demo.is_none());
        assert_eq!(boundary.read(length), 0);
    }
}
