//! Keyed collections: maps of any size, lowered to a finite view while parsing.
//!
//! `type ID key Name;` declares an opaque identifier of 32 bytes. Keys are
//! compared only for equality: in the view, every key-typed input field and
//! every key-typed field of a named entry holds a key class, an integer from
//! 0 to n-1 where n counts those key positions of the type, and only one
//! assignment per equality pattern is admitted (a restricted-growth string).
//!
//! `map ID OWNER name KEY VALUE capacity N;` declares a map in the state
//! record OWNER from keys of type KEY to records of type VALUE, holding at
//! most N entries at run time. The map has no field in the view: a decision
//! reads and writes only the entries a command or context key names.
//!
//! `entry ID name of MAP at ROOT.TYPE.FIELD;` names the entry of MAP at the
//! key in that command or context field. In the view it is the state field
//! ID, `1` when the entry is present and `0` when it is absent, followed by
//! the field `ID + j` for the j-th field of VALUE in stable-ID order. Laws
//! may write `ROOT.OWNER.ID.FIELD` for that copy, and
//! `forall e in entries MAP { ... }` over the named entries of a map.
//!
//! `summary ID OWNER name TYPE sum MAP.FIELD;` and `... count MAP;` declare
//! a state field holding the sum of a field over every entry, or the number
//! of entries. A checker proves that each decision changes it by exactly the
//! change of the entries it names, so it equals the aggregate inductively.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use crate::ast::{ProjectionRoot, RelExpr, ValueExpr};
use crate::lexer::{Token, TokenKind};
use crate::{Identifier, ProjectionPath, StableId};

/// An opaque or ordered key type and the key classes its view admits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyTypeDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) positions: u32,
    pub(crate) order: Option<crate::IntRange>,
}

impl KeyTypeDecl {
    /// The type's stable ID; in the view it is `int 0..=positions-1`.
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// The type's name.
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Concrete integer bounds for an ordered key; opaque keys have none.
    pub const fn order_range(&self) -> Option<crate::IntRange> {
        self.order
    }
    /// Whether same-type ordering comparisons are allowed.
    pub const fn is_ordered(&self) -> bool {
        self.order.is_some()
    }
    /// Key positions of this type in the view: input fields, then stored
    /// fields of named entries.
    pub const fn positions(&self) -> u32 {
        self.positions
    }
}

/// A map from keys to records in the state, of bounded run-time capacity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapDecl {
    pub(crate) id: StableId,
    pub(crate) owner: StableId,
    pub(crate) name: Identifier,
    pub(crate) key_type: StableId,
    pub(crate) value_type: StableId,
    pub(crate) capacity: u64,
    pub(crate) value_fields: Box<[(StableId, StableId)]>,
}

impl MapDecl {
    /// The map's stable ID, also the ID of its presence type in the view.
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// The state record that holds the map.
    pub const fn owner(&self) -> StableId {
        self.owner
    }
    /// The map's name.
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// The key type.
    pub const fn key_type(&self) -> StableId {
        self.key_type
    }
    /// The record type of each entry.
    pub const fn value_type(&self) -> StableId {
        self.value_type
    }
    /// The most entries the map holds at run time; never enumerated.
    pub const fn capacity(&self) -> u64 {
        self.capacity
    }
    /// The value record's fields with their types, in stable-ID order.
    pub const fn value_fields(&self) -> &[(StableId, StableId)] {
        &self.value_fields
    }
}

/// The entry of a map named by one command or context key field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntryDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) map: StableId,
    pub(crate) key: ProjectionPath,
}

impl EntryDecl {
    /// The entry's ID: its presence field in the view; value field j of the
    /// map's record is the view field `id + j`.
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// The entry's name.
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// The map.
    pub const fn map(&self) -> StableId {
        self.map
    }
    /// The command or context key field that names the entry.
    pub const fn key(&self) -> &ProjectionPath {
        &self.key
    }
}

/// The aggregate a summary holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SummaryKind {
    /// The sum of one value field over every entry.
    Sum(StableId),
    /// The number of entries.
    Count,
}

