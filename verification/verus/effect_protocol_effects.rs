//! Receiver effects and reservation retention with unrestricted transport copies.
//! Atomic receiver deduplication is an explicit environment contract, not a
//! theorem about an arbitrary provider or the production receiver.
use crate::model::*;
use vstd::prelude::*;

verus! {

pub open spec fn reservations_cover_copies(s: State, g: int) -> bool {
    guard_at(s.monitors, g)
    && forall|k: Key| #[trigger] count(s, k) > 0 ==>
        s.monitors[g].reserved.contains(k)
}

pub proof fn arbitrary_copies_keep_reservations(s: State, c: Invocation, g: int)
    requires reservations_cover_copies(s, g), sound(c.terminal),
    ensures reservations_cover_copies(advance(s, c), g),
{
    let q = c.request;
    let next = advance(s, c);
    admission_facts(s.monitors, q, g);
    assert(guard_at(next.monitors, g));
    if admitted(s.monitors, q) && q.send {
        assert(!s.monitors[g].reserved.contains(q.key));
        assert(count(s, q.key) == 0);
    }
    if rollback(c.terminal) { assert(c.terminal.copies == 0); }
    assert forall|k: Key| #[trigger] count(next, k) > 0
        implies next.monitors[g].reserved.contains(k) by {
        if !admitted(s.monitors, q) { }
        else if q.send && k == q.key {
            assert(count(next, k) == c.terminal.copies);
            assert(!rollback(c.terminal));
        } else {
            assert(count(next, k) == count(s, k));
            assert(s.monitors[g].reserved.contains(k));
        }
    }
}

pub open spec fn unrestricted_history(states: Seq<State>, calls: Seq<Invocation>) -> bool {
    states.len() == calls.len() + 1
    && forall|i: int| 0 <= i < calls.len() ==>
        states[i + 1] == advance(states[i], #[trigger] calls[i]) && sound(calls[i].terminal)
}

pub proof fn every_finite_history_keeps_reservations(states: Seq<State>, calls: Seq<Invocation>,
    g: int, n: nat)
    requires unrestricted_history(states, calls), n <= calls.len(),
        reservations_cover_copies(states[0], g),
    ensures reservations_cover_copies(states[n as int], g),
    decreases n,
{
    if n > 0 {
        every_finite_history_keeps_reservations(states, calls, g, (n - 1) as nat);
        arbitrary_copies_keep_reservations(states[n - 1], calls[n - 1], g);
    }
}

/// E3 has no bound on copies inside the suspended composed invocation.
pub proof fn suspended_retry_is_refused_without_copy_bound(s: State, c: Invocation,
    retry: Request, g: int)
    requires guard_at(s.monitors, g), admitted(s.monitors, c.request),
        c.request.send, c.terminal.outcome is Suspend, c.terminal.may_invoke,
        retry.send, retry.key == c.request.key,
    ensures advance(s, c).monitors[g].reserved.contains(retry.key),
        !admitted(advance(s, c).monitors, retry),
{
    admission_facts(s.monitors, c.request, g);
    let ms = advance(s, c).monitors;
    scan_facts(ms, retry, ms.len());
    if admitted(ms, retry) {
        assert(evidence(ms, retry)[g].verdict is Allow);
        assert(decide(ms[g], retry).verdict is Duplicate);
    }
}

/// The cache and effect commit are one atomic receiver transition.
pub struct Receiver {
    pub seen: Set<Key>,
    pub effects: Map<Key, nat>,
}

pub open spec fn effect_count(r: Receiver, k: Key) -> nat {
    if r.effects.dom().contains(k) { r.effects[k] } else { 0 }
}

pub open spec fn receiver_safe(r: Receiver) -> bool {
    forall|k: Key| #[trigger] effect_count(r, k) <= 1
        && (r.seen.contains(k) <==> effect_count(r, k) == 1)
}

pub open spec fn empty_receiver() -> Receiver {
    Receiver { seen: Set::empty(), effects: Map::empty() }
}

/// A copy with an already-seen key produces no further effect, for any payload.
pub open spec fn receive(r: Receiver, q: Request) -> Receiver {
    if !q.send || r.seen.contains(q.key) { r }
    else {
        Receiver { seen: r.seen.insert(q.key),
            effects: r.effects.insert(q.key, effect_count(r, q.key) + 1) }
    }
}

pub proof fn receiver_transition_preserves(r: Receiver, q: Request)
    requires receiver_safe(r),
    ensures receiver_safe(receive(r, q)),
{
    if q.send && !r.seen.contains(q.key) {
        assert(effect_count(r, q.key) == 0);
        assert forall|k: Key| #[trigger] effect_count(receive(r, q), k) <= 1
            && (receive(r, q).seen.contains(k) <==> effect_count(receive(r, q), k) == 1) by {
            if k == q.key { } else { assert(effect_count(receive(r, q), k) == effect_count(r, k)); }
        }
    }
}

/// An arbitrary finite transport history includes duplicates and reordering.
pub open spec fn receiver_run(r: Receiver, wire: Seq<Request>) -> Receiver
    decreases wire.len(),
{
    if wire.len() == 0 { r }
    else { receive(receiver_run(r, wire.drop_last()), wire.last()) }
}

pub proof fn receiver_history_preserves(r: Receiver, wire: Seq<Request>)
    requires receiver_safe(r),
    ensures receiver_safe(receiver_run(r, wire)),
    decreases wire.len(),
{
    if wire.len() > 0 {
        receiver_history_preserves(r, wire.drop_last());
        receiver_transition_preserves(receiver_run(r, wire.drop_last()), wire.last());
    }
}

/// E1: at most one effect per key, hence per key/payload, at every finite prefix.
/// This bound needs atomic receiver deduplication and no transport-copy bound.
pub proof fn e1_at_most_one_effect(wire: Seq<Request>, n: nat, k: Key)
    requires n <= wire.len(),
    ensures effect_count(receiver_run(empty_receiver(), wire.take(n as int)), k) <= 1,
{
    assert(receiver_safe(empty_receiver()));
    receiver_history_preserves(empty_receiver(), wire.take(n as int));
}

/// E1 and reservation coverage hold together for every finite composed history.
/// No coupling premise is needed for this upper bound: E1 allows every wire
/// history, even deliveries not generated by the shell. Origin authenticity and
/// exactly-once delivery are separate claims.
pub proof fn composed_protocol_effect_safety(states: Seq<State>, calls: Seq<Invocation>,
    g: int, n: nat, wire: Seq<Request>, k: Key)
    requires unrestricted_history(states, calls), n <= calls.len(),
        reservations_cover_copies(states[0], g),
    ensures reservations_cover_copies(states[n as int], g),
        effect_count(receiver_run(empty_receiver(), wire), k) <= 1,
{
    every_finite_history_keeps_reservations(states, calls, g, n);
    e1_at_most_one_effect(wire, wire.len(), k);
    assert(wire.take(wire.len() as int) =~= wire);
}

/// V1: the same two-handler stack admits two transport copies and one effect;
/// a composed Suspend retains its reservation and refuses a subsequent retry.
pub proof fn duplicated_transport_is_not_vacuous()
    ensures ({
        let q = crate::examples::request(1, 7, false);
        let r = Report { outcome: Outcome::Suspend, may_invoke: true, calls: 2, copies: 2 };
        let s0 = initial(crate::examples::stack());
        let c = Invocation { request: q, terminal: r };
        let s1 = advance(s0, c);
        unrestricted_history(seq![s0, s1], seq![c])
            && reservations_cover_copies(s0, 0) && reservations_cover_copies(s1, 0)
            && count(s1, q.key) == 2
            && effect_count(receiver_run(empty_receiver(), seq![q, q]), q.key) == 1
            && !admitted(s1.monitors, crate::examples::request(1, 7, true))
    }),
{
    reveal_with_fuel(scan, 3);
    reveal_with_fuel(receiver_run, 3);
    let q = crate::examples::request(1, 7, false);
    let s0 = initial(crate::examples::stack());
    let c = Invocation { request: q,
        terminal: Report { outcome: Outcome::Suspend, may_invoke: true, calls: 2, copies: 2 } };
    arbitrary_copies_keep_reservations(s0, c, 0);
    suspended_retry_is_refused_without_copy_bound(s0, c, crate::examples::request(1, 7, true), 0);
    assert(seq![q, q].drop_last() =~= seq![q]);
    assert(seq![q].drop_last() =~= Seq::<Request>::empty());
}

/// V4: without atomic deduplication a stale second receiver admission can commit
/// a second effect before either owner publishes the shared seen-key entry.
pub proof fn non_atomic_dedup_counterexample()
    ensures ({
        let q = crate::examples::request(1, 7, false);
        let r0 = empty_receiver();
        let r1 = receive(r0, q);
        let r2 = Receiver { seen: r1.seen,
            effects: r1.effects.insert(q.key, effect_count(r1, q.key) + 1) };
        !r0.seen.contains(q.key) && effect_count(r2, q.key) == 2 && !receiver_safe(r2)
    }),
{
}

/// V4: losing the receiver's dedup cache while retaining its effect also fails E1.
pub proof fn receiver_reset_counterexample()
    ensures ({
        let q = crate::examples::request(1, 7, false);
        let r1 = receive(empty_receiver(), q);
        let reset = Receiver { seen: Set::empty(), effects: r1.effects };
        effect_count(receive(reset, q), q.key) == 2
    }),
{
}

/// V2 in effect terminology: deduplication preserves E1 even for the known-bad
/// reservation trace; that trace is rejected by E2's reservation coverage.
pub proof fn historical_witness_violates_e2_with_one_receiver_effect()
    ensures ({
        let q = crate::examples::request(1, 7, false);
        let bad = crate::examples::broken_advance(initial(crate::examples::stack()),
            crate::examples::call(q, crate::examples::composed_witness()));
        !reservations_cover_copies(bad, 0)
            && effect_count(receiver_run(empty_receiver(), seq![q, q]), q.key) == 1
    }),
{
    crate::examples::historical_composed_suspend_violates_e1_e2();
    reveal_with_fuel(receiver_run, 3);
    let q = crate::examples::request(1, 7, false);
    assert(seq![q, q].drop_last() =~= seq![q]);
    assert(seq![q].drop_last() =~= Seq::<Request>::empty());
}

} // verus!
