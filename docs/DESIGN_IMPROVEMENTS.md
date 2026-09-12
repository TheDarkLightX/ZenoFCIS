# Design and algorithm improvements

This review selected three changes that reduce repeated work while preserving
FCIS acceptance rules: shared canonical decoding, one composition identity per
verification call, and reusable finite-evaluation storage. Fable 5.1 reviewed
the source with Astra; Opus 5 implemented the selected changes. Astra reviewed
the implementation, corrected admission-before-allocation behavior, and ran
the checks and comparisons below.

Baseline: `132dc15d8333d0514e38bbed4786d127057b5770`.
The [measurement record](design-improvement-results.json) binds these results
to the changed source files and records the actual model identities.

## Selected designs

| Area | Choice | Preserved behavior |
| --- | --- | --- |
| Canonical decoding | Use one complete-value decoder; borrow encoded map-key slices; remove redundant envelope and SQLite re-encoding. | Canonical admission, nested resource budgets, exact errors and their precedence. |
| Composition | Compute the immutable specification identity once per public entry; pass it to private verification helpers; binary-search sorted guarantee claims. | Footprint admission first, independent evidence checks, ordered blockers, authorization types and commitment bytes. |
| Finite synthesis | Advance domain tuples in place and reuse input, output, environment and node buffers inside exhaustive evaluation. | Public owned results, eager arithmetic, enumeration order, exhaustive relation checking, budgets and independent candidate acceptance. |

These are local implementation choices with measured benefits. The review does
not establish a globally optimal architecture or algorithm.

### Canonical decoding

`decode_complete_value` owns the repeated framing checks. Nested map blobs use
the same depth, node and payload accounting as their parent. Sharing this
routine removes duplicated decisions without separating the checks from the
data they protect.

Encoded key bytes and the previous key borrow slices of the original input.
Decoded values still own their data. The decoder still independently checks
that a map entry's ordering bytes match its semantic key. Its accepted encoding
is already canonical, so encoding the entire accepted envelope or SQLite value
again adds no admission rule.

SQLite still verifies its complete history, roots, replay and outbox state.
This change removes only the redundant encoding inside its value decoder.

### Composition

Successful parallel verification previously computed the same specification
commitment twice; successful parallel authorization computed it three times.
Each now computes it once. Assume-guarantee verification continues to compute
it once. Authorization still rejects incomplete or mismatched footprints before
computing that identity. No identity is cached across calls or accepted from
an untrusted caller.

Component construction already sorts guarantee claims and rejects duplicates.
Provider membership therefore uses binary search, reducing a lookup over `g`
claims from `O(g)` to `O(log g)` without a second index or mutable cache.
Evidence verification calls remain independent and retain their original order.

### Finite synthesis

The public domain iterator now clones each returned tuple once instead of
twice. Internal enumeration copies into reusable storage and advances in place.
The relation checker, candidate checker and replay loop reuse evaluation
storage while retaining owned public results.

Invalid inputs are rejected before reserving node or output storage. Failed
evaluations clear partial results before the buffers can be reused. Restarting
an exhausted domain iterator still creates its current tuple; the complete
synthesis operation is not allocation-free.

Finding one satisfying output does not stop relation evaluation: a later pair
may trap and must still be reported. The first unrealizable input, trapped pair
and candidate counterexample retain their original order. Selected assignments
and replay traces remain identical in the comparison cases. The source-bound
checker and certificate identities refresh because implementation source has
changed; certificates must be regenerated for the new checker identity.
The embedded durable-counter starter's manifest was regenerated with that
identity. Its program, contract, vectors, emitted Rust and trace are unchanged.

## Measured performance and complexity

The measurements used Rust 1.97.1 release builds, an AMD Ryzen Threadripper
1950X, logical CPU 31, and seven rounds alternating baseline and candidate
execution order. The table reports median time per call. Both executables used
the same harness and dependency features.
Composition timings use real SHA-256 commitments and an accepting test
verifier; they exclude the cost of checking external proofs.

| Workload | Before | After | Change in time |
| --- | ---: | ---: | ---: |
| Decode envelope, 1 entry | 1.135 us | 0.645 us | -43.2% |
| Decode envelope, 32 entries | 21.548 us | 16.538 us | -23.2% |
| Decode envelope, 512 entries | 304.047 us | 241.855 us | -20.5% |
| Parallel verification, 1 guarantee | 2.208 us | 1.240 us | -43.8% |
| Parallel verification, 32 guarantees | 12.705 us | 7.659 us | -39.7% |
| Parallel verification, 512 guarantees | 184.009 us | 117.430 us | -36.2% |
| Synthesis, 256 input / 64 output tuples | 2348.334 us | 711.831 us | -69.7% (3.30x throughput) |
| Synthesis, 2 input / 2 output tuples | 38.976 us | 40.615 us | +4.2% |

