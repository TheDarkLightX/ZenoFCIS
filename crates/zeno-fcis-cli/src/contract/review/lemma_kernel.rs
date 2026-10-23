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
    pub proof fn parent_facts(after:Seq<usize>,commits:Seq<bool>,k:int,g:int,seen:Seq<bool>,parent:Seq<Option<(usize,usize)>>,depth:Seq<usize>,s:int)
        requires parents_valid(after,commits,k,g,seen,parent,depth), 0 <= s < seen.len(), seen[s],
        ensures (s == g) == (depth[s] == 0),
            s != g ==> parent[s].is_some()
                && parent[s].unwrap().0 < seen.len() && parent[s].unwrap().1 < k
                && seen[parent[s].unwrap().0 as int]
                && step(after,commits,k,parent[s].unwrap().0 as int,parent[s].unwrap().1 as int) == s
                && depth[parent[s].unwrap().0 as int]+1 == depth[s],
    {}
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

#[cfg(verus_keep_ghost)]
verus! {
    pub open spec fn bad(holds:Seq<bool>,after:Seq<usize>,commits:Seq<bool>,k:int,j:int) -> bool {
        holds[j/k] && commits[j] && !holds[after[j] as int]
    }
    pub open spec fn failure_count(holds:Seq<bool>,after:Seq<usize>,commits:Seq<bool>,k:int,end:nat) -> nat
        decreases end,
    {
        if end == 0 { 0 } else {
            failure_count(holds,after,commits,k,(end-1) as nat)
                + if bad(holds,after,commits,k,end-1) {1nat} else {0nat}
        }
    }
    pub open spec fn cons(holds:Seq<bool>,after:Seq<usize>,commits:Seq<bool>,k:int) -> bool {
        forall|j:int| 0 <= j < after.len() ==> !bad(holds,after,commits,k,j)
    }
    pub proof fn count_facts(holds:Seq<bool>,after:Seq<usize>,commits:Seq<bool>,k:int,end:nat)
        ensures failure_count(holds,after,commits,k,end) <= end,
            (failure_count(holds,after,commits,k,end) == 0)
                == (forall|j:int| 0 <= j < end ==> !bad(holds,after,commits,k,j)),
        decreases end,
    {
        if end > 0 { count_facts(holds,after,commits,k,(end-1) as nat); }
    }
    pub open spec fn first_failure(holds:Seq<bool>,after:Seq<usize>,commits:Seq<bool>,k:int,end:int,first:Option<usize>) -> bool {
        match first {
            None => forall|j:int| 0 <= j < end ==> !bad(holds,after,commits,k,j),
            Some(j) => 0 <= j < end && bad(holds,after,commits,k,j as int)
                && forall|l:int| 0 <= l < j ==> !bad(holds,after,commits,k,l),
        }
    }
}

/// Complete committed-edge count and least ordinal induction counterexample.
pub struct Scan {
    pub count: usize,
    pub first: Option<usize>,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires table_valid(holds@.len() as int,k as int,g as int,after@,commits@),
    ensures result.count == failure_count(holds@,after@,commits@,k as int,after@.len()),
        first_failure(holds@,after@,commits@,k as int,after@.len() as int,result.first),
        (result.count == 0) == cons(holds@,after@,commits@,k as int),
        result.first.is_none() == (result.count == 0),
))]
pub fn consecution(k: usize, g: usize, after: &[usize], commits: &[bool], holds: &[bool]) -> Scan {
    let mut ordinal = 0usize;
    let mut count = 0usize;
    let mut first = None;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant table_valid(holds@.len() as int,k as int,g as int,after@,commits@),
            ordinal <= after@.len(), count <= ordinal,
            count == failure_count(holds@,after@,commits@,k as int,ordinal as nat),
            first_failure(holds@,after@,commits@,k as int,ordinal as int,first),
            first.is_none() == (count == 0),
        decreases after@.len() - ordinal,
    ))]
    while ordinal < after.len() {
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(ordinal / k < holds@.len()) by(nonlinear_arith)
                requires k > 0, ordinal < holds@.len() * k;
        }
        if holds[ordinal / k] && commits[ordinal] && !holds[after[ordinal]] {
            count += 1;
            if first.is_none() { first = Some(ordinal); }
        }
        ordinal += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { count_facts(holds@,after@,commits@,k as int,after@.len()); }
    Scan { count, first }
}

