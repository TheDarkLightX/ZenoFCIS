//! Pure core of `zeno-fcis loop`: the bounded adaptive optimization loop of
//! [`docs/neurosymbolic-loop/specs`](../../../../docs/neurosymbolic-loop/specs/INDEX.md).
//!
//! A proposer (deterministic local rewriting, a fake provider in tests, an
//! agent driving the MCP tools, or a hosted model adapter that stays disabled)
//! suggests complete canonical candidate programs. Every candidate is judged
//! only by F3's [`crate::transform::check`]: a complete enumeration of the
//! original's declared input domain on the library's verified evaluator. The
//! loop keeps an incumbent that is either the admitted original or a checked
//! replacement built from a genuine [`crate::transform::Equivalence`]; nothing
//! a model, a user, a receipt file or a `passed` flag says can construct one.
//!
//! This module is pure: no I/O, clock, randomness or environment. Elapsed
//! time, persisted bytes, process limits and model calls belong to the shell
//! (`crate::loop_command`), which passes them in as data. Requirement IDs in
//! comments (`NSC`, `NSL`, `NSF`, `NSR`, `NSM`) name the specification each
//! piece implements.
//!
//! # Layout
//!
//! | Module | Contract |
//! | --- | --- |
//! | [`limits`] | compiled resource ceilings; lower operator limits win (NSR-001) |
//! | [`profiles`] | `FunctionalBoolV1` and `CheckedI64V1` artifact admission (NSC-002/003/005) |
//! | [`request`] | frozen canonical request and its `RequestId` (NSC-001/004/006/007/008) |
//! | [`incumbent`] | original-or-checked incumbent, costs and lexicographic selection (NSL-001/005/006) |
//! | [`ledger`] | append-only hash-chained reservation ledger and accounting (NSR-005/007/008) |
//! | [`feedback`] | replayed difference witnesses and typed feedback (NSF-004/005) |
//! | [`strategy`] | closed strategy DSL and the engine seam F4 wires into (NSL-003, NSM-004) |
//! | [`session`] | the attempt state machine, stop reasons, report and resume (NSL-002/004/007/008, NSR-009) |
//! | [`local`] | deterministic local proposer over the finite IR |
//!
//! # Strategy engine
//!
//! Strategy proposals (`Proposal::Strategy`) are validated data in F4's closed
//! DSL (phases of fixed rule IDs, bounded rounds, a fixed extractor). The shell
//! runs them through a [`strategy::StrategyEngine`], which the shell wires to
//! F4's e-graph optimizer (`loop_command::OptimizeEngine`). The candidate bytes
//! it emits enter the same admission and `transform::check` path as a whole
//! candidate. No model-supplied code, path or callback is executed anywhere.

pub(crate) mod feedback;
pub(crate) mod incumbent;
pub(crate) mod ledger;
pub(crate) mod limits;
pub(crate) mod local;
pub(crate) mod profiles;
pub(crate) mod program_json;
pub(crate) mod request;
pub(crate) mod session;
pub(crate) mod strategy;

use serde_json::Value;

/// Canonical bytes of a JSON value: compact, object keys in byte order, one
/// trailing newline. Identical to the transform receipt encoding.
pub(crate) fn canonical_json(value: &Value) -> Vec<u8> {
    let mut text = String::new();
    crate::transform::write_canonical(value, &mut text);
    text.push('\n');
    text.into_bytes()
}

/// Domain-separated SHA-256: `sha256(tag || 0x00 || bytes)`, lowercase hex.
/// The tag keeps a request identity, a ledger link and an artifact digest from
/// ever colliding by construction; collision resistance of SHA-256 remains an
/// explicit assumption (DESIGN, "Frozen request and small state").
pub(crate) fn tagged_sha256(tag: &str, bytes: &[u8]) -> String {
    let mut input = Vec::with_capacity(tag.len() + 1 + bytes.len());
    input.extend_from_slice(tag.as_bytes());
    input.push(0);
    input.extend_from_slice(bytes);
    crate::transform::sha256_hex(&input)
}

/// Reads a `u64` from a JSON field, refusing anything but an exact integer.
pub(crate) fn json_u64(value: &Value, field: &str) -> Option<u64> {
    value.get(field)?.as_u64()
}

/// Refuses an object with any key outside `allowed` (NSC-004, NSF-006: unknown
/// and trailing fields fail).
pub(crate) fn only_fields(value: &Value, allowed: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|map| map.keys().all(|key| allowed.contains(&key.as_str())))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
