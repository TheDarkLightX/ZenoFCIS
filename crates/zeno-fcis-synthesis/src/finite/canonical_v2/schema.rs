//! Complete original schema-wire matching for the closed execution profile.
use super::{read_big_endian, read_signed_128};
mod admission;
#[cfg(verus_keep_ghost)]
pub use admission::spec::admit as admission_result;
pub use admission::{Checked, Failure, Limits, admit};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
mod spec;

/// One complete named payload-free variant in canonical ID order.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct Variant<'a> {
    /// Original variant identifier, including generic zero.
    pub id: u16,
    /// Original schema name bytes.
    pub name: &'a [u8],
}

/// One complete sum alternative, including its original optional payload type.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct SumVariant<'a> {
    /// Original variant identifier.
    pub id: u16,
    /// Original schema name bytes.
    pub name: &'a [u8],
    /// Original payload type, absent exactly for a payload-free alternative.
    pub payload: Option<u32>,
}

/// One complete original field definition in canonical ID order.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct Field<'a> {
    /// Original field identifier, including generic zero.
    pub id: u16,
    /// Original schema name bytes.
    pub name: &'a [u8],
    /// Referenced original type, including compound types.
    pub type_id: u32,
}

/// The complete grammar of this execution profile, without narrowed bounds.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Kind<'a> {
    /// The unit schema.
    Unit,
    /// Canonical Boolean values.
    Bool,
    /// Inclusive full-width unsigned bounds.
    U128 {
        /// Inclusive lower value bound.
        min: u128,
        /// Inclusive upper value bound.
        max: u128,
    },
    /// Inclusive full-width signed bounds.
    I128 {
        /// Inclusive lower value bound.
        min: i128,
        /// Inclusive upper value bound.
        max: i128,
    },
    /// Inclusive raw byte-length bounds.
    Bytes {
        /// Inclusive lower byte-length bound.
        min: u32,
        /// Inclusive upper byte-length bound.
        max: u32,
    },
    /// Inclusive canonical ASCII text-length bounds.
    Text {
        /// Inclusive lower byte-length bound.
        min: u32,
        /// Inclusive upper byte-length bound.
        max: u32,
    },
    /// Complete closed payload-free enum.
    Enum(&'a [Variant<'a>]),
    /// Complete ordered tuple of original type identifiers.
    Tuple(&'a [u32]),
    /// Complete record.
    Record(&'a [Field<'a>]),
    /// Complete closed payload-free sum.
    Sum(&'a [Variant<'a>]),
    /// Complete closed sum, including payload-bearing alternatives.
    SumPayload(&'a [SumVariant<'a>]),
    /// Bounded vector of the original element type.
    Vector {
        /// Original element type identifier.
        element: u32,
        /// Inclusive lower cardinality bound.
        min: u32,
        /// Inclusive upper cardinality bound.
        max: u32,
    },
    /// Bounded map, ordered by the complete original encoded key bytes.
    Map {
        /// Original key type identifier.
        key: u32,
        /// Original value type identifier.
        value: u32,
        /// Inclusive lower cardinality bound.
        min: u32,
        /// Inclusive upper cardinality bound.
        max: u32,
    },
}

/// One complete original type definition, including its original name.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct Definition<'a> {
    /// Original type identifier.
    pub id: u32,
    /// Original schema name bytes.
    pub name: &'a [u8],
    /// Complete original type grammar.
    pub kind: Kind<'a>,
}

