//! Abstract, finite-history effect reservation protocol.
//! This model is independent of production code. Refinement is a separate gate.
use vstd::prelude::*;

verus! {

pub struct Key {
    pub store: int,
    pub delivery: int,
}

pub struct Request {
    pub send: bool,
    pub key: Key,
    pub payload: Seq<u8>,
    pub destination: Seq<u8>,
    pub retry: bool,
}

pub enum Policy {
    AtMostOne { capacity: nat },
    Allowlist { recipients: Set<Seq<u8>> },
}

pub struct Monitor {
    pub policy: Policy,
    pub reserved: Seq<Key>,
}

pub enum Verdict { Allow, Duplicate, Capacity, Recipient }

pub struct Decision {
    pub policy: Policy,
    pub before: Seq<Key>,
    pub proposed: Seq<Key>,
    pub verdict: Verdict,
}

pub open spec fn decide(m: Monitor, q: Request) -> Decision {
    let verdict = if !q.send {
        Verdict::Allow
    } else {
        match m.policy {
            Policy::AtMostOne { capacity } => {
                if m.reserved.contains(q.key) { Verdict::Duplicate }
                else if m.reserved.len() >= capacity { Verdict::Capacity }
                else { Verdict::Allow }
            },
            Policy::Allowlist { recipients } => {
                if recipients.contains(q.destination) { Verdict::Allow }
                else { Verdict::Recipient }
            },
        }
    };
    Decision {
        policy: m.policy, before: m.reserved, verdict,
        proposed: if q.send && m.policy is AtMostOne && verdict is Allow {
            m.reserved.push(q.key)
        } else { m.reserved },
    }
}

pub open spec fn all_allow(ds: Seq<Decision>) -> bool {
    forall|i: int| 0 <= i < ds.len() ==> (#[trigger] ds[i]).verdict is Allow
}

/// Visit the first n handlers, stopping at the first veto.
pub open spec fn scan(ms: Seq<Monitor>, q: Request, n: nat) -> Seq<Decision>
    recommends n <= ms.len(),
    decreases n,
{
    if n == 0 { Seq::empty() }
    else {
        let previous = scan(ms, q, (n - 1) as nat);
        if all_allow(previous) { previous.push(decide(ms[n - 1], q)) }
        else { previous }
    }
}

pub open spec fn evidence(ms: Seq<Monitor>, q: Request) -> Seq<Decision> {
    scan(ms, q, ms.len())
}

pub open spec fn admitted(ms: Seq<Monitor>, q: Request) -> bool {
    all_allow(evidence(ms, q))
}

/// Candidate states are installed together only after every handler allows.
pub open spec fn prepared(ms: Seq<Monitor>, q: Request) -> Seq<Monitor> {
    if admitted(ms, q) {
        Seq::new(ms.len(), |i: int| Monitor {
            policy: ms[i].policy, reserved: evidence(ms, q)[i].proposed,
        })
    } else { ms }
}

pub enum Outcome { Forward, Suspend, Refuse, Fault, Unwind }

/// Actual calls/copies are ghost observations, independent of reported evidence.
pub struct Report {
    pub outcome: Outcome,
    pub may_invoke: bool,
    pub calls: nat,
    pub copies: nat,
}

pub open spec fn sound(r: Report) -> bool {
    (!r.may_invoke ==> r.calls == 0) && (r.calls == 0 ==> r.copies == 0)
}

pub open spec fn merge(a: Report, b: Report) -> Report {
    Report {
        outcome: b.outcome,
        may_invoke: a.may_invoke || b.may_invoke,
        calls: a.calls + b.calls,
        copies: a.copies + b.copies,
    }
}

pub open spec fn empty_report() -> Report {
    Report { outcome: Outcome::Suspend, may_invoke: false, calls: 0, copies: 0 }
}

/// A depth-first list of leaves represents any finite nested handler call.
pub open spec fn compose(rs: Seq<Report>) -> Report
    decreases rs.len(),
{
    if rs.len() == 0 { empty_report() }
    else { merge(compose(rs.drop_last()), rs.last()) }
}

pub open spec fn reports_sound(rs: Seq<Report>) -> bool {
    forall|i: int| 0 <= i < rs.len() ==> sound(#[trigger] rs[i])
}

pub proof fn merge_is_sound(a: Report, b: Report)
    requires sound(a), sound(b),
    ensures sound(merge(a, b)),
        a.may_invoke ==> merge(a, b).may_invoke,
        b.may_invoke ==> merge(a, b).may_invoke,
{
}

pub proof fn composed_non_invocation(rs: Seq<Report>)
    requires reports_sound(rs),
    ensures sound(compose(rs)),
        !compose(rs).may_invoke ==> compose(rs).calls == 0 && compose(rs).copies == 0,
    decreases rs.len(),
{
    if rs.len() > 0 {
        assert(reports_sound(rs.drop_last()));
        composed_non_invocation(rs.drop_last());
        merge_is_sound(compose(rs.drop_last()), rs.last());
    }
}

pub proof fn composition_is_associative(a: Report, b: Report, c: Report)
    ensures merge(merge(a, b), c) == merge(a, merge(b, c)),
{
}

pub proof fn composed_evidence_covers_every_leaf(rs: Seq<Report>, at: int)
    requires 0 <= at < rs.len(), rs[at].may_invoke,
    ensures compose(rs).may_invoke,
    decreases rs.len(),
{
    if at < rs.len() - 1 {
        composed_evidence_covers_every_leaf(rs.drop_last(), at);
    }
}

pub struct State {
    pub monitors: Seq<Monitor>,
    pub sent: Map<Key, nat>,
}

pub struct Invocation {
    pub request: Request,
    pub terminal: Report,
}

pub open spec fn count(s: State, key: Key) -> nat {
    if s.sent.dom().contains(key) { s.sent[key] } else { 0 }
}

pub open spec fn rollback(r: Report) -> bool {
    r.outcome is Suspend && !r.may_invoke
}

pub open spec fn advance(s: State, call: Invocation) -> State {
    let q = call.request;
    if !admitted(s.monitors, q) { s }
    else {
        State {
            monitors: if rollback(call.terminal) { s.monitors }
                else { prepared(s.monitors, q) },
            sent: if q.send {
                s.sent.insert(q.key, count(s, q.key) + call.terminal.copies)
            } else { s.sent },
        }
    }
}

pub open spec fn initial(ms: Seq<Monitor>) -> State {
    State { monitors: ms, sent: Map::empty() }
}

pub open spec fn guard_at(ms: Seq<Monitor>, g: int) -> bool {
    0 <= g < ms.len() && ms[g].policy is AtMostOne
}

/// Every counted copy has a retained reservation; each key has at most one copy.
pub open spec fn safe(s: State, g: int) -> bool {
    guard_at(s.monitors, g)
    && forall|k: Key| #[trigger] count(s, k) <= 1
        && (count(s, k) > 0 ==> s.monitors[g].reserved.contains(k))
}

pub open spec fn qualified(call: Invocation) -> bool {
    sound(call.terminal) && (call.request.send ==> call.terminal.copies <= 1)
}

pub proof fn scan_facts(ms: Seq<Monitor>, q: Request, n: nat)
    requires n <= ms.len(),
    ensures scan(ms, q, n).len() <= n,
        all_allow(scan(ms, q, n)) ==> scan(ms, q, n).len() == n,
        forall|i: int| 0 <= i < scan(ms, q, n).len() ==>
            #[trigger] scan(ms, q, n)[i] == decide(ms[i], q),
    decreases n,
{
    if n > 0 {
        scan_facts(ms, q, (n - 1) as nat);
        let previous = scan(ms, q, (n - 1) as nat);
        if all_allow(previous) {
            assert forall|i: int| 0 <= i < previous.push(decide(ms[n - 1], q)).len()
                implies #[trigger] previous.push(decide(ms[n - 1], q))[i] == decide(ms[i], q) by {
                if i < previous.len() { assert(previous.push(decide(ms[n - 1], q))[i] == previous[i]); }
            }
        }
    }
}

pub proof fn admission_facts(ms: Seq<Monitor>, q: Request, g: int)
    requires guard_at(ms, g),
    ensures guard_at(prepared(ms, q), g),
        prepared(ms, q)[g].policy == ms[g].policy,
        forall|k: Key| ms[g].reserved.contains(k) ==>
            #[trigger] prepared(ms, q)[g].reserved.contains(k),
        admitted(ms, q) && q.send ==> !ms[g].reserved.contains(q.key)
            && prepared(ms, q)[g].reserved.contains(q.key),
{
    scan_facts(ms, q, ms.len());
    if admitted(ms, q) {
        assert(evidence(ms, q)[g] == decide(ms[g], q));
        if q.send {
            assert(decide(ms[g], q).verdict is Allow);
            assert(prepared(ms, q)[g].reserved == ms[g].reserved.push(q.key));
            assert(ms[g].reserved.push(q.key)[ms[g].reserved.len() as int] == q.key);
            assert(prepared(ms, q)[g].reserved.contains(q.key));
        }
        assert forall|k: Key| ms[g].reserved.contains(k)
            implies #[trigger] prepared(ms, q)[g].reserved.contains(k) by {
            let i = choose|i: int| 0 <= i < ms[g].reserved.len() && ms[g].reserved[i] == k;
            assert(prepared(ms, q)[g].reserved[i] == k);
        }
    }
}

pub proof fn first_veto_restores_every_handler(ms: Seq<Monitor>, q: Request)
    requires !admitted(ms, q),
    ensures prepared(ms, q) == ms,
{
}

pub proof fn release_requires_non_invocation(s: State, call: Invocation, g: int, key: Key)
    requires guard_at(s.monitors, g), sound(call.terminal),
        prepared(s.monitors, call.request)[g].reserved.contains(key),
        !advance(s, call).monitors[g].reserved.contains(key),
    ensures admitted(s.monitors, call.request), rollback(call.terminal),
        !call.terminal.may_invoke, call.terminal.calls == 0, call.terminal.copies == 0,
{
}

pub proof fn invocation_preserves(s: State, call: Invocation, g: int)
    requires safe(s, g), qualified(call),
    ensures safe(advance(s, call), g),
{
    let q = call.request;
    let next = advance(s, call);
    admission_facts(s.monitors, q, g);
    assert(guard_at(next.monitors, g));
    if admitted(s.monitors, q) && q.send {
        assert(!s.monitors[g].reserved.contains(q.key));
        assert(count(s, q.key) == 0);
    }
    if rollback(call.terminal) { assert(call.terminal.copies == 0); }
    assert forall|k: Key| #[trigger] count(next, k) <= 1
        && (count(next, k) > 0 ==> next.monitors[g].reserved.contains(k)) by {
        assert(count(s, k) <= 1);
        if !admitted(s.monitors, q) { }
        else if q.send && k == q.key {
            assert(count(next, k) == count(s, k) + call.terminal.copies);
        } else {
            assert(count(next, k) == count(s, k));
        }
    }
}

pub open spec fn history(states: Seq<State>, calls: Seq<Invocation>) -> bool {
    states.len() == calls.len() + 1
    && forall|i: int| 0 <= i < calls.len() ==>
        states[i + 1] == advance(states[i], #[trigger] calls[i]) && qualified(calls[i])
}

/// E1 and reservation retention for every prefix of every finite call history.
pub proof fn every_finite_prefix(states: Seq<State>, calls: Seq<Invocation>, g: int, n: nat)
    requires history(states, calls), n <= calls.len(), safe(states[0], g),
    ensures safe(states[n as int], g),
    decreases n,
{
    if n > 0 {
        every_finite_prefix(states, calls, g, (n - 1) as nat);
        assert(states[n as int] == advance(states[n - 1], calls[n - 1]));
        assert(qualified(calls[n - 1]));
        invocation_preserves(states[n - 1], calls[n - 1], g);
    }
}

pub proof fn genesis_is_safe(ms: Seq<Monitor>, g: int)
    requires guard_at(ms, g),
    ensures safe(initial(ms), g),
{
}

/// E1 is stronger than per-(key,payload) uniqueness: it bounds the whole key.
pub proof fn e1_at_most_one_copy(states: Seq<State>, calls: Seq<Invocation>, g: int,
    n: nat, k: Key)
    requires history(states, calls), n <= calls.len(), safe(states[0], g),
    ensures count(states[n as int], k) <= 1,
{
    every_finite_prefix(states, calls, g, n);
}

/// Non-invoking Suspend may release a tentative reservation without losing work.
pub proof fn certified_suspend_preserves_admission(s: State, c: Invocation, retry: Request, k: Key)
    requires sound(c.terminal), c.terminal.outcome is Suspend, !c.terminal.may_invoke,
    ensures advance(s, c).monitors == s.monitors,
        count(advance(s, c), k) == count(s, k),
        admitted(advance(s, c).monitors, retry) == admitted(s.monitors, retry),
{
}

/// Fault/Unwind never undo the reservation installed before terminal entry.
pub proof fn faults_retain_prepared_state(s: State, c: Invocation)
    requires c.terminal.outcome is Fault || c.terminal.outcome is Unwind,
    ensures advance(s, c).monitors == prepared(s.monitors, c.request),
{
}

/// E3: possible invocation on Suspend persists and the same-key retry vetoes.
pub proof fn suspend_then_retry(s: State, a: Invocation, retry: Invocation, g: int)
    requires safe(s, g), qualified(a),
        a.request.send, a.terminal.outcome is Suspend, a.terminal.may_invoke,
        admitted(s.monitors, a.request),
        retry.request.send, retry.request.key == a.request.key,
    ensures advance(s, a).monitors[g].reserved.contains(a.request.key),
        !admitted(advance(s, a).monitors, retry.request),
        advance(advance(s, a), retry) == advance(s, a),
{
    admission_facts(s.monitors, a.request, g);
    scan_facts(advance(s, a).monitors, retry.request, s.monitors.len());
    if admitted(advance(s, a).monitors, retry.request) {
        assert(evidence(advance(s, a).monitors, retry.request)[g].verdict is Allow);
        assert(decide(advance(s, a).monitors[g], retry.request).verdict is Duplicate);
    }
}

} // verus!
