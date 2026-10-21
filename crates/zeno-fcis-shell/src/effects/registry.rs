//! The `shell-effects/1` registry: every port's signature as contract data.
//!
//! A signature gives the port's tag, argument and result schemas (byte caps
//! and checked integer ranges), capability, effect footprint, law IDs and the
//! default V2 API it maps to. [`check`] refuses a registry that is not
//! exhaustive, has a malformed schema, a footprint its capability does not
//! allow, or an unknown or unused law. [`conforms`] checks one port value
//! against its schema.

use alloc::vec::Vec;

use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};

use super::port::Val;

/// Schema identifier of this registry format.
pub const SCHEMA: &str = "zeno-fcis/shell-effects/1";

/// SHA-256 of the step-function source, framed by [`source_digest`], as hex.
/// Changing `program.rs` needs a deliberate new binding here.
pub const PROGRAM_SOURCE_SHA256: &str =
    "dde6e52db1982d4cbeef2198d24baf9a533b8af5b8239e8e716144a1def90937";

/// Largest byte cap a field may declare.
pub const MAX_BYTES: u64 = 1 << 20;
/// Largest item count a list may declare.
pub const MAX_ITEMS: u64 = 1 << 20;
const MAX_DEPTH: u8 = 8;

/// One effect port. The discriminant is its tag.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Port {
    /// One checked snapshot.
    ReadState = 1,
    /// One atomic publication.
    Commit = 2,
    /// Audited original inputs, keys and bundles; reporting only.
    ReadHistory = 3,
    /// Exact pending deliveries, ordered by version, lane and ordinal.
    ReadOutbox = 4,
    /// A transport observation.
    Send = 5,
    /// The relay acknowledgement.
    Acknowledge = 6,
    /// Open, audit and return a snapshot; never create a store.
    Reopen = 7,
    /// Monotonic ticks within one process epoch.
    Clock = 8,
    /// Retry jitter only.
    Random = 9,
    /// Yield until a deadline.
    Wait = 10,
}

impl Port {
    /// Every port, in tag order.
    pub const ALL: [Self; 10] = [
        Self::ReadState,
        Self::Commit,
        Self::ReadHistory,
        Self::ReadOutbox,
        Self::Send,
        Self::Acknowledge,
        Self::Reopen,
        Self::Clock,
        Self::Random,
        Self::Wait,
    ];

    /// The port's tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        self as u8
    }
}

/// A schema node. Integer ranges and byte caps are inclusive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// No data.
    Unit,
    /// A store handle: a 32-byte identity and a `u32` handle number.
    Store,
    /// Exactly 32 bytes.
    Hash,
    /// At most this many bytes.
    Bytes(u64),
    /// An unsigned integer in `[min, max]`.
    Int(u64, u64),
    /// Named fields, in order.
    Record(&'static [Field]),
    /// Exactly one of the named alternatives, each with its payload.
    Choice(&'static [Field]),
    /// At most this many items of one kind.
    List(&'static Kind, u64),
}

/// A named schema node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Field {
    /// Field or alternative name: lowercase ASCII, digits and `_`.
    pub name: &'static str,
    /// Its schema.
    pub kind: Kind,
}

const fn field(name: &'static str, kind: Kind) -> Field {
    Field { name, kind }
}

/// How a port answers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Answer {
    /// `Ok(T) | Refused(Code) | NotApplied(Fault) | Unknown(Fault)`.
    Outcome(Kind),
    /// `Sent(T) | NotSent(Code) | Unknown(Fault) | Refused(Code)`.
    Transport(Kind),
    /// `Ok(T) | Err(Fault)`.
    Fallible(Kind),
    /// `Ok(T) | Interrupted | Err(Fault)`.
    Wait(Kind),
}

impl Answer {
    /// The success payload's schema.
    #[must_use]
    pub const fn payload(self) -> Kind {
        match self {
            Self::Outcome(kind)
            | Self::Transport(kind)
            | Self::Fallible(kind)
            | Self::Wait(kind) => kind,
        }
    }
}

