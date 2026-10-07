//! Exact original-wire publication of the actual immutable bound evaluation.
use super::super::{bound_spec, outcome_spec as audit, spec as encoding};
use super::*;
use crate::finite::canonical_v2::output::spec as wire;
use crate::finite::execution_v2::{composition, decision};
use vstd::prelude::*;
verus! {
pub type WireView=(u32,u32,u32,u32,Seq<u8>,Seq<u8>,Seq<u8>);
pub type ArtifactsView=(Seq<u8>,Seq<WireView>,Seq<WireView>,Seq<u8>);
pub type PublicationView<'a>=(audit::EvaluationView<'a>,Seq<u8>,ArtifactsView);
pub type OutcomeView<'a>=(audit::EvaluationView<'a>,Result<Option<(Seq<u8>,ArtifactsView)>,Refusal>);

pub open spec fn link(links:Seq<(u32,u32,u32)>,id:u32,n:nat)->Option<(u32,u32)>
    recommends n<=links.len(),decreases n,
{if n==0{None}else{match link(links,id,(n-1) as nat){Some(v)=>Some(v),None=>if links[n-1].0==id{Some((links[n-1].1,links[n-1].2))}else{None}}}}
pub proof fn link_found(links:Seq<(u32,u32,u32)>,id:u32,n:nat,total:nat)
    requires n<=total<=links.len(),link(links,id,n).is_some(),
    ensures link(links,id,n)==link(links,id,total),decreases total-n,
{if total>n{link_found(links,id,n,(total-1) as nat);}}
pub open spec fn lift(r:Result<Seq<u8>,output::Failure>)->Result<Seq<u8>,Refusal>{
    match r{Ok(b)=>Ok(b),Err(e)=>Err(Refusal::Output(e))}
}
pub open spec fn state(fields:Seq<decision::Field>,binding:FrameBinding)->Result<Seq<u8>,Refusal>{
    if binding.max_bytes as int>usize::MAX as int{Err(Refusal::Encoding)}else{
        match wire::record_result(fields,binding.max_bytes as usize,fields.len()){
            Err(e)=>Err(Refusal::Output(e)),
            Ok(payload)=>lift(wire::envelope_result(binding.root,binding.schema@,payload,binding.max_bytes as usize)),
        }
    }
}
pub open spec fn delivery(d:decision::spec::DeliveryView,links:Seq<(u32,u32,u32)>)->Result<WireView,Refusal>{
    match link(links,d.1,links.len()){
        None=>Err(Refusal::Channel),Some((a,b))=>match wire::atom_result(d.2,usize::MAX){
            Err(e)=>Err(Refusal::Output(e)),Ok(destination)=>match wire::record_result(d.3,usize::MAX,d.3.len()){
                Err(e)=>Err(Refusal::Output(e)),Ok(payload)=>match wire::atom_result(d.4,usize::MAX){
                    Err(e)=>Err(Refusal::Output(e)),Ok(idempotency)=>Ok((d.0,d.1,a,b,destination,payload,idempotency)),
                }
            }
        }
    }
}
pub open spec fn deliveries(ds:Seq<decision::spec::DeliveryView>,links:Seq<(u32,u32,u32)>,n:nat)->Result<Seq<WireView>,Refusal>
    recommends n<=ds.len(),decreases n,
{if n==0{Ok(Seq::empty())}else{match deliveries(ds,links,(n-1) as nat){Err(e)=>Err(e),Ok(prefix)=>match delivery(ds[n-1],links){Err(e)=>Err(e),Ok(v)=>Ok(prefix.push(v))}}}}
pub proof fn deliveries_failure(ds:Seq<decision::spec::DeliveryView>,links:Seq<(u32,u32,u32)>,n:nat,total:nat)
    requires n<=total<=ds.len(),deliveries(ds,links,n).is_err(),
    ensures deliveries(ds,links,n)==deliveries(ds,links,total),decreases total-n,
{if total>n{deliveries_failure(ds,links,n,(total-1) as nat);}}
pub open spec fn delivery_tokens(v:WireView)->Seq<encoding::Token>{
    seq![encoding::Token::Word(v.0 as u128),encoding::Token::Word(v.1 as u128),
        encoding::Token::Word(v.2 as u128),encoding::Token::Word(v.3 as u128),
        encoding::Token::Bytes(v.4),encoding::Token::Bytes(v.5),encoding::Token::Bytes(v.6)]
}
pub open spec fn lane_prefix(ds:Seq<WireView>,n:nat)->Seq<encoding::Token>
    recommends n<=ds.len(),decreases n,
{if n==0{Seq::empty()}else{lane_prefix(ds,(n-1) as nat)+delivery_tokens(ds[n-1])}}
pub open spec fn lane(ds:Seq<WireView>)->Seq<encoding::Token>{seq![encoding::Token::Word(ds.len() as u128)]+lane_prefix(ds,ds.len())}
/// One tagged publication subject shape for both kinds; the kind word is the
/// invocation tag, genesis commits its original initial bytes as the post state.
pub open spec fn subject(kind:composition::Kind,audited:Seq<u8>,post:Seq<u8>,effects:Seq<WireView>,outbox:Seq<WireView>)->Result<Seq<u8>,Refusal>{
    match encoding::token_encode(seq![encoding::Token::Word(0x5a505532),encoding::Token::Word(1),
        encoding::Token::Word(match kind{composition::Kind::Genesis=>0,composition::Kind::Transition=>1}),
        encoding::Token::Bytes(audited),encoding::Token::Bytes(post)]+lane(effects)+lane(outbox)){
        None=>Err(Refusal::Encoding),Some(bytes)=>Ok(bytes),
    }
}
pub open spec fn prepared(e:audit::EvaluationView,kind:composition::Kind,binding:FrameBinding,links:Seq<(u32,u32,u32)>)->Result<Option<ArtifactsView>,Refusal>{
    match audit::evaluation_result(e){
        Err(error)=>Err(error),Ok(c)=>if c.0==decision::Class::Reject{Ok(None)}else{
            let post=match kind{composition::Kind::Genesis=>Ok(e.0.1.0),composition::Kind::Transition=>state(c.3,binding)};
            match post{Err(error)=>Err(error),Ok(post)=>
                match deliveries(c.5,links,c.5.len()){Err(error)=>Err(error),Ok(effects)=>
                    match deliveries(c.6,links,c.6.len()){Err(error)=>Err(error),Ok(outbox)=>
                        match subject(kind,e.1.unwrap(),post,effects,outbox){Err(error)=>Err(error),Ok(bytes)=>Ok(Some((post,effects,outbox,bytes)))}
                    }
                }
            }
        }}
}
pub open spec fn compared(r:Result<Option<ArtifactsView>,Refusal>,audit:Result<Seq<u8>,Refusal>,expected:Option<Seq<u8>>)->Result<Option<ArtifactsView>,Refusal>{
    match (r,expected){(Err(e),_)=>Err(e),(Ok(v),None)=>Ok(v),(Ok(v),Some(wanted))=>{
        let actual=match v{Some(a)=>Ok(a.3),None=>audit};
        match actual{Err(e)=>Err(e),Ok(bytes)=>if bytes==wanted{Ok(v)}else{Err(Refusal::ReplayMismatch)}}}}
}
pub open spec fn finished<'a>(e:audit::EvaluationView<'a>,kind:composition::Kind,identity:Seq<u8>,binding:FrameBinding,links:Seq<(u32,u32,u32)>,expected:Option<Seq<u8>>)->OutcomeView<'a>{
    (e,match compared(prepared(e,kind,binding,links),e.1,expected){Err(error)=>Err(error),Ok(None)=>Ok(None),Ok(Some(a))=>Ok(Some((identity,a)))})
}
pub open spec fn transition(bound:bound_spec::BoundView,raw:composition::Raw,r:OutcomeView,expected:Option<Seq<u8>>)->bool{
    bound_spec::transition(bound,raw,r.0,None)&&r==finished(r.0,composition::Kind::Transition,bound.2,bound.1.state,bound.3,expected)
}
pub open spec fn genesis(bound:bound_spec::BoundView,raw:Seq<u8>,r:OutcomeView<'_>,expected:Option<Seq<u8>>)->bool{
    bound_spec::genesis(bound,raw,r.0,None)&&r==finished(r.0,composition::Kind::Genesis,bound.2,bound.1.state,bound.3,expected)
}
}
