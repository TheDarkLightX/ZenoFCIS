//! Mathematical meaning of the shared resolver and state application.
use super::{End, Local, Resolution};
use vstd::prelude::*;

verus! {
pub(crate) open spec fn terminal(kind: Local) -> bool {
    matches!(kind, Local::Reject | Local::Refused | Local::CommitFailure)
}

pub(crate) open spec fn stop_from(decisions: Seq<Local>, at: int) -> int
    recommends 0 <= at <= decisions.len(),
    decreases decisions.len() - at,
{
    if at >= decisions.len() || terminal(decisions[at]) { at }
    else { stop_from(decisions, at + 1) }
}

pub(crate) open spec fn stop(decisions: Seq<Local>) -> int { stop_from(decisions, 0) }

pub(crate) open spec fn resolve_from(n: int, decisions: Seq<Local>, at: int) -> Resolution
    recommends n == decisions.len(), 0 <= at <= n, n <= usize::MAX,
    decreases n - at,
{
    if at >= n { Resolution { end: End::Accept, committed: n as usize } }
    else {
        match decisions[at] {
            Local::Reject => Resolution { end: End::Reject(at as usize), committed: 0 },
            Local::Refused => Resolution { end: End::Refused(at as usize), committed: 0 },
            Local::CommitFailure => Resolution {
                end: End::CommittedFailure(at as usize), committed: (at + 1) as usize },
            _ => resolve_from(n, decisions, at + 1),
        }
    }
}

pub(crate) open spec fn resolve_spec(n: int, decisions: Seq<Local>) -> Resolution {
    resolve_from(n, decisions, 0)
}

pub(crate) proof fn scan_bounds(decisions: Seq<Local>, at: int)
    requires 0 <= at <= decisions.len(),
    ensures at <= stop_from(decisions, at) <= decisions.len(),
    decreases decisions.len() - at,
{
    if at < decisions.len() && !terminal(decisions[at]) { scan_bounds(decisions, at + 1); }
}

pub(crate) proof fn continue_scan(n: int, decisions: Seq<Local>, at: int)
    requires n == decisions.len(), 0 <= at < n, n <= usize::MAX,
        at <= stop(decisions), !terminal(decisions[at]),
    ensures
        at + 1 <= stop(decisions),
        resolve_from(n, decisions, at) == resolve_from(n, decisions, at + 1),
{
    stop_prefix(decisions, at);
    scan_bounds(decisions, at + 1);
    assert(stop_from(decisions, at) == stop_from(decisions, at + 1));
}

pub(crate) proof fn stop_prefix(decisions: Seq<Local>, at: int)
    requires 0 <= at <= decisions.len(), at <= stop(decisions),
    ensures stop_from(decisions, at) == stop(decisions),
    decreases at,
{
    if at > 0 {
        stop_prefix(decisions, at - 1);
        if terminal(decisions[at - 1]) { assert(stop(decisions) == at - 1); }
    }
}

pub(crate) open spec fn rollback(resolution: Resolution) -> bool {
    matches!(resolution.end, End::Reject(_) | End::Refused(_))
}

pub(crate) open spec fn post_view(value: Option<Vec<i128>>) -> Option<Seq<i128>> {
    match value { Some(v) => Some(v@), None => None }
}

pub(crate) open spec fn apply_spec<S>(pre: Seq<S>, posts: Seq<Option<S>>, owns: Seq<bool>,
    resolution: Resolution) -> Seq<S> {
    Seq::new(pre.len(), |i: int|
        if !rollback(resolution) && i < resolution.committed && owns[i] {
            match posts[i] { Some(post) => post, None => pre[i] }
        } else { pre[i] })
}

pub(crate) proof fn application_frames<S>(pre: Seq<S>, posts: Seq<Option<S>>, owns: Seq<bool>, r: Resolution)
    requires posts.len() == pre.len(), owns.len() == pre.len(), r.committed <= pre.len(),
    ensures
        apply_spec(pre, posts, owns, r).len() == pre.len(),
        forall|i: int| 0 <= i < pre.len() &&
            (rollback(r) || i >= r.committed || !owns[i] || posts[i].is_none()) ==>
            #[trigger] apply_spec(pre, posts, owns, r)[i] == pre[i],
        forall|i: int| 0 <= i < pre.len() && !rollback(r) && i < r.committed && owns[i] &&
            posts[i].is_some() ==> #[trigger] apply_spec(pre, posts, owns, r)[i] == posts[i].unwrap(),
{
}
} // verus!
