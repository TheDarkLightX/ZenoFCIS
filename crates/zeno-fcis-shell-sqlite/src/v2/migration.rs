//! Data-migrating upgrades: a declared migration of the state from one
//! version of a lineage to the next, admitted by forward simulation over the
//! whole declared input domain, and the exact rename tier.
//!
//! A [`Migration`](crate::v2::migration::Migration) gives every field of the
//! new state record a value from the old state: an old field's value,
//! carried over or renamed; a constant, the default of an added field; or a
//! total table over one old field's values, one part of a split. Values are
//! the numbers decision examples write: 0 or 1 for a Boolean, the integer
//! itself, a variant's ID.
//!
//! [`simulate`](crate::v2::migration::simulate) admits a migration from one
//! bound catalog to another only by forward simulation. It enumerates every
//! state of the old contract's declared state domain and keeps those on
//! which every state law of the old contract holds, as the library's law
//! evaluator decides through the core's genesis framing (see
//! [`behaviour`](crate::v2::behaviour)). A store's state satisfies them
//! whichever way it got there: its genesis and every commit are checked
//! against them, a behaviour-change upgrade checks the new laws on the
//! state it keeps, a rename keeps the laws, and the second check below makes
//! a migrated state satisfy the new laws. For each kept state `s` it checks
//! that:
//!
//! - when the old contract's genesis evaluation admits `s`, the new
//!   contract's admits `m(s)`; for a generated contract, whose law 990
//!   admits only its declared genesis state, this is "`m` maps the old
//!   genesis state to the new one";
//! - every state law of the new contract holds on `m(s)`;
//! - for every command and context of the declared domain, the new
//!   contract's publication over `m(s)` gives the same observations as the
//!   old contract's over `s`: the same technical refusal, or the same
//!   decision class and reason, the same deliveries in order (lane, ordinal,
//!   channel, destination, payload and idempotency value, as the library's
//!   candidate holds them), and for a commit a successor state equal to `m`
//!   of the old successor, byte for byte as the store would hold it.
//!
//! A migration is checked only when the whole domain, every state, command
//! and context, has at most
//! [`MAX_INPUT_TUPLES`](crate::v2::migration::MAX_INPUT_TUPLES) tuples, the
//! cap of `zeno-fcis contract review`. A larger domain is refused as
//! inconclusive: no sampling, boundary set or solver result stands in for
//! the enumeration.
//!
//! What a successful simulation establishes, with the old genesis mapped and
//! every kept state covered: every state the old contract can reach maps to
//! one on which every state law of the new contract holds, and from a
//! mapped state the new contract makes the same decisions and sends the same deliveries. It covers those
//! observations and no others: sealed identities, certificates, the delivery
//! IDs of later commits and Step usage may differ.
//!
//! The rename tier admits a change of names alone without any simulation:
//! the new catalog's policy, with the old schema and its commitments in
//! place of its own, must be byte for byte the old policy
//! ([`rename_exact`](crate::v2::migration::rename_exact)). The state's
//! payload is then kept and only its envelope is framed again under the new
//! schema ([`reframe`](crate::v2::migration::reframe)).
//!
//! Pure: no SQLite, I/O, clock or threads. The file depends on
//! `zeno-fcis-synthesis` and its sibling `behaviour` alone, so the CLI
//! compiles both from these sources and runs exactly this simulation in
//! `zeno-fcis contract evolve`.

use std::fmt;

use zeno_fcis_synthesis::finite::{
    V2InputField as InputField, V2InputLeaf as Leaf, V2Resource as Resource,
    canonical_v2::{envelope, output},
    project_record_v2,
    v2_authority::{self as authority, Authority, PublicationOutcome},
    v2_catalog::BoundCatalog,
    v2_composition::{self as c, FrameBinding, Framing, Raw, Schema},
    v2_zero_limits,
};

use super::behaviour;

/// The cap on the input tuples one simulation enumerates: `zeno-fcis
/// contract review`'s, 2^20.
pub const MAX_INPUT_TUPLES: u64 = 1 << 20;

/// Where one field of the new state takes its value from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Source<'m> {
    /// This old field's value: a field carried over, or renamed.
    Field(u16),
    /// This value: an added field's default.
    Value(i128),
    /// The value this table gives for the old field's value: one part of a
    /// split. The table lists `(old value, new value)` pairs in strictly
    /// increasing order of old value and must cover the old field's domain.
    Table {
        /// The old field.
        field: u16,
        /// The pairs.
        cases: &'m [(i128, i128)],
    },
}

/// One field of the new state record and where its value comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Target<'m> {
    /// The new field's ID.
    pub id: u16,
    /// Where its value comes from.
    pub source: Source<'m>,
}

/// A declared state migration: every field of the new state record, in
/// strictly increasing ID order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Migration<'m> {
    /// The new state's fields.
    pub fields: &'m [Target<'m>],
}

/// The magic that opens [`Migration::encode`]'s bytes.
const MAGIC: &[u8] = b"ZFCIS-MIGRATION\0";
/// The encoding's format.
const FORMAT: u8 = 1;