/// A state field that holds an aggregate over every entry of a map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SummaryDecl {
    pub(crate) id: StableId,
    pub(crate) owner: StableId,
    pub(crate) name: Identifier,
    pub(crate) value_type: StableId,
    pub(crate) map: StableId,
    pub(crate) kind: SummaryKind,
}

impl SummaryDecl {
    /// The summary's state field ID.
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// The state record holding the summary field.
    pub const fn owner(&self) -> StableId {
        self.owner
    }
    /// The summary's name.
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// The summary field's integer type.
    pub const fn value_type(&self) -> StableId {
        self.value_type
    }
    /// The map it summarizes.
    pub const fn map(&self) -> StableId {
        self.map
    }
    /// Sum of a field, or count of entries.
    pub const fn kind(&self) -> SummaryKind {
        self.kind
    }
}

/// A bounded observation of a map's pre-state key order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderReadKind {
    /// Least key, if any.
    Min,
    /// Greatest key, if any.
    Max,
    /// Least key strictly greater than this input key.
    Successor(ProjectionPath),
    /// Greatest key strictly less than this input key.
    Predecessor(ProjectionPath),
    /// A witness in the closed interval, if any; reversed intervals are empty.
    Range(ProjectionPath, ProjectionPath),
}

/// A read with a context key result and an integer 0/1 found flag.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrderReadDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) map: StableId,
    pub(crate) kind: OrderReadKind,
    pub(crate) result: ProjectionPath,
    pub(crate) found: ProjectionPath,
}

impl OrderReadDecl {
    /// Declaration ID.
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Declaration name.
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Observed map.
    pub const fn map(&self) -> StableId {
        self.map
    }
    /// Query and its command/context endpoints.
    pub const fn kind(&self) -> &OrderReadKind {
        &self.kind
    }
    /// Context key result. When not found its value is unconstrained.
    pub const fn result(&self) -> &ProjectionPath {
        &self.result
    }
    /// Context integer 0/1 found flag.
    pub const fn found(&self) -> &ProjectionPath {
        &self.found
    }
}

/// A per-key nonnegative tally equal to grouped source contributions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TallyDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) target: StableId,
    pub(crate) field: StableId,
    pub(crate) source: StableId,
    pub(crate) group: StableId,
    pub(crate) weight: Option<StableId>,
    pub(crate) selector: Option<StableId>,
}

impl TallyDecl {
    /// Stable declaration identity.
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Declaration name.
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Map containing each per-key tally.
    pub const fn target(&self) -> StableId {
        self.target
    }
    /// Nonnegative integer tally field.
    pub const fn field(&self) -> StableId {
        self.field
    }
    /// Map whose entries contribute.
    pub const fn source(&self) -> StableId {
        self.source
    }
    /// Stored key field grouping source entries by target key.
    pub const fn group(&self) -> StableId {
        self.group
    }
    /// Nonnegative integer weight, or one per selected entry.
    pub const fn weight(&self) -> Option<StableId> {
        self.weight
    }
    /// Optional integer 0..=1 field; only value one contributes.
    pub const fn selector(&self) -> Option<StableId> {
        self.selector
    }
}

/// Every collection declaration of a project, in stable-ID order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CollectionsSpec {
    pub(crate) keys: Box<[KeyTypeDecl]>,
    pub(crate) maps: Box<[MapDecl]>,
    pub(crate) entries: Box<[EntryDecl]>,
    pub(crate) summaries: Box<[SummaryDecl]>,
    pub(crate) order_reads: Box<[OrderReadDecl]>,
    pub(crate) tallies: Box<[TallyDecl]>,
}

