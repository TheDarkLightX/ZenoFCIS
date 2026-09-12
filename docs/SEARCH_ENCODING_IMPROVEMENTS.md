# Reusing canonical values during synthesis search

Baseline: `be2f246f87a34444819311b9215d1154ffe85bac`.

Generic synthesis search hashes assignments using the canonical candidate
bytes already owned by each validated hole. One private helper creates a local
encoding buffer and releases it before the external checker runs. Trace
construction also reserves its exact required capacity. The assignment
constructor, public assignment encoder and commitment method remain unchanged.

Codex and Fable 5.1 reviewed the design choices. Opus 5 implemented isolated
drafts. Codex independently reviewed the source, compared alternatives against
the baseline and measured allocations and buffer lifetimes. Both models were
advisory and ran without a wall-clock cutoff.

## Why the stored bytes can be reused

`CandidateValue::try_new` derives canonical bytes from its owned value.
`Hole::try_new` uses those bytes to sort and reject duplicate candidates.
`SynthesisProblem::try_new` includes each complete length-prefixed blob in the
problem commitment. The fields are private and immutable after construction;
callers cannot supply or mutate a value independently of its encoding.

The helper uses that existing invariant. It preserves the public assignment
format: entry count, followed by each hole ID and length-prefixed value encoding.
It uses the existing domain-framed hash function and fallible length/blob
primitives. Taking the count from the same zipped iterator as the constructor
also preserves its behavior for shorter or longer private index slices.

This adds a maintenance obligation: future assignment-format changes must keep
the helper and public encoder consistent. A direct test compares their hashes
for unequal value sizes and partial index slices. Public tests independently
reconstruct complete assignment bytes, search traces and certificate bytes.

Hashing still precedes the checker call. Checker identity, both acceptance
claims, complete-search budgets, stopping behavior and all retained
counterexamples keep their existing checks. No certificate or algorithm
identity was refreshed: complete output bytes agree with the baseline.

## Comparing allocation work and memory lifetime

All four alternatives produced identical output on the same corpus. An external
allocator probe measured these allocation/reallocation call counts:

| Change | 1,000 scalar assignments | 1,000 nested-value assignments | One scalar assignment |
| --- | ---: | ---: | ---: |
| Baseline | 16,011 | 85,011 | 13 |
| Exact trace capacity only | 14,011 | 83,011 | 11 |
| Trace capacity plus buffer reuse through the public encoder | 9,016 | 78,016 | 11 |
| Trace capacity plus stored bytes and a reusable buffer | 3,016 | 57,016 | 9 |
| Selected: trace capacity plus stored bytes and a local buffer | 8,011 | 62,011 | 9 |

Keeping a buffer between assignments removes more allocation calls, but extends
its lifetime into the external checker. With one 2 MiB byte-value assignment and
a checker allocating an 8 MiB temporary workspace, the measured peak changed:

| Implementation | Peak live requested bytes |
| --- | ---: |
| Baseline | 10,485,816 |
| Either reusable-buffer alternative | 12,582,985 |
| Selected local-buffer implementation | 10,485,816 |

The selected implementation removes repeated recursive value encoding and has
no buffer to reset or retain across calls. It adds one private helper, with no
new type, stored field, dependency or public interface. It accepts more
allocation calls than the reusable-buffer version to retain the baseline
buffer lifetime. A fused assignment constructor would introduce a fallible,
multi-output contract and was not adopted.

In the other measured cases, live requested bytes at checker entry also match
the baseline. For the nested-value workload, peak live requested bytes fall
from 73,612 to 72,804. These are sums of live allocation layouts, excluding
allocator overhead and already constructed problem data. They are not RSS or
a claim that every possible workload has the same peak.

The rejecting-search allocation counts exclude problem construction and include
retained counterexamples. Measurements use Rust 1.97.1, library development
builds with debug information disabled, and an external driver at optimization
level 2. The counting allocator is outside the library. No application timing
or asymptotic complexity improvement is claimed.

Lizard 1.24.0 reports 33 to 34 production functions, 48 to 51 branch points,
and 518 to 529 non-comment lines, excluding the test module. Search's cyclomatic
complexity stays at 11. This is a measured allocation improvement with one
additional format obligation. The pass also removes two repeated, identical
`std` feature guards; one guard expresses the same condition.

## Behavior checks

The retained baseline and all four alternatives agree byte-for-byte on
3,325 search/construction cases and 13,975 checker calls. The corpus compares
complete assignments, compiled values, certificates, hashes, checker visitation
and exact errors. It varies closed value types, declaration order, hole widths,
stopping positions, missing evidence, invalid checker output and budget edges.
It also covers 63- and 64-hole problems and admission at the million-assignment
bound; that last case selects its first assignment rather than exhaustively
executing the million-element space.

Three new public regression tests passed on both implementations. An additional
private test checks the new hash helper against the unchanged public encoder.
All 24 synthesis-crate tests pass, along with formatting, Clippy with warnings
denied, the no-default-feature check and static assurance checks for 37 crates,
27 semantic boundaries and 16 effect rules.

Four deliberately broken variants compiled but failed the public tests:
hashing in the wrong domain, hashing the first candidate regardless of its
index, omitting a blob length, and accepting one missing claim. Exact source
restoration was verified before rerunning the checks. Results are recorded in
the [measurement record](search-encoding-results.json).

The record binds sources, probes, model receipts and validation logs. Full local
evidence is retained in `/tmp/zenofcis-search-20260912`. This is bounded execution
evidence, not a general equivalence proof or a portable benchmark suite.

The final commit requires `python3 tools/atdd.py run --all`; its result is
recorded with the commit. Lean, dependency versions and the RC3 version remain
unchanged. The [V1 release checklist](V1_RELEASE_CHECKLIST.md) still applies.
