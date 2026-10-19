# ZenoUI: local-agent implementation handoff

**Status:** Proposed work packages, not a claim of working ZenoUI code.  
**Plan:** [PLAN.md](PLAN.md). **Repository baseline:** ZenoFCIS 2.4.0 is already released.  
**Handoff rule:** Do not modify `main` directly. Work in short branches, create separate reviewed PRs, and never merge an unqualified feature.

## 1. Start here

Read fully, in order:

1. [ZenoUI plan](PLAN.md), including the release gate, research limits and open human decisions.
2. [Repository agent guidance](../../AGENTS.md), including `skills/handoff-completion-gate/SKILL.md`.
3. [Architecture](../ARCHITECTURE.md), [agent boundary](../AGENT_HANDOFF_BOUNDARY.md), [release assurance](../RELEASE_ASSURANCE.md), and [V2 program API migration](../V2_PROGRAM_API_MIGRATION.md).
4. [ZAL experimental specification](../zal/SPECIFICATION.md) and [source map](../ZAL_SOURCE_MAP.md). Do not misrepresent `zal/finite-fsm/1` as a general-purpose UI language.
5. Relevant code and tests **at the exact branch head**. Never assume documentation alone proves implementation readiness.

**Definition of done for any implementation PR:** one narrowly scoped executable change, positive and negative acceptance fixtures, exact evidence and explicit nonclaims; all relevant focused tests complete and inspected; no release claim without the release qualification workflow.

The 2.4 published Authority route remains the only domain decision/publication authority. Proposal schemas in PLAN are not pre-existing Rust or TypeScript APIs.

## 2. First milestone: behavior before framework

Build the smallest vertical slice that demonstrates:

```text
typed action -> admitted authoritative decision -> conditional atomic commit
      -> exact versioned snapshot -> pure presentation -> semantic tree
      -> incremental UI update -> browser adapter
```

Required example scenarios:

- Counter: two clicks with requests completed out of order; no stale overwrite, lost update or duplicate commit.
- Form: schema-validated inputs, errors, keyboard interaction, focus/selection preservation and recovery.
- Search table: query A then B, B completes first, A later, unmount/recreate, reconnect; final presentation matches latest eligible request and snapshot.
- Positive behavior: the user can complete a permitted action; reject-all is not acceptable. Include the inventory regression tuple and per-command witnesses against real Authority.
- Factory behavior: another agent follows only the written workflow to generate the document-release UI from its contract and reviewed descriptors, using the same generator and real adapter path. This blocks M1 completion; no app-specific renderer/handler/CSS or custom ZAL-UI is needed.
- Negative behavior: forged client command, fake permission, stale revision and mismatched instance are refused or reconciled as specified.

**Do not** begin by building a new visual design system, CRDT engine, full reactive compiler, DOM implementation or LLM UI generator. M1 does require deterministic baseline synthesis and the minimal shared adapter/renderer needed for the two-app test; full concurrency and reactive-runtime qualification remains in M2/M3.

## 3. Independent work packages

Use **reasoning** for semantics, proofs, adapter boundaries and acceptance decisions. Use **step-by-step** only for a frozen workflow with exact inputs, commands, expected outputs and escalation rules. Each implementation PR uses section 5's template; the added packages below spell out that template to make dependencies and acceptance concrete.

### ZUI-00: Baseline/profile and scenario corpus (blocking prerequisite)

**Work type:** reasoning.

Deliver:

- source-pinned inventory of existing checked-program/Authority/shell/ZAL hooks;
- design decision: where a prototype can live without changing the public 2.4 API;
- candidate finite presentation profile, full domains and resource ceilings;
- reviewed positive/negative scenarios for counter/form/search and the two factory apps;
- reviewed descriptors, real-Authority tuple domain, the inventory mismatch fixture, nine-invariant oracle map and M1 second-app workflow;
- exact acceptance oracle definitions and explicit expected rejection reasons.

