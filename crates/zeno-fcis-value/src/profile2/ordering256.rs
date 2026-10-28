//! High-limb-first comparison shared by fixed-width arithmetic and its proof.
use core::cmp::Ordering;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "ordering_model.rs"]
pub(crate) mod ordering_model;

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures ordering_model::lex_result(a@, b@, result),
))]
pub(super) fn compare<const N: usize>(a: &[u128; N], b: &[u128; N]) -> Ordering {
    let mut i = N;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i <= N,
            a@.len() == N, b@.len() == N,
            ordering_model::suffix_equal(a@, b@, i as int),
        decreases i,
    ))]
    while i > 0 {
        i -= 1;
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(ordering_model::suffix_equal(a@, b@, i as int + 1));
        }
        if a[i] != b[i] {
            #[cfg(verus_keep_ghost)]
            proof! {
                let pivot = i as int;
                // Every possible highest mismatch is this actual loop index.
                assert forall|j: int| 0 <= j < N
                    && ordering_model::suffix_equal(a@, b@, j + 1)
                    && a@[j] != b@[j]
                    implies j == pivot by {
                    if j < pivot {
                        assert(a@[pivot] == b@[pivot]);
                    } else if j > pivot {
                        assert(a@[j] == b@[j]);
                    }
                }
                if a@[i as int] < b@[i as int] {
                    assert(exists|j: int| 0 <= j < N && a@[j] < b@[j]
                        && ordering_model::suffix_equal(a@, b@, j + 1));
                } else {
                    assert(exists|j: int| 0 <= j < N && a@[j] > b@[j]
                        && ordering_model::suffix_equal(a@, b@, j + 1));
                }
                assert(ordering_model::lex_lt(a@, b@) == (a@[i as int] < b@[i as int]));
                assert(ordering_model::lex_gt(a@, b@) == (a@[i as int] > b@[i as int]));
                assert(a@ != b@);
            }
            return a[i].cmp(&b[i]);
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|j: int| i <= j < N implies a@[j] == b@[j] by {
                if j == i { assert(a@[i as int] == b@[i as int]); }
            }
        }
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(a@ =~= b@); }
    Ordering::Equal
}
