//! Exact original envelope custody; framing does not admit a typed value.

use super::read_big_endian;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(in super::super) mod spec;

/// A framing refusal in fixed size/header/identity/length order.
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// The complete received slice exceeds its input-size policy.
    Size,
    /// Fewer than the 48 header bytes were supplied.
    Short,
    /// The magic differs from the existing envelope format.
    Magic,
    /// The root type differs from the bound expectation.
    Root,
    /// At least one schema-commitment byte differs.
    Schema,
    /// The complete suffix length differs from the declared u32.
    Length,
}

/// Exact immutable received bytes and their borrowed payload suffix.
///
/// Private fields prevent inventing a payload independently of the frame.
/// This result alone does not establish payload typing or authorize work.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Payload<'a> {
    original: &'a [u8],
    payload: &'a [u8],
}

impl<'a> Payload<'a> {
    /// Reads the entire original received envelope.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result@ == self.view().0,
    ))]
    pub fn original(&self) -> &'a [u8] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Payload::view); }
        self.original
    }

    /// Reads exactly the original suffix after the checked frame header.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result@ == self.view().1,
    ))]
    pub fn bytes(&self) -> &'a [u8] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Payload::view); }
        self.payload
    }
}

#[cfg(verus_keep_ghost)]
verus! {
impl<'a> Payload<'a> {
    pub closed spec fn view(&self) -> (Seq<u8>, Seq<u8>) {
        (self.original@, self.payload@)
    }
}
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::matches(bytes@, offset, expected@),
))]
fn matches(bytes: &[u8], offset: usize, expected: &[u8]) -> bool {
    if offset > bytes.len() || expected.len() > bytes.len() - offset {
        return false;
    }
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= expected.len(), offset <= bytes.len(),
            expected.len() <= bytes.len() - offset,
            forall|j: int| 0 <= j < index ==> bytes@[offset as int + j] == expected@[j],
        decreases expected.len() - index,
    ))]
    while index < expected.len() {
        if bytes[offset + index] != expected[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// Frames the original envelope with exact root, schema and suffix binding.
///
/// This primitive is unmetered. A protected caller must charge header ingress
/// before calling it and admit its payload under the bound schema afterwards.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (match result {
        Ok(payload) => Ok(payload.view()), Err(error) => Err(error),
    }) == spec::frame(bytes@, root_type, schema_hash@, max_bytes),
))]
pub fn frame<'a>(
    bytes: &'a [u8],
    root_type: u32,
    schema_hash: &[u8; 32],
    max_bytes: u64,
) -> Result<Payload<'a>, Failure> {
    if bytes.len() as u64 > max_bytes {
        return Err(Failure::Size);
    }
    if bytes.len() < 48 {
        return Err(Failure::Short);
    }
    let magic = [90u8, 70u8, 67u8, 73u8, 83u8, 86u8, 49u8, 0u8];
    #[cfg(verus_keep_ghost)]
    proof! { assert(magic@ == spec::magic()); }
    if !matches(bytes, 0, &magic) {
        return Err(Failure::Magic);
    }
    let Some((root, _)) = read_big_endian(bytes, 8, 4) else {
        return Err(Failure::Short);
    };
    if root != root_type as u128 {
        return Err(Failure::Root);
    }
    if !matches(bytes, 12, schema_hash) {
        return Err(Failure::Schema);
    }
    let Some((length, _)) = read_big_endian(bytes, 44, 4) else {
        return Err(Failure::Short);
    };
    if length != (bytes.len() - 48) as u128 {
        return Err(Failure::Length);
    }
    #[cfg(verus_keep_ghost)]
    proof! { reveal(Payload::view); }
    Ok(Payload {
        original: bytes,
        payload: &bytes[48..],
    })
}

#[cfg(test)]
mod tests;
