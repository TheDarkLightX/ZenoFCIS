//! Conditional finite-history theorem for composed-domain components.
//! Table construction, parsing, and the implementation-to-model bridge are
//! premises; the theorem does not grant runtime or publication authority.
use crate::resolve::{End, Local, Resolution, model::*};
use crate::coupling_cuts::{exact_interval, group_start, ordered_members};
use vstd::prelude::*;

verus! {
#[verifier::reject_recursive_types(S)]
#[verifier::reject_recursive_types(I)]
#[verifier::reject_recursive_types(R)]
pub(crate) struct Model<S, I, R> {
    pub n: nat,
    pub owns: Seq<bool>,
    pub providers: Seq<Seq<int>>,
    pub genesis: Seq<S>,
    pub genesis_law: spec_fn(int, S) -> bool,
    pub domain: spec_fn(int, S) -> bool,
    pub input_domain: spec_fn(int, I) -> bool,
    pub request_domain: spec_fn(R) -> bool,
    pub invariant: spec_fn(int, S) -> bool,
    pub assumption: spec_fn(int, I) -> bool,
    pub participation: spec_fn(int, Seq<S>, R) -> bool,
    pub state_binding: spec_fn(int, Seq<S>) -> Option<S>,
    pub input_binding: spec_fn(int, Seq<S>, R) -> Option<I>,
    pub decision: spec_fn(int, S, I) -> (Local, S),
    pub failure_case: Seq<bool>,
    pub members: Seq<Set<int>>,
    pub coupling: spec_fn(int, Seq<S>) -> bool,
}

pub(crate) open spec fn well_formed<S, I, R>(m: Model<S, I, R>) -> bool {
    m.n <= usize::MAX && m.owns.len() == m.n && m.providers.len() == m.n
    && m.genesis.len() == m.n && m.failure_case.len() == m.n
    && (forall|i: int, j: int| 0 <= i < m.n && 0 <= j < m.providers[i].len() ==>
        0 <= #[trigger] m.providers[i][j] < i && m.owns[m.providers[i][j]])
    && (forall|k: int, i: int| 0 <= k < m.members.len() && #[trigger] m.members[k].contains(i)
        ==> 0 <= i < m.n && m.owns[i])
    && (forall|k: int, a: Seq<S>, b: Seq<S>| #![trigger (m.coupling)(k, a), (m.coupling)(k, b)] 0 <= k < m.members.len()
        && a.len() == m.n && b.len() == m.n
        && (forall|i: int| m.members[k].contains(i) ==> #[trigger] a[i] == b[i])
        ==> (m.coupling)(k, a) == (m.coupling)(k, b))
}

pub(crate) open spec fn observations<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, i: int) -> Seq<S> {
    Seq::new(m.providers[i].len(), |j: int| pre[m.providers[i][j]])
}

pub(crate) open spec fn participates<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, r: R, i: int) -> bool {
    (m.participation)(i, observations(m, pre, i), r)
}

pub(crate) open spec fn state<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, i: int) -> S {
    if m.owns[i] { pre[i] } else { (m.state_binding)(i, observations(m, pre, i)).unwrap() }
}

pub(crate) open spec fn input<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, r: R, i: int) -> Option<I> {
    (m.input_binding)(i, observations(m, pre, i), r)
}

pub(crate) open spec fn global_invariant<S, I, R>(m: Model<S, I, R>, pre: Seq<S>) -> bool {
    pre.len() == m.n && forall|i: int| 0 <= i < m.n && m.owns[i] ==>
        #[trigger] (m.domain)(i, pre[i]) && (m.invariant)(i, pre[i])
}

