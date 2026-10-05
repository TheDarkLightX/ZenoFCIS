//! Decisions through the library route. An input tuple is framed as the
//! canonical envelopes the Authority admits, the bound Authority evaluates
//! it, and the outcome is the complete decision, or the refusal, with digests
//! of the successor state and of the outbox.

use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::{
    canonical_v2::output, v2_authority::Authority, v2_composition as c,
};

use super::super::declarations::{ROOTS, Source};
use super::super::model::{Contract, RootSchema};
use super::super::rules::Class;
use super::domain::{LeafDomain, Position};

/// Bytes an encoding may take; every contract value is far smaller.
const ENCODING_LIMIT: usize = 1 << 24;

/// A typed value of a decision, owned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Value {
    Bool(bool),
    I128(i128),
    U128(u128),
    Sum { type_id: u32, variant: u16 },
    Enum { type_id: u32, variant: u16 },
    Text(Vec<u8>),
    Bytes(Vec<u8>),
    Other(String),
}

impl Value {
    fn from_atom(atom: c::Atom<'_>) -> Self {
        match atom {
            c::Atom::Bool(value) => Self::Bool(value),
            c::Atom::I128(value) => Self::I128(value),
            c::Atom::U128(value) => Self::U128(value),
            c::Atom::Sum { type_id, variant } => Self::Sum { type_id, variant },
            c::Atom::Enum { type_id, variant } => Self::Enum { type_id, variant },
            c::Atom::Text(text) => Self::Text(text.to_vec()),
            c::Atom::Bytes(bytes) => Self::Bytes(bytes.to_vec()),
            other => Self::Other(format!("{other:?}")),
        }
    }

    /// The number decision examples write: 0 or 1 for a boolean, the value
    /// of an integer, the variant ID of a sum.
    pub(super) fn number(&self) -> Option<i128> {
        match self {
            Self::Bool(value) => Some(i128::from(*value)),
            Self::I128(value) => Some(*value),
            Self::U128(value) => i128::try_from(*value).ok(),
            Self::Sum { variant, .. } | Self::Enum { variant, .. } => Some(i128::from(*variant)),
            Self::Text(_) | Self::Bytes(_) | Self::Other(_) => None,
        }
    }

    pub(super) fn json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Self::Bool(value) => json!(value),
            Self::I128(value) => json!(value.to_string()),
            Self::U128(value) => json!(value.to_string()),
            Self::Sum { type_id, variant } | Self::Enum { type_id, variant } => {
                json!({"type_id": type_id, "variant": variant})
            }
            Self::Text(text) => match std::str::from_utf8(text) {
                Ok(text) => json!(text),
                Err(_) => json!(format!("0x{}", hex(text))),
            },
            Self::Bytes(bytes) => json!(format!("0x{}", hex(bytes))),
            Self::Other(text) => json!(text),
        }
    }
}

/// Field IDs with their values, in field order.
pub(super) type Fields = Vec<(u16, Value)>;

/// One outbox delivery of a decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Delivery {
    pub(super) ordinal: u32,
    pub(super) channel: u32,
    pub(super) destination: Value,
    pub(super) payload: Fields,
    pub(super) idempotency: Value,
}

/// A complete decision of the library Authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Decision {
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    /// Successor state fields; empty for a reject.
    pub(super) post: Fields,
    pub(super) outbox: Vec<Delivery>,
    /// SHA-256 of the library's canonical encoding of the successor record.
    pub(super) post_digest: [u8; 32],
    /// SHA-256 over every effect, then every outbox delivery: ordinal and
    /// channel as big-endian 32-bit words, then the canonical destination,
    /// payload record and idempotency encodings. Generated contracts declare
    /// no effects, so the effects contribute nothing.
    pub(super) outbox_digest: [u8; 32],
}

/// What the Authority made of one input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Outcome {
    Decision(Decision),
    /// The library's refusal, as it reports it.
    Refused(String),
}

/// The atom a position's number denotes.
#[derive(Clone, Copy, Debug)]
enum AtomKind {
    Bool,
    Int,
    Sum(u32),
}

/// Frames one root: a record of fields, or one scalar.
#[derive(Debug)]
struct RootFrame {
    root: u32,
    /// Field ID and position index of each value; one entry for a scalar root.
    positions: Vec<(u16, usize)>,
    record: bool,
    max_bytes: usize,
}

/// Frames input tuples as the envelopes the contract's Authority admits.
#[derive(Debug)]
pub(super) struct Framer {
    commitment: [u8; 32],
    kinds: Vec<AtomKind>,
    roots: [RootFrame; 3],
}

impl Framer {
    pub(super) fn new(positions: &[Position], contract: &Contract<'_>) -> Self {
        let kinds = positions
            .iter()
            .map(|position| match &position.domain {
                LeafDomain::Bool => AtomKind::Bool,
                LeafDomain::Int { .. } => AtomKind::Int,
                LeafDomain::Sum { type_id, .. } => AtomKind::Sum(*type_id),
            })
            .collect();
        let frame = |index: usize| {
            let (source, root): (Source, u32) = ROOTS[index];
            RootFrame {
                root,
                positions: positions
                    .iter()
                    .enumerate()
                    .filter(|(_, position)| position.source == source)
                    .map(|(number, position)| (position.field.unwrap_or(0), number))
                    .collect(),
                record: matches!(contract.roots[index], RootSchema::Record(_)),
                max_bytes: usize::try_from(contract.frame_bytes[index]).unwrap_or(usize::MAX),
            }
        };
        Self {
            commitment: contract.commitment,
            kinds,
            roots: [frame(0), frame(1), frame(2)],
        }
    }

