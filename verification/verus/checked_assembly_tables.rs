//! The checked tables give the local check, and a validated aggregate's
//! histories are histories of the leaves' local relation.
//!
//! The composition checker establishes three tables by exhaustive
//! enumeration: the local table (each leaf preserves its invariant under its
//! assumption), the StateFact table (each invariant state's view satisfies
//! the leaf's guarantee) and the interface table (provider guarantees imply
//! the consumer's assumption). `tables_imply_local_check` derives the joint
//! module's `local_check` from them. The assembly validator establishes the
//! step equation `assembly_valid` on jointly invariant pre-states, and
//! `aggregate_histories_keep_the_joint_invariant` derives, from genesis, the
//! joint invariant at every state of every aggregate history. The tables,
//! the step equation and genesis are premises here.
use crate::joint::*;
use vstd::prelude::*;

verus! {

/// A leaf's decision on its own slot: Accept with a successor, an unchanged
/// business Reject, or a technical refusal.
pub enum Outcome { Accept(int), Reject, Refuse }

/// What leaf k observes in a joint state: each provider's view of its own
/// slot, in the order of k's interface fields.
pub open spec fn observation(k: int, pre: Seq<int>, providers: spec_fn(int) -> Seq<int>,
    view: spec_fn(int, int) -> int) -> Seq<int> {
    Seq::new(providers(k).len(), |i: int| view(providers(k)[i], pre[providers(k)[i]]))
}

/// Every interface field of leaf k is fed by another leaf.
pub open spec fn wiring(n: nat, providers: spec_fn(int) -> Seq<int>) -> bool {
    forall|k: int, i: int| 0 <= k < n && 0 <= i < providers(k).len() ==>
        0 <= #[trigger] providers(k)[i] < n && providers(k)[i] != k
}

/// The local table L: on every input whose state satisfies I and whose
/// command and observation satisfy A, an Accept keeps I, a Reject keeps the
/// state, and the Authority never refuses.
pub open spec fn local_table<C>(n: nat, inv: spec_fn(int, int) -> bool,
    assumption: spec_fn(int, Seq<int>, C) -> bool, admissible: spec_fn(int, C) -> bool,
    decide: spec_fn(int, int, Seq<int>, C) -> Outcome) -> bool {
    forall|k: int, s: int, o: Seq<int>, c: C|
        #![trigger decide(k, s, o, c)]
        0 <= k < n && inv(k, s) && admissible(k, c) && assumption(k, o, c) ==>
        match decide(k, s, o, c) {
            Outcome::Accept(t) => inv(k, t),
            Outcome::Reject => true,
            Outcome::Refuse => false,
        }
}

/// The StateFact table F: every state satisfying I has a view satisfying G.
pub open spec fn fact_table(n: nat, inv: spec_fn(int, int) -> bool,
    view: spec_fn(int, int) -> int, fact: spec_fn(int, int) -> bool) -> bool {
    forall|j: int, s: int| 0 <= j < n && inv(j, s) ==> #[trigger] fact(j, view(j, s))
}

/// The interface table E: provider facts on every observation imply the
/// consumer's assumption, for every admitted command.
pub open spec fn interface_table<C>(n: nat, providers: spec_fn(int) -> Seq<int>,
    fact: spec_fn(int, int) -> bool, assumption: spec_fn(int, Seq<int>, C) -> bool,
    admissible: spec_fn(int, C) -> bool) -> bool {
    forall|k: int, o: Seq<int>, c: C|
        #![trigger assumption(k, o, c)]
        0 <= k < n && o.len() == providers(k).len() && admissible(k, c)
        && (forall|i: int| 0 <= i < o.len() ==> #[trigger] fact(providers(k)[i], o[i]))
        ==> assumption(k, o, c)
}

/// The joint module's command assumption: the command is admitted on its route.
pub open spec fn admitted<C>(admissible: spec_fn(int, C) -> bool) -> spec_fn(int, Seq<int>, C) -> bool {
    |k: int, pre: Seq<int>, c: C| admissible(k, c)
}

/// The joint module's local relation: the leaf accepts with that successor, or
/// rejects and its slot is unchanged.
pub open spec fn local_of<C>(providers: spec_fn(int) -> Seq<int>, view: spec_fn(int, int) -> int,
    decide: spec_fn(int, int, Seq<int>, C) -> Outcome) -> spec_fn(int, Seq<int>, C, int) -> bool {
    |k: int, pre: Seq<int>, c: C, v: int|
        match decide(k, pre[k], observation(k, pre, providers, view), c) {
            Outcome::Accept(t) => v == t,
            Outcome::Reject => v == pre[k],
            Outcome::Refuse => false,
        }
}

/// Peer invariants, provider facts and the interface table give the
/// selected leaf's assumption in the same pre-state.
pub proof fn assumption_from_peers<C>(n: nat, pre: Seq<int>, k: int, c: C,
    inv: spec_fn(int, int) -> bool, view: spec_fn(int, int) -> int,
    fact: spec_fn(int, int) -> bool, providers: spec_fn(int) -> Seq<int>,
    assumption: spec_fn(int, Seq<int>, C) -> bool, admissible: spec_fn(int, C) -> bool)
    requires
        pre.len() == n, 0 <= k < n, admissible(k, c), peer_invariants(pre, inv, k),
        wiring(n, providers), fact_table(n, inv, view, fact),
        interface_table(n, providers, fact, assumption, admissible),
    ensures assumption(k, observation(k, pre, providers, view), c),
{
    let o = observation(k, pre, providers, view);
    assert forall|i: int| 0 <= i < o.len() implies #[trigger] fact(providers(k)[i], o[i]) by {
        let p = providers(k)[i];
        assert(0 <= p < n && p != k);
        assert(inv(p, pre[p]));
        assert(fact(p, view(p, pre[p])));
        assert(o[i] == view(p, pre[p]));
    }
    assert(assumption(k, o, c));
}

/// Peer invariants give provider facts, which give the consumer's
/// assumption, which with the local table gives local preservation: the
/// joint module's `local_check`.
pub proof fn tables_imply_local_check<C>(n: nat, inv: spec_fn(int, int) -> bool,
    view: spec_fn(int, int) -> int, fact: spec_fn(int, int) -> bool,
    providers: spec_fn(int) -> Seq<int>, assumption: spec_fn(int, Seq<int>, C) -> bool,
    admissible: spec_fn(int, C) -> bool, decide: spec_fn(int, int, Seq<int>, C) -> Outcome)
    requires
        wiring(n, providers),
        local_table(n, inv, assumption, admissible, decide),
        fact_table(n, inv, view, fact),
        interface_table(n, providers, fact, assumption, admissible),
    ensures local_check(n, inv, admitted(admissible), local_of(providers, view, decide)),
{
    assert forall|k: int, pre: Seq<int>, c: C, v: int|
        0 <= k < n && pre.len() == n && inv(k, pre[k])
        && peer_invariants(pre, inv, k) && admitted(admissible)(k, pre, c)
        && #[trigger] local_of(providers, view, decide)(k, pre, c, v) implies inv(k, v) by {
        assumption_from_peers(n, pre, k, c, inv, view, fact, providers, assumption, admissible);
        let o = observation(k, pre, providers, view);
        match decide(k, pre[k], o, c) {
            Outcome::Accept(t) => { assert(inv(k, t)); },
            Outcome::Reject => {},
            Outcome::Refuse => {},
        }
    }
}

