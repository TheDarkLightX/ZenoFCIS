//! The C ABI the page calls.
//!
//! Buffers cross the boundary as a pointer and a length. A result is a buffer
//! that starts with its JSON's length as four little-endian bytes; the caller
//! reads it, then frees the whole buffer with [`demo_free`]. A refusal is
//! reported as `{"error": TEXT, "stage": STAGE}`; nothing here panics on any
//! input. This is the only module that handles raw pointers.

#![allow(unsafe_code)]

use crate::demo::{Demo, Refusal, Stage};
use serde_json::Value;
use std::sync::{Mutex, PoisonError};

/// The one demo the page drives; `None` until [`demo_reset`] builds it.
static DEMO: Mutex<Option<Demo>> = Mutex::new(None);

/// The report when a result's JSON could not be measured in 32 bits.
const REPORT_TOO_LARGE: &str = r#"{"error":"the report is too large","stage":"input"}"#;

fn with_demo<T>(action: impl FnOnce(&mut Option<Demo>) -> T) -> T {
    let mut slot = DEMO.lock().unwrap_or_else(PoisonError::into_inner);
    action(&mut slot)
}

fn not_built() -> Refusal {
    Refusal::new(Stage::Input, "call demo_reset first")
}

fn reset() -> Result<Value, Refusal> {
    with_demo(|slot| {
        let demo = Demo::new()?;
        let state = demo.state()?;
        *slot = Some(demo);
        Ok(state)
    })
}

fn step(input: &[u8]) -> Result<Value, Refusal> {
    let input = std::str::from_utf8(input)
        .map_err(|error| Refusal::new(Stage::Input, format!("input is not UTF-8: {error}")))?;
    with_demo(|slot| slot.as_mut().ok_or_else(not_built)?.step(input))
}

fn state() -> Result<Value, Refusal> {
    with_demo(|slot| slot.as_ref().ok_or_else(not_built)?.state())
}

/// Leaks a length-prefixed buffer holding the report or the refusal.
fn respond(result: Result<Value, Refusal>) -> *mut u8 {
    let text = match result {
        Ok(report) => report.to_string(),
        Err(refusal) => refusal.to_json().to_string(),
    };
    let text = match u32::try_from(text.len()) {
        Ok(_) => text,
        Err(_) => REPORT_TOO_LARGE.to_owned(),
    };
    let length = u32::try_from(text.len()).unwrap_or(0);
    let mut buffer = Vec::with_capacity(4 + text.len());
    buffer.extend_from_slice(&length.to_le_bytes());
    buffer.extend_from_slice(text.as_bytes());
    Box::leak(buffer.into_boxed_slice()).as_mut_ptr()
}

/// Allocates `len` bytes for the caller's input; free them with [`demo_free`].
/// A zero length yields a null pointer.
// SAFETY: the `demo_` prefix keeps every exported symbol unique.
#[unsafe(no_mangle)]
pub extern "C" fn demo_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    Box::leak(vec![0; len].into_boxed_slice()).as_mut_ptr()
}

/// Frees the `len` bytes at `ptr` that [`demo_alloc`] or a result returned.
///
/// # Safety
///
/// `ptr` and `len` must be a pair this module handed out, passed back once; a
/// null pointer or a zero length is ignored.
// SAFETY: the `demo_` prefix keeps every exported symbol unique.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn demo_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    // SAFETY: the caller returns a pointer and a length that this module
    // leaked from one boxed slice of exactly `len` bytes, and returns it once.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
}

/// Builds the demo at the exact genesis and returns its state.
// SAFETY: the `demo_` prefix keeps every exported symbol unique.
#[unsafe(no_mangle)]
pub extern "C" fn demo_reset() -> *mut u8 {
    respond(reset())
}

/// Decides the request in the `len` bytes of JSON at `ptr` and returns the
/// report.
///
/// # Safety
///
/// `ptr` must point to `len` bytes that stay readable and unchanged during the
/// call, such as a buffer from [`demo_alloc`]; a null pointer or a zero
/// length is an empty input, which is refused.
// SAFETY: the `demo_` prefix keeps every exported symbol unique.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn demo_step(ptr: *const u8, len: usize) -> *mut u8 {
    let input: &[u8] = if ptr.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: the caller guarantees `len` readable, unchanging bytes at `ptr`.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };
    respond(step(input))
}

/// Returns the current state.
// SAFETY: the `demo_` prefix keeps every exported symbol unique.
#[unsafe(no_mangle)]
pub extern "C" fn demo_state() -> *mut u8 {
    respond(state())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Reads a result as the page does, then frees it.
    fn take(pointer: *mut u8) -> Value {
        // SAFETY: `pointer` is a result this module leaked: four length bytes
        // followed by that many bytes of JSON, freed exactly once here.
        let text = unsafe {
            let length = u32::from_le_bytes(*pointer.cast::<[u8; 4]>());
            let bytes = std::slice::from_raw_parts(pointer.add(4), length as usize);
            let text = String::from_utf8(bytes.to_vec()).unwrap();
            demo_free(pointer, 4 + length as usize);
            text
        };
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn the_abi_round_trips_length_prefixed_json_and_refuses_bad_input() {
        assert_eq!(take(demo_state())["stage"], "input");
        assert_eq!(take(demo_reset())["steps"], 0);
        let input = br#"{"command":"LoginFailed","now":1000,"admin":false}"#;
        let pointer = demo_alloc(input.len());
        // SAFETY: `pointer` addresses `input.len()` writable bytes from `demo_alloc`,
        // read once by `demo_step` and freed once after it.
        let report = unsafe {
            std::ptr::copy_nonoverlapping(input.as_ptr(), pointer, input.len());
            let report = take(demo_step(pointer, input.len()));
            demo_free(pointer, input.len());
            report
        };
        assert_eq!(report["decision"], "CommittedFailure");
        assert_eq!(report["reason"]["id"], 203);
        // SAFETY: a null pointer with a zero length is documented as empty input.
        let refused = take(unsafe { demo_step(std::ptr::null(), 0) });
        assert_eq!(refused["stage"], "input");
        assert_eq!(take(demo_state())["steps"], 1);
        assert!(demo_alloc(0).is_null());
        // SAFETY: a null pointer is documented as ignored.
        unsafe { demo_free(std::ptr::null_mut(), 0) };
    }
}
