//! Fixed-width big-endian value readers.
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use zeno_fcis_value::decoder_model::{signed_word, unsigned_bytes};

/// Reads a byte slice in most-significant-byte-first order.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == unsigned_bytes(bytes@),
))]
pub(super) fn read_word(bytes: &[u8]) -> u128 {
    let mut index = 0;
    let mut word = 0_u128;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= bytes.len(),
            word == unsigned_bytes(bytes@.subrange(0, index as int)),
        decreases bytes.len() - index,
    ))]
    while index < bytes.len() {
        #[cfg(verus_keep_ghost)]
        proof! {
            let prefix = bytes@.subrange(0, index + 1);
            assert(prefix.drop_last() == bytes@.subrange(0, index as int));
            assert(prefix.last() == bytes@[index as int]);
        }
        word = (word << 8) | u128::from(bytes[index]);
        index += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(bytes@.subrange(0, bytes@.len() as int) == bytes@); }
    word
}

/// Interprets a 128-bit pattern as a two's-complement signed value.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == signed_word(bits),))]
pub(super) const fn bits_to_signed(bits: u128) -> i128 {
    if bits <= i128::MAX as u128 {
        bits as i128
    } else {
        -((u128::MAX - bits) as i128) - 1
    }
}

/// Compares canonical byte strings in strict lexicographic order.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == zeno_fcis_value::decoder_model::lex_less(left@, right@),
))]
pub(super) fn bytes_before(left: &[u8], right: &[u8]) -> bool {
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(left@.subrange(0, left@.len() as int) == left@);
        assert(right@.subrange(0, right@.len() as int) == right@);
    }
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= left.len(), index <= right.len(),
            zeno_fcis_value::decoder_model::lex_less(left@, right@)
                == zeno_fcis_value::decoder_model::lex_less(
                    left@.subrange(index as int, left@.len() as int),
                    right@.subrange(index as int, right@.len() as int)),
        decreases left.len() - index,
    ))]
    while index < left.len() && index < right.len() {
        #[cfg(verus_keep_ghost)]
        proof! {
            let l = left@.subrange(index as int, left@.len() as int);
            let r = right@.subrange(index as int, right@.len() as int);
            assert(l[0] == left@[index as int] && r[0] == right@[index as int]);
            assert(l.drop_first() == left@.subrange(index + 1, left@.len() as int));
            assert(r.drop_first() == right@.subrange(index + 1, right@.len() as int));
        }
        if left[index] < right[index] {
            return true;
        }
        if left[index] > right[index] {
            return false;
        }
        index += 1;
    }
    left.len() < right.len()
}
