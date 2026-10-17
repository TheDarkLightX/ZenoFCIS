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

ZAL currently provides a **bounded experimental** `zal/finite-fsm/1` behavior dialogue with symbolic and controlled-English surfaces, exact-revision review and scoped checks. This is a promising **specification input**, not a released, general UI contract language. Any UI profile, type or parser proposed below must be independently specified, implemented, reviewed and qualified; do not pretend the existing ZAL parser already supports UI semantics.

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

## 4. Default UI from the application contract

**The default workflow starts with an app contract, not a UI policy.** Supply `project.zeno`, `v2/policy.json`, decision examples and a short statement of user tasks. Import declarations, laws and ordered policy through existing admission, retaining object IDs and exact source identity; see [contract rules](../CONTRACT_RULES.md). A small reviewed presentation descriptor supplies labels, units, task names, consequential-action classifications and context sources alongside the contract, without adding unsupported contract keys.

| Contract input | Generated baseline | Acceptance gate |
| --- | --- | --- |
| Commands and command fields | One discoverable action route per command; typed form, review and submit controls | Complete command coverage and exact command-field binding, never dispatch by list position |
| State fields, types, ranges and variants | Typed views and inputs with labels and units beside values | Reviewed units, including explicit unitless values; bounded Boolean/enum/integer codecs preserving full integer width |
| Reasons and admission refusals | Explanations, associated fields where known, and permitted recovery guidance | Preserve decision class, reason ID and precedence; never reinterpret refusal as success |
| Policy, laws and real Authority decisions | Eligibility and refusal reasons, followed by task, review, busy and uncertainty masks | Full declared-domain UI–Authority comparison; admission and law failures make the action unavailable |
| Channels, effect payloads and outbox records | Request, commit and delivery status views | Matching evidence for pending, committed, queued and acknowledged states; commit does not imply external delivery |

Field names may suggest labels, but a type name does not establish a real-world unit. Missing meanings become explicit descriptor questions. Principal and authorization observations come from the declared application adapter, never automatically from editable form fields. Unresolved descriptors block qualification of the affected package; unsupported or oversized domains may produce a scaffold with an explicit unsupported result.

The deterministic baseline contains task navigation, state summary, the selected form, review dialog, outcome/status and a collapsed record. The same generator emits the presentation contract, keyed Semantic UI Tree, application bindings, controlled-English readback and positive/refusal fixtures. It emits closed data interpreted by a reviewed primitive library, with no app-specific components, event handlers or CSS required. Optional ZAL-UI refines this baseline; the neural search loop is not needed to create it.

Keep any admitted `zenoui/semantic-contract/1` prototype unchanged. Typed workflow/binding additions require an explicitly migrated proposed `zenoui/semantic-contract/2`, with old/new observation comparison on the common domain. The optional `zal/ui/1` surface below does not silently widen either profile.

**Regression target:** the supplied 2.3 prototype presentation predicate allowed `available=1, reserved=5, quantity=1, authorized=true`. The inspected current [inventory policy](../../verification/kernel-laws/src/oracle/templates/inventory-reservation/v2/policy.json) refuses the Reserve request because `reserved + quantity > 5`. Retain this exact tuple and the complete source identities as a cross-version regression fixture. This is a source-comparison target, not a fresh replay of the original 2.3 Authority. Never copy demo predicates or their restricted enumeration as the acceptance oracle; replay against the actual selected Authority and include law-invalid/admission-refused tuples.

**M1 success test:** another agent receives only the written workflow, pinned tooling, the second app contract and reviewed intent/descriptors, then generates a different app's UI without hidden instructions or app-specific renderer/handler/CSS changes or a custom ZAL-UI policy. Proposed second app: [document release](../../core-apps/document-release/project.zeno), with Edit, Submit, Approve and Release, version refusals and release-outbox status. Both apps must use the same generator and real application adapters. Record all descriptor questions, reviews, per-action positive journeys, negative cases and reproduction commands. This is an M1 exit condition, not a later search milestone.

### 4.1 When a UI cannot be synthesized: four honest tiers

Some interfaces do not follow from a contract: rich visualisations (price charts, order-book depth, maps), direct
manipulation (drag and drop, canvases, games), real-time collaborative views and brand pages. Contracts too large to
check exhaustively can still get a generated UI, but not a fully checked one. Every UI region therefore states its
tier and evidence level:

