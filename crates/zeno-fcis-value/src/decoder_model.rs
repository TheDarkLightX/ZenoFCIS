//! Mathematical ZCVE values, canonical encoding and structural admission.
//! This module is ghost-only. Production correspondence is a separate obligation.
use vstd::prelude::*;

verus! {

pub enum ModelValue {
    Unit,
    Bool(bool),
    Unsigned(u128),
    Signed(i128),
    Bytes(Seq<u8>),
    Text(Seq<u8>),
    Enum(u32, u16),
    Tuple(Seq<ModelValue>),
    Record(Seq<(u16, ModelValue)>),
    Sum(u32, u16, Seq<ModelValue>),
    Vector(Seq<ModelValue>),
    Map(Seq<(ModelValue, ModelValue)>),
}

pub struct ModelLimits {
    pub max_depth: u32,
    pub max_nodes: u64,
    pub max_payload_bytes: u64,
    pub max_collection_len: u32,
    pub max_input_bytes: u64,
}

pub open spec fn depth_limit(limits: ModelLimits) -> nat {
    if limits.max_depth < 1024 { limits.max_depth as nat } else { 1024 }
}

/// The low `width` bytes of a word, most significant byte first.
pub open spec fn big_endian(word: u128, width: nat) -> Seq<u8>
    decreases width,
{
    if width == 0 { Seq::empty() }
    else { big_endian(word >> 8, (width - 1) as nat).push((word % 256) as u8) }
}

pub open spec fn signed_bits(word: i128) -> u128 {
    if word < 0 { (word as int + 340282366920938463463374607431768211456int) as u128 }
    else { word as u128 }
}

pub open spec fn signed_word(bits: u128) -> i128 {
    if bits > i128::MAX { (bits as int - 340282366920938463463374607431768211456int) as i128 }
    else { bits as i128 }
}

pub open spec fn unsigned_bytes(bytes: Seq<u8>) -> u128
    decreases bytes,
{
    if bytes.len() == 0 { 0 }
    else { (unsigned_bytes(bytes.drop_last()) << 8) | bytes.last() as u128 }
}

pub open spec fn length_prefix(length: nat) -> Seq<u8> {
    big_endian(length as u128, 4)
}

pub open spec fn blob(bytes: Seq<u8>) -> Seq<u8> {
    length_prefix(bytes.len()) + bytes
}

pub open spec fn spec_encode(value: ModelValue) -> Seq<u8>
    decreases value,
{
    match value {
        ModelValue::Unit => seq![0u8],
        ModelValue::Bool(false) => seq![1u8],
        ModelValue::Bool(true) => seq![2u8],
        ModelValue::Unsigned(word) => seq![3u8] + big_endian(word, 16),
        ModelValue::Signed(word) => seq![4u8] + big_endian(signed_bits(word), 16),
        ModelValue::Bytes(bytes) => seq![5u8] + blob(bytes),
        ModelValue::Text(bytes) => seq![6u8] + blob(bytes),
        ModelValue::Enum(id, variant) => seq![7u8] + big_endian(id as u128, 4)
            + big_endian(variant as u128, 2),
        ModelValue::Tuple(items) => seq![8u8] + length_prefix(items.len()) + encode_items(items),
        ModelValue::Record(fields) => seq![9u8] + length_prefix(fields.len()) + encode_fields(fields),
        ModelValue::Sum(id, variant, payload) => seq![10u8] + big_endian(id as u128, 4)
            + big_endian(variant as u128, 2) + if payload.len() == 0 {
                seq![0u8]
            } else { seq![1u8] + spec_encode(payload[0]) },
        ModelValue::Vector(items) => seq![11u8] + length_prefix(items.len()) + encode_items(items),
        ModelValue::Map(entries) => seq![12u8] + length_prefix(entries.len()) + encode_entries(entries),
    }
}

pub open spec fn encode_items(items: Seq<ModelValue>) -> Seq<u8>
    decreases items,
{
    if items.len() == 0 { Seq::empty() }
    else { spec_encode(items[0]) + encode_items(items.drop_first()) }
}

pub open spec fn encode_fields(fields: Seq<(u16, ModelValue)>) -> Seq<u8>
    decreases fields,
{
    if fields.len() == 0 { Seq::empty() }
    else { big_endian(fields[0].0 as u128, 2) + spec_encode(fields[0].1)
        + encode_fields(fields.drop_first()) }
}

pub open spec fn encode_entries(entries: Seq<(ModelValue, ModelValue)>) -> Seq<u8>
    decreases entries,
{
    if entries.len() == 0 { Seq::empty() }
    else { blob(spec_encode(entries[0].0)) + blob(spec_encode(entries[0].1))
        + encode_entries(entries.drop_first()) }
}

pub open spec fn ascii(bytes: Seq<u8>) -> bool {
    forall|i: int| 0 <= i < bytes.len() ==> #[trigger] bytes[i] < 128
}

pub proof fn ascii_is_utf8(bytes: Seq<u8>)
    requires ascii(bytes),
    ensures vstd::utf8::valid_utf8(bytes),
{
    assert(bytes.subrange(0, 0) == Seq::<u8>::empty());
    assert forall|i: int| 0 <= i < bytes.len() implies
        #[trigger] vstd::utf8::is_leading_byte_width_1(bytes[i]) by {
        assert(bytes[i] < 128);
    }
    vstd::utf8::partial_valid_utf8_extend_ascii_block(bytes, 0, bytes.len() as int);
    assert(bytes.subrange(0, bytes.len() as int) == bytes);
}

