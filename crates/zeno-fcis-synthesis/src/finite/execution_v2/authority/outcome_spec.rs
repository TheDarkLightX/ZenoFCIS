//! Exact complete reports and persisted subjects of actual opaque core outcomes.
use super::super::{
    composition::{self, Outcome, ReadAttempt, Selector},
    decision, laws,
};
use super::{canonical::Part, framing::Kind, refusal::Refusal, spec as encoding};
use alloc::vec::Vec;
use vstd::prelude::*;
verus! {
pub type Trace=(Seq<ReadAttempt>,Seq<decision::Attempt>,Option<Seq<u64>>,Option<Seq<u64>>);
pub type CoreView<'a>=(Result<decision::spec::CandidateView<'a>,composition::Failure>,(Seq<u8>,Seq<u8>,Seq<u8>),Trace,Seq<u64>,Seq<laws::Diagnostic>,Seq<laws::ReadAttempt>);
pub type EvaluationView<'a>=(CoreView<'a>,Result<Seq<u8>,Refusal>);

pub open spec fn read<'a>(r:ReadAttempt)->Seq<Part<'a>>{
    seq![Part::Word(r.source as u128)]+match r.selector{
        Selector::Root=>seq![Part::Word(0)],
        Selector::Field(id)=>seq![Part::Word(1),Part::Word(id as u128)],

    }+seq![Part::Word(r.permitted as u128)]
}
pub open spec fn reads_prefix<'a>(values:Seq<ReadAttempt>,n:nat)->Seq<Part<'a>>
    recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{reads_prefix(values,(n-1) as nat)+read(values[n-1])}}
pub open spec fn reads<'a>(values:Seq<ReadAttempt>)->Seq<Part<'a>>{
    seq![Part::Word(values.len() as u128)]+reads_prefix(values,values.len())
}
pub open spec fn reports<'a>(trace:Trace,usage:Seq<u64>,diagnostics:Seq<laws::Diagnostic>,law_reads:Seq<laws::ReadAttempt>)->Seq<Part<'a>>{
    encoding::optional_usage(trace.2)+encoding::optional_usage(trace.3)+encoding::usage_view(usage)+reads(trace.0)+encoding::decision_attempts(trace.1)+encoding::diagnostics(diagnostics)+encoding::law_reads(law_reads)
}
/// One uniform tagged artifact layout for both invocation kinds; genesis
/// reports its identity candidate and empty decision lanes exactly like a transition.
pub open spec fn parts<'a>(v:CoreView<'a>,kind:Kind)->Option<Seq<Part<'a>>>{
    match v.0{Err(_)=>None,Ok(candidate)=>Some(seq![Part::Word(0x5a4f5532),Part::Word(1),Part::Word(match kind{Kind::Genesis=>0,Kind::Transition=>1})]+
        encoding::candidate_projection_parts(candidate)+reports(v.2,v.3,v.4,v.5))}
}
pub open spec fn seal<'a>(v:CoreView<'a>,kind:Kind,identity:Seq<u8>)->Result<Seq<u8>,Refusal>{
    match v.0{Err(e)=>Err(Refusal::Core(e)),Ok(_)=>match encoding::encode(parts(v,kind).unwrap()){
        None=>Err(Refusal::Encoding),Some(artifact)=>frame(kind,identity,v.1.0,v.1.1,v.1.2,artifact)
    }}
}
pub open spec fn frame(kind:Kind,identity:Seq<u8>,state:Seq<u8>,command:Seq<u8>,context:Seq<u8>,artifact:Seq<u8>)->Result<Seq<u8>,Refusal>{
    match encoding::subject(kind,identity,state,command,context,artifact){None=>Err(Refusal::Encoding),Some(bytes)=>Ok(bytes)}
}
pub open spec fn replay(actual:Result<Seq<u8>,Refusal>,expected:Seq<u8>)->Result<Seq<u8>,Refusal>{
    match actual{Err(e)=>Err(e),Ok(bytes)=>if bytes==expected{Ok(bytes)}else{Err(Refusal::ReplayMismatch)}}
}
pub open spec fn evaluation_result<'a>(v:EvaluationView<'a>)->Result<decision::spec::CandidateView<'a>,Refusal>{
    match v.1{Err(e)=>Err(e),Ok(_)=>match v.0.0{Ok(c)=>Ok(c),Err(e)=>Err(Refusal::Core(e))}}
}
pub open spec fn candidate_result<'a>(r:Result<&decision::Candidate<'a>,Refusal>)->Result<decision::spec::CandidateView<'a>,Refusal>{
    match r{Ok(c)=>Ok(composition::candidate_result(Ok(c)).unwrap()),Err(e)=>Err(e)}
}
pub open spec fn bytes_result(r:Result<&[u8],Refusal>)->Result<Seq<u8>,Refusal>{match r{Ok(b)=>Ok(b@),Err(e)=>Err(e)}}
pub open spec fn owned_bytes_result(r:Result<Vec<u8>,Refusal>)->Result<Seq<u8>,Refusal>{match r{Ok(b)=>Ok(b@),Err(e)=>Err(e)}}
pub open spec fn sealed(outcome:&Outcome,identity:Seq<u8>)->Result<Seq<u8>,Refusal>{seal(outcome.view(),outcome.kind_view(),identity)}
}
