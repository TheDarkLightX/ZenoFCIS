//! Framing a decision's view from stored entries, and writing it back.
//!
//! [`Layout::frame`] builds the view the library Authority decides: scalar
//! state as stored, each named entry read from storage at the key its
//! command or context field holds, absent entries at their defaults, and each
//! key position's class by first occurrence among the keys of its type.
//! [`Layout::writes`] turns a committed decision's post-view into storage
//! writes: the scalar state, and for each named key the one new record its
//! entries agree on, or a removal. A key class written to a stored key field
//! becomes the key of that class. These are the runtime's half of the
//! collections premises: storage is read and written only at named keys.

use std::collections::{BTreeMap, BTreeSet};

use zeno_fcis_spec::{ProjectionRoot, TypeKind};

use crate::ContractError;
use crate::internals::{
    declarations::{Declarations, Source},
    review::domain::{self, Position},
    rules::Rules,
};

/// An opaque 32-byte key.
pub type Key = [u8; 32];

/// One real input record: scalar fields and key fields, by field ID.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Input {
    /// Scalar fields, as the contract's numbers.
    pub scalars: BTreeMap<u16, i128>,
    /// Key fields.
    pub keys: BTreeMap<u16, Key>,
}

/// One field of a stored record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordValue {
    /// A scalar, as the contract's number.
    Number(i128),
    /// A stored key.
    Key(Key),
}

/// Encodes a record: per field, tag 0 and 16 big-endian bytes for a number,
/// or tag 1 and 32 bytes for a key.
#[must_use]
pub fn encode_record(values: &[RecordValue]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        match value {
            RecordValue::Number(n) => {
                out.push(0);
                out.extend_from_slice(&n.to_be_bytes());
            }
            RecordValue::Key(key) => {
                out.push(1);
                out.extend_from_slice(key);
            }
        }
    }
    out
}

