//! Shared total byte, ASCII and identifier-membership checks used across the core.
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (left@ == right@),))]
pub(super) fn bytes_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= left.len(), left.len() == right.len(),
        forall|j:int| 0 <= j < i ==> left@[j] == right@[j], decreases left.len() - i,))]
    while i < left.len() {
        if left[i] != right[i] {
            return false;
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(left@ =~= right@); }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (forall|j:int| 0 <= j < bytes@.len() ==> bytes@[j] < 128),))]
pub(super) fn ascii(bytes: &[u8]) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= bytes.len(),
        forall|j:int| 0 <= j < i ==> bytes@[j] < 128, decreases bytes.len() - i,))]
    while i < bytes.len() {
        if bytes[i] >= 128 {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == values@.contains(id),))]
pub(super) fn contains(values: &[u16], id: u16) -> bool {
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant i <= values.len(),
        forall|j:int| 0 <= j < i ==> values@[j] != id, decreases values.len() - i,))]
    while i < values.len() {
        if values[i] == id {
            return true;
        }
        i += 1;
    }
    false
}