| Tier | Covers | Evidence |
|---|---|---|
| 1. Synthesized | Forms, actions, task flows, status and explanations derived from the contract | Finite checked against the real Authority |
| 2. Synthesized and shaped by ZAL-UI | Adaptive emphasis, disclosure, enablement and focus from a reviewed policy | Finite checked |
| 3. Custom component behind a typed boundary | A chart, order book or canvas that may only render declared data bindings and emit declared typed events. It never dispatches commands, decides enablement or captures consent; the Presentation Core keeps those | Boundary finite checked; component internals tested only, and labelled so |
| 4. Custom UI with ZenoUI as checker | An existing or bespoke interface whose events are mapped to typed actions | Agreement with the Authority (W ⇒ A) and the mandatory invariants checked; everything else tested |

A tier-3 component is admitted like any primitive, with its own declared bindings, events, accessibility contract and
conformance tests. It can never be used to bypass the Presentation Core or the Authority.

## 5. Presentation-policy best practices

The Presentation Core is ZenoUI's main addition to the original reactive-systems research. Encode supported rules for:

- **Intent-aware adaptation:** explicit task/selection takes priority over inferred intent; inferred recommendations are advisory only and may change **emphasis only**. Hints cannot select a task, reorder controls, reveal/hide content, move focus, change fields, enable an action, consent or invoke a command.
- **Progressive disclosure:** show the necessary next step first; reveal advanced settings when task/state warrants it. Every required task remains discoverable by a documented navigation path, including keyboard access.
- **Contextual action priority:** make a safe recovery action primary when the intended operation becomes invalid; distinguish explanatory, reversible advice from business requirements.
- **Explainability:** expose why a control is unavailable and what permitted action can repair that condition; never conceal necessary risks or unexpected consequences.
- **Predictability and user control:** preserve location, mental model, selection and drafts when possible. Adaptive changes are explainable, reversible or explicitly user-overridable wherever appropriate.
- **Honest asynchronous status:** differentiate local input, pending submission, accepted transaction, delivered effect, failed attempt, stale data and unknown outcome. Optimistic display cannot impersonate an authoritative commit.
- **Accessibility-by-contract:** explicit role/name/state, keyboard operation, focus placement/return, live announcements, and reduced-motion semantics. Color, hover or animation alone never conveys success or proof.
- **Responsive presentation:** viewport and input capabilities affect layout and disclosure, **not** domain truth.
- **Calibrated motion:** motion communicates genuine state transitions or attention movement and can be disabled. Do not conflate visual polish with actual usability.

Prefer **task success** and **error recovery** over novelty. These are UX hypotheses and policy constraints; no symbolic checker can establish universal ease of use, taste or comprehension. Test them with representative human tasks.

### 5.1 Adaptive-UX limits and user control

Keep inferred hints separate from explicit task state and checked application facts. Only the latter drive disclosure and eligibility. Each adaptive change carries a stable rule or hint ID and a short **“Why this changed”** explanation. **Undo** restores prior presentation choices; **“Keep this layout”** suppresses further hints. Neither control can restore stale permissions, consent or application state. A wrong prediction costs attention, never access or authority.

Require paired-run noninterference for identical semantic inputs and explicit event traces: changing hints, theme, width or motion must preserve keyed eligibility, command bindings, selection, consent, request identity and outcomes. Layout may differ for responsive presentation; inferred hints still change emphasis only. Inference is optional, and an inconclusive evaluation keeps the deterministic baseline.

### 5.2 Example: expired quote

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

## 6. ZAL-UI v1: optional presentation policy

`zal/ui/1` is a **proposed** small versioned language over the generated manifest, not syntax supported by the current ZAL parser. IDs refer to admitted tasks, nodes, actions, reasons and facts; directives describe semantic data, not scripts or arbitrary components.

```text
profile "zal/ui/1"
interface NAME
task TASK
when RULE_ID: CONDITION {
    DIRECTIVE ...
}
DIRECTIVE = emphasize NODE | deemphasize NODE | reveal NODE
          | enable ACTION | disable ACTION
          | explain ACTION because REASON | focus NODE
motion { transition instant | fade MILLISECONDS; reduced_motion instant }
invariant OBLIGATION_ID
CONDITION = true | false | FACT | task == TASK
          | not CONDITION | (CONDITION and CONDITION)
          | (CONDITION or CONDITION)
```

The grammar is a sketch pending profile admission. Require one profile, interface and motion declaration; unique task/rule IDs matching `[a-z][a-z0-9_]{0,31}`; precedence `not`, then `and`, then `or`, with parentheses overriding it. Facts are typed Boolean bindings to explicit interaction state or checked application predicates. Unknown facts, targets, operations, extra syntax and over-limit collections are refused. Freeze domain, size and expression bounds before checking. Arbitrary arithmetic, function calls, temporal syntax and inferred-hint reads are absent from v1 conditions.

