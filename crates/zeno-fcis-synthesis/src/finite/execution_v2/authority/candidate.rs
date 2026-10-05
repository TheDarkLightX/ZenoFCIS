//! Complete candidate projection. These encodings alone grant no authority.
use super::super::decision::{Atom, Candidate, Class, Delivery, Field, Patch};
use super::canonical::{Part, encode};
#[cfg(verus_keep_ghost)]
use super::spec as model;
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == model::class(class),))]
fn class_tag(class: Class) -> u128 {
    match class {
        Class::Accept => 0,
        Class::Reject => 1,
        Class::CommittedFailure => 2,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@ == old(parts)@+model::atom(value),))]
pub(super) fn atom<'a>(parts: &mut Vec<Part<'a>>, value: Atom<'a>) {
    match value {
        Atom::Bool(v) => {
            parts.push(Part::Word(0));
            parts.push(Part::Word(v as u128));
        }
        Atom::I128(v) => {
            parts.push(Part::Word(1));
            parts.push(Part::Word(v as u128));
        }
        Atom::U128(v) => {
            parts.push(Part::Word(2));
            parts.push(Part::Word(v));
        }
        Atom::Enum { type_id, variant } => {
            parts.push(Part::Word(3));
            parts.push(Part::Word(type_id as u128));
            parts.push(Part::Word(variant as u128));
        }
        Atom::Sum { type_id, variant } => {
            parts.push(Part::Word(4));
            parts.push(Part::Word(type_id as u128));
            parts.push(Part::Word(variant as u128));
        }
        Atom::Bytes(v) => {
            parts.push(Part::Word(5));
            parts.push(Part::Bytes(v));
        }
        Atom::Text(v) => {
            parts.push(Part::Word(6));
            parts.push(Part::Bytes(v));
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@ == old(parts)@+model::fields(values@),))]
fn fields<'a>(parts: &mut Vec<Part<'a>>, values: &[Field<'a>]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=values.len(),
        initial==old(parts)@,
        parts@ == initial+seq![Part::Word(values.len() as u128)]+model::fields_prefix(values@,i as nat),
        decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].id as u128));
        atom(parts, values[i].value);
        i += 1;
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@ == old(parts)@+model::patches(values@),))]
fn patches<'a>(parts: &mut Vec<Part<'a>>, values: &[Patch<'a>]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=values.len(),
        initial==old(parts)@,
        parts@ == initial+seq![Part::Word(values.len() as u128)]+model::patches_prefix(values@,i as nat),
        decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].field as u128));
        atom(parts, values[i].before);
        atom(parts, values[i].after);
        i += 1;
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@ == old(parts)@+model::deliveries(values@),))]
fn deliveries<'a>(parts: &mut Vec<Part<'a>>, values: &[Delivery<'a>]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=values.len(),
        initial==old(parts)@,
        parts@ == initial+seq![Part::Word(values.len() as u128)]+model::deliveries_prefix(values@,i as nat),
        decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].ordinal as u128));
        parts.push(Part::Word(values[i].channel as u128));
        atom(parts, values[i].destination);
        fields(parts, &values[i].payload);
        atom(parts, values[i].idempotency);
        i += 1;
    }
}

/// Complete class/reason/pre/post/patch/effects/outbox, in declared order.
/// Both failure classes remain distinct and preserve complete typed values.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == model::candidate(candidate),
    result@==model::projected_candidate(candidate),
    result@==model::candidate_projection_parts(super::super::composition::candidate_result(Ok(candidate)).unwrap()),))]
pub fn candidate_parts<'a>(candidate: &Candidate<'a>) -> Vec<Part<'a>> {
    let mut parts = Vec::new();
    parts.push(Part::Word(0x5a434432));
    parts.push(Part::Word(1));
    parts.push(Part::Word(class_tag(candidate.class())));
    match candidate.reason() {
        None => parts.push(Part::Word(0)),
        Some(reason) => {
            parts.push(Part::Word(1));
            parts.push(Part::Word(reason as u128));
        }
    }
    fields(&mut parts, candidate.pre());
    fields(&mut parts, candidate.post());
    patches(&mut parts, candidate.patch());
    deliveries(&mut parts, candidate.effects());
    deliveries(&mut parts, candidate.outbox());
    #[cfg(verus_keep_ghost)]
    proof! {model::candidate_projection(candidate);}
    parts
}

/// Encode every component of the supplied candidate; this pure encoding grants no authority.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    (match result{Some(v)=>Some(v@),None=>None::<Seq<u8>>}) == model::encode(model::candidate(candidate)),
))]
pub fn candidate_bytes(candidate: &Candidate<'_>) -> Option<Vec<u8>> {
    let parts = candidate_parts(candidate);
    encode(&parts)
}
