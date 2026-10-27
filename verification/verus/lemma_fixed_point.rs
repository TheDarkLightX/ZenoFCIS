//! Fixed-point forms of the lemma judge's three verdicts (Park induction).
use super::finite_machine_proofs::*;
use super::finite_machine_spec::*;
use super::lemma_kernel::{self, Verdict};
use vstd::prelude::*;

verus! {
    pub(crate) proof fn verdict_fixed_point_form(after:Seq<usize>,commits:Seq<bool>,k:int,g:int,holds:Seq<bool>)
        requires table_valid(holds.len() as int,k,g,after,commits),
        ensures (lemma_kernel::verdict_spec(after,commits,k,g,holds) == Verdict::Inductive)
                == prefixpoint(after,commits,k,g,holds),
            (lemma_kernel::verdict_spec(after,commits,k,g,holds) == Verdict::NotInductive)
                == (subset(reach_set(after,commits,holds.len() as int,k,g),holds) && !prefixpoint(after,commits,k,g,holds)),
            (lemma_kernel::verdict_spec(after,commits,k,g,holds) == Verdict::False)
                == !subset(reach_set(after,commits,holds.len() as int,k,g),holds),
    {
        let n = holds.len() as int;
        lemma_kernel::consecution_equivalence(holds,after,commits,k,g);
        prefixpoint_is_init_closed(after,commits,k,g,holds);
        assert(closed(after,commits,k,holds) == lemma_kernel::cons_by_state(holds,after,commits,k)) by {
            if closed(after,commits,k,holds) {
                assert forall|s:int,i:int| 0 <= s < n && 0 <= i < k && holds[s] && commits[s*k+i]
                    implies #[trigger] holds[after[s*k+i] as int] by {
                    assert(step(after,commits,k,s,i) == after[s*k+i]);
                }
            }
            if lemma_kernel::cons_by_state(holds,after,commits,k) {
                assert forall|s:int,i:int| 0 <= s < n && 0 <= i < k && holds[s]
                    implies #[trigger] holds[step(after,commits,k,s,i)] by {
                    if commits[s*k+i] { assert(holds[after[s*k+i] as int]); }
                }
            }
        }
        if prefixpoint(after,commits,k,g,holds) { park_induction(after,commits,k,g,holds); }
        reach_genesis(after,commits,n,k,g);
        assert(subset(reach_set(after,commits,n,k,g),holds) == lemma_kernel::all_reachable_hold(after,commits,k,g,holds));
    }
}
