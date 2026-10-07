use super::*;
use ingress::spec::{ValueView, decoded, value as value_view};
use vstd::prelude::*;
verus! {
pub(in super::super) open spec fn field_code(fields:Seq<InputField>,codes:Seq<i64>,id:u16)->Option<i64>{
    if exists|i:int|admission::spec::first(fields,id,i){let i=choose|i:int|admission::spec::first(fields,id,i);if i<codes.len(){Some(codes[i])}else{None}}else{None}
}
pub(in super::super) open spec fn scalar(schema:Schema,decoded:&ingress::Decoded,selector:Selector)->Option<i64>{scalar_data(schema,ingress::spec::decoded(*decoded),selector)}
pub(in super::super) open spec fn scalar_data(schema:Schema,data:(ValueView,Seq<i64>),selector:Selector)->Option<i64>{match(schema,selector){
    (Schema::Leaf(_),Selector::Root)=>if data.1.len()==1{Some(data.1[0])}else{None},
    (Schema::Record(fields),Selector::Field(id))=>field_code(fields@,data.1,id),_=>None,
}}
pub(in super::super) open spec fn tuple(d:&Descriptor,s:&ingress::Decoded,c:&ingress::Decoded,x:&ingress::Decoded,n:nat)->Option<Seq<i64>>{tuple_data(d,decoded(*s),decoded(*c),decoded(*x),n)}
pub(in super::super) open spec fn tuple_data<'a>(d:&Descriptor,s:(ValueView<'a>,Seq<i64>),c:(ValueView<'a>,Seq<i64>),x:(ValueView<'a>,Seq<i64>),n:nat)->Option<Seq<i64>>
    recommends n<=d.bindings@.len(),decreases n,
{if n==0{Some(Seq::empty())}else{match tuple_data(d,s,c,x,(n-1) as nat){None=>None,Some(prior)=>{
    let b=d.bindings@[n as int-1];let v=match b.source{Source::State=>scalar_data(d.state,s,b.selector),Source::Command=>scalar_data(d.command,c,b.selector),Source::Context=>scalar_data(d.context,x,b.selector)};
    match v{Some(v)=>Some(prior.push(v)),None=>None}
}}}}
pub(in super::super) proof fn tuple_failed(d:&Descriptor,s:&ingress::Decoded,c:&ingress::Decoded,x:&ingress::Decoded,n:nat,total:nat)
    requires n<=total<=d.bindings@.len(),tuple(d,s,c,x,n).is_none(),ensures tuple(d,s,c,x,total).is_none(),decreases total-n,
{if total>n{tuple_failed(d,s,c,x,n,(total-1) as nat);}}
pub(in super::super) open spec fn atoms(types:Seq<InputLeaf>,codes:Seq<i64>,n:nat)->Option<Seq<Atom<'static>>>
    recommends n<=types.len(),decreases n,
{if types.len()!=codes.len(){None}else if n==0{Some(Seq::empty())}else{match atoms(types,codes,(n-1) as nat){None=>None,Some(prior)=>match ingress::spec::reify(types[n as int-1],codes[n as int-1]){None=>None,Some(v)=>Some(prior.push(v))}}}}
pub(in super::super) proof fn atoms_failed(types:Seq<InputLeaf>,codes:Seq<i64>,n:nat,total:nat)
    requires n<=total<=types.len(),atoms(types,codes,n).is_none(),ensures atoms(types,codes,total).is_none(),decreases total-n,
{if total>n{atoms_failed(types,codes,n,(total-1) as nat);}}
pub(in super::super) open spec fn input_view<'a>(v:decision::RootView<'_,'a>)->ValueView<'a>{match v{decision::RootView::Leaf(a)=>ValueView::Leaf(a),decision::RootView::Record(f)=>ValueView::Record(f@),}}
pub type ProducedView<'a>=(decision::spec::CandidateView<'a>,ValueView<'a>,ValueView<'a>);
pub(in super::super) open spec fn produced<'a>(p:Produced<'a>)->ProducedView<'a>{(decision::spec::candidate_view(p.candidate),value_view(p.command),value_view(p.context))}
pub type TraceView=(Seq<ReadAttempt>,Seq<Attempt>,Option<Seq<u64>>,Option<Seq<u64>>);
pub(in super::super) open spec fn optional_usage(u:Option<Usage>)->Option<Seq<u64>>{match u{Some(u)=>Some(u.view()),None=>None}}
pub(in super::super) open spec fn trace(t:&Trace)->TraceView{(t.reads@,t.attempts@,optional_usage(t.ingress),optional_usage(t.decision))}
pub type Finished<'a>=(Result<ProducedView<'a>,Failure>,Seq<u64>,TraceView);
pub(in super::super) open spec fn decision_step<'a>(d:&Descriptor<'a>,pre:Seq<Field<'a>>,command:ValueView<'a>,context:ValueView<'a>,atoms:Seq<Atom<'a>>,code:i128,limits:Seq<u64>,used:Seq<u64>,reads:Seq<ReadAttempt>,attempts:Seq<Attempt>,ingress:Option<Seq<u64>>,prior_decision:Option<Seq<u64>>,finished:Finished<'a>)->bool{
    exists|inputs:decision::Inputs<'_,'a>|inputs.state@==pre&&input_view(inputs.command)==command&&input_view(inputs.context)==context&&{
        let c=decision::spec::construct(inputs,atoms,code,d.branches@,limits,used);
        match c.0{Err(e)=>finished==(Err(Failure::Decision(e)),c.1,(reads,attempts+c.2,ingress,prior_decision)),Ok(candidate)=>
            finished==(Ok((candidate,command,context)),c.1,(reads,attempts+c.2,ingress,Some(c.1)))
        }
    }
}
pub(in super::super) open spec fn from_atoms<'a>(d:&Descriptor<'a>,state:ValueView<'a>,command:ValueView<'a>,context:ValueView<'a>,atoms:Seq<Atom<'a>>,limits:Seq<u64>,used:Seq<u64>,t:TraceView,finished:Finished<'a>)->bool{
    if d.decision_output>=atoms.len(){finished==(Err(Failure::Output),used,t)}else{match atoms[d.decision_output as int]{
        Atom::I128(code)=>match state{ValueView::Leaf(_)=>finished==(Err(Failure::Binding),used,t),ValueView::Record(pre)=>decision_step(d,pre,command,context,atoms,code,limits,used,t.0,t.1,t.2,t.3,finished),},
        _=>finished==(Err(Failure::Output),used,t),
    }}
}
pub(in super::super) open spec fn after_ingress<'a>(d:&Descriptor<'a>,s:(ValueView<'a>,Seq<i64>),c:(ValueView<'a>,Seq<i64>),x:(ValueView<'a>,Seq<i64>),limits:Seq<u64>,used:Seq<u64>,t:TraceView,finished:Finished<'a>)->bool{
    {let ingress=used;
        let tr=(t.0,t.1,Some(ingress),t.3);
        match tuple_data(d,s,c,x,d.bindings@.len()){
            None=>finished==(Err(Failure::Binding),used,tr),Some(input)=>{
                let e=super::super::super::spec::execution(d.program.inputs@,d.program.outputs@,d.program.nodes@,d.program.roots@,input,limits,used);
                match e.0{Err(error)=>finished==(Err(Failure::Execution(error)),e.1,tr),Ok(output)=>match atoms(d.output_types@,output,d.output_types@.len()){
                    None=>finished==(Err(Failure::Output),e.1,tr),Some(a)=>{
                        from_atoms(d,s.0,c.0,x.0,a,limits,e.1,tr,finished)
                    }
                }}
            }
        }
    }
}
pub(in super::super) open spec fn produce<'a>(d:&Descriptor<'a>,raw:Raw,limits:Seq<u64>,used:Seq<u64>,t:TraceView,finished:Finished<'a>)->bool{
    if !admission::spec::admitted(d){finished==(Err(Failure::Metadata),used,t)}else{
        let s=ingress::spec::project(raw.state@,d.state,0,limits,used);let st=(t.0+s.2,t.1,t.2,t.3);
        match s.0{Err(e)=>finished==(Err(Failure::Ingress(0,e)),s.1,st),Ok(state)=>{
            let c=ingress::spec::project(raw.command@,d.command,1,limits,s.1);let ct=(st.0+c.2,t.1,t.2,t.3);
            match c.0{Err(e)=>finished==(Err(Failure::Ingress(1,e)),c.1,ct),Ok(command)=>{
                let x=ingress::spec::project(raw.context@,d.context,2,limits,c.1);let xt=(ct.0+x.2,t.1,t.2,t.3);
                match x.0{Err(e)=>finished==(Err(Failure::Ingress(2,e)),x.1,xt),Ok(context)=>after_ingress(d,state,command,context,limits,x.1,xt,finished)}
            }}
        }}
    }
}
}
