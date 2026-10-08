//! Shared finite comparison core for CLI transforms and SQLite upgrades.
//! Borrowed inputs are immutable; only invocation-owned tuple/tally scratch
//! mutates. Both routes use the same validation and exhaustive result scan.
//! See docs/G11_SHARED_CHECKER_PROOF_PLAN.md for qualification status.

// Each consumer intentionally uses only its applicable result route.
#![allow(dead_code)]
extern crate alloc;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[path = "checker_equality.rs"]
mod equality;
#[cfg(verus_keep_ghost)]
#[path = "checker_spec.rs"]
pub(crate) mod spec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{
    Domain, MAX_NODES, V2ExecutionFailure, V2Resource, V2ScalarProgram, execute_v2, v2_zero_limits,
};

/// The fixed full evaluation budget, separate from the declared report limit.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub const FULL_BUDGET: u64 = MAX_NODES as u64;

/// One program's full-budget result for one input tuple.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub struct Observation {
    /// Complete output tuple, or the evaluator's exact failure.
    pub result: Result<Vec<i64>, V2ExecutionFailure>,
    /// True Step usage: instruction attempts, including a trapping one.
    pub steps: u64,
}

#[cfg(verus_keep_ghost)]
verus! {
impl Observation {
    pub closed spec fn view(self) -> (Result<Seq<i64>, V2ExecutionFailure>, u64) {
        (match self.result { Ok(values) => Ok(values@), Err(error) => Err(error) }, self.steps)
    }
}
}

impl Clone for Observation {
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result.view() == self.view(),
    ))]
    fn clone(&self) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Observation::view); }
        let result = match &self.result {
            Ok(values) => Ok(values.clone()),
            Err(error) => Err(*error),
        };
        Self {
            result,
            steps: self.steps,
        }
    }
}

/// Step usage over the whole domain, from the full-budget runs.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    /// Largest Step usage on any tuple: original, then candidate.
    pub max_steps: [u64; 2],
    /// Tuples on which the candidate uses more Steps than the original.
    pub candidate_uses_more: u64,
    /// Whether the two programs use equal Steps on every tuple.
    pub usage_preserved: bool,
}

/// Every common refusal, without caller-specific import or receipt data.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub enum Failure {
    /// Input domains differ in count, order or kind.
    InputAbi,
    /// Output domains differ in count, order or kind.
    OutputAbi,
    /// The first empty input domain, before applying any cardinality cap.
    EmptyInputDomain {
        /// Zero-based input position.
        position: usize,
    },
    /// The exact product exceeds the caller's cap.
    DomainTooLarge {
        /// Exact cardinality, or `None` if it exceeds `u128::MAX`.
        size: Option<u128>,
        /// The caller's enumeration cap.
        limit: u64,
    },
    /// The first unequal full result in last-input-fastest order.
    Counterexample {
        /// Zero-based tuple ordinal.
        ordinal: u64,
        /// The input tuple, owned independently of both programs.
        input: Vec<i64>,
        /// The original program's exact result and Step usage.
        original: Observation,
        /// The candidate program's exact result and Step usage.
        candidate: Observation,
    },
    /// Defensive refusal when enumeration and exact cardinality disagree.
    CoverageMismatch {
        /// The exact cardinality admitted before enumeration.
        expected: u64,
        /// Number of tuples reached by enumeration.
        visited: u64,
    },
    /// Every result matched, but at least one run exceeded the declared Steps.
    BudgetBoundary {
        /// Per-side counts of runs strictly above the declared limit.
        over_limit: [u64; 2],
        /// Maximum Step usage across both sides and every tuple.
        minimum_limit: u64,
        /// Exact full-domain Step statistics.
        usage: Usage,
    },
}

/// Only a completed scan can construct this usage summary.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub(crate) struct Complete {
    tuples: u64,
    usage: Usage,
}

impl Complete {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.view().0,))]
    pub(crate) fn tuples(&self) -> u64 {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Complete::view); }
        self.tuples
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.view().1,))]
    pub(crate) fn usage(&self) -> Usage {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Complete::view); }
        self.usage
    }
}

/// Scalar equality; identity shortcuts deliberately have no usage summary.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub(crate) struct Equal {
    tuples: u64,
    enumerated: bool,
}

impl Equal {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.view().0,))]
    pub(crate) fn tuples(&self) -> u64 {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Equal::view); }
        self.tuples
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.view().1,))]
    pub(crate) fn enumerated(&self) -> bool {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Equal::view); }
        self.enumerated
    }
}

