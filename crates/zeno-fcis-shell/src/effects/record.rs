//! Hash-linked transcripts and handler-free replay.
//!
//! The record layer appends every request, answer, crash and run outcome.
//! Each link holds the SHA-256 of the previous link, starting from the
//! manifest's, and of its own entry: integrity evidence, not a signature or
//! an attestation. Replay re-runs the pure program with no handler at all:
//! each request must equal the recorded one before the recorded answer is
//! returned, and anything missing, extra, reordered or changed refuses.

use alloc::vec::Vec;

use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};

use super::codec;
use super::driver::{Binding, Driver, Fatal, Reply};
use super::port::{Operation, Outcome, Response};
use super::program::{Program, Step};
use super::registry::{self, REGISTRY};

/// What a run is bound to before it starts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    /// Registry identity.
    pub registry: Hash32,
    /// Step-function source digest.
    pub program: Hash32,
    /// Layer and terminal names, innermost first.
    pub stack: Vec<&'static str>,
    /// The schedule: every simulated choice, in order.
    pub schedule: Vec<u8>,
    /// Digest of the initial world.
    pub initial: Hash32,
    /// The program, how it starts and its invocation, as canonical bytes.
    pub start: Vec<u8>,
}

impl Manifest {
    /// A manifest for this library's registry and step functions.
    #[must_use]
    pub fn current(
        stack: Vec<&'static str>,
        schedule: Vec<u8>,
        initial: Hash32,
        start: Vec<u8>,
    ) -> Self {
        Self {
            registry: registry::identity(&REGISTRY),
            program: registry::program_source(),
            stack,
            schedule,
            initial,
            start,
        }
    }

    /// SHA-256 of the manifest's canonical bytes.
    #[must_use]
    pub fn digest(&self) -> Hash32 {
        let mut out = Vec::new();
        codec::hash(&mut out, self.registry);
        codec::hash(&mut out, self.program);
        out.extend_from_slice(&(self.stack.len() as u64).to_be_bytes());
        for name in &self.stack {
            codec::bytes(&mut out, name.as_bytes());
        }
        codec::bytes(&mut out, &self.schedule);
        codec::hash(&mut out, self.initial);
        codec::bytes(&mut out, &self.start);
        RustCryptoSha256::hash(&out)
    }
}

/// One recorded event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Entry {
    /// A request and its binding.
    Request(Binding, Operation),
    /// The answer to the request with this binding.
    Response(Binding, Response),
    /// The process crashed while handling the request with this binding.
    Crash(Binding),
    /// The run's outcome, as canonical bytes.
    Done(Vec<u8>),
}

impl Entry {
    fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Self::Request(binding, operation) => {
                out.push(0);
                codec::bytes(&mut out, &codec::binding(binding));
                codec::bytes(&mut out, &codec::operation(operation));
            }
            Self::Response(binding, response) => {
                out.push(1);
                codec::bytes(&mut out, &codec::binding(binding));
                codec::bytes(&mut out, &codec::response(response));
            }
            Self::Crash(binding) => {
                out.push(2);
                codec::bytes(&mut out, &codec::binding(binding));
            }
            Self::Done(outcome) => {
                out.push(3);
                codec::bytes(&mut out, outcome);
            }
        }
        out
    }
}

/// One entry with its chain link.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Link {
    /// The event.
    pub entry: Entry,
    /// Digest of the previous link, or of the manifest for the first.
    pub previous: Hash32,
    /// SHA-256 of `previous` and the entry's canonical bytes.
    pub digest: Hash32,
}

/// A manifest and its hash-linked entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transcript {
    /// What the run was bound to.
    pub manifest: Manifest,
    /// The entries, in order.
    pub links: Vec<Link>,
}

/// Where a transcript's chain fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChainError {
    /// This link does not name its predecessor, or its digest is wrong.
    Link(usize),
}

fn link_digest(previous: Hash32, entry: &Entry) -> Hash32 {
    let mut out = Vec::new();
    codec::hash(&mut out, previous);
    out.extend_from_slice(&entry.bytes());
    RustCryptoSha256::hash(&out)
}

impl Transcript {
    /// An empty transcript bound to `manifest`.
    #[must_use]
    pub const fn new(manifest: Manifest) -> Self {
        Self {
            manifest,
            links: Vec::new(),
        }
    }

