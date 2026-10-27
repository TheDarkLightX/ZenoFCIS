//! Constructive validation and premise-removal counterexamples, not assumptions.
use crate::model::*;
use crate::replay::*;
use vstd::prelude::*;

verus! {

pub open spec fn key(n: int) -> Key { Key { store: 10, delivery: n } }

pub open spec fn request(n: int, recipient: u8, retry: bool) -> Request {
    Request { send: true, key: key(n), payload: seq![11u8],
        destination: seq![recipient], retry }
}

pub open spec fn stack() -> Seq<Monitor> {
    seq![Monitor { policy: Policy::AtMostOne { capacity: 2 }, reserved: seq![] },
        Monitor { policy: Policy::Allowlist { recipients: Set::empty().insert(seq![7u8]) },
            reserved: seq![] }]
}

pub open spec fn forwarded() -> Report {
    Report { outcome: Outcome::Forward, may_invoke: true, calls: 1, copies: 1 }
}

pub open spec fn call(q: Request, r: Report) -> Invocation {
    Invocation { request: q, terminal: r }
}

pub open spec fn concrete_calls() -> Seq<Invocation> {
    seq![call(request(1, 7, false), empty_report()),
        call(request(1, 7, true), forwarded()),
        call(request(2, 8, false), empty_report())]
}

pub open spec fn concrete_states() -> Seq<State> {
    let s0 = initial(stack());
    let s1 = advance(s0, concrete_calls()[0]);
    let s2 = advance(s1, concrete_calls()[1]);
    let s3 = advance(s2, concrete_calls()[2]);
    seq![s0, s1, s2, s3]
}

/// A concrete history satisfies the exact universal theorem's premises.
pub proof fn finite_history_is_not_vacuous()
    ensures history(concrete_states(), concrete_calls()), safe(concrete_states()[0], 0),
        safe(concrete_states()[3], 0), count(concrete_states()[3], key(1)) == 1,
        concrete_states()[3].monitors == concrete_states()[2].monitors,
{
    reveal_with_fuel(scan, 3);
    assert forall|i: int| 0 <= i < concrete_calls().len() implies
        concrete_states()[i + 1] == advance(concrete_states()[i], #[trigger] concrete_calls()[i])
        && qualified(concrete_calls()[i]) by {
        if i == 0 { } else if i == 1 { } else { assert(i == 2); }
    }
    genesis_is_safe(stack(), 0);
    every_finite_prefix(concrete_states(), concrete_calls(), 0, 3);
    assert(seq![7u8][0] == 7u8 && seq![8u8][0] == 8u8);
    assert(seq![7u8] != seq![8u8]);
}

/// V1: one two-handler stack meets all premises, forwards, releases safely,
/// retries successfully, and rolls back an earlier proposal on a later veto.
pub proof fn constructive_two_handler_trace()
    ensures guard_at(stack(), 0),
        qualified(call(request(1, 7, false), empty_report())),
        qualified(call(request(1, 7, true), forwarded())),
        admitted(stack(), request(1, 7, false)),
        advance(initial(stack()), call(request(1, 7, false), empty_report())).monitors == stack(),
        count(advance(advance(initial(stack()), call(request(1, 7, false), empty_report())),
            call(request(1, 7, true), forwarded())), key(1)) == 1,
        !admitted(stack(), request(2, 8, false)),
        evidence(stack(), request(2, 8, false)).len() == 2,
        evidence(stack(), request(2, 8, false))[0].verdict is Allow,
        evidence(stack(), request(2, 8, false))[0].proposed.contains(key(2)),
        evidence(stack(), request(2, 8, false))[1].verdict is Recipient,
        prepared(stack(), request(2, 8, false)) == stack(),
{
    reveal_with_fuel(scan, 3);
    assert(seq![key(2)][0] == key(2));
    assert(seq![key(2)].contains(key(2)));
    assert(seq![7u8][0] == 7u8 && seq![8u8][0] == 8u8);
    assert(seq![7u8] != seq![8u8]);
    assert(evidence(stack(), request(2, 8, false))[1].verdict is Recipient);
}

/// The historical rule erased reservations on every Suspend, ignoring evidence.
pub open spec fn broken_advance(s: State, c: Invocation) -> State {
    let actual = advance(s, c);
    State { monitors: if admitted(s.monitors, c.request) && c.terminal.outcome is Suspend {
            s.monitors
        } else { actual.monitors }, sent: actual.sent }
}

pub open spec fn composed_witness() -> Report {
    compose(seq![forwarded(), empty_report()])
}

/// V2: outer [6], inner [0] then absent: copy, later Suspend, retry, two copies.
pub proof fn historical_composed_suspend_violates_e1_e2()
    ensures sound(composed_witness()), composed_witness().may_invoke,
        composed_witness().outcome is Suspend, composed_witness().copies == 1,
        qualified(call(request(1, 7, false), composed_witness())),
        count(broken_advance(initial(stack()), call(request(1, 7, false), composed_witness())), key(1)) == 1,
        !broken_advance(initial(stack()), call(request(1, 7, false), composed_witness()))
            .monitors[0].reserved.contains(key(1)),
        count(broken_advance(broken_advance(initial(stack()),
            call(request(1, 7, false), composed_witness())),
            call(request(1, 7, true), forwarded())), key(1)) == 2,
        !admitted(advance(initial(stack()), call(request(1, 7, false), composed_witness())).monitors,
            request(1, 7, true)),
{
    reveal_with_fuel(scan, 3);
    reveal_with_fuel(compose, 3);
    assert(seq![forwarded(), empty_report()].drop_last() =~= seq![forwarded()]);
    assert(seq![forwarded()].drop_last() =~= Seq::<Report>::empty());
    assert(seq![key(1)][0] == key(1));
    assert(seq![key(1)].contains(key(1)));
    genesis_is_safe(stack(), 0);
    suspend_then_retry(initial(stack()), call(request(1, 7, false), composed_witness()),
        call(request(1, 7, true), forwarded()), 0);
}

/// P1a: omit the guard (including bypassing it on retry), and a second copy passes.
pub proof fn missing_guard_counterexample()
    ensures count(advance(advance(initial(seq![]), call(request(1, 7, false), forwarded())),
        call(request(1, 7, true), forwarded())), key(1)) == 2,
{
}

/// P1b: resetting the relay's monitor while retaining real effects loses safety.
pub proof fn lifetime_reset_counterexample()
    ensures ({
        let first = advance(initial(stack()), call(request(1, 7, false), forwarded()));
        let reset = State { monitors: stack(), sent: first.sent };
        count(advance(reset, call(request(1, 7, true), forwarded())), key(1)) == 2
    }),
{
    reveal_with_fuel(scan, 3);
}

/// P1c: two concurrent owners may both admit from the same unreserved snapshot.
pub proof fn non_serial_counterexample()
    ensures admitted(stack(), request(1, 7, false)),
        count(advance(initial(stack()), call(request(1, 7, false), forwarded())), key(1))
        + count(advance(initial(stack()), call(request(1, 7, true), forwarded())), key(1)) == 2,
{
    reveal_with_fuel(scan, 3);
}

/// P2a: a false NotInvoked report after a real copy permits release and a retry.
pub proof fn dishonest_evidence_counterexample()
    ensures ({
        let lie = Report { outcome: Outcome::Suspend, may_invoke: false, calls: 1, copies: 1 };
        let first = advance(initial(stack()), call(request(1, 7, false), lie));
        !sound(lie) && first.monitors == stack()
            && count(advance(first, call(request(1, 7, true), forwarded())), key(1)) == 2
    }),
{
    reveal_with_fuel(scan, 3);
}

/// P2b: "no invocation, but one copy" also invalidates non-invocation evidence.
pub proof fn spontaneous_copy_counterexample()
    ensures ({
        let lie = Report { outcome: Outcome::Suspend, may_invoke: false, calls: 0, copies: 1 };
        let first = advance(initial(stack()), call(request(1, 7, false), lie));
        !sound(lie) && first.monitors == stack()
            && count(advance(first, call(request(1, 7, true), forwarded())), key(1)) == 2
    }),
{
    reveal_with_fuel(scan, 3);
}

/// P3: the reservation protocol does not stop two copies inside one terminal call.
pub proof fn duplicate_transport_counterexample()
    ensures ({
        let duplicate = Report { outcome: Outcome::Forward, may_invoke: true, calls: 1, copies: 2 };
        sound(duplicate) && count(advance(initial(stack()), call(request(1, 7, false), duplicate)), key(1)) == 2
    }),
{
    reveal_with_fuel(scan, 3);
}

/// Non-Send effects cannot create Send copies: the ghost counter tracks only Send.
/// This condition is not needed by E1/E2 and is therefore absent as a premise.
pub proof fn non_send_is_framed(s: State, q: Request, r: Report, k: Key)
    requires !q.send,
    ensures count(advance(s, call(q, r)), k) == count(s, k),
{
}

/// E5 has a positive completed transcript and a concrete altered check refusal.
pub proof fn constructive_replay_and_tampering()
    ensures ({
        let q = request(1, 7, false);
        let c = call(q, forwarded());
        let good = recorded(stack(), c);
        let bad = Record { request: q, check: seq![], outcome: Outcome::Forward };
        replay_one(stack(), q, good) == Some(advance(initial(stack()), c).monitors)
            && replay_one(stack(), q, bad) is None
    }),
{
    reveal_with_fuel(scan, 3);
    constructive_two_handler_trace();
    let q = request(1, 7, false);
    let c = call(q, forwarded());
    recorded_step_agrees(initial(stack()), c);
    scan_facts(stack(), q, 2);
    assert(evidence(stack(), q).len() == 2);
    altered_admission_refuses(stack(), q,
        Record { request: q, check: seq![], outcome: Outcome::Forward });
}

} // verus!