| Construct | Proposed semantics |
| --- | --- |
| `task` | Declares a selectable goal and manifest action bindings; does not select or invoke it. Initialization selects generated `overview`; declaration order has no meaning. |
| `when` | Evaluate all conditions against the same immutable pre-view inputs. True rules contribute directives without persistent side effects or feeding other rules in that evaluation. |
| `emphasize` / `deemphasize` | Choose an approved prominence variant without changing visibility, focusability, eligibility or order. Conflicting active variants are a finite-check error; default is normal prominence. |
| `reveal` | A conditional manifest node is shown iff at least one applicable reveal condition holds; otherwise it is hidden. Always-visible nodes stay visible. Explicit disclosure-open state is a fact; hiding preserves drafts. |
| `enable` / `disable` | E is true if no enable rules target the action, otherwise their conditions' OR; D is the OR of disable conditions. `enabled = W and E and not D and selected and ready and current_consent_if_required`. W is valid-input eligibility against the real Authority; ready excludes busy/uncertain states. Dispatch rechecks the masks and invokes Authority; enable never grants permission. |
| `explain` | Attach a registered parameterized reason. Core refusal is primary, followed by uncertainty/busy/input/consent reasons, then policy reasons sorted by stable ID. Every unavailable action needs an explanation; a label cannot fabricate a core reason. |
| `focus` | Reducer requests focus once on a rule's false-to-true entry, keyed by event and target. Only explicit navigation, review/error handling or its matching completed request may trigger it. Background progress and inference cannot; hidden/inert targets are invalid. Initialization uses manifest focus. |
| `motion` | Choose a closed preset, duration 0 or 120–160 ms; reduced motion is always instant. Animation completion never gates domain actions or reducer progress. |
| `invariant` | Select a versioned checker obligation, not assert its truth. All nine mandatory invariant rows in section 10 remain required; generators cannot omit them and unknown IDs are refused. |

**Merge precedence:** authoritative eligibility and workflow restrictions first; disable over enable; reveal by OR. Conflicting prominence variants or distinct simultaneous focus targets reject the candidate; identical focus requests coalesce. Manifest order is explicit data, and source rule order never breaks ties. Mandatory accessibility and evidence obligations veto a candidate. This deliberately differs from the application policy's first-matching-case semantics in [contract rules](../CONTRACT_RULES.md).

| Operation | User and accessibility effects |
| --- | --- |
| De-emphasise | Remains visible, keyboard reachable and in the accessibility tree, with sufficient contrast, label and target size. Never make a required task unreadable through opacity. |
| Disable | Remains visible with an independently readable reason. Native disabled controls do not activate and normally leave tab order; an `aria-disabled` variant must suppress pointer, keyboard and dispatch activation. |
| Hide | Absent from visual display, accessibility tree and tab order, retaining keyed state/drafts. Resolve focus to a declared visible return target first; preserve a discoverable disclosure/navigation route. |
| Remove | Absent from the component tree. V1 permits only optional structural choices at generation, never removal of required actions or hint-driven removal. Draft deletion requires an explicit reviewed reset/discard. |

Hide/remove describe manifest behavior; they are not additional v1 directives. The generated task navigation remains available even when an action is disabled. For inventory, explicit task selection reveals its form, checked validity and core eligibility constrain confirmation, authorization refusal is explained, and only a matching accepted commit reveals success.

### 6.1 Shared-AST readback, finite-FSM boundary and ladder

Generate symbolic and controlled-English surfaces from **one canonical AST with source maps**. Both admitted surfaces must parse back to the same AST. Include defaults, masks, precedence, bindings, definitions, scope, owner-decision status and evidence per rule; arbitrary prose remains an untrusted proposal. Human teach-back and independent checks still test whether the meaning is right.

Reuse `zal/finite-fsm/1` only for compatible workflow slices. Its existing profile permits at most 8 states, 8 events and 4 Boolean facts; arithmetic, effects and concurrency are unsupported, and overlapping enabled transitions are errors. A supplied `authorized` fact is not authenticated identity. See [ZAL grammar](../zal/GRAMMAR.md), [behavior implementation](../../integrations/zal/behavior.py) and [factory binding](../../integrations/zal/factory.py). UI lowering, parser support and real-Authority correspondence need new evidence; profile reuse does not prove the full UI.

Use the ZAL explanation ladder: **formula → formal reading → precise English → intuitive wording → question**, retaining the same definitions, scope and positive/negative examples. For `W ⇒ A`, the question is “Can this UI offer a request the core refuses?” and the search target is `W and not A`. Register UI-specific ladder templates with paired round-trip fixtures; the ladder is an explanation surface, not execution or permission. Use [question and teach-back tools](../zal/LOOPS.md) to expose missing meaning before adoption.