    /// The digest the next link must name.
    #[must_use]
    pub fn tip(&self) -> Hash32 {
        match self.links.last() {
            Some(link) => link.digest,
            None => self.manifest.digest(),
        }
    }

    /// Appends one entry, linked to the tip.
    pub fn append(&mut self, entry: Entry) {
        let previous = self.tip();
        let digest = link_digest(previous, &entry);
        self.links.push(Link {
            entry,
            previous,
            digest,
        });
    }

    /// Checks every link from the manifest digest onward.
    pub fn verify(&self) -> Result<(), ChainError> {
        let mut previous = self.manifest.digest();
        for (index, link) in self.links.iter().enumerate() {
            if link.previous != previous || link.digest != link_digest(previous, &link.entry) {
                return Err(ChainError::Link(index));
            }
            previous = link.digest;
        }
        Ok(())
    }
}

/// How a replayed run ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Ended {
    /// The program finished with this outcome, as canonical bytes.
    Done(Vec<u8>),
    /// The recorded process crashed.
    Crashed,
}

/// Why a replay refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayError {
    /// The chain does not verify.
    Chain(ChainError),
    /// The manifest names another registry, program source or start.
    Manifest,
    /// The transcript ended before the program did, at this link.
    Missing(usize),
    /// This link differs from what the program requested or produced.
    Mismatch(usize),
    /// The program ended before this link.
    Extra(usize),
    /// The driver stopped the program.
    Fatal(Fatal),
}

/// Re-runs a program from its transcript with no handler.
///
/// `start` must equal the manifest's start bytes and `first` must be that
/// start's first step; `done` gives the outcome's canonical bytes.
pub fn replay<P: Program>(
    transcript: &Transcript,
    start: &[u8],
    epoch: u64,
    first: Step<P::Continuation, P::Output>,
    done: fn(&Outcome<P::Output>) -> Vec<u8>,
) -> Result<Ended, ReplayError> {
    transcript.verify().map_err(ReplayError::Chain)?;
    let manifest = &transcript.manifest;
    if manifest.registry != registry::identity(&REGISTRY)
        || manifest.program != registry::program_source()
        || manifest.start != start
    {
        return Err(ReplayError::Manifest);
    }
    let mut driver = Driver::<P>::start(REGISTRY, epoch, first).map_err(ReplayError::Fatal)?;
    let links = &transcript.links;
    let mut at = 0;
    loop {
        if let Some(fatal) = driver.failure() {
            return Err(ReplayError::Fatal(fatal));
        }
        if let Some(outcome) = driver.outcome() {
            match links.get(at) {
                Some(Link {
                    entry: Entry::Done(recorded),
                    ..
                }) if *recorded == done(outcome) => {}
                Some(_) => return Err(ReplayError::Mismatch(at)),
                None => return Err(ReplayError::Missing(at)),
            }
            if at + 1 < links.len() {
                return Err(ReplayError::Extra(at + 1));
            }
            return Ok(Ended::Done(done(outcome)));
        }
        let Some((binding, operation)) = driver.request() else {
            return Err(ReplayError::Mismatch(at));
        };
        match links.get(at) {
            Some(Link {
                entry: Entry::Request(recorded, asked),
                ..
            }) if recorded == binding && asked == operation => {}
            Some(_) => return Err(ReplayError::Mismatch(at)),
            None => return Err(ReplayError::Missing(at)),
        }
        let binding = binding.clone();
        match links.get(at + 1) {
            Some(Link {
                entry: Entry::Response(recorded, response),
                ..
            }) if *recorded == binding => {
                driver
                    .reply(Reply {
                        binding,
                        response: response.clone(),
                    })
                    .map_err(|_| ReplayError::Mismatch(at + 1))?;
            }
            Some(Link {
                entry: Entry::Crash(recorded),
                ..
            }) if *recorded == binding => {
                if at + 2 < links.len() {
                    return Err(ReplayError::Extra(at + 2));
                }
                return Ok(Ended::Crashed);
            }
            Some(_) => return Err(ReplayError::Mismatch(at + 1)),
            None => return Err(ReplayError::Missing(at + 1)),
        }
        at += 2;
    }
}
