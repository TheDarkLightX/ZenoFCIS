# ZenoFCIS 2.7 — abstract-machine experiment checklist

**Status:** active research, not a production feature. Draft PR #151. Baseline: 2.7 integration `b7d6fb31258146130f0adfdde4348b88282433cf`; 2.6 comparison: `release/2.6.0` at `dbbdec4fb58cf630b72c8821449b72896dd682aa`. Companion isolation work: PR #150.

## Tasks and evidence

- [x] **A0 — Bounded affine microcode search.** Five supplied one-operation machines, nine discovered programs, 2,304 four-bit input checks per run, two negative controls; normal and optimized Python replay. This is not a checked-overflow proof or runtime speedup.
- [x] **A1 — Pin source revisions.** Above hashes are comparison identities, not release qualification.
- [ ] **A2 — Freeze a useful macro grammar and corpus.** Include canonical Select/Lt and arithmetic, exact domain, evaluation order, scratch/frame, refusal, budget, and size bounds. Separate training and sealed tests.
- [ ] **A3 — Implement candidate discovery.** Bounded, total, acyclic expansion; independent checker; report all proposals and rejects, including search cost.
- [ ] **A4 — Native canonical equivalence.** Test actual 2.6 evaluator and 2.7 candidate on full declared domain; preserve first failing operation, checked overflow, eager Select, and logical fuel.
- [ ] **A5 — Fair 2.6 baseline comparison.** Same workloads, resources and limits; report solved coverage, size, wall time, cold/amortized cost, and regressions.
- [ ] **A6 — Adversarial mutations.** Wrong tie, overflow, skipped eager branch, altered fuel, stale proof, scratch not restored; require semantic counterexamples rather than parser failures.
- [ ] **A7 — Optional proposer integration.** Only after independently checked gain; otherwise preserve a documented negative result.
- [ ] **A8 — Held-out generalization.** Pin toolchain, corpus, seed, and exact replay; counterexamples used in search are no longer held out.
- [ ] **A9 — VM/hardware decision.** Require quantified benefit and whole-program refinement before a new ISA; no universality claim from finite experiments.

## Handoff protocol

Every new algorithm assessment follows the
[five-component computational feasibility standard](RESEARCH_FEASIBILITY.md),
including the strongest qualified applicable backend, representative and held-out
workloads, complete resource accounting and retained negative results. The older
2.6 comparison identity above is historical and does not establish that it is
the strongest current baseline. Unknown components remain explicit.

Each agent claims one unchecked ID in a separate branch from the current PR head. Before coding, publish the exact grammar, source revision, admitted inputs, independent oracle, budget and falsifying mutation. Check off only after recording executable commands, observed counts, counterexamples and limitations. A proposal's self-authored interpreter is not independent evidence. Keep candidate code outside the trusted acceptance boundary.

## Validation / next step

Existing replay: `python3 examples/abstract-machine-search/check_replay.py` (internally checks normal and `-O`). The 2.6-to-2.7 native comparison remains **open**; no speedup or production equivalence is claimed. Rust toolchain 1.97.1 and locked dependencies were prepared in a separate local environment, but no new native result has yet been published.
