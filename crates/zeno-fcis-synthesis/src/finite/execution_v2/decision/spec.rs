//! Exact mathematical typed-candidate construction; erased in ordinary Rust.
use super::super::spec::charge;
use super::*;
use vstd::prelude::*;
verus! {
pub(in super::super) open spec fn bytes_equal(a: Seq<u8>,b: Seq<u8>) -> bool { a == b }
pub(in super::super) open spec fn equal(a: Atom,b: Atom) -> bool {
    match (a,b) {

        (Atom::Bool(a),Atom::Bool(b)) => a == b,
        (Atom::I128(a),Atom::I128(b)) => a == b,
        (Atom::U128(a),Atom::U128(b)) => a == b,
        (Atom::Enum{type_id:a,variant:x},Atom::Enum{type_id:b,variant:y}) => a == b && x == y,
        (Atom::Sum{type_id:a,variant:x},Atom::Sum{type_id:b,variant:y}) => a == b && x == y,
        (Atom::Bytes(a),Atom::Bytes(b)) | (Atom::Text(a),Atom::Text(b)) => a@ == b@,
        _ => false,
    }
}
pub(in super::super) open spec fn ordered(fields: Seq<Field>) -> bool {
    forall|j:int| 0 < j < fields.len() ==> fields[j-1].id < #[trigger] fields[j].id
}
pub(in super::super) open spec fn lookup<'a>(fields: Seq<Field<'a>>,id:u16,n:nat) -> Option<Atom<'a>>
    recommends n <= fields.len(), decreases n,
{
    if n == 0 { None } else {
        match lookup(fields,id,(n-1) as nat) {
            Some(v) => Some(v),
            None => if fields[n as int-1].id == id { Some(fields[n as int-1].value) } else { None },
        }
    }
}
pub(in super::super) proof fn lookup_found(fields:Seq<Field>,id:u16,n:nat,total:nat)
    requires n <= total <= fields.len(), lookup(fields,id,n).is_some(),
    ensures lookup(fields,id,total) == lookup(fields,id,n), decreases total-n,
{ if total > n { lookup_found(fields,id,n,(total-1) as nat); } }
pub(in super::super) open spec fn ordered_value(value:RootView) -> bool {
    match value { RootView::Leaf(_) => true, RootView::Record(fields) => ordered(fields@),}
}
pub(in super::super) open spec fn lookup_value<'a>(value:RootView<'_, 'a>,id:u16) -> Option<Atom<'a>> {
    match value { RootView::Leaf(_) => None, RootView::Record(fields) => lookup(fields@,id,fields@.len()),}
}
pub(in super::super) open spec fn root_value<'a>(value:RootView<'_, 'a>) -> Option<Atom<'a>> {
    match value { RootView::Leaf(atom) => Some(atom), RootView::Record(_) => None,}
}
pub(in super::super) open spec fn resolve<'a>(inputs:Inputs<'_, 'a>,output:Seq<Atom<'a>>,expr:Expr<'a>) -> Option<Atom<'a>> {
    match expr {
        Expr::Input(source,id) => match source {
            Source::State => lookup(inputs.state@,id,inputs.state@.len()),
            Source::Command => lookup_value(inputs.command,id),
            Source::Context => lookup_value(inputs.context,id),
        },
        Expr::Root(source) => match source {
            Source::State => None,
            Source::Command => root_value(inputs.command),
            Source::Context => root_value(inputs.context),
        },
        Expr::Output(i) => if i < output.len() { Some(output[i as int]) } else { None },
        Expr::Constant(v) => Some(v),
    }
}
pub(in super::super) open spec fn admitted(domain:Domain,value:Atom) -> bool {
    match (domain,value) {
        (Domain::Bool,Atom::Bool(_)) | (Domain::Bytes,Atom::Bytes(_)) | (Domain::Text,Atom::Text(_)) => true,
        (Domain::I128{min,max},Atom::I128(v)) => min <= v <= max,
        (Domain::U128{min,max},Atom::U128(v)) => min <= v <= max,
        (Domain::Enum{type_id,variants},Atom::Enum{type_id:t,variant}) |
        (Domain::Sum{type_id,variants},Atom::Sum{type_id:t,variant}) => type_id == t && variants@.contains(variant),
        _ => false,
    }
}
pub(in super::super) open spec fn branch_order(branches:Seq<Branch>,n:nat) -> bool
    recommends n <= branches.len(), decreases n,
{
    n <= 1 || (branch_order(branches,(n-1) as nat) && branches[n as int-2].code < branches[n as int-1].code)
}
#[verifier::opaque]
pub(in super::super) open spec fn select_branch(branches:Seq<Branch>,code:i128,n:nat) -> Option<usize>
    recommends n <= branches.len(), decreases n,
{
    if !branch_order(branches,n) { None }
    else if n == 0 { None }
    else if branches[n as int-1].code == code { Some((n-1) as usize) }
    else { select_branch(branches,code,(n-1) as nat) }
}
pub(in super::super) proof fn choose_invalid(branches:Seq<Branch>,code:i128,n:nat,total:nat)
    requires n <= total <= branches.len(), !branch_order(branches,n),
    ensures !branch_order(branches,total), select_branch(branches,code,total) == None::<usize>, decreases total-n,
{ reveal(select_branch); if total > n { choose_invalid(branches,code,n,(total-1) as nat); } }
pub(in super::super) open spec fn reason_valid(class:Class,reason:Option<u32>) -> bool {
    match (class,reason) { (Class::Accept,None) => true,
        (Class::Reject,Some(r)) | (Class::CommittedFailure,Some(r)) => r != 0,
        _ => false }
}
pub(in super::super) open spec fn payload<'a>(inputs:Inputs<'_, 'a>,output:Seq<Atom<'a>>,fields:Seq<PayloadField<'a>>,n:nat) -> Result<Seq<Field<'a>>,Failure>
    recommends n <= fields.len(), decreases n,
{
    if n == 0 { Ok(Seq::empty()) } else {
        match payload(inputs,output,fields,(n-1) as nat) {
            Err(e) => Err(e), Ok(prior) => {
                let entry = fields[n as int-1];
                if n > 1 && fields[n as int-2].field >= entry.field { Err(Failure::Delivery) }
                else { match resolve(inputs,output,entry.value) {
                    None => Err(Failure::Reference),Some(value) => Ok(prior.push(Field{id:entry.field,value})),
                } }
            }
        }
    }
}
pub(in super::super) proof fn payload_failure(inputs:Inputs,output:Seq<Atom>,fields:Seq<PayloadField>,n:nat,total:nat)
    requires n <= total <= fields.len(), payload(inputs,output,fields,n).is_err(),
    ensures payload(inputs,output,fields,total) == payload(inputs,output,fields,n), decreases total-n,
{ if total > n { payload_failure(inputs,output,fields,n,(total-1) as nat); } }