Gate: human review accepts semantic meaning; ambiguities remain explicit. Do not generate a normative policy from guesses about intent.

### ZUI-01: Pure Presentation Core

**Work type:** reasoning.

Depends on ZUI-00. Deliver:

- typed semantic tree, bounded profile/IDs, total pure `present(S,J,C)` evaluator;
- disabled reason/visibility/priority/action references;
- deterministic canonicalization or explicit decision not to canonicalize yet;
- finite oracle tests, malformed/oversize/unknown input refusal, and mutation checks;
- no ambient time, network, threads, DOM, randomness, uncontrolled interior mutability or direct application effects.

Gate: same inputs produce the same tree; admitted finite cases pass an independent oracle; negative-control mutants fail. Mark checker coverage exactly.

### ZUI-02: Command/version/synchronization adapter

**Work type:** reasoning.

Depends on ZUI-00; coordinate with ZUI-01 on view revision binding.

Deliver: a typed command/response protocol that binds request ID, idempotency identity, state scope and base revision/root; a **mounted** adapter to existing domain Authority and SQLite/outbox; explicit conflict/retry/unknown-outcome policy.

Gate: server-side CAS/idempotency and client-side stale-response refusal tested separately. Include overlapping writers, duplicate/out-of-order deliveries, adversarial mutations, multi-tab and permission changes. Never introduce a second domain commit route.

### ZUI-03: Incremental reactive evaluator

**Work type:** reasoning.

Depends on stable ZUI-01 tree; develop fixtures independently of browser work.

Deliver: bounded dependency graph, conditional dependencies, transactional invalidation, cycle policy, snapshot-bound cache keys, subscription cleanup.

Gate: differential full-recompute oracle agrees with incremental output under generated updates; atomic publication avoids mixed-revision displays; worst-case resource bounds recorded.

### ZUI-04: Browser adapter and accessibility

**Work type:** reasoning.

Depends on ZUI-01 tree and stubs/protocol from ZUI-02.

Deliver: one simple shared browser adapter using existing rendering technology (React with native HTML controls by default), safe escaping, typed event translation, keyed node identity, focus/selection/IME/announcement handling, reduced-motion compliance. A minimal real-core path supports M1; full counter/form/table integration follows in M3. A fake adapter cannot qualify that M1 path.

Gate: real-browser tests for counter/form/table and at least one keyboard-only and screen-reader acceptance path. Scope browser/device support; do not claim general WCAG conformance from static checks.

### ZUI-05: Temporal scheduler/property harness

**Work type:** reasoning.

Depends on ZUI-02 and fixture models; may start models in parallel.

Deliver: deterministic virtual scheduler and event generator for requests, timeouts, reconnect, retries, unmount/remount, timers and cancellation; minimized counterexample serialization; mutation tests.

Gate: detects deliberately broken stale-response/idempotency/lifecycle implementations and reproduces failures from fixed seeds. No unconditional network liveness theorem.

### ZUI-06: UX policy and human study fixtures

**Work type:** reasoning.

Depends on ZUI-00; parallelizable with ZUI-01.

Deliver: readable intent/task definitions, progressive disclosure obligations, disabled reason explanations, user overrides, risk/attention policy and representative usability tasks. Inferred hints change emphasis only; include rule IDs, “Why this changed”, Undo, “Keep this layout” and paired-run noninterference fixtures. Wrong predictions must never cost access or authority. Define how to observe task completion, errors, confusion and recovery without privately identifying participants.

Gate: independently reviewed semantics and observed human tasks; no arbitrary "optimal" UX score.

### ZUI-07: Neurosymbolic generation and evaluation (deferred)

**Work type:** reasoning.

Depends on ZUI-01 to ZUI-06 gates and the applicable final ZUI-09 to ZUI-12 evidence; preserve the independent ZUI-08 review.

