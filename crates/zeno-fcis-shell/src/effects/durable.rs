//! Write-ahead reservations and pure, source-bound restoration.
//! Store durability and truthful complete-call invocation reports are premises.

use alloc::{vec, vec::Vec};
use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};

use super::driver::Binding;
use super::handler::{Handled, Invocation, Terminal};
use super::policy::{self, Monitor, Policy, Verdict};
use super::port::{Delivery, Operation, Outcome, Response, Sent, Store};
use super::program::{Program, Step};
use super::record::{self, Ended, Entry, Implementation, Manifest, ReplayError, Transcript};
use super::{codec, policy_record, registry};

/// Maximum checkpoint size, including framing and checksum.
pub const MAX_BYTES: usize = 1_048_576;
const MAGIC: &[u8] = b"ZFEFD001";

/// Fail-closed restoration or persistence failure. Drop/reopen after write errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// The store failed; a write may or may not have committed.
    Storage,
    /// No existing checkpoint; opening never initializes one.
    Missing,
    /// Malformed, noncanonical or checksum-invalid bytes.
    Corrupt,
    /// Source, scope, configuration or request binding differs.
    Binding,
    /// Checkpoint size or policy count exceeds the declared limit.
    Capacity,
    /// Another writer replaced the expected checkpoint.
    Conflict,
    /// This handle needs restoration after a failure or unwind.
    Stopped,
}

/// Opaque byte storage, not policy or publication authority.
pub trait StateStore {
    /// Read the current durable checkpoint; absence is an error.
    fn load(&mut self) -> Result<Vec<u8>, Error>;
    /// Atomically replace exactly `expected`; success means durable commit.
    /// On error the caller must not forward, even if the write committed.
    fn commit(&mut self, expected: &[u8], next: &[u8]) -> Result<(), Error>;
}

/// Pins the restore/ordering implementation and unchanged policy source closure.
#[must_use]
pub fn source() -> Hash32 {
    registry::source_digest(&[
        ("durable.rs", include_bytes!("durable.rs")),
        ("policy-source", policy::source().as_bytes()),
    ])
}

fn checkpoint(
    scope: Hash32,
    monitors: &[Monitor],
    release: Option<Hash32>,
) -> Result<Vec<u8>, Error> {
    let mut out = MAGIC.to_vec();
    codec::hash(&mut out, source());
    codec::hash(&mut out, scope);
    // Tag 1 records Suspend + complete-call NotInvoked for this last key.
    out.push(u8::from(release.is_some()));
    if let Some(key) = release {
        codec::hash(&mut out, key);
    }
    out.extend_from_slice(&(monitors.len() as u64).to_be_bytes());
    for monitor in monitors {
        codec::hash(&mut out, monitor.identity());
        codec::bytes(&mut out, &monitor.state.bytes());
    }
    let checksum = RustCryptoSha256::hash(&out);
    codec::hash(&mut out, checksum);
    if out.len() > MAX_BYTES {
        return Err(Error::Capacity);
    }
    Ok(out)
}

fn take<'a>(bytes: &mut &'a [u8], count: usize) -> Result<&'a [u8], Error> {
    let (head, tail) = bytes.split_at_checked(count).ok_or(Error::Corrupt)?;
    *bytes = tail;
    Ok(head)
}

fn number(bytes: &mut &[u8]) -> Result<usize, Error> {
    let raw = take(bytes, 8)?.try_into().map_err(|_| Error::Corrupt)?;
    usize::try_from(u64::from_be_bytes(raw)).map_err(|_| Error::Capacity)
}

fn hash(bytes: &mut &[u8]) -> Result<Hash32, Error> {
    Ok(Hash32::new(
        take(bytes, 32)?.try_into().map_err(|_| Error::Corrupt)?,
    ))
}

// AtMostOne declares only this key projection. This never reaches a terminal.
fn key_operation(scope: Hash32, id: Hash32) -> Operation {
    Operation::Send(
        Store {
            identity: scope,
            handle: 0,
        },
        Delivery {
            id,
            version: 0,
            lane: 0,
            ordinal: 0,
            destination: Vec::new(),
            payload: Vec::new(),
            payload_sha256: Hash32::ZERO,
        },
    )
}

