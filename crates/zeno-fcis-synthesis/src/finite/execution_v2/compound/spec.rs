//! Mathematical original-wire grammar as a typed parsing stack and exact meter.
//! A Value obligation consumes one canonical tag and its declared body. Tuple,
//! Record, Vector and Map frames expose every original child obligation. Map
//! Boundary frames require complete local consumption before restoring scope.
use super::super::spec::charge;
use super::*;
use crate::finite::canonical_v2::spec as integer;
use vstd::prelude::*;
verus! {
pub(in super::super) open spec fn find<'a>(definitions:Seq<schema::Definition<'a>>,id:u32,n:nat)->Option<schema::Definition<'a>>
    recommends n<=definitions.len(),decreases n,
{ if n==0 {None} else {match find(definitions,id,(n-1) as nat){Some(d)=>Some(d),None=>if definitions[n as int-1].id==id {Some(definitions[n as int-1])} else {None}}} }
pub(super) proof fn find_found(definitions:Seq<schema::Definition>,id:u32,n:nat,total:nat)
    requires n<=total<=definitions.len(),find(definitions,id,n).is_some(),
    ensures find(definitions,id,total)==find(definitions,id,n),decreases total-n,
{if total>n {find_found(definitions,id,n,(total-1) as nat);}}
pub(super) open spec fn read(bytes:Seq<u8>,position:usize,width:usize,limit:usize)->Option<(u128,usize)> {
    match integer::read_unsigned(bytes,position,width){None=>None,Some((v,end))=>if end>limit {None} else {Some((v,end))}}
}
pub(super) open spec fn signed(bytes:Seq<u8>,position:usize,limit:usize)->Option<(i128,usize)> {
    match integer::read_signed(bytes,position){None=>None,Some((v,end))=>if end>limit {None} else {Some((v,end))}}
}
pub(super) open spec fn blob(bytes:Seq<u8>,position:usize,limit:usize)->Option<(usize,usize)> {
    match read(bytes,position,4,limit){None=>None,Some((length,start))=>{
        let end=start as int+length as usize as int;
        if end>usize::MAX || end>limit || end>bytes.len() {None} else {Some((start,end as usize))}
    }}
}
pub(super) open spec fn ascii(bytes:Seq<u8>,start:usize,end:usize)->bool {
    start<=end<=bytes.len() && forall|i:int|start<=i<end==>bytes[i]<128
}
pub(super) open spec fn lex_at(bytes:Seq<u8>,a:(usize,usize),b:(usize,usize),i:int)->bool {
    0<=i<a.1 as int-a.0 as int && i<b.1 as int-b.0 as int
    && (forall|j:int|0<=j<i==>#[trigger] bytes[a.0 as int+j]==bytes[b.0 as int+j])
    && bytes[a.0 as int+i]<bytes[b.0 as int+i]
}
pub(super) open spec fn less(bytes:Seq<u8>,a:(usize,usize),b:(usize,usize))->bool {
    a.0<=a.1<=bytes.len() && b.0<=b.1<=bytes.len() && (
        (exists|i:int|lex_at(bytes,a,b,i)) || (
            (a.1 as int-a.0 as int)<(b.1 as int-b.0 as int)
            && forall|i:int|0<=i<a.1 as int-a.0 as int==>#[trigger] bytes[a.0 as int+i]==bytes[b.0 as int+i]
        )
    )
}
pub(super) proof fn lex_different(bytes:Seq<u8>,a:(usize,usize),b:(usize,usize),i:int)
    requires a.0<=a.1<=bytes.len(),b.0<=b.1<=bytes.len(),0<=i<a.1 as int-a.0 as int,i<b.1 as int-b.0 as int,
        forall|j:int|0<=j<i==>#[trigger] bytes[a.0 as int+j]==bytes[b.0 as int+j],bytes[a.0 as int+i]>bytes[b.0 as int+i],
    ensures !less(bytes,a,b),
{
    assert forall|j:int| !lex_at(bytes,a,b,j) by {
        if 0<=j && j<i {assert(bytes[a.0 as int+j]==bytes[b.0 as int+j]);}
        if j>i {assert(!(forall|k:int|0<=k<j==>#[trigger] bytes[a.0 as int+k]==bytes[b.0 as int+k]));}
    }
}
pub(super) open spec fn variant(variants:Seq<schema::Variant>,id:u16)->bool {exists|i:int|0<=i<variants.len()&&variants[i].id==id}
pub(super) open spec fn sum_variant(variants:Seq<schema::SumVariant>,id:u16,n:nat)->Option<Option<u32>>
    recommends n<=variants.len(),decreases n,
{if n==0 {None} else {match sum_variant(variants,id,(n-1) as nat){Some(v)=>Some(v),None=>if variants[n as int-1].id==id {Some(variants[n as int-1].payload)} else {None}}}}
pub(super) proof fn sum_found(variants:Seq<schema::SumVariant>,id:u16,n:nat,total:nat)
    requires n<=total<=variants.len(),sum_variant(variants,id,n).is_some(),
    ensures sum_variant(variants,id,total)==sum_variant(variants,id,n),decreases total-n,
{if total>n {sum_found(variants,id,n,(total-1) as nat);}}

pub(super) open spec fn scan<'a>(bytes:Seq<u8>,definitions:Seq<schema::Definition<'a>>,position:usize,limit:usize,id:u32)->Option<(usize,Option<Task<'a>>)> {
    match find(definitions,id,definitions.len()) {None=>None,Some(def)=>match read(bytes,position,1,limit) {None=>None,Some((tag,body))=>match def.kind {
        schema::Kind::Unit=>if tag==0 {Some((body,None))} else {None},
        schema::Kind::Bool=>if tag==1||tag==2 {Some((body,None))} else {None},
        schema::Kind::U128{min,max}=>if tag!=3 {None} else {match read(bytes,body,16,limit){None=>None,Some((v,end))=>if min<=v<=max {Some((end,None))} else {None}}},
        schema::Kind::I128{min,max}=>if tag!=4 {None} else {match signed(bytes,body,limit){None=>None,Some((v,end))=>if min<=v<=max {Some((end,None))} else {None}}},
        schema::Kind::Bytes{min,max}|schema::Kind::Text{min,max}=>{
            let text=matches!(def.kind,schema::Kind::Text{..});
            if tag!=if text {6u128} else {5u128} {None} else {match blob(bytes,body,limit){None=>None,Some((start,end))=>
                if (end as int-start as int)<min || end as int-start as int>max || (text&&!ascii(bytes,start,end)) {None} else {Some((end,None))}}}
        },
        schema::Kind::Enum(variants)|schema::Kind::Sum(variants)=>{
            let sum=matches!(def.kind,schema::Kind::Sum(_));
            if tag!=if sum {10u128} else {7u128} {None} else {match read(bytes,body,4,limit){None=>None,Some((t,p))=>{
                if t!=id {None} else {match read(bytes,p,2,limit){None=>None,Some((v,end))=>{
                    if !variant(variants@,v as u16) {None} else if sum {match read(bytes,end,1,limit){Some((0,next))=>Some((next,None)),_=>None}} else {Some((end,None))}
                }}}
            }}}
        },
        schema::Kind::SumPayload(variants)=>{
            if tag!=10 {None} else {match read(bytes,body,4,limit){None=>None,Some((t,p))=>{
                if t!=id {None} else {match read(bytes,p,2,limit){None=>None,Some((v,p))=>match read(bytes,p,1,limit){None=>None,Some((present,end))=>match sum_variant(variants@,v as u16,variants@.len()){
                    None=>None,Some(None)=>if present==0 {Some((end,None))} else {None},Some(Some(payload))=>if present==1 {Some((end,Some(Task::Value(payload))))} else {None}
                }}}}
            }}}
        },
        schema::Kind::Tuple(items)=>if tag!=8 {None} else {match read(bytes,body,4,limit){Some((count,end))=>if count==items@.len() {Some((end,Some(Task::Tuple(items,0))))} else {None},None=>None}},
        schema::Kind::Record(fields)=>if tag!=9 {None} else {match read(bytes,body,4,limit){Some((count,end))=>if count==fields@.len() {Some((end,Some(Task::Record(fields,0))))} else {None},None=>None}},
        schema::Kind::Vector{element,min,max}=>if tag!=11 {None} else {match read(bytes,body,4,limit){Some((count,end))=>if min<=count<=max {Some((end,Some(Task::Vector(element,count as u32,0))))} else {None},None=>None}},
        schema::Kind::Map{key,value,min,max}=>if tag!=12 {None} else {match read(bytes,body,4,limit){Some((count,end))=>if min<=count<=max {Some((end,Some(Task::Map(key,value,count as u32,0,None))))} else {None},None=>None}},
    }}}
}

pub(super) type StateView<'a>=(usize,usize,Seq<Task<'a>>);
pub(super) open spec fn state<'a>(s:State<'a>)->StateView<'a> {(s.position,s.limit,s.tasks@)}
pub(super) open spec fn task_weight(task:Task)->nat {match task {
    Task::Value(_)|Task::Boundary(_,_)=>1,
    Task::Tuple(items,index)=>if index<=items@.len() {(2*(items@.len()-index)+1) as nat} else {1},
    Task::Record(fields,index)=>if index<=fields@.len() {(2*(fields@.len()-index)+1) as nat} else {1},
    Task::Vector(_,count,index)=>if index<=count {(2*(count as int-index as int)+1) as nat} else {1},
    Task::Map(_,_,count,index,_)=>if index<=count {(7*(count as int-index as int)+1) as nat} else {1},
    Task::MapValue(_,_,count,index,_,_)=>if index<count {(7*(count as int-index as int)-2) as nat} else {1},
}}
pub(super) open spec fn potential(tasks:Seq<Task>)->nat decreases tasks.len(), {
    if tasks.len()==0 {0} else {(potential(tasks.drop_last())+task_weight(tasks.last())) as nat}
}
pub(super) proof fn potential_push(tasks:Seq<Task>,task:Task)
    ensures potential(tasks.push(task))==potential(tasks)+task_weight(task),
{
    assert(tasks.push(task).drop_last()=~=tasks);
    assert(tasks.push(task).last()==task);
    reveal_with_fuel(potential,1);
}
pub(super) open spec fn remaining(bytes:Seq<u8>,s:StateView)->nat {if s.0<=bytes.len() {(bytes.len()-s.0) as nat} else {0}}
pub(super) open spec fn advance<'a>(bytes:Seq<u8>,definitions:Seq<schema::Definition<'a>>,s:StateView<'a>)->Option<StateView<'a>> {
    if s.0>s.1 || s.1>bytes.len() || s.2.len()==0 {None} else {
        let rest=s.2.drop_last();match s.2.last() {
            Task::Value(id)=>match scan(bytes,definitions,s.0,s.1,id){None=>None,Some((end,next))=>Some((end,s.1,match next{None=>rest,Some(t)=>rest.push(t)}))},
            Task::Tuple(items,index)=>if index>items@.len() {None} else if index==items@.len() {Some((s.0,s.1,rest))} else {Some((s.0,s.1,rest.push(Task::Tuple(items,(index+1) as usize)).push(Task::Value(items@[index as int]))))},
            Task::Record(fields,index)=>if index>fields@.len() {None} else if index==fields@.len() {Some((s.0,s.1,rest))} else {match read(bytes,s.0,2,s.1){Some((id,end))=>if id==fields@[index as int].id {Some((end,s.1,rest.push(Task::Record(fields,(index+1) as usize)).push(Task::Value(fields@[index as int].type_id))))} else {None},None=>None}},
            Task::Vector(element,count,index)=>if index>count {None} else if index==count {Some((s.0,s.1,rest))} else {Some((s.0,s.1,rest.push(Task::Vector(element,count,(index+1) as u32)).push(Task::Value(element))))},
            Task::Map(key,value,count,index,prior)=>if index>count {None} else if index==count {Some((s.0,s.1,rest))} else {match blob(bytes,s.0,s.1){None=>None,Some((start,end))=>
                if match prior{None=>false,Some(p)=>!less(bytes,p,(start,end))} {None} else {Some((start,end,rest.push(Task::MapValue(key,value,count,index,start,end)).push(Task::Boundary(end,s.1)).push(Task::Value(key))))}
            }},
            Task::MapValue(key,value,count,index,start,end)=>if index>=count {None} else {match blob(bytes,s.0,s.1){None=>None,Some((a,b))=>Some((a,b,rest.push(Task::Map(key,value,count,(index+1) as u32,Some((start,end)))).push(Task::Boundary(b,s.1)).push(Task::Value(value))))}},
            Task::Boundary(end,limit)=>if s.0!=end||limit<end||limit>bytes.len() {None} else {Some((s.0,limit,rest))},
        }
    }
}
pub(super) proof fn scan_progress(bytes:Seq<u8>,definitions:Seq<schema::Definition>,position:usize,limit:usize,id:u32,end:usize,next:Option<Task>)
    requires bytes.len()<=usize::MAX,limit<=bytes.len(),scan(bytes,definitions,position,limit,id)==Some((end,next)),
    ensures position<end<=limit,
{ }
pub(super) proof fn advance_progress(bytes:Seq<u8>,definitions:Seq<schema::Definition>,before:StateView,after:StateView)
    requires bytes.len()<=usize::MAX,advance(bytes,definitions,before)==Some(after),
        match before.2.last(){Task::Tuple(items,_)=>items@.len()<=usize::MAX,Task::Record(fields,_)=>fields@.len()<=usize::MAX,_=>true},
    ensures before.0<=after.0<=after.1<=bytes.len(),
        remaining(bytes,after)<remaining(bytes,before) || (remaining(bytes,after)==remaining(bytes,before)&&potential(after.2)<potential(before.2)),
{
    let rest=before.2.drop_last();
    let task=before.2.last();
    assert(before.2=~=rest.push(task));
    potential_push(rest,task);
    match task {
        Task::Value(id)=>{let (end,next)=scan(bytes,definitions,before.0,before.1,id).unwrap();scan_progress(bytes,definitions,before.0,before.1,id,end,next);},
        Task::Tuple(items,index)=>{
            if index<items@.len() {
                let next=Task::Tuple(items,(index+1) as usize);
                let child=Task::Value(items@[index as int]);
                potential_push(rest,next);potential_push(rest.push(next),child);
            }
        },
        Task::Vector(element,count,index)=>{
            if index<count {
                let next=Task::Vector(element,count,(index+1) as u32);
                let child=Task::Value(element);
                potential_push(rest,next);potential_push(rest.push(next),child);
            }
        },
        _=>{},
    }
}
#[verifier::opaque]
pub(super) open spec fn run<'a>(bytes:Seq<u8>,definitions:Seq<schema::Definition<'a>>,s:StateView<'a>,limits:Seq<u64>,used:Seq<u64>)->(Result<usize,Failure>,Seq<u64>)
    decreases if used.len()==8 {u64::MAX as int-used[7] as int} else {0int},
{
    if limits.len()!=8 || used.len()!=8 {(Err(Failure::Wire),used)} else if s.2.len()==0 {
        (if s.0>s.1 || s.1>bytes.len() {Err(Failure::Wire)} else {Ok(s.0)},used)
    } else {
        let c=charge(limits,used,Resource::Step,1);match c.0 {
            Err(e)=>(Err(Failure::Budget(e)),c.1),Ok(())=>match advance(bytes,definitions,s){
                None=>(Err(Failure::Wire),c.1),Some(next)=>{
                    run(bytes,definitions,next,limits,c.1)
                },
            },
        }
    }
}
pub(in super::super) open spec fn walk<'a>(bytes:Seq<u8>,definitions:Seq<schema::Definition<'a>>,position:usize,limit:usize,id:u32,limits:Seq<u64>,used:Seq<u64>)->(Result<usize,Failure>,Seq<u64>) {
    super::walked(bytes,definitions,position,limit,id,limits,used)
}

