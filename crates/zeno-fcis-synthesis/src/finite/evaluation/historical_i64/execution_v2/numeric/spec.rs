//! Mathematical tagged eager prefix and exact old-meter correspondence.
use super::super::super::evaluation::spec as scalar;
use super::super::super::evaluation::typed::spec as node;
use super::super::spec as old;
use super::*;
use vstd::prelude::*;
verus! {
pub(crate) open spec fn nodes(p:ProgramView)->Seq<Instruction> {
    match p {
        ProgramView::Legacy{nodes,..}=>Seq::new(nodes@.len(),|i:int|node::legacy_instruction(nodes@[i])),
        ProgramView::Typed(p)=>Seq::new(p.nodes@.len(),|i:int|node::typed_instruction(p.nodes@[i])),
    }
}
pub(crate) open spec fn roots<'a>(p:ProgramView<'a>)->Seq<u16> {
    match p {ProgramView::Legacy{roots,..}=>roots@,ProgramView::Typed(p)=>p.roots@}
}
pub(crate) open spec fn inputs<'a>(p:ProgramView<'a>)->Domains<'a> {
    match p {ProgramView::Legacy{inputs,..}=>Domains::Legacy(inputs),ProgramView::Typed(p)=>Domains::Typed(p.inputs)}
}
pub(crate) open spec fn outputs<'a>(p:ProgramView<'a>)->Domains<'a> {
    match p {ProgramView::Legacy{outputs,..}=>Domains::Legacy(outputs),ProgramView::Typed(p)=>Domains::Typed(p.outputs)}
}
pub(crate) open spec fn instruction(p:ProgramView,i:nat)->Option<Instruction> {
    if i<nodes(p).len(){Some(nodes(p)[i as int])}else{None}
}
pub(crate) open spec fn domain_count(d:Domains)->nat {
    match d {Domains::Legacy(d)=>d@.len(),Domains::Typed(d)=>d@.len()}
}
pub(crate) open spec fn contains(d:Domains,i:int,v:Atom)->bool {
    match d {
        Domains::Legacy(d)=>0<=i<d@.len() && match node::project(v) {Some(n)=>scalar::contains(d@[i],n),None=>false},
        Domains::Typed(d)=>0<=i<d@.len() && match d@[i] {
            Domain::Bool|Domain::I128{..}|Domain::U128{..}=>value::spec::admitted(d@[i],v),_=>false,
        },
    }
}
pub(crate) open spec fn admitted(d:Domains,v:Seq<Atom>)->bool {
    domain_count(d)==v.len() && forall|i:int|0<=i<v.len() ==> contains(d,i,v[i])
}
pub(crate) open spec fn node_failure(e:typed::Failure)->Failure {
    match e {typed::Failure::Reference=>Failure::Reference,typed::Failure::Kind=>Failure::Kind,typed::Failure::Arithmetic=>Failure::Arithmetic}
}
pub(in super::super) open spec fn prefix<'a>(ops:Seq<Instruction>,input:Seq<Atom<'a>>,count:nat,limits:Seq<u64>,used:Seq<u64>)
 ->(Result<Seq<Atom<'a>>,Failure>,Seq<u64>)
 recommends count<=ops.len(),limits.len()==8,used.len()==8,
 decreases count,
{
    if count==0 {(Ok(Seq::empty()),used)} else {
        let previous=prefix(ops,input,(count-1) as nat,limits,used);
        match previous.0 {
            Err(_)=>previous,
            Ok(values)=>{
                let charge=old::charge(limits,previous.1,Resource::Step,1);
                match charge.0 {
                    Err(e)=>(Err(Failure::Budget(e)),charge.1),
                    Ok(())=>match node::node(ops[count as int-1],input,values) {
                        Ok(v)=>(Ok(values.push(v)),charge.1),Err(e)=>(Err(node_failure(e)),charge.1),
                    },
                }
            },
        }
    }
}
pub(in super::super) proof fn failure_persists(ops:Seq<Instruction>,input:Seq<Atom>,failed:nat,count:nat,limits:Seq<u64>,used:Seq<u64>,error:Failure)
 requires failed<=count<=ops.len(),prefix(ops,input,failed,limits,used).0==Err(error),
 ensures prefix(ops,input,count,limits,used)==prefix(ops,input,failed,limits,used),
 decreases count-failed,
{if count>failed {failure_persists(ops,input,failed,(count-1) as nat,limits,used,error);}}
pub(crate) open spec fn projection<'a>(values:Seq<Atom<'a>>,roots:Seq<u16>,count:nat)->Result<Seq<Atom<'a>>,Failure>
 recommends count<=roots.len(),decreases count,
{
    if count==0 {Ok(Seq::empty())}else {
        match projection(values,roots,(count-1) as nat) {
            Err(e)=>Err(e),Ok(out)=>match node::at(values,roots[count as int-1] as nat) {
                Some(v)=>Ok(out.push(v)),None=>Err(Failure::Reference),
            },
        }
    }
}
pub(crate) proof fn projection_failure_persists(values:Seq<Atom>,roots:Seq<u16>,failed:nat,count:nat)
 requires failed<=count<=roots.len(),projection(values,roots,failed)==Err(Failure::Reference),
 ensures projection(values,roots,count)==Err(Failure::Reference),decreases count-failed,
{if count>failed{projection_failure_persists(values,roots,failed,(count-1) as nat);}}
pub(in super::super) open spec fn execution<'a>(p:ProgramView,input:Seq<Atom<'a>>,limits:Seq<u64>,used:Seq<u64>)
 ->(Result<Seq<Atom<'a>>,Failure>,Seq<u64>) {
    if !admitted(inputs(p),input){(Err(Failure::InputDomain),used)}else {
        let result=prefix(nodes(p),input,nodes(p).len(),limits,used);
        match result.0 {
            Err(_)=>result,Ok(values)=>match projection(values,roots(p),roots(p).len()) {
                Err(e)=>(Err(e),result.1),Ok(out)=>if admitted(outputs(p),out){(Ok(out),result.1)}else{(Err(Failure::OutputDomain),result.1)},
            },
        }
    }
}
pub(crate) open spec fn legacy_failure(e:Failure)->super::super::Failure {
    match e {
        Failure::InputDomain=>super::super::Failure::InputDomain,
        Failure::Reference=>super::super::Failure::Reference,
        Failure::Kind|Failure::Arithmetic=>super::super::Failure::Arithmetic,
        Failure::OutputDomain=>super::super::Failure::OutputDomain,
        Failure::Budget(e)=>super::super::Failure::Budget(e),
    }
}
pub(crate) open spec fn project_prefix(values:Seq<Atom>,count:nat)->Option<Seq<i64>>
 recommends count<=values.len(), decreases count,
{
    if count==0 {Some(Seq::empty())}else{
        match (project_prefix(values,(count-1) as nat),node::project(values[count as int-1])) {
            (Some(prefix),Some(n))=>Some(prefix.push(n)),_=>None,
        }
    }
}
pub(crate) open spec fn project_all(values:Seq<Atom>)->Option<Seq<i64>> {
    project_prefix(values,values.len())
}
pub(crate) proof fn project_failure_persists(values:Seq<Atom>,failed:nat,count:nat)
 requires failed<=count<=values.len(),project_prefix(values,failed).is_none(),
 ensures project_prefix(values,count).is_none(),decreases count-failed,
{if count>failed{project_failure_persists(values,failed,(count-1) as nat);}}
pub(crate) proof fn lift_prefix_projects(values:Seq<i64>,count:nat)
 requires count<=values.len(),
 ensures project_prefix(node::lift(values),count)==Some(values.take(count as int)),
 decreases count,
{if count>0{lift_prefix_projects(values,(count-1) as nat);}}
pub(crate) open spec fn projected(result:(Result<Seq<Atom>,Failure>,Seq<u64>))
 ->(Result<Seq<i64>,super::super::Failure>,Seq<u64>) {
    (match result.0 {Err(e)=>Err(legacy_failure(e)),Ok(values)=>match project_all(values){Some(v)=>Ok(v),None=>Err(super::super::Failure::Arithmetic)}},result.1)
}
pub(crate) proof fn lift_projects(values:Seq<i64>)
 ensures project_all(node::lift(values))==Some(values),
{lift_prefix_projects(values,values.len());}
/// Successful narrowing is a lossless inverse, never a Boolean/unsigned coercion.
pub(crate) proof fn project_prefix_inverse(values:Seq<Atom>,count:nat,codes:Seq<i64>)
 requires count<=values.len(),project_prefix(values,count)==Some(codes),
 ensures values.take(count as int)==node::lift(codes),decreases count,
{
    reveal_with_fuel(project_prefix,2);
    if count>0 {
        let prior=choose|prior:Seq<i64>|project_prefix(values,(count-1) as nat)==Some(prior);
        project_prefix_inverse(values,(count-1) as nat,prior);
        vstd::seq_lib::lemma_seq_take_len(values,(count-1) as int);
        let code=choose|code:i64|node::project(values[count as int-1])==Some(code);
        assert(codes==prior.push(code));
        assert(values[count as int-1]==Atom::I128(code as i128));
        assert(prior.len()==count-1);
        assert(codes.len()==count);
        assert forall|i:int|0<=i<count implies values[i]==node::lift(codes)[i] by {
            if i<count-1 {
                assert(values.take((count-1) as int)[i]==node::lift(prior)[i]);
            }else{assert(i==count-1);}
        }
        assert(values.take(count as int) =~= node::lift(codes));
    }else{assert(values.take(0) =~= node::lift(codes));}
}
pub(crate) proof fn project_inverse(values:Seq<Atom>,codes:Seq<i64>)
 requires project_all(values)==Some(codes),
 ensures values==node::lift(codes),
{project_prefix_inverse(values,values.len(),codes);}
pub(in super::super) proof fn legacy_prefix_embedding(ops:Seq<Op>,input:Seq<i64>,count:nat,limits:Seq<u64>,used:Seq<u64>)
 requires count<=ops.len(),limits.len()==8,used.len()==8,
 ensures prefix(Seq::new(ops.len(),|i:int|node::legacy_instruction(ops[i])),node::lift(input),count,limits,used)
    ==match old::prefix(ops,input,count,limits,used) {
        (Ok(v),usage)=>(Ok(node::lift(v)),usage),
        (Err(e),usage)=>(Err(match e {
            super::super::Failure::InputDomain=>Failure::InputDomain,super::super::Failure::Reference=>Failure::Reference,
            super::super::Failure::Arithmetic=>Failure::Arithmetic,super::super::Failure::OutputDomain=>Failure::OutputDomain,
            super::super::Failure::Budget(e)=>Failure::Budget(e),
        }),usage),
    },decreases count,
{
    reveal_with_fuel(prefix,2);
    reveal_with_fuel(old::prefix,2);
    assert(node::lift(Seq::<i64>::empty()) =~= Seq::<Atom>::empty());
    if count>0 {
        legacy_prefix_embedding(ops,input,(count-1) as nat,limits,used);
        if let (Ok(previous),_) = old::prefix(ops,input,(count-1) as nat,limits,used) {
            node::legacy_node_embedding(ops[count as int-1],input,previous);
            let actual=node::node(node::legacy_instruction(ops[count as int-1]),node::lift(input),node::lift(previous));
            let expected=scalar::node(ops[count as int-1],input,previous);
            match actual {
                Ok(value)=>{
                    match value {
                        Atom::I128(n)=>{
                            assert(i64::MIN<=n<=i64::MAX);
                            assert(expected==Ok(n as i64));
                            assert(Atom::I128((n as i64) as i128)==value);
                        },
                        _=>{assert(false);},
                    }
                },
                Err(typed::Failure::Reference)=>{
                    assert(expected==Err(super::super::super::evaluation::Failure::Reference));
                },
                Err(typed::Failure::Arithmetic)=>{
                    assert(expected==Err(super::super::super::evaluation::Failure::Arithmetic));
                },
                Err(typed::Failure::Kind)=>{assert(false);},
            }
            if let Ok(v)=scalar::node(ops[count as int-1],input,previous) {
                assert(node::lift(previous.push(v)) =~= node::lift(previous).push(Atom::I128(v as i128)));
            }
        }
    }
}
pub(crate) proof fn legacy_projection_embedding(values:Seq<i64>,roots:Seq<u16>,count:nat)
 requires count<=roots.len(),
 ensures projection(node::lift(values),roots,count)==match scalar::projection(values,roots,count) {
    Ok(out)=>Ok(node::lift(out)),Err(_)=>Err(Failure::Reference),
 },decreases count,
{
    reveal_with_fuel(projection,2);
    reveal_with_fuel(scalar::projection,2);
    assert(node::lift(Seq::<i64>::empty()) =~= Seq::<Atom>::empty());
    if count>0{
        legacy_projection_embedding(values,roots,(count-1) as nat);
        if let Ok(previous)=scalar::projection(values,roots,(count-1) as nat) {
            if roots[count as int-1]<values.len() {
                let v=values[roots[count as int-1] as int];
                assert(node::lift(previous.push(v)) =~= node::lift(previous).push(Atom::I128(v as i128)));
            }
        }
    }
}
// Exact membership lift, including malformed old domains and tuple lengths.
pub(in super::super) proof fn legacy_admission_embedding(domains:&[LegacyDomain],values:Seq<i64>)
 ensures admitted(Domains::Legacy(domains),node::lift(values))==scalar::admitted(domains@,values),
{
    if domains@.len()==values.len() {
        assert forall|i:int|0<=i<values.len() implies
            contains(Domains::Legacy(domains),i,node::lift(values)[i])
                ==scalar::contains(domains@[i],values[i]) by {
            assert(node::project(node::lift(values)[i])==Some(values[i]));
        }
        if scalar::admitted(domains@,values) {
            assert forall|i:int|0<=i<node::lift(values).len() implies
                contains(Domains::Legacy(domains),i,node::lift(values)[i]) by {
                assert(scalar::contains(domains@[i],values[i]));
            }
        }
        if admitted(Domains::Legacy(domains),node::lift(values)) {
            assert forall|i:int|0<=i<domains@.len() implies
                scalar::contains(domains@[i],values[i]) by {
                assert(contains(Domains::Legacy(domains),i,node::lift(values)[i]));
            }
        }
    }
}

