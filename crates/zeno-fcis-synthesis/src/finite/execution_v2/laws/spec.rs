//! Exact mathematical semantics of immutable frames, eager predicates and law order.
use super::*;
use vstd::prelude::*;
verus! {
pub(in super::super) enum Value {
    Bool(bool), I128(i128), U128(u128), Enum(u32,u16), Sum(u32,u16),
    Bytes(Seq<u8>), Text(Seq<u8>),
}
pub(in super::super) open spec fn atom(value:Atom) -> Value {
    match value {

        Atom::Bool(v)=>Value::Bool(v), Atom::I128(v)=>Value::I128(v), Atom::U128(v)=>Value::U128(v),
        Atom::Enum{type_id,variant}=>Value::Enum(type_id,variant),
        Atom::Sum{type_id,variant}=>Value::Sum(type_id,variant),
        Atom::Bytes(v)=>Value::Bytes(v@), Atom::Text(v)=>Value::Text(v@),
    }
}
pub(in super::super) open spec fn value_result(result:Result<Atom,Failure>) -> Result<Value,Failure> {
    match result { Ok(value)=>Ok(atom(value)), Err(e)=>Err(e) }
}
pub(in super::super) open spec fn same_type(a:Value,b:Value)->bool {
    match (a,b) {

        (Value::Bool(_),Value::Bool(_)) | (Value::I128(_),Value::I128(_)) |
        (Value::U128(_),Value::U128(_)) | (Value::Bytes(_),Value::Bytes(_)) |
        (Value::Text(_),Value::Text(_)) => true,
        (Value::Enum(t,_),Value::Enum(u,_)) | (Value::Sum(t,_),Value::Sum(u,_)) => t==u,
        _ => false,
    }
}
pub(in super::super) open spec fn integer(value:int,signed:bool)->Result<Value,Failure> {
    if signed { if i128::MIN <= value <= i128::MAX {Ok(Value::I128(value as i128))} else {Err(Failure::Undefined)} }
    else {if 0 <= value <= u128::MAX {Ok(Value::U128(value as u128))} else {Err(Failure::Undefined)}}
}
pub(in super::super) open spec fn binary(kind:u8,a:Value,b:Value)->Result<Value,Failure> {
    if kind==2 {Ok(Value::Bool(a==b))} else {
    match (kind,a,b) {
        (0,Value::I128(x),Value::I128(y))=>integer(x as int+y as int,true),
        (1,Value::I128(x),Value::I128(y))=>integer(x as int-y as int,true),
        (0,Value::U128(x),Value::U128(y))=>integer(x as int+y as int,false),
        (1,Value::U128(x),Value::U128(y))=>integer(x as int-y as int,false),
        (5,Value::I128(x),Value::I128(y))=>integer(x as int*y as int,true),
        (5,Value::U128(x),Value::U128(y))=>integer(x as int*y as int,false),
        (3,Value::I128(x),Value::I128(y))=>Ok(Value::Bool(x<y)),
        (3,Value::U128(x),Value::U128(y))=>Ok(Value::Bool(x<y)),
        (4,Value::Bool(x),Value::Bool(y))=>Ok(Value::Bool(x&&y)),
        _=>Err(Failure::Undefined),
    }}
}
}