/// An untrusted proposed description of the complete schema.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct Description<'a> {
    /// Original profile name.
    pub profile: &'a [u8],
    /// Original schema version; generic zero is not reserved here.
    pub version: u16,
    /// Original declared schema root.
    pub root: u32,
    /// Every original definition, including unused definitions.
    pub definitions: &'a [Definition<'a>],
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::word(bytes@, offset, width, expected),
))]
fn word(bytes: &[u8], offset: usize, width: usize, expected: u128) -> Option<usize> {
    let (actual, end) = read_big_endian(bytes, offset, width)?;
    if actual != expected { None } else { Some(end) }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::signed(bytes@, offset, expected),
))]
fn signed(bytes: &[u8], offset: usize, expected: i128) -> Option<usize> {
    let (actual, end) = read_signed_128(bytes, offset)?;
    if actual != expected { None } else { Some(end) }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::blob(bytes@, offset, expected@),
))]
fn blob(bytes: &[u8], offset: usize, expected: &[u8]) -> Option<usize> {
    if offset > bytes.len() || expected.len() > bytes.len() - offset {
        return None;
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
            return None;
        }
        index += 1;
    }
    Some(offset + expected.len())
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::name(bytes@, offset, expected@),
))]
fn name(bytes: &[u8], offset: usize, expected: &[u8]) -> Option<usize> {
    let next = word(bytes, offset, 2, expected.len() as u128)?;
    blob(bytes, next, expected)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::variant(bytes@, offset, *expected, sum),
))]
fn variant(bytes: &[u8], offset: usize, expected: &Variant<'_>, sum: bool) -> Option<usize> {
    let next = word(bytes, offset, 2, expected.id as u128)?;
    let next = name(bytes, next, expected.name)?;
    if sum {
        word(bytes, next, 1, 0)
    } else {
        Some(next)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::variant_sequence(bytes@, offset, expected@, expected@.len(), sum),
))]
fn variant_sequence(
    bytes: &[u8],
    offset: usize,
    expected: &[Variant<'_>],
    sum: bool,
) -> Option<usize> {
    let mut next = offset;
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= expected.len(),
            spec::variant_sequence(bytes@, offset, expected@, index as nat, sum) == Some(next),
        decreases expected.len() - index,
    ))]
    while index < expected.len() {
        match variant(bytes, next, &expected[index], sum) {
            Some(end) => {
                next = end;
                index += 1;
            }
            None => {
                #[cfg(verus_keep_ghost)]
                proof! { spec::variant_failure(bytes@, offset, expected@, (index + 1) as nat, expected@.len() as nat, sum); }
                return None;
            }
        }
    }
    Some(next)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::sum_variant(bytes@, offset, *expected),
))]
fn sum_variant(bytes: &[u8], offset: usize, expected: &SumVariant<'_>) -> Option<usize> {
    let next = word(bytes, offset, 2, expected.id as u128)?;
    let next = name(bytes, next, expected.name)?;
    match expected.payload {
        None => word(bytes, next, 1, 0),
        Some(id) => {
            let next = word(bytes, next, 1, 1)?;
            word(bytes, next, 4, id as u128)
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::sum_variants(bytes@, offset, expected@, expected@.len()),
))]
fn sum_variants(bytes: &[u8], offset: usize, expected: &[SumVariant<'_>]) -> Option<usize> {
    let mut next = offset;
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= expected.len(),
            spec::sum_variants(bytes@, offset, expected@, index as nat) == Some(next),
        decreases expected.len() - index,
    ))]
    while index < expected.len() {
        match sum_variant(bytes, next, &expected[index]) {
            Some(end) => {
                next = end;
                index += 1;
            }
            None => {
                #[cfg(verus_keep_ghost)]
                proof! { spec::sum_variants_failure(bytes@, offset, expected@, (index + 1) as nat, expected@.len()); }
                return None;
            }
        }
    }
    Some(next)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::type_sequence(bytes@, offset, expected@, expected@.len()),
))]
fn type_sequence(bytes: &[u8], offset: usize, expected: &[u32]) -> Option<usize> {
    let mut next = offset;
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= expected.len(),
            spec::type_sequence(bytes@, offset, expected@, index as nat) == Some(next),
        decreases expected.len() - index,
    ))]
    while index < expected.len() {
        match word(bytes, next, 4, expected[index] as u128) {
            Some(end) => {
                next = end;
                index += 1;
            }
            None => {
                #[cfg(verus_keep_ghost)]
                proof! { spec::type_sequence_failure(bytes@, offset, expected@, (index + 1) as nat, expected@.len()); }
                return None;
            }
        }
    }
    Some(next)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::field(bytes@, offset, *expected),
))]
fn field(bytes: &[u8], offset: usize, expected: &Field<'_>) -> Option<usize> {
    let next = word(bytes, offset, 2, expected.id as u128)?;
    let next = name(bytes, next, expected.name)?;
    word(bytes, next, 4, expected.type_id as u128)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::field_sequence(bytes@, offset, expected@, expected@.len()),
))]
fn field_sequence(bytes: &[u8], offset: usize, expected: &[Field<'_>]) -> Option<usize> {
    let mut next = offset;
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= expected.len(),
            spec::field_sequence(bytes@, offset, expected@, index as nat) == Some(next),
        decreases expected.len() - index,
    ))]
    while index < expected.len() {
        match field(bytes, next, &expected[index]) {
            Some(end) => {
                next = end;
                index += 1;
            }
            None => {
                #[cfg(verus_keep_ghost)]
                proof! { spec::field_failure(bytes@, offset, expected@, (index + 1) as nat, expected@.len() as nat); }
                return None;
            }
        }
    }
    Some(next)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::kind(bytes@, offset, *expected),
))]
fn kind(bytes: &[u8], offset: usize, expected: &Kind<'_>) -> Option<usize> {
    match expected {
        Kind::Unit => word(bytes, offset, 1, 0),
        Kind::Bool => word(bytes, offset, 1, 1),
        Kind::U128 { min, max } => {
            let next = word(bytes, offset, 1, 2)?;
            let next = word(bytes, next, 16, *min)?;
            word(bytes, next, 16, *max)
        }
        Kind::I128 { min, max } => {
            let next = word(bytes, offset, 1, 3)?;
            let next = signed(bytes, next, *min)?;
            signed(bytes, next, *max)
        }
        Kind::Bytes { min, max } | Kind::Text { min, max } => {
            let tag = match expected {
                Kind::Bytes { .. } => 4u128,
                _ => 5u128,
            };
            let next = word(bytes, offset, 1, tag)?;
            let next = word(bytes, next, 4, *min as u128)?;
            word(bytes, next, 4, *max as u128)
        }
        Kind::Enum(variants) | Kind::Sum(variants) => {
            let sum = matches!(expected, Kind::Sum(_));
            let next = word(bytes, offset, 1, if sum { 9 } else { 6 })?;
            let next = word(bytes, next, 4, variants.len() as u128)?;
            variant_sequence(bytes, next, variants, sum)
        }
        Kind::Record(fields) => {
            let next = word(bytes, offset, 1, 8)?;
            let next = word(bytes, next, 4, fields.len() as u128)?;
            field_sequence(bytes, next, fields)
        }
        Kind::Tuple(items) => {
            let next = word(bytes, offset, 1, 7)?;
            let next = word(bytes, next, 4, items.len() as u128)?;
            type_sequence(bytes, next, items)
        }
        Kind::SumPayload(variants) => {
            let next = word(bytes, offset, 1, 9)?;
            let next = word(bytes, next, 4, variants.len() as u128)?;
            sum_variants(bytes, next, variants)
        }
        Kind::Vector { element, min, max } => {
            let next = word(bytes, offset, 1, 10)?;
            let next = word(bytes, next, 4, *element as u128)?;
            let next = word(bytes, next, 4, *min as u128)?;
            word(bytes, next, 4, *max as u128)
        }
        Kind::Map {
            key,
            value,
            min,
            max,
        } => {
            let next = word(bytes, offset, 1, 11)?;
            let next = word(bytes, next, 4, *key as u128)?;
            let next = word(bytes, next, 4, *value as u128)?;
            let next = word(bytes, next, 4, *min as u128)?;
            word(bytes, next, 4, *max as u128)
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::definition(bytes@, offset, *expected),
))]
fn definition(bytes: &[u8], offset: usize, expected: &Definition<'_>) -> Option<usize> {
    let next = word(bytes, offset, 4, expected.id as u128)?;
    let next = name(bytes, next, expected.name)?;
    kind(bytes, next, &expected.kind)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::definitions(bytes@, offset, expected@, expected@.len()),
))]
fn definitions(bytes: &[u8], offset: usize, expected: &[Definition<'_>]) -> Option<usize> {
    let mut next = offset;
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= expected.len(),
            spec::definitions(bytes@, offset, expected@, index as nat) == Some(next),
        decreases expected.len() - index,
    ))]
    while index < expected.len() {
        match definition(bytes, next, &expected[index]) {
            Some(end) => {
                next = end;
                index += 1;
            }
            None => {
                #[cfg(verus_keep_ghost)]
                proof! { spec::definitions_failure(bytes@, offset, expected@, (index + 1) as nat, expected@.len() as nat); }
                return None;
            }
        }
    }
    Some(next)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::schema_end(bytes@, *description),
))]
fn schema_end(bytes: &[u8], description: &Description<'_>) -> Option<usize> {
    let magic = [
        90u8, 70u8, 67u8, 73u8, 83u8, 83u8, 67u8, 72u8, 69u8, 77u8, 65u8, 49u8, 0u8,
    ];
    #[cfg(verus_keep_ghost)]
    proof! { assert(magic@ == spec::magic()); }
    let next = blob(bytes, 0, &magic)?;
    let next = name(bytes, next, description.profile)?;
    let next = word(bytes, next, 2, description.version as u128)?;
    let next = word(bytes, next, 4, description.root as u128)?;
    let next = word(bytes, next, 4, description.definitions.len() as u128)?;
    definitions(bytes, next, description.definitions)
}

/// Checks exact original canonical encoding and consumes all bytes.
///
/// This does not yet check description well-formedness or authorize execution.
/// Its complete schema witness must also pass closed metadata admission.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::encoding_matches(bytes@, *description, max_bytes),
))]
pub fn encoding_matches(bytes: &[u8], description: &Description<'_>, max_bytes: u64) -> bool {
    if bytes.len() as u64 > max_bytes {
        return false;
    }
    match schema_end(bytes, description) {
        Some(end) => end == bytes.len(),
        None => false,
    }
}

#[cfg(test)]
mod tests;
