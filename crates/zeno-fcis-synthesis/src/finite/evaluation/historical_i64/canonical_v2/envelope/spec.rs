//! Closed frame identity and custody over the received sequence.
use super::Failure;
use vstd::prelude::*;

verus! {
pub open spec fn matches(bytes: Seq<u8>, offset: usize, expected: Seq<u8>) -> bool {
    offset <= bytes.len() && expected.len() <= bytes.len() - offset
        && forall|j: int| 0 <= j < expected.len() ==>
            bytes[offset as int + j] == expected[j]
}

pub open spec fn magic() -> Seq<u8> {
    seq![90u8, 70u8, 67u8, 73u8, 83u8, 86u8, 49u8, 0u8]
}

pub open spec fn frame(bytes: Seq<u8>, root_type: u32, schema_hash: Seq<u8>,
    max_bytes: u64) -> Result<(Seq<u8>, Seq<u8>), Failure>
{
    if bytes.len() as u64 > max_bytes { Err(Failure::Size) }
    else if bytes.len() < 48 { Err(Failure::Short) }
    else if !matches(bytes, 0, magic()) { Err(Failure::Magic) }
    else if super::super::spec::read_unsigned(bytes, 8, 4).unwrap().0 != root_type as u128 {
        Err(Failure::Root)
    }
    else if !matches(bytes, 12, schema_hash) { Err(Failure::Schema) }
    else if super::super::spec::read_unsigned(bytes, 44, 4).unwrap().0
        != (bytes.len() - 48) as u128 { Err(Failure::Length) }
    else { Ok((bytes, bytes.subrange(48, bytes.len() as int))) }
}
}
