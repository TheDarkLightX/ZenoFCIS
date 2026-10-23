//! Completeness of the mathematical decoder by structural induction on values.
use crate::codec::decoder_spec::*;
use crate::native_value::decoder_model::*;
use vstd::prelude::*;

verus! {

pub proof fn word_roundtrip(word: u128, width: u32)
    requires 1 <= width <= 16,
    ensures unsigned_bytes(big_endian(word, width as nat))
        == (word << ((128 - 8 * width) as u32)) >> ((128 - 8 * width) as u32),
    decreases width,
{
    big_endian_length(word, width as nat);
    let bytes = big_endian(word, width as nat);
    let prefix = big_endian(word >> 8, (width - 1) as nat);
    assert(bytes == prefix.push((word % 256) as u8));
    assert(bytes.drop_last() == prefix);
    assert(bytes.last() == (word % 256) as u8);
    assert(unsigned_bytes(bytes) == (unsigned_bytes(prefix) << 8) | (word % 256) as u8 as u128);
    if width == 1 {
        assert(prefix == Seq::<u8>::empty());
        assert((0u128 << 8 | (word % 256) as u8 as u128) == (word << 120) >> 120) by (bit_vector);
    } else {
        let previous_width = (width - 1) as u32;
        word_roundtrip(word >> 8, previous_width);
        assert(prefix == big_endian(word >> 8, previous_width as nat));
        assert(unsigned_bytes(prefix) == ((word >> 8) << ((128 - 8 * previous_width) as u32))
            >> ((128 - 8 * previous_width) as u32));
        assert(((((word >> 8) << ((128 - 8 * previous_width) as u32))
            >> ((128 - 8 * previous_width) as u32)) << 8 | (word % 256) as u8 as u128)
            == (word << ((128 - 8 * width) as u32)) >> ((128 - 8 * width) as u32)) by (bit_vector)
            requires 2 <= width <= 16, previous_width == (width - 1) as u32;
    }
}

pub proof fn read_encoded_blob(data: Seq<u8>, suffix: Seq<u8>, maximum: nat)
    requires data.len() <= u32::MAX, data.len() <= maximum,
    ensures read_blob(blob(data) + suffix, maximum) == Some((data, 4 + data.len())),
        tail(blob(data) + suffix, 4 + data.len()) == suffix,
{
    word_roundtrip(data.len() as u128, 4);
    big_endian_length(data.len() as u128, 4);
    let word = data.len() as u128;
    assert((word << 96) >> 96 == word) by (bit_vector) requires word <= u32::MAX;
    assert((blob(data) + suffix).subrange(0, 4) == length_prefix(data.len()));
    assert((blob(data) + suffix).subrange(4, (4 + data.len()) as int) == data);
}

pub proof fn shape_at_lower_depth(value: ModelValue, limits: ModelLimits, depth: nat, lower: nat)
    requires within_shape(value, limits, depth), lower <= depth,
    ensures within_shape(value, limits, lower),
    decreases value,
{
    match value {
        ModelValue::Tuple(items) | ModelValue::Vector(items) | ModelValue::Sum(_, _, items) => {
            assert forall|i: int| 0 <= i < items.len() implies
                within_shape(#[trigger] items[i], limits, lower + 1) by {
                vstd::seq::lemma_seq_index_decreases(items, i);
                shape_at_lower_depth(items[i], limits, depth + 1, lower + 1);
            }
        },
        ModelValue::Record(fields) => {
            assert forall|i: int| 0 <= i < fields.len() implies
                within_shape((#[trigger] fields[i]).1, limits, lower + 1) by {
                vstd::seq::lemma_seq_index_decreases(fields, i);
                shape_at_lower_depth(fields[i].1, limits, depth + 1, lower + 1);
            }
        },
        ModelValue::Map(entries) => {
            assert forall|i: int| 0 <= i < entries.len() implies
                within_shape((#[trigger] entries[i]).0, limits, lower + 1)
                && within_shape(entries[i].1, limits, lower + 1) by {
                vstd::seq::lemma_seq_index_decreases(entries, i);
                shape_at_lower_depth(entries[i].0, limits, depth + 1, lower + 1);
                shape_at_lower_depth(entries[i].1, limits, depth + 1, lower + 1);
            }
        },
        _ => {},
    }
}

pub proof fn admitted_value_is_wf(value: ModelValue, limits: ModelLimits,
    depth: nat, nodes_before: nat, payload_before: nat)
    requires structurally_valid(value), admitted(value, limits, depth, nodes_before, payload_before),
    ensures wf(value, limits),
{
    shape_at_lower_depth(value, value_limits(limits), depth, 0);
}

/// Structural size induction: node count decreases at children and sequence tails.
pub proof fn inner_complete(value: ModelValue, suffix: Seq<u8>, limits: ModelLimits,
    depth: nat, nodes_before: nat, payload_before: nat)
    requires structurally_valid(value), admitted(value, limits, depth, nodes_before, payload_before),
    ensures parse_inner(spec_encode(value) + suffix, limits, depth, nodes_before, payload_before)
        == Some(Parsed { value, used: spec_encode(value).len(),
            nodes: nodes_before + nodes(value), payload: payload_before + payload_bytes(value) }),
    decreases nodes(value), 2nat,
{
    match value {
        ModelValue::Tuple(_) | ModelValue::Vector(_) =>
            inner_items_complete(value, suffix, limits, depth, nodes_before, payload_before),
        ModelValue::Record(_) => inner_fields_complete(value, suffix, limits, depth, nodes_before, payload_before),
        ModelValue::Sum(_, _, _) => inner_sum_complete(value, suffix, limits, depth, nodes_before, payload_before),
        ModelValue::Map(_) => inner_entries_complete(value, suffix, limits, depth, nodes_before, payload_before),
        _ => inner_leaf_complete(value, suffix, limits, depth, nodes_before, payload_before),
    }
}

pub proof fn inner_leaf_complete(value: ModelValue, suffix: Seq<u8>, limits: ModelLimits,
    depth: nat, nodes_before: nat, payload_before: nat)
    requires structurally_valid(value), admitted(value, limits, depth, nodes_before, payload_before),
        !(value is Tuple || value is Vector || value is Record || value is Sum || value is Map),
    ensures parse_inner(spec_encode(value) + suffix, limits, depth, nodes_before, payload_before)
        == Some(Parsed { value, used: spec_encode(value).len(),
            nodes: nodes_before + nodes(value), payload: payload_before + payload_bytes(value) }),
    decreases nodes(value), 1nat,
{
    nodes_positive(value);
    encoding_nonempty(value);
    let bytes = spec_encode(value) + suffix;
    match value {
        ModelValue::Unit | ModelValue::Bool(_) => {},
        ModelValue::Unsigned(word) => {
            big_endian_length(word, 16);
            word_roundtrip(word, 16);
            assert((word << 0) >> 0 == word) by (bit_vector);
            assert(bytes.subrange(1, 17) == big_endian(word, 16));
        },
        ModelValue::Signed(word) => {
            big_endian_length(signed_bits(word), 16);
            word_roundtrip(signed_bits(word), 16);
            let bits = signed_bits(word);
            assert((bits << 0) >> 0 == bits) by (bit_vector);
            assert(bytes.subrange(1, 17) == big_endian(signed_bits(word), 16));
            assert(signed_word(signed_bits(word)) == word);
        },
        ModelValue::Bytes(data) | ModelValue::Text(data) => {
            read_encoded_blob(data, suffix, limits.max_payload_bytes as nat);
            big_endian_length(data.len() as u128, 4);
            assert(tail(bytes, 1) == blob(data) + suffix);
        },
        ModelValue::Enum(id, variant) => {
            header_roundtrip(id, variant, suffix, 7);
        },
        _ => {},
    }
}

pub proof fn inner_items_complete(value: ModelValue, suffix: Seq<u8>, limits: ModelLimits,
    depth: nat, nodes_before: nat, payload_before: nat)
    requires structurally_valid(value), admitted(value, limits, depth, nodes_before, payload_before),
        value is Tuple || value is Vector,
    ensures parse_inner(spec_encode(value) + suffix, limits, depth, nodes_before, payload_before)
        == Some(Parsed { value, used: spec_encode(value).len(),
            nodes: nodes_before + nodes(value), payload: payload_before + payload_bytes(value) }),
    decreases nodes(value), 1nat,
{
    nodes_positive(value);
    encoding_nonempty(value);
    let bytes = spec_encode(value) + suffix;
    match value {
        ModelValue::Tuple(items) | ModelValue::Vector(items) => {
            count_roundtrip(items.len(), encode_items(items) + suffix);
            assert(bytes.subrange(1, 5) == length_prefix(items.len()));
            assert(unsigned_bytes(bytes.subrange(1, 5)) == items.len());
            assert(tail(bytes, 5) == encode_items(items) + suffix);
            items_complete(items, suffix, limits, depth + 1, nodes_before + 1, payload_before);
        },
        _ => {},
    }
}

pub proof fn inner_fields_complete(value: ModelValue, suffix: Seq<u8>, limits: ModelLimits,
    depth: nat, nodes_before: nat, payload_before: nat)
    requires structurally_valid(value), admitted(value, limits, depth, nodes_before, payload_before),
        value is Record,
    ensures parse_inner(spec_encode(value) + suffix, limits, depth, nodes_before, payload_before)
        == Some(Parsed { value, used: spec_encode(value).len(),
            nodes: nodes_before + nodes(value), payload: payload_before + payload_bytes(value) }),
    decreases nodes(value), 1nat,
{
    nodes_positive(value);
    encoding_nonempty(value);
    let bytes = spec_encode(value) + suffix;
    match value {
        ModelValue::Record(fields) => {
            count_roundtrip(fields.len(), encode_fields(fields) + suffix);
            assert(bytes.subrange(1, 5) == length_prefix(fields.len()));
            assert(unsigned_bytes(bytes.subrange(1, 5)) == fields.len());
            assert(tail(bytes, 5) == encode_fields(fields) + suffix);
            fields_complete(fields, suffix, limits, depth + 1, None, nodes_before + 1, payload_before);
        },
        _ => {},
    }
}

pub proof fn inner_sum_complete(value: ModelValue, suffix: Seq<u8>, limits: ModelLimits,
    depth: nat, nodes_before: nat, payload_before: nat)
    requires structurally_valid(value), admitted(value, limits, depth, nodes_before, payload_before),
        value is Sum,
    ensures parse_inner(spec_encode(value) + suffix, limits, depth, nodes_before, payload_before)
        == Some(Parsed { value, used: spec_encode(value).len(),
            nodes: nodes_before + nodes(value), payload: payload_before + payload_bytes(value) }),
    decreases nodes(value), 1nat,
{
    nodes_positive(value);
    encoding_nonempty(value);
    let bytes = spec_encode(value) + suffix;
    match value {
        ModelValue::Sum(id, variant, payload) => {
            sum_costs(id, variant, payload);
            let rest = if payload.len() == 0 { seq![0u8] + suffix }
                else { seq![1u8] + spec_encode(payload[0]) + suffix };
            header_roundtrip(id, variant, rest, 10);
            assert(bytes == seq![10u8] + big_endian(id as u128, 4)
                + big_endian(variant as u128, 2) + rest);
            assert(bytes[7] == rest[0]);
            if payload.len() > 0 {
                assert(payload[0] == payload.first());
                assert(payload == seq![payload[0]]);
                assert(tail(bytes, 8) == spec_encode(payload[0]) + suffix);
                inner_complete(payload[0], suffix, limits, depth + 1, nodes_before + 1, payload_before);
            } else {
                assert(payload == Seq::<ModelValue>::empty());
            }
        },
        _ => {},
    }
}

pub proof fn inner_entries_complete(value: ModelValue, suffix: Seq<u8>, limits: ModelLimits,
    depth: nat, nodes_before: nat, payload_before: nat)
    requires structurally_valid(value), admitted(value, limits, depth, nodes_before, payload_before),
        value is Map,
    ensures parse_inner(spec_encode(value) + suffix, limits, depth, nodes_before, payload_before)
        == Some(Parsed { value, used: spec_encode(value).len(),
            nodes: nodes_before + nodes(value), payload: payload_before + payload_bytes(value) }),
    decreases nodes(value), 1nat,
{
    nodes_positive(value);
    encoding_nonempty(value);
    let bytes = spec_encode(value) + suffix;
    match value {
        ModelValue::Map(entries) => {
            count_roundtrip(entries.len(), encode_entries(entries) + suffix);
            assert(bytes[0] == 12);
            assert(bytes.len() == 5 + encode_entries(entries).len() + suffix.len());
            assert(bytes.subrange(1, 5) == length_prefix(entries.len()));
            assert(unsigned_bytes(bytes.subrange(1, 5)) == entries.len());
            assert(tail(bytes, 5) == encode_entries(entries) + suffix);
            entries_complete(entries, suffix, limits, depth + 1, None, nodes_before + 1, payload_before);
        },
        _ => {},
    }
}

pub proof fn header_roundtrip(id: u32, variant: u16, suffix: Seq<u8>, tag: u8)
    ensures big_endian(id as u128, 4).len() == 4,
        big_endian(variant as u128, 2).len() == 2,
        ({
        let bytes = seq![tag] + big_endian(id as u128, 4) + big_endian(variant as u128, 2) + suffix;
        &&& bytes.len() == 7 + suffix.len()
        &&& bytes[0] == tag
        &&& unsigned_bytes(bytes.subrange(1, 5)) == id as u128
        &&& unsigned_bytes(bytes.subrange(5, 7)) == variant as u128
        &&& tail(bytes, 7) == suffix
    }),
{
    big_endian_length(id as u128, 4);
    big_endian_length(variant as u128, 2);
    word_roundtrip(id as u128, 4);
    word_roundtrip(variant as u128, 2);
    assert(((id as u128) << 96) >> 96 == id as u128) by (bit_vector);
    assert(((variant as u128) << 112) >> 112 == variant as u128) by (bit_vector);
    let bytes = seq![tag] + big_endian(id as u128, 4) + big_endian(variant as u128, 2) + suffix;
    assert(bytes.subrange(1, 5) == big_endian(id as u128, 4));
    assert(bytes.subrange(5, 7) == big_endian(variant as u128, 2));
}

pub proof fn count_roundtrip(count: nat, suffix: Seq<u8>)
    requires count <= u32::MAX,
    ensures length_prefix(count).len() == 4, unsigned_bytes(length_prefix(count)) == count,
        (length_prefix(count) + suffix).len() == 4 + suffix.len(),
        unsigned_bytes((length_prefix(count) + suffix).subrange(0, 4)) == count,
        tail(length_prefix(count) + suffix, 4) == suffix,
{
    big_endian_length(count as u128, 4);
    word_roundtrip(count as u128, 4);
    let word = count as u128;
    assert((word << 96) >> 96 == word) by (bit_vector) requires word <= u32::MAX;
    assert((length_prefix(count) + suffix).subrange(0, 4) == length_prefix(count));
}

pub proof fn complete_complete(value: ModelValue, limits: ModelLimits,
    depth: nat, nodes_before: nat, payload_before: nat)
    requires structurally_valid(value), admitted(value, limits, depth, nodes_before, payload_before),
    ensures parse_complete(spec_encode(value), limits, depth, nodes_before, payload_before)
        == Some(Parsed { value, used: spec_encode(value).len(),
            nodes: nodes_before + nodes(value), payload: payload_before + payload_bytes(value) }),
    decreases nodes(value), 3nat,
{
    inner_complete(value, Seq::empty(), limits, depth, nodes_before, payload_before);
    assert(spec_encode(value) + Seq::<u8>::empty() == spec_encode(value));
    admitted_value_is_wf(value, limits, depth, nodes_before, payload_before);
}

pub proof fn items_complete(items: Seq<ModelValue>, suffix: Seq<u8>, limits: ModelLimits,
    depth: nat, nodes_before: nat, payload_before: nat)
    requires
        forall|i: int| 0 <= i < items.len() ==> structurally_valid(#[trigger] items[i]),
        items_admitted(items, limits, depth, nodes_before, payload_before),
    ensures parse_items(encode_items(items) + suffix, limits, depth, items.len(), nodes_before, payload_before)
        == Some(ParsedItems { values: items, used: encode_items(items).len(),
            nodes: nodes_before + item_nodes(items), payload: payload_before + item_payload(items) }),
    decreases item_nodes(items), 4nat,
{
    if items.len() == 0 {
        assert(items == Seq::<ModelValue>::empty());
    } else {
        let head = items[0];
        let rest = items.drop_first();
        nodes_positive(head);
        inner_complete(head, encode_items(rest) + suffix, limits, depth, nodes_before, payload_before);
        assert forall|i: int| 0 <= i < rest.len() implies structurally_valid(#[trigger] rest[i]) by {
            assert(rest[i] == items[i + 1]);
        }
        assert forall|i: int| 0 <= i < rest.len() implies within_shape(#[trigger] rest[i], value_limits(limits), depth) by {
            assert(rest[i] == items[i + 1]);
        }
        items_complete(rest, suffix, limits, depth, nodes_before + nodes(head), payload_before + payload_bytes(head));
        assert(encode_items(items) + suffix == spec_encode(head) + (encode_items(rest) + suffix));
        assert(tail(encode_items(items) + suffix, spec_encode(head).len()) == encode_items(rest) + suffix);
        assert(seq![head] + rest == items);
    }
}

pub proof fn fields_complete(fields: Seq<(u16, ModelValue)>, suffix: Seq<u8>, limits: ModelLimits,
    depth: nat, previous: Option<u16>, nodes_before: nat, payload_before: nat)
    requires
        forall|i: int| 0 <= i < fields.len() ==> structurally_valid((#[trigger] fields[i]).1),
        fields_ordered(fields),
        fields.len() > 0 && previous is Some ==> previous->Some_0 < fields[0].0,
        fields_admitted(fields, limits, depth, nodes_before, payload_before),
    ensures parse_fields(encode_fields(fields) + suffix, limits, depth, fields.len(), previous, nodes_before, payload_before)
        == Some(ParsedFields { values: fields, used: encode_fields(fields).len(),
            nodes: nodes_before + field_nodes(fields), payload: payload_before + field_payload(fields) }),
    decreases field_nodes(fields), 4nat,
{
    if fields.len() == 0 {
        assert(fields == Seq::<(u16, ModelValue)>::empty());
    } else {
        let head = fields[0];
        let rest = fields.drop_first();
        let bytes = encode_fields(fields) + suffix;
        nodes_positive(head.1);
        big_endian_length(head.0 as u128, 2);
        word_roundtrip(head.0 as u128, 2);
        let id = head.0;
        assert(((id as u128) << 112) >> 112 == id as u128) by (bit_vector);
        assert(bytes.subrange(0, 2) == big_endian(head.0 as u128, 2));
        assert(tail(bytes, 2) == spec_encode(head.1) + (encode_fields(rest) + suffix));
        inner_complete(head.1, encode_fields(rest) + suffix, limits, depth, nodes_before, payload_before);
        assert forall|i: int| 0 <= i < rest.len() implies structurally_valid((#[trigger] rest[i]).1) by {
            assert(rest[i] == fields[i + 1]);
        }
        assert forall|i: int| 0 <= i < rest.len() implies within_shape((#[trigger] rest[i]).1, value_limits(limits), depth) by {
            assert(rest[i] == fields[i + 1]);
        }
        assert forall|i: int| 0 <= i && i + 1 < rest.len() implies (#[trigger] rest[i]).0 < rest[i + 1].0 by {
            assert(rest[i] == fields[i + 1]);
            assert(rest[i + 1] == fields[i + 2]);
            assert(fields[i + 1].0 < fields[i + 2].0);
        }
        if rest.len() > 0 { assert(fields[0].0 < fields[1].0); }
        fields_complete(rest, suffix, limits, depth, Some(head.0),
            nodes_before + nodes(head.1), payload_before + payload_bytes(head.1));
        assert(tail(bytes, 2 + spec_encode(head.1).len()) == encode_fields(rest) + suffix);
        assert(seq![head] + rest == fields);
    }
}

pub proof fn entries_complete(entries: Seq<(ModelValue, ModelValue)>, suffix: Seq<u8>, limits: ModelLimits,
    depth: nat, previous: Option<Seq<u8>>, nodes_before: nat, payload_before: nat)
    requires
        forall|i: int| 0 <= i < entries.len() ==> {
            &&& structurally_valid((#[trigger] entries[i]).0)
            &&& structurally_valid(entries[i].1)
            &&& spec_encode(entries[i].0).len() <= u32::MAX
            &&& spec_encode(entries[i].1).len() <= u32::MAX
        },
        entries_ordered(entries),
        entries.len() > 0 && previous is Some ==> lex_less(previous->Some_0, spec_encode(entries[0].0)),
        entries_admitted(entries, limits, depth, nodes_before, payload_before),
    ensures parse_entries(encode_entries(entries) + suffix, limits, depth, entries.len(), previous, nodes_before, payload_before)
        == Some(ParsedEntries { values: entries, used: encode_entries(entries).len(),
            nodes: nodes_before + entry_nodes(entries), payload: payload_before + entry_payload(entries) }),
    decreases entry_nodes(entries), 4nat,
{
    if entries.len() == 0 {
        assert(entries == Seq::<(ModelValue, ModelValue)>::empty());
    } else {
        let head = entries[0];
        let rest = entries.drop_first();
        let key = spec_encode(head.0);
        let value = spec_encode(head.1);
        let bytes = encode_entries(entries) + suffix;
        let charged = payload_before + key.len() + value.len();
        nodes_positive(head.0);
        nodes_positive(head.1);
        read_encoded_blob(key, blob(value) + (encode_entries(rest) + suffix), limits.max_payload_bytes as nat);
        read_encoded_blob(value, encode_entries(rest) + suffix, limits.max_payload_bytes as nat);
        assert(bytes == blob(key) + (blob(value) + (encode_entries(rest) + suffix)));
        assert(tail(bytes, 4 + key.len()) == blob(value) + (encode_entries(rest) + suffix));
        complete_complete(head.0, limits, depth, nodes_before, charged);
        complete_complete(head.1, limits, depth, nodes_before + nodes(head.0), charged + payload_bytes(head.0));
        assert forall|i: int| 0 <= i < rest.len() implies {
            &&& structurally_valid((#[trigger] rest[i]).0)
            &&& structurally_valid(rest[i].1)
            &&& spec_encode(rest[i].0).len() <= u32::MAX
            &&& spec_encode(rest[i].1).len() <= u32::MAX
        } by { assert(rest[i] == entries[i + 1]); }
        assert forall|i: int| 0 <= i < rest.len() implies
            within_shape((#[trigger] rest[i]).0, value_limits(limits), depth)
            && within_shape(rest[i].1, value_limits(limits), depth) by {
            assert(rest[i] == entries[i + 1]);
        }
        assert forall|i: int| 0 <= i && i + 1 < rest.len() implies
            lex_less(spec_encode((#[trigger] rest[i]).0), spec_encode(rest[i + 1].0)) by {
            assert(rest[i] == entries[i + 1]);
            assert(rest[i + 1] == entries[i + 2]);
            assert(lex_less(spec_encode(entries[i + 1].0), spec_encode(entries[i + 2].0)));
        }
        if rest.len() > 0 { assert(lex_less(spec_encode(entries[0].0), spec_encode(entries[1].0))); }
        entries_complete(rest, suffix, limits, depth, Some(key),
            nodes_before + nodes(head.0) + nodes(head.1), charged + payload_bytes(head.0) + payload_bytes(head.1));
        assert(tail(bytes, 8 + key.len() + value.len()) == encode_entries(rest) + suffix);
        assert(seq![head] + rest == entries);
    }
}

pub proof fn spec_t5_complete(value: ModelValue, limits: ModelLimits)
    requires wf(value, limits), spec_encode(value).len() <= limits.max_input_bytes,
    ensures spec_decode(spec_encode(value), limits) == Some((value, spec_encode(value).len())),
{
    complete_complete(value, value_limits(limits), 0, 0, 0);
}

}
