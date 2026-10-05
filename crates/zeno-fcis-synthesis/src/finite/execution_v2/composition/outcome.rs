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
    proof! {reveal(BoundCore::descriptor_view);reveal(descriptor_admitted);}
    if !admission::admitted(descriptor) {
        return Err(Failure::Metadata);
    }
    Ok(BoundCore { descriptor })
}
impl<'p> BoundCore<'p> {
    /// Exact immutable bound fields for descriptor identity and independent inspection.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.descriptor_view(),descriptor_admitted(result),))]
    pub fn descriptor(&self) -> &'p Descriptor<'p> {
        #[cfg(verus_keep_ghost)]
        proof! {use_type_invariant(self);reveal(BoundCore::descriptor_view);reveal(descriptor_admitted);}
        self.descriptor
    }
    /// Admit originals, execute the graph, construct the complete candidate and evaluate every applicable law.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.view().1==raw_view(raw),
        execution(self.descriptor_view(),raw,self.descriptor_view().limits.view(),Seq::new(8,|_:int|0u64),
            (Seq::empty(),Seq::empty(),None,None),Seq::empty(),Seq::empty(),
            (result.view().0,result.view().3,result.view().2,result.view().4,result.view().5)),
        result.kind_view()==Kind::Transition,))]
    pub fn execute<'a>(&'a self, raw: Raw<'a>) -> Outcome<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(BoundCore::descriptor_view);reveal(Outcome::view);reveal(Outcome::kind_view);reveal(Limits::view);reveal(Usage::view);reveal(execution);}
        let mut meter = super::super::meter::new(self.descriptor.limits);
        let mut trace = producer::Trace {
            reads: Vec::new(),
            attempts: Vec::new(),
            ingress: None,
            decision: None,
        };
        let mut diagnostics = Vec::new();
        let mut law_reads = Vec::new();
        let result = run(
            self,
            Kind::Transition,
            raw,
            &mut meter,
            &mut trace,
            &mut diagnostics,
            &mut law_reads,
        );
        Outcome {
            kind: Kind::Transition,
            result,
            raw,
            trace,
            usage: meter.used,
            diagnostics,
            law_reads,
        }
    }
    /// Check the actual initial state, with no invented command or context.
    /// A passing genesis yields the unchanged identity candidate.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.view().1==(initial@,Seq::<u8>::empty(),Seq::<u8>::empty()),
        genesis_execution(self.descriptor_view(),initial@,self.descriptor_view().limits.view(),Seq::new(8,|_:int|0u64),
            (Seq::empty(),Seq::empty(),None,None),Seq::empty(),Seq::empty(),
            (result.view().0,result.view().3,result.view().2,result.view().4,result.view().5)),
        result.kind_view()==Kind::Genesis,))]
    pub fn genesis<'a>(&'a self, initial: &'a [u8]) -> Outcome<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(BoundCore::descriptor_view);reveal(Outcome::view);reveal(Outcome::kind_view);reveal(Limits::view);reveal(Usage::view);reveal(genesis_execution);}
        let mut meter = super::super::meter::new(self.descriptor.limits);
        let mut trace = producer::Trace {
            reads: Vec::new(),
            attempts: Vec::new(),
            ingress: None,
            decision: None,
        };
        let mut diagnostics = Vec::new();
        let mut law_reads = Vec::new();
        let raw = Raw {
            state: initial,
            command: &[],
            context: &[],
        };
        let result = run(
            self,
            Kind::Genesis,
            raw,
            &mut meter,
            &mut trace,
            &mut diagnostics,
            &mut law_reads,
        );
        Outcome {
            kind: Kind::Genesis,
            result,
            raw,
            trace,
            usage: meter.used,
            diagnostics,
            law_reads,
        }
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
pub(super) fn run<'a>(
    core: &BoundCore<'a>,
    kind: Kind,
    raw: Raw<'a>,
    meter: &mut Meter,
    trace: &mut producer::Trace,
    diagnostics: &mut Vec<laws::Diagnostic>,
    law_reads: &mut Vec<laws::ReadAttempt>,
) -> Result<Candidate<'a>, Failure> {
    match kind {
        Kind::Genesis => genesis_run(core, raw.state, meter, trace, diagnostics, law_reads),
        Kind::Transition => transition_run(core, raw, meter, trace, diagnostics, law_reads),
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    super::spec::execution(core.descriptor_view(),raw,old(meter).limits.counters@,old(meter).used.counters@,producer::spec::trace(old(trace)),old(diagnostics)@,old(law_reads)@,
        ((match result{Ok(c)=>Ok(decision::spec::candidate_view(c)),Err(e)=>Err(e)}),final(meter).used.counters@,producer::spec::trace(final(trace)),final(diagnostics)@,final(law_reads)@)),))]
