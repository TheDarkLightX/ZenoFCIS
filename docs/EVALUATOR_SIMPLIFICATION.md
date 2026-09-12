# Borrowed variable names and one evaluation stack

Baseline: `e70f1735cb025fdc1084b4f27258d0200e057308`.

The relational evaluator now borrows variable names from its immutable formula
and uses its existing variable stack for sums. Previously, every quantifier
iteration cloned its name, and each sum copied the enclosing stack, including
the names it owned. Those copies carried the same names and values that were
already available to the evaluator.

Astra proposed retaining the existing per-iteration push/pop structure while
borrowing names. Fable 5.1 reviewed this against the baseline and its earlier
push-once proposal, then recommended borrowing. Opus 5 implemented the isolated
production edit. Astra reviewed, integrated and independently tested it.

The change adds no type, helper, field, dependency or cache. Two private
signatures tie borrowed names to the formula's lifetime. Lookup still compares
name contents, searching from the innermost binding outward. The loop structure
and all range checks, checked arithmetic, eager operand evaluation, predicate
calls, early quantifier exits and fuel charges retain their order.

There is a specific tradeoff: scalar evaluation previously received an
immutable slice, which prevented it from disturbing enclosing bindings. It now
receives the shared mutable stack. Each sum must restore that stack on success
and error. It evaluates the body into a `Result`, pops its binding, and only
then propagates the error or adds the value. Quantifiers already follow this
pattern. Nested evaluation therefore sees the same ordered names and values
as the former copied stack, provided this restoration invariant holds.

The tests cover that invariant directly, including errors inside nested sums.
Hoisting a binding outside each loop would require additional exit handling
without removing another allocation from the selected design, so it was not
adopted.

## Measured allocation reduction

The same external allocator probe measured one public evaluation per case:

| Case | Allocation/reallocation calls before | After | Requested bytes before | After |
| --- | ---: | ---: | ---: | ---: |
| A 1,000-iteration universal quantifier | 1,004 | 4 | 1,320 | 320 |
| A 100-iteration quantifier containing nested 50- and 5-iteration sums | 50,404 | 4 | 1,016,520 | 320 |
| Scalar equality | 3 | 3 | 192 | 192 |

All three evaluations returned true. Input construction was outside the
counted interval; formula traversal and evaluation were inside it. The probe
used Rust 1.97.1, the library's development profile with debug information
disabled, and an external driver compiled at optimization level 2. Requested
bytes are cumulative allocator requests, not peak memory. These measurements
establish allocation reductions for these cases, not application speedups.
The external probe's counting allocator is not library code.

## Behavior evidence

The retained baseline and candidate executables produced identical complete
output for 587,840 observations: 374,000 relational evaluations and 213,840
temporal evaluations. The deterministic corpus contains 4,250 relational
formulas and varies observations, ranges, nesting, arithmetic, comparisons,
Boolean operators and limits. Temporal cases also vary operators, traces,
horizons and claim modes. Each observation includes the exact result or first
error and the ordered predicate names and arguments seen by the provider.

Seven regression tests were run against both implementations. They cover
shadowing, restoration after sums and quantifiers, every quantifier exit,
empty and extreme ranges, error cleanup, operand order and exact fuel counts.
They live under the crate's test directory and are included only for unit
tests, allowing inspection of the private stack without adding a public API.
The crate passed all 36 tests, including its existing acceptance tests.

Four deliberately broken variants compiled but failed these tests: searching
outer bindings first, skipping sum iteration charges, propagating a sum body
error before popping, and short-circuiting relational `And`. Exact candidate
source restoration was checked before rerunning the tests. The private stack
checks matter: an error-path leak can be invisible through the public API,
which discards the stack after the error.

Lizard 1.24.0 reports unchanged function and branch counts: 31 functions and
103 branch points in production `logic.rs`. Non-comment lines decrease from
539 to 538. The benefit is removing repeated ownership and allocation;
cyclomatic complexity is unchanged.

The [measurement record](evaluator-simplification-results.json) identifies
source hashes, model receipts, output hashes and validation logs. Local probe
source, replay scripts, executables and full logs are retained in
`/tmp/zenofcis-evaluator-20260912`. This is bounded comparison evidence, not a
general equivalence proof or a portable benchmark suite.

The final commit requires `python3 tools/atdd.py run --all` with the pinned
Rust and Node environments. Its result belongs to the commit's validation
record. Lean, dependency versions, public interfaces and canonical formats
remain unchanged. This change does not promote RC3 to stable V1 or replace
the [release checklist](V1_RELEASE_CHECKLIST.md).