/// The mathematical subslice is uniquely represented by its complete byte view.
pub(super) open spec fn part<'a>(bytes:&'a [u8],start:usize,end:usize)->&'a [u8] {
    slice(bytes@.subrange(start as int,end as int))
}
pub open spec fn slice<'a>(bytes:Seq<u8>)->&'a [u8] {choose|s:&'a [u8]|s@==bytes}
pub(in super::super) proof fn slice_equal(bytes:Seq<u8>,actual:&[u8])
    requires actual@==bytes,
    ensures slice(bytes)==actual,
{
    assert(exists|s:&[u8]|s@==bytes);
    let chosen=slice(bytes);
    assert(chosen@==actual@);
    assert(chosen=~=actual);
}
pub(super) proof fn part_equal(bytes:&[u8],start:usize,end:usize,actual:&[u8])
    requires start<=end<=bytes@.len(),actual@==bytes@.subrange(start as int,end as int),
    ensures part(bytes,start,end)==actual,
{
    slice_equal(bytes@.subrange(start as int,end as int),actual);
}
pub(in super::super) open spec fn materialize<'a>(bytes:&'a [u8],checked:&schema::Checked<'a>,id:u32,start:usize,end:usize)->Option<Atom<'a>> {
    super::materialized(bytes,checked,id,start,end)
}
pub(in super::super) open spec fn decode<'a>(bytes:&'a [u8],checked:&schema::Checked<'a>,start:usize,limit:usize,id:u32,limits:Seq<u64>,used:Seq<u64>)->(Result<(Atom<'a>,usize),Failure>,Seq<u64>) {
    let r=walk(bytes@,checked.view().1.definitions@,start,limit,id,limits,used);
    match r.0 {Err(e)=>(Err(e),r.1),Ok(end)=>match materialize(bytes,checked,id,start,end){None=>(Err(Failure::Wire),r.1),Some(value)=>(Ok((value,end)),r.1)}}
}