/// An aggregate decision on a joint state.
pub enum Aggregate { Commit(Seq<int>), Reject, Refuse }

/// The assembly equation the validator establishes: on a jointly invariant
/// pre-state, a command admitted on route k makes the aggregate commit the
/// selected leaf's successor in slot k with every other slot unchanged,
/// reject when the leaf rejects, and refuse only when the leaf refuses.
pub open spec fn assembly_valid<C>(n: nat, inv: spec_fn(int, int) -> bool,
    admissible: spec_fn(int, C) -> bool, providers: spec_fn(int) -> Seq<int>,
    view: spec_fn(int, int) -> int, decide: spec_fn(int, int, Seq<int>, C) -> Outcome,
    agg: spec_fn(Seq<int>, int, C) -> Aggregate) -> bool {
    forall|pre: Seq<int>, k: int, c: C|
        pre.len() == n && joint(pre, inv) && 0 <= k < n && admissible(k, c) ==>
        #[trigger] agg(pre, k, c) == match decide(k, pre[k], observation(k, pre, providers, view), c) {
            Outcome::Accept(t) => Aggregate::Commit(pre.update(k, t)),
            Outcome::Reject => Aggregate::Reject,
            Outcome::Refuse => Aggregate::Refuse,
        }
}

/// The state after an aggregate decision: a commit's successor; a Reject
/// or a refusal publishes nothing and keeps the state.
pub open spec fn next(outcome: Aggregate, pre: Seq<int>) -> Seq<int> {
    match outcome { Aggregate::Commit(post) => post, _ => pre }
}