/// Validate all table dimensions and successor indices before making a decision.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == table_valid(n as int,k as int,g as int,after@,commits@),
))]
pub fn valid_tables(n: usize, k: usize, g: usize, after: &[usize], commits: &[bool]) -> bool {
    if n == 0 || k == 0 || g >= n { return false; }
    let total = match n.checked_mul(k) { Some(total) => total, None => return false };
    if after.len() != total || commits.len() != total { return false; }
    let mut j = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant n > 0, k > 0, g < n, (n as int)*(k as int) == after@.len(),
            commits@.len() == after@.len(), j <= after@.len(),
            forall|i:int| 0 <= i < j ==> (#[trigger] after@[i]) < n,
        decreases after@.len() - j,
    ))]
    while j < after.len() {
        if after[j] >= n { return false; }
        j += 1;
    }
    true
}

#[derive(Clone, Copy)]
#[cfg_attr(not(verus_keep_ghost), derive(Debug, Eq, PartialEq))]
pub enum Verdict { False, NotInductive, Inductive }

#[cfg(verus_keep_ghost)]
verus! {
    pub open spec fn init(holds:Seq<bool>, g:int) -> bool { holds[g] }
    pub open spec fn all_reachable_hold(after:Seq<usize>,commits:Seq<bool>,k:int,g:int,holds:Seq<bool>) -> bool {
        forall|s:int| 0 <= s < holds.len() && reach(after,commits,holds.len() as int,k,g,s)
            ==> #[trigger] holds[s]
    }
    pub open spec fn verdict_spec(after:Seq<usize>,commits:Seq<bool>,k:int,g:int,holds:Seq<bool>) -> Verdict {
        if !all_reachable_hold(after,commits,k,g,holds) { Verdict::False }
        else if !cons(holds,after,commits,k) { Verdict::NotInductive }
        else { Verdict::Inductive }
    }
    pub proof fn inductive_sound(after:Seq<usize>,commits:Seq<bool>,k:int,g:int,holds:Seq<bool>)
        requires table_valid(holds.len() as int,k,g,after,commits),
            verdict_spec(after,commits,k,g,holds) == Verdict::Inductive,
        ensures init(holds,g), cons(holds,after,commits,k),
            all_reachable_hold(after,commits,k,g,holds),
    { reach_genesis(after,commits,holds.len() as int,k,g); }

    pub open spec fn selected(seen:Seq<bool>,holds:Seq<bool>,depth:Seq<usize>,end:int,best:Option<usize>) -> bool {
        match best {
            None => forall|s:int| 0 <= s < end ==> !(seen[s] && !holds[s]),
            Some(b) => b < end && seen[b as int] && !holds[b as int]
                && forall|s:int| 0 <= s < end && seen[s] && !holds[s] ==> {
                    &&& #[trigger] depth[b as int] <= #[trigger] depth[s]
                    &&& (depth[b as int] == depth[s] ==> b <= s)
                },
        }
    }
    pub open spec fn walk(after:Seq<usize>,commits:Seq<bool>,k:int,s:int,path:Seq<usize>) -> int
        decreases path.len(),
    {
        if path.len() == 0 { s }
        else if 0 <= s && 0 <= path[0] < k && 0 <= s*k+path[0] < after.len() {
            walk(after,commits,k,step(after,commits,k,s,path[0] as int),path.subrange(1,path.len() as int))
        } else { -1 }
    }
}

/// Least parent depth, then least ordinal, among reachable excluded states.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires seen@.len() == holds@.len(), depth@.len() == holds@.len(),
    ensures selected(seen@,holds@,depth@,holds@.len() as int,result),
))]
pub fn nearest(seen: &[bool], holds: &[bool], depth: &[usize]) -> Option<usize> {
    let mut best = None;
    let mut s = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant seen@.len() == holds@.len(), depth@.len() == holds@.len(),
            s <= holds@.len(), selected(seen@,holds@,depth@,s as int,best),
        decreases holds@.len() - s,
    ))]
    while s < holds.len() {
        if seen[s] && !holds[s] {
            let better = match best { None => true, Some(b) => depth[s] < depth[b] };
            if better { best = Some(s); }
        }
        s += 1;
    }
    best
}