## 7. Versioned synchronization protocol

A server-driven or distributed UI is a distributed system. The original October 15 review identified island concurrency as a motivating example; verify the comparison framework and version before making a public comparative claim.

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

## 8. Incremental reactive dependency engine

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

## 9. Temporal effects and component lifecycle

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

## 10. Formal obligations and verification strategy

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

### 10.1 Real-Authority agreement and nine mandatory invariants

For a frozen state/request/context tuple x, **A(x)** means the actual library Authority accepts after admission and laws; this Boolean predicate is distinct from the typed command A in section 3. **W(x)** is presentation valid-input eligibility before task, policy and workflow masks. Require **W(x) ⇒ A(x)**; prefer equality and enumerate every conservative omission. Build the acceptance reference from the real Authority, never a duplicate UI predicate or fake adapter. Check every declared tuple, including unreachable, law-invalid and admission-refused cases with tagged outcomes. Decoder tests separately cover malformed bytes outside the finite typed domain.

**No disable-everything escape:** each required action needs an Authority-accepted positive witness and a reachable permitted journey, plus refusal cases and an independent coverage oracle. A policy may restrict invocation but cannot remove inspection of the action/reason. An action with no accepted witness is an explicit scope/meaning blocker, not a silently passing row. Preview cannot prevent a later permission change, so actual dispatch and commit always revalidate authority and snapshot identity.

The catalogue below supplements U-SAFE/U-INTENT/U-A11Y/U-LIVE, rather than replacing the sync/lifecycle obligations. Its final row couples two named obligations; both are mandatory, giving nine catalogue rows.

| Named invariant | Finite obligation | Required browser/integration evidence |
| --- | --- | --- |
| `confirmation_requires_commit` | Success requires the matching accepted-and-committed outcome; submission, Reject, admission refusal, Unknown and CommittedFailure are not success. | Forged/mismatched replies and lost acknowledgements never display success. |
| `forbidden_actions_not_invokable` | W ⇒ real A; enabled and dispatched requests satisfy W and all workflow masks. | Actual Authority refusal paths; permission changes and permissive-guard/bypass-dispatch mutants fail. |
| `required_actions_remain_discoverable` | Persistent task navigation; controls and reasons reachable within a declared UI-step bound without a domain command. Busy/uncertain may block invocation, not inspection. | Keyboard and screen-reader paths, including disabled actions and recovery. |
| `no_stale_or_backward_revision` | Updates are nondecreasing within one store/epoch; equal revisions require identical canonical contents. Stale replies are no-ops; reset creates a new epoch and invalidates review/outcomes. | Reorder, replay, reconcile and restart schedules preserve the eligible view. |
| `consent_bound_to_what_is_shown` | Consequential dispatch matches reviewed command bytes, values/units, principal/context, contract, snapshot and request identity; edits or binding changes invalidate review. | Compare rendered review facts with exact dispatched bytes; reject stale confirmation. |
| `run_identity` | Replies match epoch, run, request and cell ordinal/selection generation where applicable. Identity never silently wraps or reuses; finite equality abstractions alone do not prove lifetime uniqueness. | Old progress, cells, cancelled runs and disposed sessions change nothing. |
| `preferences_never_change_meaning` | Paired theme/width/motion/hint inputs preserve the keyed security projection and command trace, allowing layout-only steps. | Same explicit journey across the preference matrix; intent-alters-guard/selects-task/confirms mutants fail. |
| `accessible_presentation` | Names, explanations, required routes, unique valid focus targets and dialog states; no focus on hidden/inert nodes. | Contrast, keyboard, focus return, announcements, zoom/reflow, reduced motion and manual assistive-technology review. |
| `complete_initial_view` / `truthful_evidence` | Every declared region initially has data, empty or loading status; verdicts/counts derive from matching evidence scope, including partial/unknown results. | No blank status region, fabricated claim or pending work displayed as complete. |

The noninterference projection includes keyed eligibility/enabled values, explicit selection, input-to-command bindings, consent, replay identities, outcomes and command paths. It excludes approved prominence and responsive layout. Compare initialization and every admitted reducer transition for the same supplied semantic events; this does not claim that people behave identically under different layouts.

Propose lowering the reducer/policy to an ordinary finite FCIS subject (`project.zeno`, `v2/policy.json` and finite program), with checked bindings for labels and exact payloads. Encode obligations as Boolean targets for `transform check` and `transform replay`, alongside `contract review --gaps`, `contract lemma`, `contract scenario` and `contract probe`; see [loop tools](../zal/LOOPS.md). Independently compare the presentation evaluator, lowered evaluator and real application adapter on identical domains and event traces. Preserve integer widths, overflow/refusal and eager evaluation; never narrow wide amounts through JavaScript Number. Payload equality abstractions and decomposed checks require explicit correspondence/interface evidence; otherwise bound the domain and limit the claim.

