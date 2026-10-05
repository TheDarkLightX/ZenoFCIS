//! Mathematical original-wire sequences and ordered refusal, erased in Rust.
use super::{Atom, Failure, Field};
use vstd::prelude::*;
verus! {
pub open spec fn be2(value: u16) -> Seq<u8> { seq![(value >> 8u32) as u8, (value >> 0u32) as u8] }
pub open spec fn be4(value: u32) -> Seq<u8> { seq![(value >> 24u32) as u8, (value >> 16u32) as u8, (value >> 8u32) as u8, (value >> 0u32) as u8] }
pub open spec fn be16(value: u128) -> Seq<u8> { seq![(value >> 120u32) as u8, (value >> 112u32) as u8, (value >> 104u32) as u8, (value >> 96u32) as u8, (value >> 88u32) as u8, (value >> 80u32) as u8, (value >> 72u32) as u8, (value >> 64u32) as u8, (value >> 56u32) as u8, (value >> 48u32) as u8, (value >> 40u32) as u8, (value >> 32u32) as u8, (value >> 24u32) as u8, (value >> 16u32) as u8, (value >> 8u32) as u8, (value >> 0u32) as u8] }

pub open spec fn ascii(bytes: Seq<u8>) -> bool {
    forall|i: int| 0 <= i < bytes.len() ==> bytes[i] <= 127
}
pub open spec fn atom_size(value: Atom) -> Result<usize, Failure> {
    match value {

        Atom::Bool(_) => Ok(1),
        Atom::I128(_) | Atom::U128(_) => Ok(17),
        Atom::Enum { .. } => Ok(7),
        Atom::Sum { .. } => Ok(8),
        Atom::Bytes(b) | Atom::Text(b) => {
            if b@.len() > u32::MAX { Err(Failure::Length) }
            else if value is Text && !ascii(b@) { Err(Failure::Text) }
            else if b@.len() + 5 > usize::MAX { Err(Failure::Length) }
            else { Ok((b@.len() + 5) as usize) }
        }
    }
}
pub open spec fn atom_wire(value: Atom) -> Seq<u8> {
    match value {

        Atom::Bool(v) => seq![if v { 0x02u8 } else { 0x01u8 }],
        Atom::I128(v) => seq![0x04u8] + be16(v as u128),
        Atom::U128(v) => seq![0x03u8] + be16(v),
        Atom::Enum { type_id, variant } => seq![0x07u8] + be4(type_id) + be2(variant),
        Atom::Sum { type_id, variant } => seq![0x0au8] + be4(type_id) + be2(variant) + seq![0u8],
        Atom::Bytes(b) => seq![0x05u8] + be4(b@.len() as u32) + b@,
        Atom::Text(b) => seq![0x06u8] + be4(b@.len() as u32) + b@,
    }
}
pub open spec fn atom_result(value: Atom, max_bytes: usize) -> Result<Seq<u8>, Failure> {
    match atom_size(value) {
        Err(e) => Err(e),
        Ok(n) => if n > max_bytes { Err(Failure::Limit) } else { Ok(atom_wire(value)) },
    }
}
pub open spec fn record_result(fields: Seq<Field>, max_bytes: usize, n: nat) -> Result<Seq<u8>, Failure>
    recommends n <= fields.len(), decreases n,
{
    if fields.len() > u32::MAX { Err(Failure::Length) }
    else if max_bytes < 5 { Err(Failure::Limit) }
    else if n == 0 { Ok(seq![0x09u8] + be4(fields.len() as u32)) }
    else { match record_result(fields, max_bytes, (n - 1) as nat) {
        Err(e) => Err(e),
        Ok(prior) => {
            let f = fields[n - 1];
            if n > 1 && fields[n - 2].id >= f.id { Err(Failure::Order) }
            else if max_bytes - prior.len() < 2 { Err(Failure::Limit) }
            else { match atom_result(f.value, (max_bytes - prior.len() - 2) as usize) {
                Err(e) => Err(e), Ok(b) => Ok(prior + be2(f.id) + b),
            } }
        }
    } }
}
pub proof fn record_initial_failure(fields: Seq<Field>, cap: usize, n: nat)
    requires n <= fields.len(), fields.len() > u32::MAX || cap < 5,
    ensures record_result(fields, cap, n) == if fields.len() > u32::MAX { Err(Failure::Length) } else { Err(Failure::Limit) },
{}
pub proof fn record_failure(fields: Seq<Field>, cap: usize, n: nat, total: nat)
    requires n <= total <= fields.len(), record_result(fields, cap, n).is_err(),
    ensures record_result(fields, cap, total) == record_result(fields, cap, n),
    decreases total - n,
{
    if total > n { record_failure(fields, cap, n, (total - 1) as nat); }
}
pub open spec fn envelope_wire(root: u32, schema: Seq<u8>, payload: Seq<u8>) -> Seq<u8> {
    seq![90u8,70u8,67u8,73u8,83u8,86u8,49u8,0u8] + be4(root) + schema + be4(payload.len() as u32) + payload
}
pub open spec fn envelope_result(root: u32, schema: Seq<u8>, payload: Seq<u8>, cap: usize) -> Result<Seq<u8>, Failure> {
    if payload.len() > u32::MAX || payload.len() + 48 > usize::MAX { Err(Failure::Length) }
    else if payload.len() + 48 > cap { Err(Failure::Limit) }
    else { Ok(envelope_wire(root, schema, payload)) }
}

/// Exact bridge to the actual existing unsigned-reader specification.
pub proof fn read_be2(value: u16)
    ensures crate::finite::canonical_v2::spec::read_unsigned(be2(value), 0, 2) == Some((value as u128, 2usize)),
{
    assert(value == (((value >> 8u32) as u8 as u16) * 256u16 + ((value >> 0u32) as u8 as u16) * 1u16) as u16) by (bit_vector);
    assert(0 <= ((value >> 8u32) as u8 as int) * 256 + ((value >> 0u32) as u8 as int) * 1 <= u16::MAX);
    reveal_with_fuel(crate::finite::canonical_v2::spec::unsigned, 3);
}
/// Exact bridge to the actual existing unsigned-reader specification.
pub proof fn read_be4(value: u32)
    ensures crate::finite::canonical_v2::spec::read_unsigned(be4(value), 0, 4) == Some((value as u128, 4usize)),
{
    assert(value == (((value >> 24u32) as u8 as u32) * 16777216u32 + ((value >> 16u32) as u8 as u32) * 65536u32 + ((value >> 8u32) as u8 as u32) * 256u32 + ((value >> 0u32) as u8 as u32) * 1u32) as u32) by (bit_vector);
    assert(0 <= ((value >> 24u32) as u8 as int) * 16777216 + ((value >> 16u32) as u8 as int) * 65536 + ((value >> 8u32) as u8 as int) * 256 + ((value >> 0u32) as u8 as int) * 1 <= u32::MAX);
    reveal_with_fuel(crate::finite::canonical_v2::spec::unsigned, 5);
}
/// Exact bridge to the actual existing unsigned-reader specification.
pub proof fn read_be16(value: u128)
    ensures crate::finite::canonical_v2::spec::read_unsigned(be16(value), 0, 16) == Some((value as u128, 16usize)),
{
    assert(value == (((value >> 120u32) as u8 as u128) * 1329227995784915872903807060280344576u128 + ((value >> 112u32) as u8 as u128) * 5192296858534827628530496329220096u128 + ((value >> 104u32) as u8 as u128) * 20282409603651670423947251286016u128 + ((value >> 96u32) as u8 as u128) * 79228162514264337593543950336u128 + ((value >> 88u32) as u8 as u128) * 309485009821345068724781056u128 + ((value >> 80u32) as u8 as u128) * 1208925819614629174706176u128 + ((value >> 72u32) as u8 as u128) * 4722366482869645213696u128 + ((value >> 64u32) as u8 as u128) * 18446744073709551616u128 + ((value >> 56u32) as u8 as u128) * 72057594037927936u128 + ((value >> 48u32) as u8 as u128) * 281474976710656u128 + ((value >> 40u32) as u8 as u128) * 1099511627776u128 + ((value >> 32u32) as u8 as u128) * 4294967296u128 + ((value >> 24u32) as u8 as u128) * 16777216u128 + ((value >> 16u32) as u8 as u128) * 65536u128 + ((value >> 8u32) as u8 as u128) * 256u128 + ((value >> 0u32) as u8 as u128) * 1u128) as u128) by (bit_vector);
    assert(0 <= ((value >> 120u32) as u8 as int) * 1329227995784915872903807060280344576 + ((value >> 112u32) as u8 as int) * 5192296858534827628530496329220096 + ((value >> 104u32) as u8 as int) * 20282409603651670423947251286016 + ((value >> 96u32) as u8 as int) * 79228162514264337593543950336 + ((value >> 88u32) as u8 as int) * 309485009821345068724781056 + ((value >> 80u32) as u8 as int) * 1208925819614629174706176 + ((value >> 72u32) as u8 as int) * 4722366482869645213696 + ((value >> 64u32) as u8 as int) * 18446744073709551616 + ((value >> 56u32) as u8 as int) * 72057594037927936 + ((value >> 48u32) as u8 as int) * 281474976710656 + ((value >> 40u32) as u8 as int) * 1099511627776 + ((value >> 32u32) as u8 as int) * 4294967296 + ((value >> 24u32) as u8 as int) * 16777216 + ((value >> 16u32) as u8 as int) * 65536 + ((value >> 8u32) as u8 as int) * 256 + ((value >> 0u32) as u8 as int) * 1 <= u128::MAX);
    reveal_with_fuel(crate::finite::canonical_v2::spec::unsigned, 17);
}

/// Reading a window uses the actual reader definition, not a copied decoder.
pub proof fn unsigned_window(bytes: Seq<u8>, offset: int, n: nat)
    requires 0 <= offset, offset + n <= bytes.len(),
    ensures crate::finite::canonical_v2::spec::unsigned(bytes, offset, n)
        == crate::finite::canonical_v2::spec::unsigned(bytes.subrange(offset, offset + n), 0, n),
    decreases n,
{
    if n > 0 {
        unsigned_window(bytes, offset, (n - 1) as nat);
        unsigned_window(bytes.subrange(offset, offset + n), 0, (n - 1) as nat);
        assert(bytes.subrange(offset, offset + n).subrange(0, n - 1)
            =~= bytes.subrange(offset, offset + n - 1));
    }
}
/// All 128 unsigned bits round-trip through the existing canonical reader.
pub proof fn unsigned_atom_round_trip(value: u128)
    ensures crate::finite::canonical_v2::spec::read_unsigned(atom_wire(Atom::U128(value)), 1, 16)
        == Some((value, 17usize)),
{
    read_be16(value);
    let bytes = atom_wire(Atom::U128(value));
    unsigned_window(bytes, 1, 16);
    assert(bytes.subrange(1, 17) =~= be16(value));
}
/// Two's-complement signed extremes round-trip through the actual signed reader.
pub proof fn signed_atom_round_trip(value: i128)
    ensures crate::finite::canonical_v2::spec::read_signed(atom_wire(Atom::I128(value)), 1)
        == Some((value, 17usize)),
{
    read_be16(value as u128);
    let bytes = atom_wire(Atom::I128(value));
    unsigned_window(bytes, 1, 16);
    assert(bytes.subrange(1, 17) =~= be16(value as u128));
    assert((value as u128) as i128 == value) by (bit_vector);
    assert(value >= 0 ==> value as u128 <= i128::MAX as u128) by (bit_vector);
    assert(value < 0 ==> value as u128 > i128::MAX as u128) by (bit_vector);
    assert(value < 0 ==> (u128::MAX - value as u128) as u128 == (-(value + 1)) as u128) by (bit_vector);
    if value < 0 {
        assert(u128::MAX as int - (value as u128) as int == -(value as int + 1));
    }

}
/// Every successful encoded frame has the exact original framing result.
/// Existing frame() has this same specification as its executable contract.
pub proof fn envelope_round_trip(root: u32, schema: Seq<u8>, payload: Seq<u8>, cap: usize)
    requires schema.len() == 32,
    ensures match envelope_result(root, schema, payload, cap) {
        Ok(bytes) => crate::finite::canonical_v2::envelope::spec::frame(bytes, root, schema, cap as u64)
            == Ok((bytes, payload)),
        Err(_) => true,
    },
{
    if payload.len() <= u32::MAX && payload.len() + 48 <= cap {
        let bytes = envelope_wire(root, schema, payload);
        read_be4(root);
        read_be4(payload.len() as u32);
        unsigned_window(bytes, 8, 4);
        unsigned_window(bytes, 44, 4);
        assert(bytes.subrange(8, 12) =~= be4(root));
        assert(bytes.subrange(44, 48) =~= be4(payload.len() as u32));
        assert(bytes.subrange(48, bytes.len() as int) =~= payload);
        assert(crate::finite::canonical_v2::envelope::spec::matches(bytes, 0,
            crate::finite::canonical_v2::envelope::spec::magic()));
        assert(crate::finite::canonical_v2::envelope::spec::matches(bytes, 12, schema));
    }
}
/// Complete original record bytes, independent of refusal and allocation.
pub open spec fn field_wire(fields: Seq<Field>, n: nat) -> Seq<u8>
    recommends n <= fields.len(), decreases n,
{
    if n == 0 { Seq::empty() }
    else { field_wire(fields, (n - 1) as nat) + be2(fields[n - 1].id) + atom_wire(fields[n - 1].value) }
}
pub proof fn record_exact(fields: Seq<Field>, cap: usize, n: nat)
    requires n <= fields.len(),
    ensures match record_result(fields, cap, n) {
        Ok(bytes) => bytes == seq![0x09u8] + be4(fields.len() as u32) + field_wire(fields, n)
            && (forall|i: int| 0 < i < n ==> fields[i - 1].id < #[trigger] fields[i].id),
        Err(_) => true,
    },
    decreases n,
{
    if n > 0 { record_exact(fields, cap, (n - 1) as nat); }
}
}