Deliver: the five loops in PLAN section 13, mapped to contract review/gaps, question/teach-back, sketch/search, expression repair, transform check/replay and a proposed MAP-Elites UI adapter. Include an independent policy checker, real-adapter task harness, browser benchmark, retained counterexamples and noninterference-gated archive. Complete one policy hole and repair one injected failure. Establish a Pareto report over measured task completion, responsiveness, errors and costs against held-out tasks and equal-budget baselines.

Gate: archive eligibility requires all hard safety/accessibility/noninterference checks and exact replay before ranking or adoption. Report measured held-out improvement or an explicit no-improvement result retaining the baseline, with evidence and no universal optimum claim. Research Kernel may index source-specific scoped results; it does not substitute for the actual checker.

### ZUI-08: Independent assurance review (continuous)

**Work type:** reasoning.

Review each track for: scope drift, invented authority, application bypasses, overclaiming, incomplete scenarios, missing refusal cases, stale proof/evidence identities and integration assumptions. Reject if a reviewer cannot reproduce the claimed evidence.

### ZUI-09: Default contract-to-UI synthesis

**Work type:** reasoning. **Milestone:** M1. Depends on ZUI-00 and stable ZUI-01; coordinates with minimal ZUI-02/ZUI-04 and ZUI-11.

#### Objective and exact scope

Generate the baseline task routes, typed forms/views, explanations, status, review and readback from the admitted app contract plus reviewed descriptors, without a neural loop or custom ZAL-UI.

#### Base commit and affected interfaces

Pin source/toolchain, contract and policy bytes; retain object IDs and existing Authority APIs. Declare the proposed semantic-contract profile and migration boundary.

#### Authoritative meaning and what is only presentation

Commands, laws, reasons and actual Authority determine eligibility; the generator projects them. Labels, units, tasks and classifications require review; principal sources are adapter inputs, not editable permissions.

#### Explicit input domain / assumptions / resource limits

Declare supported types, full integer widths, state/request/context products and tree/input budgets. Missing descriptors and unsupported domains block qualification rather than invent meaning.

#### Supported scenarios, negative scenarios and expected output

Cover every command, positive/refusal outcomes, units, invalid context, missing descriptors, channel status and the exact inventory mismatch tuple. Expect no unbound input or invented success.

#### Changes to versioned/canonical behavior (or "none")

Propose `zenoui/semantic-contract/2` with explicit migration; preserve v1 and compare common-domain observations.

#### Checker/test commands and results (including exit status)

Supply exact commands for deterministic regeneration, bindings/coverage, real-Authority comparison and finite check/replay. At handoff these checks are proposed, not run.

#### Reproduction artifacts, counterexample seeds and coverage

Retain inputs, descriptor answers, tree/readback, per-command witnesses, mismatch fixtures and full finite coverage counts.

#### Evidence identities and trusted base

Bind generator, schema, application Authority, adapter and checker identities; never use copied UI guards as the acceptance reference.

#### Browser/OS/runtime matrix, if relevant

Use the shared minimal ZUI-04 adapter on both M1 apps; report the actual core and environment exercised.

#### Nonclaims / unanswered questions / blockers

No unrestricted app coverage, neural improvement, release or whole-browser proof. Unresolved units, consent or context sources block affected work.

#### Reviewer verification steps

Regenerate both admitted outputs; compare stable bytes and bindings; require every command's accepted witness and refusal case. Gate: a useful default UI with W ⇒ A and reviewed omissions, not disable-everything.

### ZUI-10: Optional ZAL-UI v1 and shared-AST readback

**Work type:** reasoning. **Milestone:** M4. Depends on ZUI-00, ZUI-01, ZUI-06 and ZUI-09; gates through ZUI-11. Not required to generate M1's defaults.

#### Objective and exact scope

Implement PLAN section 6's bounded grammar, semantics/precedence, operation effects, canonical AST, paired surfaces, source maps and explanation ladder.

#### Base commit and affected interfaces

Pin existing ZAL parser/factory interfaces and generated manifest; treat `zal/ui/1` as a separate proposed profile.

#### Authoritative meaning and what is only presentation