#[cfg(verus_keep_ghost)]
verus! {
impl Complete {
    pub(crate) closed spec fn view(self) -> (u64, Usage) { (self.tuples, self.usage) }
}
impl Equal {
    pub(crate) closed spec fn view(self) -> (u64, bool) { (self.tuples, self.enumerated) }
}
}

/// Ordered ABI/domain/cap admission before any evaluation or shortcut.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::admission_result(original, candidate, cap, result),
        result.is_ok() == spec::admitted(original, candidate, cap),
))]
pub(crate) fn validate_pair(
    original: &V2ScalarProgram<'_>,
    candidate: &V2ScalarProgram<'_>,
    cap: u64,
) -> Result<u64, Failure> {
    if !equality::domains(original.inputs, candidate.inputs) {
        return Err(Failure::InputAbi);
    }
    if !equality::domains(original.outputs, candidate.outputs) {
        return Err(Failure::OutputAbi);
    }
    let size = match domain_size(original.inputs) {
        Ok(size) => size,
        Err(position) => return Err(Failure::EmptyInputDomain { position }),
    };
    match size {
        Some(n) if n <= cap as u128 => Ok(n as u64),
        _ => Err(Failure::DomainTooLarge { size, limit: cap }),
    }
}

/// CLI route: always enumerate, then enforce the declared Step boundary.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::comparison_result(original, candidate, cap, step_limit, result),
        result.is_ok() == (spec::admitted(original, candidate, cap)
            && spec::all_equal(original, candidate)
            && spec::over(original, step_limit, spec::product(original.inputs@, original.inputs@.len() as int)) == 0
            && spec::over(candidate, step_limit, spec::product(original.inputs@, original.inputs@.len() as int)) == 0),
))]
pub(crate) fn compare_with_usage(
    original: &V2ScalarProgram<'_>,
    candidate: &V2ScalarProgram<'_>,
    cap: u64,
    step_limit: u64,
) -> Result<Complete, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(Complete::view); }
    let size = validate_pair(original, candidate, cap)?;
    let tally = scan(original, candidate, size, step_limit)?;
    let usage = tally.usage();
    if tally.over_limit != [0, 0] {
        return Err(Failure::BudgetBoundary {
            over_limit: tally.over_limit,
            minimum_limit: usage.max_steps[0].max(usage.max_steps[1]),
            usage,
        });
    }
    Ok(Complete {
        tuples: size,
        usage,
    })
}

/// Shell route: the same checks and scan, with a nonbinding report threshold.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::equality_result(original, candidate, cap, result),
        result.is_ok() == (spec::admitted(original, candidate, cap) && spec::all_equal(original, candidate)),
))]
pub(crate) fn compare_equal(
    original: &V2ScalarProgram<'_>,
    candidate: &V2ScalarProgram<'_>,
    cap: u64,
) -> Result<Equal, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(Equal::view); }
    let size = validate_pair(original, candidate, cap)?;
    if equality::nodes(original.nodes, candidate.nodes)
        && equality::roots(original.roots, candidate.roots)
    {
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|xs: Seq<i64>| spec::tuple(original.inputs@, xs) implies
                spec::observation(original, xs).0 == spec::observation(candidate, xs).0 by {}
        }
        return Ok(Equal {
            tuples: size,
            enumerated: false,
        });
    }
    scan(original, candidate, size, u64::MAX)?;
    Ok(Equal {
        tuples: size,
        enumerated: true,
    })
}

