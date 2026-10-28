//! Ordered possible outside-failure cuts, shared with the proof harness.
use std::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {
pub(crate) open spec fn exact_interval(cutters: Seq<usize>, failure_case: Seq<bool>, from: int, until: int) -> bool {
    (forall|j: int| 0 <= j < cutters.len() ==> from <= #[trigger] cutters[j] < until
        && failure_case[cutters[j] as int])
    && (forall|t: int| from <= t < until && #[trigger] failure_case[t] ==> cutters.contains(t as usize))
    && (forall|i: int, j: int| 0 <= i < j < cutters.len() ==> cutters[i] < cutters[j])
}

pub(crate) open spec fn group_start(members: Seq<usize>, position: int) -> int {
    if position == 0 { 0 } else { members[position - 1] as int + 1 }
}

pub(crate) open spec fn ordered_members(members: Seq<usize>, n: int) -> bool {
    (forall|i: int| 0 <= i < members.len() ==> #[trigger] members[i] < n)
    && (forall|i: int, j: int| 0 <= i < j < members.len() ==> members[i] < members[j])
}
}

/// All flagged component positions in [from, until), in their original order.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires from <= until, until <= failure_case.len(),
    ensures exact_interval(result@, failure_case@, from as int, until as int),
))]
pub(crate) fn interval_cutters(failure_case: &[bool], from: usize, until: usize) -> Vec<usize> {
    let mut cutters: Vec<usize> = Vec::new();
    let mut at = from;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            from <= at <= until, until <= failure_case.len(),
            exact_interval(cutters@, failure_case@, from as int, at as int),
        decreases until - at,
    ))]
    while at < until {
        #[cfg(verus_keep_ghost)]
        let before = verus_exec_expr!(Ghost(cutters@));
        if failure_case[at] {
            cutters.push(at);
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|j: int| 0 <= j < cutters.len() implies
                from <= #[trigger] cutters@[j] < at + 1 && failure_case@[cutters@[j] as int] by { }
            assert forall|t: int| from <= t < at + 1 && #[trigger] failure_case@[t]
                implies cutters@.contains(t as usize) by {
                if t < at {
                    assert(before@.contains(t as usize));
                    let j = choose|j: int| 0 <= j < before@.len() && before@[j] == t as usize;
                    assert(cutters@[j] == t as usize);
                } else { assert(cutters@[cutters.len() - 1] == t as usize); }
            }
            assert forall|i: int, j: int| 0 <= i < j < cutters.len()
                implies cutters@[i] < cutters@[j] by { }
        }
        at += 1;
    }
    cutters
}

/// Before each member, the flagged positions after its preceding member.
/// Failures after the last member need only the existing final prefix check.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires ordered_members(members@, failure_case.len() as int),
    ensures
        result.len() == members.len(),
        forall|p: int| 0 <= p < members.len() ==>
            exact_interval(#[trigger] result@[p]@, failure_case@,
                group_start(members@, p), members@[p] as int),
))]
pub(crate) fn outside_failure_cutters(failure_case: &[bool], members: &[usize]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut position = 0usize;
    let mut from = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            position <= members.len(), groups.len() == position,
            ordered_members(members@, failure_case.len() as int),
            from == group_start(members@, position as int),
            forall|p: int| 0 <= p < position ==>
                exact_interval(#[trigger] groups@[p]@, failure_case@,
                    group_start(members@, p), members@[p] as int),
        decreases members.len() - position,
    ))]
    while position < members.len() {
        let member = members[position];
        let cutters = interval_cutters(failure_case, from, member);
        groups.push(cutters);
        from = member + 1;
        position += 1;
    }
    groups
}

#[cfg(test)]
#[path = "coupling_cuts_tests.rs"]
mod tests;
