//! Closed descriptor-family binding; original legacy policy and outcome equations remain independent.
use super::super::{
    catalog::BoundCatalog,
    composition::{
        self, Descriptor, DescriptorData, DescriptorProjection, Framing, Kind, NumericDescriptor,
        Raw,
    },
    laws,
};
use super::{
    Refusal, bound_spec, numeric_metadata_spec, outcome_spec as audit, policy_spec,
    publication::spec as publication, spec as encoding,
};
use composition::descriptor::{DescriptorView, spec as data};
use vstd::prelude::*;
verus! {
pub type BoundView<'a,D>=(&'a D,Framing,Seq<u8>,Seq<(u32,u32,u32)>);
#[verifier::opaque]
pub open(in super) spec fn numeric_policy(d:&NumericDescriptor,schema:Seq<u8>,framing:&Framing,channels:Seq<(u32,u32,u32)>)->Option<Seq<u8>> {
    encoding::token_encode(seq![encoding::Token::Word(0x5a504f32),encoding::Token::Word(2),encoding::Token::Bytes(schema),encoding::Token::Bytes(bound_spec::framing_bytes(framing))]
        +policy_spec::links(channels)+encoding::tokens(numeric_metadata_spec::fields(d)))
}
#[verifier::opaque]
pub open(in super) spec fn policy(d:&DescriptorProjection,schema:Seq<u8>,framing:&Framing,channels:Seq<(u32,u32,u32)>)->Option<Seq<u8>> {
    match d.view{DescriptorView::Legacy(d)=>policy_spec::policy(d,schema,framing,channels),DescriptorView::Numeric(d)=>numeric_policy(d,schema,framing,channels)}
}
#[verifier::opaque]
pub open(in super) spec fn construction<'a,D:DescriptorData<'a>+'a>(d:&'a D,f:Framing,policy:Seq<u8>,links:Seq<(u32,u32,u32)>)->Result<BoundView<'a,D>,Refusal> {
    if !d.model().admitted(){Err(Refusal::Core(composition::Failure::Metadata))}else{
        match encoding::identity(policy,super::evaluator::EVALUATOR@){None=>Err(Refusal::Encoding),Some(identity)=>Ok((d,f,identity,links))}
    }
}
#[verifier::opaque]
pub open(in super) spec fn bind_value<'a,D:DescriptorData<'a>+'a>(catalog:&BoundCatalog<'a,D>)->Result<BoundView<'a,D>,Refusal> {
    construction(catalog.view().2,*catalog.view().3,catalog.view().1,catalog.view().4)
}
#[verifier::opaque]
pub open(in super) spec fn transition<'p,'a,D:DescriptorData<'p>+'p>(bound:BoundView<'p,D>,raw:Raw,finished:audit::EvaluationView<'a>,expected:Option<Seq<u8>>)->bool {
    let v=finished.0;let d=bound.0.model();
    v.1==composition::raw_view(raw)
    &&composition::family_framed(&d,Kind::Transition,raw,&bound.1,data::limits(&d).view(),Seq::new(8,|_:int|0u64),
        (Seq::empty(),Seq::empty(),None,None),Seq::<laws::Diagnostic>::empty(),Seq::<laws::ReadAttempt>::empty(),(v.0,v.3,v.2,v.4,v.5))
    &&finished.1==bound_spec::compared(audit::seal(v,Kind::Transition,bound.2),expected)
}
#[verifier::opaque]
pub open(in super) spec fn genesis<'p,D:DescriptorData<'p>+'p>(bound:BoundView<'p,D>,original:Seq<u8>,finished:audit::EvaluationView,expected:Option<Seq<u8>>)->bool {
    let v=finished.0;let d=bound.0.model();
    v.1==(original,Seq::<u8>::empty(),Seq::<u8>::empty())
    &&composition::family_genesis(&d,original,&bound.1.state,data::limits(&d).view(),Seq::new(8,|_:int|0u64),
        (Seq::empty(),Seq::empty(),None,None),Seq::<laws::Diagnostic>::empty(),Seq::<laws::ReadAttempt>::empty(),(v.0,v.3,v.2,v.4,v.5))
    &&finished.1==bound_spec::compared(audit::seal(v,Kind::Genesis,bound.2),expected)
}
#[verifier::opaque]
pub open(in super) spec fn publication_transition<'p,D:DescriptorData<'p>+'p>(bound:BoundView<'p,D>,raw:Raw,r:publication::OutcomeView,expected:Option<Seq<u8>>)->bool {
    transition(bound,raw,r.0,None)&&r==publication::finished(r.0,Kind::Transition,bound.2,bound.1.state,bound.3,expected)
}
#[verifier::opaque]
pub open(in super) spec fn publication_genesis<'p,D:DescriptorData<'p>+'p>(bound:BoundView<'p,D>,raw:Seq<u8>,r:publication::OutcomeView,expected:Option<Seq<u8>>)->bool {
    genesis(bound,raw,r.0,None)&&r==publication::finished(r.0,Kind::Genesis,bound.2,bound.1.state,bound.3,expected)
}
proof fn legacy_construction<'a>(d:&'a Descriptor<'a>,f:Framing,policy:Seq<u8>,links:Seq<(u32,u32,u32)>)
    ensures construction(d,f,policy,links)==bound_spec::construction(d,f,policy,links),
{
    reveal(construction);reveal(<Descriptor as DescriptorData>::model);composition::descriptor::legacy_admitted(d);
}
pub proof fn legacy_policy(d:&Descriptor,schema:Seq<u8>,f:&Framing,links:Seq<(u32,u32,u32)>)
    ensures policy(&d.model(),schema,f,links)==policy_spec::policy(d,schema,f,links),
{reveal(policy);reveal(<Descriptor as DescriptorData>::model);}
/// Bridge the closed legacy wrapper within its defining authority scope.
pub(crate) proof fn legacy_policy_value(d:&Descriptor,schema:Seq<u8>,f:&Framing,links:Seq<(u32,u32,u32)>)
    ensures policy(&d.model(),schema,f,links)==super::policy_value(d,schema,f,links),
{
    legacy_policy(d,schema,f,links);
    reveal(super::policy_value);
}
/// The generic constructor's legacy specialization retains the old public binding relation.
pub proof fn legacy_bind_value<'p>(catalog:&BoundCatalog<'p>)
    ensures bind_value(catalog)==super::bind_value(catalog),
{
    reveal(bind_value);
    reveal(super::bind_value);
    legacy_construction(catalog.view().2,*catalog.view().3,catalog.view().1,catalog.view().4);
}