/// Decodes a record [`encode_record`] wrote.
pub fn decode_record(mut bytes: &[u8]) -> Option<Vec<RecordValue>> {
    let mut out = Vec::new();
    while let Some((tag, rest)) = bytes.split_first() {
        match tag {
            0 if rest.len() >= 16 => {
                let mut number = [0u8; 16];
                number.copy_from_slice(&rest[..16]);
                out.push(RecordValue::Number(i128::from_be_bytes(number)));
                bytes = &rest[16..];
            }
            1 if rest.len() >= 32 => {
                let mut key = [0u8; 32];
                key.copy_from_slice(&rest[..32]);
                out.push(RecordValue::Key(key));
                bytes = &rest[32..];
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Stored entries, read one named key at a time.
pub trait Entries {
    /// The record at `key` in `map`, if the map holds one.
    fn record(&self, map: u32, key: &Key) -> Option<Vec<u8>>;
}

struct EntryLayout {
    map: u32,
    key_field: (Source, u16),
    presence: usize,
    /// View positions of the record fields, with the key type of stored keys.
    values: Vec<(usize, Option<u32>)>,
}

/// How a collections contract's view is laid out.
pub struct Layout {
    positions: Vec<Position>,
    entries: Vec<EntryLayout>,
    /// Input key positions with their key type.
    inputs: Vec<(usize, u32)>,
    /// State positions that belong to no entry.
    scalars: Vec<usize>,
}

/// One decision's view, with what it needs to write back.
#[derive(Clone, Debug)]
pub struct Framed {
    /// The view tuple, in program input order.
    pub tuple: Vec<i128>,
    /// The keys the decision names, per map.
    pub named: BTreeMap<u32, BTreeSet<Key>>,
    /// Per key type, the key of each class.
    classes: BTreeMap<u32, Vec<Key>>,
    /// Per entry, the key it names.
    entry_keys: Vec<Key>,
}

/// Storage writes of a committed decision.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Writes {
    /// Every scalar state field after the decision.
    pub scalars: BTreeMap<u16, i128>,
    /// New records, or `None` for removals, at named keys only.
    pub entries: Vec<(u32, Key, Option<Vec<u8>>)>,
}

impl Layout {
    /// Reads a collections contract's view layout.
    pub fn read(project: &str, policy: &str) -> Result<Self, ContractError> {
        let invalid = |reason: String| ContractError::new("collections runtime", reason);
        let spec = zeno_fcis_spec::parse_project(project, Default::default())
            .and_then(|p| zeno_fcis_spec::elaborate_project(p, Default::default()))
            .map_err(|e| invalid(e.to_string()))?;
        let rules = Rules::read_for_project(project, policy)?;
        let declarations = Declarations::read(project, &rules.leaf_bindings)?;
        let positions = domain::positions(&declarations)?;
        let collections = spec.collections();
        if collections.has_ordered_keys() {
            return Err(invalid(
                "ordered collection framing is not supported by Layout".to_owned(),
            ));
        }
        let key_types: BTreeSet<u32> = collections.keys().iter().map(|k| k.id().get()).collect();
        let at = |name: String| {
            positions
                .iter()
                .position(|p| p.name == name)
                .ok_or_else(|| invalid(format!("{name} is not a view input")))
        };
        let roots: BTreeMap<u32, Source> = spec
            .types()
            .iter()
            .filter_map(|t| match t.kind() {
                TypeKind::Command => Some((t.id().get(), Source::Command)),
                TypeKind::Context => Some((t.id().get(), Source::Context)),
                _ => None,
            })
            .collect();
        let mut inputs = Vec::new();
        for field in spec.fields() {
            if let Some(source) = roots.get(&field.owner().get())
                && key_types.contains(&field.field_type().get())
            {
                let root = if *source == Source::Command {
                    "command"
                } else {
                    "context"
                };
                let index = at(format!(
                    "{root}.{}.{}",
                    field.owner().get(),
                    field.id().get()
                ))?;
                inputs.push((index, field.field_type().get()));
            }
        }
        let mut entries = Vec::new();
        let mut entry_positions = BTreeSet::new();
        for entry in collections.entries() {
            let map = collections
                .map(entry.map())
                .ok_or_else(|| invalid(format!("entry {} names no map", entry.id().get())))?;
            let owner = map.owner().get();
            let presence = at(format!("pre.{owner}.{}", entry.id().get()))?;
            entry_positions.insert(presence);
            let mut values = Vec::new();
            for (j, (_, field_type)) in map.value_fields().iter().enumerate() {
                let index = at(format!("pre.{owner}.{}", entry.id().get() + j as u32 + 1))?;
                entry_positions.insert(index);
                values.push((
                    index,
                    key_types
                        .contains(&field_type.get())
                        .then(|| field_type.get()),
                ));
            }
            let segments = entry.key().segments();
            let source = match entry.key().root() {
                ProjectionRoot::Command => Source::Command,
                _ => Source::Context,
            };
            let field = segments
                .get(1)
                .and_then(|s| u16::try_from(s.get()).ok())
                .ok_or_else(|| invalid(format!("entry {} has no key field", entry.id().get())))?;
            entries.push(EntryLayout {
                map: map.id().get(),
                key_field: (source, field),
                presence,
                values,
            });
        }
        let scalars = positions
            .iter()
            .enumerate()
            .filter(|(i, p)| p.source == Source::State && !entry_positions.contains(i))
            .map(|(i, _)| i)
            .collect();
        Ok(Self {
            positions,
            entries,
            inputs,
            scalars,
        })
    }

    /// The view's program inputs, in order.
    pub fn positions(&self) -> &[Position] {
        &self.positions
    }

    /// Frames one decision's view, reading storage only at the named keys.
    pub fn frame(
        &self,
        state: &BTreeMap<u16, i128>,
        command: &Input,
        context: &Input,
        storage: &impl Entries,
    ) -> Result<Framed, ContractError> {
        let invalid = |reason: String| ContractError::new("collections runtime", reason);
        let input = |source: Source| match source {
            Source::Command => command,
            _ => context,
        };
        let mut tuple = vec![0i128; self.positions.len()];
        let field_of = |index: usize| self.positions[index].field.unwrap_or(0);
        for &index in &self.scalars {
            tuple[index] = *state
                .get(&field_of(index))
                .ok_or_else(|| invalid(format!("state field {} is missing", field_of(index))))?;
        }
        let key_positions: BTreeSet<usize> = self.inputs.iter().map(|(i, _)| *i).collect();
        for (index, position) in self.positions.iter().enumerate() {
            if position.source != Source::State && !key_positions.contains(&index) {
                let source = input(position.source);
                tuple[index] = match position.field {
                    Some(field) => *source
                        .scalars
                        .get(&field)
                        .ok_or_else(|| invalid(format!("{} is missing", position.name)))?,
                    None => *source
                        .scalars
                        .get(&0)
                        .ok_or_else(|| invalid(format!("{} is missing", position.name)))?,
                };
            }
        }
        // Named entries, and the actual key at every key position.
        let mut actual: BTreeMap<usize, (u32, Key)> = BTreeMap::new();
        for (index, key_type) in &self.inputs {
            let field = field_of(*index);
            let key = input(self.positions[*index].source)
                .keys
                .get(&field)
                .ok_or_else(|| invalid(format!("key field {field} is missing")))?;
            actual.insert(*index, (*key_type, *key));
        }
        let mut named: BTreeMap<u32, BTreeSet<Key>> = BTreeMap::new();
        let mut entry_keys = Vec::new();
        for entry in &self.entries {
            let (source, field) = entry.key_field;
            let key = *input(source)
                .keys
                .get(&field)
                .ok_or_else(|| invalid(format!("key field {field} is missing")))?;
            named.entry(entry.map).or_default().insert(key);
            entry_keys.push(key);
            match storage.record(entry.map, &key) {
                Some(bytes) => {
                    let record = decode_record(&bytes)
                        .filter(|r| r.len() == entry.values.len())
                        .ok_or_else(|| {
                            invalid(format!("map {} holds a malformed record", entry.map))
                        })?;
                    tuple[entry.presence] = 1;
                    for ((index, key_type), value) in entry.values.iter().zip(record) {
                        match (key_type, value) {
                            (None, RecordValue::Number(n)) => tuple[*index] = n,
                            (Some(t), RecordValue::Key(k)) => {
                                actual.insert(*index, (*t, k));
                            }
                            _ => {
                                return Err(invalid(format!(
                                    "map {} holds a mistyped record",
                                    entry.map
                                )));
                            }
                        }
                    }
                }
                None => {
                    tuple[entry.presence] = 0;
                    for (index, _) in &entry.values {
                        tuple[*index] = self.positions[*index].domain.least().unwrap_or(0);
                    }
                }
            }
        }
        // Classes by first occurrence, in input order, per key type.
        let mut classes: BTreeMap<u32, Vec<Key>> = BTreeMap::new();
        for (index, (key_type, key)) in &actual {
            let seen = classes.entry(*key_type).or_default();
            let class = match seen.iter().position(|k| k == key) {
                Some(class) => class,
                None => {
                    seen.push(*key);
                    seen.len() - 1
                }
            };
            tuple[*index] = i128::try_from(class).unwrap_or(i128::MAX);
        }
        Ok(Framed {
            tuple,
            named,
            classes,
            entry_keys,
        })
    }

    /// The storage writes of a committed decision with post-state `post`,
    /// field by field. Entries that name one key must agree on at most one
    /// new value.
    pub fn writes(
        &self,
        framed: &Framed,
        post: &BTreeMap<u16, i128>,
    ) -> Result<Writes, ContractError> {
        let invalid = |reason: String| ContractError::new("collections runtime", reason);
        let read = |index: usize| {
            self.positions[index]
                .field
                .and_then(|field| post.get(&field).copied())
                .unwrap_or(framed.tuple[index])
        };
        let mut writes = Writes::default();
        for &index in &self.scalars {
            writes
                .scalars
                .insert(self.positions[index].field.unwrap_or(0), read(index));
        }
        let mut done = BTreeSet::new();
        for (i, entry) in self.entries.iter().enumerate() {
            let key = framed.entry_keys[i];
            if !done.insert((entry.map, key)) {
                continue;
            }
            let members: Vec<usize> = (0..self.entries.len())
                .filter(|j| self.entries[*j].map == entry.map && framed.entry_keys[*j] == key)
                .collect();
            let view = |j: usize, value: &dyn Fn(usize) -> i128| {
                let e = &self.entries[j];
                core::iter::once(value(e.presence))
                    .chain(e.values.iter().map(|(p, _)| value(*p)))
                    .collect::<Vec<_>>()
            };
            let before = view(members[0], &|p| framed.tuple[p]);
            let mut written: Vec<(usize, Vec<i128>)> = members
                .iter()
                .map(|j| (*j, view(*j, &read)))
                .filter(|(_, v)| *v != before)
                .collect();
            written.dedup_by(|a, b| a.1 == b.1);
            let Some((j, value)) = written.first() else {
                continue;
            };
            if written.len() > 1 {
                return Err(invalid(format!(
                    "alias merge: entries name map {} at one key and write different values",
                    entry.map
                )));
            }
            let record = if value[0] == 1 {
                let mut fields = Vec::new();
                for ((_, key_type), v) in self.entries[*j].values.iter().zip(&value[1..]) {
                    fields.push(match key_type {
                        None => RecordValue::Number(*v),
                        Some(t) => RecordValue::Key(
                            *usize::try_from(*v)
                                .ok()
                                .and_then(|class| framed.classes.get(t)?.get(class))
                                .ok_or_else(|| invalid(format!("key class {v} names no key")))?,
                        ),
                    });
                }
                Some(encode_record(&fields))
            } else {
                None
            };
            writes.entries.push((entry.map, key, record));
        }
        Ok(writes)
    }
}
