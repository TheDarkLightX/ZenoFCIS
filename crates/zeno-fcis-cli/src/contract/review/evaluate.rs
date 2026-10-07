//! Decisions through the library route. An input tuple is framed as the
//! canonical envelopes the Authority admits, the bound Authority evaluates
//! it, and the outcome is the complete decision, or the refusal, with digests
//! of the successor state and of the outbox. A refusal keeps the law the
//! library's own law diagnostics name as refusing, and whether a pre-state
//! satisfies the contract's state laws is the library's law evaluator's
//! verdict.

use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::{
    V2ExecutionFailure, canonical_v2::output, v2_authority, v2_authority::Authority,
    v2_composition as c, v2_laws as laws,
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
    Refused(Refusal),
}

/// A refusal of the library Authority.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Refusal {
    /// The refusal as the library reports it, such as `Core(Law(Violated))`.
    pub(super) text: String,
    pub(super) class: RefusalClass,
    /// The law whose evaluation refused, as the library's law diagnostics
    /// name it; `None` when no law evaluation refused.
    pub(super) law: Option<u32>,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.law {
            Some(law) => write!(formatter, "{} by law {law}", self.text),
            None => formatter.write_str(&self.text),
        }
    }
}

/// The stage of the library route that refused.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum RefusalClass {
    Law,
    Domain,
    Arithmetic,
    Meter,
    Input,
    Other,
}

impl RefusalClass {
    pub(super) const ALL: [Self; 6] = [
        Self::Law,
        Self::Domain,
        Self::Arithmetic,
        Self::Meter,
        Self::Input,
        Self::Other,
    ];

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Law => "law",
            Self::Domain => "domain",
            Self::Arithmetic => "arithmetic",
            Self::Meter => "meter",
            Self::Input => "input",
            Self::Other => "other",
        }
    }

    /// Which library refusals the class holds.
    pub(super) fn meaning(self) -> &'static str {
        match self {
            Self::Law => {
                "an applicable law refused the complete decision: Core(Law(..)), except a meter refusal"
            }
            Self::Domain => {
                "a value left its declared domain: Core(Decision(Domain)), Core(Schema), Core(Output), Core(Execution(InputDomain)) or Core(Execution(OutputDomain))"
            }
            Self::Arithmetic => {
                "checked arithmetic of the decision program overflowed: Core(Execution(Arithmetic))"
            }
            Self::Meter => "the shared meter refused work at any stage: a MeterFailure",
            Self::Input => {
                "the inputs were not admitted: Core(Ingress(..)), Core(Frame(..)) or Core(Binding)"
            }
            Self::Other => "any other refusal",
        }
    }
}