Rules refine presentation and cannot grant authority. Bind facts to explicit interaction or checked application predicates; hints are absent from conditions.

#### Explicit input domain / assumptions / resource limits

Freeze identifier, expression, node/rule and finite-domain ceilings. Use `zal/finite-fsm/1` only within its supported workflow subset; unknown facts and unsupported semantics refuse admission.

#### Supported scenarios, negative scenarios and expected output

Test disable-over-enable, OR reveal, conflicting prominence/focus refusal, duplicate focus coalescing, reason precedence, draft preservation, focus return, reduced motion and symbolic/English round trips. Every mandatory invariant stays included.

#### Changes to versioned/canonical behavior (or "none")

Version the new UI profile; do not widen finite-FSM or semantic-contract v1. Compare migrated behavior on the common admitted domain.

#### Checker/test commands and results (including exit status)

Provide exact parser, round-trip, conflict, migration, finite-invariant and browser-regression commands with inspected results; not run at this proposed handoff.

#### Reproduction artifacts, counterexample seeds and coverage

Store canonical AST/readbacks, source-map witnesses, defaults/masks, positive/refusal fixtures and ladder questions with identical definitions and scope.

#### Evidence identities and trusted base

Bind parser, renderer of text, lowering, checker and exact reviewed policy; round trips do not prove intended meaning or browser behavior.

#### Browser/OS/runtime matrix, if relevant

Rerun both app journeys for disclosure, focus, reasons, keyboard and reduced motion using ZUI-04's declared environment.

#### Nonclaims / unanswered questions / blockers

No arbitrary prose execution, silent profile expansion or inferred reordering. Owner review settles changed policy meaning.

#### Reviewer verification steps

Parse both surfaces to identical ASTs, reject conflicts/unknowns and reproduce invariant mutants. Gate: same reviewed meaning, all masks explicit and real-Authority boundaries intact.

### ZUI-11: Nine invariant gates and actual-Authority agreement

**Work type:** reasoning. **Milestones:** M1, then every affected phase. Depends on ZUI-00/ZUI-01/ZUI-09 and the minimal real adapter from ZUI-02; browser evidence uses ZUI-04.

#### Objective and exact scope

Implement every named invariant row in PLAN section 10, including both `complete_initial_view` and `truthful_evidence`, with finite obligations and separate browser/integration evidence.

#### Base commit and affected interfaces

Pin application, presentation, reducer, adapter, lowering and checker subjects before checking; document the exact security projection.

#### Authoritative meaning and what is only presentation

A is real Authority acceptance after admission/laws; W is pre-mask UI eligibility. Require W ⇒ A, prefer equality and enumerate reviewed omissions; dispatch independently rechecks.

#### Explicit input domain / assumptions / resource limits

Cost the full state/request/context and event domains, including unreachable/law-invalid inputs. Justify payload/identity abstractions and integer semantics; budget exhaustion or a missing bridge is inconclusive.

#### Supported scenarios, negative scenarios and expected output

For every required action demand an accepted witness and reachable journey. For each invariant retain a positive witness and a planted violation; include bypass dispatch, false success, stale consent/replies, identity reuse, hint-altered meaning, hidden focus and false evidence counts.

#### Changes to versioned/canonical behavior (or "none")

Version obligation IDs, evidence schema, projection and lowering bindings; do not relabel old evidence for changed source or domain.

#### Checker/test commands and results (including exit status)

Specify exact invocations for real-Authority tuple evaluation, differential lowering, `transform check`, `transform replay`, contract review/lemma/scenario/probe and browser traces. No execution result is claimed here.

#### Reproduction artifacts, counterexample seeds and coverage

Save deterministic first witnesses/minimized traces, expected/actual observations, finite counts, mutated artifacts, receipts and the inventory tuple. Preserve failed inputs across repairs.

#### Evidence identities and trusted base

Separate finite checks, bridge conformance, browser tests and theorems using PLAN's labels. A fake adapter or duplicate UI predicate cannot establish A.