verus! {
/// Exact protected-observation selector tag; a root never aliases a field.
pub(in super::super) open spec fn selector_id(s:Selector)->u32 {
    match s { Selector::Root=>65536, Selector::Field(id)=>id as u32 }
}
/// One (source tag, stable ID, permission) triple of an actual attempt.
pub type AttemptView=(u8,u32,bool);
pub(in super::super) open spec fn write_views(a:Seq<Attempt>,n:nat)->Seq<AttemptView> recommends n<=a.len(),decreases n{
    if n==0{Seq::empty()}else{let prior=write_views(a,(n-1) as nat);match a[n as int-1]{
        Attempt::Candidate(permitted)=>prior.push((0,0,permitted)),
        Attempt::Write(id,permitted)=>prior.push((1,id as u32,permitted)),
        _=>prior}}
}
pub(in super::super) open spec fn effect_views(a:Seq<Attempt>,n:nat)->Seq<AttemptView> recommends n<=a.len(),decreases n{
    if n==0{Seq::empty()}else{let prior=effect_views(a,(n-1) as nat);match a[n as int-1]{
        Attempt::Effect(outbox,id,permitted)=>prior.push((if outbox{3}else{2},id,permitted)),
        _=>prior}}
}
}
verus! {
pub(in super::super) open spec fn optional(value:Option<Atom>)->Option<Value> {
    match value {Some(v)=>Some(atom(v)),None=>None}
}
pub(in super::super) open spec fn field(fields:Seq<Field>,id:u16,n:nat)->Option<Value>
    decreases n,
{
    if n==0 {None} else {
        let prior=field(fields,id,(n-1) as nat);
        if prior.is_some(){prior} else if fields[n-1].id==id {Some(atom(fields[n-1].value))} else {None}
    }
}
pub(in super::super) proof fn field_found(fields:Seq<Field>,id:u16,n:nat,end:nat)
    requires n<=end<=fields.len(),field(fields,id,n).is_some(),
    ensures field(fields,id,end)==field(fields,id,n),
    decreases end-n,
{if n<end {field_found(fields,id,n,(end-1) as nat);}}
pub(in super::super) open spec fn observe(frame:&Frame,observation:Observation)->Option<Value> {
    match frame {
        Frame::Genesis{initial}=>match observation {
            Observation::Initial(id)|Observation::Post(id)=>view_field(*initial,id),
            Observation::PostLength=>record_length(*initial),
            Observation::InitialRoot|Observation::PostRoot=>view_atom(*initial),
            _=>None,
        },
        Frame::Transition{pre,command,context,candidate:c}=>match observation {
            Observation::Pre(id)=>view_field(*pre,id),
            Observation::Command(id)=>view_field(*command,id),
            Observation::Context(id)=>view_field(*context,id),
            Observation::Post(id)=>view_field(c.post,id),
            Observation::Initial(_)|Observation::InitialRoot=>None,
            Observation::PreRoot=>view_atom(*pre),
            Observation::CommandRoot=>view_atom(*command),
            Observation::ContextRoot=>view_atom(*context),
            Observation::PostRoot=>view_atom(c.post),
            Observation::Class=>Some(Value::I128(match c.class {Class::Accept=>0,Class::Reject=>1,Class::CommittedFailure=>2})),
            Observation::HasReason=>Some(Value::Bool(c.reason.is_some())),
            Observation::Reason=>match c.reason {Some(id)=>Some(Value::I128(id as i128)),None=>None},
            Observation::PostLength=>record_length(c.post),
            Observation::PatchLength=>Some(Value::U128(c.patch@.len() as u128)),
            Observation::EffectLength=>Some(Value::U128(c.effects@.len() as u128)),
            Observation::OutboxLength=>Some(Value::U128(c.outbox@.len() as u128)),
            Observation::ReadLength=>Some(Value::U128(c.reads@.len() as u128)),
            Observation::WriteLength=>Some(Value::U128(write_views(c.attempts@,c.attempts@.len()).len() as u128)),
            Observation::EffectAttemptLength=>Some(Value::U128(effect_views(c.attempts@,c.attempts@.len()).len() as u128)),
            Observation::PatchField(i)=>if i<c.patch@.len() {Some(Value::I128(c.patch@[i as int].field as i128))} else {None},
            Observation::PatchBefore(i)=>if i<c.patch@.len() {Some(atom(c.patch@[i as int].before))} else {None},
            Observation::PatchAfter(i)=>if i<c.patch@.len() {Some(atom(c.patch@[i as int].after))} else {None},
            Observation::EffectOrdinal(i)=>if i<c.effects@.len() {Some(Value::I128(c.effects@[i as int].ordinal as i128))} else {None},
            Observation::EffectChannel(i)=>if i<c.effects@.len() {Some(Value::I128(c.effects@[i as int].channel as i128))} else {None},
            Observation::EffectDestination(i)=>if i<c.effects@.len() {Some(atom(c.effects@[i as int].destination))} else {None},
            Observation::EffectIdempotency(i)=>if i<c.effects@.len() {Some(atom(c.effects@[i as int].idempotency))} else {None},
            Observation::OutboxOrdinal(i)=>if i<c.outbox@.len() {Some(Value::I128(c.outbox@[i as int].ordinal as i128))} else {None},
            Observation::OutboxChannel(i)=>if i<c.outbox@.len() {Some(Value::I128(c.outbox@[i as int].channel as i128))} else {None},
            Observation::OutboxDestination(i)=>if i<c.outbox@.len() {Some(atom(c.outbox@[i as int].destination))} else {None},
            Observation::OutboxIdempotency(i)=>if i<c.outbox@.len() {Some(atom(c.outbox@[i as int].idempotency))} else {None},
            Observation::ReadSource(i)=>if i<c.reads@.len() {Some(Value::I128(c.reads@[i as int].source as i128))} else {None},
            Observation::ReadId(i)=>if i<c.reads@.len() {Some(Value::I128(selector_id(c.reads@[i as int].selector) as i128))} else {None},
            Observation::ReadPermitted(i)=>if i<c.reads@.len() {Some(Value::Bool(c.reads@[i as int].permitted))} else {None},
            Observation::WriteSource(i)=>if i<write_views(c.attempts@,c.attempts@.len()).len() {Some(Value::I128(write_views(c.attempts@,c.attempts@.len())[i as int].0 as i128))} else {None},
            Observation::WriteId(i)=>if i<write_views(c.attempts@,c.attempts@.len()).len() {Some(Value::I128(write_views(c.attempts@,c.attempts@.len())[i as int].1 as i128))} else {None},
            Observation::WritePermitted(i)=>if i<write_views(c.attempts@,c.attempts@.len()).len() {Some(Value::Bool(write_views(c.attempts@,c.attempts@.len())[i as int].2))} else {None},
            Observation::EffectAttemptSource(i)=>if i<effect_views(c.attempts@,c.attempts@.len()).len() {Some(Value::I128(effect_views(c.attempts@,c.attempts@.len())[i as int].0 as i128))} else {None},
            Observation::EffectAttemptId(i)=>if i<effect_views(c.attempts@,c.attempts@.len()).len() {Some(Value::I128(effect_views(c.attempts@,c.attempts@.len())[i as int].1 as i128))} else {None},
            Observation::EffectAttemptPermitted(i)=>if i<effect_views(c.attempts@,c.attempts@.len()).len() {Some(Value::Bool(effect_views(c.attempts@,c.attempts@.len())[i as int].2))} else {None},
            Observation::EffectPayload(i,id)=>if i<c.effects@.len() {field(c.effects@[i as int].payload@,id,c.effects@[i as int].payload@.len())} else {None},
            Observation::OutboxPayload(i,id)=>if i<c.outbox@.len() {field(c.outbox@[i as int].payload@,id,c.outbox@[i as int].payload@.len())} else {None},
            Observation::Usage(resource)=>Some(Value::U128(c.usage.view()[super::super::spec::resource_index(resource) as int] as u128)),
        },
    }
}
}
verus! {
pub(in super::super) open spec fn values(v:Seq<Atom>)->Seq<Value> {v.map(|i:int,a:Atom|atom(a))}
pub(in super::super) open spec fn at(v:Seq<Value>,i:usize)->Result<Value,Failure> {
    if i<v.len(){Ok(v[i as int])}else{Err(Failure::Undefined)}
}
pub(in super::super) open spec fn pure_node(op:&Op,v:Seq<Value>)->Result<Value,Failure> {
    match *op {
        Op::Literal(x)=>Ok(atom(x)),Op::Observe(_)|Op::ObserveWhen(..)=>Err(Failure::Undefined),
        Op::Add(a,b)|Op::Sub(a,b)|Op::Mul(a,b)|Op::Eq(a,b)|Op::Lt(a,b)|Op::And(a,b)=>{
            let kind=match *op{Op::Add(..)=>0u8,Op::Sub(..)=>1u8,Op::Eq(..)=>2u8,Op::Lt(..)=>3u8,Op::Mul(..)=>5u8,_=>4u8};
            match at(v,a) {Err(e)=>Err(e),Ok(x)=>match at(v,b){Err(e)=>Err(e),Ok(y)=>binary(kind,x,y)}}
        },
        Op::Div(mode,a,b)=>match at(v,a){Err(e)=>Err(e),Ok(x)=>match at(v,b){Err(e)=>Err(e),Ok(y)=>division(mode,x,y)}},
        Op::ToI128(a)=>match at(v,a){Err(e)=>Err(e),Ok(x)=>to_i128(x)},
        Op::Not(a)=>match at(v,a){Ok(Value::Bool(b))=>Ok(Value::Bool(!b)),_=>Err(Failure::Undefined)},
        Op::Select(a,b,c)=>match at(v,a) {Err(e)=>Err(e),Ok(condition)=>
            match at(v,b){Err(e)=>Err(e),Ok(yes)=>match at(v,c){Err(e)=>Err(e),Ok(no)=>
                if !same_type(yes,no){Err(Failure::Undefined)}else{match condition{Value::Bool(t)=>Ok(if t{yes}else{no}),_=>Err(Failure::Undefined)}}}}},
    }
}
pub(in super::super) open spec fn default_text_valid(bytes:Seq<u8>)->bool {
    forall|i:int| 0<=i<bytes.len() ==> bytes[i]<128
}
pub(in super::super) open spec fn default_valid(value:Atom)->bool {
    match value {Atom::Text(bytes)=>default_text_valid(bytes@),_=>true}
}
pub(in super::super) open spec fn node_shape(op:Op,index:nat)->bool {
    match op {
        Op::Literal(_)|Op::Observe(_)=>true,
        Op::ObserveWhen(guard,_,default)=>guard<index&&default_valid(default),
        Op::Add(a,b)|Op::Sub(a,b)|Op::Mul(a,b)|Op::Div(_,a,b)|Op::Eq(a,b)|Op::Lt(a,b)|Op::And(a,b)=>a<index&&b<index,
        Op::Not(a)|Op::ToI128(a)=>a<index,
        Op::Select(a,b,c)=>a<index&&b<index&&c<index,
    }
}
pub(in super::super) open spec fn shape(p:&Program)->bool {
    p.root<p.nodes@.len() && forall|i:int| 0<=i<p.nodes@.len() ==> node_shape(p.nodes@[i],i as nat)
}
pub(in super::super) open spec fn read_observation(observation:Observation,frame:&Frame,law:u32,index:usize,
    limits:Seq<u64>,used:Seq<u64>,reads:Seq<ReadAttempt>)->(Result<Value,Failure>,Seq<u64>,Seq<ReadAttempt>) {
    let read=super::super::spec::charge(limits,used,Resource::Read,1);
    let attempts=reads.push(ReadAttempt{law,node:index,observation,permitted:read.0.is_ok()});
    match read.0 {Err(e)=>(Err(Failure::Budget(e)),read.1,attempts),Ok(())=>
        (match observe(frame,observation){Some(value)=>Ok(value),None=>Err(Failure::Undefined)},read.1,attempts)}
}
pub(in super::super) open spec fn node(op:&Op,v:Seq<Value>,frame:&Frame,law:u32,index:usize,
    limits:Seq<u64>,used:Seq<u64>,reads:Seq<ReadAttempt>)->(Result<Value,Failure>,Seq<u64>,Seq<ReadAttempt>) {
    let step=super::super::spec::charge(limits,used,Resource::Step,1);
    match step.0 {Err(e)=>(Err(Failure::Budget(e)),step.1,reads),Ok(())=>match *op {
        Op::Observe(observation)=>read_observation(observation,frame,law,index,limits,step.1,reads),
        Op::ObserveWhen(guard,observation,default)=>{
            if guard>=index {(Err(Failure::Undefined),step.1,reads)}else{
                match at(v,guard) {
                    Ok(Value::Bool(false))=>(Ok(atom(default)),step.1,reads),
                    Ok(Value::Bool(true))=>read_observation(observation,frame,law,index,limits,step.1,reads),
                    _=>(Err(Failure::Undefined),step.1,reads),
                }
            }
        },
        _=>(pure_node(op,v),step.1,reads),
    }}
}
pub(in super::super) open spec fn prefix(nodes:Seq<Op>,frame:&Frame,law:u32,n:nat,
    limits:Seq<u64>,used:Seq<u64>,reads:Seq<ReadAttempt>)->(Result<Seq<Value>,Failure>,Seq<u64>,Seq<ReadAttempt>)
    decreases n,
{
    if n==0{(Ok(Seq::empty()),used,reads)}else{
        let prior=prefix(nodes,frame,law,(n-1) as nat,limits,used,reads);
        match prior.0 {Err(_)=>prior,Ok(v)=>{
            let next=node(&nodes[n-1],v,frame,law,(n-1) as usize,limits,prior.1,prior.2);
            (match next.0{Ok(value)=>Ok(v.push(value)),Err(e)=>Err(e)},next.1,next.2)
        }}
    }
}
pub(in super::super) proof fn failed_prefix(nodes:Seq<Op>,frame:&Frame,law:u32,n:nat,end:nat,
    limits:Seq<u64>,used:Seq<u64>,reads:Seq<ReadAttempt>,error:Failure)
    requires n<=end<=nodes.len(),prefix(nodes,frame,law,n,limits,used,reads).0==Err::<Seq<Value>,Failure>(error),
    ensures prefix(nodes,frame,law,end,limits,used,reads)==prefix(nodes,frame,law,n,limits,used,reads),
    decreases end-n,
{if n<end{failed_prefix(nodes,frame,law,n,(end-1) as nat,limits,used,reads,error);}}
pub(in super::super) open spec fn predicate(p:&Program,frame:&Frame,law:u32,limits:Seq<u64>,used:Seq<u64>,reads:Seq<ReadAttempt>)
    ->(Result<(),Failure>,Seq<u64>,Seq<ReadAttempt>) {
    let result=prefix(p.nodes@,frame,law,p.nodes@.len(),limits,used,reads);
    (match result.0 {Err(e)=>Err(e),Ok(v)=>match at(v,p.root){Ok(Value::Bool(true))=>Ok(()),Ok(Value::Bool(false))=>Err(Failure::Violated),_=>Err(Failure::Undefined)}},result.1,result.2)
}
}
verus! {
pub(in super::super) open spec fn record_valid(fields:Seq<Field>)->bool {
    forall|i:int| 0<i<fields.len() ==> fields[i-1].id<#[trigger] fields[i].id
}
pub(in super::super) open spec fn deliveries_valid(deliveries:Seq<Delivery>)->bool {
    forall|i:int| #![trigger deliveries[i]] 0<=i<deliveries.len() ==> record_valid(deliveries[i].payload@) && (i>0 ==> deliveries[i-1].ordinal<deliveries[i].ordinal)
}
pub(in super::super) open spec fn patch_valid(patch:Seq<Patch>)->bool {
    forall|i:int| #![trigger patch[i]] 0<=i<patch.len() ==> same_type(atom(patch[i].before),atom(patch[i].after))
        && (i>0 ==> patch[i-1].field<patch[i].field)
}
pub(in super::super) open spec fn matching_fields(pre:Seq<Field>,post:Seq<Field>)->bool {
    pre.len()==post.len() && forall|i:int| 0<=i<pre.len() ==> pre[i].id==post[i].id && same_type(atom(pre[i].value),atom(post[i].value))
}
pub(in super::super) open spec fn frame_valid(frame:&Frame)->bool {
    match frame {
        Frame::Genesis{initial}=>root_valid(*initial),
        Frame::Transition{pre,command,context,candidate:c}=>{
            let class_valid=match c.class {Class::Accept=>c.reason.is_none(),
                _=>match c.reason{Some(id)=>id!=0,None=>false}};
            class_valid && root_valid(*pre)&&root_valid(*command)&&root_valid(*context)&&root_valid(c.post)
                &&patch_valid(c.patch@)&&deliveries_valid(c.effects@)&&deliveries_valid(c.outbox@)
                &&match c.class {Class::Reject=>empty_record(c.post)&&c.patch@.len()==0&&c.effects@.len()==0&&c.outbox@.len()==0,
                _=>matching_roots(*pre,c.post)}
        },
    }
}
}
verus! {
pub(in super::super) open spec fn kind_tag(kind:Kind)->u8 {
    match kind {Kind::StateInvariant=>0,Kind::AssetConservation=>1,Kind::MintBurnAuthorization=>2,
        Kind::DebitCreditEffectEquality=>3,Kind::FeeAndRounding=>4,Kind::AuthoritySubjectRecipient=>5,
        Kind::RejectNoAuthority=>6,Kind::CommittedFailureEffects=>7,Kind::DecisionConformance=>8,Kind::InitialCondition=>9}
}
pub(in super::super) open spec fn law_scope(law:&Law)->bool {
    match law.kind {
        Kind::StateInvariant=>law.scope==Scope::Committing&&law.genesis,
        Kind::RejectNoAuthority=>law.scope==Scope::Reject&&!law.genesis,
        Kind::CommittedFailureEffects=>law.scope==Scope::CommittedFailure&&!law.genesis,
        Kind::DecisionConformance=>law.scope==Scope::Always&&!law.genesis,
        Kind::InitialCondition=>law.scope==Scope::Always&&law.genesis,
        _=>true,
    }
}
pub(in super::super) open spec fn applies(law:&Law,frame:&Frame)->bool {
    match frame {
        Frame::Genesis{..}=>law.genesis,
        Frame::Transition{candidate,..}=>law.kind!=Kind::InitialCondition&&match law.scope {
            Scope::Always=>true,Scope::Accept=>candidate.class==Class::Accept,
            Scope::Reject=>candidate.class==Class::Reject,Scope::CommittedFailure=>candidate.class==Class::CommittedFailure,
            Scope::Committing=>candidate.class!=Class::Reject,
        },
    }
}
pub(in super::super) open spec fn has_kind(laws:Seq<Law>,tag:u8)->bool {exists|i:int| 0<=i<laws.len() && kind_tag(laws[i].kind)==tag}
pub(in super::super) open spec fn has_id(laws:Seq<Law>,id:u32)->bool {exists|i:int| 0<=i<laws.len() && laws[i].id==id}
pub(in super::super) open spec fn law_valid(laws:Seq<Law>,i:int)->bool {
    laws[i].id!=0&&law_scope(&laws[i])&&shape(&laws[i].program)
        &&forall|j:int| 0<=j<i ==> laws[j].id!=laws[i].id
}
pub(in super::super) open spec fn required_valid(laws:Seq<Law>,required:Seq<u32>,i:int)->bool {
    required[i]!=0&&has_id(laws,required[i])&&forall|j:int| 0<=j<i ==> required[j]!=required[i]
}
pub(in super::super) open spec fn metadata(laws:Seq<Law>,required:Seq<u32>)->bool {
    (forall|i:int| 0<=i<laws.len() ==> law_valid(laws,i))
        &&(forall|i:int| 0<=i<required.len() ==> required_valid(laws,required,i))
        &&has_kind(laws,0)&&has_kind(laws,6)&&has_kind(laws,7)&&has_kind(laws,8)&&has_kind(laws,9)
}
}
verus! {
pub(in super::super) open spec fn law_prefix(laws:Seq<Law>,frame:&Frame,n:nat,limits:Seq<u64>,used:Seq<u64>,diagnostics:Seq<Diagnostic>,reads:Seq<ReadAttempt>)
    ->(Result<(),Failure>,Seq<u64>,Seq<Diagnostic>,Seq<ReadAttempt>)
    decreases n,
{
    if n==0 {(Ok(()),used,diagnostics,reads)}else{
        let prior=law_prefix(laws,frame,(n-1) as nat,limits,used,diagnostics,reads);
        if prior.0.is_err(){prior}else{
            let law=laws[n-1];
            if !applies(&law,frame){(Ok(()),prior.1,prior.2.push(Diagnostic{id:law.id,verdict:Verdict::Skipped}),prior.3)}else{
                let result=predicate(&law.program,frame,law.id,limits,prior.1,prior.3);
                let verdict=match result.0{Ok(())=>Verdict::Satisfied,Err(e)=>Verdict::Refused(e)};
                (result.0,result.1,prior.2.push(Diagnostic{id:law.id,verdict}),result.2)
            }
        }
    }
}
pub(in super::super) proof fn failed_laws(laws:Seq<Law>,frame:&Frame,n:nat,end:nat,limits:Seq<u64>,used:Seq<u64>,diagnostics:Seq<Diagnostic>,reads:Seq<ReadAttempt>,e:Failure)
    requires n<=end<=laws.len(),law_prefix(laws,frame,n,limits,used,diagnostics,reads).0==Err::<(),Failure>(e),
    ensures law_prefix(laws,frame,end,limits,used,diagnostics,reads)==law_prefix(laws,frame,n,limits,used,diagnostics,reads),
    decreases end-n,
{if n<end{failed_laws(laws,frame,n,(end-1) as nat,limits,used,diagnostics,reads,e);}}
pub(in super::super) open spec fn execution(laws:Seq<Law>,required:Seq<u32>,frame:&Frame,limits:Seq<u64>,used:Seq<u64>,diagnostics:Seq<Diagnostic>,reads:Seq<ReadAttempt>)
    ->(Result<(),Failure>,Seq<u64>,Seq<Diagnostic>,Seq<ReadAttempt>) {
    if !metadata(laws,required){(Err(Failure::Metadata),used,diagnostics,reads)}
    else if !frame_valid(frame){(Err(Failure::Frame),used,diagnostics,reads)}
    else {law_prefix(laws,frame,laws.len(),limits,used,diagnostics,reads)}
}
}