pub type Writes<'a> = (Result<(Seq<Field<'a>>,Seq<Patch<'a>>),Failure>,Seq<u64>,Seq<Attempt>);
#[verifier::opaque]
pub(in super::super) open spec fn assignments<'a>(inputs:Inputs<'_, 'a>,output:Seq<Atom<'a>>,plan:Seq<Assignment<'a>>,n:nat,limits:Seq<u64>,used:Seq<u64>) -> Writes<'a>
    recommends n <= plan.len(), limits.len() == 8, used.len() == 8, decreases n,
{
    if plan.len() != inputs.state@.len() { (Err(Failure::Successor),used,Seq::empty()) }
    else if n == 0 { (Ok((Seq::empty(),Seq::empty())),used,Seq::empty()) }
    else {
        let prior = assignments(inputs,output,plan,(n-1) as nat,limits,used);
        match prior.0 {
            Err(_) => prior, Ok((post,patch)) => {
                let entry = plan[n as int-1]; let pre = inputs.state@[n as int-1];
                if entry.field != pre.id { (Err(Failure::Successor),prior.1,prior.2) }
                else {
                    let c = charge(limits,prior.1,Resource::Write,1);
                    let attempts = prior.2.push(Attempt::Write(entry.field,c.0.is_ok()));
                    match c.0 {
                        Err(e) => (Err(Failure::Budget(e)),c.1,attempts),
                        Ok(()) => match resolve(inputs,output,entry.value) {
                            None => (Err(Failure::Reference),c.1,attempts), Some(value) => {
                                if !admitted(entry.domain,pre.value) || !admitted(entry.domain,value) {
                                    (Err(Failure::Domain),c.1,attempts)
                                } else {
                                    (Ok((post.push(Field{id:entry.field,value}),
                                        if equal(pre.value,value) { patch } else { patch.push(Patch{field:entry.field,before:pre.value,after:value}) })),c.1,attempts)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
pub(in super::super) proof fn assignments_failure(inputs:Inputs,output:Seq<Atom>,plan:Seq<Assignment>,n:nat,total:nat,limits:Seq<u64>,used:Seq<u64>)
    requires n <= total <= plan.len(), assignments(inputs,output,plan,n,limits,used).0.is_err(),
    ensures assignments(inputs,output,plan,total,limits,used) == assignments(inputs,output,plan,n,limits,used), decreases total-n,
{ reveal_with_fuel(assignments,2); if total > n { assignments_failure(inputs,output,plan,n,(total-1) as nat,limits,used); } }

pub type DeliveryView<'a> = (u32,u32,Atom<'a>,Seq<Field<'a>>,Atom<'a>);
pub(in super::super) open spec fn delivery_view<'a>(d:Delivery<'a>) -> DeliveryView<'a> { (d.ordinal,d.channel,d.destination,d.payload@,d.idempotency) }
pub type Deliveries<'a> = (Result<Seq<DeliveryView<'a>>,Failure>,Seq<u64>,Seq<Attempt>);
#[verifier::opaque]
pub(in super::super) open spec fn deliveries<'a>(inputs:Inputs<'_, 'a>,output:Seq<Atom<'a>>,plan:Seq<DeliveryPlan<'a>>,outbox:bool,n:nat,limits:Seq<u64>,used:Seq<u64>) -> Deliveries<'a>
    recommends n <= plan.len(), limits.len() == 8, used.len() == 8, decreases n,
{
    if n == 0 { (Ok(Seq::empty()),used,Seq::empty()) } else {
        let prior = deliveries(inputs,output,plan,outbox,(n-1) as nat,limits,used);
        match prior.0 {
            Err(_) => prior, Ok(entries) => {
                let e = plan[n as int-1];
                if e.channel == 0 || (n > 1 && plan[n as int-2].ordinal >= e.ordinal) { (Err(Failure::Delivery),prior.1,prior.2) }
                else {
                    match resolve(inputs,output,e.when) {
                        Some(Atom::Bool(false)) => prior,
                        Some(Atom::Bool(true)) => {
                            let c = charge(limits,prior.1,Resource::Effect,1);
                            let attempts = prior.2.push(Attempt::Effect(outbox,e.ordinal,c.0.is_ok()));
                            match c.0 {
                                Err(error) => (Err(Failure::Budget(error)),c.1,attempts),
                                Ok(()) => match resolve(inputs,output,e.destination) {
                                    None => (Err(Failure::Reference),c.1,attempts), Some(destination) => match resolve(inputs,output,e.idempotency) {
                                        None => (Err(Failure::Reference),c.1,attempts), Some(idempotency) => match payload(inputs,output,e.payload@,e.payload@.len()) {
                                            Err(error) => (Err(error),c.1,attempts),
                                            Ok(fields) => (Ok(entries.push((e.ordinal,e.channel,destination,fields,idempotency))),c.1,attempts),
                                        },
                                    },
                                },
                            }
                        },
                        _ => (Err(Failure::Domain),prior.1,prior.2),
                    }
                }
            }
        }
    }
}
pub(in super::super) proof fn deliveries_failure(inputs:Inputs,output:Seq<Atom>,plan:Seq<DeliveryPlan>,outbox:bool,n:nat,total:nat,limits:Seq<u64>,used:Seq<u64>)
    requires n <= total <= plan.len(), deliveries(inputs,output,plan,outbox,n,limits,used).0.is_err(),
    ensures deliveries(inputs,output,plan,outbox,total,limits,used) == deliveries(inputs,output,plan,outbox,n,limits,used), decreases total-n,
{ reveal_with_fuel(deliveries,2); if total > n { deliveries_failure(inputs,output,plan,outbox,n,(total-1) as nat,limits,used); } }

pub type CandidateView<'a> = (Class,Option<u32>,Seq<Field<'a>>,Seq<Field<'a>>,Seq<Patch<'a>>,Seq<DeliveryView<'a>>,Seq<DeliveryView<'a>>);
pub(in super::super) open spec fn candidate_view<'a>(c:Candidate<'a>) -> CandidateView<'a> {
    (c.view().0,c.view().1,c.view().2,c.view().3,c.view().4,c.view().5.map(|_:int,d:Delivery| delivery_view(d)),c.view().6.map(|_:int,d:Delivery| delivery_view(d)))
}
pub(in super::super) open spec fn construct<'a>(inputs:Inputs<'_, 'a>,output:Seq<Atom<'a>>,code:i128,branches:Seq<Branch<'a>>,limits:Seq<u64>,used:Seq<u64>) -> (Result<CandidateView<'a>,Failure>,Seq<u64>,Seq<Attempt>) {
    if !ordered(inputs.state@) || !ordered_value(inputs.command) || !ordered_value(inputs.context) { (Err(Failure::InputOrder),used,Seq::empty()) }
    else {
        let c = charge(limits,used,Resource::Candidate,1);
        let attempts = seq![Attempt::Candidate(c.0.is_ok())];
        match c.0 {
            Err(e) => (Err(Failure::Budget(e)),c.1,attempts),
            Ok(()) => match select_branch(branches,code,branches.len()) {
                None => (Err(Failure::Branch),c.1,attempts), Some(index) => {
                    let b = branches[index as int];
                    if !reason_valid(b.class,b.reason) { (Err(Failure::Reason),c.1,attempts) }
                    else if b.class == Class::Reject {
                        if b.assignments@.len() != 0 || b.effects@.len() != 0 || b.outbox@.len() != 0 { (Err(Failure::RejectPlan),c.1,attempts) }
                        else { (Ok((b.class,b.reason,inputs.state@,Seq::empty(),Seq::empty(),Seq::empty(),Seq::empty())),c.1,attempts) }
                    } else {
                        let w = assignments(inputs,output,b.assignments@,b.assignments@.len(),limits,c.1);
                        match w.0 {
                            Err(e) => (Err(e),w.1,attempts+w.2), Ok((post,patch)) => {
                                let e = deliveries(inputs,output,b.effects@,false,b.effects@.len(),limits,w.1);
                                match e.0 {
                                    Err(error) => (Err(error),e.1,attempts+w.2+e.2),Ok(effects) => {
                                        let o = deliveries(inputs,output,b.outbox@,true,b.outbox@.len(),limits,e.1);
                                        match o.0 {
                                            Err(error) => (Err(error),o.1,attempts+w.2+e.2+o.2),
                                            Ok(outbox) => (Ok((b.class,b.reason,inputs.state@,post,patch,effects,outbox)),o.1,attempts+w.2+e.2+o.2),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
}
