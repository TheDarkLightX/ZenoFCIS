//! Total mathematical admission, atomic chunks and exact completed output.
use super::super::super::evaluation::{admission::spec as graph_admission, spec as scalar};
use super::super::spec as work;
use super::*;
use vstd::prelude::*;
verus! {
pub struct GraphView { pub inputs:Seq<Domain>,pub outputs:Seq<Domain>,pub nodes:Seq<Op>,pub roots:Seq<u16> }
pub struct FoldView { pub graph:GraphView,pub initial:Seq<i64>,pub items:Seq<Seq<i64>>,
    pub accumulator:Seq<i64>,pub context:Context,pub limits:PreparationLimits,
    pub processed:u32,pub budget:Seq<u64>,pub used:Seq<u64>,pub reserved:Seq<u64> }
pub(super) open spec fn items(v:Seq<Vec<i64>>)->Seq<Seq<i64>> { v.map(|_:int,x:Vec<i64>|x@) }
pub(super) open spec fn graph_failure(e:admission::AdmissionFailure)->GraphFailure {
    match e { admission::AdmissionFailure::Shape=>GraphFailure::Shape,
        admission::AdmissionFailure::InputReference=>GraphFailure::InputReference,
        admission::AdmissionFailure::NodeReference=>GraphFailure::NodeReference,
        admission::AdmissionFailure::TypeMismatch=>GraphFailure::TypeMismatch,
        admission::AdmissionFailure::OutputType=>GraphFailure::OutputType }
}
pub(super) open spec fn graph(a:Seq<Domain>,b:Seq<Domain>,n:Seq<Op>,r:Seq<u16>)->Result<GraphView,GraphFailure> {
    match graph_admission::program(a,b,n,r) { Err(e)=>Err(graph_failure(e)),
        Ok(())=>Ok(GraphView{inputs:a,outputs:b,nodes:n,roots:r}) }
}
pub(super) proof fn admitted_shape(g:GraphView)
requires graph_admission::program(g.inputs,g.outputs,g.nodes,g.roots)==Ok(()),
ensures graph_admission::shape_valid(g.inputs,g.outputs,g.nodes.len(),g.roots), {}
pub(super) open spec fn same_domain(a:Domain,b:Domain)->bool { a==b }
pub(super) open spec fn prefix_matches(a:Seq<Domain>,b:Seq<Domain>)->bool {
    b.len()<=a.len() && forall|i:int| 0<=i<b.len() ==> same_domain(a[i],b[i])
}
pub(super) open spec fn context_equal(a:Context,b:Context)->bool {
    a.state_root@==b.state_root@ && a.state_version==b.state_version && a.invocation_hash@==b.invocation_hash@
}
pub(super) open spec fn context_valid(c:Context)->bool {
    c.state_root@!=Seq::new(32,|_:int|0u8) && c.invocation_hash@!=Seq::new(32,|_:int|0u8)
}
pub(super) open spec fn capacity(resource:Capacity,required:u64,declared:u64)->Result<(),Failure> {
    if required>declared { Err(Failure::Capacity{resource,required,declared}) } else { Ok(()) }
}
pub(super) open spec fn item_bytes(g:GraphView)->int { 5+17*(g.inputs.len()-g.outputs.len()) }
pub(super) open spec fn input_bytes(g:GraphView,items:Seq<Seq<i64>>)->int {
    15+17*g.outputs.len()+items.len()*item_bytes(g)
}
pub(super) open spec fn item_prefix(g:GraphView,v:Seq<Seq<i64>>,n:nat,limit:u64)->Result<u64,Failure>
recommends n<=v.len(),g.outputs.len()<=g.inputs.len(),
decreases n,
{
    let total=input_bytes(g,v) as u64;
    if n==0 {
        let base=(15+17*g.outputs.len()) as u64;
        if base>limit { Err(Failure::Capacity{resource:Capacity::InputBytes,required:total,declared:limit}) }
        else { Ok(base) }
    } else { match item_prefix(g,v,(n-1) as nat,limit) {
        Err(e)=>Err(e),Ok(prefix)=>{
            if !scalar::admitted(g.inputs.subrange(g.outputs.len() as int,g.inputs.len() as int),v[n-1]) {
                Err(Failure::InvalidItem{item:(n-1) as u32})
            } else {
                let next=(prefix as int+item_bytes(g)) as u64;
                if next>limit { Err(Failure::Capacity{resource:Capacity::InputBytes,required:total,declared:limit}) }
                else { Ok(next) }
            }
        }
    } }
}
pub(super) proof fn item_failure(g:GraphView,v:Seq<Seq<i64>>,failed:nat,n:nat,limit:u64,e:Failure)
requires failed<=n<=v.len(),g.outputs.len()<=g.inputs.len(),item_prefix(g,v,failed,limit)==Err(e),
ensures item_prefix(g,v,n,limit)==Err(e),
decreases n-failed,
{ if n>failed { item_failure(g,v,failed,(n-1) as nat,limit,e); } }
pub(super) open spec fn reserve(g:GraphView,v:Seq<Seq<i64>>,budget:Seq<u64>)->Result<Seq<u64>,Failure> {
    let zero=Seq::new(8,|_:int|0u64);
    let r=work::charge(budget,zero,Resource::Read,(v.len()*g.inputs.len()) as u64);
    match r.0 { Err(e)=>Err(Failure::Budget(e)),Ok(())=>{
        let w=work::charge(budget,r.1,Resource::Write,(v.len()*g.outputs.len()) as u64);
        match w.0 { Err(e)=>Err(Failure::Budget(e)),Ok(())=>{
            let c=work::charge(budget,w.1,Resource::Candidate,v.len() as u64);
            match c.0 { Err(e)=>Err(Failure::Budget(e)),Ok(())=>{
                let b=work::charge(budget,c.1,Resource::Byte,(input_bytes(g,v)+5+17*g.outputs.len()) as u64);
                match b.0 { Err(e)=>Err(Failure::Budget(e)),Ok(())=>Ok(b.1) }
            } }
        } }
    } }
}

pub(super) open spec fn ready_inputs(g:GraphView,a:Seq<i64>,v:Seq<Seq<i64>>,c:Context,l:PreparationLimits)->bool {
    graph_admission::program(g.inputs,g.outputs,g.nodes,g.roots)==Ok(())
        && l.max_items<=65536 && 0<l.max_chunk_items<=65536
        && l.max_input_bytes<=16777216 && l.max_output_bytes<=16777216
        && context_valid(c) && prefix_matches(g.inputs,g.outputs) && scalar::admitted(g.outputs,a)
        && v.len()<=l.max_items && v.len()*g.nodes.len()<=100000000
        && 5+17*g.outputs.len()<=l.max_output_bytes
}
pub(super) open spec fn start(g:GraphView,a:Seq<i64>,v:Seq<Seq<i64>>,c:Context,l:PreparationLimits,b:Seq<u64>)
    ->Result<FoldView,Failure> {
    match graph_admission::program(g.inputs,g.outputs,g.nodes,g.roots) {
        Err(e)=>Err(Failure::Graph(graph_failure(e))),Ok(())=>{
            if l.max_items>65536 || l.max_chunk_items==0 || l.max_chunk_items>65536
                || l.max_input_bytes>16777216 || l.max_output_bytes>16777216 { Err(Failure::Invalid(Invalid::Limits)) }
            else if !context_valid(c) { Err(Failure::Invalid(Invalid::Context)) }
            else if !prefix_matches(g.inputs,g.outputs) || !scalar::admitted(g.outputs,a) { Err(Failure::Invalid(Invalid::Accumulator)) }
            else if v.len()>l.max_items { Err(Failure::Capacity{resource:Capacity::Items,required:v.len() as u64,declared:l.max_items as u64}) }
            else if v.len()*g.nodes.len()>100000000 { Err(Failure::Capacity{resource:Capacity::Steps,required:(v.len()*g.nodes.len()) as u64,declared:100000000}) }
            else if 5+17*g.outputs.len()>l.max_output_bytes { Err(Failure::Capacity{resource:Capacity::OutputBytes,required:(5+17*g.outputs.len()) as u64,declared:l.max_output_bytes}) }
            else { match item_prefix(g,v,v.len(),l.max_input_bytes) {
                Err(e)=>Err(e),Ok(_)=>{match reserve(g,v,b) {
                    Err(e)=>Err(e),Ok(used)=>{
                        if 3+g.outputs.len()+v.len()*(1+g.inputs.len()-g.outputs.len())>1000000 {
                            Err(Failure::EncodingNodes{limit:1000000,attempted:1000001})
                        } else { Ok(FoldView{graph:g,initial:a,items:v,accumulator:a,context:c,limits:l,
                            processed:0,budget:b,used:used,reserved:used}) }
                    }
                }}
            } }
        }
    }
}
pub(super) open spec fn start_result(r:Result<PreparedFold,Failure>)->Result<FoldView,Failure> {
    match r { Err(e)=>Err(e),Ok(p)=>Ok(p.view()) }
}
pub(super) open spec fn fold(g:GraphView,v:Seq<Seq<i64>>,start:nat,end:nat,a:Seq<i64>,b:Seq<u64>,used:Seq<u64>)
    ->(Result<Seq<i64>,Failure>,Seq<u64>)
recommends start<=end<=v.len(),b.len()==8,used.len()==8,
decreases end-start,
{
    if end<=start { (Ok(a),used) }
    else {
        let p=fold(g,v,start,(end-1) as nat,a,b,used);
        match p.0 { Err(e)=>p,Ok(acc)=>{
            let r=work::execution(g.inputs,g.outputs,g.nodes,g.roots,acc+v[end-1],b,p.1);
            match r.0 { Err(e)=>(Err(Failure::Evaluation{item:(end-1) as u32,source:e}),r.1),Ok(out)=>(Ok(out),r.1) }
        } }
    }
}
pub(super) proof fn fold_failure(g:GraphView,v:Seq<Seq<i64>>,start:nat,failed:nat,end:nat,a:Seq<i64>,b:Seq<u64>,used:Seq<u64>,item:u32,e:EvaluationFailure)
requires start<=failed<=end<=v.len(),b.len()==8,used.len()==8,
    fold(g,v,start,failed,a,b,used).0==Err(Failure::Evaluation{item,source:e}),
ensures fold(g,v,start,end,a,b,used)==fold(g,v,start,failed,a,b,used),
decreases end-failed,
{ if end>failed { fold_failure(g,v,start,failed,(end-1) as nat,a,b,used,item,e); } }
pub(super) open spec fn immutable(a:FoldView,b:FoldView)->bool {
    a.graph==b.graph && a.initial==b.initial && a.items==b.items && a.context==b.context
        && a.limits==b.limits && a.budget==b.budget && a.reserved==b.reserved
}
pub(super) open spec fn advance_result(a:FoldView,b:FoldView,r:Result<(),Failure>,offset:u32,count:u32)->bool {
    immutable(a,b) && if offset!=a.processed {
        r==Err(Failure::WrongOffset{expected:a.processed,supplied:offset}) && b==a
    } else if offset as int+count as int>u32::MAX || count==0 || count>a.limits.max_chunk_items
        || offset as int+count as int>a.items.len() {
        r==Err(Failure::InvalidChunk) && b==a
    } else {
        let f=fold(a.graph,a.items,offset as nat,(offset as int+count as int) as nat,a.accumulator,a.budget,a.used);
        b.used==f.1 && match f.0 {
            Err(e)=>r==Err(e) && b.accumulator==a.accumulator && b.processed==a.processed,
            Ok(out)=>r==Ok(()) && b.accumulator==out && b.processed==offset+count,
        }
    }
}
pub(super) open spec fn finish(p:FoldView,c:Context)->Result<Seq<i64>,Failure> {
    if p.processed!=p.items.len() { Err(Failure::Incomplete) }
    else if !context_equal(p.context,c) { Err(Failure::StaleContext) }
    else { Ok(p.accumulator) }
}
pub(super) open spec fn remaining(p:FoldView)->u32 {
    if p.items.len() as u32>=p.processed { (p.items.len() as u32-p.processed) as u32 } else { 0 }
}
pub(super) open spec fn same_operation(a:FoldView,b:FoldView)->bool {
    a.graph==b.graph && a.initial==b.initial && a.items==b.items && context_equal(a.context,b.context)
        && a.limits==b.limits && a.budget==b.budget
}
pub(super) proof fn metered_prefix_sound(nodes:Seq<Op>,input:Seq<i64>,count:nat,b:Seq<u64>,used:Seq<u64>)
requires count<=nodes.len(),b.len()==8,used.len()==8,
ensures work::prefix(nodes,input,count,b,used).1.len()==8,
    match work::prefix(nodes,input,count,b,used).0 {
        Ok(out)=>scalar::prefix(nodes,input,count)==Ok(out),Err(_)=>true,
    },
decreases count,
{ if count>0 { metered_prefix_sound(nodes,input,(count-1) as nat,b,used); } }
pub(super) proof fn metered_execution_sound(g:GraphView,input:Seq<i64>,b:Seq<u64>,used:Seq<u64>)
requires b.len()==8,used.len()==8,
ensures work::execution(g.inputs,g.outputs,g.nodes,g.roots,input,b,used).1.len()==8,
    match work::execution(g.inputs,g.outputs,g.nodes,g.roots,input,b,used).0 {
        Ok(out)=>scalar::execution(g.inputs,g.outputs,g.nodes,g.roots,input)==Ok(out),Err(_)=>true,
    },
{ metered_prefix_sound(g.nodes,input,g.nodes.len(),b,used); }
pub(super) open spec fn pure_fold(g:GraphView,v:Seq<Seq<i64>>,start:nat,end:nat,a:Seq<i64>)
    ->Result<Seq<i64>,evaluation::Failure>
recommends start<=end<=v.len(),
decreases end-start,
{ if end<=start { Ok(a) } else { match pure_fold(g,v,start,(end-1) as nat,a) {
    Err(e)=>Err(e),Ok(acc)=>scalar::execution(g.inputs,g.outputs,g.nodes,g.roots,acc+v[end-1]),
} } }
pub(super) proof fn fold_sound(g:GraphView,v:Seq<Seq<i64>>,start:nat,end:nat,a:Seq<i64>,b:Seq<u64>,used:Seq<u64>)
requires start<=end<=v.len(),b.len()==8,used.len()==8,
ensures fold(g,v,start,end,a,b,used).1.len()==8,
    match fold(g,v,start,end,a,b,used).0 { Err(_)=>true,Ok(out)=>pure_fold(g,v,start,end,a)==Ok(out) },
decreases end-start,
{
    if end>start {
        fold_sound(g,v,start,(end-1) as nat,a,b,used);
        let previous=fold(g,v,start,(end-1) as nat,a,b,used);
        if let Ok(acc)=previous.0 { metered_execution_sound(g,acc+v[end-1],b,previous.1); }
    }
}
pub(super) proof fn pure_split(g:GraphView,v:Seq<Seq<i64>>,start:nat,mid:nat,end:nat,a:Seq<i64>,acc:Seq<i64>)
requires start<=mid<=end<=v.len(),pure_fold(g,v,start,mid,a)==Ok(acc),
ensures pure_fold(g,v,start,end,a)==pure_fold(g,v,mid,end,acc),
decreases end-mid,
{ if end>mid { pure_split(g,v,start,mid,(end-1) as nat,a,acc); } }
pub(super) open spec fn valid_state(p:FoldView)->bool {
    p.processed<=p.items.len() && pure_fold(p.graph,p.items,0,p.processed as nat,p.initial)==Ok(p.accumulator)
}
pub(super) proof fn advance_preserves_prefix(a:FoldView,b:FoldView,r:Result<(),Failure>,offset:u32,count:u32)
requires a.budget.len()==8,a.used.len()==8,valid_state(a),advance_result(a,b,r,offset,count),
ensures valid_state(b),
{
    if r==Ok(()) {
        let end=(offset as int+count as int) as nat;
        fold_sound(a.graph,a.items,offset as nat,end,a.accumulator,a.budget,a.used);
        pure_split(a.graph,a.items,0,offset as nat,end,a.initial,a.accumulator);
    }
}

}
