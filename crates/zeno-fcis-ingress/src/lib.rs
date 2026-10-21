//! Authenticated HTTP acquisition around the existing pure core and effect programs.
//! TLS terminates at a trusted proxy; bind this cleartext listener to loopback.
//! Configuration owns its credential, and no credential or command is traceable.
#![forbid(unsafe_code)]

mod assets;
mod durable;
mod http;
mod trace;

pub use assets::Asset;
pub use durable::Durable;
pub use http::{Config, MAX_BODY, MAX_HEADER, MAX_PARTIES, Server, runtime};
pub use trace::{Event, Trace, TraceSink};

use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};

/// An owned authenticated request. Only HTTP credential validation constructs it.
pub struct Command {
    principal: Hash32,
    contract: Hash32,
    key: Hash32,
    expected: u64,
    bytes: Vec<u8>,
}
impl Command {
    /// Validated credential's configured principal.
    pub const fn principal(&self) -> Hash32 {
        self.principal
    }
    /// SHA-256 of the complete Authority identity.
    pub const fn contract(&self) -> Hash32 {
        self.contract
    }
    /// Caller-retained retry key, scoped to this principal and contract.
    pub const fn key(&self) -> Hash32 {
        self.key
    }
    /// Original expected store revision.
    pub const fn expected(&self) -> u64 {
        self.expected
    }
    /// Exact application command envelope; never an authentication source.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Durable replay key, scoped to the authenticated principal and contract.
    pub fn replay_key(&self) -> Hash32 {
        let mut bytes = b"zeno-fcis/ingress-key/1".to_vec();
        bytes.extend_from_slice(self.principal.as_bytes());
        bytes.extend_from_slice(self.contract.as_bytes());
        bytes.extend_from_slice(self.key.as_bytes());
        digest(&bytes)
    }
    /// Request identifier binds every authenticated field and exact command byte.
    pub fn id(&self) -> Hash32 {
        let mut bytes = b"zeno-fcis/ingress-request/1".to_vec();
        bytes.extend_from_slice(self.replay_key().as_bytes());
        bytes.extend_from_slice(&self.expected.to_be_bytes());
        bytes.extend_from_slice(&self.bytes);
        digest(&bytes)
    }
}

/// A fixed refusal vocabulary; diagnostics contain no user-supplied text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// Invalid configuration, wire form, headers, method or route.
    Malformed,
    /// Credential missing or invalid.
    Credential,
    /// Browser origin differs from the configured origin.
    Origin,
    /// Request exceeds a byte limit.
    Size,
    /// Request names a different contract or principal binding.
    Binding,
    /// Retry key already names different bytes, context or revision.
    Replay,
    /// Core refused the command.
    Decision,
    /// Expected revision or another storage guard refused.
    Conflict,
    /// Store or transport is unavailable, uncertain, or crashed. Retain the key.
    Unavailable,
}

/// Shell endpoint. It may acquire and commit; decisions remain with Authority.
pub trait Endpoint {
    /// Authenticated application view. This supplies reporting data, never authority.
    fn snapshot(&mut self, _principal: Hash32) -> Result<Vec<u8>, Refusal> {
        Err(Refusal::Malformed)
    }
    /// Authenticated preview and canonical command preparation. It cannot commit.
    fn preview(&mut self, _command: &Command) -> Result<Vec<u8>, Refusal> {
        Err(Refusal::Malformed)
    }

    /// Executes one immutable request and emits redacted structured observations.
    fn execute(&mut self, command: &Command, trace: &mut dyn TraceSink)
    -> Result<Vec<u8>, Refusal>;
}

/// Encode the versioned HTTP body. No actor field is accepted by the protocol.
pub fn encode_command(contract: Hash32, key: Hash32, expected: u64, command: &[u8]) -> Vec<u8> {
    let mut out = vec![1];
    out.extend_from_slice(contract.as_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(&expected.to_be_bytes());
    out.extend_from_slice(command);
    out
}

pub(crate) fn digest(bytes: &[u8]) -> Hash32 {
    RustCryptoSha256::hash(bytes)
}