/// The class of a library refusal, given with its text. The library does not
/// export the type of a decision or ingress failure, so those two are told
/// apart by the text the library reports.
fn class(refusal: &v2_authority::Refusal, text: &str) -> RefusalClass {
    if text.contains("MeterFailure") {
        return RefusalClass::Meter;
    }
    match refusal {
        v2_authority::Refusal::Core(failure) => match failure {
            c::Failure::Law(_) => RefusalClass::Law,
            c::Failure::Schema
            | c::Failure::Output
            | c::Failure::Execution(
                V2ExecutionFailure::InputDomain | V2ExecutionFailure::OutputDomain,
            ) => RefusalClass::Domain,
            c::Failure::Decision(_) if text == "Core(Decision(Domain))" => RefusalClass::Domain,
            c::Failure::Execution(V2ExecutionFailure::Arithmetic) => RefusalClass::Arithmetic,
            c::Failure::Ingress(..) | c::Failure::Frame(..) | c::Failure::Binding => {
                RefusalClass::Input
            }
            _ => RefusalClass::Other,
        },
        _ => RefusalClass::Other,
    }
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
        let payload = if root.record {
            output::encode_record(&self.fields(root, tuple), root.max_bytes)
        } else {
            match root.positions.first() {
                Some((_, index)) => {
                    output::encode_atom(atom(self.kinds[*index], tuple[*index]), root.max_bytes)
                }
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

    /// A record root's fields in field-ID order, as the library admits them.
    fn fields(&self, root: &RootFrame, tuple: &[i64]) -> Vec<c::Field<'static>> {
        let mut fields: Vec<c::Field<'static>> = root
            .positions
            .iter()
            .map(|(id, index)| c::Field {
                id: *id,
                value: atom(self.kinds[*index], tuple[*index]),
            })
            .collect();
        fields.sort_by_key(|field| field.id);
        fields
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
        Err(refusal) => {
            let text = format!("{refusal:?}");
            Outcome::Refused(Refusal {
                class: class(&refusal, &text),
                text,
                law: evaluation
                    .diagnostics()
                    .iter()
                    .find(|diagnostic| matches!(diagnostic.verdict, laws::Verdict::Refused(_)))
                    .map(|diagnostic| diagnostic.id),
            })
        }
    }
}

/// The contract's state laws, in its law order: each law that applies at
/// genesis and to every committing decision, so that every committed state
/// satisfies it, except an `InitialCondition` law, which applies at genesis
/// only. In a generated contract they are the laws declared `on commit,
/// genesis`, every `StateInvariant` among them, and those declared `on any,
/// genesis` other than an `InitialCondition`.
pub(super) fn state_laws(descriptor: &c::Descriptor<'_>) -> Vec<u32> {
    descriptor
        .laws
        .iter()
        .filter(|law| {
            law.genesis
                && matches!(law.scope, laws::Scope::Committing | laws::Scope::Always)
                && !matches!(law.kind, laws::Kind::InitialCondition)
        })
        .map(|law| law.id)
        .collect()
}

/// The first state law the tuple's pre-state does not satisfy, or `None`
/// when it satisfies every one. The library's law evaluator runs the
/// contract's own law programs on the pre-state as a genesis frame, where a
/// law that reads the successor reads that state, with the state laws
/// ordered first. A refusing verdict counts against the state: the
/// Authority refuses to commit such a state. A state law the evaluator never
/// reached, which only a refusal of the whole frame causes, leaves the state
/// counted as satisfying it, so that a law refusal on it stays a finding.
pub(super) fn first_unsatisfied_state_law(
    descriptor: &c::Descriptor<'_>,
    state_laws: &[u32],
    framer: &Framer,
    tuple: &[i64],
) -> Option<u32> {
    if state_laws.is_empty() {
        return None;
    }
    let first = |law: &&laws::Law<'_>| state_laws.contains(&law.id);
    let ordered: Vec<laws::Law<'_>> = state_laws
        .iter()
        .filter_map(|id| descriptor.laws.iter().find(|law| law.id == *id))
        .chain(descriptor.laws.iter().filter(|law| !first(law)))
        .map(|law| laws::Law {
            id: law.id,
            kind: law.kind,
            scope: law.scope,
            genesis: law.genesis,
            program: laws::Program {
                nodes: law.program.nodes,
                root: law.program.root,
            },
        })
        .collect();
    let root = &framer.roots[0];
    let fields = framer.fields(root, tuple);
    let initial = match root.positions.first() {
        Some((_, index)) if !root.record => {
            laws::RootView::Leaf(atom(framer.kinds[*index], tuple[*index]))
        }
        _ => laws::RootView::Record(&fields),
    };
    let frame = laws::Frame::Genesis { initial };
    let (_, _, diagnostics, _) =
        laws::evaluate(&ordered, descriptor.required, &frame, descriptor.limits).into_parts();
    for id in state_laws {
        match diagnostics.iter().find(|diagnostic| diagnostic.id == *id) {
            Some(diagnostic) if diagnostic.verdict == laws::Verdict::Satisfied => {}
            Some(_) => return Some(*id),
            None => return None,
        }
    }
    None
}

fn decision(candidate: &c::Candidate<'_>) -> Outcome {
    let class = match candidate.class() {
        c::Class::Accept => Class::Accept,
        c::Class::Reject => Class::Reject,
        c::Class::CommittedFailure => Class::CommittedFailure,
        other => {
            return Outcome::Refused(Refusal {
                text: format!("unknown decision class {other:?}"),
                class: RefusalClass::Other,
                law: None,
            });
        }
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
