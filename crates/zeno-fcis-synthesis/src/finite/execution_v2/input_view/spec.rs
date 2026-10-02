//! Total mathematical closed schemas, byte projection and retained logical work.
use super::super::super::canonical_v2::spec as integer;
use super::Variant;
use vstd::prelude::*;

verus! {
pub open spec fn variants_prefix(variants: Seq<Variant>, min: i64, max: i64, count: nat)
    -> bool
    recommends count <= variants.len(),
    decreases count,
{
    if count == 0 { true }
    else {
        let last = variants[count as int - 1];
        variants_prefix(variants, min, max, (count - 1) as nat)
        && min <= last.code <= max
        && forall|j: int| 0 <= j < count - 1 ==> variants[j].id != last.id
            && variants[j].code != last.code
    }
}
pub open spec fn variants_valid(variants: Seq<Variant>, min: i64, max: i64) -> bool {
    min <= max && max as int - min as int + 1 == variants.len()
        && variants_prefix(variants, min, max, variants.len())
}
pub proof fn variants_failure(variants: Seq<Variant>, min: i64, max: i64,
    failed: nat, count: nat)
    requires failed <= count <= variants.len(),
        !variants_prefix(variants, min, max, failed),
    ensures !variants_prefix(variants, min, max, count),
    decreases count - failed,
{
    if count > failed {
        variants_failure(variants, min, max, failed, (count - 1) as nat);
    }
}
}

verus! {
pub open spec fn leaf_valid(leaf: super::Leaf) -> bool {
    match leaf {
        super::Leaf::I128 { min, max } => min <= max,
        super::Leaf::Bool => true,
        super::Leaf::Enum { min, max, variants, .. }
        | super::Leaf::Sum { min, max, variants, .. } => variants_valid(variants@, min, max),
    }
}
pub open spec fn schema_prefix(fields: Seq<super::Field>, count: nat) -> bool
    recommends count <= fields.len(),
    decreases count,
{
    if count == 0 { true }
    else {
        schema_prefix(fields, (count - 1) as nat)
            && leaf_valid(fields[count as int - 1].leaf)
            && (count < 2 || fields[count as int - 2].id < fields[count as int - 1].id)
    }
}
pub open spec fn schema_valid(fields: Seq<super::Field>) -> bool {
    schema_prefix(fields, fields.len())
}
pub proof fn schema_failure(fields: Seq<super::Field>, failed: nat, count: nat)
    requires failed <= count <= fields.len(), !schema_prefix(fields, failed),
    ensures !schema_prefix(fields, count),
    decreases count - failed,
{
    if count > failed { schema_failure(fields, failed, (count - 1) as nat); }
}
pub open spec fn lookup(variants: Seq<Variant>, id: u16, count: nat) -> Option<i64>
    recommends count <= variants.len(),
    decreases count,
{
    if count == 0 { None }
    else {
        match lookup(variants, id, (count - 1) as nat) {
            Some(code) => Some(code),
            None => if variants[count as int - 1].id == id {
                Some(variants[count as int - 1].code)
            } else { None },
        }
    }
}
pub proof fn lookup_persists(variants: Seq<Variant>, id: u16,
    found: nat, count: nat, code: i64)
    requires found <= count <= variants.len(), lookup(variants, id, found) == Some(code),
    ensures lookup(variants, id, count) == Some(code),
    decreases count - found,
{
    if count > found { lookup_persists(variants, id, found, (count - 1) as nat, code); }
}
}

verus! {
pub open spec fn variant_scalar(bytes: Seq<u8>, offset: usize,
    type_id: u32, variants: Seq<Variant>, sum: bool) -> Option<(i64, usize)> {
    match integer::read_unsigned(bytes, offset, 4) {
        None => None,
        Some((raw_type, after_type)) => if raw_type != type_id as u128 { None }
        else {
            match integer::read_unsigned(bytes, after_type, 2) {
                None => None,
                Some((raw_id, after_id)) => {
                    let end = if sum {
                        match integer::read_unsigned(bytes, after_id, 1) {
                            Some((0, after_payload)) => Some(after_payload),
                            _ => None,
                        }
                    } else { Some(after_id) };
                    match end {
                        None => None,
                        Some(end) => match lookup(variants, raw_id as u16, variants.len()) {
                            Some(code) => Some((code, end)), None => None,
                        },
                    }
                },
            }
        },
    }
}
pub open spec fn scalar(bytes: Seq<u8>, offset: usize, leaf: super::Leaf)
    -> Option<(i64, usize)> {
    match integer::read_unsigned(bytes, offset, 1) {
        None => None,
        Some((tag, payload_offset)) => match leaf {
            super::Leaf::I128 { min, max } => if tag != 0x04 { None }
            else {
                match integer::read_signed(bytes, payload_offset) {
                    Some((value, end)) => if min as i128 <= value <= max as i128 {
                        Some((value as i64, end))
                    } else { None },
                    None => None,
                }
            },
            super::Leaf::Bool => if tag == 0x01 { Some((0, payload_offset)) }
                else if tag == 0x02 { Some((1, payload_offset)) } else { None },
            super::Leaf::Enum { type_id, variants, .. } => if tag != 0x07 { None }
                else { variant_scalar(bytes, payload_offset, type_id, variants@, false) },
            super::Leaf::Sum { type_id, variants, .. } => if tag != 0x0a { None }
                else { variant_scalar(bytes, payload_offset, type_id, variants@, true) },
        },
    }
}
}