Every invariant needs a positive witness, a planted defect rejected by its intended predicate, and a deterministic counterexample input or minimized reachable trace on failure. Bind results to exact contract/policy/adapter/checker identities, domains, assumptions and expected/actual observations. Exhausted budgets, missing bridges and unsupported expressions remain inconclusive. Reports separate finite checking, bridge conformance, browser evidence and theorems using the labels below; no aggregate “verified UI” flag hides absent evidence.

### 10.2 Proof and evidence labels

Use explicit labels, consistent with [epistemic status](../adr/0003-epistemic-status.md):

- `proposed`: design only; no executable evidence.
- `example-tested`: hand-picked behavior on one implementation.
- `property-tested`: randomized traces, seed/count/coverage recorded.
- `bounded-exhaustive`: all modeled inputs in explicitly declared finite scope.
- `machine-checked`: specific theorem/implementation/model/assumptions with source-bound checker evidence.
- `browser-validated`: specified adapter/runtime/device combinations.
- `human-validated`: task studies identifying participants, tasks and measures (privacy-safe aggregate reporting).

Never promote bounded model checks to universal proofs. Never claim browser-wide "bug free" or mathematically optimal UX from a semantic-core proof.

## 11. Security and trust threats

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

## 12. Performance, responsiveness and accessibility evidence

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

An INP of **200 ms or less at the 75th percentile** is a common good-responsiveness reference, **not** a promise for this architecture; separate it from round-trip server latency. The release gate should specify actual test device classes and representative tasks. Treat INP guidance and WCAG 2.2 as qualification inputs whose versions must be recorded with the eventual repository evidence.

Test at least one keyboard-only and screen-reader journey; automated accessibility lint is not sufficient. Honor `prefers-reduced-motion`; document focus restoration, live-region announcement frequency, contrast and responsive disclosure. Keep these as observable acceptance tests, not assumptions that a pure AST implies correct browser output.

## 13. Neurosymbolic generation and evaluation

The deterministic contract-to-UI baseline belongs to M1. Optional neural proposals and search belong to M6, after the semantic, Authority, browser and accessibility gates exist. Existing tools judge admitted finite subjects; the UI-specific lowering and bindings remain proposed work. Search never chooses intended meaning or grants authority.

| Loop | Existing tools and proposed UI use | Gate |
| --- | --- | --- |
| Specification | `contract review --gaps`, `contract probe`, `contract scenario`, `loop question`, `loop teach-back`: find missing descriptors, contradictions, unreachable tasks and positive/refusal examples. | Owner-reviewed meaning, explicit bounds and positive witnesses; zero findings alone does not establish complete requirements. |
| Synthesis | Deterministic baseline first; then `loop sketch`, expression proposals and program search over bounded holes where fixed-target semantics apply. | Independent obligations, real-Authority parity and `transform check` plus receipt replay. New layout freedom is checked through its security projection, not falsely called whole-view equivalence. |
| UX optimisation | Browser task runners measure real-adapter journeys; candidate policy changes return through contract review and the same check/replay gates. | Hard safety/accessibility constraints before ranking; task completion/errors first, then time, steps and attention proxies, with human review and held-out tasks. |
| Counterexample repair | Existing expression repair doors, using the exact failed input/trace and rule/source map. | Retain the old witness; rerun all affected invariants, parity, replay and browser regressions. No weakening requirements or shrinking the domain to erase a failure. |
| Meta-optimisation | Proposed UI adapter for the experimental MAP-Elites archive; compare equal-budget enumeration and single-champion search. | Noninterference, task coverage, accessibility and exact replay before archive entry or migration; human review of the selected artifact. |

See [ZAL loops](../zal/LOOPS.md) and [the contained check/replay implementation](../../crates/zeno-fcis-cli/src/loop_doors_command.rs). Platform, containment and resource prerequisites still apply; a timeout or unavailable platform cannot promote a candidate. Supported reference capture requires correspondence checks; executable-JavaScript capture is not assumed.

The existing [MAP-Elites archive](../../integrations/elite_archive/README.md) holds equivalent finite programs described by instruction count and canonical byte length. It is experimental, not a UI optimizer or a population trainer. A new UI adapter needs measured layout descriptors and an independently checked security projection; do not weaken the existing archive's whole-program equivalence gate. Archive variants may change approved grouping, density, order and prominence only while preserving commands, guards, disclosure conditions, consent and evidence meaning. This is reviewed design-time search; runtime inferred hints remain emphasis only.