verus! {
pub(in super::super) proof fn values_push(v:Seq<Atom>,a:Atom)
    ensures values(v.push(a)) == values(v).push(atom(a)),
{assert(values(v.push(a)) =~= values(v).push(atom(a)));}
}

verus! {
pub(in super::super) open spec fn magnitude(v:int)->int {if v<0{-v}else{v}}
pub(in super::super) open spec fn division(mode:Division,left:Value,right:Value)->Result<Value,Failure> {
    match (left,right) {
        (Value::I128(a),Value::I128(b))=>{
            if b==0 || (a==i128::MIN&&b == -1){Err(Failure::Undefined)}else{
                let negative=(a<0)!=(b<0);let x=magnitude(a as int);let y=magnitude(b as int);
                let q=x/y;let r=x%y;
                if mode==Division::Exact&&r!=0{Err(Failure::Undefined)}else{
                    let round=r!=0&&((mode==Division::Floor&&negative)||(mode==Division::Ceil&&!negative));
                    let result=if round{q+1}else{q};
                    if result>u128::MAX{Err(Failure::Undefined)}else{integer(if negative{-result}else{result},true)}
                }
            }
        },
        (Value::U128(a),Value::U128(b))=>{
            if b==0{Err(Failure::Undefined)}else{
                let q=a as int/b as int;let r=a as int%b as int;
                if mode==Division::Exact&&r!=0{Err(Failure::Undefined)}
                else {integer(if mode==Division::Ceil&&r!=0{q+1}else{q},false)}
            }
        },
        _=>Err(Failure::Undefined),
    }
}
pub(in super::super) open spec fn to_i128(value:Value)->Result<Value,Failure> {
    match value {
        Value::I128(v)=>Ok(Value::I128(v)),Value::Bool(v)=>Ok(Value::I128(if v{1}else{0})),
        Value::Enum(_,v)|Value::Sum(_,v)=>Ok(Value::I128(v as i128)),
        Value::U128(v)=>integer(v as int,true),_=>Err(Failure::Undefined),
    }
}
}
verus! {
/// Every applicable declared law succeeds if and only if the successful prefix
/// reaches the end; this direction rules out selective omission on success.
pub(in super::super) proof fn success_checks_every_applicable(laws:Seq<Law>,frame:&Frame,n:nat,
    limits:Seq<u64>,used:Seq<u64>,diagnostics:Seq<Diagnostic>,reads:Seq<ReadAttempt>)
    requires n<=laws.len(),law_prefix(laws,frame,n,limits,used,diagnostics,reads).0==Ok::<(),Failure>(()),
    ensures forall|i:int| 0<=i<n && applies(&laws[i],frame) ==>
        predicate(&laws[i].program,frame,laws[i].id,limits,
            law_prefix(laws,frame,i as nat,limits,used,diagnostics,reads).1,
            law_prefix(laws,frame,i as nat,limits,used,diagnostics,reads).3).0==Ok::<(),Failure>(()),
    decreases n,
{
    if n>0 {
        success_checks_every_applicable(laws,frame,(n-1) as nat,limits,used,diagnostics,reads);
    }
}
}