impl Migration<'_> {
    /// The canonical encoding an upgrade record binds by its SHA-256: the
    /// magic `ZFCIS-MIGRATION\0`, the format byte 1, a big-endian `u32`
    /// count of fields, then each field's big-endian `u16` ID and its
    /// source: tag 0 and the old field's `u16` ID; tag 1 and the value as a
    /// big-endian `i128`; or tag 2, the old field's `u16` ID, a `u32` count
    /// of pairs and each pair as two big-endian `i128`s.
    ///
    /// # Errors
    /// `Range` for more than `u32::MAX` fields or pairs.
    pub fn encode(&self) -> Result<Vec<u8>, Range> {
        let mut bytes = MAGIC.to_vec();
        bytes.push(FORMAT);
        bytes.extend_from_slice(&count(self.fields.len())?);
        for target in self.fields {
            bytes.extend_from_slice(&target.id.to_be_bytes());
            match target.source {
                Source::Field(field) => {
                    bytes.push(0);
                    bytes.extend_from_slice(&field.to_be_bytes());
                }
                Source::Value(value) => {
                    bytes.push(1);
                    bytes.extend_from_slice(&value.to_be_bytes());
                }
                Source::Table { field, cases } => {
                    bytes.push(2);
                    bytes.extend_from_slice(&field.to_be_bytes());
                    bytes.extend_from_slice(&count(cases.len())?);
                    for (old, new) in cases {
                        bytes.extend_from_slice(&old.to_be_bytes());
                        bytes.extend_from_slice(&new.to_be_bytes());
                    }
                }
            }
        }
        Ok(bytes)
    }
}

/// A length that does not fit the encoding's `u32` count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range;

fn count(len: usize) -> Result<[u8; 4], Range> {
    Ok(u32::try_from(len).map_err(|_| Range)?.to_be_bytes())
}

/// Why a migration does not fit the two state schemas.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Defect {
    /// The new state field has no source.
    Missing {
        /// The new field.
        field: u16,
    },
    /// The migration names a new field the new state does not have, or
    /// names a field twice or out of order.
    Unknown {
        /// The field named.
        field: u16,
    },
    /// A source names a field the old state does not have.
    Source {
        /// The old field named.
        field: u16,
    },
    /// The old state field is not carried into the new state: a migration
    /// renames, adds and splits fields, and drops none.
    Dropped {
        /// The old field.
        field: u16,
    },
    /// A table's pairs are not in strictly increasing order of old value.
    Table {
        /// The new field whose table it is.
        field: u16,
    },
}

impl fmt::Display for Defect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { field } => write!(f, "new state field {field} is given no value"),
            Self::Unknown { field } => write!(
                f,
                "field {field} is not a field of the new state, or is named twice or out of order"
            ),
            Self::Source { field } => write!(f, "old state field {field} does not exist"),
            Self::Dropped { field } => write!(
                f,
                "old state field {field} is not carried into the new state, and a migration drops no field"
            ),
            Self::Table { field } => write!(
                f,
                "the table of new field {field} does not list its old values in strictly increasing order"
            ),
        }
    }
}

/// The observations a simulation compares, in the order it compares them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Observation {
    /// A genesis state of the old contract maps to a state the new
    /// contract's genesis evaluation refuses.
    Genesis,
    /// A state on which every state law of the old contract holds maps to
    /// a state on which a state law of the new contract does not hold.
    StateLaws,
    /// The decision class and reason, or the technical refusal.
    Decision,
    /// The deliveries, in order: lane, ordinal, channel, destination,
    /// payload and idempotency value.
    Deliveries,
    /// The successor state, against the migrated old successor.
    Successor,
    /// For a shortcut, the state it maps to, against the state the composed
    /// route maps to.
    MappedState,
}

impl Observation {
    /// Every observation, in comparison order.
    pub const ALL: [Self; 5] = [
        Self::Genesis,
        Self::StateLaws,
        Self::Decision,
        Self::Deliveries,
        Self::Successor,
    ];

    /// The observation's name in reports.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Genesis => "genesis",
            Self::StateLaws => "new-state-laws",
            Self::Decision => "decision-class-and-reason",
            Self::Deliveries => "deliveries",
            Self::Successor => "successor-state",
            Self::MappedState => "mapped-state",
        }
    }

    fn bit(self) -> u8 {
        match self {
            Self::Genesis => 1,
            Self::Decision => 2,
            Self::Deliveries => 4,
            Self::Successor => 8,
            Self::MappedState => 16,
            Self::StateLaws => 32,
        }
    }
}

/// Why a simulation did not admit a migration. `ordinal` is a position in
/// the enumeration of the old contract's whole input domain, counting from
/// 0, which [`inputs_at`] turns back into field values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Unsimulated {
    /// A root of either contract is not a flat record or scalar of finite
    /// leaves, the state is not a record, or the two contracts' command or
    /// context schemas differ.
    Shape,
    /// The migration does not fit the two state schemas.
    Migration(Defect),
    /// The migration gives this new field a value outside its domain, or a
    /// table has no pair for the old value, at the state of tuple `ordinal`.
    Value {
        /// The new field.
        field: u16,
        /// A tuple with that state.
        ordinal: u64,
    },
    /// The migrated state of tuple `ordinal`, every value inside its new
    /// field's domain, has no encoding under the new state root's framing,
    /// for instance because it exceeds the root's byte bound.
    Unframed {
        /// A tuple with that state.
        ordinal: u64,
    },
    /// The domain has more tuples than the cap; `size` is `None` when the
    /// product exceeds `u128::MAX`. Nothing was evaluated.
    DomainTooLarge {
        /// The exact number of tuples, when representable.
        size: Option<u128>,
        /// The cap.
        cap: u64,
    },
    /// This observation differs at tuple `ordinal`.
    Differs {
        /// The first observation that differs there.
        observation: Observation,
        /// The tuple.
        ordinal: u64,
    },
}