/// A history of the aggregate's actual decisions on routed, admitted commands.
pub open spec fn aggregate_history<C>(n: nat, states: Seq<Seq<int>>, routes: Seq<int>,
    commands: Seq<C>, admissible: spec_fn(int, C) -> bool,
    agg: spec_fn(Seq<int>, int, C) -> Aggregate) -> bool {
    states.len() == commands.len() + 1 && routes.len() == commands.len()
    && forall|t: int| 0 <= t < commands.len() ==>
        0 <= #[trigger] routes[t] < n && admissible(routes[t], commands[t])
        && states[t + 1] == next(agg(states[t], routes[t], commands[t]), states[t])
}

/// One aggregate step from a jointly invariant pre-state is a joint-module
/// routed step of the leaves' local relation.
pub proof fn aggregate_step_is_routed<C>(n: nat, pre: Seq<int>, k: int, c: C,
    inv: spec_fn(int, int) -> bool, view: spec_fn(int, int) -> int,
    fact: spec_fn(int, int) -> bool, providers: spec_fn(int) -> Seq<int>,
    assumption: spec_fn(int, Seq<int>, C) -> bool, admissible: spec_fn(int, C) -> bool,
    decide: spec_fn(int, int, Seq<int>, C) -> Outcome, agg: spec_fn(Seq<int>, int, C) -> Aggregate)
    requires
        pre.len() == n, joint(pre, inv), 0 <= k < n, admissible(k, c),
        wiring(n, providers), local_table(n, inv, assumption, admissible, decide),
        fact_table(n, inv, view, fact), interface_table(n, providers, fact, assumption, admissible),
        assembly_valid(n, inv, admissible, providers, view, decide, agg),
    ensures
        routed_step(pre, next(agg(pre, k, c), pre), k, c, admitted(admissible),
            local_of(providers, view, decide)),
{
    assert(peer_invariants(pre, inv, k));
    assert(inv(k, pre[k]));
    assumption_from_peers(n, pre, k, c, inv, view, fact, providers, assumption, admissible);
    let o = observation(k, pre, providers, view);
    let post = next(agg(pre, k, c), pre);
    match decide(k, pre[k], o, c) {
        Outcome::Accept(t) => {
            assert(agg(pre, k, c) == Aggregate::Commit(pre.update(k, t)));
            assert(post == pre.update(k, t));
            assert(local_of(providers, view, decide)(k, pre, c, post[k]));
            assert forall|i: int| 0 <= i < pre.len() && i != k implies post[i] == pre[i] by {};
        },
        Outcome::Reject => {
            assert(agg(pre, k, c) == Aggregate::Reject);
            assert(post == pre);
            assert(local_of(providers, view, decide)(k, pre, c, post[k]));
        },
        Outcome::Refuse => {
            // The local table rules a refusal out under I and A.
            assert(false);
        },
    }
}

/// Every aggregate state up to `t` is jointly invariant.
pub proof fn aggregate_prefix<C>(n: nat, states: Seq<Seq<int>>, routes: Seq<int>,
    commands: Seq<C>, init: spec_fn(int, int) -> bool, inv: spec_fn(int, int) -> bool,
    view: spec_fn(int, int) -> int, fact: spec_fn(int, int) -> bool,
    providers: spec_fn(int) -> Seq<int>, assumption: spec_fn(int, Seq<int>, C) -> bool,
    admissible: spec_fn(int, C) -> bool, decide: spec_fn(int, int, Seq<int>, C) -> Outcome,
    agg: spec_fn(Seq<int>, int, C) -> Aggregate, t: nat)
    requires
        states.len() > 0, states[0].len() == n, t < states.len(),
        init_check(n, init, inv),
        forall|i: int| 0 <= i < n ==> #[trigger] init(i, states[0][i]),
        wiring(n, providers), local_table(n, inv, assumption, admissible, decide),
        fact_table(n, inv, view, fact), interface_table(n, providers, fact, assumption, admissible),
        assembly_valid(n, inv, admissible, providers, view, decide, agg),
        aggregate_history(n, states, routes, commands, admissible, agg),
    ensures states[t as int].len() == n, joint(states[t as int], inv),
    decreases t,
{
    if t == 0 {
        joint_genesis(n, states[0], init, inv);
    } else {
        aggregate_prefix(n, states, routes, commands, init, inv, view, fact, providers,
            assumption, admissible, decide, agg, (t - 1) as nat);
        let j = t as int - 1;
        assert(0 <= routes[j] < n && admissible(routes[j], commands[j]));
        assert(states[j + 1] == next(agg(states[j], routes[j], commands[j]), states[j]));
        aggregate_step_is_routed(n, states[j], routes[j], commands[j], inv, view, fact,
            providers, assumption, admissible, decide, agg);
        tables_imply_local_check(n, inv, view, fact, providers, assumption, admissible, decide);
        joint_step(n, states[j], states[j + 1], routes[j], commands[j], inv,
            admitted(admissible), local_of(providers, view, decide));
    }
}

