//! First-order safety monitors. They only veto the program's exact request.
//!
//! Admission reserves an attempt before forwarding, even if its answer is
//! NotSent, Unknown or a crash. Only explicit non-invocation evidence permits
//! rollback on Suspend; composed partial forwards stay reserved. Responses
//! cannot release reservations.

use alloc::vec::Vec;

use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};

use super::codec;
use super::port::{Code, Operation, Response, Sent};
use super::registry;

/// Premises carried by every policy exploration result and counterexample.
pub const PREMISES: &[&str] = &[
    "Provider idempotency is an environment assumption; a timeout can hide success.",
    "No second send of the same key by this protocol; no exactly-once external execution claim.",
    "Policies are defense in depth; the core still decides.",
    "One trusted relay retains policy state across retries and modelled restarts; real process loss requires durable restoration before sending.",
    "Finite simulation only; receiver deduplication is atomic with its effect; transport copies may duplicate.",
];

/// An external effect key: store identity and delivery ID, never handle or
/// payload identity. Send has no publication replay key.
pub type EffectKey = (Hash32, Hash32);

/// Immutable policy configuration. No callbacks or operation-producing case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Policy {
    /// Reserve at most one forwarded Send per key. Capacity exhaustion refuses.
    AtMostOne {
        /// Maximum retained keys; exhaustion is an explicit refusal.
        capacity: usize,
    },
    /// Exact destination bytes; use [`Self::allowlist`] for canonical ordering.
    Allowlist(Vec<Vec<u8>>),
}

impl Policy {
    /// Owns and canonicalizes the allowlist. Order and repetition have no meaning.
    #[must_use]
    pub fn allowlist(mut recipients: Vec<Vec<u8>>) -> Self {
        recipients.sort();
        recipients.dedup();
        Self::Allowlist(recipients)
    }

    /// Exactly the operation fields this policy reads, besides the Send tag.
    #[must_use]
    pub const fn fields(&self) -> &'static [&'static str] {
        match self {
            Self::AtMostOne { .. } => &["Store.identity", "Delivery.id"],
            Self::Allowlist(_) => &["Delivery.destination"],
        }
    }

    /// Versioned identity binding configuration, declarations and monitor/stack
    /// source. It supplies no decision or publication authority.
    #[must_use]
    pub fn identity(&self) -> Hash32 {
        let mut bytes = Vec::new();
        codec::bytes(&mut bytes, b"zeno-fcis/effect-policy/1");
        codec::hash(&mut bytes, source());
        match self {
            Self::AtMostOne { capacity } => {
                bytes.push(0);
                bytes.extend_from_slice(&(*capacity as u64).to_be_bytes());
            }
            Self::Allowlist(recipients) => {
                bytes.push(1);
                let canonical = Self::allowlist(recipients.clone());
                if let Self::Allowlist(recipients) = canonical {
                    bytes.extend_from_slice(&(recipients.len() as u64).to_be_bytes());
                    for recipient in recipients {
                        codec::bytes(&mut bytes, &recipient);
                    }
                }
            }
        }
        for field in self.fields() {
            codec::bytes(&mut bytes, field.as_bytes());
        }
        RustCryptoSha256::hash(&bytes)
    }
}

/// Source pin for the monitor and the exclusive stack ownership boundary.
#[must_use]
pub fn source() -> Hash32 {
    registry::source_digest(&[
        ("policy.rs", include_bytes!("policy.rs")),
        ("handler.rs", include_bytes!("handler.rs")),
        ("policy_record.rs", include_bytes!("policy_record.rs")),
        ("record.rs", include_bytes!("record.rs")),
        ("codec.rs", include_bytes!("codec.rs")),
    ])
}

/// Owned reservation log. Private data, shared immutable inputs, owned outputs;
/// there is no escaping mutable alias or invocation scratch retained here.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct State {
    attempts: Vec<EffectKey>,
}

impl State {
    /// Canonical state bytes, for restart binding and explorer fingerprints.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(self.attempts.len() as u64).to_be_bytes());
        for (store, delivery) in &self.attempts {
            codec::hash(&mut bytes, *store);
            codec::hash(&mut bytes, *delivery);
        }
        bytes
    }
}

/// The only choices a policy can produce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// Forward the original request unchanged.
    Allow,
    /// Do not invoke the terminal.
    Reject(Code),
}

// The decision body receives only declared fields. An attempted undeclared
// access is explicit absence and fails closed, rather than reading the port.
struct Fields<'a> {
    key: Option<EffectKey>,
    destination: Option<&'a [u8]>,
}

fn decide(policy: &Policy, state: &State, fields: Fields<'_>) -> (State, Verdict) {
    match policy {
        Policy::AtMostOne { capacity } => {
            let Some(key) = fields.key else {
                return (state.clone(), Verdict::Reject(Code::PolicyFields));
            };
            if state.attempts.contains(&key) {
                return (state.clone(), Verdict::Reject(Code::PolicyDuplicate));
            }
            if state.attempts.len() >= *capacity {
                return (state.clone(), Verdict::Reject(Code::PolicyCapacity));
            }
            let mut next = state.clone();
            next.attempts.push(key);
            (next, Verdict::Allow)
        }
        Policy::Allowlist(recipients) => {
            let Some(destination) = fields.destination else {
                return (state.clone(), Verdict::Reject(Code::PolicyFields));
            };
            if !recipients.iter().any(|allowed| allowed == destination) {
                return (state.clone(), Verdict::Reject(Code::PolicyRecipient));
            }
            (state.clone(), Verdict::Allow)
        }
    }
}

/// Pure admission (`response = None`) or observation (`Some(response)`). Only
/// admission reserves. These policies read no response payload fields.
/// The caller reserves before forwarding and rolls back only on certified
/// non-invocation (the simulator's Suspend).
#[must_use]
pub fn step(
    policy: &Policy,
    state: &State,
    operation: &Operation,
    response: Option<&Response>,
) -> (State, Verdict) {
    if response.is_some() {
        return (state.clone(), Verdict::Allow);
    }
    match operation {
        Operation::Send(store, delivery) => {
            let declared = policy.fields();
            let fields = Fields {
                key: if declared.contains(&"Store.identity") && declared.contains(&"Delivery.id") {
                    Some((store.identity, delivery.id))
                } else {
                    None
                },
                destination: if declared.contains(&"Delivery.destination") {
                    Some(&delivery.destination[..])
                } else {
                    None
                },
            };
            decide(policy, state, fields)
        }
        _ => (state.clone(), Verdict::Allow),
    }
}

/// A retained handler layer, separate from the pure step API. The stack owns
/// it exclusively and replaces state only after all policies admit a request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Monitor {
    /// Bound configuration.
    pub(super) policy: Policy,
    /// Current immutable reservation state.
    pub(super) state: State,
    identity: Hash32,
}

impl Monitor {
    /// Starts a new policy lifetime. Do not use this to reset a running relay.
    #[must_use]
    pub fn new(policy: Policy) -> Self {
        let identity = policy.identity();
        Self {
            policy,
            state: State::default(),
            identity,
        }
    }

    /// Identity of the privately owned, unchangeable configuration.
    #[must_use]
    pub const fn identity(&self) -> Hash32 {
        self.identity
    }
}

/// Whether a response is an explicit policy refusal.
#[must_use]
pub const fn refused(response: &Response) -> bool {
    matches!(response, Response::Send(Sent::Refused(_)))
}