pub(in super::super) proof fn legacy_projection_error(values:Seq<i64>,roots:Seq<u16>,count:nat)
 requires count<=roots.len(),
 ensures match scalar::projection(values,roots,count) {
     Err(e)=>e==super::super::super::evaluation::Failure::Reference,Ok(_)=>true,
 },decreases count,
{
    if count>0 {legacy_projection_error(values,roots,(count-1) as nat);}
}

pub(in super::super) proof fn legacy_execution_embedding(p:ProgramView,input:Seq<i64>,limits:Seq<u64>,used:Seq<u64>)
 requires matches!(p,ProgramView::Legacy{..}),limits.len()==8,used.len()==8,
 ensures projected(execution(p,node::lift(input),limits,used))==match p {
    ProgramView::Legacy{inputs,outputs,nodes,roots}=>old::execution(inputs@,outputs@,nodes@,roots@,input,limits,used),
    _=>(Err(super::super::Failure::InputDomain),used),
 },
 match execution(p,node::lift(input),limits,used).0 {Ok(v)=>project_all(v).is_some(),Err(_)=>true},
 match prefix(nodes(p),node::lift(input),nodes(p).len(),limits,used).0 {Ok(v)=>project_all(v).is_some(),Err(_)=>true},
{
    reveal(execution);
    reveal(old::execution);
    if let ProgramView::Legacy{inputs,outputs,nodes,roots}=p {
        legacy_admission_embedding(inputs,input);
        legacy_prefix_embedding(nodes@,input,nodes@.len(),limits,used);
        assert(admitted(Domains::Legacy(inputs),node::lift(input))==scalar::admitted(inputs@,input));
        if let (Ok(values),_) = old::prefix(nodes@,input,nodes@.len(),limits,used) {
            lift_projects(values);
            legacy_projection_embedding(values,roots@,roots@.len());
            legacy_projection_error(values,roots@,roots@.len());
            if let Ok(out)=scalar::projection(values,roots@,roots@.len()) {
                lift_projects(out);
                legacy_admission_embedding(outputs,out);
                assert(admitted(Domains::Legacy(outputs),node::lift(out))==scalar::admitted(outputs@,out));
            }
        }
    }
}
}