pub open spec fn lex_less(left: Seq<u8>, right: Seq<u8>) -> bool
    decreases left.len() + right.len(),
{
    if left.len() == 0 { right.len() > 0 }
    else if right.len() == 0 { false }
    else { left[0] < right[0] || (left[0] == right[0]
        && lex_less(left.drop_first(), right.drop_first())) }
}

pub open spec fn fields_ordered(fields: Seq<(u16, ModelValue)>) -> bool {
    forall|i: int| 0 <= i && i + 1 < fields.len() ==>
        (#[trigger] fields[i]).0 < fields[i + 1].0
}

pub open spec fn entries_ordered(entries: Seq<(ModelValue, ModelValue)>) -> bool {
    forall|i: int| 0 <= i && i + 1 < entries.len() ==>
        lex_less(spec_encode((#[trigger] entries[i]).0), spec_encode(entries[i + 1].0))
}

pub proof fn fields_ordered_push(fields: Seq<(u16, ModelValue)>, field: (u16, ModelValue))
    requires fields_ordered(fields), fields.len() == 0 || fields.last().0 < field.0,
    ensures fields_ordered(fields.push(field)),
{
    assert forall|i: int| 0 <= i && i + 1 < fields.push(field).len() implies
        (#[trigger] fields.push(field)[i]).0 < fields.push(field)[i + 1].0 by {
        if i + 1 < fields.len() { assert(fields[i].0 < fields[i + 1].0); }
        else { assert(fields[i] == fields.last()); }
    }
}

pub proof fn entries_ordered_push(entries: Seq<(ModelValue, ModelValue)>, entry: (ModelValue, ModelValue))
    requires entries_ordered(entries), entries.len() == 0
        || lex_less(spec_encode(entries.last().0), spec_encode(entry.0)),
    ensures entries_ordered(entries.push(entry)),
{
    assert forall|i: int| 0 <= i && i + 1 < entries.push(entry).len() implies
        lex_less(spec_encode((#[trigger] entries.push(entry)[i]).0), spec_encode(entries.push(entry)[i + 1].0)) by {
        if i + 1 < entries.len() {
            assert(lex_less(spec_encode(entries[i].0), spec_encode(entries[i + 1].0)));
        } else { assert(entries[i] == entries.last()); }
    }
}

/// Constructor invariants, independently of a caller's resource budget.
pub open spec fn structurally_valid(value: ModelValue) -> bool
    decreases value,
{
    match value {
        ModelValue::Bytes(bytes) => bytes.len() <= u32::MAX,
        ModelValue::Text(bytes) => bytes.len() <= u32::MAX && ascii(bytes),
        ModelValue::Tuple(items) => items.len() <= u32::MAX
            && forall|i: int| 0 <= i < items.len() ==> structurally_valid(#[trigger] items[i]),
        ModelValue::Vector(items) => items.len() <= u32::MAX
            && forall|i: int| 0 <= i < items.len() ==> structurally_valid(#[trigger] items[i]),
        ModelValue::Record(fields) => fields.len() <= u32::MAX && fields_ordered(fields)
            && forall|i: int| 0 <= i < fields.len() ==> structurally_valid((#[trigger] fields[i]).1),
        ModelValue::Sum(_, _, payload) => payload.len() <= 1
            && forall|i: int| 0 <= i < payload.len() ==> structurally_valid(#[trigger] payload[i]),
        ModelValue::Map(entries) => entries.len() <= u32::MAX && entries_ordered(entries)
            && forall|i: int| 0 <= i < entries.len() ==> {
                &&& structurally_valid((#[trigger] entries[i]).0)
                &&& structurally_valid(entries[i].1)
                &&& spec_encode(entries[i].0).len() <= u32::MAX
                &&& spec_encode(entries[i].1).len() <= u32::MAX
            },
        _ => true,
    }
}

pub open spec fn children(value: ModelValue) -> Seq<ModelValue>
    decreases value,
{
    match value {
        ModelValue::Tuple(items) | ModelValue::Vector(items) => items,
        ModelValue::Record(fields) => fields.map_values(|pair: (u16, ModelValue)| pair.1),
        ModelValue::Sum(_, _, payload) => payload,
        ModelValue::Map(entries) => map_children(entries),
        _ => Seq::empty(),
    }
}

pub open spec fn map_children(entries: Seq<(ModelValue, ModelValue)>) -> Seq<ModelValue>
    decreases entries,
{
    if entries.len() == 0 { Seq::empty() }
    else { seq![entries[0].0, entries[0].1] + map_children(entries.drop_first()) }
}

pub open spec fn nodes(value: ModelValue) -> nat
    decreases value,
{
    match value {
        ModelValue::Tuple(items) | ModelValue::Vector(items) => 1 + item_nodes(items),
        ModelValue::Record(fields) => 1 + field_nodes(fields),
        ModelValue::Sum(_, _, payload) => 1 + item_nodes(payload),
        ModelValue::Map(entries) => 1 + entry_nodes(entries),
        _ => 1,
    }
}

pub open spec fn item_nodes(items: Seq<ModelValue>) -> nat
    decreases items,
{
    if items.len() == 0 { 0 } else { nodes(items[0]) + item_nodes(items.drop_first()) }
}

pub open spec fn field_nodes(fields: Seq<(u16, ModelValue)>) -> nat
    decreases fields,
{
    if fields.len() == 0 { 0 } else { nodes(fields[0].1) + field_nodes(fields.drop_first()) }
}

pub open spec fn entry_nodes(entries: Seq<(ModelValue, ModelValue)>) -> nat
    decreases entries,
{
    if entries.len() == 0 { 0 } else {
        nodes(entries[0].0) + nodes(entries[0].1) + entry_nodes(entries.drop_first())
    }
}

pub open spec fn payload_bytes(value: ModelValue) -> nat
    decreases value,
{
    match value {
        ModelValue::Bytes(bytes) | ModelValue::Text(bytes) => bytes.len(),
        ModelValue::Tuple(items) | ModelValue::Vector(items) => item_payload(items),
        ModelValue::Record(fields) => field_payload(fields),
        ModelValue::Sum(_, _, payload) => item_payload(payload),
        ModelValue::Map(entries) => entry_payload(entries),
        _ => 0,
    }
}

pub open spec fn item_payload(items: Seq<ModelValue>) -> nat
    decreases items,
{
    if items.len() == 0 { 0 } else { payload_bytes(items[0]) + item_payload(items.drop_first()) }
}

pub open spec fn field_payload(fields: Seq<(u16, ModelValue)>) -> nat
    decreases fields,
{
    if fields.len() == 0 { 0 } else { payload_bytes(fields[0].1) + field_payload(fields.drop_first()) }
}

pub open spec fn entry_payload(entries: Seq<(ModelValue, ModelValue)>) -> nat
    decreases entries,
{
    if entries.len() == 0 { 0 } else {
        spec_encode(entries[0].0).len() + spec_encode(entries[0].1).len()
            + payload_bytes(entries[0].0) + payload_bytes(entries[0].1)
            + entry_payload(entries.drop_first())
    }
}

/// Depth and per-collection bounds, with the root at depth zero.
pub open spec fn within_shape(value: ModelValue, limits: ModelLimits, depth: nat) -> bool
    decreases value,
{
    depth <= depth_limit(limits) && match value {
        ModelValue::Tuple(items) => items.len() <= limits.max_collection_len
            && forall|i: int| 0 <= i < items.len() ==> within_shape(#[trigger] items[i], limits, depth + 1),
        ModelValue::Vector(items) => items.len() <= limits.max_collection_len
            && forall|i: int| 0 <= i < items.len() ==> within_shape(#[trigger] items[i], limits, depth + 1),
        ModelValue::Record(fields) => fields.len() <= limits.max_collection_len
            && forall|i: int| 0 <= i < fields.len() ==> within_shape((#[trigger] fields[i]).1, limits, depth + 1),
        ModelValue::Sum(_, _, payload) => forall|i: int| 0 <= i < payload.len() ==>
            within_shape(#[trigger] payload[i], limits, depth + 1),
        ModelValue::Map(entries) => entries.len() <= limits.max_collection_len
            && forall|i: int| 0 <= i < entries.len() ==> {
                &&& within_shape((#[trigger] entries[i]).0, limits, depth + 1)
                &&& within_shape(entries[i].1, limits, depth + 1)
            },
        _ => true,
    }
}

/// Value admission does not inspect the separate decoder input-byte ceiling.
pub open spec fn value_limits(limits: ModelLimits) -> ModelLimits {
    ModelLimits { max_input_bytes: 0, ..limits }
}

pub open spec fn admitted(value: ModelValue, limits: ModelLimits, depth: nat,
    nodes_before: nat, payload_before: nat) -> bool {
    within_shape(value, value_limits(limits), depth)
        && nodes_before + nodes(value) <= limits.max_nodes
        && payload_before + payload_bytes(value) <= limits.max_payload_bytes
}

pub open spec fn admitted_entry(key: ModelValue, value: ModelValue, limits: ModelLimits,
    depth: nat, nodes_before: nat, payload_before: nat) -> bool {
    within_shape(key, value_limits(limits), depth) && within_shape(value, value_limits(limits), depth)
        && nodes_before + nodes(key) + nodes(value) <= limits.max_nodes
        && payload_before + spec_encode(key).len() + spec_encode(value).len()
            + payload_bytes(key) + payload_bytes(value) <= limits.max_payload_bytes
}

pub open spec fn items_admitted(items: Seq<ModelValue>, limits: ModelLimits, depth: nat,
    nodes_before: nat, payload_before: nat) -> bool {
    (forall|i: int| 0 <= i < items.len() ==> within_shape(#[trigger] items[i], value_limits(limits), depth))
        && nodes_before + item_nodes(items) <= limits.max_nodes
        && payload_before + item_payload(items) <= limits.max_payload_bytes
}

pub open spec fn fields_admitted(fields: Seq<(u16, ModelValue)>, limits: ModelLimits, depth: nat,
    nodes_before: nat, payload_before: nat) -> bool {
    (forall|i: int| 0 <= i < fields.len() ==> within_shape((#[trigger] fields[i]).1, value_limits(limits), depth))
        && nodes_before + field_nodes(fields) <= limits.max_nodes
        && payload_before + field_payload(fields) <= limits.max_payload_bytes
}

pub open spec fn entries_admitted(entries: Seq<(ModelValue, ModelValue)>, limits: ModelLimits, depth: nat,
    nodes_before: nat, payload_before: nat) -> bool {
    (forall|i: int| 0 <= i < entries.len() ==>
        within_shape((#[trigger] entries[i]).0, value_limits(limits), depth)
        && within_shape(entries[i].1, value_limits(limits), depth))
        && nodes_before + entry_nodes(entries) <= limits.max_nodes
        && payload_before + entry_payload(entries) <= limits.max_payload_bytes
}

pub open spec fn wf(value: ModelValue, limits: ModelLimits) -> bool {
    structurally_valid(value) && admitted(value, limits, 0, 0, 0)
}

pub proof fn nodes_positive(value: ModelValue)
    ensures nodes(value) >= 1,
{
    match value {
        ModelValue::Tuple(_) | ModelValue::Vector(_) | ModelValue::Record(_)
        | ModelValue::Sum(_, _, _) | ModelValue::Map(_) => {},
        _ => {},
    }
}

pub proof fn sum_costs(id: u32, variant: u16, payload: Seq<ModelValue>)
    requires payload.len() <= 1,
    ensures nodes(ModelValue::Sum(id, variant, payload))
        == 1 + if payload.len() == 0 { 0nat } else { nodes(payload[0]) },
        payload_bytes(ModelValue::Sum(id, variant, payload))
        == if payload.len() == 0 { 0nat } else { payload_bytes(payload[0]) },
{
    reveal_with_fuel(nodes, 3);
    reveal_with_fuel(payload_bytes, 3);
    if payload.len() == 0 { assert(payload == Seq::<ModelValue>::empty()); }
    else { assert(payload.drop_first() == Seq::<ModelValue>::empty()); }
}

pub proof fn item_costs_step(items: Seq<ModelValue>, index: int)
    requires 0 <= index < items.len(),
    ensures item_nodes(items.subrange(index, items.len() as int)) == nodes(items[index])
        + item_nodes(items.subrange(index + 1, items.len() as int)),
        item_payload(items.subrange(index, items.len() as int)) == payload_bytes(items[index])
        + item_payload(items.subrange(index + 1, items.len() as int)),
{
    assert(items.subrange(index, items.len() as int)[0] == items[index]);
    assert(items.subrange(index, items.len() as int).drop_first() == items.subrange(index + 1, items.len() as int));
}

pub proof fn field_costs_step(fields: Seq<(u16, ModelValue)>, index: int)
    requires 0 <= index < fields.len(),
    ensures field_nodes(fields.subrange(index, fields.len() as int)) == nodes(fields[index].1)
        + field_nodes(fields.subrange(index + 1, fields.len() as int)),
        field_payload(fields.subrange(index, fields.len() as int)) == payload_bytes(fields[index].1)
        + field_payload(fields.subrange(index + 1, fields.len() as int)),
{
    assert(fields.subrange(index, fields.len() as int)[0] == fields[index]);
    assert(fields.subrange(index, fields.len() as int).drop_first() == fields.subrange(index + 1, fields.len() as int));
}

pub proof fn entry_costs_step(entries: Seq<(ModelValue, ModelValue)>, index: int)
    requires 0 <= index < entries.len(),
    ensures entry_nodes(entries.subrange(index, entries.len() as int))
        == nodes(entries[index].0) + nodes(entries[index].1)
            + entry_nodes(entries.subrange(index + 1, entries.len() as int)),
        entry_payload(entries.subrange(index, entries.len() as int))
        == spec_encode(entries[index].0).len() + spec_encode(entries[index].1).len()
            + payload_bytes(entries[index].0) + payload_bytes(entries[index].1)
            + entry_payload(entries.subrange(index + 1, entries.len() as int)),
{
    assert(entries.subrange(index, entries.len() as int)[0] == entries[index]);
    assert(entries.subrange(index, entries.len() as int).drop_first() == entries.subrange(index + 1, entries.len() as int));
}

pub broadcast proof fn big_endian_length(word: u128, width: nat)
    ensures #[trigger] big_endian(word, width).len() == width,
    decreases width,
{
    if width > 0 { big_endian_length(word >> 8, (width - 1) as nat); }
}

pub proof fn encoding_nonempty(value: ModelValue)
    ensures spec_encode(value).len() > 0,
{
    match value {
        ModelValue::Unit | ModelValue::Bool(_) | ModelValue::Unsigned(_)
        | ModelValue::Signed(_) | ModelValue::Bytes(_) | ModelValue::Text(_)
        | ModelValue::Enum(_, _) | ModelValue::Tuple(_) | ModelValue::Record(_)
        | ModelValue::Sum(_, _, _) | ModelValue::Vector(_) | ModelValue::Map(_) => {},
    }
}


pub proof fn unsigned_small(bytes: Seq<u8>)
    requires bytes.len() == 1 || bytes.len() == 2 || bytes.len() == 4,
    ensures bytes.len() == 1 ==> unsigned_bytes(bytes) == bytes[0] as u128,
        bytes.len() == 2 ==> unsigned_bytes(bytes) <= u16::MAX,
        bytes.len() == 4 ==> unsigned_bytes(bytes) <= u32::MAX,
{
    reveal_with_fuel(unsigned_bytes, 5);
    let a = bytes[0] as u128;
    if bytes.len() == 1 {
        assert(bytes == seq![bytes[0]]);
        assert((0u128 << 8 | a) == a) by (bit_vector);
    } else if bytes.len() == 2 {
        let b = bytes[1] as u128;
        assert(bytes == seq![bytes[0], bytes[1]]);
        assert(((0u128 << 8 | a) << 8 | b) <= 65535u128) by (bit_vector)
            requires a <= 255, b <= 255;
    } else {
        let b = bytes[1] as u128;
        let c = bytes[2] as u128;
        let d = bytes[3] as u128;
        assert(bytes == seq![bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert(((((0u128 << 8 | a) << 8 | b) << 8 | c) << 8 | d) <= 4294967295u128) by (bit_vector)
            requires a <= 255, b <= 255, c <= 255, d <= 255;
    }
}


pub proof fn encode_items_step(items: Seq<ModelValue>, index: int)
    requires 0 <= index < items.len(),
    ensures encode_items(items.subrange(index, items.len() as int))
        == spec_encode(items[index]) + encode_items(items.subrange(index + 1, items.len() as int)),
{
    let rest = items.subrange(index, items.len() as int);
    assert(rest[0] == items[index]);
    assert(rest.drop_first() == items.subrange(index + 1, items.len() as int));
}

pub proof fn encode_fields_step(fields: Seq<(u16, ModelValue)>, index: int)
    requires 0 <= index < fields.len(),
    ensures encode_fields(fields.subrange(index, fields.len() as int))
        == big_endian(fields[index].0 as u128, 2) + spec_encode(fields[index].1)
            + encode_fields(fields.subrange(index + 1, fields.len() as int)),
        encode_fields(fields.subrange(index, fields.len() as int)).len()
            == 2 + spec_encode(fields[index].1).len()
                + encode_fields(fields.subrange(index + 1, fields.len() as int)).len(),
{
    big_endian_length(fields[index].0 as u128, 2);
    let rest = fields.subrange(index, fields.len() as int);
    assert(rest[0] == fields[index]);
    assert(rest.drop_first() == fields.subrange(index + 1, fields.len() as int));
}

pub proof fn encode_entries_step(entries: Seq<(ModelValue, ModelValue)>, index: int)
    requires 0 <= index < entries.len(),
    ensures encode_entries(entries.subrange(index, entries.len() as int))
        == blob(spec_encode(entries[index].0)) + blob(spec_encode(entries[index].1))
            + encode_entries(entries.subrange(index + 1, entries.len() as int)),
        encode_entries(entries.subrange(index, entries.len() as int)).len()
            == 8 + spec_encode(entries[index].0).len() + spec_encode(entries[index].1).len()
                + encode_entries(entries.subrange(index + 1, entries.len() as int)).len(),
{
    big_endian_length(spec_encode(entries[index].0).len() as u128, 4);
    big_endian_length(spec_encode(entries[index].1).len() as u128, 4);
    let rest = entries.subrange(index, entries.len() as int);
    assert(rest[0] == entries[index]);
    assert(rest.drop_first() == entries.subrange(index + 1, entries.len() as int));
}

}
