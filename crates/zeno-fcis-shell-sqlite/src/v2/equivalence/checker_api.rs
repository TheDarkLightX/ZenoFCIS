//! Public comparison data API for authoring tools and library consumers.
//!
//! Every operation forwards to the single shared checker. Results describe
//! finite scalar comparison; they do not grant store or publication authority.
//! Completed internal summaries retain private constructors. Returned vectors
//! are owned, and the odometer's mutable slice is caller-owned scratch.

extern crate alloc;
use super::checker;
use alloc::vec::Vec;
pub use checker::{FULL_BUDGET, Failure, Observation, Usage};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
use zeno_fcis_synthesis::finite::{Domain, V2ScalarProgram};

#[cfg(verus_keep_ghost)]
verus! {
pub(crate) open spec fn project(result: Result<checker::Complete, Failure>) -> Result<(u64, Usage), Failure> {
    match result { Ok(done) => Ok(done.view()), Err(error) => Err(error) }
}

/// Exact projection of a result satisfying the original checker theorem.
pub closed spec fn comparison(a: &V2ScalarProgram, b: &V2ScalarProgram,
    cap: u64, limit: u64, result: Result<(u64, Usage), Failure>) -> bool {
    exists|checked: Result<checker::Complete, Failure>|
        checker::spec::comparison_result(a, b, cap, limit, checked)
        && #[trigger] project(checked) == result
}
pub closed spec fn admission(a: &V2ScalarProgram, b: &V2ScalarProgram,
    cap: u64, result: Result<u64, Failure>) -> bool {
    checker::spec::admission_result(a, b, cap, result)
}
pub closed spec fn cardinality(domains: Seq<Domain>, result: Result<Option<u128>, usize>) -> bool {
    checker::spec::size_result(domains, result)
}
pub closed spec fn first(domains: Seq<Domain>, result: Seq<i64>) -> bool {
    checker::spec::minimum(domains, result)
    && checker::spec::rank(domains, result, domains.len() as int) == 0
    && (checker::spec::valid(domains, domains.len() as int) ==> checker::spec::tuple(domains, result))
}
pub closed spec fn successor(domains: Seq<Domain>, before: Seq<i64>, after: Seq<i64>, more: bool) -> bool {
    after.len() == before.len()
    && (checker::spec::tuple(domains, before) ==> (
        checker::spec::tuple(domains, after)
        && (more == (checker::spec::rank(domains, before, domains.len() as int) + 1
            < checker::spec::product(domains, domains.len() as int)))
        && (if more {
            checker::spec::rank(domains, after, domains.len() as int)
                == checker::spec::rank(domains, before, domains.len() as int) + 1
        } else {
            checker::spec::minimum(domains, after)
            && checker::spec::rank(domains, before, domains.len() as int)
                == checker::spec::product(domains, domains.len() as int) - 1
        })
    ))
}
}

/// Validate input ABI, output ABI, empty domains and the cap, in that order.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures admission(original, candidate, cap, result),
))]
pub fn validate_pair(
    original: &V2ScalarProgram<'_>,
    candidate: &V2ScalarProgram<'_>,
    cap: u64,
) -> Result<u64, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(admission); }
    checker::validate_pair(original, candidate, cap)
}

/// Compare every input tuple, then enforce the declared Step limit.
/// Success contains the exact tuple count and full-domain Step statistics.
/// A first counterexample takes precedence over any Step-limit refusal.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures comparison(original, candidate, cap, step_limit, result),
))]
pub fn compare_with_usage(
    original: &V2ScalarProgram<'_>,
    candidate: &V2ScalarProgram<'_>,
    cap: u64,
    step_limit: u64,
) -> Result<(u64, Usage), Failure> {
    let checked = checker::compare_with_usage(original, candidate, cap, step_limit);
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost witness = checked; }
    let result = match checked {
        Ok(done) => Ok((done.tuples(), done.usage())),
        Err(error) => Err(error),
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(comparison);
        assert(checker::spec::comparison_result(original, candidate, cap, step_limit, witness));
        assert(project(witness) == result);
        assert(exists|checked: Result<checker::Complete, Failure>|
            checker::spec::comparison_result(original, candidate, cap, step_limit, checked)
            && #[trigger] project(checked) == result);
    }
    result
}

/// Exact cardinality: the first empty domain wins over earlier overflow.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures cardinality(domains@, result),))]
pub fn domain_size(domains: &[Domain]) -> Result<Option<u128>, usize> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(cardinality); }
    checker::domain_size(domains)
}

/// Construct the all-minimum tuple; zero inputs produce one empty tuple.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures first(domains@, result@),))]
pub fn first_tuple(domains: &[Domain]) -> Vec<i64> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(first); }
    checker::first_tuple(domains)
}

/// Advance caller-owned scratch in last-input-fastest order, resetting at end.
/// The exact successor guarantee applies to a same-length in-domain tuple.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures successor(domains@, old(tuple)@, final(tuple)@, result),
))]
pub fn advance(domains: &[Domain], tuple: &mut [i64]) -> bool {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(successor); }
    checker::advance(domains, tuple)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_synthesis::finite::Op;

    #[test]
    fn public_projection_preserves_counts_usage_refusals_and_order() {
        let inputs = [Domain::Bool, Domain::Bool];
        let outputs = [Domain::Bool];
        let nodes = [Op::Input(0)];
        let p = V2ScalarProgram {
            inputs: &inputs,
            outputs: &outputs,
            nodes: &nodes,
            roots: &[0],
        };
        let (count, usage) = compare_with_usage(&p, &p, 4, 1)
            .unwrap_or_else(|error| panic!("unexpected checker refusal: {error:?}"));
        assert_eq!(count, 4);
        assert_eq!(
            usage,
            Usage {
                max_steps: [1, 1],
                candidate_uses_more: 0,
                usage_preserved: true
            }
        );
        assert_eq!(
            compare_with_usage(&p, &p, 3, 1),
            Err(Failure::DomainTooLarge {
                size: Some(4),
                limit: 3
            })
        );
        assert_eq!(
            compare_with_usage(&p, &p, 4, 0),
            Err(Failure::BudgetBoundary {
                over_limit: [4, 4],
                minimum_limit: 1,
                usage,
            })
        );
        assert_eq!(validate_pair(&p, &p, 4), Ok(4));
        assert_eq!(domain_size(&inputs), Ok(Some(4)));
        let mut tuple = first_tuple(&inputs);
        assert_eq!(tuple, [0, 0]);
        for expected in [[0, 1], [1, 0], [1, 1]] {
            assert!(advance(&inputs, &mut tuple));
            assert_eq!(tuple, expected);
        }
        assert!(!advance(&inputs, &mut tuple));
        assert_eq!(tuple, [0, 0]);
    }
}