    /// The framed state, command and context bytes of a tuple.
    pub(super) fn frame(&self, tuple: &[i64]) -> [Vec<u8>; 3] {
        [
            self.envelope(&self.roots[0], tuple),
            self.envelope(&self.roots[1], tuple),
            self.envelope(&self.roots[2], tuple),
        ]
    }

    fn envelope(&self, root: &RootFrame, tuple: &[i64]) -> Vec<u8> {
        let atom = |index: usize| atom(self.kinds[index], tuple[index]);
        let payload = if root.record {
            let mut fields: Vec<c::Field<'static>> = root
                .positions
                .iter()
                .map(|(id, index)| c::Field {
                    id: *id,
                    value: atom(*index),
                })
                .collect();
            fields.sort_by_key(|field| field.id);
            output::encode_record(&fields, root.max_bytes)
        } else {
            match root.positions.first() {
                Some((_, index)) => output::encode_atom(atom(*index), root.max_bytes),
                None => Ok(Vec::new()),
            }
        };
        // A value the frame limit refuses gives an empty frame, which the
        // Authority refuses in turn, and the review records that refusal.
        payload
            .and_then(|payload| {
                output::encode_envelope(root.root, &self.commitment, &payload, root.max_bytes)
            })
            .unwrap_or_default()
    }
}

fn atom(kind: AtomKind, value: i64) -> c::Atom<'static> {
    match kind {
        AtomKind::Bool => c::Atom::Bool(value != 0),
        AtomKind::Int => c::Atom::I128(i128::from(value)),
        AtomKind::Sum(type_id) => c::Atom::Sum {
            type_id,
            variant: u16::try_from(value).unwrap_or(u16::MAX),
        },
    }
}

/// Evaluates one tuple with the bound Authority.
pub(super) fn evaluate(authority: &Authority<'_>, framer: &Framer, tuple: &[i64]) -> Outcome {
    let frames = framer.frame(tuple);
    let evaluation = authority.evaluate(c::Raw {
        state: &frames[0],
        command: &frames[1],
        context: &frames[2],
    });
    match evaluation.result() {
        Ok(candidate) => decision(candidate),
        Err(refusal) => Outcome::Refused(format!("{refusal:?}")),
    }
}

fn decision(candidate: &c::Candidate<'_>) -> Outcome {
    let class = match candidate.class() {
        c::Class::Accept => Class::Accept,
        c::Class::Reject => Class::Reject,
        c::Class::CommittedFailure => Class::CommittedFailure,
        other => return Outcome::Refused(format!("unknown decision class {other:?}")),
    };
    Outcome::Decision(Decision {
        class,
        reason: candidate.reason(),
        post: fields(candidate.post()),
        outbox: candidate
            .outbox()
            .iter()
            .map(|delivery| Delivery {
                ordinal: delivery.ordinal,
                channel: delivery.channel,
                destination: Value::from_atom(delivery.destination),
                payload: fields(&delivery.payload),
                idempotency: Value::from_atom(delivery.idempotency),
            })
            .collect(),
        post_digest: digest(&record_bytes(candidate.post())),
        outbox_digest: outbox_digest(candidate),
    })
}

fn fields(fields: &[c::Field<'_>]) -> Fields {
    fields
        .iter()
        .map(|field| (field.id, Value::from_atom(field.value)))
        .collect()
}

/// The library's canonical record encoding; a record it cannot encode is
/// digested as its debug rendering, so the digest still exists.
fn record_bytes(fields: &[c::Field<'_>]) -> Vec<u8> {
    output::encode_record(fields, ENCODING_LIMIT)
        .unwrap_or_else(|_| format!("{fields:?}").into_bytes())
}

fn atom_bytes(atom: c::Atom<'_>) -> Vec<u8> {
    output::encode_atom(atom, ENCODING_LIMIT).unwrap_or_else(|_| format!("{atom:?}").into_bytes())
}

/// The library does not export its delivery type; the candidate's effects
/// and outbox are digested in place.
fn outbox_digest(candidate: &c::Candidate<'_>) -> [u8; 32] {
    let mut bytes = Vec::new();
    for delivery in candidate.effects().iter().chain(candidate.outbox()) {
        bytes.extend_from_slice(&delivery.ordinal.to_be_bytes());
        bytes.extend_from_slice(&delivery.channel.to_be_bytes());
        bytes.extend(atom_bytes(delivery.destination));
        bytes.extend(record_bytes(&delivery.payload));
        bytes.extend(atom_bytes(delivery.idempotency));
    }
    digest(&bytes)
}

/// SHA-256 of `bytes`.
pub(super) fn digest(bytes: &[u8]) -> [u8; 32] {
    *RustCryptoSha256::hash(bytes).as_bytes()
}

/// Lowercase hexadecimal.
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
