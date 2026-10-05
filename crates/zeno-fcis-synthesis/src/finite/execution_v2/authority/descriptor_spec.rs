//! Every actual lowered policy field, plus original schema/contract and roots.
use super::super::composition::{
    Binding, Channel, Descriptor, Reason, Schema, Selector, TypedField,
};
use super::{canonical::Part, metadata_spec as metadata};
use vstd::prelude::*;
verus! {
#[verifier::opaque]
pub open spec fn schema<'a>(s:Schema<'a>)->Seq<Part<'a>>{match s{Schema::Leaf(leaf)=>seq![Part::Word(0)]+metadata::leaf(*leaf),Schema::Record(fields)=>seq![Part::Word(1)]+metadata::input_fields(fields@),}}
pub open spec fn selector<'a>(s:Selector)->Seq<Part<'a>>{match s{Selector::Root=>seq![Part::Word(0)],Selector::Field(id)=>seq![Part::Word(1),Part::Word(id as u128)],}}
pub open spec fn bindings_prefix<'a>(values:Seq<Binding>,n:nat)->Seq<Part<'a>>
    recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{let b=values[n-1];bindings_prefix(values,(n-1) as nat)+metadata::source(b.source)+selector(b.selector)}}
#[verifier::opaque]
pub open spec fn bindings<'a>(values:Seq<Binding>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+bindings_prefix(values,values.len())}
pub open spec fn reasons_prefix<'a>(values:Seq<Reason>,n:nat)->Seq<Part<'a>>
    recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{let r=values[n-1];reasons_prefix(values,(n-1) as nat)+seq![Part::Word(r.id as u128)]+metadata::class(r.class)}}
#[verifier::opaque]
pub open spec fn reasons<'a>(values:Seq<Reason>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+reasons_prefix(values,values.len())}
pub open spec fn typed_fields_prefix<'a>(values:Seq<TypedField<'a>>,n:nat)->Seq<Part<'a>>
    recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{let f=values[n-1];typed_fields_prefix(values,(n-1) as nat)+seq![Part::Word(f.field as u128)]+metadata::domain(f.domain)}}
pub open spec fn typed_fields<'a>(values:Seq<TypedField<'a>>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+typed_fields_prefix(values,values.len())}
pub open spec fn channels_prefix<'a>(values:Seq<Channel<'a>>,n:nat)->Seq<Part<'a>>
    recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{let c=values[n-1];channels_prefix(values,(n-1) as nat)+seq![Part::Word(c.id as u128)]+metadata::domain(c.destination)+typed_fields(c.payload@)+metadata::domain(c.idempotency)}}
#[verifier::opaque]
pub open spec fn channels<'a>(values:Seq<Channel<'a>>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+channels_prefix(values,values.len())}
#[verifier::opaque]
pub open spec fn fields<'a>(d:&Descriptor<'a>)->Seq<Part<'a>>{
    schema(d.state)+schema(d.command)+schema(d.context)+metadata::program(d.program)+bindings(d.bindings@)+metadata::leaves(d.output_types@)+seq![Part::Word(d.decision_output as u128)]+
    metadata::branches(d.branches@)+reasons(d.reasons@)+channels(d.channels@)+metadata::law_list(d.laws@)+metadata::ids(d.required@)+metadata::limits(d.limits)
}
}
