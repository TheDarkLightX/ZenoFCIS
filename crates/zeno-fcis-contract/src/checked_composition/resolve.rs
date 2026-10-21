//! Shared ordered outcome resolution and ownership-aware state application.
//!
//! The callback is immutable: a local table lookup is a pure decision. The
//! caller recovers payloads for the visited prefix after resolving its kinds.
use std::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Local {
    Skip,
    Commit,
    Reject,
    Refused,
    CommitFailure,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum End {
    Accept,
    Reject(usize),
    Refused(usize),
    CommittedFailure(usize),
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Resolution {
    pub end: End,
    pub committed: usize,
}

#[cfg(verus_keep_ghost)]
#[path = "resolve_spec.rs"]
pub(crate) mod model;

/// Scan lazily. A terminating component is included exactly once; its
/// successors are not queried. Reject and Refused commit no prefix.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        decisions@.len() == n,
        forall|i: usize| i < n && i <= model::stop(decisions@) ==>
            call_requires(decide, (i,)),
        forall|i: usize, kind: Local| i < n &&
            call_ensures(decide, (i,), kind) ==> kind == decisions@[i as int],
    ensures result == model::resolve_spec(n as int, decisions@),
))]
pub(crate) fn resolve(
    n: usize,
    decide: impl Fn(usize) -> Local,
    #[cfg(verus_keep_ghost)] decisions: Ghost<Seq<Local>>,
) -> Resolution {
    #[cfg(verus_keep_ghost)]
    proof! { model::scan_bounds(decisions@, 0); }
    let mut at = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            at <= n,
            decisions@.len() == n,
            at <= model::stop(decisions@),
            model::resolve_from(n as int, decisions@, 0) ==
                model::resolve_from(n as int, decisions@, at as int),
            forall|i: usize| i < n && i <= model::stop(decisions@) ==>
                call_requires(decide, (i,)),
            forall|i: usize, kind: Local| i < n &&
                call_ensures(decide, (i,), kind) ==> kind == decisions@[i as int],
        decreases n - at,
    ))]
    while at < n {
        let kind = decide(at);
        match kind {
            Local::Reject => {
                return Resolution {
                    end: End::Reject(at),
                    committed: 0,
                };
            }
            Local::Refused => {
                return Resolution {
                    end: End::Refused(at),
                    committed: 0,
                };
            }
            Local::CommitFailure => {
                return Resolution {
                    end: End::CommittedFailure(at),
                    committed: at + 1,
                };
            }
            Local::Skip | Local::Commit => {
                #[cfg(verus_keep_ghost)]
                proof! { model::continue_scan(n as int, decisions@, at as int); }
            }
        }
        at += 1;
    }
    Resolution {
        end: End::Accept,
        committed: n,
    }
}

/// Apply a resolution to the production state representation. Specializing
/// the state avoids assuming that an arbitrary user-defined Clone preserves
/// its value. An absent post denotes Skip (or an unvisited component).
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        posts.len() == pre.len(), owns.len() == pre.len(),
        resolution.committed <= pre.len(),
    ensures
        result@.map_values(|v: Vec<i128>| v@) == model::apply_spec(
            pre@.map_values(|v: Vec<i128>| v@),
            posts@.map_values(|v: Option<Vec<i128>>| model::post_view(v)),
            owns@, *resolution),
))]
pub(crate) fn apply(
    pre: &[Vec<i128>],
    posts: &[Option<Vec<i128>>],
    owns: &[bool],
    resolution: &Resolution,
) -> Vec<Vec<i128>> {
    let rollback = matches!(resolution.end, End::Reject(_) | End::Refused(_));
    let mut after: Vec<Vec<i128>> = Vec::new();
    let mut at = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            at <= pre.len(), posts.len() == pre.len(), owns.len() == pre.len(),
            resolution.committed <= pre.len(), after.len() == at,
            rollback == model::rollback(*resolution),
            forall|i: int| 0 <= i < at ==> #[trigger] after@[i]@ ==
                model::apply_spec(pre@.map_values(|v: Vec<i128>| v@),
                    posts@.map_values(|v: Option<Vec<i128>>| model::post_view(v)),
                    owns@, *resolution)[i],
        decreases pre.len() - at,
    ))]
    while at < pre.len() {
        let value = if !rollback && at < resolution.committed && owns[at] {
            match &posts[at] {
                Some(post) => post.clone(),
                None => pre[at].clone(),
            }
        } else {
            pre[at].clone()
        };
        after.push(value);
        at += 1;
    }
    after
}
