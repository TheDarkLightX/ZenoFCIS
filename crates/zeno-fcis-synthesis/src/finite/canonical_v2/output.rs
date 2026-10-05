//! Exact original ZCVE/1 output and ZFCISV1 envelopes for typed candidates.
//!
//! These unmetered utilities grant no authority. Limits bound returned bytes,
//! not allocation capacity or physical work. Inputs remain immutable and an
//! error exposes no partial output. Schema admission and identity are separate.
use crate::finite::execution_v2::composition::{Atom, Field};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "output/spec.rs"]
pub(crate) mod spec;

/// Deterministic original-wire serialization refusal.
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// A u32 wire length or native total length cannot represent the input.
    Length,
    /// Text contains a byte outside ASCII.
    Text,
    /// Original field identifiers are not strictly ascending and unique.
    Order,
    /// The next complete encoding exceeds the caller's output byte cap.
    Limit,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result@ == spec::be2(value),
))]
fn be2(value: u16) -> [u8; 2] {
    #[cfg(verus_keep_ghost)]
    proof! { assert(value >> 0u32 == value) by (bit_vector); }
    [(value >> 8u32) as u8, value as u8]
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result@ == spec::be4(value),
))]
fn be4(value: u32) -> [u8; 4] {
    #[cfg(verus_keep_ghost)]
    proof! { assert(value >> 0u32 == value) by (bit_vector); }
    [
        (value >> 24u32) as u8,
        (value >> 16u32) as u8,
        (value >> 8u32) as u8,
        value as u8,
    ]
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result@ == spec::be16(value),
))]
fn be16(value: u128) -> [u8; 16] {
    #[cfg(verus_keep_ghost)]
    proof! { assert(value >> 0u32 == value) by (bit_vector); }
    [
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

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::ascii(bytes@),
))]
fn ascii(bytes: &[u8]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i <= bytes.len(), forall|j: int| 0 <= j < i ==> bytes@[j] <= 127,
        decreases bytes.len() - i,
    ))]
    while i < bytes.len() {
        if bytes[i] > 127 {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::atom_size(value),
        match result { Ok(n) => spec::atom_wire(value).len() == n, Err(_) => true },
))]
fn atom_size(value: Atom<'_>) -> Result<usize, Failure> {
    match value {
        Atom::Bool(_) => Ok(1),
        Atom::I128(_) | Atom::U128(_) => Ok(17),
        Atom::Enum { .. } => Ok(7),
        Atom::Sum { .. } => Ok(8),
        Atom::Bytes(bytes) | Atom::Text(bytes) => {
            if bytes.len() > u32::MAX as usize {
                return Err(Failure::Length);
            }
            if matches!(value, Atom::Text(_)) && !ascii(bytes) {
                return Err(Failure::Text);
            }
            match bytes.len().checked_add(5) {
                Some(n) => Ok(n),
                None => Err(Failure::Length),
            }
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (result, final(out)@) == if old(out)@.len() + bytes@.len() > usize::MAX {
        (Err(Failure::Length), old(out)@)
    } else { (Ok(()), old(out)@ + bytes@) },
))]
fn append(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), Failure> {
    if out.len().checked_add(bytes.len()).is_none() {
        return Err(Failure::Length);
    }
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost initial = out@; }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i <= bytes.len(), initial == old(out)@,
            initial.len() + bytes.len() <= usize::MAX,
            out@ == initial + bytes@.take(i as int),
        decreases bytes.len() - i,
    ))]
    while i < bytes.len() {
        out.push(bytes[i]);
        i += 1;
        #[cfg(verus_keep_ghost)]
        proof! { assert(bytes@.take(i as int) =~= bytes@.take(i as int - 1).push(bytes@[i as int - 1])); }
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(bytes@.take(bytes@.len() as int) =~= bytes@); }
    Ok(())
}

/// Encodes one complete existing typed atom in the original ZCVE/1 format.
/// Blob length, ASCII validity, native size overflow, then cap are checked in
/// that order. Every integer bit and every original type/variant ID is retained.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (match result { Ok(b) => Ok(b@), Err(e) => Err(e) }) == spec::atom_result(value, max_bytes),
        match result { Ok(b) => b@.len() <= max_bytes, Err(_) => true },
))]
pub fn encode_atom(value: Atom<'_>, max_bytes: usize) -> Result<Vec<u8>, Failure> {
    let size = atom_size(value)?;
    if size > max_bytes {
        return Err(Failure::Limit);
    }
    let mut out = Vec::new();
    match value {
        Atom::Bool(v) => out.push(if v { 0x02 } else { 0x01 }),
        Atom::I128(v) => {
            out.push(0x04);
            append(&mut out, &be16(v as u128))?;
        }
        Atom::U128(v) => {
            out.push(0x03);
            append(&mut out, &be16(v))?;
        }
        Atom::Enum { type_id, variant } => {
            out.push(0x07);
            append(&mut out, &be4(type_id))?;
            append(&mut out, &be2(variant))?;
        }
        Atom::Sum { type_id, variant } => {
            out.push(0x0a);
            append(&mut out, &be4(type_id))?;
            append(&mut out, &be2(variant))?;
            out.push(0);
        }
        Atom::Bytes(bytes) | Atom::Text(bytes) => {
            out.push(if matches!(value, Atom::Text(_)) {
                0x06
            } else {
                0x05
            });
            append(&mut out, &be4(bytes.len() as u32))?;
            append(&mut out, bytes)?;
        }
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(out@ =~= spec::atom_wire(value)); }
    Ok(out)
}