#### Browser/OS/runtime matrix, if relevant

Exercise actual core refusal paths, review-to-bytes agreement, reorder/reconcile/remount and paired preference journeys; include keyboard, manual screen reader, focus, zoom/reflow and reduced motion.

#### Nonclaims / unanswered questions / blockers

No disable-everything pass, blanket UI verification, unconditional liveness or finite proof of lifetime identity uniqueness. Missing mandatory evidence blocks that gate.

#### Reviewer verification steps

Replay each receipt and mutant; confirm every invariant can fail, each command can succeed when allowed, and every reported scope matches the real adapter. Reject unsupported aggregate badges.

### ZUI-12: Independent second-app workflow test

**Work type:** step-by-step. **Milestone:** mandatory M1 exit. Depends on ZUI-09, ZUI-11 and minimal ZUI-02/ZUI-04; ZUI-08 reviews the evidence.

#### Objective and exact scope

A different agent receives only the written workflow and pinned input bundle, then generates and runs the proposed document-release UI through the same generator and real adapter used for inventory.

#### Base commit and affected interfaces

Pin generator/library/adapter versions and `core-apps/document-release/project.zeno` with its policy and reviewed intent/descriptors. Freeze the workflow before execution.

#### Authoritative meaning and what is only presentation

Keep Edit/Submit/Approve/Release, version refusals and release-outbox semantics in the application Authority. The UI projects them with reviewed labels, units, review and status.

#### Explicit input domain / assumptions / resource limits

Use only declared finite domains, documented tools and descriptor answers. No oral/private instructions; record new questions and return semantic choices to reasoning work.

#### Supported scenarios, negative scenarios and expected output

Run all frozen positive/refusal journeys, invalid/stale review, version conflict, commit-versus-delivery status and mandatory invariant mutants. Expect a useful second UI with no app-specific renderer/handler/CSS changes or custom ZAL-UI.

#### Changes to versioned/canonical behavior (or "none")

None beyond generated app-specific data admitted by the frozen schema; a needed generator change fails the attempt and requires a revised workflow plus clean replay.

#### Checker/test commands and results (including exit status)

Execute the exact generation, check/replay and browser commands supplied by ZUI-09/ZUI-11. Record every exit status and inconclusive result; these commands/results are not yet produced.

#### Reproduction artifacts, counterexample seeds and coverage

Retain the input bundle, workflow revision, generated artifacts, question/answer log, command transcript, finite coverage, receipts and browser traces/screenshots.

#### Evidence identities and trusted base

Record exact generator, contract, policy, adapter/core, checker and environment identities; the workflow cannot rely on the first app's copied predicates.

#### Browser/OS/runtime matrix, if relevant

Run both apps through the same pinned minimal renderer with actual cores; complete declared keyboard/screen-reader, review, refusal and status journeys. M2/M3 later broaden recovery/performance coverage.

#### Nonclaims / unanswered questions / blockers

Source inspection of document release is not qualification. A bespoke second implementation or hidden assistance fails the factory test. Do not change expected meanings or approve new baselines during execution.

#### Reviewer verification steps

Reproduce offline from the written workflow and retained inputs, inspect all questions, and verify both apps satisfy M1 gates. Gate: no hidden instructions, no bespoke code, all mandatory evidence inspected; otherwise M1 remains unfinished.

### ZUI-13: Custom component boundary (tier 3) and checker-only mode (tier 4) — reasoning work

- **Objective:** define the typed boundary for custom components (declared data bindings in, declared typed events
  out; no dispatch, enablement or consent), and the checker-only mode that maps an existing UI's events to typed
  actions for W ⇒ A and the mandatory invariants.
- **Done when:**
  - one custom visualisation renders through the boundary and its boundary checks and accessibility contract pass;
  - a planted component that tries to dispatch or enable directly is refused by a named check;
  - one existing UI is run in checker-only mode, and its agreement report separates checked from tested regions.

## 4. Suggested proposed repository structure