impl CollectionsSpec {
    /// Cross-map grouped tally invariants.
    pub const fn tallies(&self) -> &[TallyDecl] {
        &self.tallies
    }
    /// Key types.
    pub const fn keys(&self) -> &[KeyTypeDecl] {
        &self.keys
    }
    /// Maps.
    pub const fn maps(&self) -> &[MapDecl] {
        &self.maps
    }
    /// Named entries.
    pub const fn entries(&self) -> &[EntryDecl] {
        &self.entries
    }
    /// Summaries.
    pub const fn summaries(&self) -> &[SummaryDecl] {
        &self.summaries
    }
    /// Bounded order observations, at most four per project.
    pub const fn order_reads(&self) -> &[OrderReadDecl] {
        &self.order_reads
    }
    /// Whether ordered key metadata requires format 6.
    pub fn has_ordered_keys(&self) -> bool {
        self.keys.iter().any(KeyTypeDecl::is_ordered)
    }
    /// True when the project declares no collection.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
            && self.maps.is_empty()
            && self.summaries.is_empty()
            && self.tallies.is_empty()
    }
    /// The map with this ID.
    pub fn map(&self, id: StableId) -> Option<&MapDecl> {
        self.maps.iter().find(|map| map.id == id)
    }
    /// The entries of one map, in stable-ID order.
    pub fn entries_of(&self, map: StableId) -> impl Iterator<Item = &EntryDecl> {
        self.entries.iter().filter(move |entry| entry.map == map)
    }
}

/// Map, entry and record-field declarations read before parsing, so laws
/// may name entries declared later in the file.
#[derive(Clone, Debug, Default)]
pub(crate) struct Scan {
    /// Map ID to its value record type.
    pub(crate) maps: BTreeMap<u32, u32>,
    /// Entry ID to its map.
    pub(crate) entries: BTreeMap<u32, u32>,
    /// Record type to its field IDs, ascending.
    pub(crate) record_fields: BTreeMap<u32, Vec<u32>>,
}

impl Scan {
    pub(crate) fn read(tokens: &[Token]) -> Self {
        let mut scan = Self::default();
        let number = |token: &Token| match token.kind {
            TokenKind::Number(value) => u32::try_from(value).ok(),
            _ => None,
        };
        let word = |token: &Token, expected: &str| matches!(&token.kind, TokenKind::Ident(value) if value.as_ref() == expected);
        for (index, token) in tokens.iter().enumerate() {
            let rest = &tokens[index..];
            if word(token, "map")
                && rest.len() >= 6
                && let (Some(id), Some(value)) = (number(&rest[1]), number(&rest[5]))
            {
                scan.maps.entry(id).or_insert(value);
            } else if word(token, "entry")
                && rest.len() >= 5
                && word(&rest[3], "of")
                && let (Some(id), Some(map)) = (number(&rest[1]), number(&rest[4]))
            {
                scan.entries.entry(id).or_insert(map);
            } else if word(token, "field")
                && rest.len() >= 6
                && let (Some(id), Some(owner), Some(_), TokenKind::Semicolon) = (
                    number(&rest[1]),
                    number(&rest[2]),
                    number(&rest[4]),
                    &rest[5].kind,
                )
            {
                scan.record_fields.entry(owner).or_default().push(id);
            }
        }
        for fields in scan.record_fields.values_mut() {
            fields.sort_unstable();
            fields.dedup();
        }
        scan
    }

    /// The view field of an entry's copy of a value field: the entry's ID
    /// plus the field's one-based position in its record.
    pub(crate) fn entry_field(&self, entry: u32, field: u32) -> Option<u32> {
        let map = self.entries.get(&entry)?;
        let record = self.maps.get(map)?;
        let position = self
            .record_fields
            .get(record)?
            .iter()
            .position(|f| *f == field)?;
        entry.checked_add(u32::try_from(position).ok()?.checked_add(1)?)
    }

    /// The named entries of a map, ascending.
    pub(crate) fn entries_of(&self, map: u32) -> Vec<u32> {
        self.entries
            .iter()
            .filter(|(_, owner)| **owner == map)
            .map(|(entry, _)| *entry)
            .collect()
    }
}

/// View paths whose values are key classes, and map paths, which no
/// formula may read.
#[derive(Clone, Debug, Default)]
pub(crate) struct KeyPaths {
    /// Key-class view paths with their key type.
    pub(crate) keys: BTreeMap<(ProjectionRoot, Vec<u32>), u32>,
    /// `(state record, map)` pairs.
    pub(crate) maps: BTreeSet<(u32, u32)>,
    pub(crate) ordered: BTreeSet<u32>,
}

