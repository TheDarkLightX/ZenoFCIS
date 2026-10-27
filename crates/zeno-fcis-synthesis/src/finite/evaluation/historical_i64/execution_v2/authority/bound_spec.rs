//! Exact recomputation and comparison relations, with no supplied success judgment.
use super::super::{
    composition::{self, Descriptor, FrameBinding, Framing, Kind, Raw},
    laws,
};
use super::{Refusal, outcome_spec as outcome};
use vstd::prelude::*;
verus! {
pub type BoundView<'a>=(&'a Descriptor<'a>,Framing,Seq<u8>,Seq<(u32,u32,u32)>);
pub(super) open spec fn construction<'a>(d:&'a Descriptor<'a>,f:Framing,policy:Seq<u8>,links:Seq<(u32,u32,u32)>)->Result<BoundView<'a>,Refusal>{
    if !composition::descriptor_admitted(d){Err(Refusal::Core(composition::Failure::Metadata))}
    else{match super::spec::identity(policy,super::evaluator::EVALUATOR@){
        None=>Err(Refusal::Encoding),Some(identity)=>Ok((d,f,identity,links)),
    }}
}
pub open spec fn binding_bytes(value:&FrameBinding)->Seq<u8>{
    seq![0u8]+super::spec::word(value.root as u128)+seq![1u8]+super::spec::word(32)+value.schema@+seq![0u8]+super::spec::word(value.max_bytes as u128)
}
pub open spec fn framing_bytes(value:&Framing)->Seq<u8>{
    seq![0u8]+super::spec::word(0x5a465232)+seq![0u8]+super::spec::word(1)+binding_bytes(&value.state)+binding_bytes(&value.command)+binding_bytes(&value.context)
}
pub open spec fn compared(actual:Result<Seq<u8>,Refusal>,expected:Option<Seq<u8>>)->Result<Seq<u8>,Refusal>{
    match expected{None=>actual,Some(bytes)=>outcome::replay(actual,bytes)}
}
pub open spec fn transition<'a>(bound:BoundView,raw:Raw,finished:outcome::EvaluationView<'a>,expected:Option<Seq<u8>>)->bool{
    let v=finished.0;
    v.1==composition::raw_view(raw)
    &&composition::framed_execution(bound.0,raw,&bound.1,bound.0.limits.view(),Seq::new(8,|_:int|0u64),
        (Seq::empty(),Seq::empty(),None,None),Seq::<laws::Diagnostic>::empty(),Seq::<laws::ReadAttempt>::empty(),
        (v.0,v.3,v.2,v.4,v.5))
    &&finished.1==compared(outcome::seal(v,Kind::Transition,bound.2),expected)
}
pub open spec fn genesis(bound:BoundView,original:Seq<u8>,finished:outcome::EvaluationView,expected:Option<Seq<u8>>)->bool{
    let v=finished.0;
    v.1==(original,Seq::<u8>::empty(),Seq::<u8>::empty())
    &&composition::framed_genesis(bound.0,original,&bound.1.state,bound.0.limits.view(),Seq::new(8,|_:int|0u64),
        (Seq::empty(),Seq::empty(),None,None),Seq::<laws::Diagnostic>::empty(),Seq::<laws::ReadAttempt>::empty(),
        (v.0,v.3,v.2,v.4,v.5))
    &&finished.1==compared(outcome::seal(v,Kind::Genesis,bound.2),expected)
}
}
