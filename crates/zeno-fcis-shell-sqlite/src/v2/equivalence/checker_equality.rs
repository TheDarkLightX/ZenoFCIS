//! Exact structural equality with contracts over the compared sequences.
//! No evaluator callback or observation projection is accepted here.
use super::*;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
use zeno_fcis_synthesis::finite::Op;

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (a == b),))]
fn domain(a: Domain, b: Domain) -> bool {
    match (a, b) {
        (Domain::Bool, Domain::Bool) => true,
        (Domain::Int { min: a, max: b }, Domain::Int { min: c, max: d }) => a == c && b == d,
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (a@ == b@),))]
pub(super) fn domains(a: &[Domain], b: &[Domain]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i <= a.len(), a.len() == b.len(),
            forall|j: int| 0 <= j < i ==> a@[j] == b@[j],
        decreases a.len() - i,
    ))]
    while i < a.len() {
        if !domain(a[i], b[i]) {
            return false;
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(a@ =~= b@); }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (*a == *b),))]
fn op(a: &Op, b: &Op) -> bool {
    match (a, b) {
        (Op::Input(a), Op::Input(b)) => *a == *b,
        (Op::Int(a), Op::Int(b)) => *a == *b,
        (Op::Bool(a), Op::Bool(b)) => *a == *b,
        (Op::Not(a), Op::Not(b)) => *a == *b,
        (Op::Add(a, b), Op::Add(c, d))
        | (Op::Sub(a, b), Op::Sub(c, d))
        | (Op::Eq(a, b), Op::Eq(c, d))
        | (Op::Lt(a, b), Op::Lt(c, d))
        | (Op::And(a, b), Op::And(c, d)) => *a == *c && *b == *d,
        (Op::Select(a, b, c), Op::Select(d, e, f)) => *a == *d && *b == *e && *c == *f,
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (a@ == b@),))]
pub(super) fn nodes(a: &[Op], b: &[Op]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i <= a.len(), a.len() == b.len(),
            forall|j: int| 0 <= j < i ==> a@[j] == b@[j],
        decreases a.len() - i,
    ))]
    while i < a.len() {
        if !op(&a[i], &b[i]) {
            return false;
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(a@ =~= b@); }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (a@ == b@),))]
pub(super) fn roots(a: &[u16], b: &[u16]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant i <= a.len(), a.len() == b.len(),
            forall|j: int| 0 <= j < i ==> a@[j] == b@[j],
        decreases a.len() - i,
    ))]
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(a@ =~= b@); }
    true
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == (a == b),))]
fn failure(a: V2ExecutionFailure, b: V2ExecutionFailure) -> bool {
    match (a, b) {
        (V2ExecutionFailure::InputDomain, V2ExecutionFailure::InputDomain)
        | (V2ExecutionFailure::Reference, V2ExecutionFailure::Reference)
        | (V2ExecutionFailure::Arithmetic, V2ExecutionFailure::Arithmetic)
        | (V2ExecutionFailure::OutputDomain, V2ExecutionFailure::OutputDomain) => true,
        (V2ExecutionFailure::Budget(a), V2ExecutionFailure::Budget(b)) => {
            a.resource.index() == b.resource.index()
                && a.limit == b.limit
                && a.attempted == b.attempted
                && a.overflow == b.overflow
        }
        _ => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == (spec::result_view(*a) == spec::result_view(*b)),
))]
pub(super) fn results(
    a: &Result<Vec<i64>, V2ExecutionFailure>,
    b: &Result<Vec<i64>, V2ExecutionFailure>,
) -> bool {
    match (a, b) {
        (Ok(left), Ok(right)) => {
            if left.len() != right.len() {
                return false;
            }
            let mut i = 0usize;
            #[cfg_attr(verus_keep_ghost, verus_spec(
                invariant i <= left.len(), left.len() == right.len(),
                    spec::result_view(*a) == Ok(left@),
                    spec::result_view(*b) == Ok(right@),
                    forall|j: int| 0 <= j < i ==> left@[j] == right@[j],
                decreases left.len() - i,
            ))]
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
        (Err(a), Err(b)) => failure(*a, *b),
        _ => false,
    }
}
