//! Library-owned raw invocation, typed decision and law composition.
use super::super::evaluation::Domain as ScalarDomain;
use super::{
    InputField, InputLeaf, Limits, Resource, Usage, decision, input_view, laws, meter::Meter,
};
use alloc::vec::Vec;
pub use decision::{
    Assignment, Atom, Attempt, Branch, Candidate, Class, DeliveryPlan, Domain, Expr, Field,
    PayloadField, Source,
};
pub(crate) mod ingress;
pub use ingress::{Projection, Read as ReadAttempt, Schema, Selector};
pub(crate) mod descriptor;
pub use descriptor::{DescriptorData, DescriptorProjection};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Genesis cannot alias a transition, even when their state/artifact bytes match.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// Original initial state with actual initial-law evaluation.
    Genesis,
    /// Original invocation and its completed decision.
    Transition,
}

/// One typed scalar ABI input, including the distinct scalar root selector.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    /// Original invocation source.
    pub source: Source,
    /// Complete scalar root or declared record field.
    pub selector: Selector,
}
/// A payload field and its complete allowed value domain.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct TypedField<'a> {
    /// Declared field identifier.
    pub field: u16,
    /// Complete value domain.
    pub domain: Domain<'a>,
}
/// Declared delivery channel and the domains of all emitted values.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct Channel<'a> {
    /// Declared channel identifier.
    pub id: u32,
    /// Complete destination domain.
    pub destination: Domain<'a>,
    /// Complete ordered payload schema.
    pub payload: &'a [TypedField<'a>],
    /// Complete idempotency value domain.
    pub idempotency: Domain<'a>,
}
/// Declared business reason and its decision class.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct Reason {
    /// Declared reason identifier.
    pub id: u32,
    /// The class in which the reason is legal.
    pub class: Class,
}
/// Complete borrowed execution descriptor; binding checks every declaration.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Descriptor<'a, P = super::ScalarProgram<'a>, L = InputLeaf> {
    /// Original state schema.
    pub state: Schema<'a, L>,
    /// Original command schema, including scalar roots.
    pub command: Schema<'a, L>,
    /// Original context schema, including scalar roots.
    pub context: Schema<'a, L>,
    /// Actual eagerly evaluated scalar control graph.
    pub program: P,
    /// Complete scalar input ABI in program order.
    pub bindings: &'a [Binding],
    /// Typed inverse maps for each program output.
    pub output_types: &'a [L],
    /// Program output selecting the complete decision branch.
    pub decision_output: usize,
    /// Complete branch family, including every unused branch.
    pub branches: &'a [Branch<'a>],
    /// Complete reason declarations.
    pub reasons: &'a [Reason],
    /// Complete channel declarations.
    pub channels: &'a [Channel<'a>],
    /// Actual law programs and their applicability.
    pub laws: &'a [laws::Law<'a>],
    /// Complete required law identifiers.
    pub required: &'a [u32],
    /// Limits for the one meter shared by every execution stage.
    pub limits: Limits,
}
/// Full-width numeric descriptor data, admitted through the same composition boundary.
pub type NumericDescriptor<'a> = Descriptor<'a, super::numeric::NumericProgram<'a>, Domain<'a>>;
/// Exact received invocation bytes. Framed execution retains entire envelopes.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
pub struct Raw<'a> {
    /// Exact received state bytes.
    pub state: &'a [u8],
    /// Exact received command bytes.
    pub command: &'a [u8],
    /// Exact received context bytes.
    pub context: &'a [u8],
}
/// A technical refusal; no candidate is exposed after a refusal.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Failure {
    /// Complete descriptor admission failed before protected input work.
    Metadata,
    /// Typed ingress refused the numbered original source.
    Ingress(u8, ingress::Failure),
    /// The declared input ABI could not be constructed.
    Binding,
    /// Actual scalar graph evaluation refused.
    Execution(super::Failure),
    /// Actual scalar outputs could not be reified into their declared types.
    Output,
    /// Complete decision construction refused.
    Decision(decision::Failure),
    /// A produced complete candidate violated a declared value domain.
    Schema,
    /// An actual applicable law refused.
    Law(laws::Failure),
    /// Original envelope framing refused the numbered source.
    Frame(u8, FrameFailure),
}
/// Privately bound immutable descriptor with library-owned execution methods.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct BoundCore<'a, D: DescriptorData<'a> + 'a = Descriptor<'a>> {
    descriptor: &'a D,
}
mod admission;
#[cfg(verus_keep_ghost)]
mod spec;

