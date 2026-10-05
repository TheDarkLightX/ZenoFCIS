# Design and fixture validation record

Date: 2026-10-04. Scope: the authored development packet only.

## Intake

The full assignment was read through line 146. Checkout AGENTS (54 lines) and
handoff-completion-gate SKILL (22 lines) were read before writing artifacts.
Complete source reads: five named specifications (74/70/78/82/74 lines), loop
DESIGN (265), transform design (334), scalar evaluation (331), scalar admission
(224), metered execution (291), historical PROTOCOL (90) and corpus README (73).
The 504-line factory roadmap was read only at 1–105, 133–150 and 293–401,
covering prerequisites, delivery order and complete track 6. No full-read claim.
The first combined output was truncated; missing specification/evaluator chunks
were recovered in smaller calls. A first admission.rs lookup failed (exit 1);
the actual admission/mod.rs was then located, counted and completely read.
`intake-sources.json` records the live files' later hashes, not a qualified closure.
The dirty worktree's old HEAD does not pin its current modified source bodies.

Primary web evidence: Herbie 2.3 platforms and the CERES workshop abstract read.
The Herbie PDF fetch timed out in web tooling twice; shell curl failed DNS
(exit 6). Root independently verified section 6 and supplied the textbook/wider
workload details. No further literature search or implementation download occurred.

## Executed checks

| Command / check | Exit/result |
| --- | --- |
| `python3 generate_cases.py` | 0; generated 32 explicit pairs |
| `python3 check_cases.py --write-witnesses` | 0; full product observations and 11 first witnesses recorded |
| `python3 check_cases.py` on final catalog | 0; 32 pairs, 355 tuple pairs, 21 Equivalent, 11 Different |
| Required artifact existence and DESIGN line bound | 0; all five required files nonempty; DESIGN 240 lines |
| Root separately written oracle (reported to author) | 32 pairs/355 tuple pairs/witnesses confirmed; regenerate comparison confirmed |

Final catalog SHA-256:
`3af85baf6c40b1af112d7e8d26ab09f06a6b359d096f8a07a04eb21af6a64b55`.
The final alias metadata revision changes no nodes, domains, observations or witnesses.
The root checker originally found the four exact-behavior duplicate-original groups;
all are documented and conservatively grouped to avoid future split leakage.
The root owns its independent-check script/receipt and their integration status.

## Evidence levels

Designed: suite structure, fixture schema, qualifications, future evaluation and handoff.
Locally fixture-checked: admitted node shapes/types, exact decimal integer representation,
full finite-domain functional relationships, eager checked overflow and stored witnesses.
Independently fixture-checked: root's separate interpreter agrees on these cases.
Unimplemented/unrun: production canonical codec and byte cost, Rust/native correspondence,
proof/body coverage, mutation suite, receipts, meter/resource qualification, optimizer search,
model/provider calls, held-out evaluation, runtime measurement and application adoption.
No Rust/native/Lean/Verus build, solver run, paid call, source edit, Git mutation or commit.
No effectiveness, global-optimum, production-checker-soundness or application-authority claim.
Astra wrote only its assigned /tmp directory; root controls subsequent integration/review.

## Root integration and reviewed fixes

The author record above identifies the initial packet. Root integrated it under
`docs/benchmarks/` and linked it from V2.1 track 6. It added the separately written
`independent_check.py` and its replayable development report.
Astra Max performed the bounded critique and focused follow-up recorded in
[REVIEW](REVIEW.md). Root reproduced both findings and corrected the diagnostic
attribution and B02/B04 split-group metadata in the catalog and generator.
Both fixture scripts exited 0 again, and generation plus witness reconstruction
reproduced the corrected catalog byte for byte. Counts and all witnesses stayed intact.
Corrected catalog SHA-256: `65ad66a61de3bbd8d0133cdf30c451f6344a3077c5eef989f39682890c6a1f55`.
No runtime source was changed; native/proof/optimizer/hosted checks were not run.
The existing dirty checkout was preserved; no commit or push was performed.
