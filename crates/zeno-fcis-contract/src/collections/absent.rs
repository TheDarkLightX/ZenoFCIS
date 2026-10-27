//! Canonical view of an absent record.

use crate::internals::review::domain::Position;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {
pub open spec fn absent_value(positions: Seq<Position>, presence: usize,
    values: Seq<usize>, before: Seq<i128>, index: int) -> i128 {
    if index == presence { 0 }
    else if values.contains(index as usize) { crate::internals::review::domain::least_or_zero(positions[index].domain) }
    else { before[index] }
}
}
#[cfg_attr(verus_keep_ghost,verus_spec(
    requires presence < old(view)@.len(), positions@.len() == old(view)@.len(),
        forall|i:int| 0 <= i < values@.len() ==> values@[i] < old(view)@.len() && values@[i] != presence,
    ensures final(view)@.len() == old(view)@.len(),
        forall|i:int| 0 <= i < final(view)@.len() ==> final(view)@[i] ==
            absent_value(positions@, presence, values@, old(view)@, i),
))]
pub(super) fn canonical_absent_view(
    positions: &[Position],
    presence: usize,
    values: &[usize],
    view: &mut [i128],
) {
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost initial = view@; }
    view[presence] = 0;
    let mut processed = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(
        invariant processed <= values.len(), initial == old(view)@,
            presence < view@.len(), positions@.len() == view@.len(), view@.len() == initial.len(),
            forall|i:int| 0 <= i < values@.len() ==> values@[i] < view@.len() && values@[i] != presence,
            forall|i:int| 0 <= i < view@.len() ==> view@[i] ==
                absent_value(positions@, presence, values@.take(processed as int), initial, i),
        decreases values.len() - processed,
    ))]
    while processed < values.len() {
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost previous = view@; }
        let position = values[processed];
        view[position] = positions[position].domain.least().unwrap_or(0);
        processed += 1;
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(values@.take(processed as int) =~=
                values@.take(processed as int - 1).push(values@[processed as int - 1]));
            broadcast use vstd::seq_lib::lemma_seq_contains_after_push;
            assert forall|i:int| 0 <= i < view@.len() implies view@[i] ==
                absent_value(positions@, presence, values@.take(processed as int), initial, i) by {
                assert(values@.take(processed as int).contains(i as usize) ==
                    (values@.take(processed as int - 1).contains(i as usize) || i == position));
                if i != position {
                    assert(previous[i] == absent_value(positions@, presence,
                        values@.take(processed as int - 1), initial, i));
                }
            }
        }
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(values@.take(values@.len() as int) =~= values@); }
}