These are *future* directories, not claims that they exist:

```text
crates/zeno-fcis-ui-core/          # pure bounded semantic presentation
integrations/zenoui/browser/       # browser shell and demo
integrations/zenoui/protocol/      # transport adapter/protocol fixtures
tools/zenoui/                      # model/property and differential harnesses
tests/zenoui/                      # fixed acceptance and negative vectors
docs/zenoui/                       # reviewed specification and evidence
```

Choose names and paths only after reviewing crate dependency rings, toolchain, generated code conventions and the smallest possible integration. Adding a crate or dependency requires review under existing assurance and lockfile rules. No lower dependency ring imports a higher one.

## 5. Handoff template for each agent PR

Use the following headings in each PR description:

```text
## Objective and exact scope
## Base commit and affected interfaces
## Authoritative meaning and what is only presentation
## Explicit input domain / assumptions / resource limits
## Supported scenarios, negative scenarios and expected output
## Changes to versioned/canonical behavior (or "none")
## Checker/test commands and results (including exit status)
## Reproduction artifacts, counterexample seeds and coverage
## Evidence identities and trusted base
## Browser/OS/runtime matrix, if relevant
## Nonclaims / unanswered questions / blockers
## Reviewer verification steps
```

If blocked by a missing semantic choice, publish a small proposed question or example fixture for the human reviewer, not a silently chosen grammar or a fake green proof. Keep unpublished private coordination, secrets and Research Kernel workspace contents out of public commits.

## 6. Tests and publication gates

For a documentation-only PR, run the repository's documentation inventory and link/privacy gates **on the final head**, or mark the checks explicitly not run if a full checkout is unavailable. The documented commands include:

```sh
python3 tools/check_public_docs.py
```

For code changes, run relevant Rust/Python/TypeScript/DOM/ATDD tests, as applicable, and inspect exit statuses. Register new BDD/ATDD scenarios by the repository's normal closed registry rather than claiming informal example checks are acceptance evidence.

For a **release or release candidate only**, follow the complete gate in [RELEASE_ASSURANCE.md](../RELEASE_ASSURANCE.md) and run `python3 tools/atdd.py run --all` at the exact candidate revision. A green partial test run is not release qualification.

**No result recorded as "pass" without the command, exact source, environment, checker revision, scope and inspected output.** Timeout, unsupported tool, missing platform and incomplete search remain explicitly inconclusive.

## 7. Order of work and coordination

1. Read and review ZUI-00 first. Pin an initial profile and scenario oracle.
2. Parallelize reasoning work on ZUI-01, ZUI-02 and ZUI-06 once shared names/semantics are accepted. Build ZUI-09 against the stable tree and ZUI-11 against the actual Authority.
3. Start ZUI-03 against ZUI-01's stable tree; start ZUI-05 against ZUI-02's versioned action fixtures.
4. Mount minimal ZUI-02/ZUI-04 for both generated apps and run ZUI-12 from the frozen workflow before declaring M1 complete. Broaden protocol/recovery and incremental/browser checks through M2/M3; implement optional ZUI-10 and adaptive controls for M4.
5. Require ZUI-08 review and exact tests for each PR before claiming its scoped completion.
6. Only after M1–M5 gates are demonstrated, run ZUI-07's five search/evaluation loops. Deterministic ZUI-09 synthesis is already required in M1, not deferred here.

**Suggested first local-agent instruction:** "Read `docs/zenoui/PLAN.md`, `docs/zenoui/AGENT_HANDOFF.md`, `AGENTS.md`, the current 2.4 checked-program/shell interfaces, and the ZAL source map. Produce ZUI-00 only (reasoning): a source-pinned implementation map, the smallest finite presentation profile, positive/negative counter/form/table oracle fixtures, descriptor questions, the inventory regression target, and the written M1 two-app reproduction contract. Do not change the authoritative API or implement the browser yet. Deliver a bounded draft PR with open semantic questions and exact verification results."
