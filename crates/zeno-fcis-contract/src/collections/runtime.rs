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

#[cfg(not(verus_keep_ghost))]
use crate::internals::{
    declarations::Declarations,
    model::Contract,
    review::{domain, evaluate::Framer},
    rules::Rules,
    schema, schema_commitment,
};
#[cfg(not(verus_keep_ghost))]
use zeno_fcis_spec::{ProjectionRoot, TypeKind};
#[cfg(not(verus_keep_ghost))]
use zeno_fcis_synthesis::finite::{
    V2Usage, v2_authority::Evaluation, v2_composition as c, v2_laws as laws,
};

use crate::ContractError;
use crate::internals::{declarations::Source, review::domain::Position};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
/// An opaque 32-byte key.
pub type Key = [u8; 32];
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// One real input record, by scalar and key field ID.
pub struct Input {
    /// Scalar fields, as the contract's numbers.
    pub scalars: BTreeMap<u16, i128>,
    /// Key fields.
    pub keys: BTreeMap<u16, Key>,
}
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// One scalar or stored-key field of a record.
pub enum RecordValue {
    /// A scalar, as the contract's number.
    Number(i128),
    /// A stored key.
    Key(Key),
}
#[cfg(not(verus_keep_ghost))]
/// Stored records, read at the named map and key.
pub trait Entries {
    /// The record at `key` in `map`, if the map holds one.
    fn record(&self, map: u32, key: &Key) -> Option<Vec<u8>>;
}
#[cfg(verus_keep_ghost)]
verus! {
/// Stored records, read at the named map and key.
pub trait Entries {
    spec fn contents(&self) -> Map<(u32,Key),Seq<u8>>;
    fn record(&self,map:u32,key:&Key) -> (result:Option<Vec<u8>>)
        ensures record_view(result) == stored(self.contents(),map,*key);
}
pub open spec fn stored(contents:Map<(u32,Key),Seq<u8>>,map:u32,key:Key) -> Option<Seq<u8>> {
    if contents.contains_key((map,key)) { Some(contents[(map,key)]) } else { None }
}
}
#[cfg_attr(verus_keep_ghost, verus_verify)]
struct EntryLayout {
    map: u32,
    key_field: (Source, u16),
    presence: usize,
    /// View positions of the record fields, with the key type of stored keys.
    values: Vec<(usize, Option<u32>)>,
}
#[cfg_attr(verus_keep_ghost, verus_verify)]
/// The program positions and named-entry roles of a collections contract.
pub struct Layout {
    positions: Vec<Position>,
    entries: Vec<EntryLayout>,
    /// Input key positions with their key type.
    inputs: Vec<(usize, u32)>,
    /// State positions that belong to no entry.
    scalars: Vec<usize>,
    capacities: BTreeMap<u32, u64>,
    #[cfg(not(verus_keep_ghost))]
    framer: Framer,
    declared_laws: Vec<u32>,
}
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Debug)]
/// A framed decision with its named keys and invocation key classes.
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
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// Scalar state and merged named records produced by a decision.
pub struct Writes {
    /// Every scalar state field after the decision.
    pub scalars: BTreeMap<u16, i128>,
    /// New records, or `None` for removals, at named keys only.
    pub entries: Vec<(u32, Key, Option<Vec<u8>>)>,
}
#[cfg(verus_keep_ghost)]
verus! {
pub open spec fn field_bytes(value:RecordValue) -> Seq<u8> {
    match value {
        RecordValue::Number(number) => seq![0u8] + super::runtime_codec::signed_bytes(number),
        RecordValue::Key(key) => seq![1u8] + key@,
    }
}
pub open spec fn encoded(values:Seq<RecordValue>) -> Seq<u8>
    decreases values.len(),
{
    if values.len() == 0 { seq![] }
    else { encoded(values.drop_last()) + field_bytes(values.last()) }
}
proof fn encoded_length(values:Seq<RecordValue>)
    ensures 17 * values.len() <= encoded(values).len() <= 33 * values.len(),
    decreases values.len(),
{
    if values.len() > 0 {
        encoded_length(values.drop_last());
        assert(17 <= field_bytes(values.last()).len() <= 33);
    }
}
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires 33 * values@.len() <= usize::MAX,
    ensures result@ == encoded(values@),
))]
/// Encode fields with a tag and fixed-width big-endian payload.
pub fn encode_record(values: &[RecordValue]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant index <= values.len(), 33 * values@.len() <= usize::MAX,
            out@ == encoded(values@.take(index as int)), out@.len() <= 33 * index,
        decreases values.len() - index,
    ))]
    while index < values.len() {
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost previous = out@; }
        match &values[index] {
            RecordValue::Number(n) => {
                out.push(0);
                out.extend_from_slice(&encode_number(*n));
            }
            RecordValue::Key(key) => {
                out.push(1);
                out.extend_from_slice(key);
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! { assert(out@ =~= previous + field_bytes(values@[index as int])); }
        index += 1;
        #[cfg(verus_keep_ghost)]
        proof! {
            let prefix = values@.take(index as int);
            assert(prefix.drop_last() =~= values@.take(index as int - 1));
            assert(prefix.last() == values@[index as int - 1]);
            encoded_length(prefix);
        }
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(values@.take(values@.len() as int) =~= values@); }
    out
}
#[cfg(verus_keep_ghost)]
verus! {
pub type ModelField = (bool, i128, Seq<u8>);
pub open spec fn field_model(field:RecordValue) -> ModelField {
    match field {
        RecordValue::Number(number) => (true, number, seq![]),
        RecordValue::Key(key) => (false, 0, key@),
    }
}
pub open spec fn record_models(record:Seq<RecordValue>) -> Seq<ModelField> {
    record.map_values(|field:RecordValue| field_model(field))
}
pub open spec fn prepend(prefix:Seq<ModelField>, suffix:Option<Seq<ModelField>>) -> Option<Seq<ModelField>> {
    match suffix { Some(suffix) => Some(prefix + suffix), None => None }
}
pub open spec fn parsed(bytes:Seq<u8>) -> Option<Seq<ModelField>>
    decreases bytes.len(),
{
    if bytes.len() == 0 { Some(seq![]) }
    else if bytes[0] == 0 && bytes.len() >= 17 {
        prepend(seq![(true, super::runtime_codec::bits(bytes.subrange(1,17)) as i128, seq![])],
            parsed(bytes.subrange(17,bytes.len() as int)))
    } else if bytes[0] == 1 && bytes.len() >= 33 {
        prepend(seq![(false, 0, bytes.subrange(1,33))], parsed(bytes.subrange(33,bytes.len() as int)))
    } else { None }
}
proof fn prepend_associative(a:Seq<ModelField>,b:Seq<ModelField>,suffix:Option<Seq<ModelField>>)
    ensures prepend(a,prepend(b,suffix)) == prepend(a+b,suffix),
{
    if let Some(c) = suffix { assert(a + (b + c) =~= (a + b) + c); }
}
}
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (match result { Some(record) => Some(record_models(record@)), None => None }) == parsed(input@),
))]
/// Decode a complete tagged record, refusing malformed or trailing fields.
pub fn decode_record(input: &[u8]) -> Option<Vec<RecordValue>> {
    let mut bytes = input;
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost original = bytes@; }
    let mut out = Vec::new();
    #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant original == input@, out@.len() + bytes@.len() <= original.len(),
            parsed(original) == prepend(record_models(out@), parsed(bytes@)),
        decreases bytes.len(),
    ))]
    while !bytes.is_empty() {
        let tag = bytes[0];
        let rest = &bytes[1..];
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost previous_bytes = bytes@; let ghost previous_out = out@; }
        match tag {
            0 if rest.len() >= 16 => {
                let mut number = [0u8; 16];
                number.copy_from_slice(&rest[..16]);
                out.push(RecordValue::Number(decode_number(number)));
                bytes = &rest[16..];
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(number@ =~= previous_bytes.subrange(1,17));
                    assert(bytes@ =~= previous_bytes.subrange(17,previous_bytes.len() as int));
                    assert(parsed(previous_bytes) == prepend(seq![field_model(out@.last())], parsed(bytes@)));
                }
            }
            1 if rest.len() >= 32 => {
                let mut key = [0u8; 32];
                key.copy_from_slice(&rest[..32]);
                out.push(RecordValue::Key(key));
                bytes = &rest[32..];
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(key@ =~= previous_bytes.subrange(1,33));
                    assert(bytes@ =~= previous_bytes.subrange(33,previous_bytes.len() as int));
                    assert(parsed(previous_bytes) == prepend(seq![field_model(out@.last())], parsed(bytes@)));
                }
            }
            _ => {
                #[cfg(verus_keep_ghost)]
                proof! { assert(parsed(bytes@) == None); }
                return None;
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(record_models(out@) =~= record_models(previous_out).push(field_model(out@.last())));
            assert(record_models(out@) =~= record_models(previous_out) + seq![field_model(out@.last())]);
            prepend_associative(record_models(previous_out), seq![field_model(out@.last())], parsed(bytes@));
        }
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(bytes@.len() == 0);
        assert(parsed(bytes@) == Some(Seq::<ModelField>::empty()));
        assert(parsed(input@) == prepend(record_models(out@), parsed(bytes@)));
        assert(record_models(out@) + Seq::<ModelField>::empty() =~= record_models(out@));
    }
    Some(out)
}
impl Layout {
    #[cfg_attr(verus_keep_ghost,verus_spec(result =>
    requires self.well_formed(), vstd::laws_cmp::obeys_cmp::<Key>(),
    ensures match result { Ok(framed) => self.framed_from(state,command,context,storage.contents(),&framed) && self.framed_valid(&framed), Err(_) => true },
))]
    /// Frame a decision from scalar inputs and exactly its named storage reads.
    pub fn frame(
        &self,
        state: &BTreeMap<u16, i128>,
        command: &Input,
        context: &Input,
        storage: &impl Entries,
    ) -> Result<Framed, ContractError> {
        let mut tuple = self.frame_scalars(state, command, context)?;
        let mut actual = self.frame_input_keys(command, context)?;
        let mut named = BTreeMap::new();
        let mut entry_keys = Vec::new();
        let mut cursor = 0usize;
        #[cfg(verus_keep_ghost)]
        proof! {
            broadcast use vstd::seq_lib::group_seq_properties;
            assert forall|index:usize| actual@.contains_key(index) implies index < self.positions@.len() by {
                assert(self.input_position(index as int));
                assert(self.input_indices().contains(index));
                let i = choose|i:int| 0 <= i < self.input_indices().len() && self.input_indices()[i] == index;
                assert(self.inputs@[i].0 == index);
            }
        }
        #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant self.well_formed(), vstd::laws_cmp::obeys_cmp::<Key>(), cursor <= self.entries@.len(),
            tuple@.len() == self.positions@.len(),
            self.scalars_bound(state@,command,context,tuple@),
            self.inputs_bound(command,context,actual@,self.inputs@.len() as int),
            self.records_bound(command,context,storage.contents(),entry_keys@,tuple@,actual@,cursor as int),
            self.names_bound(entry_keys@,named@,cursor as int),
            self.actual_allowed(actual@,cursor as int),
        decreases self.entries@.len() - cursor,
    ))]
        while cursor < self.entries.len() {
            let entry = &self.entries[cursor];
            let key = input_key(command, context, entry.key_field.0, entry.key_field.1)?;
            #[cfg(verus_keep_ghost)]
            proof_decl! {
                let ghost previous_named = named@;
                let ghost previous_keys = entry_keys@;
                let ghost previous_tuple = tuple@;
                let ghost previous_actual = actual@;
            }
            add_named(&mut named, entry.map, key);
            entry_keys.push(key);
            #[cfg(verus_keep_ghost)]
            proof! {
                self.names_step(previous_keys,previous_named,named@,cursor as int,key);
                self.new_entry_clear(actual@,cursor as int);
            }
            let bytes = storage.record(entry.map, &key);
            apply_record(entry, &self.positions, bytes, &mut tuple, &mut actual)?;
            #[cfg(verus_keep_ghost)]
            proof! {
                broadcast use vstd::seq_lib::group_seq_properties;
                assert forall|i:int| #![trigger self.entries@[i]] 0 <= i < cursor implies {
                    let previous_entry = self.entries@[i];
                    let input = selected_keys(command,context,previous_entry.key_field.0);
                    input.contains_key(previous_entry.key_field.1) && entry_keys@[i] == input[previous_entry.key_field.1] &&
                        record_bound(previous_entry,self.positions@,stored(storage.contents(),previous_entry.map,entry_keys@[i]),tuple@,actual@)
                } by {
                    let previous_entry = self.entries@[i];
                    assert forall|p:int| 0 <= p < previous_tuple.len() implies !(touches(previous_entry,p) && touches(*entry,p)) by {};
                    record_bound_preserved(previous_entry,*entry,self.positions@,stored(storage.contents(),previous_entry.map,previous_keys[i]),
                        previous_tuple,tuple@,previous_actual,actual@);
                }
                assert forall|i:int| 0 <= i < self.scalars@.len() implies
                    state@.contains_key(field_id(self.positions@[self.scalars@[i] as int])) &&
                    tuple@[self.scalars@[i] as int] == state@[field_id(self.positions@[self.scalars@[i] as int])] by {
                    assert(self.scalars@.contains(self.scalars@[i]));
                    assert(!touches(*entry,self.scalars@[i] as int));
                }
                assert forall|i:int| 0 <= i < self.positions@.len() && self.positions@[i].source != Source::State && !self.input_position(i) implies
                    selected_scalars(command,context,self.positions@[i].source).contains_key(field_id(self.positions@[i])) &&
                    tuple@[i] == selected_scalars(command,context,self.positions@[i].source)[field_id(self.positions@[i])] by {
                    assert(!touches(*entry,i));
                }
                assert forall|i:int| #![trigger self.inputs@[i]] 0 <= i < self.inputs@.len() implies {
                    let index = self.inputs@[i].0;
                    let keys = selected_keys(command,context,self.positions@[index as int].source);
                    keys.contains_key(field_id(self.positions@[index as int])) && actual@.contains_key(index) &&
                        actual@[index] == (self.inputs@[i].1,keys[field_id(self.positions@[index as int])])
                } by {
                    let index = self.inputs@[i].0;
                    assert(!touches(*entry,index as int));
                }
                assert forall|index:usize| actual@.contains_key(index) implies index < self.positions@.len() &&
                    (self.input_position(index as int) || exists|i:int| 0 <= i < cursor+1 && value_indices(self.entries@[i]).contains(index)) by {
                    if value_indices(*entry).contains(index) {
                        let field = choose|field:int| 0 <= field < value_indices(*entry).len() && value_indices(*entry)[field] == index;
                        assert(entry.values@[field].0 == index);
                        assert(0 <= cursor < cursor+1);
                    } else {
                        assert(previous_actual.contains_key(index));
                        if !self.input_position(index as int) {
                            let i = choose|i:int| 0 <= i < cursor && value_indices(self.entries@[i]).contains(index);
                            assert(i < cursor+1);
                        }
                    }
                }
                assert(self.actual_allowed(actual@,cursor as int+1));
            }
            cursor += 1;
        }
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost before_classes = tuple@; let ghost actual_keys = actual@; }
        #[cfg(verus_keep_ghost)]
        proof! { assert(self.raw_frame(state@,command,context,storage.contents(),tuple@,actual@,entry_keys@,named@));
            assert forall|index:usize| actual@.contains_key(index) implies index < tuple@.len() by {
                assert(self.actual_allowed(actual@,self.entries@.len() as int));
            }
        }
        let classes = intern_keys(&actual, &mut tuple);
        let framed = Framed {
            tuple,
            named,
            classes,
            entry_keys,
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(self.raw_frame(state@,command,context,storage.contents(),before_classes,actual_keys,framed.entry_keys@,framed.named@));
            assert(class_framed(actual_keys,before_classes,framed.tuple@,framed.classes@));
            assert(self.framed_from(state,command,context,storage.contents(),&framed));
            self.frame_is_valid(state,command,context,storage.contents(),&framed);
        }
        Ok(framed)
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result =>
    requires self.framed_valid(framed), vstd::laws_cmp::obeys_cmp::<Key>(),
    ensures match result { Ok(writes) => self.writeback(framed,post,&writes), Err(_) => true },
))]
    /// Produce the scalar state and one agreed update per changed named key.
    pub fn writes(
        &self,
        framed: &Framed,
        post: &BTreeMap<u16, i128>,
    ) -> Result<Writes, ContractError> {
        #[cfg(verus_keep_ghost)]
        proof! { broadcast use vstd::laws_cmp::group_laws_cmp; }
        let mut writes = Writes {
            scalars: BTreeMap::new(),
            entries: Vec::new(),
        };
        let mut scalar = 0usize;
        #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant self.framed_valid(framed), scalar <= self.scalars@.len(),
            writes.entries@.len() == 0,
            writes.scalars@ == self.scalar_output(framed,post,self.scalars@.take(scalar as int)),
        decreases self.scalars@.len() - scalar,
    ))]
        while scalar < self.scalars.len() {
            let index = self.scalars[scalar];
            let value = read_post(&self.positions, &framed.tuple, Some(post), index);
            writes
                .scalars
                .insert(self.positions[index].field.unwrap_or(0), value);
            scalar += 1;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(self.scalars@.take(scalar as int).drop_last() =~= self.scalars@.take(scalar as int - 1));
                assert(self.scalars@.take(scalar as int).last() == index);
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! { assert(self.scalars@.take(self.scalars@.len() as int) =~= self.scalars@); }
        let mut done = BTreeSet::new();
        let mut cursor = 0usize;
        #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant self.framed_valid(framed), vstd::laws_cmp::obeys_cmp::<Key>(),
            vstd::laws_cmp::obeys_cmp::<(u32,Key)>(), cursor <= self.entries@.len(),
            writes.scalars@ == self.scalar_output(framed,post,self.scalars@),
            done@ == self.named_prefix(framed,cursor as int),
            self.output_valid(framed,post,writes.entries@), distinct_output(writes.entries@),
            forall|key:(u32,Key)| output_pairs(writes.entries@).contains(key) ==> done@.contains(key),
            forall|i:int| 0 <= i < self.entries@.len() && done@.contains(self.pair(framed,i)) ==>
                (self.group_changed(framed,post,i) == output_pairs(writes.entries@).contains(self.pair(framed,i))),
        decreases self.entries@.len() - cursor,
    ))]
        while cursor < self.entries.len() {
            let i = cursor;
            let entry = &self.entries[i];
            let key = framed.entry_keys[i];
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost previous_done = done@; let ghost previous_entries = writes.entries@; }
            cursor += 1;
            #[cfg(verus_keep_ghost)]
            proof! { self.named_prefix_step(framed,i as int); }
            if !done.insert((entry.map, key)) {
                continue;
            }
            let selected = self.merge_group(framed, post, i)?;
            let Some((chosen, raw)) = selected else {
                continue;
            };
            let record = record_for_view(&self.entries[chosen], &raw, &framed.classes)?;
            writes.entries.push((entry.map, key, record));
            #[cfg(verus_keep_ghost)]
            proof! {
                broadcast use vstd::seq_lib::lemma_seq_contains_after_push;
                assert(output_pairs(writes.entries@) =~= output_pairs(previous_entries).push((entry.map,key)));
                assert(self.group_changed(framed,post,i as int));
                assert forall|j:int| 0 <= j < self.entries@.len() && self.group(framed,chosen as int,j) && self.changed(framed,post,j)
                    implies self.candidate(framed,post,j) == self.candidate(framed,post,chosen as int) by {
                    assert(self.group(framed,i as int,j));
                }
                assert(self.pair(framed,chosen as int) == (entry.map,key));
                assert(self.changed(framed,post,chosen as int));
                assert(raw_record(self.entries@[chosen as int],self.candidate(framed,post,chosen as int),framed.classes@,writes.entries@.last().2));
                assert(self.output_item(framed,post,writes.entries@.last())) by {
                    assert(exists|c:int| 0 <= c < self.entries@.len() && self.pair(framed,c) == (entry.map,key) &&
                        self.changed(framed,post,c) &&
                        raw_record(self.entries@[c],self.candidate(framed,post,c),framed.classes@,writes.entries@.last().2) &&
                        forall|j:int| 0 <= j < self.entries@.len() && self.group(framed,c,j) && self.changed(framed,post,j) ==>
                            self.candidate(framed,post,j) == self.candidate(framed,post,c));
                }
                assert forall|w:int| 0 <= w < writes.entries@.len() implies self.output_item(framed,post,writes.entries@[w]) by {
                    if w < previous_entries.len() { assert(self.output_item(framed,post,previous_entries[w])); }
                }
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            broadcast use vstd::seq_lib::group_seq_properties;
            let names = Seq::new(self.entries@.len(), |i:int| self.pair(framed,i));
            assert forall|i:int| 0 <= i < self.entries@.len() implies done@.contains(self.pair(framed,i)) by {
                assert(names[i] == self.pair(framed,i));
                assert(names.contains(self.pair(framed,i)));
            }
        }
        Ok(writes)
    }
}

