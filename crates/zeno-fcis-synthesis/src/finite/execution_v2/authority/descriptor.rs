//! Complete pure projection of the actual immutable producer descriptor.
use super::super::composition::{
    Binding, Channel, Descriptor, Reason, Schema, Selector, TypedField,
};
#[cfg(verus_keep_ghost)]
use super::descriptor_spec as model;
use super::{canonical::Part, metadata};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::schema(value),))]
pub(super) fn schema<'a>(parts: &mut Vec<Part<'a>>, value: Schema<'a>) {
    match value {
        Schema::Leaf(leaf) => {
            parts.push(Part::Word(0));
            metadata::leaf(parts, leaf);
        }
        Schema::Record(fields) => {
            parts.push(Part::Word(1));
            metadata::input_fields(parts, fields);
        }
    }
    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::schema);}
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::selector(value),))]
pub(super) fn selector<'a>(parts: &mut Vec<Part<'a>>, value: Selector) {
    match value {
        Selector::Root => parts.push(Part::Word(0)),
        Selector::Field(id) => {
            parts.push(Part::Word(1));
            parts.push(Part::Word(id as u128));
        }
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::bindings(values@),))]
pub(super) fn bindings<'a>(parts: &mut Vec<Part<'a>>, values: &[Binding]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,parts@==initial+seq![Part::Word(values.len() as u128)]+model::bindings_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        metadata::source(parts, &values[i].source);
        selector(parts, values[i].selector);
        i += 1;
    }

    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::bindings);}
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::reasons(values@),))]
pub(super) fn reasons<'a>(parts: &mut Vec<Part<'a>>, values: &[Reason]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,parts@==initial+seq![Part::Word(values.len() as u128)]+model::reasons_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].id as u128));
        metadata::class(parts, &values[i].class);
        i += 1;
    }

    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::reasons);}
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::typed_fields(values@),))]
pub(super) fn typed_fields<'a>(parts: &mut Vec<Part<'a>>, values: &[TypedField<'a>]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,parts@==initial+seq![Part::Word(values.len() as u128)]+model::typed_fields_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].field as u128));
        metadata::domain(parts, &values[i].domain);
        i += 1;
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::channels(values@),))]
pub(super) fn channels<'a>(parts: &mut Vec<Part<'a>>, values: &[Channel<'a>]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,parts@==initial+seq![Part::Word(values.len() as u128)]+model::channels_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        let c = &values[i];
        parts.push(Part::Word(c.id as u128));
        metadata::domain(parts, &c.destination);
        typed_fields(parts, c.payload);
        metadata::domain(parts, &c.idempotency);
        i += 1;
    }

    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::channels);}
}

/// Original schema and reviewed V2 policy remain exact independent fields.
/// Lowered names, ranges, variants, channels, laws, graph and limits are complete.
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::fields(d),))]
pub(super) fn append_descriptor<'a>(parts: &mut Vec<Part<'a>>, d: &Descriptor<'a>) {
    schema(parts, d.state);
    schema(parts, d.command);
    schema(parts, d.context);
    metadata::program(parts, &d.program);
    bindings(parts, d.bindings);
    metadata::leaves(parts, d.output_types);
    parts.push(Part::Word(d.decision_output as u128));
    metadata::branches(parts, d.branches);
    reasons(parts, d.reasons);
    channels(parts, d.channels);
    metadata::law_list(parts, d.laws);
    metadata::ids(parts, d.required);
    metadata::limits(parts, &d.limits);

    #[cfg(verus_keep_ghost)]
    proof! {reveal(model::fields);}
}