/// Fill the owned trace backwards along decreasing parent depths.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires table_valid(search.seen@.len() as int,k as int,g as int,after@,commits@),
        parents_valid(after@,commits@,k as int,g as int,search.seen@,search.parent@,search.depth@),
        target < search.seen@.len(), search.seen@[target as int],
    ensures result@.len() == search.depth@[target as int],
        walk(after@,commits@,k as int,g as int,result@) == target,
        forall|j:int| 0 <= j < result@.len() ==> (#[trigger] result@[j]) < k,
        target != g ==> result@.len() > 0
            && result@[result@.len()-1] == search.parent@[target as int].unwrap().1
            && walk(after@,commits@,k as int,g as int,result@.subrange(0,result@.len() as int-1))
                == search.parent@[target as int].unwrap().0,
))]
pub fn trace(k: usize, g: usize, after: &[usize], commits: &[bool], search: &Search, target: usize) -> Vec<usize> {
    let mut remaining = search.depth[target];
    let mut path = vec![0; remaining];
    let mut state = target;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant table_valid(search.seen@.len() as int,k as int,g as int,after@,commits@),
            parents_valid(after@,commits@,k as int,g as int,search.seen@,search.parent@,search.depth@),
            target < search.seen@.len(), search.seen@[target as int],
            state < search.seen@.len(), search.seen@[state as int],
            path@.len() == search.depth@[target as int], remaining <= path@.len(),
            search.depth@[state as int] == remaining,
            walk(after@,commits@,k as int,state as int,path@.subrange(remaining as int,path@.len() as int)) == target,
            forall|j:int| remaining <= j < path@.len() ==> (#[trigger] path@[j]) < k,
            remaining < path@.len() ==> path@[path@.len()-1] == search.parent@[target as int].unwrap().1
                && walk(after@,commits@,k as int,state as int,path@.subrange(remaining as int,path@.len() as int-1))
                    == search.parent@[target as int].unwrap().0,
        decreases remaining,
    ))]
    while remaining > 0 {
        #[cfg(verus_keep_ghost)]
        proof! {
            parent_facts(after@,commits@,k as int,g as int,search.seen@,search.parent@,search.depth@,state as int);
        }
        let (before, input) = match search.parent[state] {
            Some(pair) => pair,
            None => return path,
        };
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost old_path = path@; }
        path[remaining - 1] = input;
        #[cfg(verus_keep_ghost)]
        proof! {
            assert((before as int)*k+input < after@.len()) by(nonlinear_arith)
                requires before < search.seen@.len(), input < k, k > 0,
                    after@.len() == search.seen@.len()*k;
            assert(path@.subrange(remaining as int,path@.len() as int)
                =~= old_path.subrange(remaining as int,old_path.len() as int));
            assert(path@.subrange(remaining as int-1,path@.len() as int).subrange(1,path@.len() as int-remaining as int+1)
                =~= old_path.subrange(remaining as int,old_path.len() as int));
            assert(walk(after@,commits@,k as int,before as int,path@.subrange(remaining as int-1,path@.len() as int)) == target);
            if remaining < path@.len() {
                assert(path@.subrange(remaining as int-1,path@.len() as int-1).subrange(1,path@.len() as int-remaining as int)
                    =~= old_path.subrange(remaining as int,old_path.len() as int-1));
                assert(walk(after@,commits@,k as int,before as int,path@.subrange(remaining as int-1,path@.len() as int-1))
                    == search.parent@[target as int].unwrap().0);
            } else { assert(state == target); }
        }
        state = before;
        remaining -= 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        parent_facts(after@,commits@,k as int,g as int,search.seen@,search.parent@,search.depth@,state as int);
        assert(state == g); assert(path@.subrange(0,path@.len() as int) =~= path@);
    }
    path
}

/// Decision plus owned witnesses; no table reference escapes the invocation.
pub struct Decision {
    pub verdict: Verdict,
    pub initiation: bool,
    pub search: Search,
    pub count: usize,
    pub counterexample: Option<(usize, usize, usize)>,
    pub target: Option<usize>,
    pub trace: Vec<usize>,
}