/// A set of logical resources a port may read or write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resources(u8);

impl Resources {
    /// Nothing.
    pub const NONE: Self = Self(0);
    /// D: durable state, history, replays and outbox.
    pub const DURABLE: Self = Self(1);
    /// V: volatile handle state and caches.
    pub const VOLATILE: Self = Self(2);
    /// X: the external transport.
    pub const TRANSPORT: Self = Self(4);
    /// T: the epoch clock.
    pub const CLOCK: Self = Self(8);
    /// J: the jitter generator.
    pub const JITTER: Self = Self(16);
    const KNOWN: u8 = 31;

    /// The union of two sets.
    #[must_use]
    pub const fn and(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every member of `self` is in `other`.
    #[must_use]
    pub const fn within(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }

    /// The raw bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// What a port may touch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Footprint {
    /// Resources the port may read.
    pub reads: Resources,
    /// Resources the port may change.
    pub writes: Resources,
}

/// The authority a program must declare to request a port.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Capability {
    /// Read the store; may refresh volatile state.
    ReadStore,
    /// Publish to the store.
    WriteStore,
    /// Acknowledge a delivery.
    Relay,
    /// Open and audit the store.
    OpenStore,
    /// Use the external transport; never the store.
    Transport,
    /// Read the epoch clock and wait.
    Clock,
    /// Draw retry jitter.
    Jitter,
}

impl Capability {
    /// The largest footprint the capability allows.
    #[must_use]
    pub const fn allows(self) -> Footprint {
        let store = Resources::DURABLE.and(Resources::VOLATILE);
        match self {
            Self::ReadStore => Footprint {
                reads: store,
                writes: Resources::VOLATILE,
            },
            Self::WriteStore | Self::Relay => Footprint {
                reads: store,
                writes: store,
            },
            Self::OpenStore => Footprint {
                reads: Resources::DURABLE,
                writes: Resources::VOLATILE,
            },
            Self::Transport => Footprint {
                reads: Resources::TRANSPORT,
                writes: Resources::TRANSPORT,
            },
            Self::Clock => Footprint {
                reads: Resources::CLOCK,
                writes: Resources::NONE,
            },
            Self::Jitter => Footprint {
                reads: Resources::JITTER,
                writes: Resources::JITTER,
            },
        }
    }
}

/// The default V2 API a port maps to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum V2 {
    /// These existing V2 calls, in order.
    Api(&'static [&'static str]),
    /// Outside the store: the external transport the relay export feeds.
    External(&'static str),
    /// No default V2 API; an adapter is opt-in.
    OptIn,
}

/// One port's declared signature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Signature {
    /// The port.
    pub port: Port,
    /// Argument record.
    pub arguments: Kind,
    /// Result shape and payload.
    pub answer: Answer,
    /// Capability a program must declare.
    pub capability: Capability,
    /// What the port may touch.
    pub footprint: Footprint,
    /// Laws every handler of this port must satisfy.
    pub laws: &'static [u16],
    /// The default V2 mapping.
    pub v2: V2,
}

/// One handler law.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Law {
    /// Law ID.
    pub id: u16,
    /// Short name.
    pub name: &'static str,
    /// The equation or frame, in words.
    pub statement: &'static str,
}

/// A complete registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Registry {
    /// Must be [`SCHEMA`].
    pub schema: &'static str,
    /// One signature per port, in tag order.
    pub signatures: &'static [Signature],
    /// Every law a signature names, by ascending ID.
    pub laws: &'static [Law],
}