#[cfg(verus_keep_ghost)]
verus! {
spec fn entry_valid(entry: EntryLayout, length: int) -> bool {
    &&& 0 <= entry.presence < length
    &&& entry.values@.len() < usize::MAX
    &&& forall|i:int| #![trigger entry.values@[i]] 0 <= i < entry.values@.len() ==> 0 <= entry.values@[i].0 < length
}
spec fn post_map(post: Option<&BTreeMap<u16,i128>>) -> Option<Map<u16,i128>> {
    match post { Some(post) => Some(post@), None => None }
}
spec fn post_value(positions: Seq<Position>, before: Seq<i128>,
    post: Option<Map<u16,i128>>, index: int) -> i128 {
    match (positions[index].field, post) {
        (Some(field), Some(post)) => if post.contains_key(field) { post[field] } else { before[index] },
        _ => before[index],
    }
}
spec fn entry_image(entry: EntryLayout, positions: Seq<Position>, before: Seq<i128>,
    post: Option<Map<u16,i128>>) -> Seq<i128> {
    Seq::new(entry.values@.len() + 1, |index:int|
        post_value(positions, before, post,
            if index == 0 { entry.presence as int } else { entry.values@[index - 1].0 as int }))
}
}
#[cfg_attr(verus_keep_ghost,verus_spec(result =>
    requires index < positions@.len(), positions@.len() == before@.len(),
    ensures result == post_value(positions@, before@, post_map(post), index as int),
))]
fn read_post(
    positions: &[Position],
    before: &[i128],
    post: Option<&BTreeMap<u16, i128>>,
    index: usize,
) -> i128 {
    match (post, positions[index].field) {
        (Some(post), Some(field)) => match post.get(&field) {
            Some(value) => *value,
            None => before[index],
        },
        _ => before[index],
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result =>
    requires entry_valid(*entry, positions@.len() as int), positions@.len() == before@.len(),
    ensures result@ == entry_image(*entry, positions@, before@, post_map(post)),
))]
fn entry_view(
    entry: &EntryLayout,
    positions: &[Position],
    before: &[i128],
    post: Option<&BTreeMap<u16, i128>>,
) -> Vec<i128> {
    let mut result = Vec::new();
    result.push(read_post(positions, before, post, entry.presence));
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant index <= entry.values@.len(), entry_valid(*entry, positions@.len() as int),
            positions@.len() == before@.len(),
            result@.len() == index + 1,
            forall|j:int| 0 <= j < result@.len() ==> result@[j] ==
                entry_image(*entry, positions@, before@, post_map(post))[j],
        decreases entry.values@.len() - index,
    ))]
    while index < entry.values.len() {
        result.push(read_post(positions, before, post, entry.values[index].0));
        index += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(result@ =~= entry_image(*entry, positions@, before@, post_map(post))); }
    result
}

