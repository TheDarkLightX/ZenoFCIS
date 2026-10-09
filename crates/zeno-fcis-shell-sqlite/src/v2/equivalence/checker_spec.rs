//! Ghost semantics and arithmetic lemmas for the shipped shared checker.
//! Observation uses the real public metered evaluator specification.
use super::*;
use vstd::prelude::*;

verus! {
pub(crate) open spec fn lo(d: Domain) -> int { match d { Domain::Bool => 0, Domain::Int { min, .. } => min as int } }
pub(crate) open spec fn hi(d: Domain) -> int { match d { Domain::Bool => 1, Domain::Int { max, .. } => max as int } }
pub(crate) open spec fn width(d: Domain) -> int { hi(d) - lo(d) + 1 }
pub(crate) open spec fn valid(ds: Seq<Domain>, n: int) -> bool {
    0 <= n <= ds.len() && forall|i: int| 0 <= i < n ==> #[trigger] lo(ds[i]) <= hi(ds[i])
}
pub(crate) open spec fn product(ds: Seq<Domain>, n: int) -> int
    decreases n,
{
    if n <= 0 { 1 } else { product(ds, n - 1) * width(ds[n - 1]) }
}
pub(crate) open spec fn tuple(ds: Seq<Domain>, xs: Seq<i64>) -> bool {
    xs.len() == ds.len() && forall|i: int| 0 <= i < ds.len() ==> lo(ds[i]) <= #[trigger] xs[i] && xs[i] <= hi(ds[i])
}
pub(crate) open spec fn rank(ds: Seq<Domain>, xs: Seq<i64>, n: int) -> int
    decreases n,
{
    if n <= 0 { 0 } else { rank(ds, xs, n - 1) * width(ds[n - 1]) + xs[n - 1] - lo(ds[n - 1]) }
}
pub(crate) open spec fn minimum(ds: Seq<Domain>, xs: Seq<i64>) -> bool {
    xs.len() == ds.len() && forall|i: int| 0 <= i < ds.len() ==> #[trigger] xs[i] == lo(ds[i])
}
pub(crate) open spec fn size_result(ds: Seq<Domain>, result: Result<Option<u128>, usize>) -> bool {
    match result {
        Err(i) => i < ds.len() && valid(ds, i as int) && lo(ds[i as int]) > hi(ds[i as int]),
        Ok(n) => valid(ds, ds.len() as int) && match n {
            Some(n) => n as int == product(ds, ds.len() as int),
            None => product(ds, ds.len() as int) > u128::MAX,
        },
    }
}

pub(crate) proof fn product_positive(ds: Seq<Domain>, n: int)
    requires valid(ds, n),
    ensures product(ds, n) >= 1,
    decreases n,
{
    if n > 0 {
        product_positive(ds, n - 1);
        assert(width(ds[n - 1]) >= 1);
        assert(product(ds, n - 1) * width(ds[n - 1]) >= 1) by(nonlinear_arith)
            requires product(ds, n - 1) >= 1, width(ds[n - 1]) >= 1;
    }
}

pub(crate) proof fn rank_bounds(ds: Seq<Domain>, xs: Seq<i64>, n: int)
    requires tuple(ds, xs), 0 <= n <= ds.len(),
    ensures valid(ds, n), 0 <= rank(ds, xs, n) < product(ds, n),
    decreases n,
{
    if n > 0 {
        rank_bounds(ds, xs, n - 1);
        let p = product(ds, n - 1);
        let r = rank(ds, xs, n - 1);
        let w = width(ds[n - 1]);
        let digit = xs[n - 1] - lo(ds[n - 1]);
        assert(0 <= digit < w);
        assert(0 <= r * w + digit < p * w) by(nonlinear_arith)
            requires 0 <= r < p, 0 <= digit < w;
    }
}

pub(crate) proof fn rank_injective(ds: Seq<Domain>, x: Seq<i64>, y: Seq<i64>, n: int)
    requires tuple(ds, x), tuple(ds, y), 0 <= n <= ds.len(),
        rank(ds, x, n) == rank(ds, y, n),
    ensures forall|i: int| 0 <= i < n ==> #[trigger] x[i] == y[i],
    decreases n,
{
    if n > 0 {
        rank_bounds(ds, x, n - 1);
        rank_bounds(ds, y, n - 1);
        let w = width(ds[n - 1]);
        let a = rank(ds, x, n - 1);
        let b = rank(ds, y, n - 1);
        let dx = x[n - 1] - lo(ds[n - 1]);
        let dy = y[n - 1] - lo(ds[n - 1]);
        assert(0 <= dx < w && 0 <= dy < w);
        assert(a * w + dx == b * w + dy);
        // Separate the integer gap from multiplication; a single nonlinear
        // uniqueness query unnecessarily asks the solver to discover it.
        if a < b {
            assert(a + 1 <= b);
            assert(a * w + dx < b * w + dy) by(nonlinear_arith)
                requires a + 1 <= b, w > 0, 0 <= dx < w, 0 <= dy < w;
        }
        if b < a {
            assert(b + 1 <= a);
            assert(b * w + dy < a * w + dx) by(nonlinear_arith)
                requires b + 1 <= a, w > 0, 0 <= dx < w, 0 <= dy < w;
        }
        assert(a == b);
        assert(dx == dy);
        rank_injective(ds, x, y, n - 1);
    }
}

pub(crate) proof fn minimum_rank(ds: Seq<Domain>, xs: Seq<i64>, n: int)
    requires minimum(ds, xs), 0 <= n <= ds.len(),
    ensures rank(ds, xs, n) == 0,
    decreases n,
{
    if n > 0 { minimum_rank(ds, xs, n - 1); }
}

pub(crate) proof fn maximum_rank(ds: Seq<Domain>, xs: Seq<i64>, n: int)
    requires tuple(ds, xs), 0 <= n <= ds.len(),
        forall|i: int| 0 <= i < n ==> #[trigger] xs[i] == hi(ds[i]),
    ensures rank(ds, xs, n) == product(ds, n) - 1,
    decreases n,
{
    if n > 0 {
        maximum_rank(ds, xs, n - 1);
        let p = product(ds, n - 1);
        let w = width(ds[n - 1]);
        assert((p - 1) * w + w - 1 == p * w - 1) by(nonlinear_arith);
    }
}

/// A carry increments one digit and resets its maximal suffix.
pub(crate) proof fn carry_rank(ds: Seq<Domain>, x: Seq<i64>, y: Seq<i64>, pivot: int, n: int)
    requires tuple(ds, x), tuple(ds, y), 0 <= pivot < n <= ds.len(),
        forall|i: int| 0 <= i < pivot ==> #[trigger] x[i] == y[i],
        y[pivot] == x[pivot] + 1,
        forall|i: int| pivot < i < n ==> #[trigger] x[i] == hi(ds[i]) && y[i] == lo(ds[i]),
    ensures rank(ds, y, n) == rank(ds, x, n) + 1,
    decreases n,
{
    if n == pivot + 1 {
        same_prefix_rank(ds, x, y, pivot);
    } else {
        carry_rank(ds, x, y, pivot, n - 1);
        let a = rank(ds, x, n - 1);
        let w = width(ds[n - 1]);
        assert((a + 1) * w == a * w + w - 1 + 1) by(nonlinear_arith);
    }
}

pub(crate) proof fn same_prefix_rank(ds: Seq<Domain>, x: Seq<i64>, y: Seq<i64>, n: int)
    requires 0 <= n <= ds.len(), x.len() == ds.len(), y.len() == ds.len(),
        forall|i: int| 0 <= i < n ==> #[trigger] x[i] == y[i],
    ensures rank(ds, x, n) == rank(ds, y, n),
    decreases n,
{
    if n > 0 { same_prefix_rank(ds, x, y, n - 1); }
}

/// Every ordinal below the exact product has an admitted tuple. Together
/// with rank_injective this establishes a bijection, including zero dimensions.
pub(crate) proof fn rank_surjective(ds: Seq<Domain>, n: int, k: int)
    requires valid(ds, ds.len() as int), 0 <= n <= ds.len(), 0 <= k < product(ds, n),
    ensures exists|xs: Seq<i64>| tuple(ds, xs) && rank(ds, xs, n) == k,
    decreases n,
{
    if n == 0 {
        let xs = Seq::new(ds.len(), |i: int| lo(ds[i]) as i64);
        assert(tuple(ds, xs));
        assert(rank(ds, xs, n) == k);
    } else {
        let w = width(ds[n - 1]);
        let p = product(ds, n - 1);
        product_positive(ds, n - 1);
        assert(w > 0);
        let q = k / w;
        let r = k % w;
        vstd::arithmetic::div_mod::lemma_fundamental_div_mod(k, w);
        vstd::arithmetic::div_mod::lemma_mod_bound(k, w);
        assert(0 <= r < w && k == q * w + r);
        assert(0 <= q < p) by(nonlinear_arith)
            requires 0 <= k < p * w, k == q * w + r, 0 <= r < w;
        rank_surjective(ds, n - 1, q);
        let prior = choose|xs: Seq<i64>| tuple(ds, xs) && rank(ds, xs, n - 1) == q;
        let digit = lo(ds[n - 1]) + r;
        assert(lo(ds[n - 1]) <= digit <= hi(ds[n - 1]));
        let xs = prior.update(n - 1, digit as i64);
        assert(tuple(ds, xs));
        same_prefix_rank(ds, prior, xs, n - 1);
        assert(rank(ds, xs, n) == k);
    }
}

/// Unique tuple of rank k. Each scan call establishes a witness before use;
/// this is ghost choice, never a runtime search or evaluator callback.
pub(crate) open spec fn at(ds: Seq<Domain>, k: int) -> Seq<i64> {
    choose|xs: Seq<i64>| tuple(ds, xs) && rank(ds, xs, ds.len() as int) == k
}
pub(crate) proof fn at_rank(ds: Seq<Domain>, xs: Seq<i64>)
    requires tuple(ds, xs),
    ensures at(ds, rank(ds, xs, ds.len() as int)) == xs,
{
    let k = rank(ds, xs, ds.len() as int);
    assert(exists|ys: Seq<i64>| tuple(ds, ys) && rank(ds, ys, ds.len() as int) == k);
    let ys = at(ds, k);
    rank_injective(ds, xs, ys, ds.len() as int);
    assert(xs =~= ys);
}

pub(crate) open spec fn limits() -> Seq<u64> {
    Seq::new(8, |i: int| if i == 7 { FULL_BUDGET } else { 0u64 })
}
pub(crate) open spec fn observation(p: &V2ScalarProgram, xs: Seq<i64>) -> (Result<Seq<i64>, V2ExecutionFailure>, u64) {
    let run = crate::checker_execution(p.inputs@, p.outputs@, p.nodes@, p.roots@,
        xs, limits(), Seq::new(8, |_: int| 0u64));
    (run.0, run.1[7])
}
pub(crate) open spec fn result_view(result: Result<Vec<i64>, V2ExecutionFailure>) -> Result<Seq<i64>, V2ExecutionFailure> {
    match result { Ok(values) => Ok(values@), Err(error) => Err(error) }
}
pub(crate) open spec fn observation_view(o: Observation) -> (Result<Seq<i64>, V2ExecutionFailure>, u64) {
    (result_view(o.result), o.steps)
}
pub(crate) open spec fn equal_prefix(a: &V2ScalarProgram, b: &V2ScalarProgram, n: int) -> bool {
    forall|k: int| 0 <= k < n ==> observation(a, at(a.inputs@, k)).0 == observation(b, at(a.inputs@, k)).0
}
pub(crate) open spec fn all_equal(a: &V2ScalarProgram, b: &V2ScalarProgram) -> bool {
    forall|xs: Seq<i64>| tuple(a.inputs@, xs) ==> observation(a, xs).0 == observation(b, xs).0
}
pub(crate) proof fn prefix_covers(a: &V2ScalarProgram, b: &V2ScalarProgram)
    requires equal_prefix(a, b, product(a.inputs@, a.inputs@.len() as int)),
    ensures all_equal(a, b),
{
    assert forall|xs: Seq<i64>| tuple(a.inputs@, xs) implies observation(a, xs).0 == observation(b, xs).0 by {
        rank_bounds(a.inputs@, xs, a.inputs@.len() as int);
        at_rank(a.inputs@, xs);
    }
}

pub(crate) open spec fn maximum(p: &V2ScalarProgram, n: int) -> u64
    decreases n,
{
    if n <= 0 { 0 } else {
        let prior = maximum(p, n - 1);
        let step = observation(p, at(p.inputs@, n - 1)).1;
        if step > prior { step } else { prior }
    }
}
pub(crate) open spec fn over(p: &V2ScalarProgram, limit: u64, n: int) -> int
    decreases n,
{
    if n <= 0 { 0 } else { over(p, limit, n - 1) + if observation(p, at(p.inputs@, n - 1)).1 > limit { 1int } else { 0int } }
}
pub(crate) open spec fn more(a: &V2ScalarProgram, b: &V2ScalarProgram, n: int) -> int
    decreases n,
{
    if n <= 0 { 0 } else { more(a, b, n - 1) + if observation(b, at(a.inputs@, n - 1)).1 > observation(a, at(a.inputs@, n - 1)).1 { 1int } else { 0int } }
}
pub(crate) open spec fn same_usage(a: &V2ScalarProgram, b: &V2ScalarProgram, n: int) -> bool {
    forall|k: int| 0 <= k < n ==> observation(a, at(a.inputs@, k)).1 == observation(b, at(a.inputs@, k)).1
}
pub(super) open spec fn tally(t: Tally, a: &V2ScalarProgram, b: &V2ScalarProgram, limit: u64, n: int) -> bool {
    t.max_steps[0] == maximum(a, n) && t.max_steps[1] == maximum(b, n)
    && t.over_limit[0] == over(a, limit, n) && t.over_limit[1] == over(b, limit, n)
    && t.candidate_uses_more == more(a, b, n) && t.usage_differs == !same_usage(a, b, n)
}
pub(crate) open spec fn usage(u: Usage, a: &V2ScalarProgram, b: &V2ScalarProgram, n: int) -> bool {
    u.max_steps[0] == maximum(a, n) && u.max_steps[1] == maximum(b, n)
    && u.candidate_uses_more == more(a, b, n) && u.usage_preserved == same_usage(a, b, n)
}
pub(crate) open spec fn admitted(a: &V2ScalarProgram, b: &V2ScalarProgram, cap: u64) -> bool {
    a.inputs@ == b.inputs@ && a.outputs@ == b.outputs@
    && valid(a.inputs@, a.inputs@.len() as int) && product(a.inputs@, a.inputs@.len() as int) <= cap
}
pub(crate) open spec fn admission_result(a: &V2ScalarProgram, b: &V2ScalarProgram, cap: u64, result: Result<u64, Failure>) -> bool {
    match result {
        Ok(n) => admitted(a, b, cap) && n == product(a.inputs@, a.inputs@.len() as int),
        Err(Failure::InputAbi) => a.inputs@ != b.inputs@,
        Err(Failure::OutputAbi) => a.inputs@ == b.inputs@ && a.outputs@ != b.outputs@,
        Err(Failure::EmptyInputDomain { position }) => a.inputs@ == b.inputs@ && a.outputs@ == b.outputs@
            && size_result(a.inputs@, Err(position)),
        Err(Failure::DomainTooLarge { size, limit }) => a.inputs@ == b.inputs@ && a.outputs@ == b.outputs@
            && size_result(a.inputs@, Ok(size)) && limit == cap && product(a.inputs@, a.inputs@.len() as int) > cap,
        _ => false,
    }
}
pub(crate) open spec fn witness(a: &V2ScalarProgram, b: &V2ScalarProgram, error: Failure) -> bool {
    match error {
        Failure::Counterexample { ordinal, input, original, candidate } =>
            tuple(a.inputs@, input@) && rank(a.inputs@, input@, a.inputs@.len() as int) == ordinal
            && equal_prefix(a, b, ordinal as int)
            && observation_view(original) == observation(a, input@)
            && observation_view(candidate) == observation(b, input@)
            && observation_view(original).0 != observation_view(candidate).0,
        _ => false,
    }
}
}

