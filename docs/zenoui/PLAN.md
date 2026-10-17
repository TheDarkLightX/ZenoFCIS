# ZenoUI: specification-driven, intent-aware UI engineering

**Status:** Proposed architecture and implementation plan; not an implemented or released ZenoFCIS capability.  
**Prepared:** 2026-10-17. **Basis:** targeted reactive-UI research conducted 2026-10-15, followed by a ZenoFCIS-specific architecture synthesis.  
**Applies to:** work following the published ZenoFCIS **2.4.0** release; possible 2.5+ experiments, subject to separate release planning.  
**Audience:** ZenoFCIS maintainers, local coding agents, verification agents, UX researchers, and human reviewers.

> This document is an **engineering proposal and agent handoff**, not a claim that a browser UI has been formally proved, that ZenoFCIS implements ZenoUI, that user research has been conducted, or that a new release has been qualified. Every MUST below is a **proposed acceptance requirement** for the indicated future phase. Do not promote a proposal, example, simulated test, or bounded check into release evidence.

For parallel work packages and concrete handoff instructions, see [AGENT_HANDOFF.md](AGENT_HANDOFF.md). For existing product and assurance boundaries, read [ZenoFCIS architecture](../ARCHITECTURE.md), [agent handoff boundary](../AGENT_HANDOFF_BOUNDARY.md), [2.4.0 release assurance](../RELEASE_ASSURANCE.md), and the existing [ZAL research prototype](../zal/SPECIFICATION.md).

## 1. Decision summary

**Goal:** extend ZenoFCIS from specification-driven **application behavior** to specification-driven **application and presentation behavior**, without confusing either with actual browser execution.

Propose three independently testable boundaries:

1. **Application Core (authority):** a pure decision model of domain state, commands, reasons, next state, laws and closed effects, using the existing ZenoFCIS authority and publication path. UI code must never replace it.
2. **Presentation Core (policy):** a pure, deterministic, typed function mapping an authorized application snapshot, explicit interaction state, and captured context into a **Semantic UI Tree**. It controls visibility, affordances, action availability, explanatory text tokens, priority, disclosure and accessibility intentions. It has **no application commit authority**.
3. **UI Shell (mechanism):** an adapter for DOM/native rendering, input capture, focus, animations, platform accessibility, network transport, persistence and external effects. It interprets vetted decisions but is not relied upon as an authorization boundary.

Support the three cores with cross-cutting services, not additional sources of authority:

- **Versioned action and synchronization protocol** with idempotency, commit-root/revision binding and concurrency policies.
- **Transactional fine-grained reactive dependency engine** deriving UI updates from one consistent snapshot.
- **Temporal-effect and lifecycle model** describing cancellation, scheduled work, subscriptions and stale responses.
- **Independent verification and evaluation pipeline** covering domain safety, presentation consistency, browser behavior, usability, accessibility and performance.
- **Optional neurosymbolic synthesis loop** that proposes candidates without determining meaning or claiming evidence.

Do **not** implement a novel DOM engine, CRDT store, browser framework, general theorem prover, or optimizer before a small vertical slice passes its gates.

## 2. Why this fits the actual repository

As of this proposal, `main` documents **2.4.0 as released**, with a checked-program and mandatory Authority route; the 2.3 boundary remains. The current library already distinguishes `Accept | Reject | CommittedFailure`, complete contract decisions, canonical admission, checked execution, law enforcement, nominal publication and outbox delivery. Preserve those existing decisions, result classes, versioned identities and refusal semantics. See [README](../../README.md), [V2 API migration](../V2_PROGRAM_API_MIGRATION.md), and [release notes](../RELEASE_NOTES.md).

ZAL currently provides a **bounded experimental** `zal/finite-fsm/1` behavior dialogue with symbolic and controlled-English surfaces, exact-revision review and scoped checks. This is a promising **authoring input**, not a released, general UI contract language. Any UI profile, type or parser proposed below must be independently specified, implemented, reviewed and qualified; do not pretend the existing ZAL parser already supports UI semantics.

Maintain the existing FCIS trust separation:

