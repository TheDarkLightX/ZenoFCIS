//! Exact policy metadata specifications.
use super::super::super::evaluation::{Domain as ScalarDomain, Op as ScalarOp};
use super::super::{
    InputField, InputLeaf, InputVariant, Limits, ScalarProgram,
    decision::{Assignment, Branch, Class, DeliveryPlan, Domain, Expr, PayloadField, Source},
    laws,
};
use super::canonical::Part;
use super::spec::atom as decision_atom;

use vstd::prelude::*;
verus! {

pub open spec fn source<'a>(value:Source)->Seq<Part<'a>> { match value {
Source::State=>seq![Part::Word(0)],
Source::Command=>seq![Part::Word(1)],
Source::Context=>seq![Part::Word(2)],
} }

pub open spec fn class<'a>(value:Class)->Seq<Part<'a>> { match value {
Class::Accept=>seq![Part::Word(0)],
Class::Reject=>seq![Part::Word(1)],
Class::CommittedFailure=>seq![Part::Word(2)],
} }

pub open spec fn reason<'a>(value:Option<u32>)->Seq<Part<'a>> { match value{None=>seq![Part::Word(0)],Some(v)=>seq![Part::Word(1),Part::Word(v as u128)]} }

pub open spec fn variants_prefix<'a>(values:Seq<InputVariant>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{variants_prefix(values,(n-1) as nat)+(seq![Part::Word(values[n-1].id as u128),Part::Word(values[n-1].code as i128 as u128)])}}
pub open spec fn variants<'a>(values:Seq<InputVariant>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+variants_prefix(values,values.len())}

pub open spec fn leaf<'a>(value:InputLeaf)->Seq<Part<'a>> { match value {
InputLeaf::I128{min,max}=>seq![Part::Word(0),Part::Word(min as i128 as u128),Part::Word(max as i128 as u128)],
InputLeaf::Bool=>seq![Part::Word(1)],
InputLeaf::U128{min,max}=>seq![Part::Word(4),Part::Word(min),Part::Word(max)],
InputLeaf::Enum{type_id,min,max,variants:vs}=>seq![Part::Word(2),Part::Word(type_id as u128),Part::Word(min as i128 as u128),Part::Word(max as i128 as u128)]+variants(vs@),
InputLeaf::Sum{type_id,min,max,variants:vs}=>seq![Part::Word(3),Part::Word(type_id as u128),Part::Word(min as i128 as u128),Part::Word(max as i128 as u128)]+variants(vs@),
} }

pub open spec fn leaves_prefix<'a>(values:Seq<InputLeaf>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{leaves_prefix(values,(n-1) as nat)+(leaf(values[n-1]))}}
#[verifier::opaque]
pub open spec fn leaves<'a>(values:Seq<InputLeaf>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+leaves_prefix(values,values.len())}

pub open spec fn input_fields_prefix<'a>(values:Seq<InputField>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{input_fields_prefix(values,(n-1) as nat)+(seq![Part::Word(values[n-1].id as u128)]+leaf(values[n-1].leaf))}}
pub open spec fn input_fields<'a>(values:Seq<InputField>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+input_fields_prefix(values,values.len())}

pub open spec fn ids_prefix<'a>(values:Seq<u32>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{ids_prefix(values,(n-1) as nat)+(seq![Part::Word(values[n-1] as u128)])}}
#[verifier::opaque]
pub open spec fn ids<'a>(values:Seq<u32>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+ids_prefix(values,values.len())}

pub open spec fn variants_ids_prefix<'a>(values:Seq<u16>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{variants_ids_prefix(values,(n-1) as nat)+(seq![Part::Word(values[n-1] as u128)])}}
pub open spec fn variants_ids<'a>(values:Seq<u16>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+variants_ids_prefix(values,values.len())}

pub open spec fn scalar_domain<'a>(value:ScalarDomain)->Seq<Part<'a>> { match value {
ScalarDomain::Bool=>seq![Part::Word(0)],
ScalarDomain::Int{min,max}=>seq![Part::Word(1),Part::Word(min as i128 as u128),Part::Word(max as i128 as u128)],
} }

pub open spec fn scalar_domains_prefix<'a>(values:Seq<ScalarDomain>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{scalar_domains_prefix(values,(n-1) as nat)+(scalar_domain(values[n-1]))}}
pub open spec fn scalar_domains<'a>(values:Seq<ScalarDomain>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+scalar_domains_prefix(values,values.len())}

pub open spec fn scalar_op<'a>(value:ScalarOp)->Seq<Part<'a>> { match value {
ScalarOp::Input(a)=>seq![Part::Word(0),Part::Word(a as u128)],
ScalarOp::Int(a)=>seq![Part::Word(1),Part::Word(a as i128 as u128)],
ScalarOp::Bool(a)=>seq![Part::Word(2),Part::Word(a as u128)],
ScalarOp::Add(a,b)=>seq![Part::Word(3),Part::Word(a as u128),Part::Word(b as u128)],
ScalarOp::Sub(a,b)=>seq![Part::Word(4),Part::Word(a as u128),Part::Word(b as u128)],
ScalarOp::Eq(a,b)=>seq![Part::Word(5),Part::Word(a as u128),Part::Word(b as u128)],
ScalarOp::Lt(a,b)=>seq![Part::Word(6),Part::Word(a as u128),Part::Word(b as u128)],
ScalarOp::And(a,b)=>seq![Part::Word(7),Part::Word(a as u128),Part::Word(b as u128)],
ScalarOp::Not(a)=>seq![Part::Word(8),Part::Word(a as u128)],
ScalarOp::Select(c,a,b)=>seq![Part::Word(9),Part::Word(c as u128),Part::Word(a as u128),Part::Word(b as u128)],
} }

pub open spec fn scalar_ops_prefix<'a>(values:Seq<ScalarOp>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{scalar_ops_prefix(values,(n-1) as nat)+(scalar_op(values[n-1]))}}
pub open spec fn scalar_ops<'a>(values:Seq<ScalarOp>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+scalar_ops_prefix(values,values.len())}

pub open spec fn roots_prefix<'a>(values:Seq<u16>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{roots_prefix(values,(n-1) as nat)+(seq![Part::Word(values[n-1] as u128)])}}
pub open spec fn roots<'a>(values:Seq<u16>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+roots_prefix(values,values.len())}

#[verifier::opaque]
pub open spec fn program<'a>(value:ScalarProgram<'a>)->Seq<Part<'a>> { scalar_domains(value.inputs@)+scalar_domains(value.outputs@)+scalar_ops(value.nodes@)+roots(value.roots@) }