const ENVELOPE: Kind = Kind::Bytes(MAX_BYTES);
const VERSION: Kind = Kind::Int(0, i64::MAX as u64);
const ANY: Kind = Kind::Int(0, u64::MAX);
const STORE: &[Field] = &[field("store", Kind::Store)];
const PUBLICATION: &[Field] = &[
    field("prestate", ENVELOPE),
    field("command", ENVELOPE),
    field("context", ENVELOPE),
    field("identity", ENVELOPE),
    field("subject", ENVELOPE),
];
const BOUNDED: &[Field] = &[field("version", VERSION), field("cap", ANY)];
const MODE: &[Field] = &[
    field("unversioned", Kind::Unit),
    field("at", VERSION),
    field("bounded_at", Kind::Record(BOUNDED)),
];
const COMMIT: &[Field] = &[
    field("store", Kind::Store),
    field("key", Kind::Hash),
    field("publication", Kind::Record(PUBLICATION)),
    field("mode", Kind::Choice(MODE)),
];
const SNAPSHOT: &[Field] = &[
    field("version", VERSION),
    field("identity", ENVELOPE),
    field("state", ENVELOPE),
    field("root", Kind::Hash),
    field("chain", Kind::Hash),
];
const STATUS: &[Field] = &[field("committed", Kind::Unit), field("replay", Kind::Unit)];
const RECEIPT: &[Field] = &[
    field("version", VERSION),
    field("chain", Kind::Hash),
    field("status", Kind::Choice(STATUS)),
];
const ENTRY: &[Field] = &[
    field("key", Kind::Hash),
    field("version", Kind::Int(1, i64::MAX as u64)),
    field("publication", Kind::Record(PUBLICATION)),
];
const HISTORY: &[Field] = &[
    field("head", Kind::Record(SNAPSHOT)),
    field("commits", Kind::List(&Kind::Record(ENTRY), MAX_ITEMS)),
];
const DELIVERY: &[Field] = &[
    field("id", Kind::Hash),
    field("version", Kind::Int(1, i64::MAX as u64)),
    field("lane", Kind::Int(0, 1)),
    field("ordinal", Kind::Int(0, u32::MAX as u64)),
    field("destination", ENVELOPE),
    field("payload", ENVELOPE),
    field("payload_sha256", Kind::Hash),
];
const SEND: &[Field] = &[
    field("store", Kind::Store),
    field("delivery", Kind::Record(DELIVERY)),
];
const REPORT: &[Field] = &[
    field("delivery_id", Kind::Hash),
    field("payload_sha256", Kind::Hash),
];
const ACKNOWLEDGE: &[Field] = &[
    field("store", Kind::Store),
    field("delivery_id", Kind::Hash),
    field("payload_sha256", Kind::Hash),
];
const TICK: &[Field] = &[field("epoch", ANY), field("tick", ANY)];
const PURPOSE: &[Field] = &[field("retry_jitter", Kind::Unit)];
const RANDOM: &[Field] = &[
    field("purpose", Kind::Choice(PURPOSE)),
    field("bound", Kind::Int(1, u64::MAX)),
];
const WAIT: &[Field] = &[field("deadline", Kind::Record(TICK))];

const READ: Footprint = Footprint {
    reads: Resources::DURABLE.and(Resources::VOLATILE),
    writes: Resources::VOLATILE,
};
const WRITE: Footprint = Footprint {
    reads: Resources::DURABLE.and(Resources::VOLATILE),
    writes: Resources::DURABLE.and(Resources::VOLATILE),
};

const SEND_ONLY: Footprint = Footprint {
    reads: Resources::TRANSPORT,
    writes: Resources::TRANSPORT,
};
const OPEN: Footprint = Footprint {
    reads: Resources::DURABLE,
    writes: Resources::VOLATILE,
};
const TIME: Footprint = Footprint {
    reads: Resources::CLOCK,
    writes: Resources::NONE,
};
const DRAW: Footprint = Footprint {
    reads: Resources::JITTER,
    writes: Resources::JITTER,
};