impl fmt::Display for Unsimulated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shape => f.write_str(
                "the contracts' roots are not flat records of finite values, or their command or context schemas differ",
            ),
            Self::Migration(defect) => write!(f, "the migration does not fit the state schemas: {defect}"),
            Self::Value { field, ordinal } => write!(
                f,
                "the migration gives new field {field} no value in its domain at the state of input tuple {ordinal}"
            ),
            Self::Unframed { ordinal } => write!(
                f,
                "the migrated state at the state of input tuple {ordinal} has no encoding under the new contract's state framing"
            ),
            Self::DomainTooLarge { size: Some(size), cap } => write!(
                f,
                "the old contract's input domain has {size} tuples, more than the cap of {cap}, so the simulation is inconclusive and admits nothing"
            ),
            Self::DomainTooLarge { size: None, cap } => write!(
                f,
                "the old contract's input domain has more than 2^128 tuples, more than the cap of {cap}, so the simulation is inconclusive and admits nothing"
            ),
            Self::Differs {
                observation: Observation::Genesis,
                ordinal,
            } => write!(
                f,
                "the old contract's genesis state at input tuple {ordinal} maps to a state the new contract's genesis laws refuse"
            ),
            Self::Differs {
                observation: Observation::StateLaws,
                ordinal,
            } => write!(
                f,
                "the state at input tuple {ordinal} satisfies every state law of the old contract, but the migration maps it to a state on which a state law of the new contract does not hold"
            ),
            Self::Differs {
                observation,
                ordinal,
            } => write!(
                f,
                "the observation `{}` differs between the old and the new contract at input tuple {ordinal}, so this is a behaviour change, not a migration",
                observation.tag()
            ),
        }
    }
}

/// What a simulation compared: every count is exact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Simulated {
    states: u64,
    admitted: u64,
    genesis: u64,
    tuples: u64,
}

impl Simulated {
    /// The states of the old contract's declared state domain.
    pub fn states(self) -> u64 {
        self.states
    }

    /// The states on which every state law of the old contract holds.
    pub fn admitted(self) -> u64 {
        self.admitted
    }

    /// The admitted states the old contract's genesis evaluation admits.
    pub fn genesis(self) -> u64 {
        self.genesis
    }

    /// The input tuples compared: each admitted state with every command
    /// and context of the domain.
    pub fn tuples(self) -> u64 {
        self.tuples
    }

    /// The observations compared, as a bit set: genesis 1, decision class
    /// and reason 2, deliveries 4, successor state 8, and 32 for the new
    /// contract's state laws on every migrated state.
    pub fn observations(self) -> u8 {
        Observation::ALL
            .iter()
            .fold(0, |bits, observation| bits | observation.bit())
    }
}

/// One root as the simulation frames it: its fields' IDs and leaves, or one
/// scalar leaf, and its envelope binding.
struct Root<'d> {
    record: bool,
    ids: Vec<u16>,
    leaves: Vec<&'d Leaf>,
    binding: FrameBinding,
}

impl<'d> Root<'d> {
    fn of(schema: Schema<'d>, binding: FrameBinding) -> Option<Self> {
        let (record, ids, leaves) = match schema {
            Schema::Record(fields) => (
                true,
                fields.iter().map(|field| field.id).collect(),
                fields.iter().map(|field| &field.leaf).collect::<Vec<_>>(),
            ),
            Schema::Leaf(leaf) => (false, vec![0], vec![leaf]),
            _ => return None,
        };
        if leaves.iter().any(|leaf| bounds(leaf).is_none()) {
            return None;
        }
        Some(Self {
            record,
            ids,
            leaves,
            binding,
        })
    }

    /// The number of values; `None` beyond `u128`.
    fn size(&self) -> Option<u128> {
        self.leaves.iter().try_fold(1u128, |size, leaf| {
            let (min, max) = bounds(leaf)?;
            let width = u128::try_from(i128::from(max) - i128::from(min)).ok()?;
            size.checked_mul(width.checked_add(1)?)
        })
    }

    /// The first value in enumeration order.
    fn first(&self) -> Vec<i64> {
        self.leaves
            .iter()
            .map(|leaf| bounds(leaf).map_or(0, |(min, _)| min))
            .collect()
    }

    /// Steps `codes` to the next value, last field fastest; false after the
    /// last value, when `codes` is the first one again.
    fn next(&self, codes: &mut [i64]) -> bool {
        for (code, leaf) in codes.iter_mut().zip(&self.leaves).rev() {
            let Some((min, max)) = bounds(leaf) else {
                return false;
            };
            if *code < max {
                *code += 1;
                return true;
            }
            *code = min;
        }
        false
    }

    /// The value at `index` in enumeration order.
    fn at(&self, mut index: u128) -> Option<Vec<i64>> {
        let mut codes = vec![0; self.leaves.len()];
        for (code, leaf) in codes.iter_mut().zip(&self.leaves).rev() {
            let (min, max) = bounds(leaf)?;
            let width = u128::try_from(i128::from(max) - i128::from(min)).ok()? + 1;
            *code = i64::try_from(i128::from(min) + i128::try_from(index % width).ok()?).ok()?;
            index /= width;
        }
        (index == 0).then_some(codes)
    }