pub open spec fn domain<'a>(value:Domain<'a>)->Seq<Part<'a>> { match value{
Domain::Bool=>seq![Part::Word(0)],
Domain::I128{min,max}=>seq![Part::Word(1),Part::Word(min as u128),Part::Word(max as u128)],
Domain::U128{min,max}=>seq![Part::Word(2),Part::Word(min),Part::Word(max)],
Domain::Enum{type_id,variants}=>seq![Part::Word(3),Part::Word(type_id as u128)]+variants_ids(variants@),
Domain::Sum{type_id,variants}=>seq![Part::Word(4),Part::Word(type_id as u128)]+variants_ids(variants@),
Domain::Bytes=>seq![Part::Word(5)],Domain::Text=>seq![Part::Word(6)],
} }

pub open spec fn expr<'a>(value:Expr<'a>)->Seq<Part<'a>> { match value{
Expr::Input(s,id)=>seq![Part::Word(0)]+source(s)+seq![Part::Word(id as u128)],
Expr::Output(index)=>seq![Part::Word(1),Part::Word(index as u128)],
Expr::Constant(v)=>seq![Part::Word(2)]+decision_atom(v),
Expr::Root(s)=>seq![Part::Word(3)]+source(s),
} }

pub open spec fn assignments_prefix<'a>(values:Seq<Assignment<'a>>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{assignments_prefix(values,(n-1) as nat)+(seq![Part::Word(values[n-1].field as u128)]+expr(values[n-1].value)+domain(values[n-1].domain))}}
#[verifier::opaque]
pub open spec fn assignments<'a>(values:Seq<Assignment<'a>>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+assignments_prefix(values,values.len())}

