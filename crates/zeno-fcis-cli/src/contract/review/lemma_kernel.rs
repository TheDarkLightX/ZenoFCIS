//! Finite state-lemma decisions over immutable tables; no evaluator or rendering.
//! Specification/refinement follows Klein et al., seL4 (SOSP 2009).
extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {
    pub open spec fn table_valid(n: int, k: int, g: int, after: Seq<usize>, commits: Seq<bool>) -> bool {
        0 < n && 0 < k && 0 <= g < n && n * k <= usize::MAX
        && after.len() == n * k && commits.len() == after.len()
        && forall|j: int| 0 <= j < after.len() ==> (#[trigger] after[j]) < n
    }
    pub open spec fn step(after: Seq<usize>, commits: Seq<bool>, k: int, s: int, i: int) -> int {
        if commits[s*k+i] { after[s*k+i] as int } else { s }
    }
    pub open spec fn closed(after: Seq<usize>, commits: Seq<bool>, k: int, set: Seq<bool>) -> bool {
        forall|s: int, i: int| 0 <= s < set.len() && 0 <= i < k && set[s]
            ==> #[trigger] set[step(after, commits, k, s, i)]
    }
    pub open spec fn reach(after: Seq<usize>, commits: Seq<bool>, n: int, k: int, g: int, s: int) -> bool {
        0 <= s < n && forall|set: Seq<bool>| set.len() == n && set[g]
            && closed(after, commits, k, set) ==> #[trigger] set[s]
    }
    pub proof fn reach_genesis(after: Seq<usize>, commits: Seq<bool>, n: int, k: int, g: int)
        requires table_valid(n,k,g,after,commits),
        ensures reach(after,commits,n,k,g,g),
    {}
    pub proof fn reach_step(after: Seq<usize>, commits: Seq<bool>, n: int, k: int, g: int, s: int, i: int)
        requires table_valid(n,k,g,after,commits), reach(after,commits,n,k,g,s), 0 <= i < k,
        ensures reach(after,commits,n,k,g,step(after,commits,k,s,i)),
    {
        assert(0 <= s*k+i < n*k) by(nonlinear_arith)
            requires 0 <= s < n, 0 <= i < k, 0 < k;
        assert forall|set: Seq<bool>| set.len() == n && set[g] && closed(after,commits,k,set)
            implies #[trigger] set[step(after,commits,k,s,i)] by {
            assert(set[s]);
        }
    }
    pub proof fn bounded_distinct(q:Seq<usize>, n:int)
        requires 0 <= n,
            forall|j:int| 0 <= j < q.len() ==> (#[trigger] q[j]) < n,
            forall|a:int,b:int| 0 <= a < b < q.len() ==> #[trigger] q[a] != #[trigger] q[b],
        ensures q.len() <= n,
        decreases n,
    {
        if q.len() > 0 {
            let last = q[q.len()-1];
            let r = Seq::new((q.len()-1) as nat, |j:int| if q[j] < last {q[j]} else {(q[j]-1) as usize});
            assert forall|j:int| 0 <= j < r.len() implies (#[trigger] r[j]) < n-1 by {
                assert(q[j] != last);
            }
            assert forall|a:int,b:int| 0 <= a < b < r.len() implies #[trigger] r[a] != #[trigger] r[b] by {
                assert(q[a] != last && q[b] != last && q[a] != q[b]);
            }
            bounded_distinct(r,n-1);
        }
    }
    pub proof fn contains_push(q:Seq<usize>, x:usize, s:usize)
        ensures q.push(x).contains(s) == (s == x || q.contains(s)),
    {
        if q.push(x).contains(s) {
            let j = choose|j:int| 0 <= j < q.push(x).len() && q.push(x)[j] == s;
            if j < q.len() { assert(q.contains(s)); }
        }
        if q.contains(s) {
            let j = choose|j:int| 0 <= j < q.len() && q[j] == s;
            assert(q.push(x)[j] == s);
            assert(q.push(x).contains(s));
        }
        if s == x {
            assert(q.push(x)[q.len() as int] == s);
            assert(q.push(x).contains(s));
        }
    }
    pub open spec fn discovered(seen: Seq<bool>, queue: Seq<usize>) -> bool {
        (forall|a:int,b:int| 0 <= a < b < queue.len() ==> #[trigger] queue[a] != #[trigger] queue[b])
        && (forall|j:int| 0 <= j < queue.len() ==> (#[trigger] queue[j]) < seen.len())
        && (forall|s:int| 0 <= s < seen.len() ==> (#[trigger] seen[s]) == queue.contains(s as usize))
    }
    pub open spec fn parents_valid(after:Seq<usize>, commits:Seq<bool>, k:int, g:int,
        seen:Seq<bool>, parent:Seq<Option<(usize,usize)>>, depth:Seq<usize>) -> bool {
        parent.len() == seen.len() && depth.len() == seen.len()
        && seen[g] && parent[g].is_none() && depth[g] == 0
        && forall|s:int| 0 <= s < seen.len() && seen[s] && s != g ==> {
            &&& (#[trigger] parent[s]).is_some()
            &&& {
                let p = parent[s].unwrap();
                &&& p.0 < seen.len() && p.1 < k
                &&& seen[p.0 as int]
                &&& step(after,commits,k,p.0 as int,p.1 as int) == s
                &&& depth[p.0 as int] + 1 == depth[s]
            }
        }
    }
    pub open spec fn frontier(queue:Seq<usize>, depth:Seq<usize>, at:int) -> bool {
        0 <= at <= queue.len()
        && (forall|j:int| 0 <= j < queue.len() ==> #[trigger] depth[queue[j] as int] <= j)
        && (forall|j:int,l:int| 0 <= j <= l < queue.len()
            ==> #[trigger] depth[queue[j] as int] <= #[trigger] depth[queue[l] as int])
        && (at < queue.len() ==> forall|j:int| at <= j < queue.len()
            ==> #[trigger] depth[queue[j] as int] <= depth[queue[at] as int]+1)
    }
    pub open spec fn sound_seen(after:Seq<usize>,commits:Seq<bool>,n:int,k:int,g:int,seen:Seq<bool>) -> bool {
        forall|s:int| 0 <= s < n && (#[trigger] seen[s]) ==> reach(after,commits,n,k,g,s)
    }
    pub open spec fn processed(after:Seq<usize>,commits:Seq<bool>,k:int,
        seen:Seq<bool>,queue:Seq<usize>,at:int) -> bool {
        forall|j:int,i:int| 0 <= j < at && 0 <= i < k
            ==> #[trigger] seen[step(after,commits,k,queue[j] as int,i)]
    }
}

/// Owned search result. Parents are indexed by state, with no parent at genesis.
pub struct Search {
    pub seen: Vec<bool>,
    pub parent: Vec<Option<(usize, usize)>>,
    pub depth: Vec<usize>,
}

/// FIFO discovery, scanning inputs in ascending ordinal order.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires table_valid(n as int,k as int,g as int,after@,commits@),
    ensures result.seen@.len() == n,
        parents_valid(after@,commits@,k as int,g as int,result.seen@,result.parent@,result.depth@),
        forall|s:int| 0 <= s < n ==> (#[trigger] result.seen@[s]) == reach(after@,commits@,n as int,k as int,g as int,s),
))]
pub fn reachable(n: usize, k: usize, g: usize, after: &[usize], commits: &[bool]) -> Search {
    #[cfg(verus_keep_ghost)]
    proof_decl! { broadcast use vstd::seq::group_seq_lemmas; }
    let mut seen = vec![false; n];
    let mut parent = vec![None; n];
    let mut depth = vec![0usize; n];
    let mut queue = Vec::new();
    seen[g] = true;
    queue.push(g);
    let mut at = 0usize;
    #[cfg(verus_keep_ghost)]
    proof! {
        reach_genesis(after@,commits@,n as int,k as int,g as int);
        assert forall|s:int| 0 <= s < n implies (#[trigger] seen@[s]) == queue@.contains(s as usize) by {
            contains_push(Seq::empty(),g,s as usize);
            assert(queue@ == Seq::<usize>::empty().push(g));
        }
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant table_valid(n as int,k as int,g as int,after@,commits@),
            seen@.len() == n, parent@.len() == n, depth@.len() == n,
            discovered(seen@,queue@), queue@.len() <= n,
            parents_valid(after@,commits@,k as int,g as int,seen@,parent@,depth@),
            frontier(queue@,depth@,at as int),
            sound_seen(after@,commits@,n as int,k as int,g as int,seen@),
            processed(after@,commits@,k as int,seen@,queue@,at as int),
        decreases n - at,
    ))]
    while at < queue.len() {
        let state = queue[at];
        let mut input = 0usize;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant table_valid(n as int,k as int,g as int,after@,commits@),
                seen@.len() == n, parent@.len() == n, depth@.len() == n,
                discovered(seen@,queue@), queue@.len() <= n,
                parents_valid(after@,commits@,k as int,g as int,seen@,parent@,depth@),
                frontier(queue@,depth@,at as int), at < queue@.len(), queue@[at as int] == state,
                sound_seen(after@,commits@,n as int,k as int,g as int,seen@),
                processed(after@,commits@,k as int,seen@,queue@,at as int), input <= k,
                forall|i:int| 0 <= i < input ==> #[trigger] seen@[step(after@,commits@,k as int,state as int,i)],
            decreases k - input,
        ))]
        while input < k {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert((state as int)*(k as int)+input < (n as int)*(k as int)) by(nonlinear_arith)
                    requires state < n, input < k;
                assert(queue@.contains(state));
                assert(seen@[state as int]);
                reach_step(after@,commits@,n as int,k as int,g as int,state as int,input as int);
            }
            let ordinal = state * k + input;
            let next = if commits[ordinal] { after[ordinal] } else { state };
            if !seen[next] {
                #[cfg(verus_keep_ghost)]
                proof_decl! { let ghost old_queue = queue@; }
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(!queue@.contains(next));
                    bounded_distinct(queue@.push(next),n as int);
                }
                seen[next] = true;
                parent[next] = Some((state, input));
                depth[next] = depth[state] + 1;
                queue.push(next);
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert forall|s:int| 0 <= s < n implies (#[trigger] seen@[s]) == queue@.contains(s as usize) by {
                        contains_push(old_queue,next,s as usize);
                    }
                }
            }
            input += 1;
        }
        at += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(closed(after@,commits@,k as int,seen@)) by {
            assert forall|s:int,i:int| 0 <= s < n && 0 <= i < k && seen@[s]
                implies #[trigger] seen@[step(after@,commits@,k as int,s,i)] by {
                let j = choose|j:int| 0 <= j < queue@.len() && queue@[j] == s as usize;
            }
        }
        assert forall|s:int| 0 <= s < n
            implies (#[trigger] seen@[s]) == reach(after@,commits@,n as int,k as int,g as int,s) by {
            if reach(after@,commits@,n as int,k as int,g as int,s) { assert(seen@[s]); }
        }
    }
    Search { seen, parent, depth }
}
