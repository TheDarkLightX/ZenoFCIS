//! Independent literals from the codec's tag corpus and ZCVE golden map.
use crate::decoder_spec::*;
use vstd::prelude::*;
use zeno_fcis_value::decoder_model::*;

verus! {

pub proof fn canonical_encoding_vectors() {
    assert(spec_encode(ModelValue::Unit) == seq![0u8]) by (compute);
    assert(spec_encode(ModelValue::Bool(false)) == seq![1u8]) by (compute);
    assert(spec_encode(ModelValue::Bool(true)) == seq![2u8]) by (compute);
    assert(spec_encode(ModelValue::Unsigned(1)) ==
        seq![3u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]) by (compute);
    assert(spec_encode(ModelValue::Signed((-1int) as i128)) ==
        seq![4u8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255]) by (compute);
    assert(spec_encode(ModelValue::Bytes(seq![1u8, 2, 3])) ==
        seq![5u8, 0, 0, 0, 3, 1, 2, 3]) by (compute);
    assert(spec_encode(ModelValue::Text(seq![97u8, 98, 99])) ==
        seq![6u8, 0, 0, 0, 3, 97, 98, 99]) by (compute);
    assert(spec_encode(ModelValue::Enum(4, 5)) == seq![7u8, 0, 0, 0, 4, 0, 5]) by (compute);
    assert(spec_encode(ModelValue::Tuple(seq![ModelValue::Unit, ModelValue::Bool(true)])) ==
        seq![8u8, 0, 0, 0, 2, 0, 2]) by (compute);
    assert(spec_encode(ModelValue::Record(seq![(1u16, ModelValue::Unit), (2u16, ModelValue::Bool(true))])) ==
        seq![9u8, 0, 0, 0, 2, 0, 1, 0, 0, 2, 2]) by (compute);
    assert(spec_encode(ModelValue::Sum(6, 7, Seq::empty())) ==
        seq![10u8, 0, 0, 0, 6, 0, 7, 0]) by (compute);
    assert(spec_encode(ModelValue::Sum(6, 7, seq![ModelValue::Bool(false)])) ==
        seq![10u8, 0, 0, 0, 6, 0, 7, 1, 1]) by (compute);
    assert(spec_encode(ModelValue::Vector(seq![ModelValue::Unit, ModelValue::Bool(true)])) ==
        seq![11u8, 0, 0, 0, 2, 0, 2]) by (compute);
    assert(spec_encode(ModelValue::Map(seq![(ModelValue::Unsigned(1), ModelValue::Bool(true))])) ==
        seq![12u8, 0, 0, 0, 1, 0, 0, 0, 17, 3, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 2]) by (compute);
}

pub open spec fn vector_limits() -> ModelLimits {
    ModelLimits { max_depth: 64, max_nodes: 1000000, max_payload_bytes: 67108864,
        max_collection_len: 1000000, max_input_bytes: 67108864 }
}

pub proof fn scalar_decoding_vectors() {
    assert(spec_decode(seq![0u8], vector_limits()) == Some((ModelValue::Unit, 1nat))) by (compute);
    assert(spec_decode(seq![1u8], vector_limits()) == Some((ModelValue::Bool(false), 1nat))) by (compute);
    assert(spec_decode(seq![2u8], vector_limits()) == Some((ModelValue::Bool(true), 1nat))) by (compute);
    assert(spec_decode(seq![2u8, 0], vector_limits()) is None) by (compute);
    assert(spec_decode(seq![255u8], vector_limits()) is None) by (compute);
    assert(spec_decode(Seq::empty(), vector_limits()) is None) by (compute);
}

// Exact literals and metrics from tests/limits.rs::independent_wire_corpus_agrees_with_actual_decoder_metrics.
pub open spec fn corpus_value(index: u32) -> ModelValue {
    match index {
        0 => ModelValue::Unit,
        1 => ModelValue::Bool(false),
        2 => ModelValue::Bool(true),
        3 => ModelValue::Unsigned(u128::MAX),
        4 => ModelValue::Signed(i128::MIN),
        5 => ModelValue::Bytes(seq![9u8, 10]),
        6 => ModelValue::Text(seq![97u8, 98]),
        7 => ModelValue::Enum(0, 0),
        8 => ModelValue::Enum(u32::MAX, u16::MAX),
        9 => ModelValue::Tuple(seq![ModelValue::Unit, ModelValue::Bool(true)]),
        10 => ModelValue::Record(seq![(0u16, ModelValue::Unit), (65535u16, ModelValue::Bool(false))]),
        11 => ModelValue::Sum(0, 0, Seq::empty()),
        12 => ModelValue::Sum(u32::MAX, u16::MAX, seq![ModelValue::Bool(true)]),
        13 => ModelValue::Vector(seq![ModelValue::Unit]),
        14 => ModelValue::Map(seq![(ModelValue::Unit, ModelValue::Bool(true))]),
        15 => ModelValue::Sum(0, 0, seq![ModelValue::Map(seq![(ModelValue::Enum(0, 0), ModelValue::Map(seq![(ModelValue::Unit, ModelValue::Bytes(seq![1u8, 2]))]))])]),
        _ => ModelValue::Unit,
    }
}

pub open spec fn corpus_wire(index: u32) -> Seq<u8> {
    match index {
        0 => seq![0u8],
        1 => seq![1u8],
        2 => seq![2u8],
        3 => seq![3u8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255],
        4 => seq![4u8, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        5 => seq![5u8, 0, 0, 0, 2, 9, 10],
        6 => seq![6u8, 0, 0, 0, 2, 97, 98],
        7 => seq![7u8, 0, 0, 0, 0, 0, 0],
        8 => seq![7u8, 255, 255, 255, 255, 255, 255],
        9 => seq![8u8, 0, 0, 0, 2, 0, 2],
        10 => seq![9u8, 0, 0, 0, 2, 0, 0, 0, 255, 255, 1],
        11 => seq![10u8, 0, 0, 0, 0, 0, 0, 0],
        12 => seq![10u8, 255, 255, 255, 255, 255, 255, 1, 2],
        13 => seq![11u8, 0, 0, 0, 1, 0],
        14 => seq![12u8, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 2],
        15 => seq![10u8, 0, 0, 0, 0, 0, 0, 1, 12, 0, 0, 0, 1, 0, 0, 0, 7, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 21, 12, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 7, 5, 0, 0, 0, 2, 1, 2],
        _ => Seq::empty(),
    }
}

pub open spec fn corpus_limits(index: u32) -> ModelLimits {
    match index {
        0 => ModelLimits { max_depth: 0, max_nodes: 1, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 1 },
        1 => ModelLimits { max_depth: 0, max_nodes: 1, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 1 },
        2 => ModelLimits { max_depth: 0, max_nodes: 1, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 1 },
        3 => ModelLimits { max_depth: 0, max_nodes: 1, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 17 },
        4 => ModelLimits { max_depth: 0, max_nodes: 1, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 17 },
        5 => ModelLimits { max_depth: 0, max_nodes: 1, max_payload_bytes: 2, max_collection_len: 2, max_input_bytes: 7 },
        6 => ModelLimits { max_depth: 0, max_nodes: 1, max_payload_bytes: 2, max_collection_len: 2, max_input_bytes: 7 },
        7 => ModelLimits { max_depth: 0, max_nodes: 1, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 7 },
        8 => ModelLimits { max_depth: 0, max_nodes: 1, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 7 },
        9 => ModelLimits { max_depth: 1, max_nodes: 3, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 7 },
        10 => ModelLimits { max_depth: 1, max_nodes: 3, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 11 },
        11 => ModelLimits { max_depth: 0, max_nodes: 1, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 8 },
        12 => ModelLimits { max_depth: 1, max_nodes: 2, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 9 },
        13 => ModelLimits { max_depth: 1, max_nodes: 2, max_payload_bytes: 0, max_collection_len: 2, max_input_bytes: 6 },
        14 => ModelLimits { max_depth: 1, max_nodes: 3, max_payload_bytes: 2, max_collection_len: 2, max_input_bytes: 15 },
        15 => ModelLimits { max_depth: 3, max_nodes: 6, max_payload_bytes: 38, max_collection_len: 2, max_input_bytes: 49 },
        _ => vector_limits(),
    }
}

pub proof fn independent_corpus_encoding()
    ensures
        spec_encode(corpus_value(0)) == corpus_wire(0),
        spec_encode(corpus_value(1)) == corpus_wire(1),
        spec_encode(corpus_value(2)) == corpus_wire(2),
        spec_encode(corpus_value(3)) == corpus_wire(3),
        spec_encode(corpus_value(4)) == corpus_wire(4),
        spec_encode(corpus_value(5)) == corpus_wire(5),
        spec_encode(corpus_value(6)) == corpus_wire(6),
        spec_encode(corpus_value(7)) == corpus_wire(7),
        spec_encode(corpus_value(8)) == corpus_wire(8),
        spec_encode(corpus_value(9)) == corpus_wire(9),
        spec_encode(corpus_value(10)) == corpus_wire(10),
        spec_encode(corpus_value(11)) == corpus_wire(11),
        spec_encode(corpus_value(12)) == corpus_wire(12),
        spec_encode(corpus_value(13)) == corpus_wire(13),
        spec_encode(corpus_value(14)) == corpus_wire(14),
        spec_encode(corpus_value(15)) == corpus_wire(15),
{
    assert(spec_encode(corpus_value(0)) == corpus_wire(0)) by (compute);
    assert(spec_encode(corpus_value(1)) == corpus_wire(1)) by (compute);
    assert(spec_encode(corpus_value(2)) == corpus_wire(2)) by (compute);
    assert(spec_encode(corpus_value(3)) == corpus_wire(3)) by (compute);
    assert(spec_encode(corpus_value(4)) == corpus_wire(4)) by (compute);
    assert(spec_encode(corpus_value(5)) == corpus_wire(5)) by (compute);
    assert(spec_encode(corpus_value(6)) == corpus_wire(6)) by (compute);
    assert(spec_encode(corpus_value(7)) == corpus_wire(7)) by (compute);
    assert(spec_encode(corpus_value(8)) == corpus_wire(8)) by (compute);
    assert(spec_encode(corpus_value(9)) == corpus_wire(9)) by (compute);
    assert(spec_encode(corpus_value(10)) == corpus_wire(10)) by (compute);
    assert(spec_encode(corpus_value(11)) == corpus_wire(11)) by (compute);
    assert(spec_encode(corpus_value(12)) == corpus_wire(12)) by (compute);
    assert(spec_encode(corpus_value(13)) == corpus_wire(13)) by (compute);
    assert(spec_encode(corpus_value(14)) == corpus_wire(14)) by (compute);
    assert(spec_encode(corpus_value(15)) == corpus_wire(15)) by (compute);
}

pub proof fn corpus_admits_unit()
    ensures wf(corpus_value(0), corpus_limits(0)),
{
    assert(nodes(corpus_value(0)) == 1 && payload_bytes(corpus_value(0)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_false()
    ensures wf(corpus_value(1), corpus_limits(1)),
{
    assert(nodes(corpus_value(1)) == 1 && payload_bytes(corpus_value(1)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_true()
    ensures wf(corpus_value(2), corpus_limits(2)),
{
    assert(nodes(corpus_value(2)) == 1 && payload_bytes(corpus_value(2)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_unsigned_max()
    ensures wf(corpus_value(3), corpus_limits(3)),
{
    assert(nodes(corpus_value(3)) == 1 && payload_bytes(corpus_value(3)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_signed_min()
    ensures wf(corpus_value(4), corpus_limits(4)),
{
    assert(nodes(corpus_value(4)) == 1 && payload_bytes(corpus_value(4)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_bytes()
    ensures wf(corpus_value(5), corpus_limits(5)),
{
    assert(nodes(corpus_value(5)) == 1 && payload_bytes(corpus_value(5)) == 2) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_ascii()
    ensures wf(corpus_value(6), corpus_limits(6)),
{
    assert(nodes(corpus_value(6)) == 1 && payload_bytes(corpus_value(6)) == 2) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_enum_zero()
    ensures wf(corpus_value(7), corpus_limits(7)),
{
    assert(nodes(corpus_value(7)) == 1 && payload_bytes(corpus_value(7)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_enum_max()
    ensures wf(corpus_value(8), corpus_limits(8)),
{
    assert(nodes(corpus_value(8)) == 1 && payload_bytes(corpus_value(8)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_tuple()
    ensures wf(corpus_value(9), corpus_limits(9)),
{
    assert(nodes(corpus_value(9)) == 3 && payload_bytes(corpus_value(9)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_record()
    ensures wf(corpus_value(10), corpus_limits(10)),
{
    assert(nodes(corpus_value(10)) == 3 && payload_bytes(corpus_value(10)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_sum_empty()
    ensures wf(corpus_value(11), corpus_limits(11)),
{
    assert(nodes(corpus_value(11)) == 1 && payload_bytes(corpus_value(11)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_sum_payload()
    ensures wf(corpus_value(12), corpus_limits(12)),
{
    assert(nodes(corpus_value(12)) == 2 && payload_bytes(corpus_value(12)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_vector()
    ensures wf(corpus_value(13), corpus_limits(13)),
{
    assert(nodes(corpus_value(13)) == 2 && payload_bytes(corpus_value(13)) == 0) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_map()
    ensures wf(corpus_value(14), corpus_limits(14)),
{
    assert(nodes(corpus_value(14)) == 3 && payload_bytes(corpus_value(14)) == 2) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn corpus_admits_nested_map()
    ensures wf(corpus_value(15), corpus_limits(15)),
{
    assert(nodes(corpus_value(15)) == 6 && payload_bytes(corpus_value(15)) == 38) by (compute);
    reveal_with_fuel(structurally_valid, 5);
    reveal_with_fuel(within_shape, 5);
    reveal_with_fuel(spec_encode, 5);
    reveal_with_fuel(encode_entries, 3);
    reveal_with_fuel(encode_items, 3);
    broadcast use big_endian_length;
}

pub proof fn independent_corpus_metrics()
    ensures
        nodes(corpus_value(0)) == 1 && payload_bytes(corpus_value(0)) == 0,
        nodes(corpus_value(1)) == 1 && payload_bytes(corpus_value(1)) == 0,
        nodes(corpus_value(2)) == 1 && payload_bytes(corpus_value(2)) == 0,
        nodes(corpus_value(3)) == 1 && payload_bytes(corpus_value(3)) == 0,
        nodes(corpus_value(4)) == 1 && payload_bytes(corpus_value(4)) == 0,
        nodes(corpus_value(5)) == 1 && payload_bytes(corpus_value(5)) == 2,
        nodes(corpus_value(6)) == 1 && payload_bytes(corpus_value(6)) == 2,
        nodes(corpus_value(7)) == 1 && payload_bytes(corpus_value(7)) == 0,
        nodes(corpus_value(8)) == 1 && payload_bytes(corpus_value(8)) == 0,
        nodes(corpus_value(9)) == 3 && payload_bytes(corpus_value(9)) == 0,
        nodes(corpus_value(10)) == 3 && payload_bytes(corpus_value(10)) == 0,
        nodes(corpus_value(11)) == 1 && payload_bytes(corpus_value(11)) == 0,
        nodes(corpus_value(12)) == 2 && payload_bytes(corpus_value(12)) == 0,
        nodes(corpus_value(13)) == 2 && payload_bytes(corpus_value(13)) == 0,
        nodes(corpus_value(14)) == 3 && payload_bytes(corpus_value(14)) == 2,
        nodes(corpus_value(15)) == 6 && payload_bytes(corpus_value(15)) == 38,
{
    assert(nodes(corpus_value(0)) == 1 && payload_bytes(corpus_value(0)) == 0) by (compute);
    assert(nodes(corpus_value(1)) == 1 && payload_bytes(corpus_value(1)) == 0) by (compute);
    assert(nodes(corpus_value(2)) == 1 && payload_bytes(corpus_value(2)) == 0) by (compute);
    assert(nodes(corpus_value(3)) == 1 && payload_bytes(corpus_value(3)) == 0) by (compute);
    assert(nodes(corpus_value(4)) == 1 && payload_bytes(corpus_value(4)) == 0) by (compute);
    assert(nodes(corpus_value(5)) == 1 && payload_bytes(corpus_value(5)) == 2) by (compute);
    assert(nodes(corpus_value(6)) == 1 && payload_bytes(corpus_value(6)) == 2) by (compute);
    assert(nodes(corpus_value(7)) == 1 && payload_bytes(corpus_value(7)) == 0) by (compute);
    assert(nodes(corpus_value(8)) == 1 && payload_bytes(corpus_value(8)) == 0) by (compute);
    assert(nodes(corpus_value(9)) == 3 && payload_bytes(corpus_value(9)) == 0) by (compute);
    assert(nodes(corpus_value(10)) == 3 && payload_bytes(corpus_value(10)) == 0) by (compute);
    assert(nodes(corpus_value(11)) == 1 && payload_bytes(corpus_value(11)) == 0) by (compute);
    assert(nodes(corpus_value(12)) == 2 && payload_bytes(corpus_value(12)) == 0) by (compute);
    assert(nodes(corpus_value(13)) == 2 && payload_bytes(corpus_value(13)) == 0) by (compute);
    assert(nodes(corpus_value(14)) == 3 && payload_bytes(corpus_value(14)) == 2) by (compute);
    assert(nodes(corpus_value(15)) == 6 && payload_bytes(corpus_value(15)) == 38) by (compute);
}

// The existing two-byte key/value map charges 14 framed bytes plus 4 leaf bytes.
pub open spec fn payload_map_value() -> ModelValue {
    ModelValue::Map(seq![(ModelValue::Bytes(seq![97u8, 98]), ModelValue::Bytes(seq![99u8, 100]))])
}

pub open spec fn payload_map_wire() -> Seq<u8> {
    seq![12u8, 0, 0, 0, 1, 0, 0, 0, 7, 5, 0, 0, 0, 2, 97, 98, 0, 0, 0, 7, 5, 0, 0, 0, 2, 99, 100]
}

pub open spec fn payload_map_limits(payload: u64) -> ModelLimits {
    ModelLimits { max_depth: 1, max_nodes: 3, max_payload_bytes: payload,
        max_collection_len: 1, max_input_bytes: 27 }
}

pub proof fn map_payload_boundaries() {
    assert(spec_encode(payload_map_value()) == payload_map_wire()) by (compute);
    assert(payload_bytes(payload_map_value()) == 18 && nodes(payload_map_value()) == 3) by (compute);
    reveal_with_fuel(structurally_valid, 3);
    reveal_with_fuel(within_shape, 3);
    broadcast use big_endian_length;
    assert(wf(payload_map_value(), payload_map_limits(18)));
    assert(!wf(payload_map_value(), payload_map_limits(17)));
    assert(!wf(payload_map_value(), payload_map_limits(16)));
    assert(!wf(payload_map_value(), payload_map_limits(15)));
    assert(!wf(payload_map_value(), payload_map_limits(14)));
    assert(!wf(payload_map_value(), payload_map_limits(13)));
    assert(!wf(payload_map_value(), payload_map_limits(7)));
    assert(!wf(payload_map_value(), payload_map_limits(6)));
}

pub open spec fn structural_boundary_limits(depth: u32, nodes: u64, payload: u64, collection: u32, input: u64) -> ModelLimits {
    ModelLimits { max_depth: depth, max_nodes: nodes, max_payload_bytes: payload,
        max_collection_len: collection, max_input_bytes: input }
}

pub open spec fn structural_boundary_value() -> ModelValue {
    ModelValue::Vector(seq![ModelValue::Bytes(seq![1u8, 2, 3])])
}

pub open spec fn structural_boundary_wire() -> Seq<u8> {
    seq![11u8, 0, 0, 0, 1, 5, 0, 0, 0, 3, 1, 2, 3]
}

pub proof fn structural_limit_boundaries() {
    assert(spec_encode(structural_boundary_value()) == structural_boundary_wire()) by (compute);
    assert(nodes(structural_boundary_value()) == 2 && payload_bytes(structural_boundary_value()) == 3) by (compute);
    reveal_with_fuel(structurally_valid, 3);
    reveal_with_fuel(within_shape, 3);
    reveal_with_fuel(lex_less, 3);
    assert(wf(structural_boundary_value(), structural_boundary_limits(1, 2, 3, 1, 13)));
    assert(!wf(structural_boundary_value(), structural_boundary_limits(0, 2, 3, 1, 13)));
    assert(!wf(structural_boundary_value(), structural_boundary_limits(1, 1, 3, 1, 13)));
    assert(!wf(structural_boundary_value(), structural_boundary_limits(1, 2, 2, 1, 13)));
    assert(!wf(structural_boundary_value(), structural_boundary_limits(1, 2, 3, 0, 13)));
    assert(spec_decode(structural_boundary_wire(), structural_boundary_limits(1, 2, 3, 1, 12)) is None);
    assert(depth_limit(structural_boundary_limits(u32::MAX, 1, 0, 0, 1)) == 1024) by (compute);
    assert(depth_limit(structural_boundary_limits(1023, 1, 0, 0, 1)) == 1023) by (compute);
    assert(!wf(ModelValue::Text(seq![195u8, 169]), vector_limits()));
    assert(spec_decode(seq![6u8, 0, 0, 0, 2, 195, 169], vector_limits()) is None);
    assert(!wf(ModelValue::Record(seq![(1u16, ModelValue::Unit), (1u16, ModelValue::Unit)]), vector_limits()));
    assert(!wf(ModelValue::Record(seq![(2u16, ModelValue::Unit), (1u16, ModelValue::Unit)]), vector_limits()));
    assert(!wf(ModelValue::Map(seq![(ModelValue::Unit, ModelValue::Unit), (ModelValue::Unit, ModelValue::Bool(true))]), vector_limits()));
    assert(!wf(ModelValue::Map(seq![(ModelValue::Bool(true), ModelValue::Unit), (ModelValue::Unit, ModelValue::Unit)]), vector_limits()));
    assert(spec_decode(seq![10u8, 0, 0, 0, 0, 0, 0, 2], vector_limits()) is None) by (compute);
    assert(spec_decode(seq![5u8, 0, 0, 0, 2, 1], vector_limits()) is None) by (compute);
}

}
