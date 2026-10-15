//! The joint invariant over every finite history of disjoint components.
//!
//! Component `i` owns position `i` of a joint state. Initialization, the
//! local preservation check, each route's command assumption, the selected
//! transition and the framing of every unselected position are premises.
//! Peer invariants come from the same established pre-state, so cyclic
//! provider reasoning needs no circular proof. `joint_induction` gives the
//! conjunction of every component's invariant at every state of a history.
use vstd::prelude::*;

verus! {
pub open spec fn joint<S>(s: Seq<S>, inv: spec_fn(int, S) -> bool) -> bool {
    forall|i: int| 0 <= i < s.len() ==> #[trigger] inv(i, s[i])
}
pub open spec fn peer_invariants<S>(s: Seq<S>, inv: spec_fn(int, S) -> bool, k: int) -> bool {
    forall|i: int| 0 <= i < s.len() && i != k ==> #[trigger] inv(i, s[i])
}
pub open spec fn init_check<S>(n: nat, init: spec_fn(int, S) -> bool,
    inv: spec_fn(int, S) -> bool) -> bool {
    forall|i: int, v: S| 0 <= i < n && #[trigger] init(i, v) ==> inv(i, v)
}
pub open spec fn local_check<S, C>(n: nat, inv: spec_fn(int, S) -> bool,
    a: spec_fn(int, Seq<S>, C) -> bool,
    local: spec_fn(int, Seq<S>, C, S) -> bool) -> bool {
    forall|k: int, pre: Seq<S>, c: C, v: S|
        0 <= k < n && pre.len() == n && inv(k, pre[k])
        && peer_invariants(pre, inv, k) && a(k, pre, c)
        && #[trigger] local(k, pre, c, v) ==> inv(k, v)
}
pub open spec fn routed_step<S, C>(pre: Seq<S>, post: Seq<S>, k: int, c: C,
    a: spec_fn(int, Seq<S>, C) -> bool,
    local: spec_fn(int, Seq<S>, C, S) -> bool) -> bool {
    pre.len() == post.len() && 0 <= k < pre.len()
    && a(k, pre, c) && local(k, pre, c, post[k])
    && forall|i: int| 0 <= i < pre.len() && i != k ==> post[i] == pre[i]
}
pub open spec fn history<S, C>(states: Seq<Seq<S>>, routes: Seq<int>, commands: Seq<C>,
    a: spec_fn(int, Seq<S>, C) -> bool,
    local: spec_fn(int, Seq<S>, C, S) -> bool) -> bool {
    states.len() == commands.len() + 1 && routes.len() == commands.len()
    && forall|t: int| 0 <= t < commands.len() ==>
        #[trigger] routed_step(states[t], states[t + 1], routes[t], commands[t], a, local)
}

pub proof fn joint_genesis<S>(n: nat, genesis: Seq<S>, init: spec_fn(int, S) -> bool,
    inv: spec_fn(int, S) -> bool)
    requires genesis.len() == n, init_check(n, init, inv),
        forall|i: int| 0 <= i < n ==> #[trigger] init(i, genesis[i]),
    ensures joint(genesis, inv),
{
    assert forall|i: int| 0 <= i < genesis.len() implies inv(i, genesis[i]) by {
        assert(init(i, genesis[i]));
    }
}
pub proof fn joint_step<S, C>(n: nat, pre: Seq<S>, post: Seq<S>, k: int, c: C,
    inv: spec_fn(int, S) -> bool, a: spec_fn(int, Seq<S>, C) -> bool,
    local: spec_fn(int, Seq<S>, C, S) -> bool)
    requires pre.len() == n, joint(pre, inv), local_check(n, inv, a, local),
        routed_step(pre, post, k, c, a, local),
    ensures post.len() == n, joint(post, inv),
{
    // The entire peer conjunction comes from the established SAME pre-state.
    assert(peer_invariants(pre, inv, k));
    assert(inv(k, pre[k]));
    assert(local(k, pre, c, post[k]));
    assert(inv(k, post[k]));
    assert forall|i: int| 0 <= i < post.len() implies inv(i, post[i]) by {
        if i != k { assert(post[i] == pre[i]); }
    }
}
pub proof fn joint_history_at<S, C>(n: nat, states: Seq<Seq<S>>, routes: Seq<int>,
    commands: Seq<C>, init: spec_fn(int, S) -> bool, inv: spec_fn(int, S) -> bool,
    a: spec_fn(int, Seq<S>, C) -> bool, local: spec_fn(int, Seq<S>, C, S) -> bool, t: nat)
    requires states.len() > 0, states[0].len() == n, t < states.len(),
        init_check(n, init, inv), local_check(n, inv, a, local),
        forall|i: int| 0 <= i < n ==> #[trigger] init(i, states[0][i]),
        history(states, routes, commands, a, local),
    ensures states[t as int].len() == n, joint(states[t as int], inv),
    decreases t,
{
    if t == 0 { joint_genesis(n, states[0], init, inv); }
    else {
        joint_history_at(n, states, routes, commands, init, inv, a, local, (t - 1) as nat);
        let j = t as int - 1;
        assert(routed_step(states[j], states[j + 1], routes[j], commands[j], a, local));
        joint_step(n, states[j], states[j + 1], routes[j], commands[j], inv, a, local);
    }
}
pub proof fn joint_induction<S, C>(n: nat, states: Seq<Seq<S>>, routes: Seq<int>,
    commands: Seq<C>, init: spec_fn(int, S) -> bool, inv: spec_fn(int, S) -> bool,
    a: spec_fn(int, Seq<S>, C) -> bool, local: spec_fn(int, Seq<S>, C, S) -> bool)
    requires states.len() > 0, states[0].len() == n,
        init_check(n, init, inv), local_check(n, inv, a, local),
        forall|i: int| 0 <= i < n ==> #[trigger] init(i, states[0][i]),
        history(states, routes, commands, a, local),
    ensures forall|t: int| 0 <= t < states.len() ==>
        states[t].len() == n && #[trigger] joint(states[t], inv),
{
    assert forall|t: int| 0 <= t < states.len()
        implies states[t].len() == n && joint(states[t], inv) by {
        joint_history_at(n, states, routes, commands, init, inv, a, local, t as nat);
    }
}

} // verus!