/// The `shell-effects/1` signatures, in tag order.
pub const SIGNATURES: &[Signature] = &[
    Signature {
        port: Port::ReadState,
        arguments: Kind::Record(STORE),
        answer: Answer::Outcome(Kind::Record(SNAPSHOT)),
        capability: Capability::ReadStore,
        footprint: READ,
        laws: &[3],
        v2: V2::Api(&["V2SqliteShell::snapshot"]),
    },
    Signature {
        port: Port::Commit,
        arguments: Kind::Record(COMMIT),
        answer: Answer::Outcome(Kind::Record(RECEIPT)),
        capability: Capability::WriteStore,
        footprint: WRITE,
        laws: &[1, 2],
        v2: V2::Api(&[
            "V2SqliteShell::commit",
            "V2SqliteShell::commit_at",
            "V2SqliteShell::commit_bounded_at",
        ]),
    },
    Signature {
        port: Port::ReadHistory,
        arguments: Kind::Record(STORE),
        answer: Answer::Outcome(Kind::Record(HISTORY)),
        capability: Capability::ReadStore,
        footprint: READ,
        laws: &[4],
        v2: V2::Api(&["Lineage::audit_history_read_only"]),
    },
    Signature {
        port: Port::ReadOutbox,
        arguments: Kind::Record(STORE),
        answer: Answer::Outcome(Kind::List(&Kind::Record(DELIVERY), MAX_ITEMS)),
        capability: Capability::ReadStore,
        footprint: READ,
        laws: &[4],
        v2: V2::Api(&["V2SqliteShell::export_pending"]),
    },
    Signature {
        port: Port::Send,
        arguments: Kind::Record(SEND),
        answer: Answer::Transport(Kind::Record(REPORT)),
        capability: Capability::Transport,
        footprint: SEND_ONLY,
        laws: &[5],
        v2: V2::External("relay::export_line"),
    },
    Signature {
        port: Port::Acknowledge,
        arguments: Kind::Record(ACKNOWLEDGE),
        answer: Answer::Outcome(Kind::Unit),
        capability: Capability::Relay,
        footprint: WRITE,
        laws: &[6],
        v2: V2::Api(&[
            "V2SqliteShell::pending_by_id",
            "Pending::relayed",
            "Delivered::acknowledge",
        ]),
    },
    Signature {
        port: Port::Reopen,
        arguments: Kind::Record(STORE),
        answer: Answer::Outcome(Kind::Record(SNAPSHOT)),
        capability: Capability::OpenStore,
        footprint: OPEN,
        laws: &[7],
        v2: V2::Api(&["V2SqliteShell::open", "V2SqliteShell::audit"]),
    },
    Signature {
        port: Port::Clock,
        arguments: Kind::Record(&[]),
        answer: Answer::Fallible(Kind::Record(TICK)),
        capability: Capability::Clock,
        footprint: TIME,
        laws: &[8],
        v2: V2::OptIn,
    },
    Signature {
        port: Port::Random,
        arguments: Kind::Record(RANDOM),
        answer: Answer::Fallible(Kind::Int(0, u64::MAX - 1)),
        capability: Capability::Jitter,
        footprint: DRAW,
        laws: &[9],
        v2: V2::OptIn,
    },
    Signature {
        port: Port::Wait,
        arguments: Kind::Record(WAIT),
        answer: Answer::Wait(Kind::Record(TICK)),
        capability: Capability::Clock,
        footprint: TIME,
        laws: &[8],
        v2: V2::OptIn,
    },
];