fn transition_run<'a>(
    core: &BoundCore<'a>,
    raw: Raw<'a>,
    meter: &mut Meter,
    trace: &mut producer::Trace,
    diagnostics: &mut Vec<laws::Diagnostic>,
    law_reads: &mut Vec<laws::ReadAttempt>,
) -> Result<Candidate<'a>, Failure> {
    let d = core.descriptor;
    #[cfg(verus_keep_ghost)]
    proof! {use_type_invariant(core);reveal(BoundCore::descriptor_view);}
    let produced = producer::produce(core, raw, meter, trace)?;
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost middle=(Ok::<producer::spec::ProducedView,Failure>(producer::spec::produced(produced)),meter.used.counters@,producer::spec::trace(trace));}
    if !schema_validation::candidate(d, &produced.candidate) {
        #[cfg(verus_keep_ghost)]
        proof! {assert(super::spec::after_producer(d,middle,old(diagnostics)@,old(law_reads)@,old(meter).limits.counters@,(Err(Failure::Schema),meter.used.counters@,producer::spec::trace(trace),diagnostics@,law_reads@)));}
        return Err(Failure::Schema);
    }
    let candidate = laws::Candidate {
        class: produced.candidate.class(),
        reason: produced.candidate.reason(),
        post: decision::RootView::Record(produced.candidate.post()),
        patch: produced.candidate.patch(),
        effects: produced.candidate.effects(),
        outbox: produced.candidate.outbox(),
        reads: &trace.reads,
        attempts: &trace.attempts,
        usage: meter.used,
    };
    let frame = laws::Frame::Transition {
        pre: decision::RootView::Record(produced.candidate.pre()),
        command: producer::value(&produced.command),
        context: producer::value(&produced.context),
        candidate: &candidate,
    };
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost law_limits=meter.limits.counters@;let ghost law_used=meter.used.counters@;
    let ghost prior_diags=diagnostics@;let ghost prior_reads=law_reads@;}
    let result = laws::evaluate_into(d.laws, d.required, &frame, meter, diagnostics, law_reads);
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(Usage::view);
        assert(law_used==middle.1);
        assert(producer::spec::trace(trace)==middle.2);
        let ghost p=producer::spec::produced(produced);
        let ghost t=producer::spec::trace(trace);
        assert(super::spec::transition_frame(&frame,p,t,law_used));
        assert((result,meter.used.counters@,diagnostics@,law_reads@)==laws::law_execution(d.laws@,d.required@,&frame,law_limits,law_used,prior_diags,prior_reads));
        assert(super::spec::frame_evaluated(d,p,t,law_used,law_limits,prior_diags,prior_reads,(result,meter.used.counters@,diagnostics@,law_reads@)));
        assert(exists|r:Result<(),laws::Failure>|super::spec::frame_evaluated(d,p,t,law_used,law_limits,prior_diags,prior_reads,(r,meter.used.counters@,diagnostics@,law_reads@)));
        assert(super::spec::after_producer(d,middle,old(diagnostics)@,old(law_reads)@,old(meter).limits.counters@,
            ((match result{Ok(())=>Ok(p.0),Err(e)=>Err(Failure::Law(e))}),meter.used.counters@,t,diagnostics@,law_reads@)));
    }
    match result {
        Ok(()) => Ok(produced.candidate),
        Err(e) => Err(Failure::Law(e)),
    }
}
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    super::spec::genesis(core.descriptor_view(),initial@,old(meter).limits.counters@,old(meter).used.counters@,producer::spec::trace(old(trace)),old(diagnostics)@,old(law_reads)@,
        ((match result{Ok(c)=>Ok(decision::spec::candidate_view(c)),Err(e)=>Err(e)}),final(meter).used.counters@,producer::spec::trace(final(trace)),final(diagnostics)@,final(law_reads)@)),))]
