# ZenoFCIS 2.7 proof-scaling integration gates

**Target:** a generated application with an independently justified global
safety property, compositional obligations over arbitrary execution histories,
checkable symbolic proof evidence, and proved correspondence for its critical
implemented transitions. **None of those end-to-end gates are complete yet.**

Keep 2.6 release qualification and the 2.7 experiments separate. All new
verification routes must preserve fail-closed `UNKNOWN` and false-claim
negative controls; no proposer, AI model, SMT solver or receipt hash gets
publication authority simply by returning success.

## Phase 0 — This isolated experiment

- [x] Define a closed model: finite typed state, complete transition tables,
  declared read/write support, exact initial states and invariant supports.
- [x] Implement independent local induction checker that does not enumerate
  unused global state and never samples an oversized obligation.
- [x] Exercise a three-account coupling, 50-variable and 200-variable
  compositions, replayed failing mutants, malformed input and tightened budgets.
- [x] Return explicit claim scope and no publication authority.
- [ ] Obtain clean, hosted Node.js test and conformance workflow result at
  the exact PR head (local executable qualification is not assumed).
- [ ] Independently review the checker's mathematical argument and mutation
  strength; retain revisions and negative evidence.

## Phase 1 — Close production composition bridge (highest priority)

- [ ] Inventory current `zeno-fcis/checked-composition/3` obligations,
  predicates, refusal behavior, domain tables and the conditional Verus
  induction theorem from #137. Freeze precise semantics and source revisions.
- [ ] Verify construction of **every actual** local/assume/couple obligation
  from parsed `.zeno` and the bound native Authority; cover skipped and
  read-only components, CommittedFailure, rejection ordering and framing.
- [ ] Machine-check or translation-validate all admitted obligation
  enumerations, including bounds and edge cases, against those semantics.
- [ ] Prove composition of checked premises with the existing Verus
  induction theorem. No equivalence may rely solely on table hashes.
- [ ] Produce *source-bound, independently replayable* native receipts.
  Turn all missing/unsupported proof cases into UNKNOWN, not `proved`.
- [ ] Retain a counterexample distinguishing a subtly wrong actual
  implementation bridge, not just a wrong abstract table.

## Phase 2 — Symbolic scaling and proof reuse

- [ ] Freeze a versioned proof-obligation IR preserving exact i128/U128
  arithmetic, eager undefinedness, reason precedence, ordered effects,
  state identities and resource-bound semantics.
- [ ] Prototype CHC/Spacer or PDR invariant proposals on pinned,
  held-out production-derived pure-core models, with an independent
  checker for the final base, step and safety obligations.
- [ ] Extend the present Carcara-backed scalar equivalence path only
  to theories for which complete certificates can actually be checked.
  Never equate two solver UNSAT answers with an independently checked proof.
- [ ] Add a dependency-indexed, semantics-bound verified-lemma library:
  changed contracts, state layouts, assumptions and checker identities
  invalidate only their affected proof nodes, without unsound reuse.
- [ ] Compare PDR/CHC proposals, existing exhaustive composition and
  manual/LLM invariant proposals under identical wall/memory/proof budgets.
- [ ] Add symbolic summary/delta collection reasoning and prove
  collection-checker/SQLite bridges to the existing Lean reduction
  (the current Lean theorem alone does not prove the production bridge).

## Phase 3 — Application assurance

- [ ] Connect a single approved atomic Start to the real stored state,
  revision, evidence, intent and receiver identities. Prove the concrete
  operations refine the abstract transition, not just that two models agree.
- [ ] Extend to crash/reopen, acknowledged/unknown sends and fencing;
  test process-kill boundaries while explicitly separating them from
  durability/power-loss proofs.
- [ ] Implement and validate installed capability confinement and
  exact identity checks at the actual credential-bearing effect owner.
- [ ] Prove the application-level safety invariant across the closed
  supported assembly profile, including parser, execution and
  checked storage/effect interfaces. No arbitrary application claims.

## Held-out benchmark requirements

- [ ] Ledger: conservation for arbitrarily many admitted keys, transactions,
  and histories, with a proved collection implementation bridge.
- [ ] Composed service: 10, 25 and 50 actual native ZenoFCIS contracts.
  Record proof dependency structure, local and coupling size, solver time,
  source-to-runtime coverage, failures and memory.
- [ ] Recoverable application: actual generation, ingress, one atomic
  publication, restart/replay, receipt validation and adversarial mutation.
- [ ] All three include intentionally false invariants, vacuous/adversarial
  specifications, incorrect generated code and incompatible component
  assumptions. Missing/wrong implementation proofs block the result.

**Recommended first engineering assignment:** Phase 1's parser/Authority →
obligation → checked composition theorem bridge. It directly unlocks
production-scale proof composition; the isolated checker is not its substitute.
