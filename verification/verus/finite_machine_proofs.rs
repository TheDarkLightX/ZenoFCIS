//! Park induction and the least fixed point for the shared finite-machine spec.
//! D. Park, "Fixpoint induction and proofs of program properties", MI 5 (1969).
use super::finite_machine_spec::*;
use vstd::prelude::*;

verus! {
    pub(crate) proof fn step_in_domain(after:Seq<usize>,commits:Seq<bool>,n:int,k:int,g:int,s:int,i:int)
        requires table_valid(n,k,g,after,commits), 0 <= s < n, 0 <= i < k,
        ensures 0 <= step(after,commits,k,s,i) < n,
    {
        assert(0 <= s*k+i < n*k) by(nonlinear_arith)
            requires 0 <= s < n, 0 <= i < k, 0 < k;
    }

    pub(crate) proof fn prefixpoint_is_init_closed(after:Seq<usize>,commits:Seq<bool>,k:int,g:int,set:Seq<bool>)
        requires table_valid(set.len() as int,k,g,after,commits),
        ensures prefixpoint(after,commits,k,g,set) == (set[g] && closed(after,commits,k,set)),
    {
        let next = image(after,commits,k,g,set);
        if prefixpoint(after,commits,k,g,set) {
            assert(next[g]);
            assert(set[g]);
            assert forall|s:int,i:int| 0 <= s < set.len() && 0 <= i < k && set[s]
                implies #[trigger] set[step(after,commits,k,s,i)] by {
                step_in_domain(after,commits,set.len() as int,k,g,s,i);
                let target = step(after,commits,k,s,i);
                assert(post(after,commits,k,set,target));
                assert(next[target]);
            }
        }
        if set[g] && closed(after,commits,k,set) {
            assert forall|target:int| 0 <= target < set.len() && next[target]
                implies #[trigger] set[target] by {
                if target != g {
                    let (s,i) = choose|s:int,i:int| 0 <= s < set.len() && 0 <= i < k && set[s]
                        && step(after,commits,k,s,i) == target;
                    assert(set[step(after,commits,k,s,i)]);
                }
            }
        }
    }

    pub(crate) proof fn park_induction(after:Seq<usize>,commits:Seq<bool>,k:int,g:int,set:Seq<bool>)
        requires table_valid(set.len() as int,k,g,after,commits), prefixpoint(after,commits,k,g,set),
        ensures subset(reach_set(after,commits,set.len() as int,k,g),set),
    {
        prefixpoint_is_init_closed(after,commits,k,g,set);
        assert forall|s:int| 0 <= s < set.len() && reach(after,commits,set.len() as int,k,g,s)
            implies #[trigger] set[s] by {}
    }

    pub(crate) proof fn reach_is_fixed_point(after:Seq<usize>,commits:Seq<bool>,n:int,k:int,g:int)
        requires table_valid(n,k,g,after,commits),
        ensures image(after,commits,k,g,reach_set(after,commits,n,k,g)) == reach_set(after,commits,n,k,g),
    {
        let r = reach_set(after,commits,n,k,g);
        reach_genesis(after,commits,n,k,g);
        assert(r[g]);
        assert forall|s:int,i:int| 0 <= s < n && 0 <= i < k && r[s]
            implies #[trigger] r[step(after,commits,k,s,i)] by {
            reach_step(after,commits,n,k,g,s,i);
        }
        prefixpoint_is_init_closed(after,commits,k,g,r);
        let f = image(after,commits,k,g,r);
        assert(subset(f,r));
        assert(f[g]);
        assert forall|s:int,i:int| 0 <= s < n && 0 <= i < k && f[s]
            implies #[trigger] f[step(after,commits,k,s,i)] by {
            step_in_domain(after,commits,n,k,g,s,i);
            assert(r[s]);
            assert(post(after,commits,k,r,step(after,commits,k,s,i)));
        }
        assert forall|s:int| 0 <= s < n && r[s] implies #[trigger] f[s] by {
            assert(reach(after,commits,n,k,g,s));
        }
        assert(f =~= r);
    }

    pub(crate) proof fn reach_is_least_fixed_point(after:Seq<usize>,commits:Seq<bool>,n:int,k:int,g:int,set:Seq<bool>)
        requires table_valid(n,k,g,after,commits), set.len() == n,
            image(after,commits,k,g,set) == set,
        ensures image(after,commits,k,g,reach_set(after,commits,n,k,g)) == reach_set(after,commits,n,k,g),
            subset(reach_set(after,commits,n,k,g),set),
    {
        reach_is_fixed_point(after,commits,n,k,g);
        park_induction(after,commits,k,g,set);
    }

    pub(crate) open spec fn effective_trace(after:Seq<usize>,commits:Seq<bool>,n:int,k:int,s:int,path:Seq<usize>) -> bool
        decreases path.len(),
    {
        0 <= s < n && (path.len() == 0 || (0 <= path[0] < k
            && effective_trace(after,commits,n,k,step(after,commits,k,s,path[0] as int),path.subrange(1,path.len() as int))))
    }

    pub(crate) proof fn trace_stays_in_domain(after:Seq<usize>,commits:Seq<bool>,n:int,k:int,g:int,s:int,path:Seq<usize>)
        requires table_valid(n,k,g,after,commits), 0 <= s < n,
            forall|j:int| 0 <= j < path.len() ==> (#[trigger] path[j]) < k,
        ensures effective_trace(after,commits,n,k,s,path), 0 <= walk(after,commits,k,s,path) < n,
        decreases path.len(),
    {
        if path.len() > 0 {
            step_in_domain(after,commits,n,k,g,s,path[0] as int);
            assert(0 <= s*k+path[0] < after.len()) by(nonlinear_arith)
                requires 0 <= s < n, 0 <= path[0] < k, 0 < k, after.len() == n*k;
            let tail = path.subrange(1,path.len() as int);
            assert forall|j:int| 0 <= j < tail.len() implies (#[trigger] tail[j]) < k by {
                assert(tail[j] == path[j+1]);
            }
            trace_stays_in_domain(after,commits,n,k,g,step(after,commits,k,s,path[0] as int),tail);
        }
    }
}