/// Encodes the complete flat record in its original ascending field order.
/// Checks u32 count, the five-byte header cap, then for each field: order,
/// two-byte ID cap, and the atom's validation/cap. A later failure discards all
/// private scratch. Field zero, gaps, empty records and large records are legal.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (match result { Ok(b) => Ok(b@), Err(e) => Err(e) })
        == spec::record_result(fields@, max_bytes, fields@.len()),
        match result { Ok(b) => b@.len() <= max_bytes, Err(_) => true },
))]
pub fn encode_record(fields: &[Field<'_>], max_bytes: usize) -> Result<Vec<u8>, Failure> {
    if fields.len() > u32::MAX as usize {
        #[cfg(verus_keep_ghost)]
        proof! { spec::record_initial_failure(fields@, max_bytes, fields@.len()); }
        return Err(Failure::Length);
    }
    if max_bytes < 5 {
        #[cfg(verus_keep_ghost)]
        proof! { spec::record_initial_failure(fields@, max_bytes, fields@.len()); }
        return Err(Failure::Limit);
    }
    let mut out = Vec::new();
    out.push(0x09);
    append(&mut out, &be4(fields.len() as u32))?;
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i <= fields.len(), fields.len() <= u32::MAX, max_bytes >= 5,
            out@.len() <= max_bytes,
            spec::record_result(fields@, max_bytes, i as nat) == Ok(out@),
        decreases fields.len() - i,
    ))]
    while i < fields.len() {
        if i > 0 && fields[i - 1].id >= fields[i].id {
            #[cfg(verus_keep_ghost)]
            proof! { spec::record_failure(fields@, max_bytes, (i + 1) as nat, fields@.len()); }
            return Err(Failure::Order);
        }
        if max_bytes - out.len() < 2 {
            #[cfg(verus_keep_ghost)]
            proof! { spec::record_failure(fields@, max_bytes, (i + 1) as nat, fields@.len()); }
            return Err(Failure::Limit);
        }
        let bytes = match encode_atom(fields[i].value, max_bytes - out.len() - 2) {
            Ok(bytes) => bytes,
            Err(e) => {
                #[cfg(verus_keep_ghost)]
                proof! { spec::record_failure(fields@, max_bytes, (i + 1) as nat, fields@.len()); }
                return Err(e);
            }
        };
        append(&mut out, &be2(fields[i].id))?;
        append(&mut out, &bytes)?;
        i += 1;
    }
    Ok(out)
}

/// Encodes the exact original frame. The payload is retained verbatim, and is
/// not interpreted or schema-admitted here. Checks u32 payload length, native
/// header addition overflow, then the complete frame cap. Empty payloads work.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (match result { Ok(b) => Ok(b@), Err(e) => Err(e) })
        == spec::envelope_result(root, schema@, payload@, max_bytes),
        match result { Ok(b) => b@.len() <= max_bytes, Err(_) => true },
))]
pub fn encode_envelope(
    root: u32,
    schema: &[u8; 32],
    payload: &[u8],
    max_bytes: usize,
) -> Result<Vec<u8>, Failure> {
    if payload.len() > u32::MAX as usize {
        return Err(Failure::Length);
    }
    let Some(size) = payload.len().checked_add(48) else {
        return Err(Failure::Length);
    };
    if size > max_bytes {
        return Err(Failure::Limit);
    }
    let mut out = Vec::new();
    let magic = [90u8, 70u8, 67u8, 73u8, 83u8, 86u8, 49u8, 0u8];
    append(&mut out, &magic)?;
    append(&mut out, &be4(root))?;
    append(&mut out, schema)?;
    append(&mut out, &be4(payload.len() as u32))?;
    append(&mut out, payload)?;
    #[cfg(verus_keep_ghost)]
    proof! { assert(out@ =~= spec::envelope_wire(root, schema@, payload@)); }
    Ok(out)
}

#[cfg(test)]
#[path = "output/tests.rs"]
mod tests;
