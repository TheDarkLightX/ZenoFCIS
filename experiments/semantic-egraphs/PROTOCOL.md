# Finite Boolean semantic e-graph experiment

## Question and scope

Can complete finite Boolean semantics improve extracted, shared ZenoFCIS
programs beyond a strong local simplifier and a rewrite-only e-graph?
This is an offline experiment on total Boolean regions. It does not change
ZenoFCIS production execution or establish equivalence of arbitrary arithmetic,
state transitions, resource accounting, or Tau Lang programs.

## Fixed corpus and comparisons

The generator uses seed 20261003 and produces exactly 100 cases: 8 small gates,
18 distributed factoring cases, 18 absorption/complement cases, 18 mixed
conditional/De Morgan cases, 18 shared-guard cases, and 20 general balanced
cases. Each case has 3--6 Boolean inputs and one or more ordered outputs.
All 2^n admitted valuations are checked, in ascending mask order; mask bit i
supplies input vi. The generated corpus and its SHA-256 digest are retained.
Corpus generation is completed before evaluation of optimizer results.

The comparisons are raw syntax, a deterministic local simplifier, rewrite-only
egg, and egg with exact truth-signature merging. Local simplification includes
constant propagation, idempotence, complement, absorption, double negation,
associative normalization, De Morgan rules, conditional identities, and common
factoring. It accepts decreasing (shared lowered node count, expression length,
lexicographic spelling) tuples. Every comparison shares identical subexpressions
when lowering all output roots, so structural sharing is not exclusive to egg.
The two egg variants use the same rewrite rules and extraction policy.

egg records equivalent alternatives and extracts a shared DAG with its integer
programming extractor. The node-cost surrogate is 1 for inputs, constants, AND,
NOT, and conditional nodes, and 4 for OR. OR lowers to NOT(AND(NOT(a),NOT(b))).
This surrogate can differ from the final shared node count when generated NOT
nodes are shared. Final emitted ZenoFCIS node count is the primary measurement;
global optimality of that count is not asserted.

## Verification and acceptance

An independent Python evaluator checks the corpus and local baseline. The Rust
harness checks every emitted program against direct expression evaluation and
the corpus signatures through the actual ZenoFCIS Program evaluator. Input and
output order are significant. Negative controls must reject changed truth
functions and removal of eager arithmetic errors from unused nodes or
conditional branches. Shape/type/domain admission checks are recorded.

Successful outputs alone are insufficient for general program equivalence:
the observation is either the entire ordered output tuple or a typed error.
The optimizer is restricted to total, well-typed Boolean expressions, and does
not propose rewrites across potentially trapping arithmetic.

An optional deployment-policy simulation retains the local baseline unless a
proposal passes equivalence and strictly decreases actual emitted node count.
Raw proposal regressions are reported separately; fallback cannot conceal them.
Optimization or verification failures, solver timeouts, and indeterminate
results are retained and count as failures, not successes.

## Success criterion fixed before the benchmark

The experiment succeeds only if all correctness and negative-control checks
pass, the raw semantic-egg proposals have a lower aggregate final node count
than the local baseline, and at least 10 of the 100 cases improve strictly
across at least two corpus families. The rewrite-only comparison quantifies
whether signature merging adds benefit under the same search policy.
This criterion supports a scoped empirical paper draft; it does not imply
novelty, superiority to mature Boolean synthesis tools, or production readiness.

## Resource limits and reproducibility

Initial search budget: 6 equality-saturation iterations, 5,000 e-nodes, and a
2-second CBC extraction limit per optimizer variant and case. Named smoke
examples may calibrate these limits before the full corpus run. Any change is
recorded before the main run. The node threshold is checked between rewrite
batches, not during each insertion, so a batch may overshoot it. The largest
calibration graph contained 10,823 nodes; all six calibration LP solves reported
optimal within two seconds. The initial budgets are retained. A 600-second
emergency saturation watchdog also fails the run if reached. Bounded search
need not reach saturation.
The full run retains stop reasons, failures, raw and accepted candidates,
per-case counts, verification totals, and optimization timings. Timing is
descriptive, host-specific, and includes no inference of runtime speedup from
node count alone. The local baseline is precomputed in Python during corpus
generation: Rust lowering time for that variant is not baseline optimization
time, and cannot support a fair optimizer-speed ratio. Repeated runs check
reproducibility of candidate outputs.

Pinned dependencies: ZenoFCIS cb1366cddc5c813f9977b377f541e64bcf94035c;
egg 12997cd15a123156abd726155363701f13358040; Rust 1.97.1; CBC 2.10.13.
The Lean model, if replayed, is an additional mathematical check of finite
signatures and the acceptance gate. It does not prove the Rust implementation,
the lowering refinement, or CBC's implementation correct.
