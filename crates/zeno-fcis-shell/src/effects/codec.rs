//! Canonical bytes of requests, responses and bindings.
//!
//! Every value is length-framed in declaration order, so equal bytes mean
//! equal values. Transcripts hash these bytes; replay compares them.

use alloc::vec::Vec;

use zeno_fcis_codec::Hash32;

use super::driver::Binding;
use super::port::{Operation, Outcome, Response, Sent, Store, Val, WaitError};

/// Canonical bytes of a request.
#[must_use]
pub fn operation(operation: &Operation) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(operation.port().tag());
    value(&mut out, &operation.value());
    out
}

/// Canonical bytes of a response, including its failure branch.
#[must_use]
pub fn response(response: &Response) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(response.port().tag());
    match response {
        Response::ReadState(outcome) | Response::Reopen(outcome) => branch(&mut out, outcome),
        Response::Commit(outcome) => branch(&mut out, outcome),
        Response::ReadHistory(outcome) => branch(&mut out, outcome),
        Response::ReadOutbox(outcome) => branch(&mut out, outcome),
        Response::Acknowledge(outcome) => branch(&mut out, outcome),
        Response::Send(Sent::Sent(_)) => out.push(0),
        Response::Send(Sent::NotSent(code)) => out.extend_from_slice(&[1, *code as u8]),
        Response::Send(Sent::Unknown(fault)) => out.extend_from_slice(&[2, *fault as u8]),
        Response::Clock(Ok(_)) | Response::Random(Ok(_)) | Response::Wait(Ok(_)) => out.push(0),
        Response::Clock(Err(fault)) | Response::Random(Err(fault)) => {
            out.extend_from_slice(&[1, *fault as u8]);
        }
        Response::Wait(Err(WaitError::Interrupted)) => out.push(2),
        Response::Wait(Err(WaitError::Failed(fault))) => out.extend_from_slice(&[1, *fault as u8]),
    }
    if let Some(payload) = response.value() {
        value(&mut out, &payload);
    }
    out
}

/// Canonical bytes of a run's outcome with a `u64` or no payload.
#[must_use]
pub fn outcome<T>(outcome: &Outcome<T>, payload: Option<u64>) -> Vec<u8> {
    let mut out = Vec::new();
    branch(&mut out, outcome);
    if let (Outcome::Ok(_), Some(payload)) = (outcome, payload) {
        out.extend_from_slice(&payload.to_be_bytes());
    }
    out
}

/// Canonical bytes of a binding.
#[must_use]
pub fn binding(binding: &Binding) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&binding.id.epoch.to_be_bytes());
    out.extend_from_slice(&binding.id.ordinal.to_be_bytes());
    out.push(binding.port.tag());
    match binding.store {
        Some(store) => {
            out.push(1);
            store_bytes(&mut out, store);
        }
        None => out.push(0),
    }
    bytes(&mut out, &binding.subject);
    out
}

/// Appends length-framed bytes.
pub fn bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value);
}

/// Appends a hash.
pub fn hash(out: &mut Vec<u8>, value: Hash32) {
    out.extend_from_slice(value.as_bytes());
}

fn branch<T>(out: &mut Vec<u8>, outcome: &Outcome<T>) {
    match outcome {
        Outcome::Ok(_) => out.push(0),
        Outcome::Refused(code) => out.extend_from_slice(&[1, *code as u8]),
        Outcome::NotApplied(fault) => out.extend_from_slice(&[2, *fault as u8]),
        Outcome::Unknown(fault) => out.extend_from_slice(&[3, *fault as u8]),
    }
}

fn store_bytes(out: &mut Vec<u8>, store: Store) {
    hash(out, store.identity);
    out.extend_from_slice(&store.handle.to_be_bytes());
}

fn value(out: &mut Vec<u8>, node: &Val<'_>) {
    match node {
        Val::Unit => out.push(0),
        Val::Store(store) => {
            out.push(1);
            store_bytes(out, *store);
        }
        Val::Hash(value) => {
            out.push(2);
            hash(out, *value);
        }
        Val::Bytes(value) => {
            out.push(3);
            bytes(out, value);
        }
        Val::Int(number) => {
            out.push(4);
            out.extend_from_slice(&number.to_be_bytes());
        }
        Val::Record(items) | Val::List(items) => {
            out.push(if matches!(node, Val::Record(_)) { 5 } else { 7 });
            out.extend_from_slice(&(items.len() as u64).to_be_bytes());
            for item in items {
                value(out, item);
            }
        }
        Val::Choice(index, payload) => {
            out.push(6);
            out.extend_from_slice(&(*index as u64).to_be_bytes());
            value(out, payload);
        }
    }
}