/// Equality of complete evaluation relations, for every expected byte sequence or None.
/// No admitted-only or success-only premise hides malformed-input or budget refusals.
pub proof fn legacy_transition<'p,'a>(bound:bound_spec::BoundView<'p>,raw:Raw,
    finished:audit::EvaluationView<'a>,expected:Option<Seq<u8>>)
    ensures transition(bound,raw,finished,expected)==bound_spec::transition(bound,raw,finished,expected),
{
    reveal(transition);
    reveal(<Descriptor as DescriptorData>::model);
    let d=bound.0;
    let v=finished.0;
    assert(data::limits(&d.model()).view()==d.limits.view());
    assert(d.limits.view().len()==8);
    composition::legacy_family_transition(d,raw,&bound.1,d.limits.view(),Seq::new(8,|_:int|0u64),
        (Seq::empty(),Seq::empty(),None,None),Seq::<laws::Diagnostic>::empty(),Seq::<laws::ReadAttempt>::empty(),
        (v.0,v.3,v.2,v.4,v.5));
}

/// Genesis specializes without reconstructing or replacing the original initial bytes.
pub proof fn legacy_genesis<'p>(bound:bound_spec::BoundView<'p>,original:Seq<u8>,
    finished:audit::EvaluationView,expected:Option<Seq<u8>>)
    ensures genesis(bound,original,finished,expected)==bound_spec::genesis(bound,original,finished,expected),
{
    reveal(genesis);
    reveal(<Descriptor as DescriptorData>::model);
    let d=bound.0;
    let v=finished.0;
    assert(data::limits(&d.model()).view()==d.limits.view());
    assert(d.limits.view().len()==8);
    composition::legacy_family_genesis(d,original,&bound.1.state,d.limits.view(),Seq::new(8,|_:int|0u64),
        (Seq::empty(),Seq::empty(),None,None),Seq::<laws::Diagnostic>::empty(),Seq::<laws::ReadAttempt>::empty(),
        (v.0,v.3,v.2,v.4,v.5));
}

/// Publication equality includes post wire bytes, all delivery wires, identity and refusals.
/// It reuses the exact existing publication::finished equation on both sides.
pub proof fn legacy_publication_transition<'p>(bound:bound_spec::BoundView<'p>,raw:Raw,
    finished:publication::OutcomeView,expected:Option<Seq<u8>>)
    ensures publication_transition(bound,raw,finished,expected)
        ==publication::transition(bound,raw,finished,expected),
{
    reveal(publication_transition);
    legacy_transition(bound,raw,finished.0,None);
}

pub proof fn legacy_publication_genesis<'p>(bound:bound_spec::BoundView<'p>,original:Seq<u8>,
    finished:publication::OutcomeView,expected:Option<Seq<u8>>)
    ensures publication_genesis(bound,original,finished,expected)
        ==publication::genesis(bound,original,finished,expected),
{
    reveal(publication_genesis);
    legacy_genesis(bound,original,finished.0,None);
}

/// Explicit evaluation-replay corollary: exact comparison, including failure precedence.
pub proof fn legacy_replay<'p,'a>(bound:bound_spec::BoundView<'p>,raw:Raw,
    finished:audit::EvaluationView<'a>,expected:Seq<u8>)
    ensures transition(bound,raw,finished,Some(expected))
        ==bound_spec::transition(bound,raw,finished,Some(expected)),
        transition(bound,raw,finished,Some(expected))&&finished.1.is_ok()
            ==> finished.1.unwrap()==expected,
{
    legacy_transition(bound,raw,finished,Some(expected));
}

pub proof fn legacy_replay_genesis<'p>(bound:bound_spec::BoundView<'p>,original:Seq<u8>,
    finished:audit::EvaluationView,expected:Seq<u8>)
    ensures genesis(bound,original,finished,Some(expected))
        ==bound_spec::genesis(bound,original,finished,Some(expected)),
        genesis(bound,original,finished,Some(expected))&&finished.1.is_ok()
            ==> finished.1.unwrap()==expected,
{
    legacy_genesis(bound,original,finished,Some(expected));
}

/// Publication replay has its own subject: it never substitutes evaluation bytes.
pub proof fn legacy_replay_publication<'p>(bound:bound_spec::BoundView<'p>,raw:Raw,
    finished:publication::OutcomeView,expected:Seq<u8>)
    ensures publication_transition(bound,raw,finished,Some(expected))
        ==publication::transition(bound,raw,finished,Some(expected)),
{
    legacy_publication_transition(bound,raw,finished,Some(expected));
}

pub proof fn legacy_replay_genesis_publication<'p>(bound:bound_spec::BoundView<'p>,original:Seq<u8>,
    finished:publication::OutcomeView,expected:Seq<u8>)
    ensures publication_genesis(bound,original,finished,Some(expected))
        ==publication::genesis(bound,original,finished,Some(expected)),
{
    legacy_publication_genesis(bound,original,finished,Some(expected));
}

}
