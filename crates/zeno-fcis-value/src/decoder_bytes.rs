//! Fixed-width byte operations shared by canonical value encoding and decoding.
extern crate alloc;
#[cfg(verus_keep_ghost)]
use crate::decoder_model::{big_endian, signed_bits};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Appends the low `width` bytes, most significant byte first.
#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures final(output)@ == old(output)@ + big_endian(word, width as nat),
    decreases width,
))]
pub(super) fn append_word(output: &mut Vec<u8>, word: u128, width: usize) {
    if width != 0 {
        append_word(output, word >> 8, width - 1);
        output.push((word % 256) as u8);
    }
}

/// Returns the two's-complement bit pattern of a signed value.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == signed_bits(word),))]
pub(super) const fn signed_to_bits(word: i128) -> u128 {
    if word < 0 {
        u128::MAX - (-(word + 1)) as u128
    } else {
        word as u128
    }
}

/// Compares canonical byte strings in strict lexicographic order.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == crate::decoder_model::lex_less(left@, right@),
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
            crate::decoder_model::lex_less(left@, right@)
                == crate::decoder_model::lex_less(
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
