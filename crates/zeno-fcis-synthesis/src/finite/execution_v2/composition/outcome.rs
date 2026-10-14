//! Opaque products from one immutable descriptor and one private shared meter.
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Opaque actual result and complete retained execution report for one
/// invocation kind. Genesis yields the unchanged identity candidate.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Outcome<'a> {
    pub(super) kind: Kind,
    pub(super) result: Result<Candidate<'a>, Failure>,
    pub(super) raw: Raw<'a>,
    pub(super) trace: producer::Trace,
    pub(super) usage: Usage,
    pub(super) diagnostics: Vec<laws::Diagnostic>,
    pub(super) law_reads: Vec<laws::ReadAttempt>,
}

/// Bind only the actual complete closed descriptor. This lower-level product grants no shell authority.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures match result{Ok(c)=>descriptor_admitted(descriptor)&&c.descriptor_view()==descriptor,Err(e)=>!descriptor_admitted(descriptor)&&e==Failure::Metadata},))]
pub fn bind<'p>(descriptor: &'p Descriptor<'p>) -> Result<BoundCore<'p>, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(BoundCore::descriptor_view);reveal(descriptor_admitted);reveal(<Descriptor as DescriptorData>::model);super::descriptor::legacy_admitted(descriptor);}
    if !descriptor.project().admit() {
        return Err(Failure::Metadata);
    }
    Ok(BoundCore { descriptor })
}
/// Admit every declaration of a full-width numeric descriptor.
/// The bound composition data does not itself grant publication authority.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures match result{Ok(c)=>numeric_descriptor_admitted(descriptor)&&c.descriptor_view()==descriptor,Err(e)=>!numeric_descriptor_admitted(descriptor)&&e==Failure::Metadata},))]
pub fn bind_numeric<'p>(
    descriptor: &'p NumericDescriptor<'p>,
) -> Result<BoundCore<'p, NumericDescriptor<'p>>, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(BoundCore::descriptor_view);reveal(numeric_descriptor_admitted);reveal(<NumericDescriptor as DescriptorData>::model);super::descriptor::numeric_admitted(descriptor);}
    if !descriptor.project().admit() {
        return Err(Failure::Metadata);
    }
    Ok(BoundCore { descriptor })
}
impl<'p> BoundCore<'p, NumericDescriptor<'p>> {
    /// Same actual descriptor reference with complete numeric admission.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.descriptor_view(),numeric_descriptor_admitted(result),))]
    pub fn descriptor(&self) -> &'p NumericDescriptor<'p> {
        #[cfg(verus_keep_ghost)]
        proof! {use_type_invariant(self);reveal(BoundCore::descriptor_view);reveal(numeric_descriptor_admitted);reveal(<NumericDescriptor as DescriptorData>::model);super::descriptor::numeric_admitted(self.descriptor);}
        self.descriptor
    }
}
impl<'p> BoundCore<'p> {
    /// Exact immutable bound fields for descriptor identity and independent inspection.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.descriptor_view(),descriptor_admitted(result),))]
    pub fn descriptor(&self) -> &'p Descriptor<'p> {
        #[cfg(verus_keep_ghost)]
        proof! {use_type_invariant(self);reveal(BoundCore::descriptor_view);reveal(descriptor_admitted);reveal(<Descriptor as DescriptorData>::model);super::descriptor::legacy_admitted(self.descriptor);}
        self.descriptor
    }
    /// Admit originals, execute the graph, construct the complete candidate and evaluate every applicable law.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.view().1==raw_view(raw),
        execution(self.descriptor_view(),raw,self.descriptor_view().limits.view(),Seq::new(8,|_:int|0u64),
            (Seq::empty(),Seq::empty(),None,None),Seq::empty(),Seq::empty(),
            (result.view().0,result.view().3,result.view().2,result.view().4,result.view().5)),
        result.kind_view()==Kind::Transition,))]
    pub fn execute<'a>(&'a self, raw: Raw<'a>) -> Outcome<'a> {
        let result = self.execute_family(Kind::Transition, raw);
        #[cfg(verus_keep_ghost)]
        proof! {reveal(BoundCore::descriptor_view);reveal(Outcome::view);reveal(Outcome::kind_view);reveal(Limits::view);reveal(Usage::view);
        shared::spec::legacy_execution(self.descriptor_view(),Kind::Transition,raw,
        self.descriptor.limits.view(),Seq::new(8,|_:int|0u64),(Seq::empty(),Seq::empty(),None,None),Seq::empty(),Seq::empty(),
        (result.view().0,result.view().3,result.view().2,result.view().4,result.view().5));}
        result
    }
    /// Check the actual initial state, with no invented command or context.
    /// A passing genesis yields the unchanged identity candidate.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.view().1==(initial@,Seq::<u8>::empty(),Seq::<u8>::empty()),
        genesis_execution(self.descriptor_view(),initial@,self.descriptor_view().limits.view(),Seq::new(8,|_:int|0u64),
            (Seq::empty(),Seq::empty(),None,None),Seq::empty(),Seq::empty(),
            (result.view().0,result.view().3,result.view().2,result.view().4,result.view().5)),
        result.kind_view()==Kind::Genesis,))]
    pub fn genesis<'a>(&'a self, initial: &'a [u8]) -> Outcome<'a> {
        let result = self.execute_family(
            Kind::Genesis,
            Raw {
                state: initial,
                command: &[],
                context: &[],
            },
        );
        #[cfg(verus_keep_ghost)]
        proof! {reveal(BoundCore::descriptor_view);reveal(Outcome::view);reveal(Outcome::kind_view);reveal(Limits::view);reveal(Usage::view);
        shared::spec::legacy_execution(self.descriptor_view(),Kind::Genesis,Raw{state:initial,command:&[],context:&[]},
        self.descriptor.limits.view(),Seq::new(8,|_:int|0u64),(Seq::empty(),Seq::empty(),None,None),Seq::empty(),Seq::empty(),
        (result.view().0,result.view().3,result.view().2,result.view().4,result.view().5));}
        result
    }
}
impl<'a> Outcome<'a> {
    /// Exact invocation kind; genesis cannot alias a transition.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.kind_view(),))]
    pub fn kind(&self) -> Kind {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Outcome::kind_view);}
        self.kind
    }
    /// Borrow the complete candidate only after every execution stage succeeds.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures candidate_result(result)==self.view().0,))]
    pub fn result(&self) -> Result<&Candidate<'a>, Failure> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Outcome::view);reveal(candidate_result);}
        let result = match &self.result {
            Ok(c) => Ok(c),
            Err(e) => Err(*e),
        };
        #[cfg(verus_keep_ghost)]
        proof! {super::spec::candidate_normalizer(result);}
        result
    }
    /// Exact received bytes, including full headers for framed execution.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures raw_view(result)==self.view().1,))]
    pub fn raw(&self) -> Raw<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Outcome::view);}
        self.raw
    }
    /// Actual usage immediately after successful original ingress.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures optional_usage(result)==self.view().2.2,))]
    pub fn ingress_usage(&self) -> Option<Usage> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Outcome::view);}
        self.trace.ingress
    }
    /// Actual usage immediately after complete decision construction.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures optional_usage(result)==self.view().2.3,))]
    pub fn decision_usage(&self) -> Option<Usage> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Outcome::view);}
        self.trace.decision
    }
    /// Final usage, including the exact prefix retained after a refusal.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.view()==self.view().3,))]
    pub fn usage(&self) -> Usage {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Outcome::view);}
        self.usage
    }
    /// Every actual ingress read attempt in interpretation order.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().2.0,))]
    pub fn reads(&self) -> &[ReadAttempt] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Outcome::view);}
        &self.trace.reads
    }
    /// Every actual candidate, write, effect, and outbox attempt in order.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().2.1,))]
    pub fn decision_attempts(&self) -> &[Attempt] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Outcome::view);}
        &self.trace.attempts
    }
    /// Actual law verdict prefix, including skipped and refusing clauses.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().4,))]
    pub fn diagnostics(&self) -> &[laws::Diagnostic] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Outcome::view);}
        &self.diagnostics
    }
    /// Every actual law observation attempt, including denied reads.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().5,))]
    pub fn law_reads(&self) -> &[laws::ReadAttempt] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Outcome::view);}
        &self.law_reads
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    super::spec::evaluated(core.descriptor_view(),kind,raw,old(meter).limits.counters@,old(meter).used.counters@,producer::spec::trace(old(trace)),old(diagnostics)@,old(law_reads)@,
        ((match result{Ok(c)=>Ok(decision::spec::candidate_view(c)),Err(e)=>Err(e)}),final(meter).used.counters@,producer::spec::trace(final(trace)),final(diagnostics)@,final(law_reads)@)),))]