    /// The envelope of the value with these codes.
    fn frame(&self, codes: &[i64]) -> Option<Vec<u8>> {
        let max = usize::try_from(self.binding.max_bytes).ok()?;
        let payload = if self.record {
            let fields = self
                .ids
                .iter()
                .zip(&self.leaves)
                .zip(codes)
                .map(|((id, leaf), code)| {
                    Some(c::Field {
                        id: *id,
                        value: atom(leaf, *code)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            output::encode_record(&fields, max).ok()?
        } else {
            output::encode_atom(atom(self.leaves.first()?, *codes.first()?)?, max).ok()?
        };
        output::encode_envelope(self.binding.root, &self.binding.schema, &payload, max).ok()
    }

    /// The codes of a record envelope's fields; `None` unless `bytes` is a
    /// complete envelope of this root.
    fn decode(&self, bytes: &[u8], fields: &[InputField]) -> Option<Vec<i64>> {
        if !self.record {
            return None;
        }
        let payload = envelope::frame(
            bytes,
            self.binding.root,
            &self.binding.schema,
            self.binding.max_bytes,
        )
        .ok()?;
        let limits = [
            Resource::Read,
            Resource::Write,
            Resource::Candidate,
            Resource::Effect,
            Resource::Byte,
            Resource::WitnessByte,
            Resource::Depth,
            Resource::Step,
        ]
        .into_iter()
        .fold(v2_zero_limits(), |limits, resource| {
            limits.with_limit(resource, u64::MAX)
        });
        let (codes, _, _) = project_record_v2(payload.bytes(), fields, limits).into_parts();
        let codes = codes.ok()?;
        (codes.len() == self.leaves.len()).then_some(codes)
    }

    /// Field IDs with the numbers of these codes.
    fn numbers(&self, codes: &[i64]) -> Vec<(u16, i128)> {
        self.ids
            .iter()
            .zip(&self.leaves)
            .zip(codes)
            .map(|((id, leaf), code)| (*id, number(leaf, *code).unwrap_or(i128::from(*code))))
            .collect()
    }
}

/// The inclusive code interval of a leaf; `None` for a leaf without one.
fn bounds(leaf: &Leaf) -> Option<(i64, i64)> {
    let (min, max) = match leaf {
        Leaf::Bool => (0, 1),
        Leaf::I128 { min, max } => (*min, *max),
        Leaf::U128 { min, max } => (i64::try_from(*min).ok()?, i64::try_from(*max).ok()?),
        Leaf::Enum { min, max, .. } | Leaf::Sum { min, max, .. } => (*min, *max),
        _ => return None,
    };
    (min <= max).then_some((min, max))
}

/// The number a leaf's code stands for: 0 or 1, the integer, or the
/// variant's ID.
fn number(leaf: &Leaf, code: i64) -> Option<i128> {
    match leaf {
        Leaf::Bool | Leaf::I128 { .. } | Leaf::U128 { .. } => Some(i128::from(code)),
        Leaf::Enum { variants, .. } | Leaf::Sum { variants, .. } => variants
            .iter()
            .find(|variant| variant.code == code)
            .map(|variant| i128::from(variant.id)),
        _ => None,
    }
}

/// The code of a number in a leaf's domain; `None` outside it.
fn code(leaf: &Leaf, number: i128) -> Option<i64> {
    let code = match leaf {
        Leaf::Bool | Leaf::I128 { .. } | Leaf::U128 { .. } => i64::try_from(number).ok()?,
        Leaf::Enum { variants, .. } | Leaf::Sum { variants, .. } => {
            variants
                .iter()
                .find(|variant| i128::from(variant.id) == number)?
                .code
        }
        _ => return None,
    };
    let (min, max) = bounds(leaf)?;
    (min..=max).contains(&code).then_some(code)
}

/// The library value of a code.
fn atom(leaf: &Leaf, code: i64) -> Option<c::Atom<'static>> {
    Some(match leaf {
        Leaf::Bool => c::Atom::Bool(match code {
            0 => false,
            1 => true,
            _ => return None,
        }),
        Leaf::I128 { .. } => c::Atom::I128(i128::from(code)),
        Leaf::U128 { .. } => c::Atom::U128(u128::try_from(code).ok()?),
        Leaf::Enum {
            type_id, variants, ..
        } => c::Atom::Enum {
            type_id: *type_id,
            variant: variants.iter().find(|variant| variant.code == code)?.id,
        },
        Leaf::Sum {
            type_id, variants, ..
        } => c::Atom::Sum {
            type_id: *type_id,
            variant: variants.iter().find(|variant| variant.code == code)?.id,
        },
        _ => return None,
    })
}

/// The three roots of a catalog.
fn roots<'d>(catalog: &BoundCatalog<'d>) -> Option<[Root<'d>; 3]> {
    let (d, framing): (_, &Framing) = (catalog.descriptor(), catalog.framing());
    Some([
        Root::of(d.state, framing.state)?,
        Root::of(d.command, framing.command)?,
        Root::of(d.context, framing.context)?,
    ])
}

/// The state's record fields.
fn state_fields<'d>(catalog: &BoundCatalog<'d>) -> Option<&'d [InputField]> {
    match catalog.descriptor().state {
        Schema::Record(fields) => Some(fields),
        _ => None,
    }
}

