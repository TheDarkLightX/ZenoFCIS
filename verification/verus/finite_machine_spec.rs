//! Finite machines over declared state/input ordinals, independent of a judge.
//! Fixed-point induction: D. Park, "Fixpoint induction and proofs of program
//! properties", Machine Intelligence 5 (1969); seL4 refinement organization.
use vstd::prelude::*;

verus! {
    pub(crate) open spec fn table_valid(n: int, k: int, g: int, after: Seq<usize>, commits: Seq<bool>) -> bool {
        0 < n && 0 < k && 0 <= g < n && n * k <= usize::MAX
        && after.len() == n * k && commits.len() == after.len()
        && forall|j: int| 0 <= j < after.len() ==> (#[trigger] after[j]) < n
    }
    pub(crate) open spec fn step(after: Seq<usize>, commits: Seq<bool>, k: int, s: int, i: int) -> int {
        if commits[s*k+i] { after[s*k+i] as int } else { s }
    }
    pub(crate) open spec fn closed(after: Seq<usize>, commits: Seq<bool>, k: int, set: Seq<bool>) -> bool {
        forall|s: int, i: int| 0 <= s < set.len() && 0 <= i < k && set[s]
            ==> #[trigger] set[step(after, commits, k, s, i)]
    }
    pub(crate) open spec fn reach(after: Seq<usize>, commits: Seq<bool>, n: int, k: int, g: int, s: int) -> bool {
        0 <= s < n && forall|set: Seq<bool>| set.len() == n && set[g]
            && closed(after, commits, k, set) ==> #[trigger] set[s]
    }
    pub(crate) proof fn reach_genesis(after: Seq<usize>, commits: Seq<bool>, n: int, k: int, g: int)
        requires table_valid(n,k,g,after,commits),
        ensures reach(after,commits,n,k,g,g),
    {}
    pub(crate) proof fn reach_step(after: Seq<usize>, commits: Seq<bool>, n: int, k: int, g: int, s: int, i: int)
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

    pub(crate) open spec fn state(n:int, s:int) -> bool { 0 <= s < n }
    pub(crate) open spec fn input(k:int, i:int) -> bool { 0 <= i < k }
    pub(crate) open spec fn subset(left:Seq<bool>, right:Seq<bool>) -> bool {
        left.len() == right.len()
        && forall|s:int| 0 <= s < left.len() && left[s] ==> #[trigger] right[s]
    }
    pub(crate) open spec fn post(after:Seq<usize>, commits:Seq<bool>, k:int, set:Seq<bool>, target:int) -> bool {
        exists|s:int,i:int| 0 <= s < set.len() && 0 <= i < k && set[s]
            && step(after,commits,k,s,i) == target
    }
    pub(crate) open spec fn image(after:Seq<usize>, commits:Seq<bool>, k:int, g:int, set:Seq<bool>) -> Seq<bool> {
        Seq::new(set.len(), |s:int| s == g || post(after,commits,k,set,s))
    }
    pub(crate) open spec fn prefixpoint(after:Seq<usize>, commits:Seq<bool>, k:int, g:int, set:Seq<bool>) -> bool {
        subset(image(after,commits,k,g,set),set)
    }
    pub(crate) open spec fn reach_set(after:Seq<usize>, commits:Seq<bool>, n:int, k:int, g:int) -> Seq<bool> {
        Seq::new(n as nat, |s:int| reach(after,commits,n,k,g,s))
    }
    pub(crate) open spec fn walk(after:Seq<usize>,commits:Seq<bool>,k:int,s:int,path:Seq<usize>) -> int
        decreases path.len(),
    {
        if path.len() == 0 { s }
        else if 0 <= s && 0 <= path[0] < k && 0 <= s*k+path[0] < after.len() {
            walk(after,commits,k,step(after,commits,k,s,path[0] as int),path.subrange(1,path.len() as int))
        } else { -1 }
    }
}