```text
Human task and examples
  -> LLM design/specification proposal (untrusted)
  -> owner-reviewed, admitted contract meaning
  -> existing authoritative Application Core
     -> closed decision and effects, canonical roots, laws
     -> authorized transactional publication in shell
  -> authenticated/versioned application snapshot
  -> deterministic Presentation Core
  -> typed Semantic UI Tree + dependency/effect metadata
  -> incremental reactive evaluator
  -> versioned renderer instructions
  -> browser/native UI Shell
  -> typed user action returns to guarded admission
```

**Nonnegotiable:** A disabled button, hidden element, prior UI check, LLM classification, checksum, or client-side policy never authorizes an action. The existing authoritative invocation and shell commit path must independently validate every mutation.

## 3. Formal semantic model

Let:

- \(S\) = immutable authoritative application snapshot, including its subject, revision/root and authenticated observations where required.
- \(A\) = admitted typed command plus explicit evidence/context.
- \(\delta(S,A)\) = existing domain **decision** under the checked application contract. Its business result remains `Accept`, `Reject`, or `CommittedFailure`, with the exact associated state/effects/reasons.
- \(J\) = explicit, typed **interaction state**: focused task, open disclosure, draft inputs, in-flight action identities, current selection. \(J\) is nonauthoritative.
- \(C\) = **captured presentation context**: locale, motion/contrast preferences, viewport capability, device input modes, and trusted privilege/read-model projections. Explicitly capture and version all information used by a pure evaluation.
- \(P(S,J,C)\) = pure Presentation Core producing typed semantic view \(V\).
- \(G\) = dependency graph for materialized presentation values.
- \(D(V_0,V_1)\) = candidate keyed UI patch from two well-defined semantic trees.
- \(B(V)\) = browser/native adapter interpretation of that semantic tree.

Proposed equations:

\[
\text{DomainDecision}=\delta(S,A),\qquad V=P(S,J,C),\qquad \Delta V=D(V_{\mathrm{old}},V_{\mathrm{new}})
\]

`P` has **no network, wall-clock, random, filesystem, HTML-DOM, or external capability access**. If time matters (for example, quote expiry), the application supplies a trust-classified time observation or expiration status as part of a captured snapshot; the presentation function does not read the ambient clock.

Separate **domain safety** from **display policy**:

- Domain policy can refuse `ConfirmSwap` regardless of client/UI behavior.
- Presentation policy can decide that `RefreshQuote` becomes prominent when a quote expires.
- The browser shell may animate that change, but neither CSS nor animation frames can produce, authorize or reorder an application commit.

### 3.1 Typed Semantic UI Tree (proposed, not an API)

Do not use arbitrary HTML strings as the verified representation. A proposed profile identifier, e.g. `zenoui/presentation/1`, would define a closed, bounded typed grammar with stable identity and meaning. The following is *illustrative schema pseudocode*, not a type already present in the repository:

```text
SemanticView {
  contract_profile, contract_revision, snapshot_root, interaction_revision,
  context_revision, root: Node, declared_dependencies, obligations
}

Node {
  key: StableNodeId,
  role: SemanticRole,                // form, field, button, status, section, ...
  label: TextToken | StaticText,
  children: BoundedList<Node>,
  visibility: Exposed | Collapsed | Hidden,
  emphasis: Primary | Secondary | Neutral | Warning,
  enabled: Boolean,
  disabled_reason: Optional<TextToken>,
  actions: BoundedList<TypedActionRef>,
  a11y: AccessibleNameHelpLiveRegion,
  focus: FocusIdentityAndTransitionPolicy,
  data_dependency_refs: BoundedList<DependencyId>
}
```

Before promoting the grammar, specify maximum depth, nodes, text, inputs, action arity, canonical IDs/bytes, unsupported roles, refusal precedence, compatible upgrade rules, and cost budgets. Stable identity is not necessarily DOM identity: adapters must demonstrate correspondence. Names, resource limits and rules are a **proposal** pending compatibility review.

Action references are a **projection** of allowed routes, not authorization capabilities. Any perceived authorization, personalisation, permissions, or user intent inferred client-side is untrusted.

### 3.2 Purity, revisions and evidence