/// Two roots frame the same values the same way, up to the envelope's
/// schema commitment.
fn same_shape(old: &Root<'_>, new: &Root<'_>) -> bool {
    old.record == new.record
        && old.ids == new.ids
        && old.leaves == new.leaves
        && old.binding.root == new.binding.root
}

/// Checks that `migration` fits the two state records.
fn fits(migration: &Migration<'_>, old: &Root<'_>, new: &Root<'_>) -> Result<(), Defect> {
    if let Some(pair) = migration
        .fields
        .windows(2)
        .find(|pair| pair[0].id >= pair[1].id)
    {
        return Err(Defect::Unknown { field: pair[1].id });
    }
    let mut targets = migration.fields.iter().peekable();
    for id in &new.ids {
        match targets.peek() {
            Some(target) if target.id == *id => {
                targets.next();
            }
            Some(target) if target.id < *id => return Err(Defect::Unknown { field: target.id }),
            _ => return Err(Defect::Missing { field: *id }),
        }
    }
    if let Some(target) = targets.next() {
        return Err(Defect::Unknown { field: target.id });
    }
    let mut used = vec![false; old.ids.len()];
    for target in migration.fields {
        let field = match target.source {
            Source::Field(field) => field,
            Source::Table { field, cases } => {
                if cases.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
                    return Err(Defect::Table { field: target.id });
                }
                field
            }
            Source::Value(_) => continue,
        };
        let position = old
            .ids
            .iter()
            .position(|id| *id == field)
            .ok_or(Defect::Source { field })?;
        used[position] = true;
    }
    match old.ids.iter().zip(&used).find(|(_, used)| !**used) {
        Some((field, _)) => Err(Defect::Dropped { field: *field }),
        None => Ok(()),
    }
}

/// The new state's codes for the old state's, or the new field that gets no
/// value in its domain.
fn apply(
    migration: &Migration<'_>,
    old: &Root<'_>,
    new: &Root<'_>,
    codes: &[i64],
) -> Result<Vec<i64>, u16> {
    let value = |field: u16| {
        let position = old.ids.iter().position(|id| *id == field)?;
        number(old.leaves[position], codes[position])
    };
    migration
        .fields
        .iter()
        .zip(&new.leaves)
        .map(|(target, leaf)| {
            let number = match target.source {
                Source::Field(field) => value(field),
                Source::Value(number) => Some(number),
                Source::Table { field, cases } => value(field).and_then(|old| {
                    cases
                        .binary_search_by_key(&old, |(key, _)| *key)
                        .ok()
                        .map(|index| cases[index].1)
                }),
            };
            number
                .and_then(|number| code(leaf, number))
                .ok_or(target.id)
        })
        .collect()
}

/// The migrated state for `state`, an exact store state under `from`, as
/// the store will hold it under `to`.
///
/// # Errors
/// `Shape` for roots the migration cannot use or a state that is not one of
/// `from`'s, `Migration` when the migration does not fit, and `Value` (at
/// tuple 0) for a value outside a new field's domain.
pub fn migrate_state(
    from: &BoundCatalog<'_>,
    to: &BoundCatalog<'_>,
    migration: &Migration<'_>,
    state: &[u8],
) -> Result<Vec<u8>, Unsimulated> {
    let ([old, ..], [new, ..]) = (
        roots(from).ok_or(Unsimulated::Shape)?,
        roots(to).ok_or(Unsimulated::Shape)?,
    );
    if !old.record || !new.record {
        return Err(Unsimulated::Shape);
    }
    fits(migration, &old, &new).map_err(Unsimulated::Migration)?;
    let fields = state_fields(from).ok_or(Unsimulated::Shape)?;
    let codes = old.decode(state, fields).ok_or(Unsimulated::Shape)?;
    let migrated = apply(migration, &old, &new, &codes)
        .map_err(|field| Unsimulated::Value { field, ordinal: 0 })?;
    new.frame(&migrated).ok_or(Unsimulated::Shape)
}

/// The first observation in which the new publication differs from the
/// old one; `None` when they agree. `migrate` maps an old successor state.
fn differs(
    old: &PublicationOutcome<'_>,
    new: &PublicationOutcome<'_>,
    migrate: impl Fn(&[u8]) -> Option<Vec<u8>>,
) -> Option<Observation> {
    let decisions = |old: &authority::Evaluation<'_>, new: &authority::Evaluation<'_>| match (
        old.result(),
        new.result(),
    ) {
        (Ok(old), Ok(new)) => {
            if old.class() != new.class() || old.reason() != new.reason() {
                Some(Observation::Decision)
            } else if old.effects() != new.effects() || old.outbox() != new.outbox() {
                Some(Observation::Deliveries)
            } else {
                None
            }
        }
        _ => Some(Observation::Decision),
    };
    match (old, new) {
        (PublicationOutcome::Commit(old), PublicationOutcome::Commit(new)) => {
            decisions(old.evaluation(), new.evaluation()).or_else(|| {
                (migrate(old.poststate()).as_deref() != Some(new.poststate()))
                    .then_some(Observation::Successor)
            })
        }
        (PublicationOutcome::Reject(old), PublicationOutcome::Reject(new)) => decisions(old, new),
        (
            PublicationOutcome::Refused { error: old, .. },
            PublicationOutcome::Refused { error: new, .. },
        ) => (old != new).then_some(Observation::Decision),
        _ => Some(Observation::Decision),
    }
}