use super::super::Resource;
use super::super::spec as logical;
use super::{AccessAttempt, Failure, Field};

verus! {
pub open spec fn field(bytes: Seq<u8>, offset: usize, field: Field) -> Option<(i64, usize)> {
    match integer::read_unsigned(bytes, offset, 2) {
        None => None,
        Some((id, payload_offset)) => if id != field.id as u128 { None }
            else { scalar(bytes, payload_offset, field.leaf) },
    }
}
pub open spec fn header(bytes: Seq<u8>, count: usize) -> Option<usize> {
    match integer::read_unsigned(bytes, 0, 1) {
        Some((0x09, count_offset)) => match integer::read_unsigned(bytes, count_offset, 4) {
            Some((raw_count, fields_offset)) => if raw_count == count as u128 {
                Some(fields_offset)
            } else { None },
            None => None,
        },
        _ => None,
    }
}
pub(super) open spec fn prefix(bytes: Seq<u8>, fields: Seq<Field>, count: nat, offset: usize,
    limits: Seq<u64>, used: Seq<u64>)
    -> (Result<(Seq<i64>, usize), Failure>, Seq<u64>, Seq<AccessAttempt>)
    recommends count <= fields.len(), limits.len() == 8, used.len() == 8,
    decreases count,
{
    if count == 0 { (Ok((Seq::empty(), offset)), used, Seq::empty()) }
    else {
        let previous = prefix(bytes, fields, (count - 1) as nat, offset, limits, used);
        match previous.0 {
            Err(error) => previous,
            Ok((values, position)) => {
                let descriptor = fields[count as int - 1];
                let charged = logical::charge(limits, previous.1, Resource::Read, 1);
                let attempts = previous.2.push(AccessAttempt {
                    field_id: descriptor.id, permitted: charged.0.is_ok(),
                });
                match charged.0 {
                    Err(error) => (Err(Failure::Budget(error)), charged.1, attempts),
                    Ok(()) => match field(bytes, position, descriptor) {
                        None => (Err(Failure::Field(descriptor.id)), charged.1, attempts),
                        Some((value, end)) => (Ok((values.push(value), end)), charged.1, attempts),
                    },
                }
            },
        }
    }
}
pub(super) proof fn prefix_failure(bytes: Seq<u8>, fields: Seq<Field>, failed: nat, count: nat,
    offset: usize, limits: Seq<u64>, used: Seq<u64>, error: Failure)
    requires failed <= count <= fields.len(), limits.len() == 8, used.len() == 8,
        prefix(bytes, fields, failed, offset, limits, used).0 == Err(error),
    ensures prefix(bytes, fields, count, offset, limits, used)
        == prefix(bytes, fields, failed, offset, limits, used),
    decreases count - failed,
{
    if count > failed {
        prefix_failure(bytes, fields, failed, (count - 1) as nat, offset, limits, used, error);
    }
}
pub(super) open spec fn projection(bytes: Seq<u8>, fields: Seq<Field>, limits: Seq<u64>, used: Seq<u64>)
    -> (Result<Seq<i64>, Failure>, Seq<u64>, Seq<AccessAttempt>) {
    if !schema_valid(fields) { (Err(Failure::Schema), used, Seq::empty()) }
    else {
        let charged = logical::charge(limits, used, Resource::Byte, bytes.len() as u64);
        match charged.0 {
            Err(error) => (Err(Failure::Budget(error)), charged.1, Seq::empty()),
            Ok(()) => match header(bytes, fields.len() as usize) {
                None => (Err(Failure::Header), charged.1, Seq::empty()),
                Some(offset) => {
                    let projected = prefix(bytes, fields, fields.len(), offset, limits, charged.1);
                    let result = match projected.0 {
                        Err(error) => Err(error),
                        Ok((values, end)) => if end as int == bytes.len() {
                            Ok(values)
                        } else { Err(Failure::Trailing) },
                    };
                    (result, projected.1, projected.2)
                },
            },
        }
    }
}
}