/// Pure validator: immutable expected configuration and bytes, owned monitors.
/// A release record retains the reserved snapshot and certifies its final key.
/// Checksums detect corruption, not coherent forgery or rollback of the store.
pub fn restore(scope: Hash32, policies: &[Policy], bytes: &[u8]) -> Result<Vec<Monitor>, Error> {
    if bytes.len() > MAX_BYTES || policies.len() > 64 {
        return Err(Error::Capacity);
    }
    if !policies
        .iter()
        .any(|p| matches!(p, Policy::AtMostOne { .. }))
    {
        return Err(Error::Binding);
    }
    let (body, checksum) = bytes
        .split_at_checked(bytes.len().checked_sub(32).ok_or(Error::Corrupt)?)
        .ok_or(Error::Corrupt)?;
    if RustCryptoSha256::hash(body).as_bytes() != checksum {
        return Err(Error::Corrupt);
    }
    let mut input = body;
    if take(&mut input, MAGIC.len())? != MAGIC
        || hash(&mut input)? != source()
        || hash(&mut input)? != scope
    {
        return Err(Error::Binding);
    }
    let release = match take(&mut input, 1)? {
        [0] => None,
        [1] => Some(hash(&mut input)?),
        _ => return Err(Error::Corrupt),
    };
    if number(&mut input)? != policies.len() {
        return Err(Error::Binding);
    }
    let mut monitors = Vec::new();
    for policy in policies {
        let mut monitor = Monitor::new(policy.clone());
        if hash(&mut input)? != monitor.identity() {
            return Err(Error::Binding);
        }
        let size = number(&mut input)?;
        let state = take(&mut input, size)?;
        let mut keys = state;
        let count = number(&mut keys)?;
        if count.checked_mul(64) != Some(keys.len()) {
            return Err(Error::Corrupt);
        }
        let guard = matches!(policy, Policy::AtMostOne { .. });
        if !guard && count != 0 || guard && release.is_some() && count == 0 {
            return Err(Error::Corrupt);
        }
        let mut preceding = monitor.state.clone();
        for index in 0..count {
            let store = hash(&mut keys)?;
            let key = hash(&mut keys)?;
            if store != scope {
                return Err(Error::Binding);
            }
            preceding = monitor.state.clone();
            let (next, verdict) =
                policy::step(policy, &monitor.state, &key_operation(store, key), None);
            if verdict != Verdict::Allow {
                return Err(Error::Corrupt);
            }
            monitor.state = next;
            if index + 1 == count && release.is_some_and(|released| released != key) {
                return Err(Error::Corrupt);
            }
        }
        if monitor.state.bytes() != state {
            return Err(Error::Corrupt);
        }
        if guard && release.is_some() {
            monitor.state = preceding;
        }
        monitors.push(monitor);
    }
    if !input.is_empty() {
        return Err(Error::Corrupt);
    }
    Ok(monitors)
}

/// Explicit genesis bytes for a NEW reservation lifetime, never recovery.
pub fn initial(scope: Hash32, policies: &[Policy]) -> Result<Vec<u8>, Error> {
    let monitors: Vec<_> = policies.iter().cloned().map(Monitor::new).collect();
    let bytes = checkpoint(scope, &monitors, None)?;
    restore(scope, policies, &bytes)?;
    Ok(bytes)
}

/// Exclusively owned shell handle; no mutable monitor or transcript aliases.
pub struct Relay<S: StateStore> {
    store: S,
    scope: Hash32,
    monitors: Vec<Monitor>,
    stored: Vec<u8>,
    transcript: Transcript,
    stopped: bool,
}

impl<S: StateStore> Relay<S> {
    /// Restore an existing lifetime and record its exact checkpoint in the manifest.
    /// The caller supplies the ordinary terminal provenance and program start.
    pub fn open(
        mut store: S,
        scope: Hash32,
        policies: &[Policy],
        mut manifest: Manifest,
    ) -> Result<Self, Error> {
        let stored = store.load()?;
        let monitors = restore(scope, policies, &stored)?;
        if !manifest.provenance_complete()
            || manifest
                .stack
                .iter()
                .any(|name| matches!(*name, "policy" | "durable"))
        {
            return Err(Error::Binding);
        }
        let mut configuration = MAGIC.to_vec();
        codec::bytes(&mut configuration, &manifest.configuration);
        codec::bytes(&mut configuration, &stored);
        manifest.configuration = configuration;
        manifest.policies = monitors.clone();
        manifest.stack.splice(0..0, vec!["policy"; monitors.len()]);
        manifest.stack.push("durable");
        manifest.implementations.push(Implementation {
            name: "durable",
            source: source(),
        });
        Ok(Self {
            store,
            scope,
            monitors,
            stored,
            transcript: Transcript::new(manifest),
            stopped: false,
        })
    }