pub(super) type FieldsResult<'a>=(Result<(Seq<super::super::decision::Field<'a>>,usize),Failure>,Seq<u64>,Seq<Access>);
#[verifier::opaque]
pub(super) open spec fn record_fields<'a>(bytes:&'a [u8],checked:&schema::Checked<'a>,fields:Seq<schema::Field<'a>>,start:usize,limit:usize,payload:bool,n:nat,limits:Seq<u64>,used:Seq<u64>)->FieldsResult<'a>
    recommends n<=fields.len(),decreases n,
{
    if n==0 {(Ok((Seq::empty(),start)),used,Seq::empty())} else {
        let prior=record_fields(bytes,checked,fields,start,limit,payload,(n-1) as nat,limits,used);
        match prior.0{Err(_)=>prior,Ok((values,position))=>{
            let s=charge(limits,prior.1,Resource::Step,1);
            match s.0{Err(e)=>(Err(Failure::Budget(e)),s.1,prior.2),Ok(())=>{
                let f=fields[n as int-1];let r=charge(limits,s.1,Resource::Read,1);
                let reads=prior.2.push(if payload{Access::PayloadField(f.id,r.0.is_ok())} else {Access::Field(f.id,r.0.is_ok())});
                match r.0{Err(e)=>(Err(Failure::Budget(e)),r.1,reads),Ok(())=>match read(bytes@,position,2,limit){
                    None=>(Err(Failure::Wire),r.1,reads),Some((id,child))=>if id!=f.id {(Err(Failure::Wire),r.1,reads)} else {
                        let v=decode(bytes,checked,child,limit,f.type_id,limits,r.1);
                        match v.0{Err(e)=>(Err(e),v.1,reads),Ok((value,end))=>(Ok((values.push(super::super::decision::Field{id:f.id,value}),end)),v.1,reads)}
                    },
                }}
            }}
        }}
    }
}
pub(super) proof fn record_failed<'a>(bytes:&'a [u8],checked:&schema::Checked<'a>,fields:Seq<schema::Field<'a>>,start:usize,limit:usize,payload:bool,n:nat,total:nat,limits:Seq<u64>,used:Seq<u64>)
    requires n<=total<=fields.len(),record_fields(bytes,checked,fields,start,limit,payload,n,limits,used).0.is_err(),
    ensures record_fields(bytes,checked,fields,start,limit,payload,total,limits,used)==record_fields(bytes,checked,fields,start,limit,payload,n,limits,used),
    decreases total-n,
{reveal_with_fuel(record_fields,2);if total>n {record_failed(bytes,checked,fields,start,limit,payload,n,(total-1) as nat,limits,used);}}
pub(super) type RecordResult<'a>=(Result<(Atom<'a>,Seq<super::super::decision::Field<'a>>,usize),Failure>,Seq<u64>,Seq<Access>);
#[verifier::opaque]
pub(super) open spec fn record<'a>(bytes:&'a [u8],checked:&schema::Checked<'a>,start:usize,limit:usize,id:u32,payload:bool,limits:Seq<u64>,used:Seq<u64>)->RecordResult<'a> {
    let s=charge(limits,used,Resource::Step,1);match s.0{Err(e)=>(Err(Failure::Budget(e)),s.1,Seq::empty()),Ok(())=>match scan(bytes@,checked.view().1.definitions@,start,limit,id){
        Some((body,Some(Task::Record(fields,0))))=>{
            let r=record_fields(bytes,checked,fields@,body,limit,payload,fields@.len(),limits,s.1);
            match r.0{Err(e)=>(Err(e),r.1,r.2),Ok((values,end))=>{
                let f=charge(limits,r.1,Resource::Step,1);match f.0{Err(e)=>(Err(Failure::Budget(e)),f.1,r.2),Ok(())=>match materialize(bytes,checked,id,start,end){Some(atom)=>(Ok((atom,values,end)),f.1,r.2),None=>(Err(Failure::Wire),f.1,r.2)}}
            }}
        },_ =>(Err(Failure::Wire),s.1,Seq::empty()),
    }}
}
pub(in super::super) type RootView<'a>=(Atom<'a>,Option<Seq<super::super::decision::Field<'a>>>,Option<(u16,Option<Seq<super::super::decision::Field<'a>>>)>);
pub(in super::super) open spec fn root_view<'a>(root:DecodedRoot<'a>)->RootView<'a> {(root.atom,match root.fields{Some(f)=>Some(f@),None=>None},match root.sum{Some((v,Some(f)))=>Some((v,Some(f@))),Some((v,None))=>Some((v,None)),None=>None})}
pub(in super::super) type RootResult<'a>=(Result<RootView<'a>,Failure>,Seq<u64>,Seq<Access>);
pub(in super::super) open spec fn decode_root<'a>(bytes:&'a [u8],checked:&schema::Checked<'a>,id:u32,limits:Seq<u64>,used:Seq<u64>)->RootResult<'a> {
    super::decoded_root(bytes,checked,id,limits,used)
}
}