impl KeyPaths {
    fn key_type(&self, value: &ValueExpr) -> Option<u32> {
        let ValueExpr::Projection(path) = value else {
            return None;
        };
        let segments: Vec<u32> = path.segments().iter().map(|s| s.get()).collect();
        // A pre or post path to a state key reads the same class.
        let roots: &[ProjectionRoot] = match path.root() {
            ProjectionRoot::Pre | ProjectionRoot::Post => &[ProjectionRoot::Pre],
            other => &[other],
        };
        roots
            .iter()
            .find_map(|root| self.keys.get(&(*root, segments.clone())).copied())
    }

    fn reads_map(&self, value: &ValueExpr) -> bool {
        match value {
            ValueExpr::Projection(path) => {
                matches!(path.root(), ProjectionRoot::Pre | ProjectionRoot::Post)
                    && path.segments().len() >= 2
                    && self
                        .maps
                        .contains(&(path.segments()[0].get(), path.segments()[1].get()))
            }
            _ => false,
        }
    }

    /// The first misuse of a key or map: same-type key comparisons only,
    /// with order comparisons restricted to ordered types. No formula reads a
    /// map except through its named entries.
    pub(crate) fn misuse(&self, formula: &RelExpr) -> Option<&'static str> {
        match formula {
            RelExpr::Circuit(c) => c.nodes().iter().find_map(|n| {
                if let crate::BoolNode::Formula(f) = n {
                    self.misuse(f)
                } else {
                    None
                }
            }),
            RelExpr::Bool(_) => None,
            RelExpr::Not(inner) => self.misuse(inner),
            RelExpr::And(a, b) | RelExpr::Or(a, b) | RelExpr::Implies(a, b) => {
                self.misuse(a).or_else(|| self.misuse(b))
            }
            RelExpr::Compare(op, a, b) => match (self.key_type(a), self.key_type(b)) {
                (Some(x), Some(y)) if x == y => (!self.ordered.contains(&x)
                    && !matches!(op, crate::CompareOp::Eq | crate::CompareOp::NotEq))
                .then_some("keys are compared only with == and !="),
                (Some(_), Some(_)) => Some("keys of different types are never compared"),
                (Some(_), None) | (None, Some(_)) => {
                    Some("a key is compared only with another key of its type")
                }
                (None, None) => self.value_misuse(a).or_else(|| self.value_misuse(b)),
            },
            RelExpr::Predicate { arguments, .. } => {
                arguments.iter().find_map(|value| self.value_misuse(value))
            }
            RelExpr::ForAll { body, .. } | RelExpr::Exists { body, .. } => self.misuse(body),
        }
    }

    fn value_misuse(&self, value: &ValueExpr) -> Option<&'static str> {
        if self.key_type(value).is_some() {
            return Some("a key has no value: it is compared only with == and != against a key");
        }
        if self.reads_map(value) {
            return Some("a map is read only through its named entries");
        }
        match value {
            ValueExpr::Int(_)
            | ValueExpr::U128(_)
            | ValueExpr::Var(_)
            | ValueExpr::Projection(_) => None,
            ValueExpr::Select(c, a, b) => self
                .misuse(c)
                .or_else(|| self.value_misuse(a))
                .or_else(|| self.value_misuse(b)),
            ValueExpr::ToI128(a) | ValueExpr::ToU128(a) => self.value_misuse(a),
            ValueExpr::Add(a, b)
            | ValueExpr::Sub(a, b)
            | ValueExpr::Mul(a, b)
            | ValueExpr::Div(_, a, b) => self.value_misuse(a).or_else(|| self.value_misuse(b)),
            ValueExpr::MulDiv(_, a, b, c)
            | ValueExpr::SignedMulDivFloor(a, b, c)
            | ValueExpr::ScaledMulDiv(_, _, a, b, c, _)
            | ValueExpr::RepeatMulDiv(_, a, b, c, _) => self
                .value_misuse(a)
                .or_else(|| self.value_misuse(b))
                .or_else(|| self.value_misuse(c)),
            ValueExpr::Sum { body, .. } => self.value_misuse(body),
        }
    }
}