#[cfg_attr(verus_keep_ghost,verus_spec(result =>
    ensures match result {
        Some(index) => index < keys@.len() && keys@[index as int] == *key &&
            forall|i:int| 0 <= i < index ==> keys@[i] != *key,
        None => forall|i:int| 0 <= i < keys@.len() ==> keys@[i] != *key,
    },
))]
fn key_index(keys: &[Key], key: &Key) -> Option<usize> {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant index <= keys.len(),
        forall|i:int| 0 <= i < index ==> keys@[i] != *key,
        decreases keys.len() - index,))]
    while index < keys.len() {
        if keys[index] == *key {
            #[cfg(verus_keep_ghost)]
            proof! { assert(keys@[index as int] =~= *key); }
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost,verus_spec(result => ensures result@ == super::runtime_codec::signed_bytes(number),))]
fn encode_number(number: i128) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant index <= 16,
        forall|j:int| 0 <= j < index ==> bytes@[j] == super::runtime_codec::byte_at(number, j),
        decreases 16 - index,))]
    while index < 16 {
        bytes[index] = ((number as u128) >> ((15 - index) * 8)) as u8;
        index += 1;
    }
    bytes
}
#[cfg_attr(verus_keep_ghost,verus_spec(result =>
    ensures result == super::runtime_codec::bits(bytes@) as i128,
        super::runtime_codec::signed_bytes(result) == bytes@,
))]
fn decode_number(bytes: [u8; 16]) -> i128 {
    let mut number = 0u128;
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant index <= 16,
        number == super::runtime_codec::bits(bytes@.take(index as int)),
        decreases 16 - index,))]
    while index < 16 {
        number = (number << 8) | (bytes[index] as u128);
        index += 1;
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(bytes@.take(index as int).drop_last() =~= bytes@.take(index as int - 1));
            assert(bytes@.take(index as int).last() == bytes@[index as int - 1]);
        }
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(bytes@.take(16) =~= bytes@);
        super::runtime_codec::decode_encode(bytes@);
    }
    number as i128
}

#[cfg(verus_keep_ghost)]
verus! {
impl Layout {
    pub closed spec fn framed_valid(&self, framed: &Framed) -> bool {
        &&& framed.tuple@.len() == self.positions@.len()
        &&& framed.entry_keys@.len() == self.entries@.len()
        &&& forall|i:int| #![trigger self.scalars@[i]] 0 <= i < self.scalars@.len() ==>
            self.scalars@[i] < self.positions@.len()
        &&& forall|i:int| #![trigger self.entries@[i]] 0 <= i < self.entries@.len() ==>
            entry_valid(self.entries@[i], self.positions@.len() as int) &&
            33 * self.entries@[i].values@.len() <= usize::MAX
        &&& forall|i:int,j:int| 0 <= i < self.entries@.len() && 0 <= j < self.entries@.len() &&
            self.group(framed, i, j) ==>
            entry_image(self.entries@[i], self.positions@, framed.tuple@, None) ==
                entry_image(self.entries@[j], self.positions@, framed.tuple@, None)
    }
    spec fn group(&self, framed:&Framed, i:int, j:int) -> bool {
        self.entries@[i].map == self.entries@[j].map && framed.entry_keys@[i] == framed.entry_keys@[j]
    }
    spec fn candidate(&self,framed:&Framed,post:&BTreeMap<u16,i128>,i:int) -> Seq<i128> {
        entry_image(self.entries@[i],self.positions@,framed.tuple@,Some(post@))
    }
    spec fn changed(&self,framed:&Framed,post:&BTreeMap<u16,i128>,i:int) -> bool {
        self.candidate(framed,post,i) != entry_image(self.entries@[i],self.positions@,framed.tuple@,None)
    }
    spec fn group_changed(&self,framed:&Framed,post:&BTreeMap<u16,i128>,i:int) -> bool {
        exists|j:int| 0 <= j < self.entries@.len() && self.group(framed,i,j) && self.changed(framed,post,j)
    }
    spec fn chosen(&self,framed:&Framed,post:&BTreeMap<u16,i128>,i:int,chosen:int,raw:Seq<i128>,upto:int) -> bool {
        &&& 0 <= chosen < upto
        &&& self.group(framed,i,chosen)
        &&& self.changed(framed,post,chosen)
        &&& raw == self.candidate(framed,post,chosen)
        &&& forall|j:int| 0 <= j < upto && self.group(framed,i,j) && self.changed(framed,post,j) ==>
            self.candidate(framed,post,j) == raw
    }
}
}
#[cfg_attr(verus_keep_ghost,verus_spec(result => ensures result == (left@ == right@),))]
fn equal_values(left: &[i128], right: &[i128]) -> bool {
    let result = left == right;
    #[cfg(verus_keep_ghost)]
    proof! { if result { assert(left@ =~= right@); } }
    result
}
impl Layout {
    #[cfg_attr(verus_keep_ghost,verus_spec(result =>
        requires i < self.entries@.len(), j < self.entries@.len(),
            framed.entry_keys@.len() == self.entries@.len(),
        ensures result == self.group(framed,i as int,j as int),
    ))]
    fn same_group(&self, framed: &Framed, i: usize, j: usize) -> bool {
        let result = self.entries[i].map == self.entries[j].map
            && framed.entry_keys[i] == framed.entry_keys[j];
        #[cfg(verus_keep_ghost)]
        proof! { if result { assert(framed.entry_keys@[i as int] =~= framed.entry_keys@[j as int]); } }
        result
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result =>
        requires self.framed_valid(framed), i < self.entries@.len(),
        ensures match result {
            Ok(None) => !self.group_changed(framed,post,i as int),
            Ok(Some((selected,raw))) => self.chosen(framed,post,i as int,selected as int,raw@,self.entries@.len() as int),
            Err(_) => exists|a:int,b:int| 0 <= a < self.entries@.len() && 0 <= b < self.entries@.len() &&
                self.group(framed,i as int,a) && self.group(framed,i as int,b) &&
                self.changed(framed,post,a) && self.changed(framed,post,b) &&
                self.candidate(framed,post,a) != self.candidate(framed,post,b),
        },
    ))]
    fn merge_group(
        &self,
        framed: &Framed,
        post: &BTreeMap<u16, i128>,
        i: usize,
    ) -> Result<Option<(usize, Vec<i128>)>, ContractError> {
        let before = entry_view(&self.entries[i], &self.positions, &framed.tuple, None);
        let mut written: Option<(usize, Vec<i128>)> = None;
        let mut cursor = 0usize;
        #[cfg_attr(verus_keep_ghost,verus_spec(
            invariant self.framed_valid(framed), i < self.entries@.len(), cursor <= self.entries@.len(),
                before@ == entry_image(self.entries@[i as int],self.positions@,framed.tuple@,None),
                match written {
                    None => forall|j:int| 0 <= j < cursor && self.group(framed,i as int,j) ==> !self.changed(framed,post,j),
                    Some((selected,raw)) => self.chosen(framed,post,i as int,selected as int,raw@,cursor as int),
                },
            decreases self.entries@.len() - cursor,
        ))]
        while cursor < self.entries.len() {
            if self.same_group(framed, i, cursor) {
                let candidate = entry_view(
                    &self.entries[cursor],
                    &self.positions,
                    &framed.tuple,
                    Some(post),
                );
                if !equal_values(&candidate, &before) {
                    match &written {
                        Some((_, value)) if !equal_values(value, &candidate) => {
                            return Err(ContractError::new(
                                "collections runtime",
                                format!(
                                    "alias merge: entries name map {} at one key and write different values",
                                    self.entries[i].map
                                ),
                            ));
                        }
                        None => written = Some((cursor, candidate)),
                        _ => {}
                    }
                }
            }
            cursor += 1;
        }
        Ok(written)
    }
}