/// The assembly correspondence: from a genesis satisfying each leaf's Init,
/// every history of a validated aggregate is a joint-module history of the
/// leaves' local relation, and `joint_induction` gives the joint
/// invariant at every step.
pub proof fn aggregate_histories_keep_the_joint_invariant<C>(n: nat, states: Seq<Seq<int>>,
    routes: Seq<int>, commands: Seq<C>, init: spec_fn(int, int) -> bool,
    inv: spec_fn(int, int) -> bool, view: spec_fn(int, int) -> int,
    fact: spec_fn(int, int) -> bool, providers: spec_fn(int) -> Seq<int>,
    assumption: spec_fn(int, Seq<int>, C) -> bool, admissible: spec_fn(int, C) -> bool,
    decide: spec_fn(int, int, Seq<int>, C) -> Outcome, agg: spec_fn(Seq<int>, int, C) -> Aggregate)
    requires
        states.len() > 0, states[0].len() == n,
        init_check(n, init, inv),
        forall|i: int| 0 <= i < n ==> #[trigger] init(i, states[0][i]),
        wiring(n, providers), local_table(n, inv, assumption, admissible, decide),
        fact_table(n, inv, view, fact), interface_table(n, providers, fact, assumption, admissible),
        assembly_valid(n, inv, admissible, providers, view, decide, agg),
        aggregate_history(n, states, routes, commands, admissible, agg),
    ensures
        history(states, routes, commands, admitted(admissible), local_of(providers, view, decide)),
        forall|t: int| 0 <= t < states.len() ==> #[trigger] joint(states[t], inv),
        forall|t: int| 0 <= t < states.len() ==> (#[trigger] states[t]).len() == n,
{
    tables_imply_local_check(n, inv, view, fact, providers, assumption, admissible, decide);
    assert forall|t: int| 0 <= t < commands.len() implies
        #[trigger] routed_step(states[t], states[t + 1], routes[t], commands[t],
            admitted(admissible), local_of(providers, view, decide)) by {
        aggregate_prefix(n, states, routes, commands, init, inv, view, fact, providers,
            assumption, admissible, decide, agg, t as nat);
        assert(0 <= routes[t] < n && admissible(routes[t], commands[t]));
        assert(states[t + 1] == next(agg(states[t], routes[t], commands[t]), states[t]));
        aggregate_step_is_routed(n, states[t], routes[t], commands[t], inv, view, fact,
            providers, assumption, admissible, decide, agg);
    }
    assert(history(states, routes, commands, admitted(admissible), local_of(providers, view, decide)));
    joint_induction(n, states, routes, commands, init, inv, admitted(admissible),
        local_of(providers, view, decide));
    assert forall|t: int| 0 <= t < states.len() implies (#[trigger] states[t]).len() == n by {
        joint_history_at(n, states, routes, commands, init, inv, admitted(admissible),
            local_of(providers, view, decide), t as nat);
    }
}

// ---------------------------------------------------------------------------
// The register ring as an instance: the premises above hold for it, so the
// theorem is not vacuous. Registers range over 0..7 with invariant 0..3;
// a command sets a register within 0..3 or copies the next register.

pub enum Command { Set(int), CopyNeighbor }
pub open spec fn register_ok(v: int) -> bool { 0 <= v <= 3 }
pub open spec fn command_ok(c: Command) -> bool {
    match c { Command::Set(v) => 0 <= v <= 3, Command::CopyNeighbor => true }
}
pub open spec fn neighbor(k: int, n: nat) -> int { if k + 1 < n { k + 1 } else { 0 } }

