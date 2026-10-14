//! Ordered finite range checks. Scheduling, clocks and threads stay outside.
//! Reports carry proved local evidence, never store/publication authority.
//! The new proof bodies are candidates until the whole-unit check has run.
use super::*;
#[cfg(verus_keep_ghost)]
#[path = "checker_range_spec.rs"]
pub(crate) mod range_spec;

/// Operational refusals are inconclusive, not comparison verdicts.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub enum RangeError {
    /// The serial checker's unchanged admission, witness or Step refusal.
    Comparison(Failure),
    /// Empty, inverted, or outside the admitted product.
    InvalidRange,
    /// Programs, cap or declared Step limit differ.
    SubjectMismatch,
    /// Reports are not left-to-right adjacent.
    NonAdjacent,
    /// The supplied reservation cannot cover the complete planned range.
    WorkExhausted,
    /// Work accounting cannot be represented exactly.
    ArithmeticOverflow,
    /// Coverage cannot justify the first witness or a complete result.
    IncompleteCoverage,
}

/// Immutable, non-cloneable range evidence tied to its exact borrowed subjects.
/// A mismatch proves only `[lo, matched)` plus the differing pair at `matched`.
/// `evaluated` includes speculative pairs retained through ordered merging.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct RangeReport<'a, 'p> {
    original: &'a V2ScalarProgram<'p>,
    candidate: &'a V2ScalarProgram<'p>,
    cap: u64,
    step_limit: u64,
    size: u64,
    lo: u64,
    hi: u64,
    matched: u64,
    tally: Tally,
    witness: Option<Failure>,
    evaluated: u64,
    reserved_work: u64,
    measured_steps: [u128; 2],
}

#[cfg(verus_keep_ghost)]
verus! {
impl<'a, 'p> RangeReport<'a, 'p> {
    // Every executable construction must prove this predicate. Rust field
    // privacy is not the justification for use_type_invariant at a consumer.
    #[verifier::type_invariant]
    spec fn well_formed(&self) -> bool { range_spec::report(*self) }
    pub closed spec fn progress_view(&self) -> (u64, u64, u64, u64, u64) {
        (self.lo, self.hi, self.matched, self.evaluated, self.reserved_work)
    }
    pub closed spec fn measured_view(&self) -> [u128; 2] { self.measured_steps }
}
}

#[cfg(verus_keep_ghost)]
verus! {
pub closed spec fn scanned(original: &V2ScalarProgram, candidate: &V2ScalarProgram, cap: u64, step_limit: u64, lo: u64, hi: u64, reserved_work: u64, result: Result<RangeReport, RangeError>) -> bool {
    range_spec::scan_result(original, candidate, cap, step_limit, lo, hi, reserved_work, result)
}
pub closed spec fn merged(left: RangeReport, right: RangeReport, result: Result<RangeReport, RangeError>) -> bool {
    match result {
        Ok(r) => range_spec::report(r)
            && range_spec::same_program(r.original, left.original)
            && range_spec::same_program(r.candidate, left.candidate)
            && r.cap == left.cap && r.step_limit == left.step_limit
            && r.lo == left.lo && r.hi == right.hi
            && r.evaluated as int == left.evaluated as int + right.evaluated as int
            && r.reserved_work as int == left.reserved_work as int + right.reserved_work as int
            && r.measured_steps[0] as int == left.measured_steps[0] as int + right.measured_steps[0] as int
            && r.measured_steps[1] as int == left.measured_steps[1] as int + right.measured_steps[1] as int
            && r.matched == if left.witness.is_some() { left.matched } else { right.matched },
        Err(RangeError::SubjectMismatch) => !range_spec::same_program(left.original, right.original)
            || !range_spec::same_program(left.candidate, right.candidate)
            || left.cap != right.cap || left.step_limit != right.step_limit || left.size != right.size,
        Err(RangeError::NonAdjacent) => left.hi != right.lo,
        Err(RangeError::ArithmeticOverflow) => left.reserved_work as int + right.reserved_work as int > u64::MAX,
        _ => false,
    }
}
pub closed spec fn finished(original: &V2ScalarProgram, candidate: &V2ScalarProgram, cap: u64, step_limit: u64, report: RangeReport, result: Result<(u64, Usage), RangeError>) -> bool {
    match result {
        Err(RangeError::SubjectMismatch) => !range_spec::same_program(original, report.original)
            || !range_spec::same_program(candidate, report.candidate)
            || cap != report.cap || step_limit != report.step_limit,
        Err(RangeError::IncompleteCoverage) => report.lo != 0
            || (report.witness.is_none() && report.hi != report.size),
        _ => range_spec::final_comparison(original, candidate, cap, step_limit, result),
    }
}
pub closed spec fn two_ranges_result(original: &V2ScalarProgram, candidate: &V2ScalarProgram, cap: u64, step_limit: u64, split: u64, reserved_work: u64, result: Result<(u64, Usage), RangeError>) -> bool {
    (match result {
        Err(RangeError::InvalidRange) => spec::admitted(original, candidate, cap)
            && !(0 < split <= spec::product(original.inputs@, original.inputs@.len() as int)),
        Err(RangeError::ArithmeticOverflow) => spec::admitted(original, candidate, cap)
            && range_spec::work(original, candidate, 0, spec::product(original.inputs@, original.inputs@.len() as int)) > u64::MAX,
        Err(RangeError::WorkExhausted) => spec::admitted(original, candidate, cap)
            && reserved_work < range_spec::work(original, candidate, 0, spec::product(original.inputs@, original.inputs@.len() as int)) <= u64::MAX,
        _ => range_spec::final_comparison(original, candidate, cap, step_limit, result),
    }) && (spec::admitted(original, candidate, cap)
        && 0 < split <= spec::product(original.inputs@, original.inputs@.len() as int)
        && range_spec::work(original, candidate, 0, spec::product(original.inputs@, original.inputs@.len() as int)) <= reserved_work
        ==> (result.is_ok() == (spec::all_equal(original, candidate)
            && spec::over(original, step_limit, spec::product(original.inputs@, original.inputs@.len() as int)) == 0
            && spec::over(candidate, step_limit, spec::product(original.inputs@, original.inputs@.len() as int)) == 0)))
}
}