pub open spec fn payload_prefix<'a>(values:Seq<PayloadField<'a>>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{payload_prefix(values,(n-1) as nat)+(seq![Part::Word(values[n-1].field as u128)]+expr(values[n-1].value))}}
pub open spec fn payload<'a>(values:Seq<PayloadField<'a>>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+payload_prefix(values,values.len())}

pub open spec fn deliveries_prefix<'a>(values:Seq<DeliveryPlan<'a>>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{deliveries_prefix(values,(n-1) as nat)+(seq![Part::Word(values[n-1].ordinal as u128),Part::Word(values[n-1].channel as u128)]+expr(values[n-1].when)+expr(values[n-1].destination)+payload(values[n-1].payload@)+expr(values[n-1].idempotency))}}
#[verifier::opaque]
pub open spec fn deliveries<'a>(values:Seq<DeliveryPlan<'a>>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+deliveries_prefix(values,values.len())}

pub open spec fn branches_prefix<'a>(values:Seq<Branch<'a>>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{branches_prefix(values,(n-1) as nat)+(seq![Part::Word(values[n-1].code as u128)]+class(values[n-1].class)+reason(values[n-1].reason)+assignments(values[n-1].assignments@)+deliveries(values[n-1].effects@)+deliveries(values[n-1].outbox@))}}
#[verifier::opaque]
pub open spec fn branches<'a>(values:Seq<Branch<'a>>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+branches_prefix(values,values.len())}

pub open spec fn law_atom<'a>(value:laws::Atom<'a>)->Seq<Part<'a>> { match value{
laws::Atom::Bool(v)=>seq![Part::Word(0),Part::Word(v as u128)],
laws::Atom::I128(v)=>seq![Part::Word(1),Part::Word(v as u128)],
laws::Atom::U128(v)=>seq![Part::Word(2),Part::Word(v)],
laws::Atom::Enum{type_id,variant}=>seq![Part::Word(3),Part::Word(type_id as u128),Part::Word(variant as u128)],
laws::Atom::Sum{type_id,variant}=>seq![Part::Word(4),Part::Word(type_id as u128),Part::Word(variant as u128)],
laws::Atom::Bytes(v)=>seq![Part::Word(5),Part::Bytes(v)],laws::Atom::Text(v)=>seq![Part::Word(6),Part::Bytes(v)],

} }

pub open spec fn division<'a>(value:laws::Division)->Seq<Part<'a>> { match value {
laws::Division::Exact=>seq![Part::Word(0)],
laws::Division::Floor=>seq![Part::Word(1)],
laws::Division::Ceil=>seq![Part::Word(2)],
} }

