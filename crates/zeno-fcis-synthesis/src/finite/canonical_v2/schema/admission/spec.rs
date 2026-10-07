//! Closed metadata predicates and monotone-prefix proof helpers.
use super::super::{Definition, Description, Field, Kind, SumVariant, Variant};
use super::{Failure, Limits};
use alloc::vec::Vec;
use vstd::prelude::*;
verus! {
pub open spec fn first(b:u8)->bool{(65<=b<=90)||(97<=b<=122)||b==95}
pub open spec fn character(b:u8)->bool{first(b)||(48<=b<=57)}
pub open spec fn name_valid(bytes:Seq<u8>)->bool{
    0<bytes.len()<=96 && first(bytes[0]) && forall|i:int|0<=i<bytes.len() ==> character(bytes[i])
}
pub open spec fn find(definitions:Seq<Definition>,id:u32,count:nat)->Option<usize>
    recommends count<=definitions.len(),
    decreases count,
{
    if count==0{None}else{match find(definitions,id,(count-1) as nat){Some(p)=>Some(p),None=>
        if definitions[count as int-1].id==id{Some((count-1) as usize)}else{None}}}
}
pub proof fn find_persists(definitions:Seq<Definition>,id:u32,found:nat,count:nat,p:usize)
    requires found<=count<=definitions.len(),find(definitions,id,found)==Some(p),
    ensures find(definitions,id,count)==Some(p),
    decreases count-found,
{if count>found{find_persists(definitions,id,found,(count-1) as nat,p);}}
pub proof fn find_identity(definitions:Seq<Definition>,id:u32,count:nat,p:usize)
    requires count<=definitions.len(),count<=usize::MAX,find(definitions,id,count)==Some(p),
    ensures p<count,definitions[p as int].id==id,
    decreases count,
{
    if count>0{match find(definitions,id,(count-1) as nat){
        Some(q)=>find_identity(definitions,id,(count-1) as nat,q),None=>{},
    }}
}
pub open spec fn leaf(kind:Kind)->bool{!matches!(kind,Kind::Record(_))}
pub open spec fn variants_prefix(v:Seq<Variant>,count:nat)->bool
    recommends count<=v.len(),
    decreases count,
{
    if count==0{true}else{variants_prefix(v,(count-1) as nat)
        &&name_valid(v[count as int-1].name@)
        &&(count==1||v[count as int-2].id<v[count as int-1].id)
        &&(forall|j:int|0<=j<count-1 ==> #[trigger] v[j].name@!=v[count as int-1].name@)}
}
pub proof fn variants_failure(v:Seq<Variant>,failed:nat,count:nat)
    requires failed<=count<=v.len(),!variants_prefix(v,failed),
    ensures !variants_prefix(v,count),
    decreases count-failed,
{if count>failed{variants_failure(v,failed,(count-1) as nat);}}
pub open spec fn variants(v:Seq<Variant>,max:u32)->bool{v.len()<=max&&variants_prefix(v,v.len())}
pub open spec fn sum_variants_prefix(v:Seq<SumVariant>,count:nat)->bool
    recommends count<=v.len(),
    decreases count,
{
    if count==0{true}else{sum_variants_prefix(v,(count-1) as nat)
        &&name_valid(v[count as int-1].name@)
        &&(count==1||v[count as int-2].id<v[count as int-1].id)
        &&(forall|j:int|0<=j<count-1 ==> #[trigger] v[j].name@!=v[count as int-1].name@)}
}
pub proof fn sum_variants_failure(v:Seq<SumVariant>,failed:nat,count:nat)
    requires failed<=count<=v.len(),!sum_variants_prefix(v,failed),
    ensures !sum_variants_prefix(v,count),
    decreases count-failed,
{if count>failed{sum_variants_failure(v,failed,(count-1) as nat);}}
pub open spec fn sum_variants(v:Seq<SumVariant>,max:u32)->bool{v.len()<=max&&sum_variants_prefix(v,v.len())}
pub open spec fn field_valid(f:Field,definitions:Seq<Definition>)->bool{
    name_valid(f.name@)&&match find(definitions,f.type_id,definitions.len()){
        Some(p)=>p<definitions.len(),None=>false,
    }
}
pub open spec fn fields_prefix(fields:Seq<Field>,definitions:Seq<Definition>,count:nat)->bool
    recommends count<=fields.len(),
    decreases count,
{
    if count==0{true}else{fields_prefix(fields,definitions,(count-1) as nat)
        &&field_valid(fields[count as int-1],definitions)
        &&(count==1||fields[count as int-2].id<fields[count as int-1].id)
        &&(forall|j:int|0<=j<count-1 ==>#[trigger] fields[j].name@!=fields[count as int-1].name@)}
}
pub proof fn fields_failure(fields:Seq<Field>,definitions:Seq<Definition>,failed:nat,count:nat)
    requires failed<=count<=fields.len(),!fields_prefix(fields,definitions,failed),
    ensures !fields_prefix(fields,definitions,count),
    decreases count-failed,
{if count>failed{fields_failure(fields,definitions,failed,(count-1) as nat);}}
pub open spec fn fields(fields:Seq<Field>,definitions:Seq<Definition>,max:u32)->bool{
    fields.len()<=max&&fields_prefix(fields,definitions,fields.len())
}
pub open spec fn kind_valid(kind:Kind,definitions:Seq<Definition>,limits:Limits)->bool{
    match kind{
        Kind::Unit|Kind::Bool=>true,
        Kind::I128{min,max}=>min<=max,Kind::U128{min,max}=>min<=max,
        Kind::Bytes{min,max}|Kind::Text{min,max}=>min<=max,
        Kind::Enum(v)|Kind::Sum(v)=>variants(v@,limits.variants),
        Kind::Record(f)=>fields(f@,definitions,limits.fields),
        Kind::Tuple(items)=>items@.len()<=limits.fields,
        Kind::SumPayload(v)=>sum_variants(v@,limits.variants),
        Kind::Vector{min,max,..}|Kind::Map{min,max,..}=>min<=max,
    }
}
pub open spec fn field_count(kind:Kind)->nat{match kind{Kind::Record(f)=>f@.len(),_=>0}}
pub open spec fn variant_count(kind:Kind)->nat{match kind{Kind::Enum(v)|Kind::Sum(v)=>v@.len(),Kind::SumPayload(v)=>v@.len(),_=>0}}
pub proof fn kind_counts_bounded(kind:Kind,definitions:Seq<Definition>,limits:Limits)
    requires kind_valid(kind,definitions,limits),
    ensures field_count(kind)<=u32::MAX,variant_count(kind)<=u32::MAX,
{match kind{Kind::Record(_)=>{},Kind::Enum(_)|Kind::Sum(_)|Kind::SumPayload(_)=>{},_=>{}}}
pub open spec fn total_fields(definitions:Seq<Definition>,count:nat)->nat
    recommends count<=definitions.len(),
    decreases count,
{if count==0{0}else{total_fields(definitions,(count-1) as nat)+field_count(definitions[count as int-1].kind)}}
pub open spec fn total_variants(definitions:Seq<Definition>,count:nat)->nat
    recommends count<=definitions.len(),
    decreases count,
{if count==0{0}else{total_variants(definitions,(count-1) as nat)+variant_count(definitions[count as int-1].kind)}}
pub open spec fn definitions_prefix(definitions:Seq<Definition>,limits:Limits,count:nat)->bool
    recommends count<=definitions.len(),
    decreases count,
{
    if count==0{true}else{definitions_prefix(definitions,limits,(count-1) as nat)
        &&name_valid(definitions[count as int-1].name@)
        &&kind_valid(definitions[count as int-1].kind,definitions,limits)
        &&(count==1||definitions[count as int-2].id<definitions[count as int-1].id)
        &&(forall|j:int|0<=j<count-1 ==>#[trigger] definitions[j].name@!=definitions[count as int-1].name@)
        &&total_fields(definitions,count)<=u32::MAX
        &&total_variants(definitions,count)<=u32::MAX}
}
pub proof fn definitions_failure(definitions:Seq<Definition>,limits:Limits,failed:nat,count:nat)
    requires failed<=count<=definitions.len(),!definitions_prefix(definitions,limits,failed),
    ensures !definitions_prefix(definitions,limits,count),
    decreases count-failed,
{if count>failed{definitions_failure(definitions,limits,failed,(count-1) as nat);}}
pub open spec fn definitions_valid(definitions:Seq<Definition>,limits:Limits)->bool{
    0<definitions.len()<=limits.types&&definitions_prefix(definitions,limits,definitions.len())
}
pub open spec fn field_references(fields:Seq<Field>)->Seq<u32>{fields.map(|_:int,f:Field|f.type_id)}
pub open spec fn sum_references(variants:Seq<SumVariant>,count:nat)->Seq<u32>
    recommends count<=variants.len(),decreases count,
{if count==0{Seq::empty()}else{let prior=sum_references(variants,(count-1) as nat);match variants[count as int-1].payload{Some(id)=>prior.push(id),None=>prior}}}
pub open spec fn references(kind:Kind)->Seq<u32>{match kind{
    Kind::Tuple(items)=>items@,Kind::Record(fields)=>field_references(fields@),
    Kind::SumPayload(variants)=>sum_references(variants@,variants@.len()),
    Kind::Vector{element,..}=>seq![element],Kind::Map{key,value,..}=>seq![key,value],_=>Seq::empty(),
}}
// The native construction algorithm resolves each edge once, then performs at
// most one synchronous pass per definition. No path is expanded recursively.
pub open spec fn edge_indices(definitions:Seq<Definition>,ids:Seq<u32>,count:nat)->Option<Seq<usize>>
    recommends count<=ids.len(),decreases count,
{if count==0{Some(Seq::empty())}else{match edge_indices(definitions,ids,(count-1) as nat){
    None=>None,Some(prior)=>match find(definitions,ids[count as int-1],definitions.len()){
        Some(index)=>if index<definitions.len(){Some(prior.push(index))}else{None},None=>None,
    },
}}}
pub proof fn edges_failure(definitions:Seq<Definition>,ids:Seq<u32>,failed:nat,count:nat)
    requires failed<=count<=ids.len(),edge_indices(definitions,ids,failed)==None::<Seq<usize>>,
    ensures edge_indices(definitions,ids,count)==None::<Seq<usize>>,decreases count-failed,
{if count>failed{edges_failure(definitions,ids,failed,(count-1) as nat);}}
pub open spec fn graph_prefix(definitions:Seq<Definition>,count:nat)->Option<Seq<Seq<usize>>>
    recommends count<=definitions.len(),decreases count,
{if count==0{Some(Seq::empty())}else{match graph_prefix(definitions,(count-1) as nat){
    None=>None,Some(prior)=>{
        let ids=references(definitions[count as int-1].kind);
        match edge_indices(definitions,ids,ids.len()){None=>None,Some(row)=>Some(prior.push(row))}
    },
}}}
pub proof fn graph_failure(definitions:Seq<Definition>,failed:nat,count:nat)
    requires failed<=count<=definitions.len(),graph_prefix(definitions,failed)==None::<Seq<Seq<usize>>>,
    ensures graph_prefix(definitions,count)==None::<Seq<Seq<usize>>>,decreases count-failed,
{if count>failed{graph_failure(definitions,failed,(count-1) as nat);}}
pub open spec fn graph_view(rows:Seq<Vec<usize>>)->Seq<Seq<usize>>{rows.map(|_:int,row:Vec<usize>|row@)}
pub open spec fn row_prefix(row:Seq<usize>,resolved:Seq<bool>,count:nat)->bool
    recommends count<=row.len(),decreases count,
{if count==0{true}else{row_prefix(row,resolved,(count-1) as nat)
    &&row[count as int-1]<resolved.len()&&resolved[row[count as int-1] as int]}}
pub proof fn row_failure(row:Seq<usize>,resolved:Seq<bool>,failed:nat,count:nat)
    requires failed<=count<=row.len(),!row_prefix(row,resolved,failed),
    ensures !row_prefix(row,resolved,count),decreases count-failed,
{if count>failed{row_failure(row,resolved,failed,(count-1) as nat);}}
pub open spec fn advance(graph:Seq<Seq<usize>>,resolved:Seq<bool>)->Seq<bool>{
    Seq::new(graph.len(),|i:int|i<resolved.len()&&(resolved[i]||row_prefix(graph[i],resolved,graph[i].len())))
}
pub open spec fn settled(graph:Seq<Seq<usize>>,rounds:nat)->Seq<bool>
    decreases rounds,
{if rounds==0{Seq::new(graph.len(),|_:int|false)}else{advance(graph,settled(graph,(rounds-1) as nat))}}
pub open spec fn all_prefix(resolved:Seq<bool>,count:nat)->bool
    recommends count<=resolved.len(),decreases count,
{if count==0{true}else{all_prefix(resolved,(count-1) as nat)&&resolved[count as int-1]}}
pub proof fn all_failure(resolved:Seq<bool>,failed:nat,count:nat)
    requires failed<=count<=resolved.len(),!all_prefix(resolved,failed),
    ensures !all_prefix(resolved,count),decreases count-failed,
{if count>failed{all_failure(resolved,failed,(count-1) as nat);}}
// A successful node has a finite height: its edges lead to nodes settled in an
// earlier round. Conversely every finite closed DAG has height at most its
// number of vertices. The pass count is therefore the complete finite-domain
// bound, rather than a cutoff on an otherwise recursive native search.
pub open spec fn closed(definitions:Seq<Definition>)->bool{
    match graph_prefix(definitions,definitions.len()){
        None=>false,Some(graph)=>all_prefix(settled(graph,definitions.len()),graph.len()),
    }
}
// Independent graph meaning: a vertex has height below r exactly when every
// dependency has height below r-1. Unknown vertices have no finite height.
pub open spec fn height(graph:Seq<Seq<usize>>,node:usize,rounds:nat)->bool
    decreases rounds,
{node<graph.len()&&rounds>0&&(forall|j:int|0<=j<graph[node as int].len() ==>
    height(graph,graph[node as int][j],(rounds-1) as nat))}
pub proof fn height_monotone(graph:Seq<Seq<usize>>,node:usize,low:nat,high:nat)
    requires low<=high,height(graph,node,low),ensures height(graph,node,high),
    decreases low,
{
    assert(low>0);
    assert forall|j:int|0<=j<graph[node as int].len() implies
        height(graph,graph[node as int][j],(high-1) as nat) by {
        height_monotone(graph,graph[node as int][j],(low-1) as nat,(high-1) as nat);
    }
}
pub proof fn row_all(row:Seq<usize>,resolved:Seq<bool>,count:nat)
    requires count<=row.len(),
    ensures row_prefix(row,resolved,count)==(forall|j:int|0<=j<count ==>
        row[j]<resolved.len()&&resolved[row[j] as int]),decreases count,
{if count>0{row_all(row,resolved,(count-1) as nat);}}
pub proof fn settled_height(graph:Seq<Seq<usize>>,rounds:nat)
    requires graph.len()<=usize::MAX,
    ensures settled(graph,rounds).len()==graph.len(),
        forall|node:int|0<=node<graph.len() ==>
            #[trigger] settled(graph,rounds)[node]==height(graph,node as usize,rounds),
    decreases rounds,
{
    if rounds>0 {
        settled_height(graph,(rounds-1) as nat);
        let prior=settled(graph,(rounds-1) as nat);
        assert forall|node:int|0<=node<graph.len() implies
            settled(graph,rounds)[node]==height(graph,node as usize,rounds) by {
            row_all(graph[node],prior,graph[node].len());
            assert forall|j:int|0<=j<graph[node].len() implies
                (graph[node][j]<prior.len()&&prior[graph[node][j] as int])
                    ==height(graph,graph[node][j],(rounds-1) as nat) by {}
            if prior[node] {height_monotone(graph,node as usize,(rounds-1) as nat,rounds);}
        }
    }
}
pub open spec fn cycle(graph:Seq<Seq<usize>>,path:Seq<usize>)->bool{
    path.len()>=2&&path[0]==path[path.len() as int-1]
        &&(forall|j:int|0<=j<path.len()-1 ==> path[j]<graph.len()
            &&graph[path[j] as int].contains(path[j+1]))
}
pub proof fn height_child(graph:Seq<Seq<usize>>,node:usize,child:usize,rounds:nat)
    requires height(graph,node,rounds),graph[node as int].contains(child),
    ensures height(graph,child,(rounds-1) as nat),
{
    let j=choose|j:int|0<=j<graph[node as int].len()&&graph[node as int][j]==child;
}
pub proof fn height_excludes_cycle(graph:Seq<Seq<usize>>,path:Seq<usize>,rounds:nat)
    requires path.len()>0,height(graph,path[0],rounds),ensures !cycle(graph,path),
{
    if cycle(graph,path) {cycle_excludes_height(graph,path,0,rounds);}
}
pub proof fn cycle_excludes_height(graph:Seq<Seq<usize>>,path:Seq<usize>,position:int,rounds:nat)
    requires cycle(graph,path),0<=position<path.len()-1,
    ensures !height(graph,path[position],rounds),decreases rounds,
{
    if rounds>0&&height(graph,path[position],rounds) {
        let next=if position+1<path.len()-1{position+1}else{0};
        if next==0 {assert(path[position+1]==path[0]);}
        height_child(graph,path[position],path[next],rounds);
        cycle_excludes_height(graph,path,next,(rounds-1) as nat);
    }
}
pub proof fn all_member(resolved:Seq<bool>,count:nat,index:int)
    requires count<=resolved.len(),0<=index<count,all_prefix(resolved,count),
    ensures resolved[index],decreases count,
{if index<count-1{all_member(resolved,(count-1) as nat,index);}}
pub proof fn closed_has_finite_height_and_no_cycles(definitions:Seq<Definition>,graph:Seq<Seq<usize>>)
    requires closed(definitions),graph_prefix(definitions,definitions.len())==Some(graph),graph.len()<=usize::MAX,
    ensures forall|node:int|0<=node<graph.len() ==> #[trigger] height(graph,node as usize,definitions.len()),
        forall|path:Seq<usize>|!cycle(graph,path),
{
    settled_height(graph,definitions.len());
    assert forall|node:int|0<=node<graph.len() implies #[trigger] height(graph,node as usize,definitions.len()) by {
        all_member(settled(graph,definitions.len()),graph.len(),node);
    }
    assert forall|path:Seq<usize>|!cycle(graph,path) by {
        if cycle(graph,path){
            assert(graph[path[0] as int].contains(path[1]));
            assert(path[0]<graph.len());
            let first=path[0] as int;
            assert(height(graph,first as usize,definitions.len()));
            height_excludes_cycle(graph,path,definitions.len());
        }
    }
}
pub open spec fn metadata(description:Description,limits:Limits)->bool{
    name_valid(description.profile@)&&definitions_valid(description.definitions@,limits)
        &&find(description.definitions@,description.root,description.definitions@.len()).is_some()
        &&closed(description.definitions@)
}
pub open spec fn admit(bytes:Seq<u8>,description:Description,limits:Limits)->Result<(Seq<u8>,Description),Failure>{
    if bytes.len() as u64>limits.bytes{Err(Failure::Size)}
    else if !metadata(description,limits){Err(Failure::Metadata)}
    else if !super::super::spec::encoding_matches(bytes,description,limits.bytes){Err(Failure::Encoding)}
    else{Ok((bytes,description))}
}
}