impl RangeReport<'_, '_> {
    /// Actual evaluator Step usage, including a mismatching pair and all
    /// speculative ranges merged into this report. Reservations are not refunds.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.measured_view(),))]
    pub fn measured_steps(&self) -> [u128; 2] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(RangeReport::measured_view); }
        self.measured_steps
    }
    /// Planned interval, proved equal endpoint, evaluated pairs, reserved work.
    /// The equal endpoint is local to this interval, not a global prefix.
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == self.progress_view(),))]
    pub fn progress(&self) -> (u64, u64, u64, u64, u64) {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(RangeReport::progress_view); }
        (
            self.lo,
            self.hi,
            self.matched,
            self.evaluated,
            self.reserved_work,
        )
    }
}

/// O(arity) mixed-radix seek, including full-width signed domains.
/// Invalid/empty domains and out-of-product ordinals are refused, never clamped.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures match result {
        Some(xs) => spec::tuple(domains@, xs@)
            && spec::rank(domains@, xs@, domains@.len() as int) == ordinal
            && xs@ == spec::at(domains@, ordinal as int),
        None => !spec::valid(domains@, domains@.len() as int)
            || ordinal >= spec::product(domains@, domains@.len() as int),
    },
))]
pub(super) fn tuple_at(domains: &[Domain], ordinal: u64) -> Option<Vec<i64>> {
    let size = match domain_size(domains) {
        Ok(size) => size,
        Err(_) => return None,
    };
    let outside = match size {
        Some(size) => ordinal as u128 >= size,
        None => false,
    };
    if outside {
        return None;
    }
    let mut tuple = first_tuple(domains);
    let mut position = domains.len();
    let mut remaining = ordinal as u128;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant position <= domains.len(), spec::tuple(domains@, tuple@),
            spec::valid(domains@, domains@.len() as int),
            remaining < spec::product(domains@, position as int),
            range_spec::tail_rank(domains@, tuple@, position as int, domains@.len() as int, remaining as int) == ordinal,
        decreases position,
    ))]
    while position > 0 {
        position -= 1;
        let (min, max) = domains[position].bounds();
        let width = ((max as i128) - (min as i128) + 1) as u128;
        let digit = remaining % width;
        let next = remaining / width;
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost before = tuple@; }
        #[cfg(verus_keep_ghost)]
        proof! {
            spec::product_positive(domains@, position as int);
            vstd::arithmetic::div_mod::lemma_fundamental_div_mod(remaining as int, width as int);
            vstd::arithmetic::div_mod::lemma_mod_bound(remaining as int, width as int);
            let p = spec::product(domains@, position as int);
            assert(remaining as int == next as int * width as int + digit as int) by(nonlinear_arith)
                requires remaining as int == width as int * next as int + digit as int, width > 0;
            assert(next < p) by(nonlinear_arith)
                requires remaining < p * width, remaining == next * width + digit,
                    0 <= digit < width, width > 0;
        }
        tuple[position] = ((min as i128) + (digit as i128)) as i64;
        #[cfg(verus_keep_ghost)]
        proof! {
            range_spec::unrank_step(domains@, before, tuple@, position as int,
                domains@.len() as int, remaining as int, next as int);
        }
        remaining = next;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        range_spec::tail_zero(domains@, tuple@, domains@.len() as int);
        spec::at_rank(domains@, tuple@);
    }
    Some(tuple)
}

