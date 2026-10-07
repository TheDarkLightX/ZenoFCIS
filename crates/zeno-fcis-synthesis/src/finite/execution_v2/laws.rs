//! Closed library-owned law/genesis evaluation. This lower-level unit does not authorize.
use super::{Limits, Resource, Usage, meter, meter::Meter};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
mod types;
pub use types::*;
mod atoms;
mod frame;
mod predicate;
#[cfg(verus_keep_ghost)]
pub(super) mod spec;

/// Library-created result with exact retained diagnostics, reads and usage.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Outcome {
    result: Result<(), Failure>,
    usage: Usage,
    diagnostics: Vec<Diagnostic>,
    reads: Vec<ReadAttempt>,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::kind_tag(kind),))]
fn kind_tag(kind: Kind) -> u8 {
    match kind {
        Kind::StateInvariant => 0,
        Kind::AssetConservation => 1,
        Kind::MintBurnAuthorization => 2,
        Kind::DebitCreditEffectEquality => 3,
        Kind::FeeAndRounding => 4,
        Kind::AuthoritySubjectRecipient => 5,
        Kind::RejectNoAuthority => 6,
        Kind::CommittedFailureEffects => 7,
        Kind::DecisionConformance => 8,
        Kind::InitialCondition => 9,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::law_scope(law),))]
fn law_scope(law: &Law<'_>) -> bool {
    match law.kind {
        Kind::StateInvariant => matches!(law.scope, Scope::Committing) && law.genesis,
        Kind::RejectNoAuthority => matches!(law.scope, Scope::Reject) && !law.genesis,
        Kind::CommittedFailureEffects => {
            matches!(law.scope, Scope::CommittedFailure) && !law.genesis
        }
        Kind::DecisionConformance => matches!(law.scope, Scope::Always) && !law.genesis,
        Kind::InitialCondition => matches!(law.scope, Scope::Always) && law.genesis,
        _ => true,
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::applies(law,frame),))]
fn applies(law: &Law<'_>, frame: &Frame<'_>) -> bool {
    match frame {
        Frame::Genesis { .. } => law.genesis,
        Frame::Transition { candidate, .. } => {
            if matches!(law.kind, Kind::InitialCondition) {
                return false;
            }
            match law.scope {
                Scope::Always => true,
                Scope::Accept => matches!(candidate.class, Class::Accept),
                Scope::Reject => matches!(candidate.class, Class::Reject),
                Scope::CommittedFailure => matches!(candidate.class, Class::CommittedFailure),
                Scope::Committing => !matches!(candidate.class, Class::Reject),
            }
        }
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::has_kind(laws@,tag),))]
fn has_kind(laws: &[Law<'_>], tag: u8) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=laws.len(),forall|j:int| 0<=j<i ==> spec::kind_tag(laws@[j].kind)!=tag,
        decreases laws.len()-i,
    ))]
    while i < laws.len() {
        if kind_tag(laws[i].kind) == tag {
            return true;
        }
        i += 1;
    }
    false
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::has_id(laws@,id),))]
fn has_id(laws: &[Law<'_>], id: u32) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=laws.len(),forall|j:int| 0<=j<i ==> laws@[j].id!=id,
        decreases laws.len()-i,
    ))]
    while i < laws.len() {
        if laws[i].id == id {
            return true;
        }
        i += 1;
    }
    false
}
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == spec::metadata(laws@,required@),))]
pub(super) fn metadata(laws: &[Law<'_>], required: &[u32]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=laws.len(),forall|j:int| 0<=j<i ==> spec::law_valid(laws@,j),
        decreases laws.len()-i,
    ))]
    while i < laws.len() {
        if laws[i].id == 0 || !law_scope(&laws[i]) || !predicate::shape(&laws[i].program) {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::law_valid(laws@,i as int));}
            return false;
        }
        let mut j = 0usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant i<laws.len(),j<=i,forall|k:int| 0<=k<j ==> laws@[k].id!=laws@[i as int].id,
            decreases i-j,
        ))]
        while j < i {
            if laws[j].id == laws[i].id {
                #[cfg(verus_keep_ghost)]
                proof! {assert(!spec::law_valid(laws@,i as int));}
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    let mut r = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant r<=required.len(),forall|j:int| 0<=j<laws.len() ==> spec::law_valid(laws@,j),
            forall|j:int| 0<=j<r ==> spec::required_valid(laws@,required@,j),
        decreases required.len()-r,
    ))]
    while r < required.len() {
        if required[r] == 0 || !has_id(laws, required[r]) {
            #[cfg(verus_keep_ghost)]
            proof! {assert(!spec::required_valid(laws@,required@,r as int));}
            return false;
        }
        let mut j = 0usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant r<required.len(),j<=r,forall|k:int| 0<=k<j ==> required@[k]!=required@[r as int],
            decreases r-j,
        ))]
        while j < r {
            if required[j] == required[r] {
                #[cfg(verus_keep_ghost)]
                proof! {assert(!spec::required_valid(laws@,required@,r as int));}
                return false;
            }
            j += 1;
        }
        r += 1;
    }
    has_kind(laws, 0)
        && has_kind(laws, 6)
        && has_kind(laws, 7)
        && has_kind(laws, 8)
        && has_kind(laws, 9)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(meter).limits==old(meter).limits,
        (result,final(meter).used.counters@,final(diagnostics)@,final(reads)@)==
            law_execution(laws@,required@,frame,old(meter).limits.counters@,
                old(meter).used.counters@,old(diagnostics)@,old(reads)@),
))]
pub(super) fn evaluate_into<'a>(
    laws: &[Law<'a>],
    required: &[u32],
    frame: &Frame<'a>,
    meter: &mut Meter,
    diagnostics: &mut Vec<Diagnostic>,
    reads: &mut Vec<ReadAttempt>,
) -> Result<(), Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(law_execution);}
    if !metadata(laws, required) {
        return Err(Failure::Metadata);
    }
    if !frame::valid(frame) {
        return Err(Failure::Frame);
    }
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost initial_used=meter.used.counters@;
        let ghost initial_diagnostics=diagnostics@;
        let ghost initial_reads=reads@;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i<=laws.len(),spec::metadata(laws@,required@),spec::frame_valid(frame),
            meter.limits==old(meter).limits,
            initial_used==old(meter).used.counters@,initial_diagnostics==old(diagnostics)@,initial_reads==old(reads)@,
            spec::law_prefix(laws@,frame,i as nat,meter.limits.counters@,initial_used,initial_diagnostics,initial_reads)
                ==(Ok(()),meter.used.counters@,diagnostics@,reads@),
        decreases laws.len()-i,
    ))]
    while i < laws.len() {
        if !applies(&laws[i], frame) {
            diagnostics.push(Diagnostic {
                id: laws[i].id,
                verdict: Verdict::Skipped,
            });
        } else {
            let result = predicate::evaluate(&laws[i].program, frame, laws[i].id, meter, reads);
            let verdict = match result {
                Ok(()) => Verdict::Satisfied,
                Err(e) => Verdict::Refused(e),
            };
            diagnostics.push(Diagnostic {
                id: laws[i].id,
                verdict,
            });
            #[cfg(verus_keep_ghost)]
            proof! {if let Err(e)=result {spec::failed_laws(laws@,frame,(i+1) as nat,laws@.len(),meter.limits.counters@,initial_used,initial_diagnostics,initial_reads,e);}}
            result?;
        }
        i += 1;
    }
    Ok(())
}