/// The single exhaustive scan. Its size is supplied only by pair validation.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::admitted(original, candidate, size)
        && size as int == spec::product(original.inputs@, original.inputs@.len() as int)
        ==> match result {
            Ok(t) => spec::all_equal(original, candidate)
                && spec::tally(t, original, candidate, step_limit, size as int),
            Err(error) => spec::witness(original, candidate, error),
        },
))]
fn scan(
    original: &V2ScalarProgram<'_>,
    candidate: &V2ScalarProgram<'_>,
    size: u64,
    step_limit: u64,
) -> Result<Tally, Failure> {
    let domains = original.inputs;
    let mut tally = new_tally();
    let mut input = first_tuple(domains);
    let mut visited = 0_u64;
    #[cfg(verus_keep_ghost)]
    proof_decl! {
        let ghost qualified = spec::admitted(original, candidate, size)
            && size as int == spec::product(domains@, domains@.len() as int);
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        if qualified { spec::product_positive(domains@, domains@.len() as int); }
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            qualified == (spec::admitted(original, candidate, size)
                && size as int == spec::product(domains@, domains@.len() as int)),
            domains@ == original.inputs@,
            visited <= size,
            tally.over_limit[0] <= visited, tally.over_limit[1] <= visited,
            tally.candidate_uses_more <= visited,
            qualified ==> spec::tuple(domains@, input@),
            qualified ==> spec::equal_prefix(original, candidate, visited as int),
            qualified ==> spec::tally(tally, original, candidate, step_limit, visited as int),
        invariant_except_break
            qualified ==> visited < size,
            qualified ==> spec::rank(domains@, input@, domains@.len() as int) == visited,
        ensures qualified ==> visited == size,
        decreases size - visited,
    ))]
    loop {
        if visited == size {
            return Err(Failure::CoverageMismatch {
                expected: size,
                visited: visited.saturating_add(1),
            });
        }
        #[cfg(verus_keep_ghost)]
        proof! { if qualified { spec::at_rank(domains@, input@); } }
        let left = observe(original, &input);
        let right = observe(candidate, &input);
        if !equality::results(&left.result, &right.result) {
            return Err(Failure::Counterexample {
                ordinal: visited,
                input,
                original: left,
                candidate: right,
            });
        }
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost before_tally = tally; }
        tally.record(left.steps, right.steps, step_limit);
        #[cfg(verus_keep_ghost)]
        proof! {
            if qualified {
                spec::tally_extension(before_tally, tally, original, candidate, step_limit,
                    visited as int, left.steps, right.steps);
                assert forall|k: int| 0 <= k < visited + 1 implies
                    spec::observation(original, spec::at(domains@, k)).0
                    == spec::observation(candidate, spec::at(domains@, k)).0 by {}
            }
        }
        visited += 1;
        if !advance(domains, &mut input) {
            #[cfg(verus_keep_ghost)]
            proof! { assert(qualified ==> visited == size); }
            break;
        }
    }
    if visited != size {
        return Err(Failure::CoverageMismatch {
            expected: size,
            visited,
        });
    }
    #[cfg(verus_keep_ghost)]
    proof! { if qualified { spec::prefix_covers(original, candidate); } }
    Ok(tally)
}

/// Direct eager evaluation: no callback, copied model or observation projection.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::observation_view(result) == spec::observation(program, input@),
))]
fn observe(program: &V2ScalarProgram<'_>, input: &[i64]) -> Observation {
    let meter = v2_zero_limits().with_limit(V2Resource::Step, FULL_BUDGET);
    #[cfg(verus_keep_ghost)]
    proof! { assert(meter.view() =~= spec::limits()); }
    let (result, usage) = execute_v2(
        program.inputs,
        program.outputs,
        program.nodes,
        program.roots,
        input,
        meter,
    )
    .into_parts();
    Observation {
        result,
        steps: usage.used(V2Resource::Step),
    }
}

/// Exact number of tuples in the product of `domains`; `None` when it exceeds
/// `u128::MAX`. `Err` names the first empty domain. Zero domains give one
/// empty tuple.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::size_result(domains@, result),
))]
pub(crate) fn domain_size(domains: &[Domain]) -> Result<Option<u128>, usize> {
    let mut size = Some(1_u128);
    let mut position = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant position <= domains.len(), spec::valid(domains@, position as int),
            match size {
                Some(n) => n as int == spec::product(domains@, position as int),
                None => spec::product(domains@, position as int) > u128::MAX,
            },
        decreases domains.len() - position,
    ))]
    while position < domains.len() {
        let (min, max) = domains[position].bounds();
        if min > max {
            return Err(position);
        }
        // i128 represents the full i64 width, including 2^64.
        let width = ((max as i128) - (min as i128) + 1) as u128;
        #[cfg(verus_keep_ghost)]
        proof! {
            spec::product_positive(domains@, position as int);
            if size.is_none() {
                let prior = spec::product(domains@, position as int);
                assert(prior * width > u128::MAX) by(nonlinear_arith)
                    requires prior > u128::MAX, width >= 1;
            }
        }
        size = match size {
            Some(n) => n.checked_mul(width),
            None => None,
        };
        position += 1;
    }
    Ok(size)
}