pub(crate) open spec fn local_obligation<S, I, R>(m: Model<S, I, R>) -> bool {
    global_invariant(m, m.genesis)
    && (forall|i: int| 0 <= i < m.n && m.owns[i] ==> #[trigger] (m.genesis_law)(i, m.genesis[i]))
    && (forall|i: int, s: S, c: I| 0 <= i < m.n && (m.domain)(i, s)
        && (m.invariant)(i, s) && (m.input_domain)(i, c) && (m.assumption)(i, c) ==>
        match #[trigger] (m.decision)(i, s, c) {
            (Local::Commit, post) | (Local::CommitFailure, post) =>
                (m.domain)(i, post) && (m.invariant)(i, post) && (!m.owns[i] ==> post == s),
            _ => true,
        })
    && (forall|i: int, s: S, c: I| 0 <= i < m.n &&
        (#[trigger] (m.decision)(i, s, c)).0 == Local::CommitFailure ==> m.failure_case[i])
}

pub(crate) open spec fn delivery_invariant<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, r: R, i: int) -> bool {
    input(m, pre, r, i).is_some()
    && (m.input_domain)(i, input(m, pre, r, i).unwrap())
    && (m.assumption)(i, input(m, pre, r, i).unwrap())
    && (m.domain)(i, state(m, pre, i)) && (m.invariant)(i, state(m, pre, i))
    && (!m.owns[i] ==> (m.state_binding)(i, observations(m, pre, i)).is_some())
}

pub(crate) open spec fn assume_obligation<S, I, R>(m: Model<S, I, R>) -> bool {
    forall|pre: Seq<S>, r: R, i: int| global_invariant(m, pre) && (m.request_domain)(r)
        && 0 <= i < m.n && participates(m, pre, r, i) ==>
        input(m, pre, r, i).is_some()
        && (m.input_domain)(i, input(m, pre, r, i).unwrap())
        && (m.assumption)(i, input(m, pre, r, i).unwrap())
        && (!m.owns[i] ==> (m.state_binding)(i, observations(m, pre, i)).is_some()
            && (m.domain)(i, state(m, pre, i))
            && (m.invariant)(i, state(m, pre, i)))
}

pub(crate) open spec fn outcome<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, r: R, i: int) -> (Local, S) {
    if !participates(m, pre, r, i) { (Local::Skip, pre[i]) }
    else if input(m, pre, r, i).is_none()
        || (!m.owns[i] && (m.state_binding)(i, observations(m, pre, i)).is_none()) {
        (Local::Refused, pre[i])
    } else {
        let decision = (m.decision)(i, state(m, pre, i), input(m, pre, r, i).unwrap());
        if !m.owns[i] && (decision.0 == Local::Commit || decision.0 == Local::CommitFailure)
            && decision.1 != state(m, pre, i) { (Local::Refused, pre[i]) }
        else { decision }
    }
}

pub(crate) open spec fn decisions<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, r: R) -> Seq<Local> {
    Seq::new(m.n, |i: int| outcome(m, pre, r, i).0)
}

pub(crate) open spec fn posts<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, r: R) -> Seq<Option<S>> {
    Seq::new(m.n, |i: int| {
        let d = outcome(m, pre, r, i);
        if d.0 == Local::Commit || d.0 == Local::CommitFailure { Some(d.1) } else { None }
    })
}

pub(crate) open spec fn joint_step<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, r: R) -> Seq<S> {
    apply_spec(pre, posts(m, pre, r), m.owns, resolve_spec(m.n as int, decisions(m, pre, r)))
}

