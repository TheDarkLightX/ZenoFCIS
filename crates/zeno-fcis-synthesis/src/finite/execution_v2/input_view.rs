//! Closed flat-record decoding for the protected V2 input proof chain.
use super::super::canonical_v2::{read_big_endian, read_signed_128};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
mod spec;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// One closed variant ID mapped to its declared signed scalar code.
pub struct Variant {
    /// Closed wire variant identifier; zero is legal in generic ZCVE.
    pub id: u16,
    /// Declared scalar code within the complete inclusive interval.
    pub code: i64,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
/// A complete finite leaf descriptor; codes may occupy any inclusive i64 interval.
pub enum Leaf {
    /// A signed canonical i128 constrained by an inclusive i64 value interval.
    I128 {
        /// Inclusive lower value bound.
        min: i64,
        /// Inclusive upper value bound.
        max: i64,
    },
    /// Canonical false and true, projected to scalar zero and one.
    Bool,
    /// A closed Enum with complete signed codes and arbitrary map order.
    Enum {
        /// Exact wire type identifier.
        type_id: u32,
        /// Inclusive lower scalar-code bound.
        min: i64,
        /// Inclusive upper scalar-code bound.
        max: i64,
        /// Distinct variant IDs and codes covering the entire declared interval.
        variants: Vec<Variant>,
    },
    /// A closed Sum whose payload-present flag must be zero.
    Sum {
        /// Exact wire type identifier.
        type_id: u32,
        /// Inclusive lower scalar-code bound.
        min: i64,
        /// Inclusive upper scalar-code bound.
        max: i64,
        /// Distinct variant IDs and codes covering the entire declared interval.
        variants: Vec<Variant>,
    },
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
/// A canonical field ID and its complete closed leaf descriptor.
pub struct Field {
    /// Expected wire field ID; record descriptors must be strictly increasing.
    pub id: u16,
    /// Complete closed schema of this field.
    pub leaf: Leaf,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::variants_valid(variants@, min, max),
))]
fn validate_variants(variants: &[Variant], min: i64, max: i64) -> bool {
    if min > max {
        return false;
    }
    let cardinality = max as i128 - min as i128 + 1;
    if cardinality != variants.len() as i128 {
        return false;
    }
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= variants.len(), min <= max,
            max as int - min as int + 1 == variants@.len(),
            spec::variants_prefix(variants@, min, max, index as nat),
        decreases variants.len() - index,
    ))]
    while index < variants.len() {
        let variant = variants[index];
        if variant.code < min || variant.code > max {
            #[cfg(verus_keep_ghost)]
            proof! {
                spec::variants_failure(variants@, min, max, (index + 1) as nat,
                    variants@.len() as nat);
            }
            return false;
        }
        let mut prior = 0usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant prior <= index < variants.len(),
                variant == variants@[index as int], min <= variant.code <= max,
                spec::variants_prefix(variants@, min, max, index as nat),
                forall|j: int| 0 <= j < prior ==> variants@[j].id != variant.id
                    && variants@[j].code != variant.code,
            decreases index - prior,
        ))]
        while prior < index {
            if variants[prior].id == variant.id || variants[prior].code == variant.code {
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(!spec::variants_prefix(variants@, min, max, (index + 1) as nat));
                    spec::variants_failure(variants@, min, max, (index + 1) as nat,
                        variants@.len() as nat);
                }
                return false;
            }
            prior += 1;
        }
        index += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::leaf_valid(*leaf),
))]
fn validate_leaf(leaf: &Leaf) -> bool {
    match leaf {
        Leaf::I128 { min, max } => min <= max,
        Leaf::Bool => true,
        Leaf::Enum {
            min, max, variants, ..
        }
        | Leaf::Sum {
            min, max, variants, ..
        } => validate_variants(variants, *min, *max),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == schema_valid(fields@),
))]
pub(super) fn validate_schema(fields: &[Field]) -> bool {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(schema_valid); }
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= fields.len(), spec::schema_prefix(fields@, index as nat),
        decreases fields.len() - index,
    ))]
    while index < fields.len() {
        if (index > 0 && fields[index - 1].id >= fields[index].id)
            || !validate_leaf(&fields[index].leaf)
        {
            #[cfg(verus_keep_ghost)]
            proof! {
                spec::schema_failure(fields@, (index + 1) as nat, fields@.len() as nat);
            }
            return false;
        }
        index += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::lookup(variants@, id, variants@.len()),
))]
fn lookup(variants: &[Variant], id: u16) -> Option<i64> {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= variants.len(),
            spec::lookup(variants@, id, index as nat) == None::<i64>,
        decreases variants.len() - index,
    ))]
    while index < variants.len() {
        if variants[index].id == id {
            let code = variants[index].code;
            #[cfg(verus_keep_ghost)]
            proof! {
                spec::lookup_persists(variants@, id, (index + 1) as nat,
                    variants@.len() as nat, code);
            }
            return Some(code);
        }
        index += 1;
    }
    None
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::variant_scalar(bytes@, offset, type_id, variants@, sum),
))]
fn decode_variant(
    bytes: &[u8],
    offset: usize,
    type_id: u32,
    variants: &[Variant],
    sum: bool,
) -> Option<(i64, usize)> {
    let (raw_type, after_type) = read_big_endian(bytes, offset, 4)?;
    if raw_type != type_id as u128 {
        return None;
    }
    let (raw_id, after_id) = read_big_endian(bytes, after_type, 2)?;
    let end = if sum {
        let (payload, after_payload) = read_big_endian(bytes, after_id, 1)?;
        if payload != 0 {
            return None;
        }
        after_payload
    } else {
        after_id
    };
    let code = lookup(variants, raw_id as u16)?;
    Some((code, end))
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::scalar(bytes@, offset, *leaf),
))]
fn decode_scalar(bytes: &[u8], offset: usize, leaf: &Leaf) -> Option<(i64, usize)> {
    let (tag, payload_offset) = read_big_endian(bytes, offset, 1)?;
    match leaf {
        Leaf::I128 { min, max } => {
            if tag != 0x04 {
                return None;
            }
            let (value, end) = read_signed_128(bytes, payload_offset)?;
            if value < *min as i128 || value > *max as i128 {
                return None;
            }
            Some((value as i64, end))
        }
        Leaf::Bool => {
            if tag == 0x01 {
                Some((0, payload_offset))
            } else if tag == 0x02 {
                Some((1, payload_offset))
            } else {
                None
            }
        }
        Leaf::Enum {
            type_id, variants, ..
        } => {
            if tag != 0x07 {
                return None;
            }
            decode_variant(bytes, payload_offset, *type_id, variants, false)
        }
        Leaf::Sum {
            type_id, variants, ..
        } => {
            if tag != 0x0a {
                return None;
            }
            decode_variant(bytes, payload_offset, *type_id, variants, true)
        }
    }
}

