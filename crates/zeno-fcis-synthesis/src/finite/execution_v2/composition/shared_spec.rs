//! Mathematical composition over one closed descriptor projection.
//! The legacy model remains independently defined in the original spec modules.
use super::super::descriptor::spec as data;
use super::super::ingress::spec::ValueView;
use super::super::producer::spec::{Finished as ProducedFinished, TraceView};
use super::super::spec::{Finished, genesis_frame, identity_candidate, transition_frame};
use super::*;
use vstd::prelude::*;
verus! {
pub(in super::super) open spec fn first(fs:Seq<(u16,LeafView)>,id:u16,i:int)->bool {
    0<=i<fs.len()&&fs[i].0==id&&(forall|j:int|0<=j<i ==> fs[j].0!=id)
}
pub(in super::super) open spec fn scalar(schema:SchemaView,values:Seq<Atom<'static>>,selector:Selector)->Option<Atom<'static>> {
    match(input::spec::shape(schema),selector){
        (Shape::Leaf(_),Selector::Root)=>if values.len()==1{Some(values[0])}else{None},
        (Shape::Record(fs),Selector::Field(id))=>{
            let fs=input_view::typed::spec::fields(fs);
            if exists|i:int|first(fs,id,i){let i=choose|i:int|first(fs,id,i);if i<values.len(){Some(values[i])}else{None}}else{None}
        },_=>None,
    }
}
pub(in super::super) open spec fn tuple(d:&DescriptorProjection,s:Seq<Atom<'static>>,c:Seq<Atom<'static>>,x:Seq<Atom<'static>>,n:nat)->Option<Seq<Atom<'static>>>
    recommends n<=data::bindings(d)@.len(),decreases n,
{
    if n==0{Some(Seq::empty())}else{match tuple(d,s,c,x,(n-1) as nat){None=>None,Some(prior)=>{
        let b=data::bindings(d)[n as int-1];let v=match b.source{Source::State=>scalar(data::state(d),s,b.selector),Source::Command=>scalar(data::command(d),c,b.selector),Source::Context=>scalar(data::context(d),x,b.selector)};
        match v{Some(v)=>Some(prior.push(v)),None=>None}
    }}}
}
pub(in super::super) proof fn tuple_failed(d:&DescriptorProjection,s:Seq<Atom<'static>>,c:Seq<Atom<'static>>,x:Seq<Atom<'static>>,n:nat,total:nat)
    requires n<=total<=data::bindings(d)@.len(),tuple(d,s,c,x,n).is_none(),
    ensures tuple(d,s,c,x,total).is_none(),decreases total-n,
{if total>n{tuple_failed(d,s,c,x,n,(total-1) as nat);}}
pub(in super::super) open spec fn output_count(types:OutputTypes)->nat {match types{OutputTypes::Legacy(t)=>t@.len(),OutputTypes::Numeric(t)=>t@.len()}}
pub(in super::super) open spec fn output_prefix(types:OutputTypes,codes:Seq<Atom<'static>>,n:nat)->Option<Seq<Atom<'static>>>
    recommends n<=output_count(types),decreases n,
{
    if output_count(types)!=codes.len(){None}else if n==0{Some(Seq::empty())}else{
        match output_prefix(types,codes,(n-1) as nat){None=>None,Some(prior)=>{
            let leaf=match types{OutputTypes::Legacy(t)=>LeafView::Legacy(&t[(n-1) as int]),OutputTypes::Numeric(t)=>LeafView::Numeric(t[n as int-1])};
            match input::spec::reify(leaf,codes[n as int-1]){None=>None,Some(v)=>Some(prior.push(v))}
        }}
    }
}
pub(in super::super) open spec fn outputs(types:OutputTypes,codes:Seq<Atom<'static>>)->Option<Seq<Atom<'static>>>{output_prefix(types,codes,output_count(types))}
pub(in super::super) proof fn outputs_failed(types:OutputTypes,codes:Seq<Atom<'static>>,n:nat,total:nat)
    requires n<=total<=output_count(types),output_prefix(types,codes,n).is_none(),
    ensures output_prefix(types,codes,total).is_none(),decreases total-n,
{if total>n{outputs_failed(types,codes,n,(total-1) as nat);}}
pub(in super::super) open spec fn decision_step<'a>(d:&DescriptorProjection<'a>,pre:Seq<Field<'a>>,command:ValueView<'a>,context:ValueView<'a>,atoms:Seq<Atom<'a>>,code:i128,limits:Seq<u64>,used:Seq<u64>,reads:Seq<ReadAttempt>,attempts:Seq<Attempt>,ingress:Option<Seq<u64>>,prior_decision:Option<Seq<u64>>,finished:ProducedFinished<'a>)->bool{
    exists|inputs:decision::Inputs<'_,'a>|inputs.state@==pre&&producer::spec::input_view(inputs.command)==command&&producer::spec::input_view(inputs.context)==context&&{
        let c=decision::spec::construct(inputs,atoms,code,data::branches(d)@,limits,used);
        match c.0{Err(e)=>finished==(Err(Failure::Decision(e)),c.1,(reads,attempts+c.2,ingress,prior_decision)),Ok(candidate)=>
            finished==(Ok((candidate,command,context)),c.1,(reads,attempts+c.2,ingress,Some(c.1)))
        }
    }
}
pub(in super::super) open spec fn from_atoms<'a>(d:&DescriptorProjection<'a>,state:ValueView<'a>,command:ValueView<'a>,context:ValueView<'a>,atoms:Seq<Atom<'a>>,limits:Seq<u64>,used:Seq<u64>,t:TraceView,finished:ProducedFinished<'a>)->bool{
    if data::decision_output(d)>=atoms.len(){finished==(Err(Failure::Output),used,t)}else{match atoms[data::decision_output(d) as int]{
        Atom::I128(code)=>match state{ValueView::Leaf(_)=>finished==(Err(Failure::Binding),used,t),ValueView::Record(pre)=>decision_step(d,pre,command,context,atoms,code,limits,used,t.0,t.1,t.2,t.3,finished),},
        _=>finished==(Err(Failure::Output),used,t),
    }}
}
pub(in super::super) open spec fn after_ingress<'a>(d:&DescriptorProjection<'a>,s:(ValueView<'a>,Seq<Atom<'static>>),c:(ValueView<'a>,Seq<Atom<'static>>),x:(ValueView<'a>,Seq<Atom<'static>>),limits:Seq<u64>,used:Seq<u64>,t:TraceView,finished:ProducedFinished<'a>)->bool{
    {let ingress=used;
        let tr=(t.0,t.1,Some(ingress),t.3);
        match tuple(d,s.1,c.1,x.1,data::bindings(d)@.len()){
            None=>finished==(Err(Failure::Binding),used,tr),Some(input)=>{
                let e=numeric::spec::execution(data::program(d),input,limits,used);
                match e.0{Err(error)=>finished==(Err(Failure::Execution(numeric::spec::legacy_failure(error))),e.1,tr),Ok(output)=>match outputs(data::outputs(d),output){
                    None=>finished==(Err(Failure::Output),e.1,tr),Some(a)=>{
                        from_atoms(d,s.0,c.0,x.0,a,limits,e.1,tr,finished)
                    }
                }}
            }
        }
    }
}
pub(in super::super) open spec fn produce<'a>(d:&DescriptorProjection<'a>,raw:Raw,limits:Seq<u64>,used:Seq<u64>,t:TraceView,finished:ProducedFinished<'a>)->bool{
    if !d.admitted(){finished==(Err(Failure::Metadata),used,t)}else{
        let s=input::spec::project(raw.state@,data::state(d),0,limits,used);let st=(t.0+s.2,t.1,t.2,t.3);
        match s.0{Err(e)=>finished==(Err(Failure::Ingress(0,e)),s.1,st),Ok(state)=>{
            let c=input::spec::project(raw.command@,data::command(d),1,limits,s.1);let ct=(st.0+c.2,t.1,t.2,t.3);
            match c.0{Err(e)=>finished==(Err(Failure::Ingress(1,e)),c.1,ct),Ok(command)=>{
                let x=input::spec::project(raw.context@,data::context(d),2,limits,c.1);let xt=(ct.0+x.2,t.1,t.2,t.3);
                match x.0{Err(e)=>finished==(Err(Failure::Ingress(2,e)),x.1,xt),Ok(context)=>after_ingress(d,state,command,context,limits,x.1,xt,finished)}
            }}
        }}
    }
}

pub(in super::super) open spec fn channel<'a>(cs:Seq<Channel<'a>>,id:u32)->Option<Channel<'a>> {
    if exists|i:int|0<=i<cs.len()&&cs[i].id==id&&(forall|j:int|0<=j<i ==> cs[j].id!=id) {
        let i=choose|i:int|0<=i<cs.len()&&cs[i].id==id&&(forall|j:int|0<=j<i ==> cs[j].id!=id);Some(cs[i])
    }else{None}
}
pub(in super::super) open spec fn delivery(cs:Seq<Channel>,p:decision::spec::DeliveryView)->bool {
    match channel(cs,p.1){None=>false,Some(c)=>schema_validation::spec::atom(c.destination,p.2)&&schema_validation::spec::atom(c.idempotency,p.4)&&schema_validation::spec::payload(p.3,c.payload@)}
}
pub(in super::super) open spec fn deliveries(cs:Seq<Channel>,ps:Seq<decision::spec::DeliveryView>)->bool{forall|i:int|0<=i<ps.len() ==> delivery(cs,ps[i])}
pub(in super::super) open spec fn candidate(d:&DescriptorProjection,c:decision::spec::CandidateView)->bool{deliveries(data::channels(d)@,c.5)&&deliveries(data::channels(d)@,c.6)}
pub(in super::super) open spec fn frame_evaluated(d:&DescriptorProjection,p:producer::spec::ProducedView<'_>,t:producer::spec::TraceView,usage:Seq<u64>,limits:Seq<u64>,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:(Result<(),laws::Failure>,Seq<u64>,Seq<laws::Diagnostic>,Seq<laws::ReadAttempt>))->bool{
    exists|f:&laws::Frame|transition_frame(f,p,t,usage)&&
        finished==laws::law_execution(data::laws(d)@,data::required(d)@,f,limits,usage,diagnostics,reads)
}
/// The law engine run over exactly the admitted genesis frame.
pub(in super::super) open spec fn genesis_evaluated(d:&DescriptorProjection,v:ingress::spec::ValueView,limits:Seq<u64>,used:Seq<u64>,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:(Result<(),laws::Failure>,Seq<u64>,Seq<laws::Diagnostic>,Seq<laws::ReadAttempt>))->bool{
    exists|f:&laws::Frame|genesis_frame(f,v)&&
        finished==laws::law_execution(data::laws(d)@,data::required(d)@,f,limits,used,diagnostics,reads)
}
pub(in super::super) open spec fn after_producer(d:&DescriptorProjection,middle:producer::spec::Finished,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,limits:Seq<u64>,finished:Finished)->bool{
    match middle.0{Err(e)=>finished==(Err(e),middle.1,middle.2,diagnostics,reads),Ok(p)=>{
        if !candidate(d,p.0){finished==(Err(Failure::Schema),middle.1,middle.2,diagnostics,reads)}else{
            exists|r:Result<(),laws::Failure>|frame_evaluated(d,p,middle.2,middle.1,limits,diagnostics,reads,(r,finished.1,finished.3,finished.4))&&
                finished.0==(match r{Ok(())=>Ok(p.0),Err(e)=>Err(Failure::Law(e))})&&finished.2==middle.2
        }
    }}
}
pub(in super::super) open spec fn execution(d:&DescriptorProjection,r:Raw,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:Finished)->bool{
    exists|middle:producer::spec::Finished|produce(d,r,limits,used,trace,middle)&&after_producer(d,middle,diagnostics,reads,limits,finished)
}
pub(in super::super) open spec fn genesis(d:&DescriptorProjection,initial:Seq<u8>,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:Finished)->bool{
    if !d.admitted(){finished==(Err(Failure::Metadata),used,trace,diagnostics,reads)}else{
        let s=input::spec::project(initial,data::state(d),0,limits,used);
        let t=(trace.0+s.2,trace.1,trace.2,trace.3);
        match s.0{Err(e)=>finished==(Err(Failure::Ingress(0,e)),s.1,t,diagnostics,reads),Ok(decoded)=>{
            exists|r:Result<(),laws::Failure>|genesis_evaluated(d,decoded.0,limits,s.1,diagnostics,reads,(r,finished.1,finished.3,finished.4))&&
                finished.0==(match r{Ok(())=>Ok(identity_candidate(decoded.0)),Err(e)=>Err(Failure::Law(e))})&&finished.2==(t.0,t.1,Some(s.1),t.3)
        }}
    }
}
/// Close genesis with the actual decoded input and actual law-result witnesses.
/// Keep this structural step separate from the numeric interpreter's theories.
pub(in super::super) proof fn finish_genesis<'a>(d:&DescriptorProjection<'a>,initial:Seq<u8>,limits:Seq<u64>,used:Seq<u64>,trace:TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,decoded:(ValueView<'a>,Seq<Atom<'static>>),law:Result<(),laws::Failure>,finished:Finished<'a>)
    requires d.admitted(),
        input::spec::project(initial,data::state(d),0,limits,used).0==Ok(decoded),
        genesis_evaluated(d,decoded.0,limits,input::spec::project(initial,data::state(d),0,limits,used).1,diagnostics,reads,(law,finished.1,finished.3,finished.4)),
        finished.0==(match law{Ok(())=>Ok(identity_candidate(decoded.0)),Err(e)=>Err(Failure::Law(e))}),
        finished.2==(trace.0+input::spec::project(initial,data::state(d),0,limits,used).2,trace.1,Some(input::spec::project(initial,data::state(d),0,limits,used).1),trace.3),
    ensures genesis(d,initial,limits,used,trace,diagnostics,reads,finished),
{
    let s=input::spec::project(initial,data::state(d),0,limits,used);
    assert(s.0==Ok(decoded));
    match s.0 {
        Err(_) => { assert(false); },
        Ok(actual) => {
            assert(actual==decoded);
            assert(genesis_evaluated(d,actual.0,limits,s.1,diagnostics,reads,(law,finished.1,finished.3,finished.4)));
            assert(exists|r:Result<(),laws::Failure>|
                genesis_evaluated(d,actual.0,limits,s.1,diagnostics,reads,(r,finished.1,finished.3,finished.4))&&
                finished.0==(match r{Ok(())=>Ok(identity_candidate(actual.0)),Err(e)=>Err(Failure::Law(e))})&&
                finished.2==(trace.0+s.2,trace.1,Some(s.1),trace.3));
        },
    }
    assert(genesis(d,initial,limits,used,trace,diagnostics,reads,finished));
}
/// One actual run dispatched by invocation kind; genesis ignores the absent lanes.
pub(crate) open(in super::super) spec fn evaluated(d:&DescriptorProjection,kind:Kind,r:Raw,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:Finished)->bool{
    match kind{
        Kind::Genesis=>genesis(d,r.state@,limits,used,trace,diagnostics,reads,finished),
        Kind::Transition=>execution(d,r,limits,used,trace,diagnostics,reads,finished),
    }
}
pub(crate) open(in super::super) spec fn framed(d:&DescriptorProjection,kind:Kind,r:Raw,f:&Framing,limits:Seq<u64>,used:Seq<u64>,t:TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:Finished)->bool {
    if !d.admitted(){finished==(Err(Failure::Metadata),used,t,diagnostics,reads)}else{match kind {
        Kind::Genesis=>{let p=framed::spec::project(r.state@,&f.state,limits,used);match p.0{
            Err(e)=>finished==(Err(Failure::Frame(0,e)),p.1,t,diagnostics,reads),
            Ok(raw)=>genesis(d,raw.1,limits,p.1,t,diagnostics,reads,finished),
        }},
        Kind::Transition=>{let p=framed::spec::payloads(raw_view(r),f,limits,used);match p.0{
            Err(e)=>finished==(Err(e),p.1,t,diagnostics,reads),
            Ok(raw)=>exists|actual:Raw|raw_view(actual)==raw&&execution(d,actual,limits,p.1,t,diagnostics,reads,finished),
        }},
    }}
}

pub(in super::super) open spec fn framed_genesis(d:&DescriptorProjection,initial:Seq<u8>,f:&FrameBinding,limits:Seq<u64>,used:Seq<u64>,t:TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:Finished)->bool {
    if !d.admitted(){finished==(Err(Failure::Metadata),used,t,diagnostics,reads)}else{
        let p=framed::spec::project(initial,f,limits,used);match p.0{
            Err(e)=>finished==(Err(Failure::Frame(0,e)),p.1,t,diagnostics,reads),
            Ok(raw)=>genesis(d,raw.1,limits,p.1,t,diagnostics,reads,finished),
        }
    }
}

/// Pointwise binding correspondence; the value view is retained, never rebuilt from codes.
pub(in super::super) proof fn legacy_scalar<'a>(schema:Schema<'a>,v:(ValueView<'a>,Seq<i128>),selector:Selector)
    ensures scalar(SchemaView::Legacy(schema),input_view::typed::spec::lift(v.1),selector)
        ==match producer::spec::scalar_data(schema,v,selector){None=>None,Some(code)=>Some(Atom::I128(code as i128))},
{
    if let (Schema::Record(fs),Selector::Field(id))=(schema,selector) {
        let mapped=input_view::typed::spec::fields(input_view::typed::FieldsView::Legacy(fs));
        assert(mapped.len()==fs@.len());
        assert forall|j:int|0<=j<fs@.len() implies mapped[j].0==fs@[j].id by {}
        assert forall|i:int|first(mapped,id,i)==admission::spec::first(fs@,id,i) by {
            if first(mapped,id,i) {
                assert forall|j:int|0<=j<i implies fs@[j].id!=id by { assert(mapped[j].0!=id); }
            }
            if admission::spec::first(fs@,id,i) {
                assert forall|j:int|0<=j<i implies mapped[j].0!=id by { assert(fs@[j].id!=id); }
            }
        }
        if exists|i:int|first(mapped,id,i) {
            let i=choose|i:int|first(mapped,id,i);
            assert(admission::spec::first(fs@,id,i));
            assert(exists|j:int|admission::spec::first(fs@,id,j));
        }
        if exists|i:int|admission::spec::first(fs@,id,i) {
            let i=choose|i:int|admission::spec::first(fs@,id,i);
            assert(first(mapped,id,i));
            assert(exists|j:int|first(mapped,id,j));
        }
        assert((exists|i:int|first(mapped,id,i))==(exists|i:int|admission::spec::first(fs@,id,i)));
        if exists|i:int|first(mapped,id,i) {
            let selected=choose|i:int|first(mapped,id,i);
            assert(admission::spec::first(fs@,id,selected));
            admission::spec::first_chosen(fs@,id,selected);
        }
    }
}
pub(in super::super) proof fn legacy_tuple<'a>(d:&Descriptor<'a>,s:(ValueView<'a>,Seq<i128>),c:(ValueView<'a>,Seq<i128>),x:(ValueView<'a>,Seq<i128>),n:nat)
    requires n<=d.bindings@.len(),
    ensures tuple(&d.model(),input_view::typed::spec::lift(s.1),input_view::typed::spec::lift(c.1),input_view::typed::spec::lift(x.1),n)
        ==match producer::spec::tuple_data(d,s,c,x,n){None=>None,Some(values)=>Some(input_view::typed::spec::lift(values))},
    decreases n,
{
    reveal(<Descriptor as DescriptorData>::model);
    reveal_with_fuel(tuple,2);reveal_with_fuel(producer::spec::tuple_data,2);
    if n>0{
        legacy_tuple(d,s,c,x,(n-1) as nat);
        let b=d.bindings[n as int-1];
        match b.source {
            Source::State=>legacy_scalar(d.state,s,b.selector),
            Source::Command=>legacy_scalar(d.command,c,b.selector),
            Source::Context=>legacy_scalar(d.context,x,b.selector),
        }
        if let Some(prior)=producer::spec::tuple_data(d,s,c,x,(n-1) as nat) {
            let code=match b.source {
                Source::State=>producer::spec::scalar_data(d.state,s,b.selector),
                Source::Command=>producer::spec::scalar_data(d.command,c,b.selector),
                Source::Context=>producer::spec::scalar_data(d.context,x,b.selector),
            };
            if let Some(code)=code{input_view::typed::spec::lift_push(prior,code);}
        }
    }else{assert(input_view::typed::spec::lift(Seq::<i128>::empty()) =~= Seq::<Atom>::empty());}
}
/// Exact old inverse mapping, including raw invalid codes and incomplete tuples.
pub(in super::super) proof fn legacy_outputs(types:&[InputLeaf],codes:Seq<i128>,n:nat)
    requires n<=types@.len(),
    ensures output_prefix(OutputTypes::Legacy(types),input_view::typed::spec::lift(codes),n)==producer::spec::atoms(types@,codes,n),
    decreases n,
{
    if n>0{legacy_outputs(types,codes,(n-1) as nat);}
}
pub(in super::super) proof fn legacy_after_ingress<'a>(d:&Descriptor<'a>,s:(ValueView<'a>,Seq<i128>),c:(ValueView<'a>,Seq<i128>),x:(ValueView<'a>,Seq<i128>),limits:Seq<u64>,used:Seq<u64>,t:TraceView,finished:ProducedFinished<'a>)
    requires limits.len()==8,used.len()==8,
    ensures after_ingress(&d.model(),(s.0,input_view::typed::spec::lift(s.1)),(c.0,input_view::typed::spec::lift(c.1)),(x.0,input_view::typed::spec::lift(x.1)),limits,used,t,finished)
        ==producer::spec::after_ingress(d,s,c,x,limits,used,t,finished),
{
    reveal(<Descriptor as DescriptorData>::model);
    legacy_tuple(d,s,c,x,d.bindings@.len());
    if let Some(input)=producer::spec::tuple_data(d,s,c,x,d.bindings@.len()) {
        assert(input_view::typed::spec::lift(input) =~= super::super::super::super::evaluation::typed::spec::lift(input));
        numeric::spec::legacy_execution_embedding(data::program(&d.model()),input,limits,used);
        let e=super::super::super::spec::execution(d.program.inputs@,d.program.outputs@,d.program.nodes@,d.program.roots@,input,limits,used);
        let actual=numeric::spec::execution(data::program(&d.model()),input_view::typed::spec::lift(input),limits,used);
        if let Ok(codes)=e.0 {
            let atoms=match actual.0 {Ok(atoms)=>atoms,Err(_)=>Seq::<Atom>::empty()};
            assert(actual.0==Ok(atoms));
            assert(numeric::spec::project_all(atoms)==Some(codes));
            numeric::spec::project_inverse(atoms,codes);
            assert(atoms =~= input_view::typed::spec::lift(codes));
            legacy_outputs(d.output_types,codes,d.output_types@.len());
            if let Some(reified)=producer::spec::atoms(d.output_types@,codes,d.output_types@.len()) {
                assert(from_atoms(&d.model(),s.0,c.0,x.0,reified,limits,e.1,(t.0,t.1,Some(used),t.3),finished)
                    ==producer::spec::from_atoms(d,s.0,c.0,x.0,reified,limits,e.1,(t.0,t.1,Some(used),t.3),finished));
            }
        }
    }
}
/// Every admitted or refused ingress retains the fixed eight-counter shape.
pub(in super::super) proof fn legacy_ingress_usage(bytes:Seq<u8>,schema:Schema,source:u8,limits:Seq<u64>,used:Seq<u64>)
    requires limits.len()==8,used.len()==8,
    ensures ingress::spec::project(bytes,schema,source,limits,used).1.len()==8,
{
    if let Schema::Record(fs)=schema {input_view::projection_usage(bytes,fs@,limits,used);}
}
pub(in super::super) proof fn legacy_produce<'a>(d:&Descriptor<'a>,raw:Raw,limits:Seq<u64>,used:Seq<u64>,t:TraceView,finished:ProducedFinished<'a>)
    requires limits.len()==8,used.len()==8,
    ensures produce(&d.model(),raw,limits,used,t,finished)==producer::spec::produce(d,raw,limits,used,t,finished),
{
    reveal(<Descriptor as DescriptorData>::model);super::super::descriptor::legacy_admitted(d);reveal(descriptor_admitted);
    input::spec::legacy_embedding(raw.state@,d.state,0,limits,used);
    legacy_ingress_usage(raw.state@,d.state,0,limits,used);
    let s=ingress::spec::project(raw.state@,d.state,0,limits,used);
    if let Ok(state)=s.0 {
        input::spec::legacy_embedding(raw.command@,d.command,1,limits,s.1);
        legacy_ingress_usage(raw.command@,d.command,1,limits,s.1);
        let c=ingress::spec::project(raw.command@,d.command,1,limits,s.1);
        if let Ok(command)=c.0 {
            input::spec::legacy_embedding(raw.context@,d.context,2,limits,c.1);
            legacy_ingress_usage(raw.context@,d.context,2,limits,c.1);
            let x=ingress::spec::project(raw.context@,d.context,2,limits,c.1);
            if let Ok(context)=x.0 {legacy_after_ingress(d,state,command,context,limits,x.1,(t.0+s.2+c.2+x.2,t.1,t.2,t.3),finished);}
        }
    }
}
/// Both directions retain every refusal, candidate, law result and usage field.
pub(in super::super) proof fn legacy_execution(d:&Descriptor,kind:Kind,r:Raw,limits:Seq<u64>,used:Seq<u64>,t:TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:Finished)
    requires limits.len()==8,used.len()==8,
    ensures evaluated(&d.model(),kind,r,limits,used,t,diagnostics,reads,finished)==super::super::spec::evaluated(d,kind,r,limits,used,t,diagnostics,reads,finished),
{
    reveal(<Descriptor as DescriptorData>::model);super::super::descriptor::legacy_admitted(d);reveal(descriptor_admitted);
    match kind {
        Kind::Genesis=>legacy_genesis(d,r.state@,limits,used,t,diagnostics,reads,finished),
        Kind::Transition=>{
            reveal(execution);reveal(super::super::spec::execution);
            assert forall|middle:ProducedFinished|produce(&d.model(),r,limits,used,t,middle)==producer::spec::produce(d,r,limits,used,t,middle) by {
                legacy_produce(d,r,limits,used,t,middle);
            }
            assert forall|middle:ProducedFinished|after_producer(&d.model(),middle,diagnostics,reads,limits,finished)
                ==super::super::spec::after_producer(d,middle,diagnostics,reads,limits,finished) by {
                legacy_after_producer(d,middle,diagnostics,reads,limits,finished);
            }
            if execution(&d.model(),r,limits,used,t,diagnostics,reads,finished) {
                let middle=choose|middle:ProducedFinished|produce(&d.model(),r,limits,used,t,middle)&&after_producer(&d.model(),middle,diagnostics,reads,limits,finished);
                legacy_produce(d,r,limits,used,t,middle);
                legacy_after_producer(d,middle,diagnostics,reads,limits,finished);
                assert(super::super::spec::execution(d,r,limits,used,t,diagnostics,reads,finished));
            }
            if super::super::spec::execution(d,r,limits,used,t,diagnostics,reads,finished) {
                let middle=choose|middle:ProducedFinished|producer::spec::produce(d,r,limits,used,t,middle)&&super::super::spec::after_producer(d,middle,diagnostics,reads,limits,finished);
                legacy_produce(d,r,limits,used,t,middle);
                legacy_after_producer(d,middle,diagnostics,reads,limits,finished);
                assert(execution(&d.model(),r,limits,used,t,diagnostics,reads,finished));
            }
        },
    }
}
pub(in super::super) proof fn legacy_framed(d:&Descriptor,kind:Kind,r:Raw,f:&Framing,limits:Seq<u64>,used:Seq<u64>,t:TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:Finished)
    requires limits.len()==8,used.len()==8,
    ensures framed(&d.model(),kind,r,f,limits,used,t,diagnostics,reads,finished)==match kind{
        Kind::Genesis=>super::super::framed::spec::genesis(d,r.state@,&f.state,limits,used,t,diagnostics,reads,finished),
        Kind::Transition=>super::super::framed::spec::execution(d,r,f,limits,used,t,diagnostics,reads,finished),
    },
{
    reveal(<Descriptor as DescriptorData>::model);super::super::descriptor::legacy_admitted(d);reveal(descriptor_admitted);
    match kind {
        Kind::Genesis=>{
            let p=framed::spec::project(r.state@,&f.state,limits,used);
            if let Ok(raw)=p.0{legacy_genesis(d,raw.1,limits,p.1,t,diagnostics,reads,finished);}
        },
        Kind::Transition=>{
            let p=framed::spec::payloads(raw_view(r),f,limits,used);
            assert(p.1.len()==8);
            assert forall|actual:Raw|execution(&d.model(),actual,limits,p.1,t,diagnostics,reads,finished)
                ==super::super::spec::execution(d,actual,limits,p.1,t,diagnostics,reads,finished) by {
                legacy_execution(d,Kind::Transition,actual,limits,p.1,t,diagnostics,reads,finished);
            }
        },
    }
}

/// The same first matching channel is chosen, including malformed duplicate IDs.
pub(in super::super) proof fn legacy_channel(d:&Descriptor,id:u32)
    ensures channel(d.channels@,id)==admission::spec::channel(d,id),
{
    assert forall|i:int|admission::spec::first_channel(d,id,i)==
        (0<=i<d.channels@.len()&&d.channels@[i].id==id
            &&(forall|j:int|0<=j<i ==> d.channels@[j].id!=id)) by {}
    if exists|i:int|admission::spec::first_channel(d,id,i) {
        let i=choose|i:int|admission::spec::first_channel(d,id,i);
        assert(0<=i<d.channels@.len()&&d.channels@[i].id==id&&(forall|j:int|0<=j<i ==> d.channels@[j].id!=id));
        assert(exists|k:int|0<=k<d.channels@.len()&&d.channels@[k].id==id&&(forall|j:int|0<=j<k ==> d.channels@[j].id!=id));
    }
    if exists|i:int|0<=i<d.channels@.len()&&d.channels@[i].id==id&&(forall|j:int|0<=j<i ==> d.channels@[j].id!=id) {
        let i=choose|i:int|0<=i<d.channels@.len()&&d.channels@[i].id==id&&(forall|j:int|0<=j<i ==> d.channels@[j].id!=id);
        assert(admission::spec::first_channel(d,id,i));
        assert(exists|k:int|admission::spec::first_channel(d,id,k));
    }
    assert((exists|i:int|admission::spec::first_channel(d,id,i))==
        (exists|i:int|0<=i<d.channels@.len()&&d.channels@[i].id==id&&(forall|j:int|0<=j<i ==> d.channels@[j].id!=id)));
    if exists|i:int|admission::spec::first_channel(d,id,i) {
        let old=choose|i:int|admission::spec::first_channel(d,id,i);
        let new=choose|i:int|0<=i<d.channels@.len()&&d.channels@[i].id==id
            &&(forall|j:int|0<=j<i ==> d.channels@[j].id!=id);
        if old<new {assert(d.channels@[old].id!=id);}
        if new<old {assert(d.channels@[new].id!=id);}
        assert(old==new);
        assert(channel(d.channels@,id)==Some(d.channels@[new]));
        assert(admission::spec::channel(d,id)==Some(d.channels@[old]));
    }
}

/// Equality retains both ordered delivery lanes and every payload/domain check.
pub(in super::super) proof fn legacy_candidate(d:&Descriptor,c:decision::spec::CandidateView)
    ensures candidate(&d.model(),c)==schema_validation::spec::candidate(d,c),
{
    reveal(<Descriptor as DescriptorData>::model);
    assert forall|p:decision::spec::DeliveryView|
        delivery(d.channels@,p)==schema_validation::spec::delivery(d,p) by {
        legacy_channel(d,p.1);
    }
    assert(deliveries(d.channels@,c.5)==schema_validation::spec::deliveries(d,c.5));
    assert(deliveries(d.channels@,c.6)==schema_validation::spec::deliveries(d,c.6));
}

/// Identical producer failure, schema failure, complete law frame and law result.
/// No success or admission premise discards a refusal case.
pub(in super::super) proof fn legacy_after_producer(d:&Descriptor,middle:ProducedFinished,
    diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,limits:Seq<u64>,finished:Finished)
    ensures after_producer(&d.model(),middle,diagnostics,reads,limits,finished)
        ==super::super::spec::after_producer(d,middle,diagnostics,reads,limits,finished),
{
    reveal(<Descriptor as DescriptorData>::model);
    reveal(after_producer);reveal(super::super::spec::after_producer);
    if let Ok(p)=middle.0 {
        legacy_candidate(d,p.0);
        assert forall|result:Result<(),laws::Failure>|
            frame_evaluated(&d.model(),p,middle.2,middle.1,limits,diagnostics,reads,
                (result,finished.1,finished.3,finished.4))
            ==super::super::spec::frame_evaluated(d,p,middle.2,middle.1,limits,diagnostics,reads,
                (result,finished.1,finished.3,finished.4)) by {}
        if candidate(&d.model(),p.0) {
            if after_producer(&d.model(),middle,diagnostics,reads,limits,finished) {
                let result=choose|result:Result<(),laws::Failure>|frame_evaluated(&d.model(),p,middle.2,middle.1,limits,diagnostics,reads,(result,finished.1,finished.3,finished.4))
                    &&finished.0==(match result{Ok(())=>Ok(p.0),Err(e)=>Err(Failure::Law(e))})&&finished.2==middle.2;
                assert(super::super::spec::frame_evaluated(d,p,middle.2,middle.1,limits,diagnostics,reads,(result,finished.1,finished.3,finished.4)));
                assert(super::super::spec::after_producer(d,middle,diagnostics,reads,limits,finished));
            }
            if super::super::spec::after_producer(d,middle,diagnostics,reads,limits,finished) {
                let result=choose|result:Result<(),laws::Failure>|super::super::spec::frame_evaluated(d,p,middle.2,middle.1,limits,diagnostics,reads,(result,finished.1,finished.3,finished.4))
                    &&finished.0==(match result{Ok(())=>Ok(p.0),Err(e)=>Err(Failure::Law(e))})&&finished.2==middle.2;
                assert(frame_evaluated(&d.model(),p,middle.2,middle.1,limits,diagnostics,reads,(result,finished.1,finished.3,finished.4)));
                assert(after_producer(&d.model(),middle,diagnostics,reads,limits,finished));
            }
        }
    }
}

/// Arbitrary initial byte sequences, including every metadata/ingress/law refusal.
/// The candidate, trace, diagnostic/read sequences and usage are the same tuple.
pub(in super::super) proof fn legacy_genesis(d:&Descriptor,initial:Seq<u8>,limits:Seq<u64>,used:Seq<u64>,
    t:TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:Finished)
    requires limits.len()==8,used.len()==8,
    ensures genesis(&d.model(),initial,limits,used,t,diagnostics,reads,finished)
        ==super::super::spec::genesis(d,initial,limits,used,t,diagnostics,reads,finished),
{
    reveal(<Descriptor as DescriptorData>::model);
    super::super::descriptor::legacy_admitted(d);
    reveal(descriptor_admitted);
    reveal(genesis);reveal(super::super::spec::genesis);
    input::spec::legacy_embedding(initial,d.state,0,limits,used);
    let s=ingress::spec::project(initial,d.state,0,limits,used);
    if let Ok(decoded)=s.0 {
        assert forall|result:Result<(),laws::Failure>|
            genesis_evaluated(&d.model(),decoded.0,limits,s.1,diagnostics,reads,
                (result,finished.1,finished.3,finished.4))
            ==super::super::spec::genesis_evaluated(d,decoded.0,limits,s.1,diagnostics,reads,
                (result,finished.1,finished.3,finished.4)) by {}
        if d.model().admitted() {
            if genesis(&d.model(),initial,limits,used,t,diagnostics,reads,finished) {
                let result=choose|result:Result<(),laws::Failure>|genesis_evaluated(&d.model(),decoded.0,limits,s.1,diagnostics,reads,(result,finished.1,finished.3,finished.4))
                    &&finished.0==(match result{Ok(())=>Ok(identity_candidate(decoded.0)),Err(e)=>Err(Failure::Law(e))})&&finished.2==(t.0+s.2,t.1,Some(s.1),t.3);
                assert(super::super::spec::genesis_evaluated(d,decoded.0,limits,s.1,diagnostics,reads,(result,finished.1,finished.3,finished.4)));
                assert(super::super::spec::genesis(d,initial,limits,used,t,diagnostics,reads,finished));
            }
            if super::super::spec::genesis(d,initial,limits,used,t,diagnostics,reads,finished) {
                let result=choose|result:Result<(),laws::Failure>|super::super::spec::genesis_evaluated(d,decoded.0,limits,s.1,diagnostics,reads,(result,finished.1,finished.3,finished.4))
                    &&finished.0==(match result{Ok(())=>Ok(identity_candidate(decoded.0)),Err(e)=>Err(Failure::Law(e))})&&finished.2==(t.0+s.2,t.1,Some(s.1),t.3);
                assert(genesis_evaluated(&d.model(),decoded.0,limits,s.1,diagnostics,reads,(result,finished.1,finished.3,finished.4)));
                assert(genesis(&d.model(),initial,limits,used,t,diagnostics,reads,finished));
            }
        }
    }
}

/// Direct genesis framing needs no invented Raw reference for arbitrary ghost bytes.
pub(in super::super) proof fn legacy_framed_genesis(d:&Descriptor,initial:Seq<u8>,f:&FrameBinding,
    limits:Seq<u64>,used:Seq<u64>,t:TraceView,diagnostics:Seq<laws::Diagnostic>,
    reads:Seq<laws::ReadAttempt>,finished:Finished)
    requires limits.len()==8,used.len()==8,
    ensures framed_genesis(&d.model(),initial,f,limits,used,t,diagnostics,reads,finished)
        ==super::super::framed::spec::genesis(d,initial,f,limits,used,t,diagnostics,reads,finished),
{
    reveal(<Descriptor as DescriptorData>::model);
    super::super::descriptor::legacy_admitted(d);
    reveal(descriptor_admitted);
    let p=framed::spec::project(initial,f,limits,used);
    assert(p.1.len()==8);
    if let Ok(raw)=p.0 {
        legacy_genesis(d,raw.1,limits,p.1,t,diagnostics,reads,finished);
    }
}

}
