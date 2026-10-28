//! C1's four original nonrecursive lexical definitions only.
use core::cmp::Ordering;
use vstd::prelude::*;
verus! {
pub(crate) open spec fn suffix_equal(a: Seq<u128>, b: Seq<u128>, start: int) -> bool {
    a.len() == b.len() && 0 <= start <= a.len()
    && forall|j: int| start <= j < a.len() ==> a[j] == b[j]
}
pub(crate) open spec fn lex_lt(a: Seq<u128>, b: Seq<u128>) -> bool {
    a.len() == b.len()
    && exists|j: int| 0 <= j < a.len() && a[j] < b[j] && suffix_equal(a, b, j + 1)
}
pub(crate) open spec fn lex_gt(a: Seq<u128>, b: Seq<u128>) -> bool {
    a.len() == b.len()
    && exists|j: int| 0 <= j < a.len() && a[j] > b[j] && suffix_equal(a, b, j + 1)
}
pub(crate) open spec fn lex_result(a: Seq<u128>, b: Seq<u128>, result: Ordering) -> bool {
    &&& a.len() == b.len()
    &&& (result == Ordering::Less || result == Ordering::Equal || result == Ordering::Greater)
    &&& (result == Ordering::Less <==> lex_lt(a, b))
    &&& (result == Ordering::Equal <==> a == b)
    &&& (result == Ordering::Greater <==> lex_gt(a, b))
}
}
