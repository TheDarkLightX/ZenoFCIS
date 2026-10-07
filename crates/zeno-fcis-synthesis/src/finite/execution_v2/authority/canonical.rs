//! Prefix-free canonical words and byte strings; allocator failure is outside the model.
#[cfg(verus_keep_ghost)]
use super::spec as model;
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// A canonical unsigned word or an exact byte string. Not an authorizing witness.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub enum Part<'a> {
    /// A full-width unsigned value, encoded in big-endian order.
    Word(u128),
    /// A counted exact byte string.
    Bytes(&'a [u8]),
}

/// Computes the complete encoded size, refusing representational overflow.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == model::size(parts@),))]
pub fn encoded_size(parts: &[Part<'_>]) -> Option<usize> {
    let mut size = 0usize;
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= parts.len(),
        size as nat == model::length(parts@,i as nat), decreases parts.len()-i,))]
    while i < parts.len() {
        let Some(header) = size.checked_add(17) else {
            #[cfg(verus_keep_ghost)]
            proof! { model::length_monotone(parts@,(i+1) as nat,parts@.len()); }
            return None;
        };
        size = match parts[i] {
            Part::Word(_) => header,
            Part::Bytes(bytes) => match header.checked_add(bytes.len()) {
                Some(next) => next,
                None => {
                    #[cfg(verus_keep_ghost)]
                    proof! { model::length_monotone(parts@,(i+1) as nat,parts@.len()); }
                    return None;
                }
            },
        };
        i += 1;
    }
    Some(size)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == model::word(value),))]
fn word(value: u128) -> Vec<u8> {
    #[cfg(verus_keep_ghost)]
    proof! {assert((value >> 0u32) == value) by(bit_vector);}
    alloc::vec![
        (value >> 120u32) as u8,
        (value >> 112u32) as u8,
        (value >> 104u32) as u8,
        (value >> 96u32) as u8,
        (value >> 88u32) as u8,
        (value >> 80u32) as u8,
        (value >> 72u32) as u8,
        (value >> 64u32) as u8,
        (value >> 56u32) as u8,
        (value >> 48u32) as u8,
        (value >> 40u32) as u8,
        (value >> 32u32) as u8,
        (value >> 24u32) as u8,
        (value >> 16u32) as u8,
        (value >> 8u32) as u8,
        value as u8,
    ]
}

#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(out)@ == old(out)@ + bytes@,))]
fn append(out: &mut Vec<u8>, bytes: &[u8]) {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=bytes.len(),
        out@ == old(out)@ + bytes@.take(i as int), decreases bytes.len()-i,))]
    while i < bytes.len() {
        out.push(bytes[i]);
        i += 1;
        #[cfg(verus_keep_ghost)]
        proof! {assert(bytes@.take(i as int) =~= bytes@.take(i as int-1).push(bytes@[i as int-1]));}
    }
    #[cfg(verus_keep_ghost)]
    proof! {assert(bytes@.take(bytes@.len() as int) =~= bytes@);}
}

/// Encodes Word as 0 followed by 16 big-endian bytes; Bytes as 1, a
/// 16-byte big-endian length, and the exact bytes. Lengths preserve all usize bits.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    (match result {Some(bytes)=>Some(bytes@),None=>None::<Seq<u8>>}) == model::encode(parts@),
))]
pub fn encode(parts: &[Part<'_>]) -> Option<Vec<u8>> {
    encoded_size(parts)?;
    let mut out = Vec::new();
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=parts.len(),
        model::length(parts@,parts@.len())<=usize::MAX,
        out@ == model::prefix(parts@,i as nat), decreases parts.len()-i,))]
    while i < parts.len() {
        match parts[i] {
            Part::Word(value) => {
                out.push(0);
                let bytes = word(value);
                append(&mut out, &bytes);
            }
            Part::Bytes(value) => {
                out.push(1);
                let length = word(value.len() as u128);
                append(&mut out, &length);
                append(&mut out, value);
            }
        }
        i += 1;
    }
    Some(out)
}

/// Full exact comparison. Empty, trailing, changed, and same-length substitutions
/// all follow byte equality; no digest collision assumption is required.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (left@ == right@),))]
pub fn exact(left: &[u8], right: &[u8]) -> bool {
    super::super::util::bytes_equal(left, right)
}