For a fixed admitted snapshot and explicit context:

\[
(S_1,J_1,C_1)=(S_2,J_2,C_2)\Longrightarrow
P(S_1,J_1,C_1)=P(S_2,J_2,C_2)
\]

Canonical UI meaning is revision-bound. A digest can identify bytes; **it does not prove those bytes are correct or authenticate external facts**. In a future UI-specific protocol, bind model/profile, exact source, checker identity, scope, dependencies, and any accepted owner review. Keep proof artifacts separate from application authority.

The existing ZAL review path may be explored for *meaning review*; it cannot be reused as an unqualified source of production UI authorization.

## 4. Presentation-policy best practices

The Presentation Core is ZenoUI's main addition to the original reactive-systems research. Encode supported rules for:

- **Intent-aware adaptation:** explicit task/selection takes priority over inferred intent; inferred recommendations are advisory only. A change in inferred intent cannot silently invoke, authorize or consent to a command.
- **Progressive disclosure:** show the necessary next step first; reveal advanced settings when task/state warrants it. Every required task remains discoverable by a documented navigation path, including keyboard access.
- **Contextual action priority:** make a safe recovery action primary when the intended operation becomes invalid; distinguish explanatory, reversible advice from business requirements.
- **Explainability:** expose why a control is unavailable and what permitted action can repair that condition; never conceal necessary risks or unexpected consequences.
- **Predictability and user control:** preserve location, mental model, selection and drafts when possible. Adaptive changes are explainable, reversible or explicitly user-overridable wherever appropriate.
- **Honest asynchronous status:** differentiate local input, pending submission, accepted transaction, delivered effect, failed attempt, stale data and unknown outcome. Optimistic display cannot impersonate an authoritative commit.
- **Accessibility-by-contract:** explicit role/name/state, keyboard operation, focus placement/return, live announcements, and reduced-motion semantics. Color, hover or animation alone never conveys success or proof.
- **Responsive presentation:** viewport and input capabilities affect layout and disclosure, **not** domain truth.
- **Calibrated motion:** motion communicates genuine state transitions or attention movement and can be disabled. Do not conflate visual polish with actual usability.

Prefer **task success** and **error recovery** over novelty. These are UX hypotheses and policy constraints; no symbolic checker can establish universal ease of use, taste or comprehension. Test them with representative human tasks.

### 4.1 Example: expired quote

```text
given state.quote = Expired
and interaction.intent = ConfirmSwap

Presentation:
  ConfirmSwap: disabled; reason = "quote expired"
  RefreshQuote: exposed, primary
  RiskDisclosure: exposed when relevant
  QuoteStatus: accessible status announcement
  InputDraft: preserved

Application:
  ConfirmSwap on expired quote -> Reject with specified reason.
  RefreshQuote -> typed request; no invented success until an accepted result.
```

The presentation policy can be checked against exhaustive finite projections and mutated examples. Only the authoritative core may decide or publish the business action. Provide positive and negative intent examples so a trivial **disable-everything** implementation cannot masquerade as correct.

## 5. Versioned synchronization protocol

