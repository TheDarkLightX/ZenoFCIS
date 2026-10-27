//! Exact recomputation of admission evidence under fixed inputs and manifest.
use crate::model::*;
use vstd::prelude::*;

verus! {

pub struct Record {
    pub request: Request,
    pub check: Seq<Decision>,
    pub outcome: Outcome,
}

pub open spec fn recordable(ms: Seq<Monitor>, c: Invocation) -> bool {
    !admitted(ms, c.request)
        || (!(c.terminal.outcome is Suspend) && !(c.terminal.outcome is Unwind))
}

pub open spec fn recorded(ms: Seq<Monitor>, c: Invocation) -> Record {
    Record { request: c.request, check: evidence(ms, c.request),
        outcome: if admitted(ms, c.request) { c.terminal.outcome } else { Outcome::Refuse } }
}

pub open spec fn replay_one(ms: Seq<Monitor>, q: Request, r: Record) -> Option<Seq<Monitor>> {
    if r.request != q || r.check != evidence(ms, q)
        || r.outcome is Suspend || r.outcome is Unwind
        || (!admitted(ms, q) && !(r.outcome is Refuse)) {
        None
    } else { Some(prepared(ms, q)) }
}

pub open spec fn replay_n(ms: Seq<Monitor>, qs: Seq<Request>, rs: Seq<Record>, n: nat)
    -> Option<Seq<Monitor>>
    recommends n <= qs.len(), n <= rs.len(),
    decreases n,
{
    if n == 0 { Some(ms) }
    else {
        match replay_n(ms, qs, rs, (n - 1) as nat) {
            None => None,
            Some(before) => replay_one(before, qs[n - 1], rs[n - 1]),
        }
    }
}

pub open spec fn replay_all(ms: Seq<Monitor>, qs: Seq<Request>, rs: Seq<Record>)
    -> Option<Seq<Monitor>> {
    if qs.len() != rs.len() { None } else { replay_n(ms, qs, rs, qs.len()) }
}

pub open spec fn requests(cs: Seq<Invocation>) -> Seq<Request> {
    Seq::new(cs.len(), |i: int| cs[i].request)
}

pub open spec fn records(states: Seq<State>, cs: Seq<Invocation>) -> Seq<Record> {
    Seq::new(cs.len(), |i: int| recorded(states[i].monitors, cs[i]))
}

pub proof fn recorded_step_agrees(s: State, c: Invocation)
    requires recordable(s.monitors, c),
    ensures replay_one(s.monitors, c.request, recorded(s.monitors, c))
        == Some(advance(s, c).monitors),
{
}

/// E5a: the same immutable initial monitors and requests give the same admissions.
pub proof fn replay_recomputes(states: Seq<State>, cs: Seq<Invocation>, n: nat)
    requires history(states, cs), n <= cs.len(),
        forall|i: int| 0 <= i < cs.len() ==> recordable(states[i].monitors, #[trigger] cs[i]),
    ensures replay_n(states[0].monitors, requests(cs), records(states, cs), n)
        == Some(states[n as int].monitors),
    decreases n,
{
    if n > 0 {
        replay_recomputes(states, cs, (n - 1) as nat);
        assert(states[n as int] == advance(states[n - 1], cs[n - 1]));
        recorded_step_agrees(states[n - 1], cs[n - 1]);
    }
}

/// E5b: changing any admission field refuses even with a newly valid hash chain.
pub proof fn altered_admission_refuses(ms: Seq<Monitor>, q: Request, r: Record)
    requires r.check != evidence(ms, q),
    ensures replay_one(ms, q, r) is None,
{
}

pub proof fn altered_request_refuses(ms: Seq<Monitor>, q: Request, r: Record)
    requires r.request != q,
    ensures replay_one(ms, q, r) is None,
{
}

pub proof fn refusal_has_no_live_fallback(ms: Seq<Monitor>, qs: Seq<Request>, rs: Seq<Record>,
    at: nat, end: nat)
    requires at <= end, end <= qs.len(), end <= rs.len(),
        replay_n(ms, qs, rs, at) is None,
    ensures replay_n(ms, qs, rs, end) is None,
    decreases end - at,
{
    if at < end { refusal_has_no_live_fallback(ms, qs, rs, at, (end - 1) as nat); }
}

pub proof fn changed_evidence_in_a_run_refuses(ms: Seq<Monitor>, qs: Seq<Request>, rs: Seq<Record>,
    at: nat, before: Seq<Monitor>)
    requires at < qs.len(), qs.len() == rs.len(),
        replay_n(ms, qs, rs, at) == Some(before), rs[at as int].check != evidence(before, qs[at as int]),
    ensures replay_all(ms, qs, rs) is None,
{
    altered_admission_refuses(before, qs[at as int], rs[at as int]);
    refusal_has_no_live_fallback(ms, qs, rs, at + 1, qs.len());
}

} // verus!
