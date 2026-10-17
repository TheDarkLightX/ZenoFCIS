//! Closed flat-record decoding for the protected V2 input proof chain.
use super::super::canonical_v2::{read_big_endian, read_signed_128};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// One closed variant ID mapped to its declared signed scalar code.
pub struct Variant {
    /// Closed wire variant identifier; zero is legal in generic ZCVE.
    pub id: u16,
    /// Declared scalar code within the complete inclusive interval.
    pub code: i128,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
/// A complete finite leaf descriptor; codes may occupy any inclusive i128 interval.
#[non_exhaustive]
pub enum Leaf {
    /// Original unsigned canonical u128, with an injective bounded scalar projection.
    U128 {
        /// Complete inclusive unsigned lower bound.
        min: u128,
        /// Complete inclusive unsigned upper bound; scalar admission requires i128 fit.
        max: u128,
    },
    /// A signed canonical i128 constrained by an inclusive i128 value interval.
    I128 {
        /// Inclusive lower value bound.
        min: i128,
        /// Inclusive upper value bound.
        max: i128,
    },
    /// Canonical false and true, projected to scalar zero and one.
    Bool,
    /// A closed Enum with complete signed codes and arbitrary map order.
    Enum {
        /// Exact wire type identifier.
        type_id: u32,
        /// Inclusive lower scalar-code bound.
        min: i128,
        /// Inclusive upper scalar-code bound.
        max: i128,
        /// Distinct variant IDs and codes covering the entire declared interval.
        variants: Vec<Variant>,
    },
    /// A closed Sum whose payload-present flag must be zero.
    Sum {
        /// Exact wire type identifier.
        type_id: u32,
        /// Inclusive lower scalar-code bound.
        min: i128,
        /// Inclusive upper scalar-code bound.
        max: i128,
        /// Distinct variant IDs and codes covering the entire declared interval.
        variants: Vec<Variant>,
    },
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
/// A canonical field ID and its complete closed leaf descriptor.
pub struct Field<L = Leaf> {
    /// Expected wire field ID; record descriptors must be strictly increasing.
    pub id: u16,
    /// Complete closed schema of this field.
    pub leaf: L,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == spec::variants_valid(variants@, min, max),
))]
fn validate_variants(variants: &[Variant], min: i128, max: i128) -> bool {
    // Materialize the native length before the overflow exits. A sequence view
    // alone does not give the prover the machine-sized slice-length bound.
    let variant_count = variants.len();
    if min > max {
        return false;
    }
    // A complete variant table has at most usize::MAX entries. A difference
    // that exceeds i128 therefore cannot equal its cardinality.
    let cardinality = match max.checked_sub(min) {
        Some(span) => match span.checked_add(1) {
            Some(cardinality) => cardinality,
            None => {
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(variants@.len() <= usize::MAX);
                    assert(max as int - min as int + 1 > variants@.len());
                }
                return false;
            }
        },
        None => {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(variants@.len() <= usize::MAX);
                assert(max as int - min as int + 1 > variants@.len());
            }
            return false;
        }
    };
    if cardinality != variant_count as i128 {
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
pub(super) fn validate_leaf(leaf: &Leaf) -> bool {
    match leaf {
        Leaf::I128 { min, max } => min <= max,
        Leaf::U128 { min, max } => min <= max && *max <= i128::MAX as u128,
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
fn lookup(variants: &[Variant], id: u16) -> Option<i128> {
    let mut index = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= variants.len(),
            spec::lookup(variants@, id, index as nat) == None::<i128>,
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
) -> Option<(i128, usize)> {
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
#[cfg(any(test, verus_keep_ghost))]
pub(super) fn decode_scalar(bytes: &[u8], offset: usize, leaf: &Leaf) -> Option<(i128, usize)> {
    let (value, end) = typed::decode_value(bytes, offset, typed::LeafView::Legacy(leaf))?;
    match value {
        super::super::value::Atom::I128(value)
            if value >= i128::MIN as i128 && value <= i128::MAX as i128 =>
        {
            Some((value as i128, end))
        }
        _ => None,
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
    result: Result<Vec<i128>, Failure>,
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
    pub fn into_parts(self) -> (Result<Vec<i128>, Failure>, super::Usage, Vec<AccessAttempt>) {
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
        -> (Result<Seq<i128>, Failure>, Seq<u64>, Seq<AccessAttempt>) {
        (match self.result { Ok(values) => Ok(values@), Err(error) => Err(error) },
            self.usage.view(), self.attempts@)
    }
}
pub closed spec fn projection(bytes: Seq<u8>, fields: Seq<Field>,
    limits: Seq<u64>, used: Seq<u64>)
    -> (Result<Seq<i128>, Failure>, Seq<u64>, Seq<AccessAttempt>) {
    spec::projection(bytes, fields, limits, used)
}
pub(super) proof fn projection_usage(bytes: Seq<u8>, fields: Seq<Field>,
    limits: Seq<u64>, used: Seq<u64>)
    requires limits.len() == 8, used.len() == 8,
    ensures projection(bytes, fields, limits, used).1.len() == 8,
{
    reveal(projection);
    let charged = super::spec::charge(limits, used, super::Resource::Byte, bytes.len() as u64);
    if let Some(offset) = spec::header(bytes, fields.len() as usize) {
        spec::prefix_usage(bytes, fields, fields.len(), offset, limits, charged.1);
    }
}
pub closed spec fn schema_valid(fields: Seq<Field>) -> bool {
    spec::schema_valid(fields)
}
pub closed spec fn leaf_domain(leaf: Leaf) -> super::super::evaluation::Domain {
    match leaf {
        Leaf::Bool => super::super::evaluation::Domain::Bool,
        Leaf::U128 { min, max } => super::super::evaluation::Domain::Int { min: min as i128, max: max as i128 },
        Leaf::I128 { min, max } | Leaf::Enum { min, max, .. }
        | Leaf::Sum { min, max, .. } => super::super::evaluation::Domain::Int { min, max },
    }
}
pub(super) proof fn projected_is_typed(bytes: Seq<u8>, fields: Seq<Field>,
    limits: Seq<u64>, used: Seq<u64>, values: Seq<i128>)
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
        Leaf::U128 { min, max } => super::super::evaluation::Domain::Int {
            min: *min as i128,
            max: *max as i128,
        },
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
#[cfg(any(test, verus_keep_ghost))]
fn decode_field(bytes: &[u8], offset: usize, field: &Field) -> Option<(i128, usize)> {
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
        (match result { Ok(()) => Ok(final(output)@), Err(error) => Err(error) },
            final(meter).used.counters@, final(attempts)@) == {
                let projected = projection(bytes@, fields@,
                    old(meter).limits.counters@, old(meter).used.counters@);
                (projected.0, projected.1, old(attempts)@ + projected.2)
            },
        result.is_err() ==> final(output)@ == Seq::<i128>::empty(),
))]
pub(super) fn project_into(
    bytes: &[u8],
    fields: &[Field],
    meter: &mut super::meter::Meter,
    output: &mut Vec<i128>,
    attempts: &mut Vec<AccessAttempt>,
) -> Result<(), Failure> {
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost projected = projection(bytes@,fields@,meter.limits.counters@,meter.used.counters@); }
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(projection);
        typed::spec::legacy_projection(bytes@,fields,meter.limits.counters@,meter.used.counters@);
    }
    output.clear();
    let mut values = Vec::new();
    typed::project_into(
        bytes,
        typed::FieldsView::Legacy(fields),
        meter,
        &mut values,
        attempts,
    )?;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(projected.0.is_ok());
        let codes = match projected.0 { Ok(codes) => codes, Err(_) => Seq::<i128>::empty() };
        assert(values@ == typed::spec::lift(codes));
        assert(typed::spec::lift(codes) =~= super::super::evaluation::typed::spec::lift(codes));
        super::numeric::spec::lift_projects(codes);
        assert(super::numeric::spec::project_all(values@) == Some(codes));
    }
    if !super::numeric::project_legacy(&values, output) {
        output.clear();
        return Err(Failure::Schema);
    }
    Ok(())
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

pub(super) mod typed;