/// Exact same-subject test. Neither pointer nor hash identity is sufficient.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == range_spec::same_program(a, b),))]
fn same_program(a: &V2ScalarProgram<'_>, b: &V2ScalarProgram<'_>) -> bool {
    equality::domains(a.inputs, b.inputs)
        && equality::domains(a.outputs, b.outputs)
        && equality::nodes(a.nodes, b.nodes)
        && equality::roots(a.roots, b.roots)
}

/// Admission precedes range/work refusal. No evaluation occurs before the
/// complete range reservation is checked; Steps never terminate a local scan.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures scanned(original, candidate, cap, step_limit, lo, hi, reserved_work, result),))]
pub fn scan_range<'a, 'p>(
    original: &'a V2ScalarProgram<'p>,
    candidate: &'a V2ScalarProgram<'p>,
    cap: u64,
    step_limit: u64,
    lo: u64,
    hi: u64,
    reserved_work: u64,
) -> Result<RangeReport<'a, 'p>, RangeError> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(scanned); }
    let size = match validate_pair(original, candidate, cap) {
        Ok(size) => size,
        Err(error) => return Err(RangeError::Comparison(error)),
    };
    if lo >= hi || hi > size {
        return Err(RangeError::InvalidRange);
    }
    let cost = original.nodes.len() as u128 + candidate.nodes.len() as u128 + 2;
    let required = match ((hi - lo) as u128).checked_mul(cost) {
        Some(required) if required <= u64::MAX as u128 => required as u64,
        _ => return Err(RangeError::ArithmeticOverflow),
    };
    if reserved_work < required {
        return Err(RangeError::WorkExhausted);
    }
    let mut input = match tuple_at(original.inputs, lo) {
        Some(input) => input,
        None => return Err(RangeError::IncompleteCoverage),
    };
    let mut ordinal = lo;
    let mut tally = new_tally();
    let mut measured_steps = [0_u128; 2];
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant lo <= ordinal <= hi <= size, lo < hi,
            spec::admitted(original, candidate, cap),
            size == spec::product(original.inputs@, original.inputs@.len() as int),
            spec::tuple(original.inputs@, input@),
            ordinal < hi ==> spec::rank(original.inputs@, input@, original.inputs@.len() as int) == ordinal,
            range_spec::equal(original, candidate, lo as int, ordinal as int),
            range_spec::tally(tally, original, candidate, step_limit, lo as int, ordinal as int),
            tally.over_limit[0] <= ordinal - lo, tally.over_limit[1] <= ordinal - lo,
            tally.candidate_uses_more <= ordinal - lo,
            measured_steps[0] == range_spec::steps(original, lo as int, ordinal as int),
            measured_steps[1] == range_spec::steps(candidate, lo as int, ordinal as int),
            measured_steps[0] <= (ordinal as int - lo as int) * u64::MAX as int,
            measured_steps[1] <= (ordinal as int - lo as int) * u64::MAX as int,
            required == range_spec::work(original, candidate, lo as int, hi as int), required <= reserved_work,
        decreases hi - ordinal,
    ))]
    while ordinal < hi {
        #[cfg(verus_keep_ghost)]
        proof! { spec::at_rank(original.inputs@, input@); }
        let left = observe(original, &input);
        let right = observe(candidate, &input);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert((ordinal as int - lo as int + 1) * u64::MAX as int <= u128::MAX) by(nonlinear_arith)
                requires 0 <= (ordinal as int - lo as int), (ordinal as int - lo as int) < u64::MAX;
        }
        measured_steps[0] += left.steps as u128;
        measured_steps[1] += right.steps as u128;
        if !equality::results(&left.result, &right.result) {
            return Ok(RangeReport {
                original,
                candidate,
                cap,
                step_limit,
                size,
                lo,
                hi,
                matched: ordinal,
                tally,
                witness: Some(Failure::Counterexample {
                    ordinal,
                    input,
                    original: left,
                    candidate: right,
                }),
                evaluated: ordinal - lo + 1,
                reserved_work: required,
                measured_steps,
            });
        }
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost before = tally; }
        tally.record(left.steps, right.steps, step_limit);
        #[cfg(verus_keep_ghost)]
        proof! {
            range_spec::extend(before, tally, original, candidate, step_limit,
                lo as int, ordinal as int, left.steps, right.steps);
            assert forall|k: int| lo <= k < ordinal + 1 implies
                spec::observation(original, spec::at(original.inputs@, k)).0
                    == spec::observation(candidate, spec::at(original.inputs@, k)).0 by {}
        }
        ordinal += 1;
        if ordinal < hi && !advance(original.inputs, &mut input) {
            return Err(RangeError::IncompleteCoverage);
        }
    }
    Ok(RangeReport {
        original,
        candidate,
        cap,
        step_limit,
        size,
        lo,
        hi,
        matched: hi,
        tally,
        witness: None,
        evaluated: hi - lo,
        reserved_work: required,
        measured_steps,
    })
}