verus! {
pub(crate) open spec fn increment(n: u64, yes: bool) -> int {
    if yes && n < u64::MAX { n + 1 } else { n as int }
}
pub(super) open spec fn recorded(old: Tally, new: Tally, a: u64, b: u64, limit: u64) -> bool {
    new.max_steps[0] == (if a > old.max_steps[0] { a } else { old.max_steps[0] })
    && new.max_steps[1] == (if b > old.max_steps[1] { b } else { old.max_steps[1] })
    && new.over_limit[0] == increment(old.over_limit[0], a > limit)
    && new.over_limit[1] == increment(old.over_limit[1], b > limit)
    && new.candidate_uses_more == increment(old.candidate_uses_more, b > a)
    && new.usage_differs == (old.usage_differs || b != a)
}
pub(super) proof fn tally_extension(old: Tally, new: Tally, a: &V2ScalarProgram, b: &V2ScalarProgram,
    limit: u64, n: int, left: u64, right: u64)
    requires a.inputs@ == b.inputs@, 0 <= n < u64::MAX,
        tally(old, a, b, limit, n), recorded(old, new, left, right, limit),
        old.over_limit[0] <= n, old.over_limit[1] <= n, old.candidate_uses_more <= n,
        left == observation(a, at(a.inputs@, n)).1,
        right == observation(b, at(a.inputs@, n)).1,
    ensures tally(new, a, b, limit, n + 1),
{
    assert(same_usage(a, b, n + 1) == (same_usage(a, b, n) && left == right)) by {
        if same_usage(a, b, n) && left == right {
            assert forall|k: int| 0 <= k < n + 1 implies
                observation(a, at(a.inputs@, k)).1 == observation(b, at(a.inputs@, k)).1 by {}
        }
        if same_usage(a, b, n + 1) {
            assert forall|k: int| 0 <= k < n implies
                observation(a, at(a.inputs@, k)).1 == observation(b, at(a.inputs@, k)).1 by {}
        }
    }
}
pub(crate) open spec fn comparison_result(a: &V2ScalarProgram, b: &V2ScalarProgram,
    cap: u64, limit: u64, result: Result<Complete, Failure>) -> bool {
    let n = product(a.inputs@, a.inputs@.len() as int);
    match result {
        Ok(done) => admitted(a, b, cap) && all_equal(a, b) && done.view().0 == n
            && usage(done.view().1, a, b, n) && over(a, limit, n) == 0 && over(b, limit, n) == 0,
        Err(Failure::BudgetBoundary { over_limit, minimum_limit, usage: used }) =>
            admitted(a, b, cap) && all_equal(a, b) && usage(used, a, b, n)
            && over_limit[0] == over(a, limit, n) && over_limit[1] == over(b, limit, n)
            && (over_limit[0] > 0 || over_limit[1] > 0)
            && minimum_limit == (if maximum(a, n) > maximum(b, n) { maximum(a, n) } else { maximum(b, n) }),
        Err(error) => (admitted(a, b, cap) && witness(a, b, error))
            || admission_result(a, b, cap, Err(error)),
    }
}
pub(crate) open spec fn equality_result(a: &V2ScalarProgram, b: &V2ScalarProgram,
    cap: u64, result: Result<Equal, Failure>) -> bool {
    match result {
        Ok(done) => admitted(a, b, cap) && all_equal(a, b)
            && done.view().0 == product(a.inputs@, a.inputs@.len() as int)
            && done.view().1 == !(a.nodes@ == b.nodes@ && a.roots@ == b.roots@),
        Err(error) => (admitted(a, b, cap) && witness(a, b, error))
            || admission_result(a, b, cap, Err(error)),
    }
}
}

