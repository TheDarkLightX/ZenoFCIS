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

/// An opaque 32-byte key type and the key classes its view admits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyTypeDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) positions: u32,
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

/// Every collection declaration of a project, in stable-ID order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CollectionsSpec {
    pub(crate) keys: Box<[KeyTypeDecl]>,
    pub(crate) maps: Box<[MapDecl]>,
    pub(crate) entries: Box<[EntryDecl]>,
    pub(crate) summaries: Box<[SummaryDecl]>,
}

impl CollectionsSpec {
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
    /// True when the project declares no collection.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.maps.is_empty() && self.summaries.is_empty()
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
            if word(token, "map") && rest.len() >= 6 {
                if let (Some(id), Some(value)) = (number(&rest[1]), number(&rest[5])) {
                    scan.maps.entry(id).or_insert(value);
                }
            } else if word(token, "entry") && rest.len() >= 5 && word(&rest[3], "of") {
                if let (Some(id), Some(map)) = (number(&rest[1]), number(&rest[4])) {
                    scan.entries.entry(id).or_insert(map);
                }
            } else if word(token, "field") && rest.len() >= 6 {
                if let (Some(id), Some(owner), Some(_), TokenKind::Semicolon) = (
                    number(&rest[1]),
                    number(&rest[2]),
                    number(&rest[4]),
                    &rest[5].kind,
                ) {
                    scan.record_fields.entry(owner).or_default().push(id);
                }
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

    /// The first misuse of a key or a map in a formula: keys only compare
    /// for (in)equality with keys of the same type, and no formula reads a
    /// map except through its named entries.
    pub(crate) fn misuse(&self, formula: &RelExpr) -> Option<&'static str> {
        match formula {
            RelExpr::Bool(_) => None,
            RelExpr::Not(inner) => self.misuse(inner),
            RelExpr::And(a, b) | RelExpr::Or(a, b) | RelExpr::Implies(a, b) => {
                self.misuse(a).or_else(|| self.misuse(b))
            }
            RelExpr::Compare(op, a, b) => match (self.key_type(a), self.key_type(b)) {
                (Some(x), Some(y)) if x == y => {
                    (!matches!(op, crate::CompareOp::Eq | crate::CompareOp::NotEq))
                        .then_some("keys are compared only with == and !=")
                }
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
    Ok(())
}