fn genesis_run<'i>(
    core: &BoundCore<'i>,
    initial: &'i [u8],
    meter: &mut Meter,
    trace: &mut producer::Trace,
    diagnostics: &mut Vec<laws::Diagnostic>,
    law_reads: &mut Vec<laws::ReadAttempt>,
) -> Result<Candidate<'i>, Failure> {
    let d = core.descriptor;
    #[cfg(verus_keep_ghost)]
    proof! {use_type_invariant(core);reveal(BoundCore::descriptor_view);}
    let decoded = match ingress::project(initial, d.state, 0, meter, &mut trace.reads) {
        Ok(v) => v,
        Err(e) => return Err(Failure::Ingress(0, e)),
    };
    #[cfg(verus_keep_ghost)]
    proof! {reveal(Usage::view);}
    trace.ingress = Some(meter.used);
    let frame = laws::Frame::Genesis {
        initial: producer::value(&decoded.value),
    };
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost law_limits=meter.limits.counters@;let ghost law_used=meter.used.counters@;
    let ghost prior_diags=diagnostics@;let ghost prior_reads=law_reads@;}
    let result = laws::evaluate_into(d.laws, d.required, &frame, meter, diagnostics, law_reads);
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(Usage::view);
        let ghost s=ingress::spec::decoded(decoded);
        let ghost p=ingress::spec::project(initial@,d.state,0,old(meter).limits.counters@,old(meter).used.counters@);
        let ghost t=producer::spec::trace(old(trace));
        assert(p.0==Ok::<(ingress::spec::ValueView,Seq<i64>),ingress::Failure>(s));
        assert(law_limits==old(meter).limits.counters@&&law_used==p.1);
        assert(prior_diags==old(diagnostics)@&&prior_reads==old(law_reads)@);
        assert(trace.reads@==t.0+p.2&&trace.attempts@==t.1);
        assert(producer::spec::optional_usage(trace.ingress)==Some(p.1));
        assert(producer::spec::optional_usage(trace.decision)==t.3);
        assert(producer::spec::trace(trace)==(t.0+p.2,t.1,Some(p.1),t.3));
        assert(super::spec::genesis_frame(&frame,s.0));
        assert((result,meter.used.counters@,diagnostics@,law_reads@)==laws::law_execution(d.laws@,d.required@,&frame,law_limits,law_used,prior_diags,prior_reads));
        assert(super::spec::genesis_evaluated(d,s.0,law_limits,law_used,prior_diags,prior_reads,(result,meter.used.counters@,diagnostics@,law_reads@)));
        assert(exists|r:Result<(),laws::Failure>|super::spec::genesis_evaluated(d,s.0,law_limits,law_used,prior_diags,prior_reads,(r,meter.used.counters@,diagnostics@,law_reads@)));
        assert(admission::spec::admitted(d));
        let ghost finished=((match result{Ok(())=>Ok::<decision::spec::CandidateView,Failure>(super::spec::identity_candidate(s.0)),Err(e)=>Err(Failure::Law(e))}),meter.used.counters@,producer::spec::trace(trace),diagnostics@,law_reads@);
        assert(exists|r:Result<(),laws::Failure>|
            super::spec::genesis_evaluated(d,s.0,law_limits,law_used,prior_diags,prior_reads,(r,finished.1,finished.3,finished.4))
            &&finished.0==(match r{Ok(())=>Ok(super::spec::identity_candidate(s.0)),Err(e)=>Err(Failure::Law(e))})
            &&finished.2==(t.0+p.2,t.1,Some(p.1),t.3));
        assert(exists|r:Result<(),laws::Failure>|
            super::spec::genesis_evaluated(d,s.0,old(meter).limits.counters@,p.1,old(diagnostics)@,old(law_reads)@,(r,finished.1,finished.3,finished.4))
            &&finished.0==(match r{Ok(())=>Ok(super::spec::identity_candidate(s.0)),Err(e)=>Err(Failure::Law(e))})
            &&finished.2==(t.0+p.2,t.1,Some(p.1),t.3));
        assert(super::spec::genesis(d,initial@,old(meter).limits.counters@,old(meter).used.counters@,t,old(diagnostics)@,old(law_reads)@,finished)) by {
            reveal(super::spec::genesis);
            match p.0 {
                Ok(projected) => {
                    assert(projected == s);
                    assert(exists|r:Result<(),laws::Failure>|
                        super::spec::genesis_evaluated(d,projected.0,old(meter).limits.counters@,p.1,old(diagnostics)@,old(law_reads)@,(r,finished.1,finished.3,finished.4))
                        &&finished.0==(match r{Ok(())=>Ok(super::spec::identity_candidate(s.0)),Err(e)=>Err(Failure::Law(e))})
                        &&finished.2==(t.0+p.2,t.1,Some(p.1),t.3));
                }
                Err(_) => { assert(false); }
            }
        }
    }
    match result {
        Ok(()) => {
            #[cfg(verus_keep_ghost)]
            proof_decl! {let ghost s=ingress::spec::decoded(decoded);}
            let fields = match decoded.value {
                ingress::Value::Record(fields) => fields,
                ingress::Value::Leaf(_) => Vec::new(),
            };
            let candidate = decision::identity(fields);
            #[cfg(verus_keep_ghost)]
            proof! {assert(decision::spec::candidate_view(candidate)==super::spec::identity_candidate(s.0));}
            Ok(candidate)
        }
        Err(e) => Err(Failure::Law(e)),
    }
}