/// Admits `migration` from `from`, bound as `from_authority`, to `to`, bound
/// as `to_authority`, by forward simulation over the whole declared input
/// domain; see the module documentation.
///
/// Refusal order: shapes, the migration's fit, the cap, then in
/// enumeration order a value outside a new field's domain or the first
/// observation that differs.
///
/// # Errors
/// Why the migration is not admitted.
pub fn simulate(
    from: &BoundCatalog<'_>,
    from_authority: &Authority<'_>,
    to: &BoundCatalog<'_>,
    to_authority: &Authority<'_>,
    migration: &Migration<'_>,
) -> Result<Simulated, Unsimulated> {
    let [old_state, old_command, old_context] = roots(from).ok_or(Unsimulated::Shape)?;
    let [new_state, new_command, new_context] = roots(to).ok_or(Unsimulated::Shape)?;
    if !old_state.record
        || !new_state.record
        || !same_shape(&old_command, &new_command)
        || !same_shape(&old_context, &new_context)
    {
        return Err(Unsimulated::Shape);
    }
    fits(migration, &old_state, &new_state).map_err(Unsimulated::Migration)?;
    let sizes = [old_state.size(), old_command.size(), old_context.size()];
    let size = sizes
        .iter()
        .try_fold(1u128, |product, size| product.checked_mul((*size)?));
    let too_large = Unsimulated::DomainTooLarge {
        size,
        cap: MAX_INPUT_TUPLES,
    };
    let size = size
        .and_then(|size| u64::try_from(size).ok())
        .filter(|size| *size <= MAX_INPUT_TUPLES)
        .ok_or(too_large)?;
    let states = sizes[0]
        .and_then(|size| u64::try_from(size).ok())
        .ok_or(too_large)?;
    let inputs = size.checked_div(states).unwrap_or(0);
    let fields = state_fields(from).ok_or(Unsimulated::Shape)?;
    let migrate = |bytes: &[u8]| {
        let codes = old_state.decode(bytes, fields)?;
        new_state.frame(&apply(migration, &old_state, &new_state, &codes).ok()?)
    };
    let mut simulated = Simulated {
        states,
        admitted: 0,
        genesis: 0,
        tuples: 0,
    };
    let mut state = old_state.first();
    for index in 0..states {
        let first = index * inputs;
        let old_bytes = old_state.frame(&state).ok_or(Unsimulated::Shape)?;
        // Only a state every state law holds on can be reached.
        if behaviour::check(from, &[], &old_bytes).is_ok() {
            simulated.admitted += 1;
            let new_codes = apply(migration, &old_state, &new_state, &state).map_err(|field| {
                Unsimulated::Value {
                    field,
                    ordinal: first,
                }
            })?;
            let new_bytes = new_state
                .frame(&new_codes)
                .ok_or(Unsimulated::Unframed { ordinal: first })?;
            if matches!(
                from_authority.publish_genesis(&old_bytes),
                PublicationOutcome::Commit(_)
            ) {
                simulated.genesis += 1;
                if !matches!(
                    to_authority.publish_genesis(&new_bytes),
                    PublicationOutcome::Commit(_)
                ) {
                    return Err(Unsimulated::Differs {
                        observation: Observation::Genesis,
                        ordinal: first,
                    });
                }
            }
            // Every state a store under `from` can hold is a kept state, so
            // this makes every state a store holds after the migration one
            // that `to`'s state laws hold on, whatever route led to it.
            if behaviour::check(to, &[], &new_bytes).is_err() {
                return Err(Unsimulated::Differs {
                    observation: Observation::StateLaws,
                    ordinal: first,
                });
            }
            let mut command = old_command.first();
            let mut position = first;
            loop {
                let mut context = old_context.first();
                loop {
                    let frames = (
                        old_command.frame(&command),
                        new_command.frame(&command),
                        old_context.frame(&context),
                        new_context.frame(&context),
                    );
                    let (Some(old_c), Some(new_c), Some(old_x), Some(new_x)) = frames else {
                        return Err(Unsimulated::Shape);
                    };
                    let old = from_authority.publish(Raw {
                        state: &old_bytes,
                        command: &old_c,
                        context: &old_x,
                    });
                    let new = to_authority.publish(Raw {
                        state: &new_bytes,
                        command: &new_c,
                        context: &new_x,
                    });
                    if let Some(observation) = differs(&old, &new, migrate) {
                        return Err(Unsimulated::Differs {
                            observation,
                            ordinal: position,
                        });
                    }
                    simulated.tuples += 1;
                    position += 1;
                    if !old_context.next(&mut context) {
                        break;
                    }
                }
                if !old_command.next(&mut command) {
                    break;
                }
            }
        }
        old_state.next(&mut state);
    }
    Ok(simulated)
}

