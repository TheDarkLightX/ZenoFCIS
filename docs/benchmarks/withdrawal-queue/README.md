# Withdrawal-queue optimization: native research benchmark

This public, application-derived experiment compares a smaller supplied graph
with the actual ZenoFCIS scalar evaluator. It also demonstrates why scalar
equivalence alone cannot authorize an application replacement. The production
template, generator, authority and resource policy are unchanged.

The [research brief intake](../../research/FUNCTIONAL_CORE_RESEARCH_20261004.md)
motivated this case. Its 69-node example is the **retained controller fixture**,
not today's production decision path. The current template executes the complete
descriptor in `src/v2_contract.rs`; both subjects are tested separately.

## Reproduced results

Rust 1.97.1, fresh frozen source, offline dependencies, debug and optimized native
builds. Node counts include every stored input, literal and compute instruction.
Byte counts come from `Program::value().canonical_bytes()` and round-trip through
the existing `finite_runtime::import_program`, rather than JSON length.

| Subject | Complete scalar input product | Nodes, original → reduced | Canonical bytes, original → reduced |
| --- | ---: | ---: | ---: |
| Extracted Boolean kernel | 16 | 16 → 7 | 1,063 → 763 |
| Retained controller, six outputs | 384 | 69 → 60 | 4,090 → 3,790 |
| Current decision scalar graph, nine outputs | 1,296,000 | 106 → 100 | 6,686 → 6,486 |

The kernel is extracted from the controller's actual canonical program: wire
positions 0, 1, 5 and 6, followed by its three ORs. Its ordered roots compute
`pending_a OR arriving_a`, `pending_b OR arriving_b`, and their OR. The proposer
uses the existing eager `Select(a,a,b)` instruction for each OR.

The controller is imported from the actual `synthesized/program.zcve`. Every
tuple also matches the retained emitted Rust function and a separately written
direct domain-rule oracle. The current graph is copied directly from the actual
`Contract::descriptor()`. All its scalar outputs match a separate direct rule
oracle across the full Cartesian domain, including law-invalid and unreachable
states. This is scalar coverage, not an application-publication proof.

Debug checks all kernel/controller tuples and every Step budget 0–17 / 0–70.
For the current graph debug checks its first tuple at budgets 0–107; optimized
execution checks **all 1,296,000 tuples** at generous budget 107 and the reduced
graph's boundary budget 100. The receipt keeps these coverage scopes distinct.
No speedup is claimed from verification-run durations.

## What the meter exposes

At sufficient budget, successful Step usage equals stored node count: 16 versus
7, 69 versus 60, and 106 versus 100. At the reduced count, the original refuses
before its next instruction while the reduced graph succeeds. The benchmark
checks the exact refusal resource, limit, attempted charge, overflow flag and
all eight usage counters. Lower logical work changes observable behavior.

A second candidate preserves every original instruction position. It replaces
the three private intermediate Boolean nodes with harmless Boolean literals and
the fourth with `Select`, leaving arithmetic and other nodes in place. Its
result and complete meter observation match the original at every tested budget.
Controller padding is checked on 384 × 71 = 27,264 tuple/budget pairs per build.
Current-graph padding is checked at budgets 107 and 100 on every tuple, plus
all 108 budget classes on the first tuple in debug.

Padding keeps node count unchanged and **increases** encoded size: kernel 1,114,
controller 4,141, current graph 6,720 bytes. It is a metering control, not an
accepted improvement under the planned original-node/original-byte cost guards.
No application resource-refinement or identity-migration contract is qualified.

## Negative checks and reproduction

The comparison checks exact ABI domains, complete ordered roots and bijective
row coverage. It detects a wrong OR candidate and swapped roots. Structural
admission rejects bad references, dead mistyped instructions and invalid roots.
Malformed input coverage is 11 kernel, 19 controller and 29 current-graph cases,
including length errors and both neighbors of each bound; all refuse before any
Step charge. Dead/unselected overflow remains eager, including after padding.
The proposer keeps intermediates used elsewhere and refuses a pattern whose
second operand depends on a removed intermediate. Duplicate/omitted row controls
retain the same-count coverage failure.

From the repository root, with the pinned toolchain and dependencies cached:

```sh
python3 docs/benchmarks/withdrawal-queue/run.py \
  --output /tmp/zenofcis-withdrawal-replay \
  --heavy-lock /tmp/zenofcis-v2-parallel/heavy-check.lock
```

Use a new output directory outside the checkout. The runner copies the five
local dependency crates, template and benchmark, checks their SHA-256 values,
narrows only frozen workspace membership, and uses a fresh Cargo target. It
checks registry versions/checksums against the original lockfile. It runs
formatting, builds, native comparisons and strict Clippy under the shared lock.
Failures produce no successful evidence receipt. The output includes exact
commands, logs, input manifest, original/candidate/padded canonical artifacts and
`evidence.json`. No network access or solver/model call is used.

[Recorded evidence](../../evidence/v2_1_benchmarks/withdrawal-native-20261004.json)
binds this execution; the [artifacts](artifacts/manifest.json) retain its nine
actual programs. Replay after any source change produces new evidence. These
source-bound development records do not attest the old Git HEAD's dirty tree.

## Assurance boundary

This is an offline research adapter, not the production transform command,
opaque equivalence receipt or optimizer. It grants no publication authority.
No raw-input decoder, laws, complete decisions/effects, sealed publication,
historical replay or SQLite application path is compared here. No Lean/Verus
proof, whole-template optimum, physical performance, neural loop or held-out
benchmark result was established by this run.

Only the four-input Boolean kernel fits the first planned Boolean transform
milestone. The mixed-input controller and 1,296,000-row current graph are research
subjects; they do not widen product profile or tuple limits. This adds no smaller
V2 completion gate. Follow the existing [transform design](../../V2_1_TRANSFORM_DESIGN.md)
and [roadmap](../../V2_1_FACTORY_PLAN.md) before any production adoption.