verus! {
pub(crate) proof fn at_valid(ds: Seq<Domain>, k: int)
    requires valid(ds, ds.len() as int), 0 <= k < product(ds, ds.len() as int),
    ensures tuple(ds, at(ds, k)), rank(ds, at(ds, k), ds.len() as int) == k,
{
    rank_surjective(ds, ds.len() as int, k);
}

pub(crate) proof fn prefix_complete_iff(a: &V2ScalarProgram, b: &V2ScalarProgram)
    requires valid(a.inputs@, a.inputs@.len() as int),
    ensures equal_prefix(a, b, product(a.inputs@, a.inputs@.len() as int)) == all_equal(a, b),
{
    if equal_prefix(a, b, product(a.inputs@, a.inputs@.len() as int)) { prefix_covers(a, b); }
    if all_equal(a, b) {
        assert forall|k: int| 0 <= k < product(a.inputs@, a.inputs@.len() as int) implies
            observation(a, at(a.inputs@, k)).0 == observation(b, at(a.inputs@, k)).0 by {
            at_valid(a.inputs@, k);
        }
    }
}

/// Exact over-limit count is zero precisely when no tuple in the prefix binds.
pub(crate) proof fn over_zero_iff(p: &V2ScalarProgram, limit: u64, n: int)
    requires n >= 0,
    ensures 0 <= over(p, limit, n) <= n,
        (over(p, limit, n) == 0) == (forall|k: int| 0 <= k < n ==> observation(p, at(p.inputs@, k)).1 <= limit),
    decreases n,
{
    if n > 0 {
        over_zero_iff(p, limit, n - 1);
        if over(p, limit, n) == 0 {
            assert forall|k: int| 0 <= k < n implies observation(p, at(p.inputs@, k)).1 <= limit by {}
        }
        if forall|k: int| 0 <= k < n ==> observation(p, at(p.inputs@, k)).1 <= limit {
            assert forall|k: int| 0 <= k < n - 1 implies observation(p, at(p.inputs@, k)).1 <= limit by {}
        }
    }
}

/// Maximum is attained on a nonempty prefix, and bounds every tuple's usage.
pub(crate) proof fn maximum_exact(p: &V2ScalarProgram, n: int)
    requires n >= 0,
    ensures forall|k: int| 0 <= k < n ==> observation(p, at(p.inputs@, k)).1 <= maximum(p, n),
        n > 0 ==> exists|k: int| 0 <= k < n && observation(p, at(p.inputs@, k)).1 == maximum(p, n),
    decreases n,
{
    if n > 0 {
        maximum_exact(p, n - 1);
        let latest = observation(p, at(p.inputs@, n - 1)).1;
        assert forall|k: int| 0 <= k < n implies observation(p, at(p.inputs@, k)).1 <= maximum(p, n) by {}
        if latest >= maximum(p, n - 1) {
            assert(observation(p, at(p.inputs@, n - 1)).1 == maximum(p, n));
        }
    }
}
}