pub(crate) open spec fn members_before_continue<S, I, R>(m: Model<S, I, R>, k: int, ds: Seq<Local>, cut: int) -> bool {
    forall|i: int| 0 <= i < cut && m.members[k].contains(i) ==> !terminal(#[trigger] ds[i])
}

pub(crate) open spec fn member_end<S, I, R>(m: Model<S, I, R>, k: int, ds: Seq<Local>, cut: int) -> bool {
    (cut == m.n && members_before_continue(m, k, ds, cut))
    || (0 < cut <= m.n && m.members[k].contains(cut - 1) && ds[cut - 1] == Local::CommitFailure
        && members_before_continue(m, k, ds, cut - 1))
}

pub(crate) open spec fn outside_cut<S, I, R>(m: Model<S, I, R>, k: int, ds: Seq<Local>, cut: int) -> bool {
    0 < cut <= m.n && !m.members[k].contains(cut - 1) && m.failure_case[cut - 1]
        && members_before_continue(m, k, ds, cut - 1)
}

pub(crate) open spec fn member_prefix<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, r: R, k: int, cut: int) -> Seq<S> {
    apply_spec(pre, posts(m, pre, r), Seq::new(m.n, |i: int| m.owns[i] && m.members[k].contains(i)),
        Resolution { end: End::Accept, committed: cut as usize })
}

