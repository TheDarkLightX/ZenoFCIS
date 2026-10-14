//! Interval semantics over the same actual evaluator as the serial checker.
//! Proof candidates: these bodies must pass the pinned whole checker unit.
use super::*;
use vstd::prelude::*;

verus! {
pub(crate) open spec fn same_program(a: &V2ScalarProgram, b: &V2ScalarProgram) -> bool {
    a.inputs@ == b.inputs@ && a.outputs@ == b.outputs@
        && a.nodes@ == b.nodes@ && a.roots@ == b.roots@
}
pub(crate) open spec fn equal(a: &V2ScalarProgram, b: &V2ScalarProgram, lo: int, hi: int) -> bool {
    forall|k: int| lo <= k < hi ==> spec::observation(a, spec::at(a.inputs@, k)).0
        == spec::observation(b, spec::at(a.inputs@, k)).0
}
pub(crate) open spec fn maximum(p: &V2ScalarProgram, lo: int, hi: int) -> u64
    decreases hi - lo,
{
    if hi <= lo { 0 } else {
        let prior = maximum(p, lo, hi - 1);
        let step = spec::observation(p, spec::at(p.inputs@, hi - 1)).1;
        if step > prior { step } else { prior }
    }
}
pub(crate) open spec fn over(p: &V2ScalarProgram, limit: u64, lo: int, hi: int) -> int
    decreases hi - lo,
{
    if hi <= lo { 0 } else { over(p, limit, lo, hi - 1)
        + if spec::observation(p, spec::at(p.inputs@, hi - 1)).1 > limit { 1int } else { 0int } }
}
pub(crate) open spec fn more(a: &V2ScalarProgram, b: &V2ScalarProgram, lo: int, hi: int) -> int
    decreases hi - lo,
{
    if hi <= lo { 0 } else { more(a, b, lo, hi - 1)
        + if spec::observation(b, spec::at(a.inputs@, hi - 1)).1
            > spec::observation(a, spec::at(a.inputs@, hi - 1)).1 { 1int } else { 0int } }
}
pub(crate) open spec fn same_usage(a: &V2ScalarProgram, b: &V2ScalarProgram, lo: int, hi: int) -> bool {
    forall|k: int| lo <= k < hi ==> spec::observation(a, spec::at(a.inputs@, k)).1
        == spec::observation(b, spec::at(a.inputs@, k)).1
}
pub(super) open spec fn tally(t: Tally, a: &V2ScalarProgram, b: &V2ScalarProgram,
    limit: u64, lo: int, hi: int) -> bool {
    t.max_steps[0] == maximum(a, lo, hi) && t.max_steps[1] == maximum(b, lo, hi)
        && t.over_limit[0] == over(a, limit, lo, hi) && t.over_limit[1] == over(b, limit, lo, hi)
        && t.candidate_uses_more == more(a, b, lo, hi) && t.usage_differs == !same_usage(a, b, lo, hi)
}
/// Structural subject equality preserves the actual evaluator at every input.
/// Induction transports recursive statistics without equating borrowed objects.
pub(super) proof fn transport_tally(t: Tally, a: &V2ScalarProgram, b: &V2ScalarProgram,
    c: &V2ScalarProgram, d: &V2ScalarProgram, limit: u64, lo: int, hi: int)
    requires same_program(a, c), same_program(b, d), 0 <= lo <= hi,
    ensures maximum(a, lo, hi) == maximum(c, lo, hi), maximum(b, lo, hi) == maximum(d, lo, hi),
        over(a, limit, lo, hi) == over(c, limit, lo, hi), over(b, limit, lo, hi) == over(d, limit, lo, hi),
        more(a, b, lo, hi) == more(c, d, lo, hi), same_usage(a, b, lo, hi) == same_usage(c, d, lo, hi),
        tally(t, a, b, limit, lo, hi) == tally(t, c, d, limit, lo, hi),
    decreases hi - lo,
{
    if hi > lo {
        transport_tally(t, a, b, c, d, limit, lo, hi - 1);
        assert(spec::observation(a, spec::at(a.inputs@, hi - 1))
            == spec::observation(c, spec::at(c.inputs@, hi - 1)));
        assert(spec::observation(b, spec::at(b.inputs@, hi - 1))
            == spec::observation(d, spec::at(d.inputs@, hi - 1)));
        assert(spec::observation(b, spec::at(a.inputs@, hi - 1))
            == spec::observation(d, spec::at(c.inputs@, hi - 1)));
    }
    assert forall|k: int| lo <= k < hi implies
        (spec::observation(a, spec::at(a.inputs@, k)).1 == spec::observation(b, spec::at(a.inputs@, k)).1)
        == (spec::observation(c, spec::at(c.inputs@, k)).1 == spec::observation(d, spec::at(c.inputs@, k)).1) by {}
    if same_usage(a, b, lo, hi) {
        assert forall|k: int| lo <= k < hi implies spec::observation(c, spec::at(c.inputs@, k)).1
            == spec::observation(d, spec::at(c.inputs@, k)).1 by {}
    }
    if same_usage(c, d, lo, hi) {
        assert forall|k: int| lo <= k < hi implies spec::observation(a, spec::at(a.inputs@, k)).1
            == spec::observation(b, spec::at(a.inputs@, k)).1 by {}
    }
}
pub(crate) open spec fn local_witness(a: &V2ScalarProgram, b: &V2ScalarProgram,
    lo: int, matched: int, hi: int, error: Failure) -> bool {
    match error {
        Failure::Counterexample { ordinal, input, original, candidate } =>
            lo <= matched < hi && ordinal == matched && equal(a, b, lo, matched)
            && spec::tuple(a.inputs@, input@) && spec::rank(a.inputs@, input@, a.inputs@.len() as int) == matched
            && spec::observation_view(original) == spec::observation(a, input@)
            && spec::observation_view(candidate) == spec::observation(b, input@)
            && spec::observation_view(original).0 != spec::observation_view(candidate).0,
        _ => false,
    }
}
pub(crate) open spec fn work(a: &V2ScalarProgram, b: &V2ScalarProgram, lo: int, hi: int) -> int {
    (hi - lo) * (a.nodes@.len() + b.nodes@.len() + 2)
}
pub(crate) open spec fn steps(p: &V2ScalarProgram, lo: int, hi: int) -> int
    decreases hi - lo,
{
    if hi <= lo { 0 } else { steps(p, lo, hi - 1) + spec::observation(p, spec::at(p.inputs@, hi - 1)).1 }
}
pub(super) open spec fn report(r: RangeReport) -> bool {
    spec::admitted(r.original, r.candidate, r.cap)
        && r.size == spec::product(r.original.inputs@, r.original.inputs@.len() as int)
        && r.lo < r.hi <= r.size && r.lo <= r.matched <= r.hi
        && equal(r.original, r.candidate, r.lo as int, r.matched as int)
        && tally(r.tally, r.original, r.candidate, r.step_limit, r.lo as int, r.matched as int)
        && r.tally.over_limit[0] <= r.matched - r.lo
        && r.tally.over_limit[1] <= r.matched - r.lo
        && r.tally.candidate_uses_more <= r.matched - r.lo
        && r.matched - r.lo <= r.evaluated <= r.hi - r.lo
        && r.reserved_work == work(r.original, r.candidate, r.lo as int, r.hi as int)
        && r.measured_steps[0] <= r.evaluated as int * u64::MAX as int
        && r.measured_steps[1] <= r.evaluated as int * u64::MAX as int
        && match r.witness {
            None => r.matched == r.hi && r.evaluated == r.hi - r.lo,
            Some(error) => local_witness(r.original, r.candidate, r.lo as int, r.matched as int, r.hi as int, error)
                && r.evaluated > r.matched - r.lo,
        }
}
pub(super) open spec fn scan_result(a: &V2ScalarProgram, b: &V2ScalarProgram,
    cap: u64, limit: u64, lo: u64, hi: u64, reservation: u64,
    result: Result<RangeReport, RangeError>) -> bool {
    let n = spec::product(a.inputs@, a.inputs@.len() as int);
    match result {
        Err(RangeError::Comparison(error)) => spec::admission_result(a, b, cap, Err(error)),
        Err(RangeError::InvalidRange) => spec::admitted(a, b, cap) && !(lo < hi <= n),
        Err(RangeError::ArithmeticOverflow) => spec::admitted(a, b, cap) && lo < hi <= n
            && work(a, b, lo as int, hi as int) > u64::MAX,
        Err(RangeError::WorkExhausted) => spec::admitted(a, b, cap) && lo < hi <= n
            && reservation < work(a, b, lo as int, hi as int) <= u64::MAX,
        Ok(r) => report(r) && same_program(r.original, a) && same_program(r.candidate, b)
            && r.cap == cap && r.step_limit == limit && r.lo == lo && r.hi == hi
            && r.reserved_work == work(a, b, lo as int, hi as int) <= reservation
            && r.evaluated == r.matched - lo + if r.witness.is_some() { 1int } else { 0int }
            && r.measured_steps[0] == steps(a, lo as int, lo as int + r.evaluated as int)
            && r.measured_steps[1] == steps(b, lo as int, lo as int + r.evaluated as int),
        _ => false,
    }
}

/// Rebuild a full rank from a prefix rank and the remaining fixed digits.
pub(crate) open spec fn tail_rank(ds: Seq<Domain>, xs: Seq<i64>, start: int, n: int, prefix: int) -> int
    decreases n - start,
{
    if n <= start { prefix } else { tail_rank(ds, xs, start, n - 1, prefix)
        * spec::width(ds[n - 1]) + xs[n - 1] - spec::lo(ds[n - 1]) }
}
pub(crate) proof fn tail_zero(ds: Seq<Domain>, xs: Seq<i64>, n: int)
    requires 0 <= n <= ds.len(), xs.len() == ds.len(),
    ensures tail_rank(ds, xs, 0, n, 0) == spec::rank(ds, xs, n),
    decreases n,
{ if n > 0 { tail_zero(ds, xs, n - 1); } }
pub(crate) proof fn unrank_step(ds: Seq<Domain>, old: Seq<i64>, new: Seq<i64>, pos: int, n: int,
    prior: int, next: int)
    requires 0 <= pos < n <= ds.len(), old.len() == ds.len(), new.len() == ds.len(),
        prior == next * spec::width(ds[pos]) + new[pos] - spec::lo(ds[pos]),
        forall|i: int| pos < i < n ==> #[trigger] old[i] == new[i],
    ensures tail_rank(ds, old, pos + 1, n, prior) == tail_rank(ds, new, pos, n, next),
    decreases n - pos,
{
    reveal_with_fuel(tail_rank, 2);
    if n > pos + 1 {
        unrank_step(ds, old, new, pos, n - 1, prior, next);
    } else {
        assert(tail_rank(ds, old, pos + 1, n, prior) == prior);
        assert(tail_rank(ds, new, pos, pos, next) == next);
    }
}

pub(super) proof fn extend(old: Tally, new: Tally, a: &V2ScalarProgram, b: &V2ScalarProgram,
    limit: u64, lo: int, hi: int, left: u64, right: u64)
    requires a.inputs@ == b.inputs@, 0 <= lo <= hi < u64::MAX,
        tally(old, a, b, limit, lo, hi), spec::recorded(old, new, left, right, limit),
        old.over_limit[0] <= hi - lo, old.over_limit[1] <= hi - lo, old.candidate_uses_more <= hi - lo,
        left == spec::observation(a, spec::at(a.inputs@, hi)).1,
        right == spec::observation(b, spec::at(a.inputs@, hi)).1,
    ensures tally(new, a, b, limit, lo, hi + 1),
{
    assert(same_usage(a, b, lo, hi + 1) == (same_usage(a, b, lo, hi) && left == right)) by {
        if same_usage(a, b, lo, hi) && left == right {
            assert forall|k: int| lo <= k < hi + 1 implies
                spec::observation(a, spec::at(a.inputs@, k)).1 == spec::observation(b, spec::at(a.inputs@, k)).1
                    by { if k < hi {} }
        }
        if same_usage(a, b, lo, hi + 1) {
            assert forall|k: int| lo <= k < hi implies
                spec::observation(a, spec::at(a.inputs@, k)).1 == spec::observation(b, spec::at(a.inputs@, k)).1 by {}
        }
    }
}
pub(crate) proof fn interval_split(a: &V2ScalarProgram, b: &V2ScalarProgram,
    limit: u64, lo: int, mid: int, hi: int)
    requires 0 <= lo <= mid <= hi,
    ensures equal(a, b, lo, hi) == (equal(a, b, lo, mid) && equal(a, b, mid, hi)),
        same_usage(a, b, lo, hi) == (same_usage(a, b, lo, mid) && same_usage(a, b, mid, hi)),
        over(a, limit, lo, hi) == over(a, limit, lo, mid) + over(a, limit, mid, hi),
        over(b, limit, lo, hi) == over(b, limit, lo, mid) + over(b, limit, mid, hi),
        more(a, b, lo, hi) == more(a, b, lo, mid) + more(a, b, mid, hi),
        maximum(a, lo, hi) == if maximum(a, lo, mid) > maximum(a, mid, hi) { maximum(a, lo, mid) } else { maximum(a, mid, hi) },
        maximum(b, lo, hi) == if maximum(b, lo, mid) > maximum(b, mid, hi) { maximum(b, lo, mid) } else { maximum(b, mid, hi) },
    decreases hi - mid,
{
    if hi > mid { interval_split(a, b, limit, lo, mid, hi - 1); }
    assert(equal(a, b, lo, hi) == (equal(a, b, lo, mid) && equal(a, b, mid, hi))) by {
        if equal(a, b, lo, mid) && equal(a, b, mid, hi) {
            assert forall|k: int| lo <= k < hi implies spec::observation(a, spec::at(a.inputs@, k)).0
                == spec::observation(b, spec::at(a.inputs@, k)).0 by { if k < mid {} }
        }
    }
    assert(same_usage(a, b, lo, hi) == (same_usage(a, b, lo, mid) && same_usage(a, b, mid, hi))) by {
        if same_usage(a, b, lo, mid) && same_usage(a, b, mid, hi) {
            assert forall|k: int| lo <= k < hi implies spec::observation(a, spec::at(a.inputs@, k)).1
                == spec::observation(b, spec::at(a.inputs@, k)).1 by { if k < mid {} }
        }
    }
}
pub(crate) proof fn zero_origin(a: &V2ScalarProgram, b: &V2ScalarProgram, limit: u64, n: int)
    requires n >= 0,
    ensures maximum(a, 0, n) == spec::maximum(a, n), maximum(b, 0, n) == spec::maximum(b, n),
        over(a, limit, 0, n) == spec::over(a, limit, n), over(b, limit, 0, n) == spec::over(b, limit, n),
        more(a, b, 0, n) == spec::more(a, b, n), same_usage(a, b, 0, n) == spec::same_usage(a, b, n),
        equal(a, b, 0, n) == spec::equal_prefix(a, b, n),
    decreases n,
{ if n > 0 { zero_origin(a, b, limit, n - 1); } }

/// Public data projection, with no executable Complete/Equal construction.
pub(crate) open spec fn final_comparison(a: &V2ScalarProgram, b: &V2ScalarProgram,
    cap: u64, limit: u64, result: Result<(u64, Usage), RangeError>) -> bool {
    let n = spec::product(a.inputs@, a.inputs@.len() as int);
    match result {
        Ok((count, used)) => spec::admitted(a, b, cap) && spec::all_equal(a, b)
            && count == n && spec::usage(used, a, b, n)
            && spec::over(a, limit, n) == 0 && spec::over(b, limit, n) == 0,
        Err(RangeError::Comparison(Failure::BudgetBoundary { over_limit, minimum_limit, usage })) =>
            spec::admitted(a, b, cap) && spec::all_equal(a, b) && spec::usage(usage, a, b, n)
            && over_limit[0] == spec::over(a, limit, n) && over_limit[1] == spec::over(b, limit, n)
            && (over_limit[0] > 0 || over_limit[1] > 0)
            && minimum_limit == if spec::maximum(a, n) > spec::maximum(b, n) { spec::maximum(a, n) } else { spec::maximum(b, n) },
        Err(RangeError::Comparison(error)) => spec::admission_result(a, b, cap, Err(error))
            || (spec::admitted(a, b, cap) && spec::witness(a, b, error)),
        _ => false,
    }
}

/// The public result predicate is the serial theorem's exact data projection.
/// This is a ghost witness only; no range route constructs runtime Complete.
pub(crate) proof fn serial_projection(a: &V2ScalarProgram, b: &V2ScalarProgram,
    cap: u64, limit: u64, result: Result<(u64, Usage), RangeError>)
    requires final_comparison(a, b, cap, limit, result),
    ensures match result {
        Ok((n, used)) => exists|done: Complete| spec::comparison_result(a, b, cap, limit, Ok(done))
            && done.view() == (n, used),
        Err(RangeError::Comparison(error)) => spec::comparison_result(a, b, cap, limit, Err(error)),
        _ => false,
    },
{
    reveal(Complete::view);
    if let Ok((n, used)) = result {
        let done = Complete { tuples: n, usage: used };
        assert(spec::comparison_result(a, b, cap, limit, Ok(done)));
        assert(done.view() == (n, used));
    }
}
}