#[cfg(verus_keep_ghost)]
verus! {
    pub open spec fn decision_spec(d:Decision,after:Seq<usize>,commits:Seq<bool>,k:int,g:int,holds:Seq<bool>) -> bool {
        &&& d.verdict == verdict_spec(after,commits,k,g,holds)
        &&& d.initiation == init(holds,g)
        &&& d.count == failure_count(holds,after,commits,k,after.len())
        &&& d.search.seen@.len() == holds.len()
        &&& parents_valid(after,commits,k,g,d.search.seen@,d.search.parent@,d.search.depth@)
        &&& (forall|s:int| 0 <= s < holds.len() ==> (#[trigger] d.search.seen@[s])
            == reach(after,commits,holds.len() as int,k,g,s))
        &&& match d.verdict {
            Verdict::False => {
                &&& d.target.is_some()
                &&& {
                    let t=d.target.unwrap() as int;
                    &&& 0 <= t < holds.len() && d.search.seen@[t] && !holds[t]
                    &&& walk(after,commits,k,g,d.trace@) == t
                    &&& d.trace@.len() == d.search.depth@[t]
                    &&& (forall|j:int| 0 <= j < d.trace@.len() ==> (#[trigger] d.trace@[j]) < k)
                    &&& if !holds[g] { t == g && d.trace@.len() == 0 && d.counterexample.is_none() }
                        else { match d.counterexample {
                            None => false,
                            Some((s,i,u)) => s < holds.len() && i < k && u == t && holds[s as int]
                                && d.search.seen@[s as int] && commits[s*k+i]
                                && step(after,commits,k,s as int,i as int) == t
                                && d.trace@.len() > 0 && d.trace@[d.trace@.len()-1] == i
                                && walk(after,commits,k,g,d.trace@.subrange(0,d.trace@.len() as int-1)) == s,
                        } }
                }
            },
            Verdict::NotInductive => d.target.is_none() && d.trace@.len() == 0 && d.count > 0
                && match d.counterexample {
                    None => false,
                    Some((s,i,u)) => s < holds.len() && i < k && u == after[s*k+i]
                        && holds[s as int] && commits[s*k+i] && !holds[u as int]
                        && first_failure(holds,after,commits,k,after.len() as int,Some((s*k+i) as usize)),
                },
            Verdict::Inductive => init(holds,g) && cons(holds,after,commits,k)
                && all_reachable_hold(after,commits,k,g,holds)
                && d.count == 0 && d.counterexample.is_none() && d.target.is_none() && d.trace@.len() == 0,
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.is_some() == table_valid(holds@.len() as int,k as int,g as int,after@,commits@),
        match result { None => true, Some(d) => decision_spec(d,after@,commits@,k as int,g as int,holds@) },
))]
pub fn decide(k: usize, g: usize, after: &[usize], commits: &[bool], holds: &[bool]) -> Option<Decision> {
    let n = holds.len();
    if !valid_tables(n,k,g,after,commits) { return None; }
    #[cfg(verus_keep_ghost)]
    proof! { reach_genesis(after@,commits@,n as int,k as int,g as int); }
    let search = reachable(n,k,g,after,commits);
    let scan = consecution(k,g,after,commits,holds);
    let initiation = holds[g];
    let violation = nearest(&search.seen,holds,&search.depth);
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(violation.is_none() == all_reachable_hold(after@,commits@,k as int,g as int,holds@));
    }
    let (verdict, target, counterexample, path) = if !initiation {
        (Verdict::False,Some(g),None,Vec::new())
    } else if let Some(state) = violation {
        #[cfg(verus_keep_ghost)]
        proof! { parent_facts(after@,commits@,k as int,g as int,search.seen@,search.parent@,search.depth@,state as int); }
        let (before,input) = match search.parent[state] {
            Some(pair) => pair,
            None => return None,
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(holds@[before as int]);
            assert(commits@[before as int*k+input]);
        }
        let path = trace(k,g,after,commits,&search,state);
        (Verdict::False,Some(state),Some((before,input,state)),path)
    } else if let Some(ordinal) = scan.first {
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(ordinal/k < n) by(nonlinear_arith) requires k>0, ordinal < n*k;
            assert((ordinal/k)*k+ordinal%k == ordinal) by(nonlinear_arith) requires k>0;
        }
        (Verdict::NotInductive,None,Some((ordinal/k,ordinal%k,after[ordinal])),Vec::new())
    } else {
        #[cfg(verus_keep_ghost)]
        proof! { inductive_sound(after@,commits@,k as int,g as int,holds@); }
        (Verdict::Inductive,None,None,Vec::new())
    };
    Some(Decision { verdict, initiation, search, count:scan.count, counterexample, target, trace:path })
}