/// Every view field of a named entry, mapped to that entry's identity.
pub(crate) fn entry_fields(maps: &[MapDecl], entries: &[EntryDecl]) -> BTreeMap<u32, u32> {
    entries
        .iter()
        .flat_map(|entry| {
            let width = maps
                .iter()
                .find(|map| map.id == entry.map)
                .map_or(0, |map| map.value_fields.len() as u32);
            (0..=width).map(move |offset| (entry.id.get() + offset, entry.id.get()))
        })
        .collect()
}

/// A genesis law stays valid when unnamed entries retain their values.
pub(crate) fn state_law_is_local(
    formula: &RelExpr,
    owner: u32,
    entries: &BTreeMap<u32, u32>,
) -> bool {
    if entries.is_empty() {
        return true;
    }
    let mut parts = Vec::new();
    conjuncts(formula, &mut parts);
    parts.iter().all(|part| {
        let mut reads = BTreeSet::new();
        state_reads(part, owner, &mut reads);
        let named: BTreeSet<u32> = reads
            .iter()
            .filter_map(|field| entries.get(field))
            .copied()
            .collect();
        named.len() <= 1
            && !(named.len() == 1 && reads.iter().any(|field| !entries.contains_key(field)))
    })
}

/// Appends the collection declarations to a format-5 project encoding.
pub(crate) fn encode(
    spec: &CollectionsSpec,
    output: &mut Vec<u8>,
) -> Result<(), zeno_fcis_codec::EncodeError> {
    use crate::ast::{encode_path, put_length};
    put_length(output, spec.keys.len())?;
    for key in &spec.keys {
        key.id.encode_to(output)?;
        key.name.encode_to(output)?;
        output.extend_from_slice(&key.positions.to_be_bytes());
    }
    put_length(output, spec.maps.len())?;
    for map in &spec.maps {
        map.id.encode_to(output)?;
        map.owner.encode_to(output)?;
        map.name.encode_to(output)?;
        map.key_type.encode_to(output)?;
        map.value_type.encode_to(output)?;
        output.extend_from_slice(&map.capacity.to_be_bytes());
        put_length(output, map.value_fields.len())?;
        for (field, field_type) in &map.value_fields {
            field.encode_to(output)?;
            field_type.encode_to(output)?;
        }
    }
    put_length(output, spec.entries.len())?;
    for entry in &spec.entries {
        entry.id.encode_to(output)?;
        entry.name.encode_to(output)?;
        entry.map.encode_to(output)?;
        encode_path(&entry.key, output)?;
    }
    put_length(output, spec.summaries.len())?;
    for summary in &spec.summaries {
        summary.id.encode_to(output)?;
        summary.owner.encode_to(output)?;
        summary.name.encode_to(output)?;
        summary.value_type.encode_to(output)?;
        summary.map.encode_to(output)?;
        match summary.kind {
            SummaryKind::Sum(field) => {
                output.push(0);
                field.encode_to(output)?;
            }
            SummaryKind::Count => output.push(1),
        }
    }
    if spec.has_ordered_keys() {
        // Format 6 preserves the format-5 prefix, then binds all new semantics.
        output.extend_from_slice(b"ORDERED-READS\0");
        put_length(output, spec.keys.len())?;
        for key in &spec.keys {
            key.id.encode_to(output)?;
            match key.order {
                None => output.push(0),
                Some(range) => {
                    output.push(1);
                    output.extend_from_slice(&range.min().to_be_bytes());
                    output.extend_from_slice(&range.max().to_be_bytes());
                }
            }
        }
        put_length(output, spec.order_reads.len())?;
        for read in &spec.order_reads {
            read.id.encode_to(output)?;
            read.name.encode_to(output)?;
            read.map.encode_to(output)?;
            match &read.kind {
                OrderReadKind::Min => output.push(0),
                OrderReadKind::Max => output.push(1),
                OrderReadKind::Successor(at) => {
                    output.push(2);
                    encode_path(at, output)?;
                }
                OrderReadKind::Predecessor(at) => {
                    output.push(3);
                    encode_path(at, output)?;
                }
                OrderReadKind::Range(lo, hi) => {
                    output.push(4);
                    encode_path(lo, output)?;
                    encode_path(hi, output)?;
                }
            }
            encode_path(&read.result, output)?;
            encode_path(&read.found, output)?;
        }
    }
    if !spec.tallies.is_empty() {
        output.extend_from_slice(b"KEYED-TALLIES\0");
        put_length(output, spec.tallies.len())?;
        for tally in &spec.tallies {
            tally.id.encode_to(output)?;
            tally.name.encode_to(output)?;
            tally.target.encode_to(output)?;
            tally.field.encode_to(output)?;
            tally.source.encode_to(output)?;
            tally.group.encode_to(output)?;
            for field in [tally.weight, tally.selector] {
                output.push(u8::from(field.is_some()));
                if let Some(field) = field {
                    field.encode_to(output)?;
                }
            }
        }
    }
    Ok(())
}

