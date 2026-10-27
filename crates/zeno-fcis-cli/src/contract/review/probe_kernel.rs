//! Reachable-predicate answers over complete immutable finite-machine tables.
//! Reuses the lemma kernel's verified FIFO parents and trace construction.
extern crate alloc;
#[cfg(verus_keep_ghost)]
use super::finite_machine_proofs::{effective_trace, trace_stays_in_domain};
#[cfg(verus_keep_ghost)]
use super::finite_machine_spec::*;
use super::lemma_kernel as search_kernel;
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Domain-size admission; an unknown/overflowed size is never a proof.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == match size { Some(n) => n <= limit as u128, None => false },
))]
pub(crate) fn within_limit(size: Option<u128>, limit: u64) -> bool {
    match size {
        Some(size) => size <= u128::from(limit),
        None => false,
    }
}

/// Unknown carries no yes/no answer, partial count or witness.
pub(crate) enum Answer {
    Unknown,
    Known {
        target: Option<usize>,
        trace: Vec<usize>,
        reachable_count: usize,
    },
}

#[cfg(verus_keep_ghost)]
verus! {
    pub(crate) open spec fn seen_count(seen:Seq<bool>, end:nat) -> nat
        decreases end,
    {
        if end == 0 { 0 } else { seen_count(seen,(end-1) as nat) + if seen[end-1] { 1nat } else { 0nat } }
    }
    pub(crate) open spec fn first_match(seen:Seq<bool>,query:Seq<bool>,end:int,target:Option<usize>) -> bool {
        match target {
            None => forall|s:int| 0 <= s < end ==> !(seen[s] && query[s]),
            Some(t) => t < end && seen[t as int] && query[t as int]
                && forall|s:int| 0 <= s < t ==> !(seen[s] && query[s]),
        }
    }
    pub(crate) open spec fn answer_spec(d:Answer,k:int,g:int,after:Seq<usize>,commits:Seq<bool>,query:Seq<bool>,limit:u64) -> bool {
        match d {
            Answer::Unknown => after.len() > limit,
            Answer::Known { target, trace, reachable_count } => {
                let r = reach_set(after,commits,query.len() as int,k,g);
                &&& after.len() <= limit
                &&& first_match(r,query,query.len() as int,target)
                &&& reachable_count == seen_count(r,query.len())
                &&& match target {
                    None => trace@.len() == 0,
                    Some(t) => {
                        &&& t < query.len() && query[t as int] && reach(after,commits,query.len() as int,k,g,t as int)
                        &&& walk(after,commits,k,g,trace@) == t
                        &&& (forall|j:int| 0 <= j < trace@.len() ==> (#[trigger] trace@[j]) < k)
                        &&& effective_trace(after,commits,query.len() as int,k,g,trace@)
                    },
                }
            },
        }
    }

    pub(crate) proof fn probe_sound_complete(d:Answer,k:int,g:int,after:Seq<usize>,commits:Seq<bool>,query:Seq<bool>,limit:u64)
        requires table_valid(query.len() as int,k,g,after,commits), answer_spec(d,k,g,after,commits,query,limit),
        ensures match d {
            Answer::Unknown => after.len() > limit,
            Answer::Known { target, trace, reachable_count } => {
                &&& after.len() <= limit
                &&& (target.is_some() == exists|s:int| 0 <= s < query.len() && (#[trigger] query[s]) && reach(after,commits,query.len() as int,k,g,s))
                &&& (target.is_none() == !(exists|s:int| 0 <= s < query.len() && (#[trigger] query[s]) && reach(after,commits,query.len() as int,k,g,s)))
                &&& match target { None => true, Some(t) => walk(after,commits,k,g,trace@) == t
                    && effective_trace(after,commits,query.len() as int,k,g,trace@) }
            },
        },
    {}

    pub(crate) proof fn singleton_probe(d:Answer,k:int,g:int,after:Seq<usize>,commits:Seq<bool>,query:Seq<bool>,limit:u64,state:usize)
        requires table_valid(query.len() as int,k,g,after,commits), answer_spec(d,k,g,after,commits,query,limit),
            state < query.len(), after.len() <= limit,
            forall|s:int| 0 <= s < query.len() ==> (#[trigger] query[s]) == (s == state),
        ensures match d {
            Answer::Unknown => false,
            Answer::Known { target, .. } => target == if reach(after,commits,query.len() as int,k,g,state as int) { Some(state) } else { None },
        },
    {
        probe_sound_complete(d,k,g,after,commits,query,limit);
        match d {
            Answer::Unknown => {},
            Answer::Known { target, .. } => match target {
                Some(t) => {
                    assert(query[t as int]);
                    assert(t == state);
                },
                None => {
                    let r = reach_set(after,commits,query.len() as int,k,g);
                    assert(query[state as int]);
                    assert(!(r[state as int] && query[state as int]));
                },
            },
        }
    }
}

/// Exact existence, least state ordinal and complete reachable count.
/// Invalid tables refuse with None; a tuple limit produces only `Unknown`.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.is_some() == table_valid(query@.len() as int,k as int,g as int,after@,commits@),
        match result { None => true, Some(d) => answer_spec(d,k as int,g as int,after@,commits@,query@,limit) },
))]
pub(crate) fn decide_reachable(
    k: usize,
    g: usize,
    after: &[usize],
    commits: &[bool],
    query: &[bool],
    limit: u64,
) -> Option<Answer> {
    let n = query.len();
    if !search_kernel::valid_tables(n, k, g, after, commits) {
        return None;
    }
    if !within_limit(Some(after.len() as u128), limit) {
        return Some(Answer::Unknown);
    }
    let search = search_kernel::reachable(n, k, g, after, commits);
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(search.seen@ =~= reach_set(after@,commits@,n as int,k as int,g as int));
    }
    let mut at = 0usize;
    let mut count = 0usize;
    let mut found = None;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant table_valid(n as int,k as int,g as int,after@,commits@), query@.len() == n,
            after@.len() <= limit, at <= n, count <= at,
            search.seen@ == reach_set(after@,commits@,n as int,k as int,g as int),
            search_kernel::parents_valid(after@,commits@,k as int,g as int,search.seen@,search.parent@,search.depth@),
            count == seen_count(search.seen@,at as nat),
            first_match(search.seen@,query@,at as int,found),
        decreases n-at,
    ))]
    while at < n {
        if search.seen[at] {
            count += 1;
        }
        if found.is_none() && query[at] && search.seen[at] {
            found = Some(at);
        }
        at += 1;
    }
    let path = match found {
        None => Vec::new(),
        Some(target) => {
            let path = search_kernel::trace(k, g, after, commits, &search, target);
            #[cfg(verus_keep_ghost)]
            proof! { trace_stays_in_domain(after@,commits@,n as int,k as int,g as int,g as int,path@); }
            path
        }
    };
    Some(Answer::Known {
        target: found,
        trace: path,
        reachable_count: count,
    })
}