/// The handler laws, by ascending ID.
pub const LAWS: &[Law] = &[
    Law {
        id: 1,
        name: "commit-once",
        statement: "Commit(k,p); Commit(k,p) applies once and the second answers Replay with \
                    the original receipt; changed bytes under k refuse with D unchanged",
    },
    Law {
        id: 2,
        name: "commit-atomic",
        statement: "a success changes state, sequence, root, chain, replay record and every \
                    required outbox row together; after a crash D is the pre-state or the \
                    complete post-state; NotApplied and Refused leave D unchanged",
    },
    Law {
        id: 3,
        name: "read-state",
        statement: "one consistent snapshot; after a successful commit it sees the post-state \
                    or a validated descendant; a failure is never an empty state",
    },
    Law {
        id: 4,
        name: "read-exact",
        statement: "consistent and exact; repeated reads with no write between agree; reads \
                    consume nothing; history proves nothing about later commits",
    },
    Law {
        id: 5,
        name: "send-exact",
        statement: "sender D is unchanged and copies are exact; a report proves neither \
                    receipt nor authentication; duplicates may duplicate external effects",
    },
    Law {
        id: 6,
        name: "acknowledge-exact",
        statement: "a matching (id, hash) sets only that record's bit, atomically and \
                    monotonically; a wrong ID or hash refuses; repeating gives \
                    AlreadyAcknowledged",
    },
    Law {
        id: 7,
        name: "reopen-audits",
        statement: "recovers and audits D and drops V, or refuses; never resets, silently \
                    repairs or substitutes a store",
    },
    Law {
        id: 8,
        name: "time-monotone",
        statement: "ticks are monotonic within an epoch; Wait returns a tick at or after its \
                    deadline or an explicit interruption; neither changes D",
    },
    Law {
        id: 9,
        name: "jitter-in-range",
        statement: "a draw is below its bound; identical generator state gives an identical \
                    draw in simulation; no independence or cryptographic claim",
    },
];

/// The registry this library ships.
pub const REGISTRY: Registry = Registry {
    schema: SCHEMA,
    signatures: SIGNATURES,
    laws: LAWS,
};

/// Why a registry is refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryError {
    /// The schema identifier is not [`SCHEMA`].
    Schema,
    /// A port has no signature.
    Missing(Port),
    /// A port has a second signature, or signatures are out of tag order.
    Order(Port),
    /// A schema node is malformed: an empty or repeated name, a bad cap or
    /// range, nesting deeper than eight, or arguments that are not a record.
    Malformed(Port, &'static str),
    /// The footprint names an unknown resource or exceeds the capability.
    Footprint(Port),
    /// A signature names no law, an unknown law or one law twice.
    Law(Port),
    /// Law IDs are not strictly ascending, or a law is never named.
    UnusedLaw(u16),
    /// A store port does not map to a V2 API.
    Unmapped(Port),
}

/// Checks a registry completely.
pub fn check(registry: &Registry) -> Result<(), RegistryError> {
    if registry.schema != SCHEMA {
        return Err(RegistryError::Schema);
    }
    for (index, port) in Port::ALL.iter().enumerate() {
        let Some(signature) = registry.signatures.get(index) else {
            return Err(RegistryError::Missing(*port));
        };
        if signature.port != *port {
            return Err(if signature.port < *port {
                RegistryError::Order(signature.port)
            } else {
                RegistryError::Missing(*port)
            });
        }
        check_signature(registry, signature)?;
    }
    if let Some(extra) = registry.signatures.get(Port::ALL.len()) {
        return Err(RegistryError::Order(extra.port));
    }
    let mut previous = None;
    for law in registry.laws {
        if previous.is_some_and(|id| id >= law.id)
            || law.name.is_empty()
            || law.statement.is_empty()
            || !registry
                .signatures
                .iter()
                .any(|signature| signature.laws.contains(&law.id))
        {
            return Err(RegistryError::UnusedLaw(law.id));
        }
        previous = Some(law.id);
    }
    Ok(())
}