/// The first tuple in enumeration order: every input at its minimum.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures spec::minimum(domains@, result@),
        spec::rank(domains@, result@, domains@.len() as int) == 0,
        spec::valid(domains@, domains@.len() as int) ==> spec::tuple(domains@, result@),
))]
pub(crate) fn first_tuple(domains: &[Domain]) -> Vec<i64> {
    let mut result = Vec::new();
    let mut position = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant position <= domains.len(), result@.len() == position,
            forall|i: int| 0 <= i < position ==> result@[i] == spec::lo(domains@[i]),
        decreases domains.len() - position,
    ))]
    while position < domains.len() {
        result.push(domains[position].bounds().0);
        position += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { spec::minimum_rank(domains@, result@, domains@.len() as int); }
    result
}

/// Steps `tuple` to its successor, last input fastest. Returns false after the
/// last tuple, leaving the first one in place. An input is incremented only
/// while it is below its maximum, so no value overflows at an `i64` endpoint.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures final(tuple)@.len() == old(tuple)@.len(),
        spec::tuple(domains@, old(tuple)@) ==> (
            spec::tuple(domains@, final(tuple)@)
            && (result == (spec::rank(domains@, old(tuple)@, domains@.len() as int) + 1
                < spec::product(domains@, domains@.len() as int)))
            && (if result {
                spec::rank(domains@, final(tuple)@, domains@.len() as int)
                    == spec::rank(domains@, old(tuple)@, domains@.len() as int) + 1
            } else { spec::minimum(domains@, final(tuple)@)
                && spec::rank(domains@, old(tuple)@, domains@.len() as int)
                    == spec::product(domains@, domains@.len() as int) - 1 })
        ),
))]
pub(crate) fn advance(domains: &[Domain], tuple: &mut [i64]) -> bool {
    let mut position = tuple.len().min(domains.len());
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant position <= domains.len(), position <= tuple.len(),
            tuple@.len() == old(tuple)@.len(),
            forall|i: int| 0 <= i < position ==> tuple@[i] == old(tuple)@[i],
            spec::tuple(domains@, old(tuple)@) ==> spec::tuple(domains@, tuple@),
            spec::tuple(domains@, old(tuple)@) ==> forall|i: int| position <= i < domains@.len()
                ==> old(tuple)@[i] == spec::hi(domains@[i]) && tuple@[i] == spec::lo(domains@[i]),
        decreases position,
    ))]
    while position > 0 {
        position -= 1;
        let (min, max) = domains[position].bounds();
        if tuple[position] < max {
            tuple[position] += 1;
            #[cfg(verus_keep_ghost)]
            proof! {
                if spec::tuple(domains@, old(tuple)@) {
                    spec::carry_rank(domains@, old(tuple)@, tuple@, position as int, domains@.len() as int);
                    spec::rank_bounds(domains@, tuple@, domains@.len() as int);
                }
            }
            return true;
        }
        tuple[position] = min;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        if spec::tuple(domains@, old(tuple)@) {
            spec::maximum_rank(domains@, old(tuple)@, domains@.len() as int);
        }
    }
    false
}

/// Step accounting over tuples whose results already matched.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
struct Tally {
    max_steps: [u64; 2],
    over_limit: [u64; 2],
    candidate_uses_more: u64,
    usage_differs: bool,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result.max_steps == [0u64, 0u64], result.over_limit == [0u64, 0u64],
            result.candidate_uses_more == 0, !result.usage_differs,
    ))]
fn new_tally() -> Tally {
    Tally {
        max_steps: [0, 0],
        over_limit: [0, 0],
        candidate_uses_more: 0,
        usage_differs: false,
    }
}

impl Tally {
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures spec::recorded(*old(self), *final(self), original, candidate, limit),
    ))]
    fn record(&mut self, original: u64, candidate: u64, limit: u64) {
        self.max_steps[0] = self.max_steps[0].max(original);
        self.max_steps[1] = self.max_steps[1].max(candidate);
        if original > limit {
            self.over_limit[0] = self.over_limit[0].saturating_add(1);
        }
        if candidate > limit {
            self.over_limit[1] = self.over_limit[1].saturating_add(1);
        }
        if candidate > original {
            self.candidate_uses_more = self.candidate_uses_more.saturating_add(1);
        }
        self.usage_differs = self.usage_differs || candidate != original;
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result.max_steps == self.max_steps,
            result.candidate_uses_more == self.candidate_uses_more,
            result.usage_preserved == !self.usage_differs,
    ))]
    fn usage(&self) -> Usage {
        Usage {
            max_steps: self.max_steps,
            candidate_uses_more: self.candidate_uses_more,
            usage_preserved: !self.usage_differs,
        }
    }
}

#[cfg(test)]
#[path = "checker_tests.rs"]
mod tests;