pub open spec fn law_op<'a>(value:laws::Op<'a>)->Seq<Part<'a>> { match value{
laws::Op::Literal(v)=>seq![Part::Word(0)]+law_atom(v),
laws::Op::Observe(o)=>seq![Part::Word(1)]+super::spec::observation(o),
laws::Op::ObserveWhen(guard,o,default)=>seq![Part::Word(12),Part::Word(guard as u128)]+super::spec::observation(o)+law_atom(default),
laws::Op::Add(a,b)=>seq![Part::Word(2)]+seq![Part::Word(a as u128),Part::Word(b as u128)],
laws::Op::Sub(a,b)=>seq![Part::Word(3)]+seq![Part::Word(a as u128),Part::Word(b as u128)],
laws::Op::Mul(a,b)=>seq![Part::Word(4)]+seq![Part::Word(a as u128),Part::Word(b as u128)],
laws::Op::Div(round,a,b)=>seq![Part::Word(5)]+division(round)+seq![Part::Word(a as u128),Part::Word(b as u128)],
laws::Op::ToI128(a)=>seq![Part::Word(6)]+seq![Part::Word(a as u128)],
laws::Op::Eq(a,b)=>seq![Part::Word(7)]+seq![Part::Word(a as u128),Part::Word(b as u128)],
laws::Op::Lt(a,b)=>seq![Part::Word(8)]+seq![Part::Word(a as u128),Part::Word(b as u128)],
laws::Op::And(a,b)=>seq![Part::Word(9)]+seq![Part::Word(a as u128),Part::Word(b as u128)],
laws::Op::Not(a)=>seq![Part::Word(10)]+seq![Part::Word(a as u128)],
laws::Op::Select(c,a,b)=>seq![Part::Word(11)]+seq![Part::Word(c as u128),Part::Word(a as u128),Part::Word(b as u128)],
} }

pub open spec fn law_ops_prefix<'a>(values:Seq<laws::Op<'a>>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{law_ops_prefix(values,(n-1) as nat)+(law_op(values[n-1]))}}
pub open spec fn law_ops<'a>(values:Seq<laws::Op<'a>>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+law_ops_prefix(values,values.len())}

pub open spec fn law_kind<'a>(value:laws::Kind)->Seq<Part<'a>> { match value {
laws::Kind::StateInvariant=>seq![Part::Word(0)],
laws::Kind::AssetConservation=>seq![Part::Word(1)],
laws::Kind::MintBurnAuthorization=>seq![Part::Word(2)],
laws::Kind::DebitCreditEffectEquality=>seq![Part::Word(3)],
laws::Kind::FeeAndRounding=>seq![Part::Word(4)],
laws::Kind::AuthoritySubjectRecipient=>seq![Part::Word(5)],
laws::Kind::RejectNoAuthority=>seq![Part::Word(6)],
laws::Kind::CommittedFailureEffects=>seq![Part::Word(7)],
laws::Kind::DecisionConformance=>seq![Part::Word(8)],
laws::Kind::InitialCondition=>seq![Part::Word(9)],
} }

pub open spec fn law_scope<'a>(value:laws::Scope)->Seq<Part<'a>> { match value {
laws::Scope::Always=>seq![Part::Word(0)],
laws::Scope::Accept=>seq![Part::Word(1)],
laws::Scope::Reject=>seq![Part::Word(2)],
laws::Scope::CommittedFailure=>seq![Part::Word(3)],
laws::Scope::Committing=>seq![Part::Word(4)],
} }

pub open spec fn law_list_prefix<'a>(values:Seq<laws::Law<'a>>,n:nat)->Seq<Part<'a>>
recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{law_list_prefix(values,(n-1) as nat)+(seq![Part::Word(values[n-1].id as u128)]+law_kind(values[n-1].kind)+law_scope(values[n-1].scope)+seq![Part::Word(values[n-1].genesis as u128)]+law_ops(values[n-1].program.nodes@)+seq![Part::Word(values[n-1].program.root as u128)])}}
#[verifier::opaque]
pub open spec fn law_list<'a>(values:Seq<laws::Law<'a>>)->Seq<Part<'a>>{seq![Part::Word(values.len() as u128)]+law_list_prefix(values,values.len())}

#[verifier::opaque]
pub open spec fn limits<'a>(value:Limits)->Seq<Part<'a>> { seq![Part::Word(value.view()[0] as u128),Part::Word(value.view()[1] as u128),Part::Word(value.view()[2] as u128),Part::Word(value.view()[3] as u128),Part::Word(value.view()[4] as u128),Part::Word(value.view()[5] as u128),Part::Word(value.view()[6] as u128),Part::Word(value.view()[7] as u128)] }

}