fn check_signature(registry: &Registry, signature: &Signature) -> Result<(), RegistryError> {
    let port = signature.port;
    if !matches!(signature.arguments, Kind::Record(_)) {
        return Err(RegistryError::Malformed(port, "arguments"));
    }
    check_kind(port, signature.arguments, 0)?;
    check_kind(port, signature.answer.payload(), 0)?;
    let footprint = signature.footprint;
    let allowed = signature.capability.allows();
    if footprint.reads.bits() & !Resources::KNOWN != 0
        || footprint.writes.bits() & !Resources::KNOWN != 0
        || !footprint.reads.within(allowed.reads)
        || !footprint.writes.within(allowed.writes)
    {
        return Err(RegistryError::Footprint(port));
    }
    if signature.laws.is_empty() {
        return Err(RegistryError::Law(port));
    }
    for (index, id) in signature.laws.iter().enumerate() {
        if signature.laws[..index].contains(id) || !registry.laws.iter().any(|law| law.id == *id) {
            return Err(RegistryError::Law(port));
        }
    }
    let store = !matches!(
        signature.capability,
        Capability::Transport | Capability::Clock | Capability::Jitter
    );
    match signature.v2 {
        V2::Api(calls) if !calls.is_empty() && calls.iter().all(|call| !call.is_empty()) => Ok(()),
        V2::External(name) if !store && !name.is_empty() => Ok(()),
        V2::OptIn if !store => Ok(()),
        _ => Err(RegistryError::Unmapped(port)),
    }
}

fn check_kind(port: Port, kind: Kind, depth: u8) -> Result<(), RegistryError> {
    if depth > MAX_DEPTH {
        return Err(RegistryError::Malformed(port, "depth"));
    }
    match kind {
        Kind::Unit | Kind::Store | Kind::Hash => Ok(()),
        Kind::Bytes(cap) if cap > 0 && cap <= MAX_BYTES => Ok(()),
        Kind::Bytes(_) => Err(RegistryError::Malformed(port, "byte cap")),
        Kind::Int(min, max) if min <= max => Ok(()),
        Kind::Int(..) => Err(RegistryError::Malformed(port, "range")),
        Kind::List(item, cap) if cap > 0 && cap <= MAX_ITEMS => check_kind(port, *item, depth + 1),
        Kind::List(..) => Err(RegistryError::Malformed(port, "item cap")),
        Kind::Choice([]) => Err(RegistryError::Malformed(port, "empty choice")),
        Kind::Record(fields) | Kind::Choice(fields) => {
            for (index, field) in fields.iter().enumerate() {
                if field.name.is_empty()
                    || !field.name.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
                    || fields[..index].iter().any(|other| other.name == field.name)
                {
                    return Err(RegistryError::Malformed(port, "name"));
                }
                check_kind(port, field.kind, depth + 1)?;
            }
            Ok(())
        }
    }
}

/// The signature of one port in a checked registry.
#[must_use]
pub fn signature(registry: &Registry, port: Port) -> Option<&'static Signature> {
    registry
        .signatures
        .iter()
        .find(|signature| signature.port == port)
}

