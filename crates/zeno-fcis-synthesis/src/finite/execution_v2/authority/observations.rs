//! Exact logical usage and law diagnostic projection; no authorizing constructor.
use super::super::{MeterFailure, Resource, Usage};
use super::super::{decision, laws};
use super::canonical::Part;
#[cfg(verus_keep_ghost)]
use super::spec as model;
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Stable canonical tag of a logical resource.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result==model::resource(resource),))]
pub fn resource_tag(resource: Resource) -> u128 {
    resource.index() as u128
}
/// Append all eight logical usage counters in canonical order.
#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@==old(parts)@+model::usage(usage),))]
pub(super) fn append_usage<'a>(parts: &mut Vec<Part<'a>>, usage: Usage) {
    parts.push(Part::Word(usage.used(Resource::Read) as u128));
    parts.push(Part::Word(usage.used(Resource::Write) as u128));
    parts.push(Part::Word(usage.used(Resource::Candidate) as u128));
    parts.push(Part::Word(usage.used(Resource::Effect) as u128));
    parts.push(Part::Word(usage.used(Resource::Byte) as u128));
    parts.push(Part::Word(usage.used(Resource::WitnessByte) as u128));
    parts.push(Part::Word(usage.used(Resource::Depth) as u128));
    parts.push(Part::Word(usage.used(Resource::Step) as u128));
}
/// Presence remains distinct from a completed stage whose usage is all zero.
#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@==old(parts)@+model::optional_usage(match usage{None=>None,Some(u)=>Some(u.view())}),))]
pub(super) fn append_optional_usage<'a>(parts: &mut Vec<Part<'a>>, usage: Option<Usage>) {
    match usage {
        None => parts.push(Part::Word(0)),
        Some(u) => {
            parts.push(Part::Word(1));
            append_usage(parts, u);
        }
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@==old(parts)@+model::budget(failure),))]
fn append_budget<'a>(parts: &mut Vec<Part<'a>>, failure: MeterFailure) {
    parts.push(Part::Word(resource_tag(failure.resource)));
    parts.push(Part::Word(failure.limit as u128));
    parts.push(Part::Word(failure.attempted as u128));
    parts.push(Part::Word(failure.overflow as u128));
}
#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@==old(parts)@+model::law_failure(failure),))]
fn append_failure<'a>(parts: &mut Vec<Part<'a>>, failure: laws::Failure) {
    match failure {
        laws::Failure::Metadata => parts.push(Part::Word(0)),
        laws::Failure::Frame => parts.push(Part::Word(1)),
        laws::Failure::Undefined => parts.push(Part::Word(2)),
        laws::Failure::Violated => parts.push(Part::Word(3)),
        laws::Failure::Budget(b) => {
            parts.push(Part::Word(4));
            append_budget(parts, b);
        }
    }
}
/// Append the complete tagged law observation.
#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@==old(parts)@+model::observation(observation),))]
pub(super) fn append_observation<'a>(parts: &mut Vec<Part<'a>>, observation: laws::Observation) {
    match observation {
        laws::Observation::PreRoot => {
            parts.push(Part::Word(0));
        }
        laws::Observation::CommandRoot => {
            parts.push(Part::Word(1));
        }
        laws::Observation::ContextRoot => {
            parts.push(Part::Word(2));
        }
        laws::Observation::PostRoot => {
            parts.push(Part::Word(3));
        }
        laws::Observation::InitialRoot => {
            parts.push(Part::Word(4));
        }
        laws::Observation::Pre(a) => {
            parts.push(Part::Word(5));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::Command(a) => {
            parts.push(Part::Word(6));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::Context(a) => {
            parts.push(Part::Word(7));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::Post(a) => {
            parts.push(Part::Word(8));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::Initial(a) => {
            parts.push(Part::Word(9));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::Class => {
            parts.push(Part::Word(10));
        }
        laws::Observation::HasReason => {
            parts.push(Part::Word(11));
        }
        laws::Observation::Reason => {
            parts.push(Part::Word(12));
        }
        laws::Observation::PostLength => {
            parts.push(Part::Word(13));
        }
        laws::Observation::PatchLength => {
            parts.push(Part::Word(14));
        }
        laws::Observation::EffectLength => {
            parts.push(Part::Word(15));
        }
        laws::Observation::OutboxLength => {
            parts.push(Part::Word(16));
        }
        laws::Observation::PatchField(a) => {
            parts.push(Part::Word(17));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::PatchBefore(a) => {
            parts.push(Part::Word(18));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::PatchAfter(a) => {
            parts.push(Part::Word(19));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::EffectOrdinal(a) => {
            parts.push(Part::Word(20));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::EffectChannel(a) => {
            parts.push(Part::Word(21));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::EffectDestination(a) => {
            parts.push(Part::Word(22));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::EffectPayload(a, b) => {
            parts.push(Part::Word(23));
            parts.push(Part::Word(a as u128));
            parts.push(Part::Word(b as u128));
        }
        laws::Observation::EffectIdempotency(a) => {
            parts.push(Part::Word(24));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::OutboxOrdinal(a) => {
            parts.push(Part::Word(25));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::OutboxChannel(a) => {
            parts.push(Part::Word(26));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::OutboxDestination(a) => {
            parts.push(Part::Word(27));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::OutboxPayload(a, b) => {
            parts.push(Part::Word(28));
            parts.push(Part::Word(a as u128));
            parts.push(Part::Word(b as u128));
        }
        laws::Observation::OutboxIdempotency(a) => {
            parts.push(Part::Word(29));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::ReadLength => {
            parts.push(Part::Word(30));
        }
        laws::Observation::WriteLength => {
            parts.push(Part::Word(31));
        }
        laws::Observation::EffectAttemptLength => {
            parts.push(Part::Word(32));
        }
        laws::Observation::ReadSource(a) => {
            parts.push(Part::Word(33));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::ReadId(a) => {
            parts.push(Part::Word(34));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::ReadPermitted(a) => {
            parts.push(Part::Word(35));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::WriteSource(a) => {
            parts.push(Part::Word(36));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::WriteId(a) => {
            parts.push(Part::Word(37));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::WritePermitted(a) => {
            parts.push(Part::Word(38));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::EffectAttemptSource(a) => {
            parts.push(Part::Word(39));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::EffectAttemptId(a) => {
            parts.push(Part::Word(40));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::EffectAttemptPermitted(a) => {
            parts.push(Part::Word(41));
            parts.push(Part::Word(a as u128));
        }
        laws::Observation::Usage(a) => {
            parts.push(Part::Word(42));
            parts.push(Part::Word(resource_tag(a)));
        }
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@==old(parts)@+model::diagnostic(diagnostic),))]
fn append_diagnostic<'a>(parts: &mut Vec<Part<'a>>, diagnostic: laws::Diagnostic) {
    parts.push(Part::Word(diagnostic.id as u128));
    match diagnostic.verdict {
        laws::Verdict::Skipped => parts.push(Part::Word(0)),
        laws::Verdict::Satisfied => parts.push(Part::Word(1)),
        laws::Verdict::Refused(f) => {
            parts.push(Part::Word(2));
            append_failure(parts, f);
        }
    }
}
#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@==old(parts)@+model::decision_attempt(attempt),))]
fn append_decision_attempt<'a>(parts: &mut Vec<Part<'a>>, attempt: decision::Attempt) {
    match attempt {
        decision::Attempt::Candidate(p) => {
            parts.push(Part::Word(0));
            parts.push(Part::Word(p as u128));
        }
        decision::Attempt::Write(id, p) => {
            parts.push(Part::Word(1));
            parts.push(Part::Word(id as u128));
            parts.push(Part::Word(p as u128));
        }
        decision::Attempt::Effect(outbox, id, p) => {
            parts.push(Part::Word(2));
            parts.push(Part::Word(outbox as u128));
            parts.push(Part::Word(id as u128));
            parts.push(Part::Word(p as u128));
        }
    }
}
/// Append all ordered law verdicts and their usage checkpoints.
#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@==old(parts)@+model::diagnostics(values@),))]
pub(super) fn append_diagnostics<'a>(parts: &mut Vec<Part<'a>>, values: &[laws::Diagnostic]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::diagnostics_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        append_diagnostic(parts, values[i]);
        i += 1;
    }
}
/// Append all ordered decision attempts, including refused work.
#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@==old(parts)@+model::decision_attempts(values@),))]
pub(super) fn append_decision_attempts<'a>(
    parts: &mut Vec<Part<'a>>,
    values: &[decision::Attempt],
) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::decision_attempts_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        append_decision_attempt(parts, values[i]);
        i += 1;
    }
}
/// Append all ordered law reads, including unsuccessful reads.
#[cfg_attr(verus_keep_ghost, verus_spec(ensures final(parts)@==old(parts)@+model::law_reads(values@),))]
pub(super) fn append_law_reads<'a>(parts: &mut Vec<Part<'a>>, values: &[laws::ReadAttempt]) {
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost initial=parts@;}
    parts.push(Part::Word(values.len() as u128));
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i<=values.len(),initial==old(parts)@,
parts@==initial+seq![Part::Word(values.len() as u128)]+model::law_reads_prefix(values@,i as nat),decreases values.len()-i,))]
    while i < values.len() {
        parts.push(Part::Word(values[i].law as u128));
        parts.push(Part::Word(values[i].node as u128));
        append_observation(parts, values[i].observation);
        parts.push(Part::Word(values[i].permitted as u128));
        i += 1;
    }
}