// Typing lemmas derived from the actual closed wire and projection models.
verus! {
pub proof fn lookup_in_interval(variants: Seq<Variant>, min: i64, max: i64,
    id: u16, count: nat, code: i64)
    requires count <= variants.len(), variants_prefix(variants, min, max, count),
        lookup(variants, id, count) == Some(code),
    ensures min <= code <= max,
    decreases count,
{
    if count > 0 {
        match lookup(variants, id, (count - 1) as nat) {
            Some(previous) => {
                lookup_in_interval(variants, min, max, id, (count - 1) as nat, previous);
            },
            None => {},
        }
    }
}
pub proof fn scalar_has_declared_type(bytes: Seq<u8>, offset: usize,
    leaf: super::Leaf, value: i64, end: usize)
    requires leaf_valid(leaf), scalar(bytes, offset, leaf) == Some((value, end)),
    ensures match leaf {
        super::Leaf::Bool => 0 <= value <= 1,
        super::Leaf::I128 { min, max }
        | super::Leaf::Enum { min, max, .. }
        | super::Leaf::Sum { min, max, .. } => min <= value <= max,
    },
{
    match leaf {
        super::Leaf::Bool => {},
        super::Leaf::I128 { min, max } => {},
        super::Leaf::Enum { type_id, min, max, variants }
        | super::Leaf::Sum { type_id, min, max, variants } => {
            match integer::read_unsigned(bytes, offset, 1) {
                None => {},
                Some((tag, payload)) => {
                    match integer::read_unsigned(bytes, payload, 4) {
                        None => {},
                        Some((raw_type, after_type)) => {
                            match integer::read_unsigned(bytes, after_type, 2) {
                                None => {},
                                Some((raw_id, after_id)) => {
                                    lookup_in_interval(variants@, min, max,
                                        raw_id as u16, variants@.len(), value);
                                },
                            }
                        },
                    }
                },
            }
        },
    }
}
}

verus! {
pub open spec fn leaf_contains(leaf: super::Leaf, value: i64) -> bool {
    match leaf {
        super::Leaf::Bool => 0 <= value <= 1,
        super::Leaf::I128 { min, max }
        | super::Leaf::Enum { min, max, .. }
        | super::Leaf::Sum { min, max, .. } => min <= value <= max,
    }
}
pub(super) proof fn prefix_is_typed(bytes: Seq<u8>, fields: Seq<Field>, count: nat,
    offset: usize, limits: Seq<u64>, used: Seq<u64>, values: Seq<i64>, end: usize)
    requires count <= fields.len(), schema_prefix(fields, count),
        limits.len() == 8, used.len() == 8,
        prefix(bytes, fields, count, offset, limits, used).0 == Ok((values, end)),
    ensures values.len() == count,
        forall|j: int| 0 <= j < count ==> leaf_contains(fields[j].leaf, values[j]),
    decreases count,
{
    if count > 0 {
        let previous = prefix(bytes, fields, (count - 1) as nat, offset, limits, used);
        match previous.0 {
            Err(error) => {},
            Ok((prior, position)) => {
                prefix_is_typed(bytes, fields, (count - 1) as nat, offset,
                    limits, used, prior, position);
                let descriptor = fields[count as int - 1];
                match field(bytes, position, descriptor) {
                    None => {},
                    Some((value, field_end)) => {
                        match integer::read_unsigned(bytes, position, 2) {
                            None => {},
                            Some((id, payload)) => {
                                scalar_has_declared_type(bytes, payload,
                                    descriptor.leaf, value, field_end);
                            },
                        }
                        assert(values == prior.push(value));
                        assert forall|j: int| 0 <= j < count implies
                            leaf_contains(fields[j].leaf, values[j]) by {
                            if j < count - 1 {
                                assert(leaf_contains(fields[j].leaf, prior[j]));
                            }
                        }
                    },
                }
            },
        }
    }
}
pub(super) proof fn projection_is_typed(bytes: Seq<u8>, fields: Seq<Field>,
    limits: Seq<u64>, used: Seq<u64>, values: Seq<i64>)
    requires limits.len() == 8, used.len() == 8,
        projection(bytes, fields, limits, used).0 == Ok(values),
    ensures values.len() == fields.len(),
        forall|j: int| 0 <= j < fields.len() ==> leaf_contains(fields[j].leaf, values[j]),
{
    let charged = logical::charge(limits, used, Resource::Byte, bytes.len() as u64);
    match header(bytes, fields.len() as usize) {
        None => {},
        Some(offset) => {
            let projected = prefix(bytes, fields, fields.len(), offset, limits, charged.1);
            match projected.0 {
                Err(error) => {},
                Ok((output, end)) => {
                    prefix_is_typed(bytes, fields, fields.len(), offset,
                        limits, charged.1, output, end);
                },
            }
        },
    }
}
}