    /// Admit purely, commit reservations, then forward unchanged. A failed commit
    /// or unwind poisons the handle; only restoration can resolve uncertain writes.
    pub fn handle(
        &mut self,
        terminal: &mut dyn Terminal,
        binding: &Binding,
        operation: &Operation,
    ) -> Result<Handled, Error> {
        if self.stopped {
            return Err(Error::Stopped);
        }
        if Binding::of(binding.id, operation) != *binding
            || operation.store().is_some_and(|s| s.identity != self.scope)
        {
            return Err(Error::Binding);
        }
        let previous = self.monitors.clone();
        let (next, check) = policy_record::admit(&previous, operation);
        let handled = if let Some(code) = check.rejection() {
            Handled::Answer(Response::Send(Sent::Refused(code)))
        } else {
            self.stopped = true;
            if matches!(operation, Operation::Send(..)) {
                let reserved = checkpoint(self.scope, &next, None)?;
                self.store.commit(&self.stored, &reserved)?;
                self.stored = reserved;
            }
            self.monitors = next;
            let report = terminal.handle_with_invocation(binding, operation);
            if report.outcome == Handled::Suspend && report.invocation == Invocation::NotInvoked {
                if let Operation::Send(_, delivery) = operation {
                    let released = checkpoint(self.scope, &self.monitors, Some(delivery.id))?;
                    self.store.commit(&self.stored, &released)?;
                    self.stored = released;
                }
                self.monitors = previous;
            }
            self.stopped = report.outcome == Handled::Crash;
            report.outcome
        };
        let entry = match &handled {
            Handled::Answer(response) => Some(Entry::Response(binding.clone(), response.clone())),
            Handled::Crash => Some(Entry::Crash(binding.clone())),
            Handled::Suspend => None,
        };
        if let Some(entry) = entry {
            self.transcript
                .append(Entry::Request(binding.clone(), operation.clone()));
            self.transcript
                .append(Entry::Policy(binding.clone(), check));
            self.transcript.append(entry);
        }
        Ok(handled)
    }

    /// Record a completed program outcome; this conveys no publication authority.
    pub fn done(&mut self, outcome: &[u8]) {
        self.transcript.append(Entry::Done(outcome.to_vec()));
    }

    /// Private in-memory evidence, shared immutably.
    #[must_use]
    pub const fn transcript(&self) -> &Transcript {
        &self.transcript
    }
}

/// Replay restoration against expected scope/configuration, then the pure program.
/// Ordinary replay alone does not qualify durable restoration evidence.
pub fn replay<P: Program>(
    transcript: &Transcript,
    expected: (Hash32, &[Policy]),
    start: &[u8],
    epoch: u64,
    first: Step<P::Continuation, P::Output>,
    done: fn(&Outcome<P::Output>) -> Vec<u8>,
) -> Result<Ended, ReplayError> {
    let validate = || -> Result<(), Error> {
        let manifest = &transcript.manifest;
        let mut input = manifest.configuration.as_slice();
        if take(&mut input, MAGIC.len())? != MAGIC {
            return Err(Error::Binding);
        }
        let original_size = number(&mut input)?;
        take(&mut input, original_size)?;
        let size = number(&mut input)?;
        let stored = take(&mut input, size)?;
        if !input.is_empty()
            || restore(expected.0, expected.1, stored)? != manifest.policies
            || manifest
                .stack
                .iter()
                .filter(|name| **name == "durable")
                .count()
                != 1
            || !manifest
                .implementations
                .iter()
                .any(|i| i.name == "durable" && i.source == source())
        {
            return Err(Error::Binding);
        }
        Ok(())
    };
    validate().map_err(|_| ReplayError::Manifest)?;
    record::replay::<P>(transcript, start, epoch, first, done)
}

#[cfg(test)]
#[path = "durable_explore.rs"]
mod explore;
#[cfg(test)]
#[path = "durable_tests.rs"]
mod tests;
