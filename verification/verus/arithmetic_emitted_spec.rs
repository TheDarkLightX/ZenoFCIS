//! Mathematical semantics of the checked eager Rust emission grammar.
//! Text-to-AST validation is performed by a separately pinned, reviewed gate.
use crate::arithmetic_typed_spec::abstract_node;
use crate::evaluation::{Domain, Failure, Op};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn contains(domain:Domain,value:i128)->bool {
    match domain {Domain::Bool=>0<=value<=1,
        Domain::Int{min,max}=>min<=value<=max}
}
pub(crate) open spec fn admitted(domains:Seq<Domain>,values:Seq<i128>)->bool {
    domains.len()==values.len() && forall|i:int| 0<=i<domains.len() ==>
        #[trigger] contains(domains[i],values[i])
}

pub(crate) open spec fn prefix(nodes:Seq<Op>,input:Seq<i128>,count:nat)->Result<Seq<i128>,Failure>
    recommends count<=nodes.len(), decreases count,
{
    if count==0{Ok(Seq::empty())}else{
        match prefix(nodes,input,(count-1) as nat){
            Err(e)=>Err(e),
            Ok(v)=>match abstract_node(nodes[count as int-1],input,v){
                Err(e)=>Err(e),Ok(n)=>Ok(v.push(n)),
            },
        }
    }
}

pub(crate) open spec fn project(values:Seq<i128>,roots:Seq<u16>,count:nat)->Result<Seq<i128>,Failure>
    recommends count<=roots.len(), decreases count,
{
    if count==0{Ok(Seq::empty())}else{
        match project(values,roots,(count-1) as nat){
            Err(e)=>Err(e),
            Ok(v)=>if roots[count as int-1]<values.len(){Ok(v.push(values[roots[count as int-1] as int]))}
                else{Err(Failure::Reference)},
        }
    }
}

pub(crate) open spec fn run(inputs:Seq<Domain>,outputs:Seq<Domain>,nodes:Seq<Op>,roots:Seq<u16>,input:Seq<i128>)->Result<Seq<i128>,Failure>{
    if !admitted(inputs,input){Err(Failure::InputDomain)}else{
        match prefix(nodes,input,nodes.len()){
            Err(e)=>Err(e),Ok(values)=>match project(values,roots,roots.len()){
                Err(e)=>Err(e),Ok(out)=>if admitted(outputs,out){Ok(out)}else{Err(Failure::OutputDomain)},
            },
        }
    }
}

pub(crate) proof fn admitted_correspondence(d:Seq<Domain>,v:Seq<i128>)
    ensures admitted(d,v)==crate::evaluation::spec::admitted(d,v),
{
    if admitted(d,v){
        assert forall|i:int| 0<=i<d.len() implies
            crate::evaluation::spec::contains(d[i],v[i]) by {
            assert(contains(d[i],v[i]));
        }
        assert(crate::evaluation::spec::admitted(d,v));
    }
    if crate::evaluation::spec::admitted(d,v){
        assert forall|i:int| 0<=i<d.len() implies
            contains(d[i],v[i]) by {
            assert(crate::evaluation::spec::contains(d[i],v[i]));
        }
        assert(admitted(d,v));
    }
}

pub(crate) proof fn prefix_correspondence(nodes:Seq<Op>,input:Seq<i128>,count:nat)
    requires count<=nodes.len(),
    ensures prefix(nodes,input,count)==crate::evaluation::spec::prefix(nodes,input,count),
    decreases count,
{
    if count>0{
        prefix_correspondence(nodes,input,(count-1) as nat);
        match prefix(nodes,input,(count-1) as nat){
            Ok(v)=>crate::arithmetic_typed_spec::legacy_node_correspondence(nodes[count as int-1],input,v),
            Err(_)=>{},
        }
    }
}

pub(crate) proof fn projection_correspondence(values:Seq<i128>,roots:Seq<u16>,count:nat)
    requires count<=roots.len(),
    ensures project(values,roots,count)==crate::evaluation::spec::projection(values,roots,count),
    decreases count,
{
    if count>0{projection_correspondence(values,roots,(count-1) as nat);}
}

/// All input vectors and even malformed graphs: both semantics refuse alike.
/// For admitted Program instances the reference-failure cases are unreachable.
pub(crate) proof fn emitted_program_correspondence(inputs:Seq<Domain>,outputs:Seq<Domain>,nodes:Seq<Op>,roots:Seq<u16>,input:Seq<i128>)
    ensures run(inputs,outputs,nodes,roots,input)==crate::evaluation::spec::execution(inputs,outputs,nodes,roots,input),
{
    admitted_correspondence(inputs,input);
    prefix_correspondence(nodes,input,nodes.len());
    match prefix(nodes,input,nodes.len()){
        Ok(v)=>{
            projection_correspondence(v,roots,roots.len());
            match project(v,roots,roots.len()){
                Ok(out)=>admitted_correspondence(outputs,out),Err(_)=>{},
            }
        },
        Err(_)=>{},
    }
}

pub(crate) proof fn unused_division_refuses()
    ensures run(Seq::empty(),seq![Domain::Int{min:i128::MIN,max:i128::MAX}],
        seq![Op::Int(1),Op::Int(0),Op::Bool(false),Op::DivFloor(0,1),Op::Int(0),Op::Select(2,3,4)],
        seq![5u16],Seq::empty())==Err(Failure::Arithmetic),
{
    reveal_with_fuel(prefix,7);
}

}