/// The top-level conjuncts of a formula.
pub(crate) fn conjuncts<'a>(formula: &'a RelExpr, out: &mut Vec<&'a RelExpr>) {
    match formula {
        RelExpr::And(a, b) => {
            conjuncts(a, out);
            conjuncts(b, out);
        }
        other => out.push(other),
    }
}

/// The state fields of record `owner` a formula reads, before or after.
pub(crate) fn state_reads(formula: &RelExpr, owner: u32, out: &mut BTreeSet<u32>) {
    fn value(v: &ValueExpr, owner: u32, out: &mut BTreeSet<u32>) {
        match v {
            ValueExpr::Projection(path) => {
                if matches!(path.root(), ProjectionRoot::Pre | ProjectionRoot::Post)
                    && path.segments().len() >= 2
                    && path.segments()[0].get() == owner
                {
                    out.insert(path.segments()[1].get());
                }
            }
            ValueExpr::Select(c, a, b) => {
                state_reads(c, owner, out);
                value(a, owner, out);
                value(b, owner, out);
            }
            ValueExpr::Int(_) | ValueExpr::U128(_) | ValueExpr::Var(_) => {}
            ValueExpr::ToI128(a) | ValueExpr::ToU128(a) | ValueExpr::Sum { body: a, .. } => {
                value(a, owner, out)
            }
            ValueExpr::Add(a, b)
            | ValueExpr::Sub(a, b)
            | ValueExpr::Mul(a, b)
            | ValueExpr::Div(_, a, b) => {
                value(a, owner, out);
                value(b, owner, out);
            }
            ValueExpr::MulDiv(_, a, b, c)
            | ValueExpr::SignedMulDivFloor(a, b, c)
            | ValueExpr::ScaledMulDiv(_, _, a, b, c, _)
            | ValueExpr::RepeatMulDiv(_, a, b, c, _) => {
                value(a, owner, out);
                value(b, owner, out);
                value(c, owner, out);
            }
        }
    }
    match formula {
        RelExpr::Circuit(c) => {
            for n in c.nodes() {
                if let crate::BoolNode::Formula(f) = n {
                    state_reads(f, owner, out);
                }
            }
        }
        RelExpr::Bool(_) => {}
        RelExpr::Not(a) | RelExpr::ForAll { body: a, .. } | RelExpr::Exists { body: a, .. } => {
            state_reads(a, owner, out)
        }
        RelExpr::And(a, b) | RelExpr::Or(a, b) | RelExpr::Implies(a, b) => {
            state_reads(a, owner, out);
            state_reads(b, owner, out);
        }
        RelExpr::Compare(_, a, b) => {
            value(a, owner, out);
            value(b, owner, out);
        }
        RelExpr::Predicate { arguments, .. } => {
            for argument in arguments.iter() {
                value(argument, owner, out);
            }
        }
    }
}