/// Consume adjacent reports in ordinal order. Speculative work is added even
/// when the left witness makes the right report irrelevant to the verdict.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures merged(left, right, result),))]
pub fn merge_adjacent<'a, 'p>(
    left: RangeReport<'a, 'p>,
    right: RangeReport<'a, 'p>,
) -> Result<RangeReport<'a, 'p>, RangeError> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(merged); }
    #[cfg(verus_keep_ghost)]
    proof! { use_type_invariant(&left); use_type_invariant(&right); }
    if !same_program(left.original, right.original)
        || !same_program(left.candidate, right.candidate)
        || left.cap != right.cap
        || left.step_limit != right.step_limit
        || left.size != right.size
    {
        return Err(RangeError::SubjectMismatch);
    }
    if left.hi != right.lo {
        return Err(RangeError::NonAdjacent);
    }
    let reserved_work = match left.reserved_work.checked_add(right.reserved_work) {
        Some(work) => work,
        None => return Err(RangeError::ArithmeticOverflow),
    };
    let evaluated = left.evaluated + right.evaluated;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(range_spec::work(left.original, left.candidate, left.lo as int, right.hi as int)
            == left.reserved_work as int + right.reserved_work as int) by(nonlinear_arith)
            requires left.reserved_work == range_spec::work(left.original, left.candidate, left.lo as int, left.hi as int),
                right.reserved_work == range_spec::work(left.original, left.candidate, right.lo as int, right.hi as int),
                left.hi == right.lo;
        assert(evaluated as int * u64::MAX as int <= u128::MAX) by(nonlinear_arith)
            requires 0 <= evaluated, evaluated <= u64::MAX;
    }
    let measured_steps = [
        left.measured_steps[0] + right.measured_steps[0],
        left.measured_steps[1] + right.measured_steps[1],
    ];
    let hi = right.hi;
    if left.witness.is_some() {
        return Ok(RangeReport {
            hi,
            evaluated,
            reserved_work,
            measured_steps,
            ..left
        });
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        range_spec::transport_tally(right.tally, right.original, right.candidate,
            left.original, left.candidate, left.step_limit, right.lo as int, right.matched as int);
        range_spec::interval_split(left.original, left.candidate, left.step_limit,
            left.lo as int, left.hi as int, right.matched as int);
    }
    let tally = Tally {
        max_steps: [
            left.tally.max_steps[0].max(right.tally.max_steps[0]),
            left.tally.max_steps[1].max(right.tally.max_steps[1]),
        ],
        over_limit: [
            left.tally.over_limit[0] + right.tally.over_limit[0],
            left.tally.over_limit[1] + right.tally.over_limit[1],
        ],
        candidate_uses_more: left.tally.candidate_uses_more + right.tally.candidate_uses_more,
        usage_differs: left.tally.usage_differs || right.tally.usage_differs,
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(range_spec::equal(left.original, left.candidate, left.lo as int, right.matched as int));
        assert(range_spec::tally(tally, left.original, left.candidate, left.step_limit,
            left.lo as int, right.matched as int));
        assert(match right.witness {
            None => right.matched == hi && evaluated == hi - left.lo,
            Some(error) => range_spec::local_witness(left.original, left.candidate,
                left.lo as int, right.matched as int, hi as int, error)
                && evaluated > right.matched - left.lo,
        });
    }
    Ok(RangeReport {
        original: left.original,
        candidate: left.candidate,
        cap: left.cap,
        step_limit: left.step_limit,
        size: left.size,
        lo: left.lo,
        hi,
        matched: right.matched,
        tally,
        witness: right.witness,
        evaluated,
        reserved_work,
        measured_steps,
    })
}