#[cfg(verus_keep_ghost)]
verus! {
spec fn model_field_bytes(value:ModelField) -> Seq<u8> {
    if value.0 { seq![0u8] + super::runtime_codec::signed_bytes(value.1) }
    else { seq![1u8] + value.2 }
}
spec fn model_encoded(values:Seq<ModelField>) -> Seq<u8>
    decreases values.len(),
{
    if values.len() == 0 { seq![] }
    else { model_encoded(values.drop_last()) + model_field_bytes(values.last()) }
}
proof fn encoding_models(values:Seq<RecordValue>)
    ensures model_encoded(record_models(values)) == encoded(values),
    decreases values.len(),
{
    if values.len() > 0 {
        encoding_models(values.drop_last());
        assert(record_models(values).drop_last() =~= record_models(values.drop_last()));
        assert(record_models(values).last() == field_model(values.last()));
        assert(model_field_bytes(field_model(values.last())) == field_bytes(values.last()));
    }
}
spec fn raw_keys_valid(entry:EntryLayout,raw:Seq<i128>,classes:Map<u32,Vec<Key>>,upto:int) -> bool {
    forall|index:int| #![trigger entry.values@[index]] 0 <= index < upto ==> match entry.values@[index].1 {
        None => true,
        Some(kind) => classes.contains_key(kind) &&
            0 <= raw[index+1] < classes[kind]@.len(),
    }
}
spec fn raw_fields(entry:EntryLayout,raw:Seq<i128>,classes:Map<u32,Vec<Key>>) -> Seq<ModelField> {
    Seq::new(entry.values@.len(), |index:int| match entry.values@[index].1 {
        None => (true,raw[index+1],seq![]),
        Some(kind) => (false,0,classes[kind]@[raw[index+1] as int]@),
    })
}
pub open spec fn record_view(record:Option<Vec<u8>>) -> Option<Seq<u8>> {
    match record { Some(bytes) => Some(bytes@), None => None }
}
spec fn raw_record(entry:EntryLayout,raw:Seq<i128>,classes:Map<u32,Vec<Key>>,record:Option<Vec<u8>>) -> bool {
    if raw[0] != 1 { record_view(record) == None }
    else { raw_keys_valid(entry,raw,classes,entry.values@.len() as int) &&
        record_view(record) == Some(model_encoded(raw_fields(entry,raw,classes))) }
}
}
#[cfg_attr(verus_keep_ghost,verus_spec(result =>
    requires raw@.len() == entry.values@.len() + 1,
        33 * entry.values@.len() <= usize::MAX,
    ensures match result {
        Ok(record) => raw_record(*entry,raw@,classes@,record),
        Err(_) => raw@[0] == 1 && !raw_keys_valid(*entry,raw@,classes@,entry.values@.len() as int),
    },
))]
fn record_for_view(
    entry: &EntryLayout,
    raw: &[i128],
    classes: &BTreeMap<u32, Vec<Key>>,
) -> Result<Option<Vec<u8>>, ContractError> {
    if raw[0] != 1 {
        return Ok(None);
    }
    let mut fields = Vec::new();
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant index <= entry.values@.len(), raw@.len() == entry.values@.len() + 1,
            raw@[0] == 1, 33 * entry.values@.len() <= usize::MAX,
            fields@.len() == index,
            raw_keys_valid(*entry,raw@,classes@,index as int),
            forall|i:int| 0 <= i < index ==> field_model(fields@[i]) == raw_fields(*entry,raw@,classes@)[i],
        decreases entry.values@.len() - index,
    ))]
    while index < entry.values.len() {
        let value = raw[index + 1];
        let field = match entry.values[index].1 {
            None => RecordValue::Number(value),
            Some(kind) => {
                let invalid = || {
                    ContractError::new(
                        "collections runtime",
                        format!("key class {value} names no key"),
                    )
                };
                let keys = match classes.get(&kind) {
                    Some(keys) => keys,
                    None => return Err(invalid()),
                };
                let count = keys.len();
                if value < 0 || value >= count as i128 {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        assert(keys@.len() <= usize::MAX);
                        assert(!(0 <= raw@[index as int+1] < classes@[kind]@.len()));
                    }
                    return Err(invalid());
                }
                let class = value as usize;
                let key = match keys.get(class) {
                    Some(key) => key,
                    None => return Err(invalid()),
                };
                RecordValue::Key(*key)
            }
        };
        fields.push(field);
        index += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(record_models(fields@) =~= raw_fields(*entry,raw@,classes@));
        encoding_models(fields@);
    }
    Ok(Some(encode_record(&fields)))
}

#[cfg(verus_keep_ghost)]
verus! {
spec fn output_pairs(entries:Seq<(u32,Key,Option<Vec<u8>>)>) -> Seq<(u32,Key)> {
    entries.map_values(|entry:(u32,Key,Option<Vec<u8>>)| (entry.0,entry.1))
}
spec fn distinct_output(entries:Seq<(u32,Key,Option<Vec<u8>>)>) -> bool {
    forall|i:int,j:int| 0 <= i < j < entries.len() ==>
        output_pairs(entries)[i] != output_pairs(entries)[j]
}
impl Layout {
    spec fn pair(&self,framed:&Framed,i:int) -> (u32,Key) {
        (self.entries@[i].map,framed.entry_keys@[i])
    }
    spec fn named_prefix(&self,framed:&Framed,count:int) -> Set<(u32,Key)> {
        Seq::new(count as nat, |i:int| self.pair(framed,i)).to_set()
    }
    proof fn named_prefix_step(&self,framed:&Framed,count:int)
        requires 0 <= count < self.entries@.len(), framed.entry_keys@.len() == self.entries@.len(),
        ensures self.named_prefix(framed,count+1) == self.named_prefix(framed,count).insert(self.pair(framed,count)),
    {
        broadcast use vstd::seq_lib::group_seq_properties;
        let before = Seq::new(count as nat, |i:int| self.pair(framed,i));
        let after = Seq::new((count+1) as nat, |i:int| self.pair(framed,i));
        assert(after =~= before.push(self.pair(framed,count)));
        assert(self.named_prefix(framed,count+1) =~= self.named_prefix(framed,count).insert(self.pair(framed,count)));
    }
    spec fn scalar_output(&self,framed:&Framed,post:&BTreeMap<u16,i128>,indices:Seq<usize>) -> Map<u16,i128>
        decreases indices.len(),
    {
        if indices.len() == 0 { Map::empty() }
        else { self.scalar_output(framed,post,indices.drop_last()).insert(
            match self.positions@[indices.last() as int].field { Some(field) => field, None => 0 },
            post_value(self.positions@,framed.tuple@,Some(post@),indices.last() as int)) }
    }
    spec fn output_item(&self,framed:&Framed,post:&BTreeMap<u16,i128>,entry:(u32,Key,Option<Vec<u8>>)) -> bool {
        exists|i:int| 0 <= i < self.entries@.len() && self.pair(framed,i) == (entry.0,entry.1) &&
            self.changed(framed,post,i) &&
            raw_record(self.entries@[i],self.candidate(framed,post,i),framed.classes@,entry.2) &&
            forall|j:int| 0 <= j < self.entries@.len() && self.group(framed,i,j) && self.changed(framed,post,j) ==>
                self.candidate(framed,post,j) == self.candidate(framed,post,i)
    }
    spec fn output_valid(&self,framed:&Framed,post:&BTreeMap<u16,i128>,entries:Seq<(u32,Key,Option<Vec<u8>>)>) -> bool {
        forall|w:int| #![trigger entries[w]] 0 <= w < entries.len() ==> self.output_item(framed,post,entries[w])
    }
    pub closed spec fn writeback(&self,framed:&Framed,post:&BTreeMap<u16,i128>,writes:&Writes) -> bool {
        &&& writes.scalars@ == self.scalar_output(framed,post,self.scalars@)
        &&& self.output_valid(framed,post,writes.entries@)
        &&& distinct_output(writes.entries@)
        &&& forall|i:int| 0 <= i < self.entries@.len() ==>
            (self.group_changed(framed,post,i) == output_pairs(writes.entries@).contains(self.pair(framed,i)))
    }
}
}