/// Normalizes one entry-local predicate so that identical predicates on
/// different named entries compare structurally. Mutation owns this clone.
pub(crate) fn entry_predicate(formula: &RelExpr, owner: u32, entry: u32) -> RelExpr {
    fn value(v: &mut ValueExpr, owner: u32, entry: u32) {
        match v {
            ValueExpr::Projection(p) => {
                if matches!(p.root, ProjectionRoot::Pre | ProjectionRoot::Post)
                    && p.segments.len() == 2
                    && p.segments[0].get() == owner
                    && let Some(id) = p.segments[1]
                        .get()
                        .checked_sub(entry)
                        .and_then(|n| n.checked_add(1))
                        .and_then(StableId::new)
                {
                    p.segments[1] = id;
                }
            }
            ValueExpr::Select(condition, yes, no) => {
                rel(condition, owner, entry);
                value(yes, owner, entry);
                value(no, owner, entry);
            }
            ValueExpr::Int(_) | ValueExpr::U128(_) | ValueExpr::Var(_) => {}
            ValueExpr::ToI128(a) | ValueExpr::ToU128(a) | ValueExpr::Sum { body: a, .. } => {
                value(a, owner, entry)
            }
            ValueExpr::Add(a, b)
            | ValueExpr::Sub(a, b)
            | ValueExpr::Mul(a, b)
            | ValueExpr::Div(_, a, b) => {
                value(a, owner, entry);
                value(b, owner, entry);
            }
            ValueExpr::MulDiv(_, a, b, c)
            | ValueExpr::SignedMulDivFloor(a, b, c)
            | ValueExpr::ScaledMulDiv(_, _, a, b, c, _)
            | ValueExpr::RepeatMulDiv(_, a, b, c, _) => {
                value(a, owner, entry);
                value(b, owner, entry);
                value(c, owner, entry);
            }
        }
    }
    fn rel(f: &mut RelExpr, owner: u32, entry: u32) {
        match f {
            RelExpr::Circuit(c) => {
                *c = Box::new(c.map(|child| entry_predicate(child, owner, entry)));
            }
            RelExpr::Bool(_) => {}
            RelExpr::Not(a) | RelExpr::ForAll { body: a, .. } | RelExpr::Exists { body: a, .. } => {
                rel(a, owner, entry)
            }
            RelExpr::And(a, b) | RelExpr::Or(a, b) | RelExpr::Implies(a, b) => {
                rel(a, owner, entry);
                rel(b, owner, entry);
            }
            RelExpr::Compare(_, a, b) => {
                value(a, owner, entry);
                value(b, owner, entry);
            }
            RelExpr::Predicate { arguments, .. } => {
                for a in arguments.iter_mut() {
                    value(a, owner, entry);
                }
            }
        }
    }
    let mut normalized = formula.clone();
    rel(&mut normalized, owner, entry);
    normalized
}

#[cfg(test)]
mod entry_predicate_tests {
    use super::*;
    use crate::ast::{CompareOp, ProjectionPath};

    fn projection(field: u32) -> ValueExpr {
        let ids = [100, field].map(|id| StableId::new(id).unwrap_or_else(|| panic!("valid ID")));
        ValueExpr::Projection(
            ProjectionPath::try_new(ProjectionRoot::Post, ids.to_vec())
                .unwrap_or_else(|| panic!("valid projection")),
        )
    }

    fn circuit(entry: u32) -> RelExpr {
        let condition = RelExpr::Compare(CompareOp::Eq, projection(entry + 1), ValueExpr::Int(0));
        let selected = ValueExpr::Select(
            Box::new(condition),
            Box::new(projection(entry + 2)),
            Box::new(projection(entry + 3)),
        );
        let formula = RelExpr::Compare(CompareOp::Eq, selected, ValueExpr::Int(0));
        RelExpr::Circuit(Box::new(
            crate::BoolCircuit::try_new(
                alloc::vec![
                    crate::BoolNode::Formula(formula),
                    crate::BoolNode::Literal(true)
                ],
                1,
            )
            .unwrap_or_else(|| panic!("valid circuit")),
        ))
    }

    #[test]
    fn shared_circuit_entry_normalization_visits_every_conditional_operand() {
        let first = circuit(200);
        let second = circuit(300);
        let before = first.clone();
        assert_eq!(
            entry_predicate(&first, 100, 200),
            entry_predicate(&second, 100, 300)
        );
        assert_eq!(first, before);
        assert_eq!(first, circuit(200));
        assert_eq!(second, circuit(300));
        assert_ne!(first, second);
    }
}