/// Checks a shortcut: a migration `shortcut` from `from` directly to `to`,
/// bound as `to_authority`, against `route`, which maps a state of `from`
/// along the composed route of consecutive steps. On every state of `from`'s
/// declared state domain on which `from`'s state laws hold, and for every
/// command and context of the domain, the new contract's publication over
/// the shortcut's state must give the same observations as over the route's
/// state, every one [`simulate`] compares; then the two states must be the
/// same. Returns the input tuples compared. The same cap applies.
///
/// # Errors
/// As [`simulate`]; a state the route does not map is a difference in the
/// mapped state.
pub fn agrees(
    from: &BoundCatalog<'_>,
    to: &BoundCatalog<'_>,
    to_authority: &Authority<'_>,
    shortcut: &Migration<'_>,
    route: impl Fn(&[u8]) -> Option<Vec<u8>>,
) -> Result<u64, Unsimulated> {
    let [old_state, old_command, old_context] = roots(from).ok_or(Unsimulated::Shape)?;
    let [new_state, new_command, new_context] = roots(to).ok_or(Unsimulated::Shape)?;
    if !old_state.record || !new_state.record {
        return Err(Unsimulated::Shape);
    }
    let same = |old: &Root<'_>, new: &Root<'_>| {
        old.record == new.record && old.ids == new.ids && old.leaves == new.leaves
    };
    if !same(&old_command, &new_command) || !same(&old_context, &new_context) {
        return Err(Unsimulated::Shape);
    }
    fits(shortcut, &old_state, &new_state).map_err(Unsimulated::Migration)?;
    let sizes = [old_state.size(), new_command.size(), new_context.size()];
    let size = sizes
        .iter()
        .try_fold(1u128, |product, size| product.checked_mul((*size)?));
    let too_large = Unsimulated::DomainTooLarge {
        size,
        cap: MAX_INPUT_TUPLES,
    };
    let size = size
        .and_then(|size| u64::try_from(size).ok())
        .filter(|size| *size <= MAX_INPUT_TUPLES)
        .ok_or(too_large)?;
    let states = sizes[0]
        .and_then(|size| u64::try_from(size).ok())
        .ok_or(too_large)?;
    let inputs = size.checked_div(states).unwrap_or(0);
    let mut tuples = 0;
    let mut state = old_state.first();
    for index in 0..states {
        let first = index * inputs;
        let old_bytes = old_state.frame(&state).ok_or(Unsimulated::Shape)?;
        if behaviour::check(from, &[], &old_bytes).is_ok() {
            let codes = apply(shortcut, &old_state, &new_state, &state).map_err(|field| {
                Unsimulated::Value {
                    field,
                    ordinal: first,
                }
            })?;
            let mapped = new_state.frame(&codes).ok_or(Unsimulated::Shape)?;
            let routed = route(&old_bytes).ok_or(Unsimulated::Differs {
                observation: Observation::MappedState,
                ordinal: first,
            })?;
            let mut command = new_command.first();
            let mut position = first;
            loop {
                let mut context = new_context.first();
                loop {
                    let (Some(c), Some(x)) =
                        (new_command.frame(&command), new_context.frame(&context))
                    else {
                        return Err(Unsimulated::Shape);
                    };
                    let along = to_authority.publish(Raw {
                        state: &routed,
                        command: &c,
                        context: &x,
                    });
                    let direct = to_authority.publish(Raw {
                        state: &mapped,
                        command: &c,
                        context: &x,
                    });
                    if let Some(observation) =
                        differs(&along, &direct, |bytes| Some(bytes.to_vec()))
                    {
                        return Err(Unsimulated::Differs {
                            observation,
                            ordinal: position,
                        });
                    }
                    tuples += 1;
                    position += 1;
                    if !new_context.next(&mut context) {
                        break;
                    }
                }
                if !new_command.next(&mut command) {
                    break;
                }
            }
            if mapped != routed {
                return Err(Unsimulated::Differs {
                    observation: Observation::MappedState,
                    ordinal: first,
                });
            }
        }
        old_state.next(&mut state);
    }
    Ok(tuples)
}

/// The state, command and context field values of tuple `ordinal` in the
/// enumeration of `from`'s input domain, each as field IDs with numbers; a
/// scalar root has field 0. `None` beyond the domain.
pub fn inputs_at(from: &BoundCatalog<'_>, ordinal: u64) -> Option<[Vec<(u16, i128)>; 3]> {
    let [state, command, context] = roots(from)?;
    let (commands, contexts) = (command.size()?, context.size()?);
    let ordinal = u128::from(ordinal);
    let inputs = commands.checked_mul(contexts)?;
    let state_codes = state.at(ordinal / inputs)?;
    let command_codes = command.at(ordinal % inputs / contexts)?;
    let context_codes = context.at(ordinal % contexts)?;
    Some([
        state.numbers(&state_codes),
        command.numbers(&command_codes),
        context.numbers(&context_codes),
    ])
}

/// Whether `to` differs from `from` in its original schema alone: `to`'s
/// complete canonical policy, with `from`'s original schema and schema
/// commitments in place of its own, is byte for byte `from`'s policy, so
/// every descriptor field, channel link and framing size limit is `from`'s.
/// `zeno-fcis contract diff` decides the rename kind with this comparison.
///
/// # Errors
/// `Range` when the policy cannot be encoded.
pub fn rename_exact(from: &BoundCatalog<'_>, to: &BoundCatalog<'_>) -> Result<bool, Range> {
    let (old, new) = (from.framing(), to.framing());
    let frame = |binding: FrameBinding, commitment: FrameBinding| FrameBinding {
        schema: commitment.schema,
        ..binding
    };
    let framing = Framing {
        state: frame(new.state, old.state),
        command: frame(new.command, old.command),
        context: frame(new.context, old.context),
    };
    let policy = authority::policy_bytes(
        to.descriptor(),
        from.original_schema(),
        &framing,
        to.channel_roots(),
    )
    .ok_or(Range)?;
    Ok(policy == from.original_contract() && from.original_schema() != to.original_schema())
}