#[cfg(any(test, verus_keep_ghost))]
pub(super) fn run<'a>(
    core: &BoundCore<'a>,
    kind: Kind,
    raw: Raw<'a>,
    meter: &mut Meter,
    trace: &mut producer::Trace,
    diagnostics: &mut Vec<laws::Diagnostic>,
    law_reads: &mut Vec<laws::ReadAttempt>,
) -> Result<Candidate<'a>, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {use_type_invariant(core);reveal(BoundCore::descriptor_view);}
    let result = shared::run(core, kind, raw, meter, trace, diagnostics, law_reads);
    #[cfg(verus_keep_ghost)]
    proof! {shared::spec::legacy_execution(core.descriptor_view(),kind,raw,
    old(meter).limits.counters@,old(meter).used.counters@,producer::spec::trace(old(trace)),old(diagnostics)@,old(law_reads)@,
    ((match &result{Ok(c)=>Ok(decision::spec::candidate_view(*c)),Err(e)=>Err(*e)}),meter.used.counters@,producer::spec::trace(trace),diagnostics@,law_reads@));}
    result
}

impl<'p, D: DescriptorData<'p> + 'p> BoundCore<'p, D> {
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.kind_view()==kind,result.view().1==raw_view(raw),
        shared::spec::evaluated(&self.descriptor_view().model(),kind,raw,descriptor::spec::limits(&self.descriptor_view().model()).view(),Seq::new(8,|_:int|0u64),
        (Seq::empty(),Seq::empty(),None,None),Seq::empty(),Seq::empty(),(result.view().0,result.view().3,result.view().2,result.view().4,result.view().5)),))]
    pub(crate) fn execute_family<'a>(&'a self, kind: Kind, raw: Raw<'a>) -> Outcome<'a> {
        let d = self.descriptor.project();
        let mut meter = super::super::meter::new(d.limits());
        let mut trace = producer::Trace {
            reads: Vec::new(),
            attempts: Vec::new(),
            ingress: None,
            decision: None,
        };
        let mut diagnostics = Vec::new();
        let mut law_reads = Vec::new();
        let result = shared::run(
            self,
            kind,
            raw,
            &mut meter,
            &mut trace,
            &mut diagnostics,
            &mut law_reads,
        );
        Outcome {
            kind,
            result,
            raw,
            trace,
            usage: meter.used,
            diagnostics,
            law_reads,
        }
    }
}
impl<'p> BoundCore<'p, NumericDescriptor<'p>> {
    /// Evaluate the actual full-width numeric descriptor through the shared composition pipeline.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.kind_view()==Kind::Transition,result.view().1==raw_view(raw),
        numeric_execution(self.descriptor_view(),raw,self.descriptor_view().limits.view(),Seq::new(8,|_:int|0u64),
        (Seq::empty(),Seq::empty(),None,None),Seq::empty(),Seq::empty(),(result.view().0,result.view().3,result.view().2,result.view().4,result.view().5)),))]
    pub fn execute<'a>(&'a self, raw: Raw<'a>) -> Outcome<'a> {
        self.execute_family(Kind::Transition, raw)
    }
    /// Check the original numeric state and applicable initial laws on the same meter.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.kind_view()==Kind::Genesis,result.view().1==(initial@,Seq::<u8>::empty(),Seq::<u8>::empty()),
        numeric_genesis(self.descriptor_view(),initial@,self.descriptor_view().limits.view(),Seq::new(8,|_:int|0u64),
        (Seq::empty(),Seq::empty(),None,None),Seq::empty(),Seq::empty(),(result.view().0,result.view().3,result.view().2,result.view().4,result.view().5)),))]
    pub fn genesis<'a>(&'a self, initial: &'a [u8]) -> Outcome<'a> {
        self.execute_family(
            Kind::Genesis,
            Raw {
                state: initial,
                command: &[],
                context: &[],
            },
        )
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures match result{Ok(c)=>descriptor.model().admitted()&&c.descriptor_view()==descriptor,Err(e)=>!descriptor.model().admitted()&&e==Failure::Metadata},))]
pub(crate) fn bind_family<'p, D: DescriptorData<'p> + 'p>(
    descriptor: &'p D,
) -> Result<BoundCore<'p, D>, Failure> {
    if !descriptor.project().admit() {
        return Err(Failure::Metadata);
    }
    Ok(BoundCore { descriptor })
}
impl<'p, D: DescriptorData<'p> + 'p> BoundCore<'p, D> {
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.descriptor_view(),result.model().admitted(),))]
    pub(crate) fn descriptor_data(&self) -> &'p D {
        #[cfg(verus_keep_ghost)]
        proof! {use_type_invariant(self);reveal(BoundCore::descriptor_view);}
        self.descriptor
    }
}