A server-driven or distributed UI is a distributed system. The original October 15 review identified Livewire 4 island concurrency as a concrete motivating example; consult the current official [islands documentation](https://livewire.laravel.com/docs/4.x/islands) before making a public comparative claim.

Proposed **illustrative** envelopes:

```text
UICommand {
  protocol, contract_revision, component_instance, request_id,
  idempotency_key, state_scope, base_revision_or_root,
  typed_action, arguments, interaction_revision
}

UIResponse {
  protocol, contract_revision, component_instance, request_id,
  state_scope, previous_root, committed_root, outcome,
  patch_base_revision, patch, effect_status, reconciliation_hint
}
```

Required semantics:

1. Server admission revalidates authentication, authority, structure and schema of every request. Never hydrate browser-submitted fields as authoritative state without checking them.
2. A shared state scope has a serializable/atomic update policy (such as expected-root compare-and-swap) that rejects, retries safely, or explicitly reconciles conflicting writes.
3. Request identities are idempotent for effectful or retryable operations. Duplicate delivery must not produce duplicate business effects.
4. The browser accepts only a patch bound to a compatible instance, profile, base snapshot/root, revision and context. Out-of-order replies may be discarded and refreshed; they must not overwrite a newer committed view.
5. Different scopes may use different revisions **only** when dependency ownership and cross-scope snapshot consistency are explicit. A global scalar version is not an automatic solution to shared-state races.
6. A destroyed/recreated component has a fresh **instance generation**; old responses cannot update the new instance by key reuse.
7. Reject, timeout, disconnection, unknown commit outcome and reconciliation are distinguished in both protocols and UI text. Never retry a non-idempotent action merely because the display timed out.
8. Do not claim exactly-once external delivery. Use transactional outbox and recipient-side idempotency under documented assumptions, consistent with ZenoFCIS.

**Important distinction:** browser-side monotonic versions prevent some stale rendering; they do **not** prevent server-side lost updates. Both the application commit protocol and UI reconciliation must be checked.

## 6. Incremental reactive dependency engine

Propose a small deterministic, bounded dependency DAG over semantic projections. For an input \(x\) and graph \(G\), affected computations lie in the dependency-reachable set:

\[
Affected(x)\subseteq Reachable_G(x)
\]

Reachability indicates *possible* invalidation, not that every reachable computation must change output. Cache/coalescing decisions require evidence.

- Compute derived nodes against **one immutable snapshot**; publish their visible results atomically to avoid inconsistent intermediate totals ("glitches").
- Support *conditional/dynamic dependencies*: invalidating a previously subscribed branch when active branch selection changes.
- Detect/reject unintended cycles. Feedback, if supported later, needs an explicit tick/delay and a bounded convergence policy.
- Version/reclaim subscriptions on unmount; cancel work by stable generation identity.
- Avoid unnecessary re-rendering of unaffected keyed regions, while preserving focus/selection and accessibility semantics.
- Profile worst-case depth, fan-out, memory retention, dirty-node count and workload-dependent update time.
- Keep browser adapter-specific reactive machinery in the shell; it must not become a second authority on business meaning.

Start with signals/selectors and transactional invalidation. Treat reactive relational queries (as explored by **Riffle**) as a later opt-in, benchmark-driven extension, not an immediate database rewrite.

## 7. Temporal effects and component lifecycle

Model time and sequencing **explicitly** in the shell's effect protocol and verification traces. Drawing on Willow, describe for each effect its:

- read and write footprints;
- scheduling trigger / delay domain;
- cancellation and replacement key;
- expected component generation and state revision;
- response applicability predicate;
- cleanup on unmount;
- maximum outstanding work / queue budget.

Example:

```text
SearchChanged(query):
  reads: draft.query
  invalidates: results-for-old-query
  schedules: DebouncedSearch(query, generation)
  cancels: prior DebouncedSearch for this component
  applies result only if generation and query revision still match
  cleanup: cancel timers/subscriptions and ignore stale responses on unmount
```

Initial implementation: developer-visible traces, sequence-model tests, and explicit cancellation. A Willow-like **static temporal effect checker** is a research track after those practical contracts exist. Do not advertise it as implemented.

## 8. Formal obligations and verification strategy

Separate three questions:

- **Adequacy:** does the reviewed specification describe the product humans actually intend? Human examples, counterexamples, and scenario review are mandatory.
- **Conformance:** does the executable core/presentation evaluator satisfy the contract in its declared domain? Use exact replay, differential or proof checks with explicit coverage.
- **Integration:** does the mounted shell actually preserve required observable behavior under browser, server, network and lifecycle effects? Use integration, accessibility, security and performance tests.

Proposed safety properties (temporal notation is schematic; a particular checker must define the model and semantics):

| ID | Candidate obligation | Evidence required |
| --- | --- | --- |
| U-SAFE-01 | `G(not_authorized(a) -> not_committed(a))` | Authoritative core/commit tests and checked-domain proof where available; disabling UI is insufficient. |
| U-SAFE-02 | `G(visible_authoritative_revision_next >= current)` | Explicit revision-scope model plus reordered-response traces. |
| U-SAFE-03 | `G(expired_quote -> not confirm_enabled)` | Presentation-policy checker over admitted snapshots; paired with core rejection. |
| U-SAFE-04 | Never apply a patch whose base/root/profile/instance generation does not match | Protocol model, negative vectors, browser adapter tests. |
| U-SAFE-05 | A displayed derived set comes from one consistent snapshot | Transactional reactive oracle and adversarial event traces. |
| U-SAFE-06 | A destroyed component cannot receive an applied stale effect | Lifecycle generator and browser remount tests. |
| U-INTENT-01 | Every required task has an exposed, documented reachable interaction path | Finite navigation graph plus independent task review; no claim of universal discoverability. |
| U-INTENT-02 | Positive admitted actions remain possible; reject-all is not a valid specification refinement | Human-labeled acceptance cases and executable positive/negative scenarios. |
| U-A11Y-01 | Required controls have typed accessible semantics; focus and announcements follow contract | Structural checks *and* human/assistive-technology tests. |
| U-LIVE-01 | `G(accepted(a) -> F resolved(a))` only under stated scheduler/network/fairness/retry assumptions | Scoped liveness model; otherwise timeout/inconclusive and user-visible reconciliation state. |

Additional obligations: deterministic render output for identical complete inputs, total refusal on unsupported input, no unbounded recursion/queue growth, correct reason precedence, no unauthorized server method exposure, no unchecked HTML/JS injection.

Use a property-based model harness inspired by **PropRatt**: generate actions, asynchronous completions, timer firings, disconnects, retries, permission changes, unmount/remount and injected failures. Minimize failing event traces and turn every counterexample into a stable regression fixture. Pair with mutation testing so a checker that never fails is not accepted on its own.

### 8.1 Proof and evidence labels

Use explicit labels, consistent with [epistemic status](../adr/0003-epistemic-status.md):

- `proposed`: design only; no executable evidence.
- `example-tested`: hand-picked behavior on one implementation.
- `property-tested`: randomized traces, seed/count/coverage recorded.
- `bounded-exhaustive`: all modeled inputs in explicitly declared finite scope.
- `machine-checked`: specific theorem/implementation/model/assumptions with source-bound checker evidence.
- `browser-validated`: specified adapter/runtime/device combinations.
- `human-validated`: task studies identifying participants, tasks and measures (privacy-safe aggregate reporting).

Never promote bounded model checks to universal proofs. Never claim browser-wide "bug free" or mathematically optimal UX from a semantic-core proof.

## 9. Security and trust threats

Explicitly consider:

- Forged requests, IDs, revisions and browser-submitted serialized state.
- Authorization changes between display and execution; TOCTOU over quote/price, balance or permissions.
- CSRF where cookie authentication applies; XSS/HTML injection; constrained content rendering and CSP.
- Replay or duplicate delivery, unknown-outcome retries, multi-tab competition.
- Mismatched component key, generation, stale patch, locale or profile.
- Leaked timers/listeners, subscription cycles, memory exhaustion, malicious oversized trees or effects.
- Screen-reader/focus regressions masking a security-relevant warning.
- Misleading optimistic status and inferred intent used as a capability.
- Prompt injection through UI content if an LLM agent consumes displayed or remote text.

No signed UI snapshot grants business authorization. The server treats UI state as an untrusted view even when it is integrity-protected.

## 10. Performance, responsiveness and accessibility evidence

Use the **same behavior and workload** when comparing ZenoUI against an established renderer/framework (optionally Livewire 4 in an applicable server-driven configuration); do not extrapolate from synthetic microbenchmarks.

Prototype portfolio, in increasing complexity:

1. Counter: repeated clicks, duplicate submissions, concurrent updates and remount.
2. Validated form: draft-preservation, dependent fields, async validation, keyboard and error recovery.
3. Searchable/filterable table: dynamic dependencies, delayed queries, cancellation, pagination, stale results.
4. Domain showcase (after foundations): quote-expiry swap and recovery UI backed by an independently authoritative mock/qualified core, never a fake transaction success.

Measure per test environment:

| Dimension | Metric/observation |
| --- | --- |
| Correctness | Invariant failures, accepted stale updates, rejected/retained counterexamples |
| UI update efficiency | Dependency nodes evaluated, changed semantic nodes, DOM mutations |
| Interaction responsiveness | Interaction to Next Paint (INP) and long tasks |
| Authoritative latency | Input to accepted commit; commit to coherent published UI |
| Network | Bytes/request, requests/interaction, deduplication and retry behavior |
| Resources | Browser memory, CPU, retained subscriptions, server CPU |
| Accessibility | WCAG 2.2 AA-oriented audit, keyboard journey, focus/selection, screen reader |
| Usability | Task success, completion time, recovery rate, comprehension and perceived predictability |
| Developer effort | Specification complexity, implementation size, defects/feature, maintenance burden |

An INP of **200 ms or less at the 75th percentile** is a common good-responsiveness reference, **not** a promise for this architecture; separate it from round-trip server latency. The release gate should specify actual test device classes and representative tasks. Use [web.dev INP](https://web.dev/articles/inp) and [W3C WCAG 2.2](https://www.w3.org/TR/WCAG22/) as baseline guidance.

Test at least one keyboard-only and screen-reader journey; automated accessibility lint is not sufficient. Honor `prefers-reduced-motion`; document focus restoration, live-region announcement frequency, contrast and responsive disclosure. Keep these as observable acceptance tests, not assumptions that a pure AST implies correct browser output.

## 11. Implementation phases and dependency gates

These are **proposed future milestones**, not promises for the already published 2.4.0 release. Keep each task in a separate bounded PR. No subtask may claim completion merely because this design PR is merged.

### M0: Public architecture agreement (docs only)

- Confirm the three-core boundary and nonclaims against current 2.4 code.
- Record which semantic view features are *new proposals*, which ZAL capabilities are experimental, and which interfaces are already released.
- Select an initial narrow UI profile and declare unsupported constructs explicitly.
- Approve one small counter, form and search example corpus and human-reviewed expected outcomes.
- **Exit:** reviewed architecture, threat model, bounded profile, ownership matrix and initial expected test fixtures. **No code capability claimed.**

### M1: Pure Presentation Core (minimal vertical slice)

- Define closed `SemanticView`, stable IDs, visibility/action/role enums, explicit captured context and total refusal semantics.
- Implement deterministic `P(S,J,C)` as a pure evaluator with bounded resource accounting; prefer existing ZenoFCIS patterns without changing the 2.4 public API.
- Define canonical serialization / round-trip only if needed and justified. Evaluate any reused codec against actual profile semantics, no ad hoc compatibility assumptions.
- Pair user-readable scenario cases with positive and negative admission expectations.
- **Exit:** exhaustive bounded cases for a toy profile, independent reference oracle, evidence of purity and refusal paths; no claim of verified browser behavior.

### M2: Versioned command protocol and reconciliation

- Model typed actions, base version/root, request/response identity, idempotency and conflict/refusal outcomes.
- Mount an integration adapter using the existing Authority and SQLite/outbox path; prohibit a second commit route.
- Exercise overlapping requests, disconnects, duplicate delivery, multi-tab, delayed response and restart.
- **Exit:** no accepted stale overwrite in generated test scope; server CAS/idempotency tested independently; unknown outcomes preserved.

### M3: Transactional reactive runtime and one browser shell

- Build a small dependency graph keyed by snapshot and presentation input revisions.
- Update only affected projections; publish a coherent UI revision.
- Implement a simple adapter into an established browser rendering technology with keyed identity, event capture and safe text output.
- **Exit:** counter/form/table live in a browser; differential semantic tree oracle, resource bounds and focus/selection tests pass in declared environments.

### M4: Adaptive UX + accessibility

- Introduce progressive disclosure, action priority, error explanation, task-based navigation and transparent user overrides.
- Bind permissions and safety to the **Application Core**; UI adapts but never authorizes.
- Add the expired-quote showcase and accessibility expectations.
- **Exit:** explicit finite policy checks, keyboard and screen-reader scenarios, human review of task success and recovery. A11y status remains scoped.

### M5: Temporal verification and adversarial sequence generation

- Instrument effect footprints, cancellation and generation lifetimes.
- Build property-based action/environment scheduler, minimized counterexamples, mutation tests and reproducible seeds.
- Evaluate static temporal dependency checks as an optional research extension.
- **Exit:** event-order/timeout/remount matrix with falsifying mutation controls and archived result scope; no liveness claim without fairness assumptions.

### M6: Neurosymbolic search and measured optimization

- Human supplies tasks and semantic constraints; agent proposes policy/layout/component variations.
- Independently admit and check candidate meaning, run browser/usability evaluations, retain negative examples and Pareto tradeoffs.
- Optimize only among candidates satisfying hard security, correctness and accessibility gates; hold out user tasks and device conditions.
- **Exit:** demonstrated improvements over a measured baseline with raw artifacts, unchanged specifications where behavior preservation is claimed, and explicit generalization limits.

### Deferred until evidence warrants

- General-purpose CRDT/offline invariant engine, relational reactive database, self-built DOM reconciler, universal temporal type system, unrestricted natural-language execution semantics, native/mobile multiple-shell productization, automatic "optimal UI" claims.

## 12. Agent work packages

Use the detailed [handoff](AGENT_HANDOFF.md). Suggested parallel ownership after M0:

| Track | Deliverable | Prerequisite | Independent acceptance |
| --- | --- | --- | --- |
| UI-P | Semantic profile + pure presentation evaluator | M0 contracts | Exact finite policy oracle, negative vectors |
| UI-C | Versioned action/commit protocol adapter | Existing Authority route + M0 | Concurrent commit/replay/unknown-outcome matrix |
| UI-R | Reactive dependency engine and keyed patch structure | Stable SemanticView | Snapshot consistency, invalidation/dynamic-dependency tests |
| UI-B | Browser shell and accessibility translation | Stable SemanticView + protocol stub | Browser interoperability, focus, screen-reader checks |
| UI-T | Temporal scheduler/test harness | Versioned protocol + shell fixture | Property sequences, minimized counterexamples, mutation kills |
| UI-U | UX scenarios and empirical test protocol | M0 human tasks | Tasks, labeled expected paths, study design |
| UI-N | Neurosymbolic synthesis/optimization | M1–M5 validated interfaces | Hard-constraint gate + held-out tasks + measured comparison |
| UI-V | Independent security/assurance reviewer | Every stage | Source-bound claims, threat checks, no unsupported guarantees |

No agent silently invents owner intent. A contract ambiguity blocks the affected stage and produces an explicit question or counterexample, while unrelated tracks continue.

## 13. Release gate and definition of done

**Design PR definition of done:** documentation is understandable, public-source safe, accurately labeled as a proposal, indexed, and reviewed against the existing 2.4 architecture; no code changes or unqualified success claims. GitHub CI/docs policy acceptance is separate.

**First runnable prototype definition of done:** a counter, form and table share (a) one checked/replayed application contract where applicable, (b) one deterministic presentation contract, (c) a revision-aware action protocol and (d) a real browser adapter. Adversarial tests must cover concurrent overlapping events, reversals, retries, interruptions, remount, invalid client values and focus/accessibility obligations. Publish all assumptions and coverage.

**Future promotion requires:** exact source/checker/adapter/device identifiers, independent replay, relevant ATDD integration, documentation privacy and link checks, browser+manual a11y evaluation, benchmark comparisons and owner approval. The existing [release assurance gate](../RELEASE_ASSURANCE.md) remains in force. Neither this proposal nor a successful `cargo test` alone qualifies a release.

## 14. Open decisions for human review

These are intentionally **not** chosen by a coding agent without evidence:

1. Is ZenoUI initially **a library extension plus adapter**, a standalone package, or a separate integration project? Prefer the least invasive option until measured.
2. What finite semantic-UI profile is expressive enough for the first three workloads without importing unrestricted HTML, scripting or CSS?
3. Should the first shell target a React-style client adapter or a simple server-rendered/HTML adapter? Benchmark and compare complexity.
4. Which user intents are explicit and which are inferred? What are opt-out, override, explanation and privacy requirements?
5. Which presentation data must be canonical/protocol-visible versus nonauthoritative browser styling?
6. How are trusted permission projections, time observations and quote freshness bound to snapshots?
7. Which formal checker(s), random scheduler and empirical UX tasks constitute sufficient evidence at each tier?
8. What is the source-of-truth for UI policy review: a new ZAL profile, dedicated contract format, or a typed intermediate representation with controlled-English views? No unreviewed parser extension.

Any accepted answer must document why simpler alternatives were insufficient and state what needs to be measured.

## 15. Research-to-requirement map

This document **synthesizes** public papers and the targeted October 15, 2026 research notes; it is not a systematic literature review, comparative product benchmark, or claim that one paper proves the combined architecture.

| Source | Usable insight | ZenoUI choice | Limit |
| --- | --- | --- | --- |
| [Riffle, UIST 2023](https://doi.org/10.1145/3586183.3606801) | Reactive relational/incremental state | Fine-grained dependencies and measured invalidation | Does not prove a whole browser database is optimal |
| [Wunder, Das & Gaboardi, 2026 preprint](https://arxiv.org/abs/2607.27074) | Temporal type/effect and dependency analysis | Explicit scheduling, cancellation and lifecycle metadata | Static checker is future research, not delivered |
| [Nielsen, Kristiansen & Bahr, PADL 2026](https://doi.org/10.1007/978-3-032-15981-6_3) | Asynchronous FRP property testing with temporal logic | Generated event schedules, minimized counterexamples | Model correctness and test coverage remain scoped |
| [Bombarda, Bonfanti & Gargantini, 2025](https://doi.org/10.1007/s10009-025-00812-2) | Formal MVC UI specifications and tests | Typed application/presentation models | Does not imply full browser proof |
| [Köhler et al., IEEE TSE 2025](https://doi.org/10.1109/TSE.2024.3477723) | Convergence is not application invariant safety | Authority-aware synchronization and CAS | No automatic invariants from CRDT convergence |
| [Ma'ayan, Maoz & Ringert, 2026](https://doi.org/10.1145/3767159) | Modular reactive synthesis specifications | Small independent contracts and incremental milestones | Evidence is scoped to its study |
| [Maoz, Shalom & Taieb, 2026](https://doi.org/10.1007/s10270-026-01364-y) | Spec quality/validity review | Human-readable positive and negative scenarios | A verified implementation can meet a bad specification |
| [Lee et al., ReDemon UI, UIST Adjunct 2025](https://doi.org/10.1145/3746058.3758454) | Reactive UI synthesis from examples | Optional LLM proposal and independent checking loop | Early research, not production qualification |
| [Livewire 4 islands](https://livewire.laravel.com/docs/4.x/islands) | Shared-state race warning in an established framework | Adversarial consistency benchmark | Confirm version and comparable semantics when testing |
| [7GUIs](https://eugenkiss.github.io/7guis/) | Small UI interaction benchmark portfolio | Counter/form/table plus additional cases | Not a complete usability benchmark |
| [WCAG 2.2](https://www.w3.org/TR/WCAG22/), [web.dev INP](https://web.dev/articles/inp) | Accessibility and responsiveness standards | Explicit browser/manual acceptance | Standards conformance needs actual evaluation |

**Interpretation:** The best supported immediate combination is *formal behavior models + versioned synchronization + fine-grained reactivity + asynchronous model/property testing*. ZenoUI's separate **Presentation Core** and neurosymbolic optimization workflow are architectural proposals that require their own validation.

## 16. Mental shortcuts

- **Core decides truth; presentation decides communication; shell performs effects.**
- **A disabled control is not authorization.** Recheck at the committing boundary.
- **Old responses do not get to rewrite newer reality.** Version both server commit and client reconciliation.
- **Recompute dependencies, not the whole world.** Publish a coherent snapshot.
- **Time is an input, not a hidden side effect.** Model cancellation, revisions and lifecycle explicitly.
- **A passing proof checks a specification, not its adequacy.** Human examples and counterexamples matter.
- **Neural agents propose; deterministic checks judge.** UX quality still needs people and measurement.
