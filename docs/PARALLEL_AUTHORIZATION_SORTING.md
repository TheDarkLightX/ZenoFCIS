# Parallel authorization sorting

The authorization path now sorts references to the caller's immutable authority
bindings. It previously copied each complete binding, including nested read,
write, context, effect and outbox paths, solely to sort them. Three production
statements change; no public type, field, helper, validation rule or dependency
is added.

Baseline: `dfdcc04c5f6d944f9c4395b6ec119ee3a5765e49`. The
[measurement record](parallel-authorization-sorting-results.json) binds the
comparison to exact source, test and executable hashes.

## Preserved contract

Both versions sort by the same concrete component identifier with a stable
sort. Cloning previously preserved every identifier and field. Borrowing the
original immutable value therefore gives each loop iteration the same binding.
Safe callers cannot mutate these bindings while shared references exist.
The local vector changes from copied bindings to references; its conceptual
role and the comparison-sort complexity remain the same.

All cardinality, duplicate, component, profile, footprint, outbox, verifier,
completeness and composition checks remain, in the same order. Hashing still
follows all footprint checks. Canonical error indexes and verifier callbacks
retain their order. The public `verify_complete_footprint` boundary still
independently checks the binding supplied to it.

Returned witnesses own claims moved from the supplied evidence. They do not
borrow the expected bindings. The authorization remains usable after the
caller's inputs are dropped. No lifetime or API change is needed.

Requiring sorted caller input would change accepted behavior. An index or cache
would add state without a need. The selected reference vector is the smallest
change among these alternatives. Allocation activity and memory addresses are
intentionally different; canonical values and authority are preserved.

## Measurements

The external probe used real SHA-256 with an exact claim-hash comparison as its
test verifier. It measures authorization and that comparison, excluding an
external theorem checker. Each populated component has 16 read paths and 16
context paths, with four atoms per path. Counts include allocation and
reallocation requests, and requested bytes rather than peak memory.

| Accepted input | Allocation calls before / after | Requested bytes before / after | Release median time before / after |
| --- | ---: | ---: | ---: |
| 1 component, empty paths | 50 / 50 | 5,710 / 5,374 | 3.86 / 3.62 us |
| 1 component, populated paths | 362 / 328 | 59,598 / 53,886 | 21.54 / 19.38 us |
| 4 components, populated paths | 1,398 / 1,262 | 224,523 / 201,675 | 82.26 / 79.54 us |
| 16 components, populated paths | 5,541 / 4,997 | 899,063 / 807,671 | 318.01 / 310.01 us |
| 64 components, populated paths | 22,105 / 19,928 | 3,650,983 / 3,263,399 | 1,380.00 / 1,323.16 us |

Six interleaved timing rounds used cached release libraries, a probe compiled
with optimization level 2, pinned Rust 1.97.1, and logical CPU 31. The earlier
unpinned development-profile run was mixed, including a 58.9% regression for
one component with empty paths. Both sets remain in the record. Allocation
counts and requested sizes agreed across both profiles. These local samples
support reduced copying, not a portable latency guarantee.

Lizard 1.24.0 reported the same 194 production functions, 1,897 function lines,
and 273 branch points. The changed function remains 86 lines with CCN 13.
The improvement removes copied data; it does not hide control flow or lower
complexity by moving decisions into helpers.

## Validation and review

Both development and release executables matched all 2,376 observations:
complete authorization bytes, exact error details, complete hash-provider
inputs, verifier callbacks and input snapshots. The corpus includes 72
three-component permutation cases and 2,304 cases combining eight defects
with independent input reorderings. This is bounded evidence, not a theorem.

Three permanent tests reuse existing builders and protect independent ordering,
input immutability, owned returned witnesses, six first-error cases, verifier
identity rejection, canonical rejection indexes and callback stopping.
All new tests also pass against the frozen baseline production code.
Four controlled mutants fail: omitted sorting, one-based error indexes,
skipped later evidence, and omitted duplicate-authority checking.
The restored candidate passes 52 tests and one compile-fail doctest across
composition, domain and composed-program crates, Clippy, the host no-std
check, formatting and static assurance checks. Full ATDD is required
immediately before the accompanying commit.

Actual Opus 5 implemented the patch and completed a separate read-only review.
The main agent consolidated overlapping tests, verified the patch, retained
measurements, and corrected unsupported conclusions in the model's release
assessment. The attempted Fable review failed for exhausted usage credits;
it is not counted as a completed review. Neither model has release authority.

Raw probes and logs are under `/tmp/zenofcis-binding-order-20260912`.
Lean, dependency versions and finite-checker certificate identities are
unchanged. This optimization does not promote RC3 to stable V1.