/// Refusal from a protected flat-record projection.
#[non_exhaustive]
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// The closed descriptor is malformed.
    Schema,
    /// The record tag, count or header is invalid or truncated.
    Header,
    /// A declared field ID or its closed leaf value could not be decoded.
    Field(u16),
    /// Bytes remain after all declared fields.
    Trailing,
    /// A logical charge refused before its associated raw access.
    Budget(super::MeterFailure),
}

/// A descriptor-based Read request and whether its charge permitted access.
/// This is a logical observation, not physical byte-access tracing.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccessAttempt {
    field_id: u16,
    permitted: bool,
}

impl AccessAttempt {
    /// Expected field ID from the admitted descriptor, including a denied request.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.view().0,))]
    pub fn field_id(self) -> u16 {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(AccessAttempt::view); }
        self.field_id
    }

    /// Whether the Read charge permitted a field-decode attempt.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.view().1,))]
    pub fn permitted(self) -> bool {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(AccessAttempt::view); }
        self.permitted
    }
}

/// One complete record projection with library-computed usage and attempts.
/// This result does not authorize a transition.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub struct Projection {
    result: Result<Vec<i64>, Failure>,
    usage: super::Usage,
    attempts: Vec<AccessAttempt>,
}

impl Projection {
    /// Reads retained usage without permitting replacement.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result.view() == self.view().1,
    ))]
    pub fn usage(&self) -> super::Usage {
        self.usage
    }

    /// Reads the descriptor-based request sequence, including refusal.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result@ == self.view().2,
    ))]
    pub fn attempts(&self) -> &[AccessAttempt] {
        &self.attempts
    }

    /// Consumes the outcome without separating a refusal from its retained report.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures (match result.0 { Ok(values) => Ok(values@), Err(error) => Err(error) },
            result.1.view(), result.2@) == self.view(),
    ))]
    pub fn into_parts(self) -> (Result<Vec<i64>, Failure>, super::Usage, Vec<AccessAttempt>) {
        (self.result, self.usage, self.attempts)
    }
}

