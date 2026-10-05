//! Private sealing consumes actual opaque producer outcomes and every report.
#[cfg(verus_keep_ghost)]
use super::super::composition;
use super::super::{
    Usage,
    composition::{Outcome, Raw, ReadAttempt, Selector},
    decision, laws,
};
#[cfg(verus_keep_ghost)]
use super::outcome_spec as model;
use super::{
    candidate::candidate_parts,
    canonical::{Part, encode},
    framing::{self, Kind},
    observations,
    refusal::Refusal,
};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures model::owned_bytes_result(result)==model::frame(kind,identity@,state@,command@,context@,artifact@),))]
fn seal_bytes(
    kind: Kind,
    identity: &[u8],
    state: &[u8],
    command: &[u8],
    context: &[u8],
    artifact: &[u8],
) -> Result<Vec<u8>, Refusal> {
    match framing::subject_bytes(kind, identity, state, command, context, artifact) {
        Some(b) => Ok(b),
        None => Err(Refusal::Encoding),
    }
}

#[cfg_attr(verus_keep_ghost,verus_spec(ensures final(parts)@==old(parts)@+model::reads(values@),))]
pub(super) fn append_reads<'a>(parts: &mut Vec<Part<'a>>, values: &[ReadAttempt]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=values.len(),initial==old(parts)@,
        parts@==initial+seq![Part::Word(values.len() as u128)]+model::reads_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        let value = values[i];
        parts.push(Part::Word(value.source as u128));
        match value.selector {
            Selector::Root => parts.push(Part::Word(0)),
            Selector::Field(id) => {
                parts.push(Part::Word(1));
                parts.push(Part::Word(id as u128));
            }
        }
        parts.push(Part::Word(value.permitted as u128));
        i += 1;
    }
}

/// Seal any actual outcome under the uniform tagged artifact shape. Genesis
/// reports its identity candidate and absent decision sections exactly like a
/// transition; only the kind tag differs.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures model::owned_bytes_result(result)==model::sealed(outcome,identity@),))]
pub(super) fn seal(outcome: &Outcome<'_>, identity: &[u8]) -> Result<Vec<u8>, Refusal> {
    let candidate = match outcome.result() {
        Ok(c) => c,
        Err(e) => return Err(Refusal::Core(e)),
    };
    let kind = outcome.kind();
    let mut parts = Vec::new();
    parts.push(Part::Word(0x5a4f5532));
    parts.push(Part::Word(1));
    parts.push(Part::Word(match kind {
        Kind::Genesis => 0,
        Kind::Transition => 1,
    }));
    let candidate_parts = candidate_parts(candidate);
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=candidate_parts.len(),
        parts@==seq![Part::Word(0x5a4f5532),Part::Word(1),Part::Word(match outcome.kind_view(){Kind::Genesis=>0u128,Kind::Transition=>1u128})]+candidate_parts@.take(i as int),decreases candidate_parts.len()-i,))]
    while i < candidate_parts.len() {
        parts.push(candidate_parts[i]);
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(candidate_parts@.take(candidate_parts@.len() as int) =~= candidate_parts@); }
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost head = parts@; }
    observations::append_optional_usage(&mut parts, outcome.ingress_usage());
    observations::append_optional_usage(&mut parts, outcome.decision_usage());
    observations::append_usage(&mut parts, outcome.usage());
    append_reads(&mut parts, outcome.reads());
    observations::append_decision_attempts(&mut parts, outcome.decision_attempts());
    observations::append_diagnostics(&mut parts, outcome.diagnostics());
    observations::append_law_reads(&mut parts, outcome.law_reads());
    #[cfg(verus_keep_ghost)]
    proof! {
        let v = outcome.view();
        assert(parts@ =~= head + model::reports(v.2, v.3, v.4, v.5));
        assert(Some(parts@)==model::parts(outcome.view(),outcome.kind_view()));
    }
    let artifact = match encode(&parts) {
        Some(b) => b,
        None => return Err(Refusal::Encoding),
    };
    let raw = outcome.raw();
    seal_bytes(
        kind,
        identity,
        raw.state,
        raw.command,
        raw.context,
        &artifact,
    )
}

/// Reports always come from the retained actual core outcome. A refused seal or
/// replay exposes neither a candidate nor a persisted subject.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Evaluation<'a> {
    pub(super) outcome: Outcome<'a>,
    pub(super) subject: Result<Vec<u8>, Refusal>,
}

impl<'a> Evaluation<'a> {
    /// Expose success only when the actual outcome and sealing or replay verdict both succeed.
    /// A successful genesis yields the unchanged identity candidate.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures model::candidate_result(result)==model::evaluation_result(self.view()),))]
    pub fn result(&self) -> Result<&decision::Candidate<'a>, Refusal> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::view);}
        match self.subject {
            Err(e) => Err(e),
            Ok(_) => match self.outcome.result() {
                Ok(c) => Ok(c),
                Err(e) => Err(Refusal::Core(e)),
            },
        }
    }
    /// Exact invocation kind; genesis cannot alias a transition.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result==self.kind_view(),))]
    pub fn kind(&self) -> Kind {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::kind_view);}
        self.outcome.kind()
    }
    /// Borrow the retained actual core outcome with every report.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.view()==self.view().0,result.kind_view()==self.kind_view(),))]
    pub fn outcome(&self) -> &Outcome<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::view);reveal(Evaluation::kind_view);}
        &self.outcome
    }
    /// Return the complete persisted subject, or the original technical refusal.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures model::bytes_result(result)==self.view().1,))]
    pub fn subject(&self) -> Result<&[u8], Refusal> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::view);}
        match &self.subject {
            Ok(bytes) => Ok(bytes.as_slice()),
            Err(e) => Err(*e),
        }
    }
    /// Original immutable framed input bytes retained by the actual evaluation.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures composition::raw_view(result)==self.view().0.1,))]
    pub fn raw(&self) -> Raw<'a> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::view);}
        self.outcome.raw()
    }
    /// Actual completed ingress checkpoint, when ingress completed.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures composition::optional_usage(result)==self.view().0.2.2,))]
    pub fn ingress_usage(&self) -> Option<Usage> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::view);}
        self.outcome.ingress_usage()
    }
    /// Actual completed decision checkpoint, when decision production completed.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures composition::optional_usage(result)==self.view().0.2.3,))]
    pub fn decision_usage(&self) -> Option<Usage> {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::view);}
        self.outcome.decision_usage()
    }
    /// Actual final logical usage, retained even after refusal.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result.view()==self.view().0.3,))]
    pub fn usage(&self) -> Usage {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::view);}
        self.outcome.usage()
    }
    /// Actual ordered ingress reads, including refused attempts.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().0.2.0,))]
    pub fn reads(&self) -> &[ReadAttempt] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::view);}
        self.outcome.reads()
    }
    /// Actual ordered decision attempts, including refused work.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().0.2.1,))]
    pub fn decision_attempts(&self) -> &[decision::Attempt] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::view);}
        self.outcome.decision_attempts()
    }
    /// Actual ordered law verdicts, retained even after refusal.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().0.4,))]
    pub fn diagnostics(&self) -> &[laws::Diagnostic] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::view);}
        self.outcome.diagnostics()
    }
    /// Actual ordered law reads, retained even after refusal.
    #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures result@==self.view().0.5,))]
    pub fn law_reads(&self) -> &[laws::ReadAttempt] {
        #[cfg(verus_keep_ghost)]
        proof! {reveal(Evaluation::view);}
        self.outcome.law_reads()
    }
}