verus! {
pub(in super::super) open spec fn view_field(value:RootView,id:u16)->Option<Value> {
    match value {RootView::Record(fields)=>field(fields@,id,fields@.len()),RootView::Leaf(_)=>None,}
}
pub(in super::super) open spec fn view_atom(value:RootView)->Option<Value> {
    match value {RootView::Leaf(a)=>Some(atom(a)),RootView::Record(_)=>None,}
}
pub(in super::super) open spec fn record_length(value:RootView)->Option<Value> {
    match value {RootView::Record(fields)=>Some(Value::U128(fields@.len() as u128)),RootView::Leaf(_)=>None,}
}
pub(in super::super) open spec fn root_valid(value:RootView)->bool {
    match value {RootView::Record(fields)=>record_valid(fields@),RootView::Leaf(_)=>true,}
}
pub(in super::super) open spec fn empty_record(value:RootView)->bool {
    match value {RootView::Record(fields)=>fields@.len()==0,RootView::Leaf(_)=>false,}
}
pub(in super::super) open spec fn matching_roots(pre:RootView,post:RootView)->bool {
    match (pre,post) {
        (RootView::Record(a),RootView::Record(b))=>matching_fields(a@,b@),
        (RootView::Leaf(a),RootView::Leaf(b))=>same_type(atom(a),atom(b)),
        _=>false,
    }
}
}
