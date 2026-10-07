# 05 — Qualification, evidence and future evaluation

Parent: [design](../DESIGN.md). Applies to each enabled slice, not all future modes.

## Inputs and outputs

Input: exact candidate source/dependency manifest, named profile, implementation,
proof obligations, native/oracle/mutation tests and existing repository gates.
Output: source-bound evidence with passed/failed/unrun distinctions, or a blocked
feature release. This document does not authorize paid experiments or publication.

## Requirements

- **NSE-001:** Smaller V2 and roadmap prerequisites MUST remain first. A design
  packet MUST NOT become a new V2 completion gate. Feature review MUST obey
  ADR 0003 epistemic labels and ADR 0005 whole-application decision boundaries.
- **NSE-002:** Each enabled slice MUST pin exact source/body/dependency coverage,
  toolchain and environment. Source changes MUST invalidate dependent receipts.
  Run relevant proofs, native/oracle comparisons, meaningful mutations, ATDD
  and exact-head CI before release. Review/push/CI/merge are distinct statuses.
- **NSE-003:** Formal obligations MUST include canonical decode correspondence,
  admitted evaluator semantics/totality, enumeration bijection, successful-loop
  soundness, no incomplete-success path, immutable custody, charge-before-work,
  receipt binding/replay, loop invariants and bounded attempts. Paper sketches
  MUST NOT be labeled machine checked. Optimizer/global-optimum proofs are not
  prerequisites for equivalence of a particular fully checked artifact.
- **NSE-004:** Evidence MUST distinguish the published study's reported Rust/Lean
  execution, current Python replay of stored results, current proposed abstract
  arguments and unperformed neural/target/application qualification. Preserve
  both failed configurations and the exploratory status of the final one.
- **NSE-005:** Neural evaluation MUST be separately authorized and preregister
  held-out contracts accepted independently of the optimizer, task/repeat counts,
  seeds/provider versions, budgets, metrics, stopping and missing-data rules.
  The old 100-case Boolean corpus MUST be calibration-only for that evaluation.
- **NSE-006:** Compare six arms: local-only, fixed rewrite e-graph, semantic
  e-graph, model-only whole proposals with checker, hybrid without feedback,
  hybrid with feedback. Use identical contracts, checker/cost objectives and
  total per-task resource ceilings. An arm need not waste unused resources;
  report actual spend. Optional whole-vs-strategy ablation is a later study.
- **NSE-007:** Count every attempted task/run, including no improvement, refusal,
  timeout and invalid output. Report actual checked node/byte deltas, improvement
  frequency, calls/tokens/money, total time, time to first complete equivalence
  pass and first improvement with censoring, failure classes, human review and
  attempted specification weakening. Counterexamples are not successful passes.
- **NSE-008:** Use repeated nondeterministic trials and paired uncertainty over
  tasks without treating dependent retries as independent samples. Predeclare
  interval/aggregation methods and publish all repeats, never only best runs.
  Provider drift and unobservable hosted state MUST be reported. A seed alone
  MUST NOT be claimed to guarantee reproducibility.
- **NSE-009:** Qualification MUST include planted checker bugs and known semantic
  counterexamples. Report false acceptance/detection with the injected population
  and denominators. Zero observed failures MUST NOT be promoted to universal
  checker soundness. Do not run fault-injected checkers on an authority path.
- **NSE-010:** No neural benefit, target preservation, complete application
  correctness, natural-language adequacy, global minimality or production
  leadership may be claimed without corresponding evidence. A negative or null
  neural result MUST remain publishable evidence; do not enlarge budgets after
  seeing failures unless a new explicitly exploratory study is labeled.

## Gates and continuation

First gate: Boolean 00/02 on supplied candidates, no real model or native solver.
Second: receipts/resume/accounting and local-loop fake-proposer tests. Third:
optional adapter qualification, including real host caps and disclosure policy,
still with fake providers. A real neural experiment is a later authorized task.
Dependency/license review and historical Rust/Lean reproduction remain separate
open tasks; neither is silently marked complete by Python receipt replay.

Record command, exit status, source digest, environment, artifact and reviewer
scope for each executed gate. A missing tool means unrun/blocked, not passed.
Keep one heavy build at a time under
`/tmp/zenofcis-v2-parallel/heavy-check.lock`; honor current repository guidance.
If a change weakens domain, observations, law, meter, error or authority checks,
stop that implementation path and surface the concrete counterexample.