pub(crate) open spec fn couple_obligation<S, I, R>(m: Model<S, I, R>) -> bool {
    (forall|k: int| 0 <= k < m.members.len() ==> #[trigger] (m.coupling)(k, m.genesis))
    && (forall|pre: Seq<S>, r: R, k: int, cut: int| global_invariant(m, pre) && (m.request_domain)(r)
        && 0 <= k < m.members.len() && (m.coupling)(k, pre) && 0 <= cut <= m.n
        && (member_end(m, k, decisions(m, pre, r), cut)
            || outside_cut(m, k, decisions(m, pre, r), cut))
        ==> #[trigger] (m.coupling)(k, member_prefix(m, pre, r, k, cut)))
}

pub(crate) open spec fn all_couplings<S, I, R>(m: Model<S, I, R>, pre: Seq<S>) -> bool {
    forall|k: int| 0 <= k < m.members.len() ==> #[trigger] (m.coupling)(k, pre)
}

pub(crate) proof fn delivered_inputs_keep_domains_assumptions_and_invariants<S, I, R>(
    m: Model<S, I, R>, pre: Seq<S>, r: R)
    requires well_formed(m), global_invariant(m, pre), assume_obligation(m), (m.request_domain)(r),
    ensures forall|i: int| 0 <= i < m.n && participates(m, pre, r, i) ==>
        #[trigger] delivery_invariant(m, pre, r, i),
{
    assert forall|i: int| 0 <= i < m.n && participates(m, pre, r, i)
        implies delivery_invariant(m, pre, r, i) by {
        assert(input(m, pre, r, i).is_some());
        if m.owns[i] { assert((m.domain)(i, pre[i])); assert((m.invariant)(i, pre[i])); }
    }
}

pub(crate) proof fn resolution_prefix(ds: Seq<Local>, at: int)
    requires 0 <= at <= ds.len(), ds.len() <= usize::MAX,
    ensures ({
        let r = resolve_from(ds.len() as int, ds, at);
        r.committed <= ds.len() && match r.end {
            End::Accept => r.committed == ds.len() && (forall|i: int| at <= i < ds.len() ==> !terminal(#[trigger] ds[i])),
            End::Reject(t) => at <= t < ds.len() && ds[t as int] == Local::Reject && r.committed == 0
                && (forall|i: int| at <= i < t ==> !terminal(#[trigger] ds[i])),
            End::Refused(t) => at <= t < ds.len() && ds[t as int] == Local::Refused && r.committed == 0
                && (forall|i: int| at <= i < t ==> !terminal(#[trigger] ds[i])),
            End::CommittedFailure(t) => at <= t < ds.len() && ds[t as int] == Local::CommitFailure && r.committed == t + 1
                && (forall|i: int| at <= i < t ==> !terminal(#[trigger] ds[i])),
        }
    }),
    decreases ds.len() - at,
{
    if at < ds.len() && !terminal(ds[at]) {
        resolution_prefix(ds, at + 1);
    }
}

pub(crate) proof fn component_invariants_preserved<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, r: R)
    requires well_formed(m), global_invariant(m, pre), local_obligation(m), assume_obligation(m), (m.request_domain)(r),
    ensures global_invariant(m, joint_step(m, pre, r)),
{
    delivered_inputs_keep_domains_assumptions_and_invariants(m, pre, r);
    let ds = decisions(m, pre, r);
    resolution_prefix(ds, 0);
    assert forall|i: int| 0 <= i < m.n && m.owns[i]
        implies (m.domain)(i, joint_step(m, pre, r)[i]) && (m.invariant)(i, joint_step(m, pre, r)[i]) by {
        let resolution = resolve_spec(m.n as int, ds);
        let d = outcome(m, pre, r, i);
        assert((m.domain)(i, pre[i]));
        assert((m.invariant)(i, pre[i]));
        if !rollback(resolution) && i < resolution.committed && posts(m, pre, r)[i].is_some() {
            assert(d.0 == Local::Commit || d.0 == Local::CommitFailure);
            assert(participates(m, pre, r, i));
            assert(delivery_invariant(m, pre, r, i));
            assert(d == (m.decision)(i, pre[i], input(m, pre, r, i).unwrap()));
            assert((m.domain)(i, d.1) && (m.invariant)(i, d.1));
        }
    }
}

pub(crate) proof fn coupling_invariants_preserved<S, I, R>(m: Model<S, I, R>, pre: Seq<S>, r: R)
    requires well_formed(m), global_invariant(m, pre), all_couplings(m, pre),
        local_obligation(m), assume_obligation(m), couple_obligation(m), (m.request_domain)(r),
    ensures all_couplings(m, joint_step(m, pre, r)),
{
    let ds = decisions(m, pre, r);
    resolution_prefix(ds, 0);
    let resolution = resolve_spec(m.n as int, ds);
    assert forall|k: int| 0 <= k < m.members.len()
        implies #[trigger] (m.coupling)(k, joint_step(m, pre, r)) by {
        assert((m.coupling)(k, pre));
        if rollback(resolution) {
            assert(joint_step(m, pre, r) =~= pre);
        } else {
            let cut = resolution.committed as int;
            match resolution.end {
                End::Accept => { assert(member_end(m, k, ds, cut)); },
                End::CommittedFailure(t) => {
                    if m.members[k].contains(t as int) { assert(member_end(m, k, ds, cut)); }
                    else {
                        assert(outcome(m, pre, r, t as int).0 == Local::CommitFailure);
                        assert((m.decision)(t as int, state(m, pre, t as int), input(m, pre, r, t as int).unwrap()).0 == Local::CommitFailure);
                        assert(m.failure_case[t as int]);
                        assert(outside_cut(m, k, ds, cut));
                    }
                },
                _ => { assert(false); },
            }
            assert((m.coupling)(k, member_prefix(m, pre, r, k, cut)));
            assert forall|i: int| m.members[k].contains(i)
                implies joint_step(m, pre, r)[i] == member_prefix(m, pre, r, k, cut)[i] by {
                assert(0 <= i < m.n && m.owns[i]);
            }
            assert((m.coupling)(k, joint_step(m, pre, r)) == (m.coupling)(k, member_prefix(m, pre, r, k, cut)));
        }
    }
}

pub(crate) open spec fn history<S, I, R>(m: Model<S, I, R>, states: Seq<Seq<S>>, requests: Seq<R>) -> bool {
    states.len() == requests.len() + 1 && states[0] == m.genesis
    && (forall|t: int| 0 <= t < requests.len() ==> #[trigger] (m.request_domain)(requests[t]))
    && (forall|t: int| 0 <= t < requests.len() ==> #[trigger] states[t + 1] == joint_step(m, states[t], requests[t]))
}

pub(crate) proof fn history_at<S, I, R>(m: Model<S, I, R>, states: Seq<Seq<S>>, requests: Seq<R>, t: nat)
    requires well_formed(m), local_obligation(m), assume_obligation(m), couple_obligation(m),
        history(m, states, requests), t < states.len(),
    ensures global_invariant(m, states[t as int]), all_couplings(m, states[t as int]),
    decreases t,
{
    if t == 0 {
        assert(states[0] == m.genesis);
        assert(global_invariant(m, m.genesis));
        assert(all_couplings(m, m.genesis));
    } else {
        history_at(m, states, requests, (t - 1) as nat);
        let j = t as int - 1;
        assert((m.request_domain)(requests[j]));
        assert(states[j + 1] == joint_step(m, states[j], requests[j]));
        component_invariants_preserved(m, states[j], requests[j]);
        coupling_invariants_preserved(m, states[j], requests[j]);
    }
}

/// The guide's safety claim: every reachable owned state and every coupling
/// invariant holds, and every participating delivery has admitted inputs and
/// a state satisfying its domain and invariant, including read-only state.
pub(crate) proof fn joint_induction<S, I, R>(m: Model<S, I, R>, states: Seq<Seq<S>>, requests: Seq<R>)
    requires well_formed(m), local_obligation(m), assume_obligation(m), couple_obligation(m),
        history(m, states, requests),
    ensures
        forall|t: int| 0 <= t < states.len() ==>
            #[trigger] global_invariant(m, states[t]) && all_couplings(m, states[t]),
        forall|t: int, i: int| 0 <= t < requests.len() && 0 <= i < m.n
            && participates(m, states[t], requests[t], i) ==>
            #[trigger] delivery_invariant(m, states[t], requests[t], i),
{
    assert forall|t: int| 0 <= t < states.len() implies #[trigger] global_invariant(m, states[t]) && all_couplings(m, states[t]) by {
        history_at(m, states, requests, t as nat);
    }
    assert forall|t: int, i: int| 0 <= t < requests.len() && 0 <= i < m.n
        && participates(m, states[t], requests[t], i)
        implies #[trigger] delivery_invariant(m, states[t], requests[t], i) by {
        delivered_inputs_keep_domains_assumptions_and_invariants(m, states[t], requests[t]);
    }
}

/// Three components: owners 0 and 2, a read-only guard at 1, and an
/// outside committed failure when the two owners do not participate.
pub(crate) open spec fn pilot_model() -> Model<int, bool, bool> {
    Model {
        n: 3,
        owns: seq![true, false, true],
        providers: seq![Seq::empty(), seq![0int], Seq::empty()],
        genesis: seq![0int, 0int, 0int],
        genesis_law: |i: int, s: int| s == 0,
        domain: |i: int, s: int| 0 <= s <= 1,
        input_domain: |i: int, c: bool| true,
        request_domain: |r: bool| true,
        invariant: |i: int, s: int| s >= 0,
        assumption: |i: int, c: bool| true,
        participation: |i: int, observed: Seq<int>, r: bool| i == 1 || r,
        state_binding: |i: int, observed: Seq<int>| Some(observed[0]),
        input_binding: |i: int, observed: Seq<int>, r: bool| Some(r),
        decision: |i: int, s: int, c: bool|
            if i == 1 { (if c { Local::Commit } else { Local::CommitFailure }, s) }
            else { (Local::Commit, if i == 0 { 0int } else { 1int }) },
        failure_case: seq![false, true, false],
        members: seq![set![0int, 2int]],
        coupling: |k: int, s: Seq<int>| s[0] <= s[2],
    }
}

pub(crate) proof fn pilot_obligations()
    ensures well_formed(pilot_model()), local_obligation(pilot_model()),
        assume_obligation(pilot_model()), couple_obligation(pilot_model()),
{
    let m = pilot_model();
    assert forall|pre: Seq<int>, r: bool, i: int| global_invariant(m, pre) && (m.request_domain)(r)
        && 0 <= i < m.n && participates(m, pre, r, i) implies
        input(m, pre, r, i).is_some()
        && (m.input_domain)(i, input(m, pre, r, i).unwrap())
        && (m.assumption)(i, input(m, pre, r, i).unwrap())
        && (!m.owns[i] ==> (m.state_binding)(i, observations(m, pre, i)).is_some()
            && (m.domain)(i, state(m, pre, i))
            && (m.invariant)(i, state(m, pre, i))) by {
        if i == 1 {
            assert((m.domain)(0, pre[0]));
            assert(observations(m, pre, i) =~= seq![pre[0]]);
            assert(state(m, pre, i) == pre[0]);
        }
    }
    assert forall|pre: Seq<int>, r: bool, k: int, cut: int|
        global_invariant(m, pre) && (m.request_domain)(r)
        && 0 <= k < m.members.len() && (m.coupling)(k, pre) && 0 <= cut <= m.n
        && (member_end(m, k, decisions(m, pre, r), cut)
            || outside_cut(m, k, decisions(m, pre, r), cut))
        implies #[trigger] (m.coupling)(k, member_prefix(m, pre, r, k, cut)) by {
        assert(k == 0);
        assert((m.domain)(0, pre[0]));
        assert((m.domain)(2, pre[2]));
        if r {
            assert(decisions(m, pre, r) =~= seq![Local::Commit, Local::Commit, Local::Commit]);
            assert(cut == 2 || cut == 3);
        } else {
            assert(decisions(m, pre, r) =~= seq![Local::Skip, Local::CommitFailure, Local::Skip]);
        }
    }
}

pub(crate) proof fn three_component_pilot(states: Seq<Seq<int>>, requests: Seq<bool>)
    requires history(pilot_model(), states, requests),
    ensures
        forall|t: int| 0 <= t < states.len() ==>
            #[trigger] global_invariant(pilot_model(), states[t]) && all_couplings(pilot_model(), states[t]),
        forall|t: int, i: int| 0 <= t < requests.len() && 0 <= i < 3
            && participates(pilot_model(), states[t], requests[t], i) ==>
            #[trigger] delivery_invariant(pilot_model(), states[t], requests[t], i),
{
    pilot_obligations();
    joint_induction(pilot_model(), states, requests);
}

/// A domain-admitted provider can deliver a read-only state outside that
/// guard's invariant. Domain and assumption checks alone do not discharge it.
pub(crate) open spec fn read_only_bad_model() -> Model<int, bool, bool> {
    let m = pilot_model();
    Model { n: m.n, owns: m.owns, providers: m.providers, genesis: m.genesis,
        genesis_law: m.genesis_law, domain: m.domain, input_domain: m.input_domain,
        request_domain: m.request_domain, invariant: |i: int, s: int| if i == 1 { s == 0 } else { s >= 0 },
        assumption: m.assumption, participation: m.participation, state_binding: m.state_binding,
        input_binding: m.input_binding, decision: m.decision, failure_case: m.failure_case,
        members: m.members, coupling: m.coupling }
}

pub(crate) proof fn read_only_invariant_counterexample()
    ensures
        global_invariant(read_only_bad_model(), seq![1int, 0int, 1int]),
        input(read_only_bad_model(), seq![1int, 0int, 1int], false, 1).is_some(),
        (read_only_bad_model().domain)(1, state(read_only_bad_model(), seq![1int, 0int, 1int], 1)),
        !(read_only_bad_model().invariant)(1, state(read_only_bad_model(), seq![1int, 0int, 1int], 1)),
        !delivery_invariant(read_only_bad_model(), seq![1int, 0int, 1int], false, 1),
{
}

/// A gap containing no member cannot change a member-masked prefix. This
/// includes the interval after the last member, up to the final full prefix.
pub(crate) proof fn member_prefix_constant_between_members<S, I, R>(
    m: Model<S, I, R>, pre: Seq<S>, r: R, k: int, left: int, right: int)
    requires
        well_formed(m), pre.len() == m.n, 0 <= k < m.members.len(),
        0 <= left <= right <= m.n,
        forall|i: int| left <= i < right ==> !m.members[k].contains(i),
    ensures member_prefix(m, pre, r, k, left) == member_prefix(m, pre, r, k, right),
{
    assert forall|i: int| 0 <= i < m.n implies
        member_prefix(m, pre, r, k, left)[i] == member_prefix(m, pre, r, k, right)[i] by {
        if left <= i < right { assert(!m.members[k].contains(i)); }
    }
    assert(member_prefix(m, pre, r, k, left) =~= member_prefix(m, pre, r, k, right));
}

/// Possible outside failures map to a pre-member or final checkpoint.
/// Native member/model binding and row-state correspondence remain premises.
pub(crate) open spec fn checkpoint(n: nat, members: Seq<usize>, p: int) -> int {
    if p < members.len() { members[p] as int } else { n as int }
}

pub(crate) proof fn find_boundary(members: Seq<usize>, cut: int, at: nat) -> (p: int)
    requires at <= members.len(),
        forall|q: int| 0 <= q < at ==> #[trigger] members[q] < cut,
    ensures at <= p <= members.len(),
        forall|q: int| 0 <= q < p ==> #[trigger] members[q] < cut,
        p < members.len() ==> cut <= members[p],
    decreases members.len() - at,
{
    if at == members.len() || cut <= members[at as int] { at as int }
    else {
        assert forall|q: int| 0 <= q < at + 1 implies #[trigger] members[q] < cut by {
            if q < at { assert(members[q] < cut); }
        }
        find_boundary(members, cut, at + 1)
    }
}

pub(crate) proof fn outside_cut_has_checkpoint<S, I, R>(
    m: Model<S, I, R>, pre: Seq<S>, r: R, k: int, cut: int,
    members: Seq<usize>, groups: Seq<Seq<usize>>) -> (p: int)
    requires
        well_formed(m), pre.len() == m.n, 0 <= k < m.members.len(),
        ordered_members(members, m.n as int), groups.len() == members.len(),
        forall|i: int| 0 <= i < m.n ==> #[trigger] m.members[k].contains(i) == members.contains(i as usize),
        forall|q: int| 0 <= q < members.len() ==> exact_interval(#[trigger] groups[q],
            m.failure_case, group_start(members, q), members[q] as int),
        outside_cut(m, k, decisions(m, pre, r), cut),
    ensures
        0 <= p <= members.len(),
        p < members.len() ==> groups[p].contains((cut - 1) as usize),
        forall|q: int| 0 <= q < p ==> !terminal(#[trigger] decisions(m, pre, r)[members[q] as int]),
        member_prefix(m, pre, r, k, cut) == member_prefix(m, pre, r, k, checkpoint(m.n, members, p)),
{
    let p = find_boundary(members, cut, 0);
    let end = checkpoint(m.n, members, p);
    assert(cut <= end <= m.n);
    if p < members.len() {
        if p > 0 {
            assert(members.contains(members[p - 1]));
            assert(m.members[k].contains(members[p - 1] as int));
            assert(members[p - 1] != cut - 1);
        }
        assert(group_start(members, p) <= cut - 1 < members[p]);
        assert(groups[p].contains((cut - 1) as usize));
    }
    assert forall|q: int| 0 <= q < p implies !terminal(#[trigger] decisions(m, pre, r)[members[q] as int]) by {
        assert(members.contains(members[q]));
        assert(m.members[k].contains(members[q] as int));
        assert(members[q] != cut - 1);
        assert(members[q] < cut - 1);
    }
    assert forall|i: int| cut <= i < end implies !m.members[k].contains(i) by {
        if m.members[k].contains(i) {
            assert(members.contains(i as usize));
            let q = choose|q: int| 0 <= q < members.len() && members[q] == i as usize;
            if q < p { assert(members[q] < cut); }
            else {
                assert(p < members.len());
                if p < q { assert(members[p] < members[q]); }
                assert(end <= members[q]);
            }
        }
    }
    member_prefix_constant_between_members(m, pre, r, k, cut, end);
    p
}
} // verus!
