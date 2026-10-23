//! Total mathematical decoder. Native correspondence is proved separately.
use vstd::prelude::*;
use zeno_fcis_value::decoder_model::*;

verus! {

pub struct Parsed {
    pub value: ModelValue,
    pub used: nat,
    pub nodes: nat,
    pub payload: nat,
}

pub struct ParsedItems {
    pub values: Seq<ModelValue>,
    pub used: nat,
    pub nodes: nat,
    pub payload: nat,
}

pub struct ParsedFields {
    pub values: Seq<(u16, ModelValue)>,
    pub used: nat,
    pub nodes: nat,
    pub payload: nat,
}

pub struct ParsedEntries {
    pub values: Seq<(ModelValue, ModelValue)>,
    pub used: nat,
    pub nodes: nat,
    pub payload: nat,
}

pub open spec fn depth_remaining(limits: ModelLimits, depth: nat) -> nat {
    if depth <= depth_limit(limits) { (depth_limit(limits) + 1 - depth) as nat } else { 0 }
}

pub open spec fn tail(bytes: Seq<u8>, used: nat) -> Seq<u8> {
    bytes.subrange(used as int, bytes.len() as int)
}

pub open spec fn read_blob(bytes: Seq<u8>, maximum: nat) -> Option<(Seq<u8>, nat)> {
    if bytes.len() < 4 { None } else {
        let length = unsigned_bytes(bytes.subrange(0, 4)) as nat;
        if length > maximum || 4 + length > bytes.len() { None }
        else { Some((bytes.subrange(4, (4 + length) as int), 4 + length)) }
    }
}

pub open spec fn parse_inner(bytes: Seq<u8>, limits: ModelLimits, depth: nat,
    nodes_before: nat, payload_before: nat) -> Option<Parsed>
    decreases depth_remaining(limits, depth), 0nat, 1nat,
{
    if depth > depth_limit(limits) || nodes_before >= limits.max_nodes
        || payload_before > limits.max_payload_bytes || bytes.len() == 0 { None }
    else {
        let nodes = nodes_before + 1;
        let tag = bytes[0];
        if tag == 0 || tag == 1 || tag == 2 {
            Some(Parsed { value: if tag == 0 { ModelValue::Unit } else { ModelValue::Bool(tag == 2) },
                used: 1, nodes, payload: payload_before })
        } else if tag == 3 || tag == 4 {
            if bytes.len() < 17 { None } else {
                let word = unsigned_bytes(bytes.subrange(1, 17));
                Some(Parsed { value: if tag == 3 { ModelValue::Unsigned(word) }
                    else { ModelValue::Signed(signed_word(word)) }, used: 17, nodes, payload: payload_before })
            }
        } else if tag == 5 || tag == 6 {
            match read_blob(tail(bytes, 1), limits.max_payload_bytes as nat) {
                None => None,
                Some((data, used)) => if payload_before + data.len() > limits.max_payload_bytes
                    || (tag == 6 && !ascii(data)) { None } else {
                    Some(Parsed { value: if tag == 5 { ModelValue::Bytes(data) } else { ModelValue::Text(data) },
                        used: 1 + used, nodes, payload: payload_before + data.len() })
                },
            }
        } else if tag == 7 {
            if bytes.len() < 7 { None } else {
                Some(Parsed { value: ModelValue::Enum(unsigned_bytes(bytes.subrange(1, 5)) as u32,
                    unsigned_bytes(bytes.subrange(5, 7)) as u16), used: 7, nodes, payload: payload_before })
            }
        } else if tag == 8 || tag == 11 {
            if bytes.len() < 5 { None } else {
                let count = unsigned_bytes(bytes.subrange(1, 5)) as nat;
                if count > limits.max_collection_len { None } else {
                    match parse_items(tail(bytes, 5), limits, depth + 1, count, nodes, payload_before) {
                        None => None,
                        Some(result) => Some(Parsed {
                            value: if tag == 8 { ModelValue::Tuple(result.values) } else { ModelValue::Vector(result.values) },
                            used: 5 + result.used, nodes: result.nodes, payload: result.payload }),
                    }
                }
            }
        } else if tag == 9 {
            if bytes.len() < 5 { None } else {
                let count = unsigned_bytes(bytes.subrange(1, 5)) as nat;
                if count > limits.max_collection_len { None } else {
                    match parse_fields(tail(bytes, 5), limits, depth + 1, count, None, nodes, payload_before) {
                        None => None,
                        Some(result) => Some(Parsed { value: ModelValue::Record(result.values),
                            used: 5 + result.used, nodes: result.nodes, payload: result.payload }),
                    }
                }
            }
        } else if tag == 10 {
            if bytes.len() < 8 { None } else {
                let id = unsigned_bytes(bytes.subrange(1, 5)) as u32;
                let variant = unsigned_bytes(bytes.subrange(5, 7)) as u16;
                if bytes[7] == 0 {
                    Some(Parsed { value: ModelValue::Sum(id, variant, Seq::empty()),
                        used: 8, nodes, payload: payload_before })
                } else if bytes[7] == 1 {
                    match parse_inner(tail(bytes, 8), limits, depth + 1, nodes, payload_before) {
                        None => None,
                        Some(result) => Some(Parsed { value: ModelValue::Sum(id, variant, seq![result.value]),
                            used: 8 + result.used, nodes: result.nodes, payload: result.payload }),
                    }
                } else { None }
            }
        } else if tag == 12 {
            if bytes.len() < 5 { None } else {
                let count = unsigned_bytes(bytes.subrange(1, 5)) as nat;
                if count > limits.max_collection_len { None } else {
                    match parse_entries(tail(bytes, 5), limits, depth + 1, count, None, nodes, payload_before) {
                        None => None,
                        Some(result) => Some(Parsed { value: ModelValue::Map(result.values),
                            used: 5 + result.used, nodes: result.nodes, payload: result.payload }),
                    }
                }
            }
        } else { None }
    }
}

pub open spec fn parse_items(bytes: Seq<u8>, limits: ModelLimits, depth: nat,
    count: nat, nodes: nat, payload: nat) -> Option<ParsedItems>
    decreases depth_remaining(limits, depth), count + 1, 3nat,
{
    if count == 0 { Some(ParsedItems { values: Seq::empty(), used: 0, nodes, payload }) }
    else {
        match parse_inner(bytes, limits, depth, nodes, payload) {
            None => None,
            Some(head) => if head.used > bytes.len() { None } else {
                match parse_items(tail(bytes, head.used), limits, depth, (count - 1) as nat, head.nodes, head.payload) {
                    None => None,
                    Some(rest) => Some(ParsedItems { values: seq![head.value] + rest.values,
                        used: head.used + rest.used, nodes: rest.nodes, payload: rest.payload }),
                }
            },
        }
    }
}

pub open spec fn parse_fields(bytes: Seq<u8>, limits: ModelLimits, depth: nat,
    count: nat, previous: Option<u16>, nodes: nat, payload: nat) -> Option<ParsedFields>
    decreases depth_remaining(limits, depth), count + 1, 3nat,
{
    if count == 0 { Some(ParsedFields { values: Seq::empty(), used: 0, nodes, payload }) }
    else if bytes.len() < 2 { None } else {
        let id = unsigned_bytes(bytes.subrange(0, 2)) as u16;
        if previous matches Some(old) && old >= id { None } else {
            match parse_inner(tail(bytes, 2), limits, depth, nodes, payload) {
                None => None,
                Some(head) => if 2 + head.used > bytes.len() { None } else {
                    match parse_fields(tail(bytes, 2 + head.used), limits, depth, (count - 1) as nat,
                        Some(id), head.nodes, head.payload) {
                        None => None,
                        Some(rest) => Some(ParsedFields { values: seq![(id, head.value)] + rest.values,
                            used: 2 + head.used + rest.used, nodes: rest.nodes, payload: rest.payload }),
                    }
                },
            }
        }
    }
}

pub open spec fn parse_entries(bytes: Seq<u8>, limits: ModelLimits, depth: nat,
    count: nat, previous: Option<Seq<u8>>, nodes: nat, payload: nat) -> Option<ParsedEntries>
    decreases depth_remaining(limits, depth), count + 1, 3nat,
{
    if count == 0 { Some(ParsedEntries { values: Seq::empty(), used: 0, nodes, payload }) }
    else {
        match read_blob(bytes, limits.max_payload_bytes as nat) {
            None => None,
            Some((key_bytes, key_used)) => {
                if payload + key_bytes.len() > limits.max_payload_bytes
                    || (previous matches Some(old) && !lex_less(old, key_bytes)) { None } else {
                    match read_blob(tail(bytes, key_used), limits.max_payload_bytes as nat) {
                        None => None,
                        Some((value_bytes, value_used)) => {
                            let charged = payload + key_bytes.len() + value_bytes.len();
                            if charged > limits.max_payload_bytes { None } else {
                                match parse_complete(key_bytes, limits, depth, nodes, charged) {
                                    None => None,
                                    Some(key) => match parse_complete(value_bytes, limits, depth, key.nodes, key.payload) {
                                        None => None,
                                        Some(value) => match parse_entries(tail(bytes, key_used + value_used), limits, depth,
                                            (count - 1) as nat, Some(key_bytes), value.nodes, value.payload) {
                                            None => None,
                                            Some(rest) => Some(ParsedEntries { values: seq![(key.value, value.value)] + rest.values,
                                                used: key_used + value_used + rest.used, nodes: rest.nodes, payload: rest.payload }),
                                        },
                                    },
                                }
                            }
                        },
                    }
                }
            },
        }
    }
}

/// Complete frames perform the production validation and exact re-encoding check.
pub open spec fn parse_complete(bytes: Seq<u8>, limits: ModelLimits, depth: nat,
    nodes: nat, payload: nat) -> Option<Parsed>
    decreases depth_remaining(limits, depth), 0nat, 2nat,
{
    match parse_inner(bytes, limits, depth, nodes, payload) {
        None => None,
        Some(result) => if result.used == bytes.len() && wf(result.value, limits)
            && spec_encode(result.value) == bytes { Some(result) } else { None },
    }
}

pub open spec fn spec_decode(bytes: Seq<u8>, limits: ModelLimits) -> Option<(ModelValue, nat)> {
    if bytes.len() > limits.max_input_bytes { None } else {
        match parse_complete(bytes, value_limits(limits), 0, 0, 0) {
            None => None,
            Some(result) => Some((result.value, result.used)),
        }
    }
}

pub proof fn spec_t2_canonical(bytes: Seq<u8>, limits: ModelLimits)
    ensures spec_decode(bytes, limits) matches Some((value, used)) ==>
        wf(value, limits) && bytes == spec_encode(value) && used == bytes.len(),
{
    super::decoder_induction::complete_bounds(bytes, value_limits(limits), 0, 0, 0);
}

pub proof fn spec_t3_input_limit(bytes: Seq<u8>, limits: ModelLimits)
    ensures spec_decode(bytes, limits) is Some ==> bytes.len() <= limits.max_input_bytes,
{
}

pub proof fn spec_accepted_encoding_unique(left: Seq<u8>, right: Seq<u8>, limits: ModelLimits, value: ModelValue)
    requires
        spec_decode(left, limits) matches Some((v, _)) && v == value,
        spec_decode(right, limits) matches Some((v, _)) && v == value,
    ensures left == right,
{
    spec_t2_canonical(left, limits);
    spec_t2_canonical(right, limits);
}

}
