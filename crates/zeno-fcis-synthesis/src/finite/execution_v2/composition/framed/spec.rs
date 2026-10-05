use super::*;
use vstd::prelude::*;
verus! {
pub(in super::super) open spec fn project(bytes:Seq<u8>,b:&FrameBinding,limits:Seq<u64>,used:Seq<u64>)->(Result<(Seq<u8>,Seq<u8>),Failure>,Seq<u64>){
    let c=super::super::super::spec::charge(limits,used,Resource::Byte,if bytes.len()<48{bytes.len() as u64}else{48});
    match c.0{Err(e)=>(Err(Failure::Budget(e)),c.1),Ok(())=>(match envelope::spec::frame(bytes,b.root,b.schema@,b.max_bytes){Ok(p)=>Ok(p),Err(e)=>Err(Failure::Envelope(e))},c.1)}
}
pub(in super::super) open spec fn payloads(r:super::super::spec::RawView,f:&Framing,limits:Seq<u64>,used:Seq<u64>)->(Result<super::super::spec::RawView,super::super::Failure>,Seq<u64>){
    let s=project(r.0,&f.state,limits,used);match s.0{Err(e)=>(Err(super::super::Failure::Frame(0,e)),s.1),Ok(sv)=>{
        let c=project(r.1,&f.command,limits,s.1);match c.0{Err(e)=>(Err(super::super::Failure::Frame(1,e)),c.1),Ok(cv)=>{
            let x=project(r.2,&f.context,limits,c.1);match x.0{Err(e)=>(Err(super::super::Failure::Frame(2,e)),x.1),Ok(xv)=>(Ok((sv.1,cv.1,xv.1)),x.1)}
        }}
    }}
}
pub(in super::super) open spec fn execution(d:&Descriptor,r:Raw,f:&Framing,limits:Seq<u64>,used:Seq<u64>,t:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:super::super::spec::Finished)->bool{
    if !admission::spec::admitted(d){finished==(Err(super::super::Failure::Metadata),used,t,diagnostics,reads)}else{
        let p=payloads(raw_view(r),f,limits,used);match p.0{Err(e)=>finished==(Err(e),p.1,t,diagnostics,reads),Ok(raw)=>{
            exists|actual:Raw|raw_view(actual)==raw&&super::super::spec::execution(d,actual,limits,p.1,t,diagnostics,reads,finished)
        }}
    }
}
pub(in super::super) open spec fn genesis(d:&Descriptor,r:Seq<u8>,b:&FrameBinding,limits:Seq<u64>,used:Seq<u64>,t:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:super::super::spec::Finished)->bool{
    if !admission::spec::admitted(d){finished==(Err(super::super::Failure::Metadata),used,t,diagnostics,reads)}else{
        let p=project(r,b,limits,used);match p.0{Err(e)=>finished==(Err(super::super::Failure::Frame(0,e)),p.1,t,diagnostics,reads),Ok(raw)=>super::super::spec::genesis(d,raw.1,limits,p.1,t,diagnostics,reads,finished)}
    }
}
}