Among candidates satisfying correctness, safety and accessibility, evaluate `Q = w1*Usability + w2*Efficiency + w3*Clarity - w4*Latency`. Freeze tasks, weights, attention proxies and budgets before comparison. Validate proxies with people, report Pareto tradeoffs and retain the deterministic baseline if improvement is unsupported. Ratings, attractive previews and search completion do not establish eligibility or universal optimality.

## 14. Implementation phases and dependency gates

These are **proposed future milestones**, not promises for the already published 2.4.0 release. Keep each task in a separate bounded PR. No subtask may claim completion merely because this design PR is merged.

### M0: Public architecture agreement (docs only)

- Confirm the three-core boundary and nonclaims against current 2.4 code.
- Record which semantic view features are *new proposals*, which ZAL capabilities are experimental, and which interfaces are already released.
- Select an initial narrow UI profile and declare unsupported constructs explicitly.
- Approve one small counter, form and search example corpus and human-reviewed expected outcomes. Pin inventory and the proposed document-release contract for the M1 factory test, including units, principal sources and consequential-action classifications.
- **Exit:** reviewed architecture, threat model, bounded profile, ownership matrix and initial expected test fixtures. **No code capability claimed.**

### M1: Pure Presentation Core (minimal vertical slice)

- Define closed `SemanticView`, stable IDs, visibility/action/role enums, explicit captured context and total refusal semantics.
- Implement deterministic `P(S,J,C)` as a pure evaluator with bounded resource accounting; prefer existing ZenoFCIS patterns without changing the 2.4 public API.
- Define canonical serialization / round-trip only if needed and justified. Evaluate any reused codec against actual profile semantics, no ad hoc compatibility assumptions.
- Pair user-readable scenario cases with positive and negative admission expectations.
- Add deterministic default synthesis from the application contract and reviewed descriptors, complete command coverage, shared-AST readback and positive/refusal cases; no neural loop or custom ZAL-UI is required.
- Check the nine mandatory invariants, W ⇒ real A, per-action positive journeys, noninterference and the exact inventory regression tuple; record finite bounds, mutants, replay and adapter correspondence.
- Run the second-app success test: a different agent uses only the written workflow and pinned inputs to generate the document-release UI through the same generator, primitive library and real adapter path, recording questions and all reproduction steps.
- **Exit:** exhaustive bounded cases, independent reference oracle, purity/refusal evidence, and both apps running through a shared minimal renderer with actual Authority bindings, per-action positive/negative journeys and scoped browser/accessibility checks. No app-specific renderer/handler/CSS or custom ZAL-UI is needed for the second app; no hidden instructions. Browser evidence covers only the exercised adapter/environment, never a whole-browser proof.

M1 therefore requires the minimal typed adapter and renderer portions of ZUI-02/ZUI-04 as supporting work. It does not wait for the full M2 concurrency/recovery matrix or M3 incremental runtime. M2/M3 retain their broader gates; milestone numbering is not permission to defer the second-app test.

### M2: Versioned command protocol and reconciliation

- Model typed actions, base version/root, request/response identity, idempotency and conflict/refusal outcomes.
- Mount an integration adapter using the existing Authority and SQLite/outbox path; prohibit a second commit route.
- Exercise overlapping requests, disconnects, duplicate delivery, multi-tab, delayed response and restart.
- **Exit:** no accepted stale overwrite in generated test scope; server CAS/idempotency tested independently; unknown outcomes preserved.

### M3: Transactional reactive runtime and one browser shell

- Build a small dependency graph keyed by snapshot and presentation input revisions.
- Update only affected projections; publish a coherent UI revision.
- Implement a simple adapter into an established browser rendering technology with keyed identity, event capture and safe text output. Proposed default: React with native HTML controls; pin and qualify it before implementation, preserving drafts, selection and IME composition. Extend the minimal M1 adapter rather than introducing app-specific renderers.
- **Exit:** counter/form/table live in a browser; differential semantic tree oracle, resource bounds and focus/selection tests pass in declared environments.

### M4: Adaptive UX + accessibility