impl Outcome {
    /// Consumes the checked result and retained operational reports.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures (result.0,result.1.view(),result.2@,result.3@)==self.view(),
    ))]
    pub fn into_parts(
        self,
    ) -> (
        Result<(), Failure>,
        Usage,
        Vec<Diagnostic>,
        Vec<ReadAttempt>,
    ) {
        (self.result, self.usage, self.diagnostics, self.reads)
    }
}
#[cfg(verus_keep_ghost)]
verus! {
impl Outcome {
    pub closed spec fn view(&self)->(Result<(),Failure>,Seq<u64>,Seq<Diagnostic>,Seq<ReadAttempt>) {
        (self.result,self.usage.view(),self.diagnostics@,self.reads@)
    }
}
}
/// Evaluates every applicable law against one frame with a fresh private meter.
/// This returns checked lower-level evidence, never transition authorization.
#[must_use]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.view()==law_execution(laws@,required@,frame,limits.view(),Seq::new(8,|_:int|0u64),Seq::empty(),Seq::empty()),
))]
pub fn evaluate<'a>(
    laws: &[Law<'a>],
    required: &[u32],
    frame: &Frame<'a>,
    limits: Limits,
) -> Outcome {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(Outcome::view);reveal(Limits::view);reveal(Usage::view);}
    let mut meter = meter::new(limits);
    let mut diagnostics = Vec::new();
    let mut reads = Vec::new();
    let result = evaluate_into(
        laws,
        required,
        frame,
        &mut meter,
        &mut diagnostics,
        &mut reads,
    );
    Outcome {
        result,
        usage: meter.used,
        diagnostics,
        reads,
    }
}
#[cfg(test)]
mod tests;

#[cfg(verus_keep_ghost)]
verus! {
pub closed spec fn law_execution(laws:Seq<Law>,required:Seq<u32>,frame:&Frame,limits:Seq<u64>,used:Seq<u64>,diagnostics:Seq<Diagnostic>,reads:Seq<ReadAttempt>)
    ->(Result<(),Failure>,Seq<u64>,Seq<Diagnostic>,Seq<ReadAttempt>) {
    spec::execution(laws,required,frame,limits,used,diagnostics,reads)
}
}

#[cfg(verus_keep_ghost)]
verus! {
/// Exports successful engine evaluation as every applicable predicate success.
pub(super) proof fn success_checks_every_applicable(laws:Seq<Law>,required:Seq<u32>,frame:&Frame,
    limits:Seq<u64>,used:Seq<u64>,diagnostics:Seq<Diagnostic>,reads:Seq<ReadAttempt>)
    requires law_execution(laws,required,frame,limits,used,diagnostics,reads).0==Ok::<(),Failure>(()),
    ensures spec::metadata(laws,required),spec::frame_valid(frame),
        forall|i:int| 0<=i<laws.len() && spec::applies(&laws[i],frame) ==>
            spec::predicate(&laws[i].program,frame,laws[i].id,limits,
                spec::law_prefix(laws,frame,i as nat,limits,used,diagnostics,reads).1,
                spec::law_prefix(laws,frame,i as nat,limits,used,diagnostics,reads).3).0==Ok::<(),Failure>(()),
{
    reveal(law_execution);
    spec::success_checks_every_applicable(laws,frame,laws.len(),limits,used,diagnostics,reads);
}
}