#[cfg(verus_keep_ghost)]
verus! {
impl AccessAttempt {
    pub closed spec fn view(self) -> (u16, bool) { (self.field_id, self.permitted) }
}
impl Projection {
    pub closed spec fn view(&self)
        -> (Result<Seq<i64>, Failure>, Seq<u64>, Seq<AccessAttempt>) {
        (match self.result { Ok(values) => Ok(values@), Err(error) => Err(error) },
            self.usage.view(), self.attempts@)
    }
}
pub closed spec fn projection(bytes: Seq<u8>, fields: Seq<Field>,
    limits: Seq<u64>, used: Seq<u64>)
    -> (Result<Seq<i64>, Failure>, Seq<u64>, Seq<AccessAttempt>) {
    spec::projection(bytes, fields, limits, used)
}
pub closed spec fn schema_valid(fields: Seq<Field>) -> bool {
    spec::schema_valid(fields)
}
pub closed spec fn leaf_domain(leaf: Leaf) -> super::super::evaluation::Domain {
    match leaf {
        Leaf::Bool => super::super::evaluation::Domain::Bool,
        Leaf::I128 { min, max } | Leaf::Enum { min, max, .. }
        | Leaf::Sum { min, max, .. } => super::super::evaluation::Domain::Int { min, max },
    }
}
pub(super) proof fn projected_is_typed(bytes: Seq<u8>, fields: Seq<Field>,
    limits: Seq<u64>, used: Seq<u64>, values: Seq<i64>)
    requires limits.len() == 8, used.len() == 8,
        projection(bytes, fields, limits, used).0 == Ok(values),
    ensures values.len() == fields.len(),
        forall|i: int| 0 <= i < fields.len() ==>
            super::super::evaluation::spec::contains(leaf_domain(fields[i].leaf), values[i]),
{
    reveal(projection);
    spec::projection_is_typed(bytes, fields, limits, used, values);
    assert forall|i: int| 0 <= i < fields.len() implies
        super::super::evaluation::spec::contains(leaf_domain(fields[i].leaf), values[i]) by {
        reveal(leaf_domain);
        assert(spec::leaf_contains(fields[i].leaf, values[i]));
    }
}
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == leaf_domain(*leaf),
))]
pub(super) fn scalar_domain(leaf: &Leaf) -> super::super::evaluation::Domain {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(leaf_domain); }
    match leaf {
        Leaf::Bool => super::super::evaluation::Domain::Bool,
        Leaf::I128 { min, max } | Leaf::Enum { min, max, .. } | Leaf::Sum { min, max, .. } => {
            super::super::evaluation::Domain::Int {
                min: *min,
                max: *max,
            }
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::field(bytes@, offset, *field),
))]
fn decode_field(bytes: &[u8], offset: usize, field: &Field) -> Option<(i64, usize)> {
    let (id, payload_offset) = read_big_endian(bytes, offset, 2)?;
    if id != field.id as u128 {
        return None;
    }
    decode_scalar(bytes, payload_offset, &field.leaf)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::header(bytes@, field_count),
))]
fn decode_header(bytes: &[u8], field_count: usize) -> Option<usize> {
    let (tag, count_offset) = read_big_endian(bytes, 0, 1)?;
    if tag != 0x09 {
        return None;
    }
    let (count, fields_offset) = read_big_endian(bytes, count_offset, 4)?;
    if count != field_count as u128 {
        return None;
    }
    Some(fields_offset)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits == old(meter).limits,
        (match result { Ok(end) => Ok((final(output)@, end)), Err(error) => Err(error) },
            final(meter).used.counters@,
            final(attempts)@) == {
                let projected = spec::prefix(bytes@, fields@, fields@.len(), offset,
                    old(meter).limits.counters@, old(meter).used.counters@);
                (projected.0, projected.1, old(attempts)@ + projected.2)
            },
))]
fn project_fields(
    bytes: &[u8],
    fields: &[Field],
    offset: usize,
    meter: &mut super::meter::Meter,
    output: &mut Vec<i64>,
    attempts: &mut Vec<AccessAttempt>,
) -> Result<usize, Failure> {
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost initial_used = meter.used.counters@;
        let ghost initial_attempts = attempts@;
    }
    output.clear();
    let mut index = 0usize;
    let mut position = offset;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= fields.len(),
            meter.limits == old(meter).limits,
            initial_used == old(meter).used.counters@, initial_used.len() == 8,
            initial_attempts == old(attempts)@,
            spec::prefix(bytes@, fields@, index as nat, offset,
                meter.limits.counters@, initial_used).0 == Ok((output@, position)),
            spec::prefix(bytes@, fields@, index as nat, offset,
                meter.limits.counters@, initial_used).1 == meter.used.counters@,
            attempts@ == initial_attempts + spec::prefix(bytes@, fields@, index as nat,
                offset, meter.limits.counters@, initial_used).2,
        decreases fields.len() - index,
    ))]
    while index < fields.len() {
        let charged = meter.charge(super::Resource::Read, 1);
        attempts.push(AccessAttempt {
            field_id: fields[index].id,
            permitted: charged.is_ok(),
        });
        if let Err(error) = charged {
            #[cfg(verus_keep_ghost)]
            proof! {
                spec::prefix_failure(bytes@, fields@, (index + 1) as nat,
                    fields@.len() as nat, offset, meter.limits.counters@,
                    initial_used, Failure::Budget(error));
            }
            return Err(Failure::Budget(error));
        }
        match decode_field(bytes, position, &fields[index]) {
            Some((value, end)) => {
                output.push(value);
                position = end;
            }
            None => {
                #[cfg(verus_keep_ghost)]
                proof! {
                    spec::prefix_failure(bytes@, fields@, (index + 1) as nat,
                        fields@.len() as nat, offset, meter.limits.counters@,
                        initial_used, Failure::Field(fields@[index as int].id));
                }
                return Err(Failure::Field(fields[index].id));
            }
        }
        index += 1;
    }
    Ok(position)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits == old(meter).limits,
        (match result { Ok(()) => Ok(final(output)@), Err(error) => Err(error) },
            final(meter).used.counters@, final(attempts)@) == {
                let projected = projection(bytes@, fields@,
                    old(meter).limits.counters@, old(meter).used.counters@);
                (projected.0, projected.1, old(attempts)@ + projected.2)
            },
        result.is_err() ==> final(output)@ == Seq::<i64>::empty(),
))]
pub(super) fn project_into(
    bytes: &[u8],
    fields: &[Field],
    meter: &mut super::meter::Meter,
    output: &mut Vec<i64>,
    attempts: &mut Vec<AccessAttempt>,
) -> Result<(), Failure> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(projection); reveal(schema_valid); }
    output.clear();
    if !validate_schema(fields) {
        return Err(Failure::Schema);
    }
    if let Err(error) = meter.charge(super::Resource::Byte, bytes.len() as u64) {
        return Err(Failure::Budget(error));
    }
    let Some(offset) = decode_header(bytes, fields.len()) else {
        return Err(Failure::Header);
    };
    let result = project_fields(bytes, fields, offset, meter, output, attempts);
    match result {
        Ok(end) => {
            if end != bytes.len() {
                output.clear();
                return Err(Failure::Trailing);
            }
            Ok(())
        }
        Err(error) => {
            output.clear();
            Err(error)
        }
    }
}

/// Projects one canonical flat record with a fresh private V2 meter.
///
/// Descriptor admission precedes ingress. Byte charges precede the record header,
/// and each Read charge precedes field ID and payload interpretation. Refusal
/// retains all prior charges and descriptor requests, and exposes no partial
/// projected scalars. This profile does not measure CPU, allocation or hashing.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.view() == projection(bytes@, fields@, limits.view(), Seq::new(8, |_: int| 0u64)),
))]
pub fn project(bytes: &[u8], fields: &[Field], limits: super::Limits) -> Projection {
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(Projection::view); reveal(super::Limits::view); reveal(super::Usage::view);
    }
    let mut meter = super::meter::new(limits);
    let mut output = Vec::new();
    let mut attempts = Vec::new();
    let result = match project_into(bytes, fields, &mut meter, &mut output, &mut attempts) {
        Ok(()) => Ok(output),
        Err(error) => Err(error),
    };
    Projection {
        result,
        usage: meter.used,
        attempts,
    }
}

#[cfg(test)]
pub(super) mod tests;
