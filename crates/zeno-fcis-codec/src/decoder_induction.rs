//! Depth induction for the total specification's consumption and resource bounds.
use super::decoder_spec::*;
use vstd::prelude::*;
use zeno_fcis_value::decoder_model::*;

verus! {

pub proof fn inner_bounds(bytes: Seq<u8>, limits: ModelLimits, depth: nat, nodes: nat, payload: nat)
    requires nodes <= limits.max_nodes, payload <= limits.max_payload_bytes,
    ensures parse_inner(bytes, limits, depth, nodes, payload) matches Some(p) ==>
        0 < p.used <= bytes.len() && nodes < p.nodes <= limits.max_nodes
        && payload <= p.payload <= limits.max_payload_bytes,
    decreases depth_remaining(limits, depth), 0nat, 1nat,
{
    if depth <= depth_limit(limits) && nodes < limits.max_nodes && bytes.len() > 0 {
        let tag = bytes[0];
        if (tag == 8 || tag == 11) && bytes.len() >= 5 {
            let count = unsigned_bytes(bytes.subrange(1, 5)) as nat;
            if count <= limits.max_collection_len {
                items_bounds(tail(bytes, 5), limits, depth + 1, count, nodes + 1, payload);
            }
        } else if tag == 9 && bytes.len() >= 5 {
            let count = unsigned_bytes(bytes.subrange(1, 5)) as nat;
            if count <= limits.max_collection_len {
                fields_bounds(tail(bytes, 5), limits, depth + 1, count, None, nodes + 1, payload);
            }
        } else if tag == 10 && bytes.len() >= 8 && bytes[7] == 1 {
            inner_bounds(tail(bytes, 8), limits, depth + 1, nodes + 1, payload);
        } else if tag == 12 && bytes.len() >= 5 {
            let count = unsigned_bytes(bytes.subrange(1, 5)) as nat;
            if count <= limits.max_collection_len {
                entries_bounds(tail(bytes, 5), limits, depth + 1, count, None, nodes + 1, payload);
            }
        }
    }
}

pub proof fn complete_bounds(bytes: Seq<u8>, limits: ModelLimits, depth: nat, nodes: nat, payload: nat)
    requires nodes <= limits.max_nodes, payload <= limits.max_payload_bytes,
    ensures parse_complete(bytes, limits, depth, nodes, payload) matches Some(p) ==>
        p.used == bytes.len() && 0 < p.used && nodes < p.nodes <= limits.max_nodes
        && payload <= p.payload <= limits.max_payload_bytes
        && wf(p.value, limits) && spec_encode(p.value) == bytes,
    decreases depth_remaining(limits, depth), 0nat, 2nat,
{
    inner_bounds(bytes, limits, depth, nodes, payload);
}

pub proof fn items_bounds(bytes: Seq<u8>, limits: ModelLimits, depth: nat,
    count: nat, nodes: nat, payload: nat)
    requires nodes <= limits.max_nodes, payload <= limits.max_payload_bytes,
    ensures parse_items(bytes, limits, depth, count, nodes, payload) matches Some(p) ==>
        p.used <= bytes.len() && nodes <= p.nodes <= limits.max_nodes
        && payload <= p.payload <= limits.max_payload_bytes && p.values.len() == count,
    decreases depth_remaining(limits, depth), count + 1, 3nat,
{
    if count > 0 {
        inner_bounds(bytes, limits, depth, nodes, payload);
        if let Some(head) = parse_inner(bytes, limits, depth, nodes, payload) {
            items_bounds(tail(bytes, head.used), limits, depth, (count - 1) as nat, head.nodes, head.payload);
        }
    }
}

pub proof fn fields_bounds(bytes: Seq<u8>, limits: ModelLimits, depth: nat,
    count: nat, previous: Option<u16>, nodes: nat, payload: nat)
    requires nodes <= limits.max_nodes, payload <= limits.max_payload_bytes,
    ensures parse_fields(bytes, limits, depth, count, previous, nodes, payload) matches Some(p) ==>
        p.used <= bytes.len() && nodes <= p.nodes <= limits.max_nodes
        && payload <= p.payload <= limits.max_payload_bytes && p.values.len() == count,
    decreases depth_remaining(limits, depth), count + 1, 3nat,
{
    if count > 0 && bytes.len() >= 2 {
        let id = unsigned_bytes(bytes.subrange(0, 2)) as u16;
        if !(previous matches Some(old) && old >= id) {
            inner_bounds(tail(bytes, 2), limits, depth, nodes, payload);
            if let Some(head) = parse_inner(tail(bytes, 2), limits, depth, nodes, payload) {
                fields_bounds(tail(bytes, 2 + head.used), limits, depth, (count - 1) as nat,
                    Some(id), head.nodes, head.payload);
            }
        }
    }
}

pub proof fn entries_bounds(bytes: Seq<u8>, limits: ModelLimits, depth: nat,
    count: nat, previous: Option<Seq<u8>>, nodes: nat, payload: nat)
    requires nodes <= limits.max_nodes, payload <= limits.max_payload_bytes,
    ensures parse_entries(bytes, limits, depth, count, previous, nodes, payload) matches Some(p) ==>
        p.used <= bytes.len() && nodes <= p.nodes <= limits.max_nodes
        && payload <= p.payload <= limits.max_payload_bytes && p.values.len() == count,
    decreases depth_remaining(limits, depth), count + 1, 3nat,
{
    if count > 0 {
        if let Some((key_bytes, key_used)) = read_blob(bytes, limits.max_payload_bytes as nat) {
            if payload + key_bytes.len() <= limits.max_payload_bytes
                && !(previous matches Some(old) && !lex_less(old, key_bytes)) {
                if let Some((value_bytes, value_used)) = read_blob(tail(bytes, key_used), limits.max_payload_bytes as nat) {
                    let charged = payload + key_bytes.len() + value_bytes.len();
                    if charged <= limits.max_payload_bytes {
                        complete_bounds(key_bytes, limits, depth, nodes, charged);
                        if let Some(key) = parse_complete(key_bytes, limits, depth, nodes, charged) {
                            complete_bounds(value_bytes, limits, depth, key.nodes, key.payload);
                            if let Some(value) = parse_complete(value_bytes, limits, depth, key.nodes, key.payload) {
                                entries_bounds(tail(bytes, key_used + value_used), limits, depth,
                                    (count - 1) as nat, Some(key_bytes), value.nodes, value.payload);
                            }
                        }
                    }
                }
            }
        }
    }
}

}