The tiny synthesis case costs about 1.6 microseconds more. These are local
measurements, not portable performance guarantees. The unchanged path-overlap
control also varied substantially: the 4096-namespace case took 6.594 ms before
and 11.635 ms after. Disassembly shows the same 320-byte instruction sequence
apart from a relocated jump-table address. That is consistent with sensitivity
to binary layout, not evidence of a different path algorithm. The measurement
record retains every control result, including regressions.

Lizard 1.24.0 measured lexical Rust cyclomatic complexity, excluding test
functions. To distinguish fewer branches from extra helper functions, the
table counts branch points as the sum of `CCN - 1`.

| Changed production module | Branch points before / after | Maximum function CCN before / after |
| --- | ---: | ---: |
| Codec | 106 / 98 | 51 / 45 |
| Composition | 275 / 273 | 18 / 18 |
| SQLite shell | 360 / 357 | 53 / 53 |
| Finite synthesis | 76 / 80 | 29 / 29 |
| Finite program evaluation | 45 / 48 | 21 / 21 |

Across these modules there are six fewer branch points, ten more functions and
103 more non-comment lines. The raw sum of function CCN rises from 1198 to 1202
because every added function contributes a base of one. Reusable evaluation
storage adds seven branches; the measured improvement on larger searches
justifies that cost. Distinct integrity checks and explicit operation dispatch
remain intact even where their function CCN is high.

## Validation

Independent baseline and candidate executables produced identical observations
for 405,364 decoder cases, 85 finite spaces, 60 synthesis outcomes and 3,072
composition scenarios. The composition comparison also checks all 6,144
ordered reports and evidence-verifier call sequences from its two public
verification entries. These are bounded comparisons, not an equivalence proof.

The changed crates passed 92 tests including doctests, Clippy with warnings
denied, and applicable no-default-feature library checks. Static assurance
checks passed for 37 crates, 27 semantic boundaries and 16 effect rules.

Three deliberately broken variants were rejected by the tests: resetting
nested decode budgets, skipping later relation checks after finding a satisfying
output, and retaining partial evaluation results after failure. The single-hash
composition test also failed on the baseline's repeated hashing. All controlled
mutations were restored and the affected tests passed again.

The first full acceptance run caught the starter's stale source-bound
certificate. Regenerating it changed only the manifest's certificate field;
the artifact-drift check remains intact.

The final commit additionally requires `python3 tools/atdd.py run --all` with
the pinned Rust and Node environments. That gate covers the generated
applications and cross-language synthesis checks. Its execution result belongs
to the commit's validation log, rather than being predicted by this report.

## Further candidates

The subsequent [shell simplification review](SHELL_SIMPLIFICATION.md) covers
removal of repeated checks within one SQLite transaction and unnecessary
authorization-record cloning.

The [evaluator simplification review](EVALUATOR_SIMPLIFICATION.md) covers
borrowed variable names and removal of copied sum stacks, with identical
before-and-after evaluation observations and measured allocation reductions.

The [search encoding review](SEARCH_ENCODING_IMPROVEMENTS.md) compares four
ways to reduce synthesis allocations while preserving complete certificate
bytes, checker behavior and temporary-buffer lifetimes.

The [finite choice lookup review](FINITE_CHOICE_LOOKUP.md) compares stored-value
lookup with a hole iterator and binary search. It removes temporary instruction
values while preserving programs, traces and failure order, with the expected
checker-source identity refresh recorded separately.

The [outbox encoding and SQLite review](OUTBOX_ENCODING_AND_SQLITE_CHECKS.md)
removes a temporary delivery-identifier buffer while preserving exact hash inputs.
A measured SQLite statement-cache prototype was rejected because schema changes
altered error details. The new regression checks preserve that counterexample,
full-history validation and later-row error precedence.

1. Any future SQLite batching or expected-row reuse must preserve every row
   check and exact first-error detail, including schema failures. The existing
   direct queries remain; checking only touched rows would narrow corruption
   detection and was rejected.
2. Revisit namespace grouping for path conflicts when application workloads
   justify it. A scratch prototype helped large multi-namespace sets but hurt
   tiny and single-namespace sets. A simple merge over full sorted paths is
   incorrect with wildcards, so the existing algorithm remains.

Broad interpreter dispatch extraction would mostly move decisions between
functions. Extra traits, mutable caches, unsafe code, SIMD and handwritten
assembly were unnecessary for the selected improvements.

Fable's source review was tool-disabled. Opus could read and edit its isolated
checkout but could not run tests or git commands. Both completed without a
wall-clock cutoff. Their output was advisory; the main agent owned integration
and validation. The raw local evidence directory is
`/tmp/zenofcis-design-review-20260909`; its one-off benchmark harness and full
logs are not installed library components or a portable benchmark suite.

Dependency versions and Lean remain unchanged. This pass does not promote RC3
to stable V1 or replace the [release checklist](V1_RELEASE_CHECKLIST.md).