#[cfg(verus_keep_ghost)]
verus! {
spec fn value_indices(entry:EntryLayout) -> Seq<usize> {
    entry.values@.map_values(|value:(usize,Option<u32>)| value.0)
}
spec fn touches(entry:EntryLayout,index:int) -> bool {
    index == entry.presence || value_indices(entry).contains(index as usize)
}
spec fn entry_well_formed(entry:EntryLayout,length:int) -> bool {
    &&& entry_valid(entry,length)
    &&& 33 * entry.values@.len() <= usize::MAX
    &&& forall|i:int| 0 <= i < entry.values@.len() ==> entry.values@[i].0 != entry.presence
    &&& forall|i:int,j:int| 0 <= i < j < entry.values@.len() ==> entry.values@[i].0 != entry.values@[j].0
}
spec fn field_bound(field:(usize,Option<u32>),model:ModelField,tuple:Seq<i128>,actual:Map<usize,(u32,Key)>) -> bool {
    match field.1 {
        None => model.0 && tuple[field.0 as int] == model.1 && !actual.contains_key(field.0),
        Some(kind) => !model.0 && actual.contains_key(field.0) &&
            actual[field.0].0 == kind && actual[field.0].1@ == model.2,
    }
}
spec fn record_bound(entry:EntryLayout,positions:Seq<Position>,bytes:Option<Seq<u8>>,
    tuple:Seq<i128>,actual:Map<usize,(u32,Key)>) -> bool {
    match bytes {
        None => tuple[entry.presence as int] == 0 && forall|i:int| 0 <= i < entry.values@.len() ==>
            tuple[entry.values@[i].0 as int] == crate::internals::review::domain::least_or_zero(positions[entry.values@[i].0 as int].domain) &&
                !actual.contains_key(entry.values@[i].0),
        Some(bytes) => match parsed(bytes) {
            None => false,
            Some(fields) => tuple[entry.presence as int] == 1 && fields.len() == entry.values@.len() &&
                forall|i:int| 0 <= i < fields.len() ==> field_bound(entry.values@[i],fields[i],tuple,actual),
        },
    }
}
spec fn record_footprint(entry:EntryLayout,before:Seq<i128>,after:Seq<i128>,
    before_actual:Map<usize,(u32,Key)>,after_actual:Map<usize,(u32,Key)>) -> bool {
    &&& after.len() == before.len()
    &&& forall|i:int| 0 <= i < after.len() && !touches(entry,i) ==> after[i] == before[i]
    &&& forall|i:usize| #![trigger after_actual.contains_key(i)] !value_indices(entry).contains(i) ==>
        after_actual.contains_key(i) == before_actual.contains_key(i)
    &&& forall|i:usize| #![trigger after_actual[i]] !value_indices(entry).contains(i) && after_actual.contains_key(i) ==>
        after_actual[i] == before_actual[i]
}
}
#[cfg_attr(verus_keep_ghost,verus_spec(result => ensures result@ == value_indices(*entry),))]
fn entry_indices(entry: &EntryLayout) -> Vec<usize> {
    let mut result = Vec::new();
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant index <= entry.values@.len(), result@ == value_indices(*entry).take(index as int),
        decreases entry.values@.len() - index,
    ))]
    while index < entry.values.len() {
        result.push(entry.values[index].0);
        index += 1;
        #[cfg(verus_keep_ghost)]
        proof! { assert(value_indices(*entry).take(index as int) =~=
        value_indices(*entry).take(index as int - 1).push(entry.values@[index as int - 1].0)); }
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(value_indices(*entry).take(entry.values@.len() as int) =~= value_indices(*entry)); }
    result
}
#[cfg_attr(verus_keep_ghost,verus_spec(result =>
    requires entry_well_formed(*entry,positions@.len() as int), old(tuple)@.len() == positions@.len(),
        forall|i:int| 0 <= i < entry.values@.len() ==> !old(actual)@.contains_key(entry.values@[i].0),
    ensures result.is_ok() ==> record_bound(*entry,positions@,record_view(bytes),final(tuple)@,final(actual)@) &&
        record_footprint(*entry,old(tuple)@,final(tuple)@,old(actual)@,final(actual)@),
))]
fn apply_record(
    entry: &EntryLayout,
    positions: &[Position],
    bytes: Option<Vec<u8>>,
    tuple: &mut [i128],
    actual: &mut BTreeMap<usize, (u32, Key)>,
) -> Result<(), ContractError> {
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost initial_tuple = tuple@; let ghost initial_actual = actual@; }
    match bytes {
        None => {
            let indices = entry_indices(entry);
            super::canonical_absent_view(positions, entry.presence, &indices, tuple);
            #[cfg(verus_keep_ghost)]
            proof! {
                broadcast use vstd::seq_lib::group_seq_properties;
                assert forall|i:int| 0 <= i < entry.values@.len() implies
                    tuple@[entry.values@[i].0 as int] == crate::internals::review::domain::least_or_zero(positions@[entry.values@[i].0 as int].domain) &&
                        !actual@.contains_key(entry.values@[i].0) by {
                    assert(indices@[i] == entry.values@[i].0);
                    assert(indices@.contains(entry.values@[i].0));
                }
                assert(record_bound(*entry,positions@,None,tuple@,actual@));
                assert(record_footprint(*entry,initial_tuple,tuple@,initial_actual,actual@));
            }
            Ok(())
        }
        Some(bytes) => {
            let invalid = || {
                ContractError::new(
                    "collections runtime",
                    format!("map {} holds a malformed record", entry.map),
                )
            };
            let record = match decode_record(&bytes) {
                Some(record) => record,
                None => return Err(invalid()),
            };
            if record.len() != entry.values.len() {
                return Err(invalid());
            }
            tuple[entry.presence] = 1;
            let mut cursor = 0usize;
            #[cfg_attr(verus_keep_ghost,verus_spec(
                invariant entry_well_formed(*entry,positions@.len() as int),
                    tuple@.len() == positions@.len(), cursor <= entry.values@.len(),
                    record@.len() == entry.values@.len(),
                    parsed(bytes@) == Some(record_models(record@)),
                    initial_tuple == old(tuple)@, initial_actual == old(actual)@,
                    tuple@[entry.presence as int] == 1,
                    forall|i:int| 0 <= i < cursor ==> field_bound(entry.values@[i],field_model(record@[i]),tuple@,actual@),
                    forall|i:int| cursor <= i < entry.values@.len() ==> !actual@.contains_key(entry.values@[i].0),
                    record_footprint(*entry,initial_tuple,tuple@,initial_actual,actual@),
                decreases entry.values@.len() - cursor,
            ))]
            while cursor < entry.values.len() {
                let (index, kind) = entry.values[cursor];
                #[cfg(verus_keep_ghost)]
                proof_decl! { let ghost previous_tuple = tuple@; let ghost previous_actual = actual@; }
                #[cfg(verus_keep_ghost)]
                proof! { assert(record_footprint(*entry,initial_tuple,previous_tuple,initial_actual,previous_actual)); }
                match (kind, &record[cursor]) {
                    (None, RecordValue::Number(number)) => tuple[index] = *number,
                    (Some(kind), RecordValue::Key(key)) => {
                        actual.insert(index, (kind, *key));
                    }
                    _ => {
                        return Err(ContractError::new(
                            "collections runtime",
                            format!("map {} holds a mistyped record", entry.map),
                        ));
                    }
                }
                cursor += 1;
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert forall|i:int| 0 <= i < cursor implies field_bound(entry.values@[i],field_model(record@[i]),tuple@,actual@) by {
                        if i < cursor - 1 { assert(entry.values@[i].0 != index); }
                    }
                    assert forall|i:int| cursor <= i < entry.values@.len() implies !actual@.contains_key(entry.values@[i].0) by {
                        assert(entry.values@[i].0 != index);
                    }
                    broadcast use vstd::seq_lib::group_seq_properties;
                    assert(value_indices(*entry)[cursor as int - 1] == index);
                    assert(value_indices(*entry).contains(index));
                    assert forall|i:int| 0 <= i < tuple@.len() && !touches(*entry,i) implies tuple@[i] == initial_tuple[i] by {
                        assert(i != index);
                        assert(previous_tuple[i] == initial_tuple[i]);
                    }
                    assert forall|i:usize| !value_indices(*entry).contains(i) implies
                        actual@.contains_key(i) == initial_actual.contains_key(i) &&
                            (actual@.contains_key(i) ==> actual@[i] == initial_actual[i]) by {
                        assert(i != index);
                        assert(previous_actual.contains_key(i) == initial_actual.contains_key(i));
                        assert(actual@.contains_key(i) == previous_actual.contains_key(i));
                        if actual@.contains_key(i) {
                            assert(actual@[i] == previous_actual[i]);
                            assert(previous_actual[i] == initial_actual[i]);
                        }
                    }
                }
            }
            #[cfg(verus_keep_ghost)]
            proof! { assert(record_bound(*entry,positions@,Some(bytes@),tuple@,actual@)); }
            Ok(())
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {
spec fn selected_scalars(command:&Input,context:&Input,source:Source) -> Map<u16,i128> {
    if source == Source::Command { command.scalars@ } else { context.scalars@ }
}
spec fn selected_keys(command:&Input,context:&Input,source:Source) -> Map<u16,Key> {
    if source == Source::Command { command.keys@ } else { context.keys@ }
}
spec fn field_id(position:Position) -> u16 {
    match position.field { Some(field) => field, None => 0 }
}
impl Layout {
    spec fn input_indices(&self) -> Seq<usize> {
        self.inputs@.map_values(|input:(usize,u32)| input.0)
    }
    spec fn input_position(&self,index:int) -> bool { self.input_indices().contains(index as usize) }
    pub closed spec fn well_formed(&self) -> bool {
        &&& forall|i:int| 0 <= i < self.scalars@.len() ==> self.scalars@[i] < self.positions@.len() &&
            self.positions@[self.scalars@[i] as int].source == Source::State
        &&& forall|i:int| 0 <= i < self.inputs@.len() ==> self.inputs@[i].0 < self.positions@.len() &&
            self.positions@[self.inputs@[i].0 as int].source != Source::State
        &&& forall|i:int,j:int| 0 <= i < j < self.inputs@.len() ==> self.inputs@[i].0 != self.inputs@[j].0
        &&& forall|i:int| 0 <= i < self.entries@.len() ==> entry_well_formed(self.entries@[i],self.positions@.len() as int)
        &&& forall|i:int,p:int| 0 <= i < self.entries@.len() && 0 <= p < self.positions@.len() && touches(self.entries@[i],p) ==>
            self.positions@[p].source == Source::State && !self.scalars@.contains(p as usize)
        &&& forall|i:int,j:int,p:int| 0 <= i < j < self.entries@.len() && 0 <= p < self.positions@.len() ==>
            !(touches(self.entries@[i],p) && touches(self.entries@[j],p))
        &&& forall|i:int,j:int| 0 <= i < self.entries@.len() && 0 <= j < self.entries@.len() && self.entries@[i].map == self.entries@[j].map ==>
            same_schema(self.entries@[i],self.entries@[j],self.positions@)
    }
    spec fn state_bound(&self,state:Map<u16,i128>,tuple:Seq<i128>,upto:int) -> bool {
        forall|i:int| 0 <= i < upto ==> state.contains_key(field_id(self.positions@[self.scalars@[i] as int])) &&
            tuple[self.scalars@[i] as int] == state[field_id(self.positions@[self.scalars@[i] as int])]
    }
    spec fn scalars_bound(&self,state:Map<u16,i128>,command:&Input,context:&Input,tuple:Seq<i128>) -> bool {
        &&& self.state_bound(state,tuple,self.scalars@.len() as int)
        &&& forall|i:int| 0 <= i < self.positions@.len() && self.positions@[i].source != Source::State && !self.input_position(i) ==>
            selected_scalars(command,context,self.positions@[i].source).contains_key(field_id(self.positions@[i])) &&
            tuple[i] == selected_scalars(command,context,self.positions@[i].source)[field_id(self.positions@[i])]
    }
    spec fn inputs_bound(&self,command:&Input,context:&Input,actual:Map<usize,(u32,Key)>,upto:int) -> bool {
        forall|i:int| #![trigger self.inputs@[i]] 0 <= i < upto ==> {
            let index = self.inputs@[i].0;
            let keys = selected_keys(command,context,self.positions@[index as int].source);
            keys.contains_key(field_id(self.positions@[index as int])) && actual.contains_key(index) &&
                actual[index] == (self.inputs@[i].1,keys[field_id(self.positions@[index as int])])
        }
    }
}
}
#[cfg_attr(verus_keep_ghost,verus_spec(result =>
    ensures match result {
        Ok(value) => selected_scalars(command,context,source).contains_key(field) &&
            value == selected_scalars(command,context,source)[field],
        Err(_) => !selected_scalars(command,context,source).contains_key(field),
    },
))]
fn input_scalar(
    command: &Input,
    context: &Input,
    source: Source,
    field: u16,
    name: &str,
) -> Result<i128, ContractError> {
    let input = match source {
        Source::Command => command,
        _ => context,
    };
    match input.scalars.get(&field) {
        Some(value) => Ok(*value),
        None => Err(ContractError::new(
            "collections runtime",
            format!("{name} is missing"),
        )),
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result =>
    ensures match result {
        Ok(key) => selected_keys(command,context,source).contains_key(field) && key == selected_keys(command,context,source)[field],
        Err(_) => !selected_keys(command,context,source).contains_key(field),
    },
))]
fn input_key(
    command: &Input,
    context: &Input,
    source: Source,
    field: u16,
) -> Result<Key, ContractError> {
    let input = match source {
        Source::Command => command,
        _ => context,
    };
    match input.keys.get(&field) {
        Some(key) => Ok(*key),
        None => Err(ContractError::new(
            "collections runtime",
            format!("key field {field} is missing"),
        )),
    }
}
impl Layout {
    #[cfg_attr(verus_keep_ghost,verus_spec(result => ensures result == self.input_position(index as int),))]
    fn is_key_position(&self, index: usize) -> bool {
        let mut cursor = 0usize;
        #[cfg_attr(verus_keep_ghost,verus_spec(
            invariant cursor <= self.inputs@.len(), forall|i:int| 0 <= i < cursor ==> self.inputs@[i].0 != index,
            decreases self.inputs@.len() - cursor,
        ))]
        while cursor < self.inputs.len() {
            if self.inputs[cursor].0 == index {
                #[cfg(verus_keep_ghost)]
                proof! { broadcast use vstd::seq_lib::group_seq_properties;
                assert(self.input_indices()[cursor as int] == index);
                assert(self.input_indices().contains(index)); }
                return true;
            }
            cursor += 1;
        }
        #[cfg(verus_keep_ghost)]
        proof! { assert(!self.input_indices().contains(index)); }
        false
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result =>
        requires self.well_formed(),
        ensures match result { Ok(tuple) => tuple@.len() == self.positions@.len() && self.scalars_bound(state@,command,context,tuple@), Err(_) => true },
    ))]
    fn frame_scalars(
        &self,
        state: &BTreeMap<u16, i128>,
        command: &Input,
        context: &Input,
    ) -> Result<Vec<i128>, ContractError> {
        let mut tuple = vec![0i128; self.positions.len()];
        let mut cursor = 0usize;
        #[cfg_attr(verus_keep_ghost,verus_spec(
            invariant self.well_formed(), cursor <= self.scalars@.len(), tuple@.len() == self.positions@.len(),
                self.state_bound(state@,tuple@,cursor as int),
            decreases self.scalars@.len() - cursor,
        ))]
        while cursor < self.scalars.len() {
            let index = self.scalars[cursor];
            let field = self.positions[index].field.unwrap_or(0);
            let value = match state.get(&field) {
                Some(value) => *value,
                None => {
                    return Err(ContractError::new(
                        "collections runtime",
                        format!("state field {field} is missing"),
                    ));
                }
            };
            tuple[index] = value;
            cursor += 1;
        }
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost state_tuple = tuple@; }
        let mut index = 0usize;
        #[cfg_attr(verus_keep_ghost,verus_spec(
            invariant self.well_formed(), tuple@.len() == self.positions@.len(), index <= self.positions@.len(),
                self.state_bound(state@,tuple@,self.scalars@.len() as int),
                forall|i:int| 0 <= i < index && self.positions@[i].source != Source::State && !self.input_position(i) ==>
                    selected_scalars(command,context,self.positions@[i].source).contains_key(field_id(self.positions@[i])) &&
                    tuple@[i] == selected_scalars(command,context,self.positions@[i].source)[field_id(self.positions@[i])],
            decreases self.positions@.len() - index,
        ))]
        while index < self.positions.len() {
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost previous_tuple = tuple@; }
            let position = &self.positions[index];
            if !matches!(position.source, Source::State) && !self.is_key_position(index) {
                tuple[index] = input_scalar(
                    command,
                    context,
                    position.source,
                    position.field.unwrap_or(0),
                    &position.name,
                )?;
            }
            index += 1;
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|i:int| 0 <= i < self.scalars@.len() implies
                    state@.contains_key(field_id(self.positions@[self.scalars@[i] as int])) &&
                    tuple@[self.scalars@[i] as int] == state@[field_id(self.positions@[self.scalars@[i] as int])] by {
                    assert(self.positions@[self.scalars@[i] as int].source == Source::State);
                    assert(tuple@[self.scalars@[i] as int] == previous_tuple[self.scalars@[i] as int]);
                }
                assert forall|i:int| 0 <= i < index && self.positions@[i].source != Source::State && !self.input_position(i) implies
                    selected_scalars(command,context,self.positions@[i].source).contains_key(field_id(self.positions@[i])) &&
                    tuple@[i] == selected_scalars(command,context,self.positions@[i].source)[field_id(self.positions@[i])] by {
                    if i < index - 1 { assert(tuple@[i] == previous_tuple[i]); }
                }
            }
        }
        Ok(tuple)
    }
    #[cfg_attr(verus_keep_ghost,verus_spec(result =>
        requires self.well_formed(),
        ensures match result { Ok(actual) => self.inputs_bound(command,context,actual@,self.inputs@.len() as int) &&
            forall|index:usize| actual@.contains_key(index) ==> self.input_position(index as int), Err(_) => true },
    ))]
    fn frame_input_keys(
        &self,
        command: &Input,
        context: &Input,
    ) -> Result<BTreeMap<usize, (u32, Key)>, ContractError> {
        let mut actual = BTreeMap::new();
        let mut cursor = 0usize;
        #[cfg_attr(verus_keep_ghost,verus_spec(
            invariant self.well_formed(), cursor <= self.inputs@.len(), self.inputs_bound(command,context,actual@,cursor as int),
                forall|index:usize| actual@.contains_key(index) ==> self.input_position(index as int),
            decreases self.inputs@.len() - cursor,
        ))]
        while cursor < self.inputs.len() {
            let (index, kind) = self.inputs[cursor];
            let position = &self.positions[index];
            let key = input_key(
                command,
                context,
                position.source,
                position.field.unwrap_or(0),
            )?;
            actual.insert(index, (kind, key));
            #[cfg(verus_keep_ghost)]
            proof! { broadcast use vstd::seq_lib::group_seq_properties;
            assert(self.input_indices()[cursor as int] == index);
            assert(self.input_position(index as int)); }
            cursor += 1;
        }
        Ok(actual)
    }
}