mod outcome;
mod producer;
mod schema_validation;
pub(crate) mod shared;
pub(crate) use outcome::bind_family;
pub use outcome::{Outcome, bind, bind_numeric};
mod framed;
pub use framed::{Failure as FrameFailure, FrameBinding, Framing};
#[cfg(verus_keep_ghost)]
verus! {
impl<'p,D:DescriptorData<'p>+'p> BoundCore<'p,D>{pub closed spec fn descriptor_view(&self)->&'p D{self.descriptor}
    #[verifier::type_invariant]
    spec fn well_formed(&self)->bool{self.descriptor.model().admitted()}}
impl<'a> Outcome<'a>{pub closed spec fn kind_view(&self)->Kind{self.kind}
pub closed spec fn view(&self)->spec::OutcomeView<'a>{
    ((match &self.result{Ok(c)=>Ok(decision::spec::candidate_view(*c)),Err(e)=>Err(*e)}),spec::raw(self.raw),producer::spec::trace(&self.trace),self.usage.view(),self.diagnostics@,self.law_reads@)
}}
pub closed spec fn descriptor_admitted(d:&Descriptor)->bool{admission::spec::admitted(d)}
pub closed spec fn numeric_descriptor_admitted(d:&NumericDescriptor)->bool{admission::numeric::spec::admitted(d)}
pub open spec fn raw_view(r:Raw)->(Seq<u8>,Seq<u8>,Seq<u8>){(r.state@,r.command@,r.context@)}
pub open spec fn optional_usage(u:Option<Usage>)->Option<Seq<u64>>{match u{Some(u)=>Some(u.view()),None=>None}}
pub open spec fn candidate_result<'a>(r:Result<&Candidate<'a>,Failure>)->Result<decision::spec::CandidateView<'a>,Failure>{match r{
    Err(e)=>Err(e),Ok(c)=>{let v=c.view();Ok((v.0,v.1,v.2,v.3,v.4,
        v.5.map(|_:int,d:decision::Delivery|(d.ordinal,d.channel,d.destination,d.payload@,d.idempotency)),
        v.6.map(|_:int,d:decision::Delivery|(d.ordinal,d.channel,d.destination,d.payload@,d.idempotency))))},
}}
pub closed spec fn execution(d:&Descriptor,r:Raw,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)->bool{spec::execution(d,r,limits,used,trace,diagnostics,reads,finished)}
pub closed spec fn genesis_execution(d:&Descriptor,initial:Seq<u8>,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)->bool{spec::genesis(d,initial,limits,used,trace,diagnostics,reads,finished)}
pub closed spec fn framed_execution(d:&Descriptor,r:Raw,f:&Framing,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)->bool{framed::spec::execution(d,r,f,limits,used,trace,diagnostics,reads,finished)}
pub closed spec fn framed_genesis(d:&Descriptor,initial:Seq<u8>,f:&FrameBinding,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)->bool{framed::spec::genesis(d,initial,f,limits,used,trace,diagnostics,reads,finished)}
pub closed spec fn numeric_execution(d:&NumericDescriptor,r:Raw,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)->bool {
    shared::spec::execution(&d.model(),r,limits,used,trace,diagnostics,reads,finished)
}
pub closed spec fn numeric_genesis(d:&NumericDescriptor,initial:Seq<u8>,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)->bool {
    shared::spec::genesis(&d.model(),initial,limits,used,trace,diagnostics,reads,finished)
}

pub closed spec fn numeric_framed(d:&NumericDescriptor,kind:Kind,r:Raw,f:&Framing,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)->bool {
    shared::spec::framed(&d.model(),kind,r,f,limits,used,trace,diagnostics,reads,finished)
}

#[verifier::opaque]
pub open(in super) spec fn family_framed(d:&DescriptorProjection,kind:Kind,r:Raw,f:&Framing,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)->bool {
    shared::spec::framed(d,kind,r,f,limits,used,trace,diagnostics,reads,finished)
}
pub closed spec fn family_genesis(d:&DescriptorProjection,initial:Seq<u8>,f:&FrameBinding,limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)->bool {
    shared::spec::framed_genesis(d,initial,f,limits,used,trace,diagnostics,reads,finished)
}

/// Framed Genesis ignores command/context and uses the identical initial-state relation.
pub(crate) proof fn family_framed_genesis(d:&DescriptorProjection,r:Raw,f:&Framing,
    limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,
    diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)
    ensures family_framed(d,Kind::Genesis,r,f,limits,used,trace,diagnostics,reads,finished)
        ==family_genesis(d,r.state@,&f.state,limits,used,trace,diagnostics,reads,finished),
{
    reveal(family_framed);
    reveal(family_genesis);
}

/// Public-family transition relation specializes to the unchanged legacy wrapper.
/// Ghost meter-shape premises are not executable admission restrictions.
pub(crate) proof fn legacy_family_transition(d:&Descriptor,r:Raw,f:&Framing,
    limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,
    diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)
    requires limits.len()==8,used.len()==8,
    ensures family_framed(&d.model(),Kind::Transition,r,f,limits,used,trace,diagnostics,reads,finished)
        ==framed_execution(d,r,f,limits,used,trace,diagnostics,reads,finished),
{
    reveal(family_framed);
    reveal(framed_execution);
    shared::spec::legacy_framed(d,Kind::Transition,r,f,limits,used,trace,diagnostics,reads,finished);
}

/// Genesis keeps arbitrary original bytes and the complete old framing relation.
pub(crate) proof fn legacy_family_genesis(d:&Descriptor,initial:Seq<u8>,f:&FrameBinding,
    limits:Seq<u64>,used:Seq<u64>,trace:producer::spec::TraceView,
    diagnostics:Seq<laws::Diagnostic>,reads:Seq<laws::ReadAttempt>,finished:spec::Finished)
    requires limits.len()==8,used.len()==8,
    ensures family_genesis(&d.model(),initial,f,limits,used,trace,diagnostics,reads,finished)
        ==framed_genesis(d,initial,f,limits,used,trace,diagnostics,reads,finished),
{
    reveal(family_genesis);
    reveal(framed_genesis);
    shared::spec::legacy_framed_genesis(d,initial,f,limits,used,trace,diagnostics,reads,finished);
}

}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod record_tests;

#[cfg(test)]
mod account_tests;
