//! Version-two policy fields for the same full-width numeric graph and value domains.
use super::super::super::{
    evaluation::typed::NumericOp,
    value::{Domain, IntegerWidth},
};
use super::super::{
    composition::{NumericDescriptor, Projection, Schema},
    input_view::Field,
    numeric::NumericProgram,
};
use super::{canonical::Part, descriptor, metadata};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use {super::numeric_metadata_spec as model, vstd::prelude::*};
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==model::width(value),))]
fn width(value: IntegerWidth) -> u128 {
    match value {
        IntegerWidth::Signed64 => 0,
        IntegerWidth::Signed128 => 1,
        IntegerWidth::Unsigned128 => 2,
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::op(op),))]
fn op<'a>(parts: &mut Vec<Part<'a>>, op: NumericOp) {
    match op {
        NumericOp::Input(i) => {
            parts.push(Part::Word(0));
            parts.push(Part::Word(i as u128));
        }
        NumericOp::Bool(v) => {
            parts.push(Part::Word(1));
            parts.push(Part::Word(if v { 1 } else { 0 }));
        }
        NumericOp::I128(v) => {
            parts.push(Part::Word(2));
            parts.push(Part::Word(v as u128));
        }
        NumericOp::U128(v) => {
            parts.push(Part::Word(3));
            parts.push(Part::Word(v));
        }
        NumericOp::Add(w, a, b) => {
            parts.push(Part::Word(4));
            parts.push(Part::Word(width(w)));
            parts.push(Part::Word(a as u128));
            parts.push(Part::Word(b as u128));
        }
        NumericOp::Sub(w, a, b) => {
            parts.push(Part::Word(5));
            parts.push(Part::Word(width(w)));
            parts.push(Part::Word(a as u128));
            parts.push(Part::Word(b as u128));
        }
        NumericOp::Eq(a, b) => {
            parts.push(Part::Word(6));
            parts.push(Part::Word(a as u128));
            parts.push(Part::Word(b as u128));
        }
        NumericOp::Lt(a, b) => {
            parts.push(Part::Word(7));
            parts.push(Part::Word(a as u128));
            parts.push(Part::Word(b as u128));
        }
        NumericOp::And(a, b) => {
            parts.push(Part::Word(8));
            parts.push(Part::Word(a as u128));
            parts.push(Part::Word(b as u128));
        }
        NumericOp::Not(a) => {
            parts.push(Part::Word(9));
            parts.push(Part::Word(a as u128));
        }
        NumericOp::Select(c, a, b) => {
            parts.push(Part::Word(10));
            parts.push(Part::Word(c as u128));
            parts.push(Part::Word(a as u128));
            parts.push(Part::Word(b as u128));
        }
        NumericOp::MulDivFloor(a, b, den) => {
            parts.push(Part::Word(11));
            parts.push(Part::Word(a as u128));
            parts.push(Part::Word(b as u128));
            parts.push(Part::Word(den as u128));
        }
        NumericOp::MulDivCeil(a, b, den) => {
            parts.push(Part::Word(12));
            parts.push(Part::Word(a as u128));
            parts.push(Part::Word(b as u128));
            parts.push(Part::Word(den as u128));
        }
        NumericOp::ProductsGe(left, right) => {
            parts.push(Part::Word(13));
            parts.push(Part::Word(left[0] as u128));
            parts.push(Part::Word(left[1] as u128));
            parts.push(Part::Word(left[2] as u128));
            parts.push(Part::Word(right[0] as u128));
            parts.push(Part::Word(right[1] as u128));
            parts.push(Part::Word(right[2] as u128));
        }
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::domains(values@),))]
fn domains<'a>(parts: &mut Vec<Part<'a>>, values: &[Domain<'a>]) {
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),parts@==old(parts)@+seq![Part::Word(values.len() as u128)]+model::domains_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        metadata::domain(parts, &values[i]);
        i += 1;
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::input_fields(values@),))]
fn input_fields<'a>(parts: &mut Vec<Part<'a>>, values: &[Field<Domain<'a>>]) {
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),parts@==old(parts)@+seq![Part::Word(values.len() as u128)]+model::input_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].id as u128));
        metadata::domain(parts, &values[i].leaf);
        i += 1;
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::original(checked,root,projection),))]
pub(super) fn original<'a>(
    parts: &mut Vec<Part<'a>>,
    checked: &crate::finite::historical_i64::canonical_v2::schema::Checked<'a>,
    root: u32,
    projection: Projection<'a>,
) {
    parts.push(Part::Word(2));
    let bytes = checked.original();
    #[cfg(verus_keep_ghost)]
    proof! {crate::finite::historical_i64::execution_v2::compound::spec::slice_equal(checked.view().0,bytes);}
    parts.push(Part::Bytes(bytes));
    parts.push(Part::Word(root as u128));
    match projection {
        Projection::None => parts.push(Part::Word(0)),
        Projection::Root(d) => {
            parts.push(Part::Word(1));
            metadata::domain(parts, d);
        }
        Projection::Fields(f) => {
            parts.push(Part::Word(2));
            input_fields(parts, f);
        }
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::schema(value),))]
fn schema<'a>(parts: &mut Vec<Part<'a>>, value: Schema<'a, Domain<'a>>) {
    match value {
        Schema::Original {
            checked,
            root,
            projection,
        } => original(parts, checked, root, projection),
        Schema::Leaf(l) => {
            parts.push(Part::Word(0));
            metadata::domain(parts, l);
        }
        Schema::Record(f) => {
            parts.push(Part::Word(1));
            input_fields(parts, f);
        }
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::program(p),))]
fn program<'a>(parts: &mut Vec<Part<'a>>, p: NumericProgram<'a>) {
    domains(parts, p.inputs);
    domains(parts, p.outputs);
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost before=parts@;}
    parts.push(Part::Word(p.nodes.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=p.nodes.len(),parts@==before+seq![Part::Word(p.nodes.len() as u128)]+model::ops_prefix(p.nodes@,i as nat),decreases p.nodes.len()-i,))]
    while i < p.nodes.len() {
        op(parts, p.nodes[i]);
        i += 1;
    }
    metadata::variants_ids(parts, p.roots);
}
#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::fields(d),))]
pub(super) fn append<'a>(parts: &mut Vec<Part<'a>>, d: &NumericDescriptor<'a>) {
    schema(parts, d.state);
    schema(parts, d.command);
    schema(parts, d.context);
    program(parts, d.program);
    descriptor::bindings(parts, d.bindings);
    domains(parts, d.output_types);
    parts.push(Part::Word(d.decision_output as u128));
    metadata::branches(parts, d.branches);
    descriptor::reasons(parts, d.reasons);
    descriptor::channels(parts, d.channels);
    metadata::law_list(parts, d.laws);
    metadata::ids(parts, d.required);
    metadata::limits(parts, &d.limits);
}