#[cfg(verus_keep_ghost)]
verus! {
spec fn key_prefix(actual:Map<usize,(u32,Key)>,kind:u32,count:int) -> Seq<Key>
    decreases count,
{
    if count <= 0 { seq![] }
    else {
        let previous = key_prefix(actual,kind,count-1);
        if actual.contains_key((count-1) as usize) && actual[(count-1) as usize].0 == kind &&
            !previous.contains(actual[(count-1) as usize].1) {
            previous.push(actual[(count-1) as usize].1)
        } else { previous }
    }
}
proof fn key_prefix_length(actual:Map<usize,(u32,Key)>,kind:u32,count:int)
    requires 0 <= count,
    ensures key_prefix(actual,kind,count).len() <= count,
    decreases count,
{
    if count > 0 { key_prefix_length(actual,kind,count-1); }
}
spec fn class_keys(classes:Map<u32,Vec<Key>>,kind:u32) -> Seq<Key> {
    if classes.contains_key(kind) { classes[kind]@ } else { seq![] }
}
spec fn classes_match(actual:Map<usize,(u32,Key)>,classes:Map<u32,Vec<Key>>,count:int) -> bool {
    forall|kind:u32| #![trigger class_keys(classes,kind)] class_keys(classes,kind) == key_prefix(actual,kind,count) &&
        (classes.contains_key(kind) == (key_prefix(actual,kind,count).len() > 0))
}
spec fn key_binding(actual:Map<usize,(u32,Key)>,tuple:Seq<i128>,classes:Map<u32,Vec<Key>>,index:usize) -> bool {
    let (kind,key) = actual[index];
    classes.contains_key(kind) && 0 <= tuple[index as int] < classes[kind]@.len() &&
        classes[kind]@[tuple[index as int] as int] == key
}
spec fn class_framed(actual:Map<usize,(u32,Key)>,before:Seq<i128>,after:Seq<i128>,classes:Map<u32,Vec<Key>>) -> bool {
    &&& before.len() == after.len()
    &&& classes_match(actual,classes,after.len() as int)
    &&& forall|index:usize| #![trigger key_binding(actual,after,classes,index)] index < after.len() && actual.contains_key(index) ==>
        key_binding(actual,after,classes,index)
    &&& forall|i:int| 0 <= i < after.len() && !actual.contains_key(i as usize) ==> after[i] == before[i]
}
}
#[cfg_attr(verus_keep_ghost,verus_spec(result =>
    requires forall|index:usize| actual@.contains_key(index) ==> index < old(tuple)@.len(),
    ensures class_framed(actual@,old(tuple)@,final(tuple)@,result@),
))]
fn intern_keys(
    actual: &BTreeMap<usize, (u32, Key)>,
    tuple: &mut [i128],
) -> BTreeMap<u32, Vec<Key>> {
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost initial = tuple@; }
    let mut classes: BTreeMap<u32, Vec<Key>> = BTreeMap::new();
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant index <= tuple@.len(), tuple@.len() == initial.len(), initial == old(tuple)@,
            forall|p:usize| actual@.contains_key(p) ==> p < tuple@.len(),
            classes_match(actual@,classes@,index as int),
            forall|p:int| #![trigger key_binding(actual@,tuple@,classes@,p as usize)] 0 <= p < index && actual@.contains_key(p as usize) ==> key_binding(actual@,tuple@,classes@,p as usize),
            forall|p:int| 0 <= p < tuple@.len() && (p >= index || !actual@.contains_key(p as usize)) ==> tuple@[p] == initial[p],
        decreases tuple@.len() - index,
    ))]
    while index < tuple.len() {
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost previous_classes = classes@; let ghost previous_tuple = tuple@; }
        if let Some(&(kind, key)) = actual.get(&index) {
            #[cfg(verus_keep_ghost)]
            proof! { key_prefix_length(actual@,kind,index as int);
            assert(class_keys(classes@,kind) == key_prefix(actual@,kind,index as int)); }
            let mut seen = classes.remove(&kind).unwrap_or_default();
            #[cfg(verus_keep_ghost)]
            proof_decl! { let ghost previous_seen = seen@; }
            #[cfg(verus_keep_ghost)]
            proof! { assert(previous_seen == class_keys(previous_classes,kind)); }
            let class = match key_index(&seen, &key) {
                Some(class) => class,
                None => {
                    seen.push(key);
                    seen.len() - 1
                }
            };
            tuple[index] = class as i128;
            classes.insert(kind, seen);
            #[cfg(verus_keep_ghost)]
            proof! {
                broadcast use vstd::seq_lib::group_seq_properties;
                assert(previous_seen == key_prefix(actual@,kind,index as int));
                assert(class_keys(classes@,kind) == key_prefix(actual@,kind,index as int + 1));
                assert forall|other:u32| class_keys(classes@,other) == key_prefix(actual@,other,index as int + 1) &&
                    (classes@.contains_key(other) == (key_prefix(actual@,other,index as int + 1).len() > 0)) by {
                    if other != kind { assert(class_keys(classes@,other) == class_keys(previous_classes,other)); }
                }
                assert forall|p:int| #![trigger key_binding(actual@,tuple@,classes@,p as usize)] 0 <= p <= index && actual@.contains_key(p as usize) implies key_binding(actual@,tuple@,classes@,p as usize) by {
                    if p < index {
                        assert(key_binding(actual@,previous_tuple,previous_classes,p as usize));
                        assert(tuple@[p] == previous_tuple[p]);
                        if actual@[p as usize].0 == kind {
                            assert(0 <= tuple@[p] < previous_seen.len());
                            assert(class_keys(classes@,kind)[tuple@[p] as int] == previous_seen[tuple@[p] as int]);
                        }
                    }
                }
            }
        } else {
            #[cfg(verus_keep_ghost)]
            proof! { assert forall|kind:u32| class_keys(classes@,kind) == key_prefix(actual@,kind,index as int + 1) &&
            (classes@.contains_key(kind) == (key_prefix(actual@,kind,index as int + 1).len() > 0)) by {}; }
        }
        #[cfg(verus_keep_ghost)]
        proof! { assert(classes_match(actual@,classes@,index as int + 1)); }
        index += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|p:usize| #![trigger key_binding(actual@,tuple@,classes@,p)] p < tuple@.len() && actual@.contains_key(p)
            implies key_binding(actual@,tuple@,classes@,p) by {
            assert(0 <= (p as int) && (p as int) < index);
            assert(key_binding(actual@,tuple@,classes@,(p as int) as usize));
        }
    }
    classes
}