- Introduce progressive disclosure, action priority, error explanation, task-based navigation and transparent user overrides. Implement optional ZAL-UI v1 with the grammar, shared-AST readback, fixed merge precedence, finite-FSM limits and ladder fixtures in section 6.
- Keep inferred intent at emphasis only. Every adaptation has a rule/hint ID and “Why this changed”; provide Undo and “Keep this layout”. Wrong predictions may cost attention, never access or authority; check preference/hint noninterference and planted violations.
- Bind permissions and safety to the **Application Core**; UI adapts but never authorizes.
- Add the expired-quote showcase and accessibility expectations.
- **Exit:** explicit finite policy checks, paired symbolic/English round trips, conflict/unsupported-input refusals, adaptation undo/freeze and noninterference tests, keyboard and screen-reader scenarios, and human review of task success and recovery. Rerun both app journeys; A11y status remains scoped.

### M5: Temporal verification and adversarial sequence generation

- Instrument effect footprints, cancellation and generation lifetimes.
- Build property-based action/environment scheduler, minimized counterexamples, mutation tests and reproducible seeds.
- Evaluate static temporal dependency checks as an optional research extension.
- **Exit:** event-order/timeout/remount matrix with falsifying mutation controls and archived result scope; no liveness claim without fairness assumptions.

### M6: Neurosymbolic search and measured optimization

- Run the five loops in section 13: contract review/gaps and question/teach-back; sketch/search; measured UX; counterexample repair; and a proposed MAP-Elites UI archive under noninterference. Human supplies tasks and semantic constraints; proposals stay within the reviewed policy/layout/primitive grammar.
- Reuse existing `transform check` and receipt replay for admitted finite targets, qualifying UI lowering and bindings separately. Demonstrate at least one policy-hole completion and one injected-failure repair without weakening requirements.
- Independently admit and check candidate meaning, run browser/usability evaluations, retain negative examples and Pareto tradeoffs.
- Optimize only among candidates satisfying hard security, correctness and accessibility gates; hold out user tasks and device conditions.
- **Exit:** all five loops have reproducible scoped results; eligible archive entries replay and planted semantic changes fail. Record demonstrated improvement over the measured baseline, or an explicit no-improvement result retaining it, with held-out tasks, equal budgets, raw artifacts and unchanged specifications wherever preservation is claimed. No generalization or universal-optimum claim.

### Deferred until evidence warrants

- General-purpose CRDT/offline invariant engine, relational reactive database, self-built DOM reconciler, universal temporal type system, unrestricted natural-language execution semantics, native/mobile multiple-shell productization, automatic "optimal UI" claims.

## 15. Agent work packages

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
| UI-D | Default contract-to-UI synthesis (ZUI-09) | M0 + stable tree | Coverage, exact bindings, units/reasons/status and real-Authority agreement |
| UI-L | Optional ZAL-UI v1 (ZUI-10) | Generated manifest + reviewed policy meaning | Shared-AST readback, merge/conflict tests, ladder and migration gates |
| UI-I | Nine invariant gates (ZUI-11) | Finite profile + actual adapter | Positive witnesses, planted defects, finite replay and distinct browser evidence |
| UI-F | Independent second-app reproduction (ZUI-12) | Generator + minimal adapter/renderer + invariant gates | Written workflow alone produces document-release UI; mandatory M1 exit |

Label assignments **reasoning** or **step-by-step** by the judgment required; never assign semantic or proof decisions to a frozen execution checklist. No agent silently invents owner intent. A contract ambiguity blocks the affected stage and produces an explicit question or counterexample, while unrelated tracks continue.

## 16. Release gate and definition of done

**Design PR definition of done:** documentation is understandable, public-source safe, accurately labeled as a proposal, indexed, and reviewed against the existing 2.4 architecture; no code changes or unqualified success claims. GitHub CI/docs policy acceptance is separate.

**First runnable prototype definition of done:** a counter, form and table share (a) one checked/replayed application contract where applicable, (b) one deterministic presentation contract, (c) a revision-aware action protocol and (d) a real browser adapter. Adversarial tests must cover concurrent overlapping events, reversals, retries, interruptions, remount, invalid client values and focus/accessibility obligations. Publish all assumptions and coverage.

**Factory capability additionally requires:** the M1 two-app reproduction from the written workflow, with full command coverage, real-Authority agreement, positive/refusal journeys, all nine invariant gates and separate browser evidence. A single bespoke demo does not meet that requirement.

**Future promotion requires:** exact source/checker/adapter/device identifiers, independent replay, relevant ATDD integration, documentation privacy and link checks, browser+manual a11y evaluation, benchmark comparisons and owner approval. The existing [release assurance gate](../RELEASE_ASSURANCE.md) remains in force. Neither this proposal nor a successful `cargo test` alone qualifies a release.

## 17. Open decisions for human review

These are intentionally **not** chosen by a coding agent without evidence:

1. Is ZenoUI initially **a library extension plus adapter**, a standalone package, or a separate integration project? Prefer the least invasive option until measured.
2. What finite semantic-UI profile is expressive enough for the first three workloads without importing unrestricted HTML, scripting or CSS?
3. Is React with native HTML controls the right first renderer? Proposed default: React; compare a simpler HTML adapter only with equivalent behavior and measured complexity, without building a new renderer.
4. Which user intents are explicit, and is optional inferred emphasis useful? Proposed default: emphasis only, with rule IDs, explanations, Undo and “Keep this layout”. Would opt-in inferred reordering ever be desirable? It is excluded from v1 and needs a separate reviewed profile change, evidence and consent.
5. Which presentation data must be canonical/protocol-visible versus nonauthoritative browser styling?
6. How are trusted permission projections, time observations and quote freshness bound to snapshots?
7. Which formal checker(s), random scheduler and empirical UX tasks constitute sufficient evidence at each tier?
8. What is the accepted source of truth for policy review? Proposed default: a canonical typed presentation AST, generated baseline and optional `zal/ui/1` symbolic/controlled-English surfaces with round-trip checks. No unreviewed parser extension.
9. Is document release the right independent M1 app? Proposed default: yes; require a different agent to follow only the published workflow.
10. Which commands are consequential? Proposed default: every mutating command needs explicit review until individually classified; bind consent to the exact facts shown and request identity.
11. Which units and principal/context sources are authoritative for each app? These require descriptor answers; neither names nor inferred intent supply them.
12. Which representative user tasks and acceptable attention/latency tradeoffs should M4 define and M6 freeze for comparison? Proposed default: completion and errors before speed; validate attention proxies with human users.

Any accepted answer must document why simpler alternatives were insufficient and state what needs to be measured.

## 18. Research-to-requirement map

This document **synthesizes** the research themes in the baseline proposal; it is not a systematic literature review, comparative product benchmark, or claim that one paper proves the combined architecture. Research leads below were not revalidated during this documentation merge. Repository paths are the only cited references; record source/version validation with future repository evidence before comparative or standards-conformance claims.

| Research theme | Usable insight | ZenoUI choice | Limit |
| --- | --- | --- | --- |
| Reactive relational state (Riffle) | Reactive relational/incremental state | Fine-grained dependencies and measured invalidation | Does not prove a whole browser database is optimal |
| Temporal effects (Willow) | Temporal type/effect and dependency analysis | Explicit scheduling, cancellation and lifecycle metadata | Static checker is future research, not delivered |
| Asynchronous FRP testing (PropRatt) | Asynchronous FRP property testing with temporal logic | Generated event schedules, minimized counterexamples | Model correctness and test coverage remain scoped |
| Formal MVC UI specifications | Formal MVC UI specifications and tests | Typed application/presentation models | Does not imply full browser proof |
| Invariant-aware replicated state | Convergence is not application invariant safety | Authority-aware synchronization and CAS | No automatic invariants from CRDT convergence |
| Modular reactive synthesis | Modular reactive synthesis specifications | Small independent contracts and incremental milestones | Evidence is scoped to its study |
| Reactive specification review | Spec quality/validity review | Human-readable positive and negative scenarios | A verified implementation can meet a bad specification |
| Example-driven reactive UI synthesis (ReDemon UI) | Reactive UI synthesis from examples | Optional LLM proposal and independent checking loop | Early research, not production qualification |
| Server-driven islands (Livewire) | Shared-state race warning in an established framework | Adversarial consistency benchmark | Confirm version and comparable semantics when testing |
| Small interaction workloads (7GUIs) | Small UI interaction benchmark portfolio | Counter/form/table plus additional cases | Not a complete usability benchmark |
| Accessibility and responsiveness (WCAG 2.2, INP) | Accessibility and responsiveness standards | Explicit browser/manual acceptance | Standards conformance needs actual evaluation |

**Interpretation:** The best supported immediate combination is *formal behavior models + versioned synchronization + fine-grained reactivity + asynchronous model/property testing*. ZenoUI's separate **Presentation Core** and neurosymbolic optimization workflow are architectural proposals that require their own validation.

## 19. Mental shortcuts

- **Core decides truth; presentation decides communication; shell performs effects.**
- **A disabled control is not authorization.** Recheck at the committing boundary.
- **Old responses do not get to rewrite newer reality.** Version both server commit and client reconciliation.
- **Recompute dependencies, not the whole world.** Publish a coherent snapshot.
- **Time is an input, not a hidden side effect.** Model cancellation, revisions and lifecycle explicitly.
- **A passing proof checks a specification, not its adequacy.** Human examples and counterexamples matter.
- **Neural agents propose; deterministic checks judge.** UX quality still needs people and measurement.