/// The state envelope `state`, exact under `from`, framed again under `to`
/// with the same payload; `None` when `state` is not a complete envelope of
/// `from`'s state root.
pub fn reframe(from: &BoundCatalog<'_>, to: &BoundCatalog<'_>, state: &[u8]) -> Option<Vec<u8>> {
    let (old, new) = (from.framing().state, to.framing().state);
    let payload = envelope::frame(state, old.root, &old.schema, old.max_bytes).ok()?;
    output::encode_envelope(
        new.root,
        &new.schema,
        payload.bytes(),
        usize::try_from(new.max_bytes).ok()?,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_encoding_binds_every_field_source_and_order() {
        let cases = [(150, 0), (151, 1)];
        let fields = [
            Target {
                id: 120,
                source: Source::Field(120),
            },
            Target {
                id: 121,
                source: Source::Value(-2),
            },
            Target {
                id: 122,
                source: Source::Table {
                    field: 120,
                    cases: &cases,
                },
            },
        ];
        let bytes = Migration { fields: &fields }
            .encode()
            .unwrap_or_else(|_| panic!("encodes"));
        let mut expected = b"ZFCIS-MIGRATION\0\x01".to_vec();
        expected.extend_from_slice(&3_u32.to_be_bytes());
        expected.extend_from_slice(&120_u16.to_be_bytes());
        expected.push(0);
        expected.extend_from_slice(&120_u16.to_be_bytes());
        expected.extend_from_slice(&121_u16.to_be_bytes());
        expected.push(1);
        expected.extend_from_slice(&(-2_i128).to_be_bytes());
        expected.extend_from_slice(&122_u16.to_be_bytes());
        expected.push(2);
        expected.extend_from_slice(&120_u16.to_be_bytes());
        expected.extend_from_slice(&2_u32.to_be_bytes());
        for (old, new) in cases {
            expected.extend_from_slice(&i128::to_be_bytes(old));
            expected.extend_from_slice(&i128::to_be_bytes(new));
        }
        assert_eq!(bytes, expected);
        let mut swapped = fields;
        swapped[1].source = Source::Value(2);
        assert_ne!(Migration { fields: &swapped }.encode().ok(), Some(expected));
    }

    #[test]
    fn a_migration_must_give_every_new_field_a_value_and_drop_no_old_one() {
        let leaf = Leaf::Bool;
        let root = |ids: &[u16]| Root {
            record: true,
            ids: ids.to_vec(),
            leaves: ids.iter().map(|_| &leaf).collect(),
            binding: FrameBinding {
                root: 100,
                schema: [0; 32],
                max_bytes: 1024,
            },
        };
        let (old, new) = (root(&[1, 2]), root(&[1, 2, 3]));
        let carry = |id| Target {
            id,
            source: Source::Field(id),
        };
        let default = Target {
            id: 3,
            source: Source::Value(0),
        };
        let fit = |fields: &[Target<'_>]| fits(&Migration { fields }, &old, &new);
        assert_eq!(fit(&[carry(1), carry(2), default]), Ok(()));
        assert_eq!(
            fit(&[carry(1), carry(2)]),
            Err(Defect::Missing { field: 3 })
        );
        assert_eq!(
            fit(&[carry(1), default, carry(2)]),
            Err(Defect::Unknown { field: 2 })
        );
        assert_eq!(
            fit(&[carry(1), carry(1), carry(2), default]),
            Err(Defect::Unknown { field: 1 })
        );
        assert_eq!(
            fit(&[carry(1), carry(2), default, carry(4)]),
            Err(Defect::Unknown { field: 4 })
        );
        let renamed = Target {
            id: 2,
            source: Source::Field(9),
        };
        assert_eq!(
            fit(&[carry(1), renamed, default]),
            Err(Defect::Source { field: 9 })
        );
        let twice = Target {
            id: 2,
            source: Source::Field(1),
        };
        assert_eq!(
            fit(&[carry(1), twice, default]),
            Err(Defect::Dropped { field: 2 })
        );
        let unordered = [(1, 0), (0, 1)];
        let table = Target {
            id: 3,
            source: Source::Table {
                field: 1,
                cases: &unordered,
            },
        };
        assert_eq!(
            fit(&[carry(1), carry(2), table]),
            Err(Defect::Table { field: 3 })
        );
    }

    #[test]
    fn codes_and_numbers_round_trip_within_each_leaf_domain() {
        assert_eq!(code(&Leaf::Bool, 1), Some(1));
        assert_eq!(code(&Leaf::Bool, 2), None);
        let tier = Leaf::I128 { min: 0, max: 3 };
        assert_eq!(code(&tier, 3), Some(3));
        assert_eq!(code(&tier, 4), None);
        assert_eq!(number(&tier, 2), Some(2));
    }
}
