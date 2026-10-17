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
- Positive behavior: the user can complete a permitted action; reject-all is not acceptable.
- Negative behavior: forged client command, fake permission, stale revision and mismatched instance are refused or reconciled as specified.

**Do not** begin by building a new visual design system, CRDT engine, full reactive compiler, DOM implementation or LLM UI generator.

## 3. Independent work packages

### ZUI-00: Baseline/profile and scenario corpus (blocking prerequisite)

Deliver:
- source-pinned inventory of existing checked-program/Authority/shell/ZAL hooks;
- design decision: where a prototype can live without changing the public 2.4 API;
- candidate finite presentation profile, full domains and resource ceilings;
- reviewed positive/negative scenarios for counter/form/search;
- exact acceptance oracle definitions and explicit expected rejection reasons.

Gate: human review accepts semantic meaning; ambiguities remain explicit. Do not generate a normative policy from guesses about intent.

### ZUI-01: Pure Presentation Core

Depends on ZUI-00. Deliver:
- typed semantic tree, bounded profile/IDs, total pure `present(S,J,C)` evaluator;
- disabled reason/visibility/priority/action references;
- deterministic canonicalization or explicit decision not to canonicalize yet;
- finite oracle tests, malformed/oversize/unknown input refusal, and mutation checks;
- no ambient time, network, threads, DOM, randomness, uncontrolled interior mutability or direct application effects.

Gate: same inputs produce the same tree; admitted finite cases pass an independent oracle; negative-control mutants fail. Mark checker coverage exactly.

### ZUI-02: Command/version/synchronization adapter

Depends on ZUI-00; coordinate with ZUI-01 on view revision binding.

Deliver: a typed command/response protocol that binds request ID, idempotency identity, state scope and base revision/root; a **mounted** adapter to existing domain Authority and SQLite/outbox; explicit conflict/retry/unknown-outcome policy.

Gate: server-side CAS/idempotency and client-side stale-response refusal tested separately. Include overlapping writers, duplicate/out-of-order deliveries, adversarial mutations, multi-tab and permission changes. Never introduce a second domain commit route.

### ZUI-03: Incremental reactive evaluator

Depends on stable ZUI-01 tree; develop fixtures independently of browser work.

Deliver: bounded dependency graph, conditional dependencies, transactional invalidation, cycle policy, snapshot-bound cache keys, subscription cleanup.

Gate: differential full-recompute oracle agrees with incremental output under generated updates; atomic publication avoids mixed-revision displays; worst-case resource bounds recorded.

### ZUI-04: Browser adapter and accessibility

Depends on ZUI-01 tree and stubs/protocol from ZUI-02.

Deliver: one simple browser adapter using existing rendering technology, safe escaping, typed event translation, keyed node identity, focus/selection/announcement handling, reduced-motion compliance.

Gate: real-browser tests for counter/form/table and at least one keyboard-only and screen-reader acceptance path. Scope browser/device support; do not claim general WCAG conformance from static checks.

### ZUI-05: Temporal scheduler/property harness

Depends on ZUI-02 and fixture models; may start models in parallel.

Deliver: deterministic virtual scheduler and event generator for requests, timeouts, reconnect, retries, unmount/remount, timers and cancellation; minimized counterexample serialization; mutation tests.

Gate: detects deliberately broken stale-response/idempotency/lifecycle implementations and reproduces failures from fixed seeds. No unconditional network liveness theorem.

### ZUI-06: UX policy and human study fixtures

Depends on ZUI-00; parallelizable with ZUI-01.

Deliver: readable intent/task definitions, progressive disclosure obligations, disabled reason explanations, user overrides, risk/attention policy and representative usability tasks. Define how to observe task completion, errors, confusion and recovery without privately identifying participants.

Gate: independently reviewed semantics and observed human tasks; no arbitrary "optimal" UX score.

### ZUI-07: Neurosymbolic generation and evaluation (deferred)

Depends on ZUI-01 to ZUI-06 gates.

Deliver: optional candidate generation/search within a fixed reviewed grammar; independent policy checker, test harness, browser benchmark and archive of counterexamples. Establish a Pareto report over measured task completion, responsiveness, errors and costs.

Gate: measurable improvement on held-out scenarios **subject to hard safety/accessibility constraints**, with evidence and no universal optimum claim. Research Kernel may index source-specific scoped results; it does not substitute for the actual checker.

### ZUI-08: Independent assurance review (continuous)

Review each track for: scope drift, invented authority, application bypasses, overclaiming, incomplete scenarios, missing refusal cases, stale proof/evidence identities and integration assumptions. Reject if a reviewer cannot reproduce the claimed evidence.

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
2. Parallelize ZUI-01, ZUI-02 and ZUI-06 once shared names/semantics are accepted.
3. Start ZUI-03 against ZUI-01's stable tree; start ZUI-05 against ZUI-02's versioned action fixtures.
4. Mount ZUI-04 with the smallest working end-to-end example, then run browser/real-user checks.
5. Require ZUI-08 review and exact tests for each PR before claiming its scoped completion.
6. Only after these are demonstrated, run ZUI-07 synthesis, measurement and optimization.

**Suggested first local-agent instruction:** "Read `docs/zenoui/PLAN.md`, `docs/zenoui/AGENT_HANDOFF.md`, `AGENTS.md`, the current 2.4 checked-program/shell interfaces, and the ZAL source map. Produce ZUI-00 only: a source-pinned implementation map, the smallest finite presentation profile, and positive/negative counter/form/table oracle fixtures. Do not change the authoritative API or implement the browser yet. Deliver a bounded draft PR with open semantic questions and exact verification results."