/// Why a port value does not conform to its schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeError {
    /// The value has another shape than the schema node.
    Shape(&'static str),
    /// Bytes or items exceed the cap.
    TooLong(&'static str),
    /// An integer is outside its range.
    Range(&'static str),
}

/// Checks one value against a schema node; `name` labels the node.
pub fn conforms(kind: Kind, value: &Val<'_>, name: &'static str) -> Result<(), TypeError> {
    match (kind, value) {
        (Kind::Unit, Val::Unit) | (Kind::Store, Val::Store(_)) | (Kind::Hash, Val::Hash(_)) => {
            Ok(())
        }
        (Kind::Bytes(cap), Val::Bytes(bytes)) => match u64::try_from(bytes.len()) {
            Ok(length) if length <= cap => Ok(()),
            _ => Err(TypeError::TooLong(name)),
        },
        (Kind::Int(min, max), Val::Int(number)) if (min..=max).contains(number) => Ok(()),
        (Kind::Int(..), Val::Int(_)) => Err(TypeError::Range(name)),
        (Kind::Record(fields), Val::Record(values)) if fields.len() == values.len() => {
            for (field, value) in fields.iter().zip(values) {
                conforms(field.kind, value, field.name)?;
            }
            Ok(())
        }
        (Kind::Choice(fields), Val::Choice(index, value)) => match fields.get(*index) {
            Some(field) => conforms(field.kind, value, field.name),
            None => Err(TypeError::Shape(name)),
        },
        (Kind::List(item, cap), Val::List(values)) => {
            match u64::try_from(values.len()) {
                Ok(length) if length <= cap => {}
                _ => return Err(TypeError::TooLong(name)),
            }
            for value in values {
                conforms(*item, value, name)?;
            }
            Ok(())
        }
        _ => Err(TypeError::Shape(name)),
    }
}

/// Canonical bytes of a registry: every schema node, capability, footprint,
/// law and V2 mapping, length-framed in declaration order.
#[must_use]
pub fn canonical_bytes(registry: &Registry) -> Vec<u8> {
    let mut out = Vec::new();
    text(&mut out, registry.schema);
    count(&mut out, registry.signatures.len());
    for signature in registry.signatures {
        out.push(signature.port.tag());
        kind(&mut out, signature.arguments);
        let (shape, payload) = match signature.answer {
            Answer::Outcome(payload) => (0, payload),
            Answer::Transport(payload) => (1, payload),
            Answer::Fallible(payload) => (2, payload),
            Answer::Wait(payload) => (3, payload),
        };
        out.push(shape);
        kind(&mut out, payload);
        out.push(signature.capability as u8);
        out.push(signature.footprint.reads.bits());
        out.push(signature.footprint.writes.bits());
        count(&mut out, signature.laws.len());
        for id in signature.laws {
            out.extend_from_slice(&id.to_be_bytes());
        }
        match signature.v2 {
            V2::Api(calls) => {
                out.push(0);
                count(&mut out, calls.len());
                for call in calls {
                    text(&mut out, call);
                }
            }
            V2::External(name) => {
                out.push(1);
                text(&mut out, name);
            }
            V2::OptIn => out.push(2),
        }
    }
    count(&mut out, registry.laws.len());
    for law in registry.laws {
        out.extend_from_slice(&law.id.to_be_bytes());
        text(&mut out, law.name);
        text(&mut out, law.statement);
    }
    out
}

/// SHA-256 of [`canonical_bytes`].
#[must_use]
pub fn identity(registry: &Registry) -> Hash32 {
    RustCryptoSha256::hash(&canonical_bytes(registry))
}

/// SHA-256 of the step-function source files, each framed by its name and
/// length after the schema identifier.
#[must_use]
pub fn source_digest(files: &[(&str, &[u8])]) -> Hash32 {
    let mut out = Vec::new();
    text(&mut out, SCHEMA);
    count(&mut out, files.len());
    for (name, bytes) in files {
        text(&mut out, name);
        count(&mut out, bytes.len());
        out.extend_from_slice(bytes);
    }
    RustCryptoSha256::hash(&out)
}

/// The step-function source this library compiles.
#[must_use]
pub fn program_source() -> Hash32 {
    source_digest(&[("program.rs", include_bytes!("program.rs"))])
}

fn count(out: &mut Vec<u8>, length: usize) {
    // Lengths come from static data and allocations; usize always fits u64 here.
    out.extend_from_slice(&(length as u64).to_be_bytes());
}

fn text(out: &mut Vec<u8>, value: &str) {
    count(out, value.len());
    out.extend_from_slice(value.as_bytes());
}

fn kind(out: &mut Vec<u8>, node: Kind) {
    match node {
        Kind::Unit => out.push(0),
        Kind::Store => out.push(1),
        Kind::Hash => out.push(2),
        Kind::Bytes(cap) => {
            out.push(3);
            out.extend_from_slice(&cap.to_be_bytes());
        }
        Kind::Int(min, max) => {
            out.push(4);
            out.extend_from_slice(&min.to_be_bytes());
            out.extend_from_slice(&max.to_be_bytes());
        }
        Kind::Record(fields) | Kind::Choice(fields) => {
            out.push(if matches!(node, Kind::Record(_)) {
                5
            } else {
                6
            });
            count(out, fields.len());
            for field in fields {
                text(out, field.name);
                kind(out, field.kind);
            }
        }
        Kind::List(item, cap) => {
            out.push(7);
            out.extend_from_slice(&cap.to_be_bytes());
            kind(out, *item);
        }
    }
}
