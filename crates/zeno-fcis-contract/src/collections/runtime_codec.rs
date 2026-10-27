//! Integer byte definitions and constructive codec lemmas.

use vstd::prelude::*;
verus! {
pub open spec fn byte_at(number: i128, index: int) -> u8 {
    ((number as u128) >> (((15 - index) * 8) as u32)) as u8
}
pub open spec fn signed_bytes(number: i128) -> Seq<u8> {
    Seq::new(16, |index: int| byte_at(number, index))
}
pub open spec fn bits(bytes: Seq<u8>) -> u128
    decreases bytes.len(),
{
    if bytes.len() == 0 { 0 }
    else { (bits(bytes.drop_last()) << 8) | (bytes.last() as u128) }
}
pub proof fn shifted_byte(prefix: u128, byte: u8, shift: u32)
    requires shift <= 112,
    ensures ((((prefix << 8) | (byte as u128)) >> (shift + 8)) as u8) == ((prefix >> shift) as u8),
{
    assert(((((prefix << 8) | (byte as u128)) >> (shift + 8)) as u8) == ((prefix >> shift) as u8)) by(bit_vector)
        requires shift <= 112;
}
pub proof fn low_byte(prefix:u128, byte:u8)
    ensures (((prefix << 8) | (byte as u128)) as u8) == byte,
{
    assert((((prefix << 8) | (byte as u128)) as u8) == byte) by(bit_vector);
}
pub proof fn bits_contains(bytes:Seq<u8>)
    requires bytes.len() <= 16,
    ensures forall|index:int| 0 <= index < bytes.len() ==>
        ((bits(bytes) >> (((bytes.len() - 1 - index) * 8) as u32)) as u8) == #[trigger] bytes[index],
    decreases bytes.len(),
{
    if bytes.len() > 0 {
        let prefix = bytes.drop_last();
        assert(bits(bytes) == (bits(prefix) << 8) | (bytes.last() as u128));
        bits_contains(prefix);
        assert forall|index:int| 0 <= index < bytes.len() implies
            ((bits(bytes) >> (((bytes.len() - 1 - index) * 8) as u32)) as u8) == bytes[index] by {
            if index == bytes.len() - 1 {
                low_byte(bits(prefix), bytes.last());
                assert(bytes.last() == bytes[index]);
                assert(((bytes.len() - 1 - index) * 8) as u32 == 0);
                let value = bits(bytes);
                assert(value >> 0 == value) by(bit_vector);
            } else {
                let shift = ((prefix.len() - 1 - index) * 8) as u32;
                assert(shift <= 112);
                shifted_byte(bits(prefix), bytes.last(), shift);
                assert((((bytes.len() - 1 - index) * 8) as u32) == shift + 8);
                assert(((bits(prefix) >> shift) as u8) == prefix[index]);
                assert(prefix[index] == bytes[index]);
            }
        }
    }
}
pub proof fn decode_encode(bytes:Seq<u8>)
    requires bytes.len() == 16,
    ensures signed_bytes(bits(bytes) as i128) == bytes,
{
    bits_contains(bytes);
    let value = bits(bytes);
    assert((value as i128) as u128 == value) by(bit_vector);
    assert(signed_bytes(value as i128) =~= bytes);
}
pub proof fn append_byte(number:u128, shift:u32)
    requires shift <= 112,
    ensures ((number >> (shift + 8)) << 8) | (((number >> shift) as u8) as u128) == number >> shift,
{
    assert(((number >> (shift + 8)) << 8) | (((number >> shift) as u8) as u128) == number >> shift) by(bit_vector)
        requires shift <= 112;
}
pub proof fn encoded_prefix(number:i128, count:nat)
    requires count <= 16,
    ensures bits(signed_bytes(number).take(count as int)) ==
        if count == 0 { 0u128 } else { (number as u128) >> (((16 - count) * 8) as u32) },
    decreases count,
{
    if count > 0 {
        encoded_prefix(number, (count - 1) as nat);
        let all = signed_bytes(number);
        let prefix = all.take(count as int);
        assert(prefix.drop_last() =~= all.take(count as int - 1));
        assert(prefix.last() == byte_at(number, count as int - 1));
        assert(bits(prefix) == (bits(all.take(count as int - 1)) << 8) | (prefix.last() as u128));
        let unsigned = number as u128;
        if count == 1 {
            assert(((unsigned >> 120) as u8) as u128 == unsigned >> 120) by(bit_vector);
            let byte = prefix.last();
            assert(((0u128 << 8) | (byte as u128)) == byte as u128) by(bit_vector);
            assert(bits(all.take(0)) == 0);
            assert(byte == (unsigned >> 120) as u8);
        } else {
            let shift = ((16 - count) * 8) as u32;
            assert((((16 - (count - 1)) * 8) as u32) == shift + 8);
            assert(bits(all.take(count as int - 1)) == unsigned >> (shift + 8));
            append_byte(unsigned, shift);
            assert(prefix.last() == (unsigned >> shift) as u8);
        }
    }
}
pub proof fn encode_decode(number:i128)
    ensures bits(signed_bytes(number)) as i128 == number,
{
    encoded_prefix(number, 16);
    assert(signed_bytes(number).take(16) =~= signed_bytes(number));
    assert(bits(signed_bytes(number)) == (number as u128) >> 0);
    assert((number as u128) >> 0 == number as u128) by(bit_vector);
    assert((number as u128) as i128 == number) by(bit_vector);
}
}