/// Revalidate the expected pair and require a complete prefix before exposing
/// any verdict. A mismatch needs no suffix; equality needs the whole domain.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures finished(original, candidate, cap, step_limit, report, result),))]
pub fn finish_range(
    original: &V2ScalarProgram<'_>,
    candidate: &V2ScalarProgram<'_>,
    cap: u64,
    step_limit: u64,
    report: RangeReport<'_, '_>,
) -> Result<(u64, Usage), RangeError> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(finished); }
    #[cfg(verus_keep_ghost)]
    proof! { use_type_invariant(&report); }
    let size = match validate_pair(original, candidate, cap) {
        Ok(size) => size,
        Err(error) => return Err(RangeError::Comparison(error)),
    };
    if !same_program(original, report.original)
        || !same_program(candidate, report.candidate)
        || cap != report.cap
        || step_limit != report.step_limit
    {
        return Err(RangeError::SubjectMismatch);
    }
    if report.lo != 0 {
        return Err(RangeError::IncompleteCoverage);
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        range_spec::transport_tally(report.tally, report.original, report.candidate,
            original, candidate, step_limit, report.lo as int, report.matched as int);
        range_spec::zero_origin(original, candidate, step_limit, report.matched as int);
    }
    if let Some(witness) = report.witness {
        return Err(RangeError::Comparison(witness));
    }
    if report.hi != size {
        return Err(RangeError::IncompleteCoverage);
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        spec::prefix_covers(original, candidate);
        assert(spec::tally(report.tally, original, candidate, step_limit, size as int));
    }
    let usage = report.tally.usage();
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(spec::usage(usage, original, candidate, size as int));
        assert(report.tally.over_limit[0] == spec::over(original, step_limit, size as int));
        assert(report.tally.over_limit[1] == spec::over(candidate, step_limit, size as int));
    }
    if report.tally.over_limit != [0, 0] {
        return Err(RangeError::Comparison(Failure::BudgetBoundary {
            over_limit: report.tally.over_limit,
            minimum_limit: usage.max_steps[0].max(usage.max_steps[1]),
            usage,
        }));
    }
    Ok((size, usage))
}

/// Actual producer/consumer composition with at most two retained reports.
/// `split == size` uses one range, including the zero-arity singleton case.
/// This is a serial proof/qualification path, not a parallel scheduler.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures two_ranges_result(original, candidate, cap, step_limit, split, reserved_work, result),))]
pub fn compare_two_ranges(
    original: &V2ScalarProgram<'_>,
    candidate: &V2ScalarProgram<'_>,
    cap: u64,
    step_limit: u64,
    split: u64,
    reserved_work: u64,
) -> Result<(u64, Usage), RangeError> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(two_ranges_result); }
    let size = match validate_pair(original, candidate, cap) {
        Ok(size) => size,
        Err(error) => return Err(RangeError::Comparison(error)),
    };
    if split == 0 || split > size {
        return Err(RangeError::InvalidRange);
    }
    let cost = original.nodes.len() as u128 + candidate.nodes.len() as u128 + 2;
    let required = match (size as u128).checked_mul(cost) {
        Some(work) if work <= u64::MAX as u128 => work as u64,
        _ => return Err(RangeError::ArithmeticOverflow),
    };
    if required > reserved_work {
        return Err(RangeError::WorkExhausted);
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(split as int * cost as int <= size as int * cost as int) by(nonlinear_arith)
            requires split <= size, cost >= 0;
    }
    let left_work = ((split as u128) * cost) as u64;
    let left = scan_range(original, candidate, cap, step_limit, 0, split, left_work)?;
    let combined = if split < size {
        #[cfg(verus_keep_ghost)]
        proof! {
            assert((size as int - split as int) * cost as int
                == required as int - left_work as int) by(nonlinear_arith)
                requires required as int == size as int * cost as int,
                    left_work as int == split as int * cost as int;
        }
        let right = scan_range(
            original,
            candidate,
            cap,
            step_limit,
            split,
            size,
            required - left_work,
        )?;
        merge_adjacent(left, right)?
    } else {
        left
    };
    let result = finish_range(original, candidate, cap, step_limit, combined);
    #[cfg(verus_keep_ghost)]
    proof! {
        range_spec::serial_projection(original, candidate, cap, step_limit, result);
        if let Err(RangeError::Comparison(Failure::Counterexample { ordinal, ref input, .. })) = result {
            spec::at_rank(original.inputs@, input@);
        }
    }
    result
}

#[cfg(test)]
#[path = "checker_range_tests.rs"]
mod tests;
