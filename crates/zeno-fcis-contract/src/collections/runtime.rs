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
    model::Contract,
    review::{
        domain::{self, Position},
        evaluate::Framer,
    },
    rules::Rules,
    schema, schema_commitment,
};

use zeno_fcis_synthesis::finite::{
    V2Usage, v2_authority::Evaluation, v2_composition as c, v2_laws as laws,
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
    capacities: BTreeMap<u32, u64>,
    framer: Framer,
    declared_laws: Vec<u32>,
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

/// Owned evidence for the declared laws on a canonical stored successor.
/// This is an additional storage guard, never an Authority publication.
#[derive(Debug)]
pub struct SuccessorCheck {
    result: Result<(), laws::Failure>,
    usage: V2Usage,
    diagnostics: Vec<laws::Diagnostic>,
    reads: Vec<laws::ReadAttempt>,
}
impl SuccessorCheck {
    /// Declared-law result, actual recheck usage, full diagnostics and reads.
    /// Diagnostics also retain any subsequent raw-conformance refusal; the
    /// original Authority already checked raw conformance on its candidate.
    pub fn into_parts(
        self,
    ) -> (
        Result<(), laws::Failure>,
        V2Usage,
        Vec<laws::Diagnostic>,
        Vec<laws::ReadAttempt>,
    ) {
        (self.result, self.usage, self.diagnostics, self.reads)
    }
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
        let encoded = schema::encode(&declarations)?;
        let contract = Contract::build(&declarations, &rules, schema_commitment(&encoded)?)?;
        let framer = Framer::new(&positions, &contract);
        Ok(Self {
            declared_laws: declarations.laws.iter().map(|law| law.id).collect(),
            framer,
            positions,
            entries,
            inputs,
            scalars,
            capacities: collections
                .maps()
                .iter()
                .map(|map| (map.id().get(), map.capacity()))
                .collect(),
        })
    }

    /// The view's program inputs, in order.
    pub fn positions(&self) -> &[Position] {
        &self.positions
    }

    /// Declared map capacities; the storage shell enforces these resource limits.
    pub fn capacities(&self) -> &BTreeMap<u32, u64> {
        &self.capacities
    }

    /// Scalar state only, excluding request-dependent entry-view fields.
    pub fn scalar_state(&self, fields: &BTreeMap<u16, i128>) -> BTreeMap<u16, i128> {
        self.scalars
            .iter()
            .filter_map(|index| {
                let id = self.positions[*index].field.unwrap_or(0);
                fields.get(&id).map(|value| (id, *value))
            })
            .collect()
    }

    /// Decode canonical number strings and fixed-width lowercase key hex in input order.
    pub fn decode_input(&self, source: Source, values: &[String]) -> Result<Input, ContractError> {
        let invalid =
            || ContractError::new("collections input", "field count, canonical form or domain");
        let positions: Vec<_> = self
            .positions
            .iter()
            .enumerate()
            .filter(|(_, p)| p.source == source)
            .collect();
        if source == Source::State || positions.len() != values.len() {
            return Err(invalid());
        }
        let mut input = Input::default();
        for ((index, position), text) in positions.into_iter().zip(values) {
            let field = position.field.unwrap_or(0);
            if self.inputs.iter().any(|(i, _)| *i == index) {
                input
                    .keys
                    .insert(field, decode_key(text).ok_or_else(invalid)?);
            } else {
                let value: i128 = text.parse().map_err(|_| invalid())?;
                let fits = match &position.domain {
                    domain::LeafDomain::Bool => value == 0 || value == 1,
                    domain::LeafDomain::Int { min, max } => (*min..=*max).contains(&value),
                    domain::LeafDomain::Sum { variants, .. } => {
                        variants.iter().any(|id| i128::from(*id) == value)
                    }
                };
                if value.to_string() != *text || !fits {
                    return Err(invalid());
                }
                input.scalars.insert(field, value);
            }
        }
        Ok(input)
    }

    /// Encode a real input with exactly the same canonical wire forms as the decoder.
    pub fn encode_input(
        &self,
        source: Source,
        input: &Input,
    ) -> Result<Vec<String>, ContractError> {
        self.positions
            .iter()
            .enumerate()
            .filter(|(_, p)| p.source == source)
            .map(|(index, position)| {
                let field = position.field.unwrap_or(0);
                if self.inputs.iter().any(|(i, _)| *i == index) {
                    input
                        .keys
                        .get(&field)
                        .map(|key| key.iter().map(|b| format!("{b:02x}")).collect())
                } else {
                    input.scalars.get(&field).map(ToString::to_string)
                }
                .ok_or_else(|| ContractError::new("collections input", "missing input field"))
            })
            .collect()
    }

    /// Distinct named keys, before any storage read. Aliased positions share one load.
    pub fn named(
        &self,
        command: &Input,
        context: &Input,
    ) -> Result<BTreeMap<u32, BTreeSet<Key>>, ContractError> {
        let mut named: BTreeMap<u32, BTreeSet<Key>> = BTreeMap::new();
        for entry in &self.entries {
            let (source, field) = entry.key_field;
            let input = if source == Source::Command {
                command
            } else {
                context
            };
            let key = input
                .keys
                .get(&field)
                .ok_or_else(|| ContractError::new("collections input", "missing named key"))?;
            named.entry(entry.map).or_default().insert(*key);
        }
        Ok(named)
    }

    /// Transport interface over real inputs, retaining key types rather than their classes.
    pub fn interface(&self) -> serde_json::Value {
        use serde_json::json;
        let fields: Vec<_> = self
            .positions
            .iter()
            .enumerate()
            .filter(|(_, p)| p.source != Source::State)
            .map(|(index, p)| {
                let mut value = p.json();
                if let Some((_, key_type)) = self.inputs.iter().find(|(i, _)| *i == index) {
                    value["domain"] =
                        json!({"kind":"key","bytes":32,"encoding":"hex","type":key_type});
                }
                value
            })
            .collect();
        json!({"format":"zeno-fcis/collection-interface/1","fields":fields,"maps":self.capacities.iter().map(|(id,capacity)| json!({"id":id,"capacity":capacity.to_string()})).collect::<Vec<_>>()})
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
                    super::canonical_absent_view(
                        &self.positions,
                        entry.presence,
                        entry.values.iter().map(|(index, _)| *index),
                        &mut tuple,
                    );
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

    /// Reconstruct the exact stored successor in the invocation's existing key
    /// classes. No new storage read or key-class renaming occurs here.
    pub fn successor(&self, framed: &Framed, writes: &Writes) -> Result<Vec<i128>, ContractError> {
        let invalid = |reason| ContractError::new("collections successor", reason);
        let mut merged = framed.tuple.clone();
        for &index in &self.scalars {
            merged[index] = *writes
                .scalars
                .get(&self.positions[index].field.unwrap_or(0))
                .ok_or_else(|| invalid("missing scalar write"))?;
        }
        for (entry, key) in self.entries.iter().zip(&framed.entry_keys) {
            let Some((_, _, record)) = writes
                .entries
                .iter()
                .find(|(map, written_key, _)| *map == entry.map && written_key == key)
            else {
                continue;
            };
            if let Some(bytes) = record {
                let record = decode_record(bytes).ok_or_else(|| invalid("malformed record"))?;
                if record.len() != entry.values.len() {
                    return Err(invalid("record field count"));
                }
                merged[entry.presence] = 1;
                for ((index, key_type), value) in entry.values.iter().zip(record) {
                    merged[*index] = match (key_type, value) {
                        (None, RecordValue::Number(n)) => n,
                        (Some(t), RecordValue::Key(k)) => framed
                            .classes
                            .get(t)
                            .and_then(|keys| keys.iter().position(|key| *key == k))
                            .and_then(|class| i128::try_from(class).ok())
                            .ok_or_else(|| invalid("post key has no pre-invocation class"))?,
                        _ => return Err(invalid("mistyped record")),
                    };
                }
            } else {
                super::canonical_absent_view(
                    &self.positions,
                    entry.presence,
                    entry.values.iter().map(|(index, _)| *index),
                    &mut merged,
                );
            }
        }
        Ok(merged)
    }

    /// Check every declared applicable law on the canonical successor that will be
    /// stored. This lower-level check grants no decision authority. Its owned
    /// outcome retains actual recheck work and diagnostics, including refusal;
    /// it does not replace or alter the Authority evaluation's usage.
    pub fn check_successor(
        &self,
        descriptor: &c::Descriptor<'_>,
        framed: &Framed,
        writes: &Writes,
        evaluation: &Evaluation<'_>,
    ) -> Result<SuccessorCheck, ContractError> {
        let invalid = |reason| ContractError::new("collections successor", reason);
        let decision = evaluation
            .result()
            .map_err(|_| invalid("refused decision"))?;
        if decision.class() == c::Class::Reject {
            return Err(invalid("a reject has no successor"));
        }
        let merged = self.successor(framed, writes)?;
        let views = [0, 1, 2].map(|i| self.framer.fields(&self.framer.roots[i], &framed.tuple));
        let post = self.framer.fields(&self.framer.roots[0], &merged);
        let patch: Vec<_> = post
            .iter()
            .zip(&views[0])
            .filter_map(|(after, before)| {
                let index = self
                    .positions
                    .iter()
                    .position(|p| p.source == Source::State && p.field == Some(after.id))?;
                (merged[index] != framed.tuple[index]).then_some(laws::Patch {
                    field: after.id,
                    before: before.value,
                    after: after.value,
                })
            })
            .collect();
        let candidate = laws::Candidate {
            class: decision.class(),
            reason: decision.reason(),
            post: laws::RootView::Record(&post),
            patch: &patch,
            effects: decision.effects(),
            outbox: decision.outbox(),
            reads: evaluation.reads(),
            attempts: evaluation.decision_attempts(),
            usage: evaluation
                .decision_usage()
                .ok_or_else(|| invalid("missing decision usage"))?,
        };
        // Mirror the collection judges: declared contract laws precede the
        // framework's raw-decision conformance laws. Those remain checked on
        // the original candidate by Authority and are not canonical-map laws.
        let selected = self
            .declared_laws
            .iter()
            .map(|id| {
                descriptor
                    .laws
                    .iter()
                    .find(|law| law.id == *id)
                    .ok_or_else(|| invalid("missing declared law"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ordered: Vec<_> = selected
            .into_iter()
            .chain(
                descriptor
                    .laws
                    .iter()
                    .filter(|law| !self.declared_laws.contains(&law.id)),
            )
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
        let (result, usage, diagnostics, reads) = laws::evaluate(
            &ordered,
            descriptor.required,
            &laws::Frame::Transition {
                pre: self
                    .framer
                    .view(&self.framer.roots[0], &framed.tuple, &views[0]),
                command: self
                    .framer
                    .view(&self.framer.roots[1], &framed.tuple, &views[1]),
                context: self
                    .framer
                    .view(&self.framer.roots[2], &framed.tuple, &views[2]),
                candidate: &candidate,
            },
            descriptor.limits,
        )
        .into_parts();
        let declared_hold = diagnostics
            .get(..self.declared_laws.len())
            .is_some_and(|selected| {
                selected
                    .iter()
                    .all(|d| !matches!(d.verdict, laws::Verdict::Refused(_)))
            });
        Ok(SuccessorCheck {
            result: if declared_hold {
                Ok(())
            } else {
                Err(result.err().unwrap_or(laws::Failure::Frame))
            },
            usage,
            diagnostics,
            reads,
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

/// Exactly 32 bytes encoded as 64 lowercase hexadecimal digits; no aliases or prefixes.
pub fn decode_key(text: &str) -> Option<Key> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let mut key = [0; 32];
    for (out, pair) in key.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        let digit = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
        *out = digit(pair[0]) * 16 + digit(pair[1]);
    }
    Some(key)
}

/// The existing compiler/checker source identity, including this layout and the evaluator.
/// This is provenance; it confers no decision or publication authority.
pub fn source_digest() -> [u8; 32] {
    crate::checked_composition::Identity::current().digest()
}