#[cfg(verus_keep_ghost)]
verus! {
spec fn named_member(named:Map<u32,BTreeSet<Key>>,map:u32,key:Key) -> bool {
    named.contains_key(map) && named[map]@.contains(key)
}
spec fn name_added(before:Map<u32,BTreeSet<Key>>,after:Map<u32,BTreeSet<Key>>,map:u32,key:Key) -> bool {
    &&& forall|m:u32| after.contains_key(m) == (m == map || before.contains_key(m))
    &&& forall|m:u32,k:Key| named_member(after,m,k) == (m == map && k == key || named_member(before,m,k))
}
impl Layout {
    spec fn names_bound(&self,keys:Seq<Key>,named:Map<u32,BTreeSet<Key>>,count:int) -> bool {
        &&& keys.len() == count
        &&& forall|map:u32| named.contains_key(map) == (exists|i:int| 0 <= i < count && self.entries@[i].map == map)
        &&& forall|map:u32,key:Key| named_member(named,map,key) ==
            (exists|i:int| 0 <= i < count && self.entries@[i].map == map && keys[i] == key)
    }
    spec fn records_bound(&self,command:&Input,context:&Input,contents:Map<(u32,Key),Seq<u8>>,
        keys:Seq<Key>,tuple:Seq<i128>,actual:Map<usize,(u32,Key)>,count:int) -> bool {
        forall|i:int| #![trigger self.entries@[i]] 0 <= i < count ==> {
            let entry = self.entries@[i];
            let input = selected_keys(command,context,entry.key_field.0);
            input.contains_key(entry.key_field.1) && keys[i] == input[entry.key_field.1] &&
                record_bound(entry,self.positions@,stored(contents,entry.map,keys[i]),tuple,actual)
        }
    }
    spec fn actual_allowed(&self,actual:Map<usize,(u32,Key)>,count:int) -> bool {
        forall|index:usize| #![trigger actual.contains_key(index)] actual.contains_key(index) ==> index < self.positions@.len() &&
            (self.input_position(index as int) || exists|i:int| 0 <= i < count && value_indices(self.entries@[i]).contains(index))
    }
    spec fn raw_frame(&self,state:Map<u16,i128>,command:&Input,context:&Input,contents:Map<(u32,Key),Seq<u8>>,
        tuple:Seq<i128>,actual:Map<usize,(u32,Key)>,keys:Seq<Key>,named:Map<u32,BTreeSet<Key>>) -> bool {
        &&& tuple.len() == self.positions@.len()
        &&& self.scalars_bound(state,command,context,tuple)
        &&& self.inputs_bound(command,context,actual,self.inputs@.len() as int)
        &&& self.records_bound(command,context,contents,keys,tuple,actual,self.entries@.len() as int)
        &&& self.names_bound(keys,named,self.entries@.len() as int)
        &&& self.actual_allowed(actual,self.entries@.len() as int)
    }
    pub closed spec fn framed_from(&self,state:&BTreeMap<u16,i128>,command:&Input,context:&Input,
        contents:Map<(u32,Key),Seq<u8>>,framed:&Framed) -> bool {
        exists|before:Seq<i128>,actual:Map<usize,(u32,Key)>|
            self.raw_frame(state@,command,context,contents,before,actual,framed.entry_keys@,framed.named@) &&
                class_framed(actual,before,framed.tuple@,framed.classes@)
    }
    proof fn new_entry_clear(&self,actual:Map<usize,(u32,Key)>,cursor:int)
        requires self.well_formed(), 0 <= cursor < self.entries@.len(), self.actual_allowed(actual,cursor),
        ensures forall|field:int| 0 <= field < self.entries@[cursor].values@.len() ==>
            !actual.contains_key(self.entries@[cursor].values@[field].0),
    {
        broadcast use vstd::seq_lib::group_seq_properties;
        let entry = self.entries@[cursor];
        assert forall|field:int| 0 <= field < entry.values@.len() implies !actual.contains_key(entry.values@[field].0) by {
            let index = entry.values@[field].0;
            assert(value_indices(entry)[field] == index);
            assert(value_indices(entry).contains(index));
            assert(touches(entry,index as int));
            if actual.contains_key(index) {
                if self.input_position(index as int) {
                    let i = choose|i:int| 0 <= i < self.input_indices().len() && self.input_indices()[i] == index;
                    assert(self.inputs@[i].0 == index);
                    assert(self.positions@[index as int].source != Source::State);
                } else {
                    let i = choose|i:int| 0 <= i < cursor && value_indices(self.entries@[i]).contains(index);
                    assert(touches(self.entries@[i],index as int));
                }
            }
        }
    }
    proof fn names_step(&self,previous_keys:Seq<Key>,before:Map<u32,BTreeSet<Key>>,after:Map<u32,BTreeSet<Key>>,cursor:int,key:Key)
        requires 0 <= cursor < self.entries@.len(), self.names_bound(previous_keys,before,cursor),
            name_added(before,after,self.entries@[cursor].map,key),
        ensures self.names_bound(previous_keys.push(key),after,cursor+1),
    {
        assert forall|map:u32| after.contains_key(map) == (exists|i:int| 0 <= i < cursor+1 && self.entries@[i].map == map) by {
            if after.contains_key(map) {
                if map == self.entries@[cursor].map { assert(0 <= cursor < cursor+1); }
                else { let i = choose|i:int| 0 <= i < cursor && self.entries@[i].map == map; }
            }
        }
        assert forall|map:u32,k:Key| named_member(after,map,k) ==
            (exists|i:int| 0 <= i < cursor+1 && self.entries@[i].map == map && previous_keys.push(key)[i] == k) by {
            if named_member(after,map,k) {
                if map == self.entries@[cursor].map && k == key { assert(previous_keys.push(key)[cursor] == key); }
                else { let i = choose|i:int| 0 <= i < cursor && self.entries@[i].map == map && previous_keys[i] == k;
                    assert(previous_keys.push(key)[i] == k); }
            }
        }
    }
}
proof fn record_bound_preserved(entry:EntryLayout,changed:EntryLayout,positions:Seq<Position>,bytes:Option<Seq<u8>>,
    before:Seq<i128>,after:Seq<i128>,before_actual:Map<usize,(u32,Key)>,after_actual:Map<usize,(u32,Key)>)
    requires entry_well_formed(entry,positions.len() as int), before.len() == positions.len(),
        record_bound(entry,positions,bytes,before,before_actual),
        record_footprint(changed,before,after,before_actual,after_actual),
        forall|index:int| 0 <= index < before.len() ==> !(touches(entry,index) && touches(changed,index)),
    ensures record_bound(entry,positions,bytes,after,after_actual),
{
    broadcast use vstd::seq_lib::group_seq_properties;
    assert(!touches(changed,entry.presence as int));
    assert forall|i:int| 0 <= i < entry.values@.len() implies
        after[entry.values@[i].0 as int] == before[entry.values@[i].0 as int] &&
        after_actual.contains_key(entry.values@[i].0) == before_actual.contains_key(entry.values@[i].0) &&
        (after_actual.contains_key(entry.values@[i].0) ==> after_actual[entry.values@[i].0] == before_actual[entry.values@[i].0]) by {
        let index = entry.values@[i].0;
        assert(value_indices(entry)[i] == index);
        assert(value_indices(entry).contains(index));
        assert(!touches(changed,index as int));
    }
}
}
#[cfg_attr(verus_keep_ghost,verus_spec(
    requires vstd::laws_cmp::obeys_cmp::<Key>(),
    ensures name_added(old(named)@,final(named)@,map,key),
))]
fn add_named(named: &mut BTreeMap<u32, BTreeSet<Key>>, map: u32, key: Key) {
    let mut keys = named.remove(&map).unwrap_or_default();
    keys.insert(key);
    named.insert(map, keys);
}

