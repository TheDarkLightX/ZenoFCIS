# Reusing stored values when selecting finite instructions

Baseline: `ceaf13b23c9bef92bfcc9b7c4bb9f4368e1dca7e`.

Finite synthesis now compares assigned values with the values already stored
in each validated hole. It finds the hole by its stable ID, then pairs its
borrowed values with the corresponding instructions. It clones only the
matching instruction. Previously, every comparison rebuilt a temporary value.

Codex and Fable 5.1 compared the existing implementation, borrowed values, a
mutable hole iterator and binary search over stored bytes. Opus 5 implemented
the borrowed-value lookup in an isolated draft. Codex independently reviewed,
tested and measured all three alternatives. Both external models were advisory
and ran without a wall-clock cutoff.

## The constructor establishes the correspondence

`Sketch::try_new` validates every instruction alternative and sorts alternatives
by their canonical value bytes. `Hole::try_new` receives those values, sorts by
the same bytes and rejects duplicate encodings. Sorting the resulting strictly
ordered sequence again preserves its order. Thus each stored value equals the
value of the instruction at the same position.

The vectors are privately owned, and construction and cloning preserve them
together. This change relies on that existing relationship in a new place.
Future changes to either ordering or ownership must preserve it. The new tests
check the correspondence and compare lookup against the previous implementation.

Holes retain node order; they are not sorted by ID. Looking up by ID avoids
another positional dependency between a cursor and choice visitation. The
assignment lookup still precedes value matching, so missing and unknown
assignments retain their error order. `Program::try_new`, validation of every
alternative, exhaustive relation checks and all budgets remain in place.

## Resource comparison

These counts cover one complete synthesis operation with the contract and
sketch already constructed. The first five cases reject every candidate; the
last selects the single assignment across 64 holes.

| Workload | Baseline calls | Borrowed values, selected | Hole iterator | Binary search |
| --- | ---: | ---: | ---: | ---: |
| One choice | 110 | 109 | 109 | 112 |
| 16 alternatives | 535 | 399 | 399 | 447 |
| 256 alternatives | 37,867 | 4,971 | 4,971 | 5,739 |
| 1,024 alternatives | 544,369 | 19,569 | 19,569 | 22,641 |
| Three holes, 1,000 assignments | 38,728 | 22,228 | 22,228 | 31,228 |
| 64 holes with one choice each | 807 | 743 | 743 | 935 |

For a rejecting search through one hole with width `w`, the removed temporary
values number `w * (w + 1) / 2`. At width 1,024, the observed reduction is exactly
524,800 allocation calls. The three-hole workload removes 16,500 calls.
The number of value comparisons remains linear in the alternatives per lookup.

All four implementations have the same measured peak live requested allocation
sizes in these cases. Removing short-lived allocations reduces allocation work
and cumulative requested bytes without reducing the measured peak. These are
sums of allocation layouts, excluding allocator overhead and prebuilt problem
data, not process RSS.

Interleaved timing runs used the same frozen executables, with 15 samples per
implementation and workload. Median full-synthesis times in milliseconds were:

| Workload | Baseline | Borrowed values | Binary search |
| --- | ---: | ---: | ---: |
| One choice | 1.437 | 1.541 | 1.553 |
| 16 alternatives | 2.071 | 2.025 | 2.223 |
| 256 alternatives | 26.531 | 15.101 | 13.172 |
| 1,024 alternatives | 263.321 | 81.950 | 47.112 |
| Three holes, 1,000 assignments | 60.835 | 54.856 | 56.422 |
| 64 holes with one choice each | 2.594 | 2.730 | 2.775 |

The selected version is about 0.10 and 0.14 ms slower in the two tiny cases in
this run. No improvement for every workload is claimed. These are local Rust
1.97.1 development builds with debug information disabled and an external driver
at optimization level 2. Full synthesis includes hashing the implementation
source. Timing distributions and initial samples are retained in the evidence.

Binary search is a viable measured alternative for wide grammars. The prototype
encodes the assigned value, searches the stored bytes and rechecks structural
equality before selecting an instruction. It passed the same behavior checks
and was faster on the two widest cases, but adds allocations, encoding and
private representation dependencies. The hole iterator saves no further
allocations and adds cursor state. Neither was adopted. The selected design
does not claim global optimality.

Lizard 1.24.0 reports six additional production lines, with 23 functions and
80 branch points unchanged. `close` remains at cyclomatic complexity 5. There
is no new helper, stored field, public interface or dependency.

## Behavior and identity checks

Four new tests passed against both implementations. They include 1,268 direct
comparisons with the previous lookup, every instruction variant, signed bounds,
reversed alternatives, nonascending hole IDs, intervening fixed instructions,
clones, 1,024 alternatives and 64 singleton holes. Malformed and incomplete
assignments retain their exact errors; extra IDs remain ignored.

An external corpus compares 155 public synthesis cases, including selected
programs, replay cases, witnesses, traces, budget errors and complete certificate
bytes. Changing `finite/mod.rs` intentionally changes the checker identity and
the certificate commitment. The driver independently reconstructs that checker
identity from the actual sources and checks it before comparing every other
certificate byte. It does not pin the previous identity to force a byte match.

Four compiled lookup mutations failed the new tests: reversed value/instruction
pairing, ignoring the hole ID, returning the first alternative and changing the
missing-assignment error. A fifth compiled mutation used the wrong checker
source and failed the independent identity check. Exact source restoration was
verified before rerunning tests, formatting, Clippy and the no-default-feature
check. All 28 synthesis tests and static assurance checks pass.

The durable-counter starter was regenerated normally. Only its manifest's
certificate field changed; the problem, program, emitted Rust and replay vectors
remain byte-identical. Certificates for the earlier checker remain bound to
that earlier source; regeneration is required for the updated checker identity.

The [measurement record](finite-choice-results.json) binds the source, probes,
model receipts and validation logs. Full local evidence is retained in
`/tmp/zenofcis-finite-choices-20260912`. The comparisons are bounded execution
evidence, not a general equivalence proof or a portable benchmark suite.

The commit additionally requires `python3 tools/atdd.py run --all` immediately
before committing; its result is recorded with the commit. Lean, dependencies
and the RC3 version remain unchanged. The [V1 release checklist](V1_RELEASE_CHECKLIST.md)
still applies.
