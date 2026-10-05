//! Exact closed raw-to-decision composition semantics.
use super::*;
use vstd::prelude::*;
verus! {
pub type RawView=(Seq<u8>,Seq<u8>,Seq<u8>);
pub type Finished<'a>=(Result<decision::spec::CandidateView<'a>,Failure>,Seq<u64>,producer::spec::TraceView,Seq<laws::Diagnostic>,Seq<laws::ReadAttempt>);
pub type OutcomeView<'a>=(Result<decision::spec::CandidateView<'a>,Failure>,RawView,producer::spec::TraceView,Seq<u64>,Seq<laws::Diagnostic>,Seq<laws::ReadAttempt>);
pub type GenesisView=(Result<(),Failure>,Seq<u8>,producer::spec::TraceView,Seq<u64>,Seq<laws::Diagnostic>,Seq<laws::ReadAttempt>);
pub(super) open spec fn raw(r:Raw)->RawView{(r.state@,r.command@,r.context@)}
pub(super) open spec fn borrowed_result<'a>(r:Result<&Candidate<'a>,Failure>)->Result<decision::spec::CandidateView<'a>,Failure>{match r{Ok(c)=>Ok(decision::spec::candidate_view(*c)),Err(e)=>Err(e)}}
pub(super) proof fn candidate_normalizer<'a>(r:Result<&Candidate<'a>,Failure>)
    ensures candidate_result(r)==borrowed_result(r),
{
    if let Ok(c)=r{
        let v=c.view();
        assert(v.5.map(|_:int,d:decision::Delivery|(d.ordinal,d.channel,d.destination,d.payload@,d.idempotency))=~=decision::spec::candidate_view(*c).5);
        assert(v.6.map(|_:int,d:decision::Delivery|(d.ordinal,d.channel,d.destination,d.payload@,d.idempotency))=~=decision::spec::candidate_view(*c).6);
    }
}
/// The actual delivery lane as the shared candidate view.
pub(super) open spec fn delivery_tags<'a>(ds:Seq<decision::Delivery<'a>>)->Seq<decision::spec::DeliveryView<'a>>{
    ds.map(|_:int,d:decision::Delivery<'a>|decision::spec::delivery_view(d))
}
/// One law frame borrowing exactly the produced decision, trace and usage.
pub(super) open spec fn transition_frame<'a>(f:&laws::Frame<'a>,p:producer::spec::ProducedView<'a>,t:producer::spec::TraceView,usage:Seq<u64>)->bool{
    match f{
        laws::Frame::Genesis{..}=>false,
        laws::Frame::Transition{pre,command,context,candidate:c}=>{
            c.class==p.0.0&&c.reason==p.0.1
            &&producer::spec::input_view(*pre)==ingress::spec::ValueView::Record(p.0.2)
            &&producer::spec::input_view(*command)==p.1
            &&producer::spec::input_view(*context)==p.2
            &&producer::spec::input_view(c.post)==ingress::spec::ValueView::Record(p.0.3)
            &&c.patch@==p.0.4
            &&delivery_tags(c.effects@)==p.0.5&&delivery_tags(c.outbox@)==p.0.6
            &&c.reads@==t.0&&c.attempts@==t.1&&c.usage.view()==usage
        }
    }
}
/// One genesis law frame borrowing exactly the admitted initial state.
pub(super) open spec fn genesis_frame(f:&laws::Frame,v:ingress::spec::ValueView)->bool{
    match f{laws::Frame::Genesis{initial}=>producer::spec::input_view(*initial)==v,_=>false}
}
/// The identity candidate a successful genesis yields: the unchanged
/// initial record, no reason, no patch and no deliveries.
pub(super) open spec fn identity_candidate<'a>(v:ingress::spec::ValueView<'a>)->decision::spec::CandidateView<'a>{
    let fields=match v{ingress::spec::ValueView::Record(f)=>f,_=>Seq::<Field>::empty()};
    (decision::Class::Accept,None,fields,fields,Seq::<decision::Patch>::empty(),Seq::<decision::spec::DeliveryView>::empty(),Seq::<decision::spec::DeliveryView>::empty())
}
/// The law engine run over exactly the transition frame the producer implies.
pub(super) open spec fn frame_evaluated(d:&Descriptor,p:producer::spec::ProducedView<'_>,t:producer::spec::TraceView,usage:Seq<u64>,limits:Seq<u64>,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:(Result<(),laws::Failure>,Seq<u64>,Seq<laws::Diagnostic>,Seq<laws::ReadAttempt>))->bool{
    exists|f:&laws::Frame|transition_frame(f,p,t,usage)&&
        finished==laws::law_execution(d.laws@,d.required@,f,limits,usage,diagnostics,reads)
}
/// The law engine run over exactly the admitted genesis frame.
pub(super) open spec fn genesis_evaluated(d:&Descriptor,v:ingress::spec::ValueView,limits:Seq<u64>,used:Seq<u64>,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:(Result<(),laws::Failure>,Seq<u64>,Seq<laws::Diagnostic>,Seq<laws::ReadAttempt>))->bool{
    exists|f:&laws::Frame|genesis_frame(f,v)&&
        finished==laws::law_execution(d.laws@,d.required@,f,limits,used,diagnostics,reads)
}
pub(super) open spec fn after_producer(d:&Descriptor,middle:producer::spec::Finished,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,limits:Seq<u64>,finished:Finished)->bool{
    match middle.0{Err(e)=>finished==(Err(e),middle.1,middle.2,diagnostics,reads),Ok(p)=>{
        if !schema_validation::spec::candidate(d,p.0){finished==(Err(Failure::Schema),middle.1,middle.2,diagnostics,reads)}else{
            exists|r:Result<(),laws::Failure>|frame_evaluated(d,p,middle.2,middle.1,limits,diagnostics,reads,(r,finished.1,finished.3,finished.4))&&
                finished.0==(match r{Ok(())=>Ok(p.0),Err(e)=>Err(Failure::Law(e))})&&finished.2==middle.2
        }
    }}
}
pub(super) open spec fn execution(d:&Descriptor,r:Raw,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:Finished)->bool{
    exists|middle:producer::spec::Finished|producer::spec::produce(d,r,limits,used,trace,middle)&&after_producer(d,middle,diagnostics,reads,limits,finished)
}
pub(super) open spec fn genesis(d:&Descriptor,initial:Seq<u8>,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:Finished)->bool{
    if !admission::spec::admitted(d){finished==(Err(Failure::Metadata),used,trace,diagnostics,reads)}else{
        let s=ingress::spec::project(initial,d.state,0,limits,used);
        let t=(trace.0+s.2,trace.1,trace.2,trace.3);
        match s.0{Err(e)=>finished==(Err(Failure::Ingress(0,e)),s.1,t,diagnostics,reads),Ok(decoded)=>{
            exists|r:Result<(),laws::Failure>|genesis_evaluated(d,decoded.0,limits,s.1,diagnostics,reads,(r,finished.1,finished.3,finished.4))&&
                finished.0==(match r{Ok(())=>Ok(identity_candidate(decoded.0)),Err(e)=>Err(Failure::Law(e))})&&finished.2==(t.0,t.1,Some(s.1),t.3)
        }}
    }
}
/// One actual run dispatched by invocation kind; genesis ignores the absent lanes.
pub(super) open spec fn evaluated(d:&Descriptor,kind:Kind,r:Raw,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:Finished)->bool{
    match kind{
        Kind::Genesis=>genesis(d,r.state@,limits,used,trace,diagnostics,reads,finished),
        Kind::Transition=>execution(d,r,limits,used,trace,diagnostics,reads,finished),
    }
}
}