pub open spec fn ring_providers(n: nat) -> spec_fn(int) -> Seq<int> {
    |k: int| seq![neighbor(k, n)]
}
pub open spec fn ring_view() -> spec_fn(int, int) -> int { |j: int, s: int| s }
pub open spec fn ring_inv() -> spec_fn(int, int) -> bool { |j: int, s: int| register_ok(s) }
pub open spec fn ring_fact() -> spec_fn(int, int) -> bool { |j: int, o: int| register_ok(o) }
pub open spec fn ring_admissible() -> spec_fn(int, Command) -> bool { |k: int, c: Command| command_ok(c) }
/// The leaf assumption: a copy needs the peer view within the invariant.
pub open spec fn ring_assumption() -> spec_fn(int, Seq<int>, Command) -> bool {
    |k: int, o: Seq<int>, c: Command| match c {
        Command::CopyNeighbor => o.len() == 1 && register_ok(o[0]),
        Command::Set(v) => true,
    }
}
pub open spec fn ring_value(o: Seq<int>, c: Command) -> int {
    match c { Command::Set(v) => v, Command::CopyNeighbor => if o.len() > 0 { o[0] } else { 0 } }
}
/// The leaf's decision: an unchanged value is the business Reject, a value
/// outside the invariant is refused by the leaf's state law, otherwise Accept.
pub open spec fn ring_decide() -> spec_fn(int, int, Seq<int>, Command) -> Outcome {
    |k: int, s: int, o: Seq<int>, c: Command|
        if ring_value(o, c) == s { Outcome::Reject }
        else if register_ok(ring_value(o, c)) { Outcome::Accept(ring_value(o, c)) }
        else { Outcome::Refuse }
}
/// The aggregate on route k: the selected register's successor, every other unchanged.
pub open spec fn ring_aggregate(n: nat) -> spec_fn(Seq<int>, int, Command) -> Aggregate {
    |pre: Seq<int>, k: int, c: Command|
        match ring_decide()(k, pre[k], observation(k, pre, ring_providers(n), ring_view()), c) {
            Outcome::Accept(t) => Aggregate::Commit(pre.update(k, t)),
            Outcome::Reject => Aggregate::Reject,
            Outcome::Refuse => Aggregate::Refuse,
        }
}

pub proof fn ring_premises_hold(n: nat)
    requires n >= 2,
    ensures
        wiring(n, ring_providers(n)),
        local_table(n, ring_inv(), ring_assumption(), ring_admissible(), ring_decide()),
        fact_table(n, ring_inv(), ring_view(), ring_fact()),
        interface_table(n, ring_providers(n), ring_fact(), ring_assumption(), ring_admissible()),
        assembly_valid(n, ring_inv(), ring_admissible(), ring_providers(n), ring_view(),
            ring_decide(), ring_aggregate(n)),
{
    assert forall|k: int, i: int| 0 <= k < n && 0 <= i < ring_providers(n)(k).len() implies
        0 <= #[trigger] ring_providers(n)(k)[i] < n && ring_providers(n)(k)[i] != k by {
        assert(ring_providers(n)(k)[i] == neighbor(k, n));
    }
    assert forall|k: int, s: int, o: Seq<int>, c: Command|
        #![trigger ring_decide()(k, s, o, c)]
        0 <= k < n && ring_inv()(k, s) && ring_admissible()(k, c) && ring_assumption()(k, o, c) implies
        match ring_decide()(k, s, o, c) {
            Outcome::Accept(t) => ring_inv()(k, t),
            Outcome::Reject => true,
            Outcome::Refuse => false,
        } by {
        match c {
            Command::Set(v) => {},
            Command::CopyNeighbor => { assert(register_ok(o[0])); },
        }
    }
    assert forall|k: int, o: Seq<int>, c: Command|
        #![trigger ring_assumption()(k, o, c)]
        0 <= k < n && o.len() == ring_providers(n)(k).len() && ring_admissible()(k, c)
        && (forall|i: int| 0 <= i < o.len() ==> #[trigger] ring_fact()(ring_providers(n)(k)[i], o[i]))
        implies ring_assumption()(k, o, c) by {
        assert(o.len() == 1);
        assert(ring_fact()(ring_providers(n)(k)[0], o[0]));
    }
}

/// The ring through the theorem: from all-zero genesis, every history of the
/// ring aggregate keeps every register within 0..3.
pub proof fn ring_aggregate_histories(n: nat, states: Seq<Seq<int>>, routes: Seq<int>,
    commands: Seq<Command>)
    requires
        n >= 2, states.len() > 0, states[0] == Seq::new(n, |i: int| 0int),
        aggregate_history(n, states, routes, commands, ring_admissible(), ring_aggregate(n)),
    ensures forall|t: int| 0 <= t < states.len() ==> #[trigger] joint(states[t], ring_inv()),
{
    ring_premises_hold(n);
    let init = |i: int, v: int| v == 0;
    assert(init_check(n, init, ring_inv()));
    assert forall|i: int| 0 <= i < n implies #[trigger] init(i, states[0][i]) by {}
    aggregate_histories_keep_the_joint_invariant(n, states, routes, commands, init, ring_inv(),
        ring_view(), ring_fact(), ring_providers(n), ring_assumption(), ring_admissible(), ring_decide(),
        ring_aggregate(n));
}
} // verus!