#[cfg(verus_keep_ghost)]
verus! {
spec fn same_schema(left:EntryLayout,right:EntryLayout,positions:Seq<Position>) -> bool {
    &&& left.values@.len() == right.values@.len()
    &&& forall|i:int| 0 <= i < left.values@.len() ==> left.values@[i].1 == right.values@[i].1 &&
        crate::internals::review::domain::least_or_zero(positions[left.values@[i].0 as int].domain) == crate::internals::review::domain::least_or_zero(positions[right.values@[i].0 as int].domain)
}
proof fn key_prefix_unique(actual:Map<usize,(u32,Key)>,kind:u32,count:int)
    requires 0 <= count,
    ensures forall|i:int,j:int| 0 <= i < j < key_prefix(actual,kind,count).len() ==>
        key_prefix(actual,kind,count)[i] != key_prefix(actual,kind,count)[j],
    decreases count,
{
    if count > 0 {
        key_prefix_unique(actual,kind,count-1);
        let previous = key_prefix(actual,kind,count-1);
        let current = key_prefix(actual,kind,count);
        broadcast use vstd::seq_lib::group_seq_properties;
        assert forall|i:int,j:int| 0 <= i < j < current.len() implies current[i] != current[j] by {
            if j < previous.len() { assert(previous[i] != previous[j]); }
            else { assert(previous.contains(previous[i])); }
        }
    }
}
proof fn same_key_class(actual:Map<usize,(u32,Key)>,before:Seq<i128>,after:Seq<i128>,classes:Map<u32,Vec<Key>>,left:usize,right:usize)
    requires class_framed(actual,before,after,classes), left < after.len(), right < after.len(),
        actual.contains_key(left), actual.contains_key(right), actual[left] == actual[right],
    ensures after[left as int] == after[right as int],
{
    let kind = actual[left].0;
    key_prefix_unique(actual,kind,after.len() as int);
    assert(key_binding(actual,after,classes,left));
    assert(key_binding(actual,after,classes,right));
    assert(class_keys(classes,kind) == key_prefix(actual,kind,after.len() as int));
    let i = after[left as int] as int;
    let j = after[right as int] as int;
    if i < j { assert(key_prefix(actual,kind,after.len() as int)[i] != key_prefix(actual,kind,after.len() as int)[j]); }
    if j < i { assert(key_prefix(actual,kind,after.len() as int)[j] != key_prefix(actual,kind,after.len() as int)[i]); }
}
proof fn record_alias(left:EntryLayout,right:EntryLayout,positions:Seq<Position>,bytes:Option<Seq<u8>>,
    before:Seq<i128>,after:Seq<i128>,actual:Map<usize,(u32,Key)>,classes:Map<u32,Vec<Key>>)
    requires entry_well_formed(left,positions.len() as int),entry_well_formed(right,positions.len() as int),
        same_schema(left,right,positions),before.len() == positions.len(),
        record_bound(left,positions,bytes,before,actual),record_bound(right,positions,bytes,before,actual),
        !actual.contains_key(left.presence),!actual.contains_key(right.presence),
        class_framed(actual,before,after,classes),
    ensures entry_image(left,positions,after,None) == entry_image(right,positions,after,None),
{
    assert(after[left.presence as int] == before[left.presence as int]);
    assert(after[right.presence as int] == before[right.presence as int]);
    assert(after[left.presence as int] == after[right.presence as int]);
    assert forall|i:int| 0 <= i < left.values@.len() implies after[left.values@[i].0 as int] == after[right.values@[i].0 as int] by {
        let l = left.values@[i].0;
        let r = right.values@[i].0;
        match bytes {
            None => { assert(!actual.contains_key(l)); assert(!actual.contains_key(r)); }
            Some(bytes) => {
                let fields = parsed(bytes).unwrap();
                assert(field_bound(left.values@[i],fields[i],before,actual));
                assert(field_bound(right.values@[i],fields[i],before,actual));
                match left.values@[i].1 {
                    None => {},
                    Some(kind) => {
                        assert(actual[l].1@ == actual[r].1@);
                        assert(actual[l].1 =~= actual[r].1);
                        assert(actual[l] == actual[r]);
                        same_key_class(actual,before,after,classes,l,r);
                    },
                }
            },
        }
    }
    assert(entry_image(left,positions,after,None) =~= entry_image(right,positions,after,None));
}
impl Layout {
    proof fn presence_not_key(&self,actual:Map<usize,(u32,Key)>,cursor:int)
        requires self.well_formed(), self.actual_allowed(actual,self.entries@.len() as int), 0 <= cursor < self.entries@.len(),
        ensures !actual.contains_key(self.entries@[cursor].presence),
    {
        broadcast use vstd::seq_lib::group_seq_properties;
        let index = self.entries@[cursor].presence;
        assert(touches(self.entries@[cursor],index as int));
        if actual.contains_key(index) {
            if self.input_position(index as int) {
                let i = choose|i:int| 0 <= i < self.input_indices().len() && self.input_indices()[i] == index;
                assert(self.inputs@[i].0 == index);
                assert(self.positions@[index as int].source != Source::State);
            } else {
                let i = choose|i:int| 0 <= i < self.entries@.len() && value_indices(self.entries@[i]).contains(index);
                let field = choose|field:int| 0 <= field < value_indices(self.entries@[i]).len() && value_indices(self.entries@[i])[field] == index;
                assert(self.entries@[i].values@[field].0 == index);
                assert(touches(self.entries@[i],index as int));
                if i == cursor { assert(self.entries@[i].values@[field].0 != self.entries@[i].presence); }
            }
        }
    }
    pub proof fn frame_is_valid(&self,state:&BTreeMap<u16,i128>,command:&Input,context:&Input,
        contents:Map<(u32,Key),Seq<u8>>,framed:&Framed)
        requires self.well_formed(),self.framed_from(state,command,context,contents,framed),
        ensures self.framed_valid(framed),
    {
        let (before,actual) = choose|before:Seq<i128>,actual:Map<usize,(u32,Key)>|
            self.raw_frame(state@,command,context,contents,before,actual,framed.entry_keys@,framed.named@) &&
                class_framed(actual,before,framed.tuple@,framed.classes@);
        assert forall|i:int,j:int| 0 <= i < self.entries@.len() && 0 <= j < self.entries@.len() && self.group(framed,i,j) implies
            entry_image(self.entries@[i],self.positions@,framed.tuple@,None) == entry_image(self.entries@[j],self.positions@,framed.tuple@,None) by {
            self.presence_not_key(actual,i);
            self.presence_not_key(actual,j);
            record_alias(self.entries@[i],self.entries@[j],self.positions@,stored(contents,self.entries@[i].map,framed.entry_keys@[i]),
                before,framed.tuple@,actual,framed.classes@);
        }
    }
}
}

/// Owned evidence for the declared laws on a canonical stored successor.
/// This is an additional storage guard, never an Authority publication.
#[cfg(not(verus_keep_ghost))]
#[derive(Debug)]
pub struct SuccessorCheck {
    result: Result<(), laws::Failure>,
    usage: V2Usage,
    diagnostics: Vec<laws::Diagnostic>,
    reads: Vec<laws::ReadAttempt>,
}
#[cfg(not(verus_keep_ghost))]
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

#[cfg(not(verus_keep_ghost))]
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
                    &entry
                        .values
                        .iter()
                        .map(|(index, _)| *index)
                        .collect::<Vec<_>>(),
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
}

/// Exactly 32 bytes encoded as 64 lowercase hexadecimal digits; no aliases or prefixes.
#[cfg(not(verus_keep_ghost))]
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
#[cfg(not(verus_keep_ghost))]
pub fn source_digest() -> [u8; 32] {
    crate::checked_composition::Identity::current().digest()
}

#[cfg(test)]
#[path = "runtime_proof_tests.rs"]
mod proof_tests;
