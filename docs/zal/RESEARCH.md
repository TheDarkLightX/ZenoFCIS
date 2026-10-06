# ZAL research, decisions and provenance

Research date: 2026-10-06. This document explains the design choices behind
`zal/finite-fsm/1`; it is not a proof or usability evaluation. The
[specification](SPECIFICATION.md) and [grammar](GRAMMAR.md) define the delivered
profile. Broader ideas below remain research until separately implemented and
qualified against the actual factory.

## Primary sources and the narrower design adopted

| Source | Supported lesson | ZAL design inference and limit |
| --- | --- | --- |
| [Attempto interpretation rules](https://attempto.ifi.uzh.ch/site/docs/ace_interpretationrules.html) | Controlled English has explicit interpretation rules; its reading can differ from ordinary English. | Use fixed, deterministic templates and expose the canonical reading. A unique parse does not establish the user's intent. |
| [Kaljurand, 2009, ACE paraphrasing](https://ceur-ws.org/Vol-448/paper8.pdf) | An intermediate representation can support paraphrases that reparse to a structurally equivalent representation in a supported fragment. | Require round trips for both rendered surfaces. LLM explanations are not normative paraphrases; tests are not a universal proof of the renderer. |
| [Lamport, PlusCal tutorial, labels and translation](https://lamport.azurewebsites.net/tla/tutorial/session6.html) | Labels specify the atomic steps used by the translated model. | One event-response decision is explicitly atomic; no scheduler or hidden intermediate write is inferred. No TLA+ backend is implemented here. |
| [Alloy 6 documentation](https://alloytools.org/alloy6.html) | Analysis has explicit finite scopes and distinguishes bounded from complete temporal checking. | Exhaust the small declared decision domain and finite reachable graph. This is scoped finite safety evidence, not unbounded data, liveness or implementation verification. No Alloy backend is used. |
| [W3C SCXML Recommendation](https://www.w3.org/TR/scxml/) | Hierarchy and parallel statecharts require defined transition selection, ordering and run-to-completion behavior. | Start with a flat FSM, disjoint enabled rules and a visible default. Hierarchy, internal event loops and concurrent-region scheduling are unsupported. |
| [Maoz, Ringert and Rumpe, 2011, semantic model differences](https://www.se-rwth.de/publications/Summarizing-Semantic-Model-Differences.pdf) | Concrete distinguishing witnesses make semantic differences inspectable; summarization can reduce witness overload. | Show changed objects and input/initial-state witnesses. Equal finite decision observations do not establish equal resource, delivery or publication behavior. |
| [Amershi et al., 2019, human–AI interaction guidelines](https://www.microsoft.com/en-us/research/publication/guidelines-for-human-ai-interaction/) | Communicate capability and uncertainty, support correction, and give people control. | Separate proposal, evidence and agreement; make exact-revision rejection and acceptance explicit. These guidelines do not validate ZAL comprehension. |
| [W3C WCAG 2.2](https://www.w3.org/TR/WCAG22/) | Keyboard access, visible focus, non-color-only meaning and controllable motion matter. | Pair graph interactions with text/table controls and preserve focus/drafts. Browser smoke checks are not a full WCAG conformance audit. |

The semantic relation is a union of enabled guards conjoined with their declared
outcomes, plus the explicit unmatched-input default. A conjunction of
guard-implies-outcome formulas would leave the all-guards-false case
underconstrained. The implementation therefore has a total explicit default
and reports overlap rather than inheriting the factory's first-match ordering.

State invariants, fresh environment observations, human requirements and
implementation correspondence are different obligations. Finite exploration
does not authenticate a fact named `authorized`. Future arithmetic needs its
own width, overflow, evaluation-order and refusal semantics; Boolean
canonicalization here cannot justify reordering eager, trapping factory code.

## Small explicit knowledge representation and reasoning

[Davis, Shrobe and Szolovits, 1993](https://groups.csail.mit.edu/medg/ftp/psz/k-rep.html)
describe representation as a choice of concepts, sanctioned reasoning and human
expression, distinguishing those semantics from the implementing data structure.
The design inference for ZAL is that its typed declarations, Boolean formulas,
transition semantics and bounded reasoning already supply the useful small
knowledge-representation-and-reasoning layer. A graph database would not by
itself improve its meaning or assurance. A general ontology framework, rule
engine or solver is unnecessary for exact topic lookup, guard evaluation and
the present domain of at most 1,024 raw input tuples.

The improvement is to expose that existing representation clearly: one local
help resolver, canonical paired examples, input-specific truth trees and scoped
advisories. There are fewer semantic translations to maintain than with a second
reasoning engine. A future solver may help a separately qualified larger profile
or independent encoding; that would require explicit result/unknown handling,
replay and source-bound assumptions. No such backend is implemented or required
for current help.

The [OWL 2 Primer](https://www.w3.org/TR/owl2-primer/) distinguishes missing
information under open-world reasoning from closed-world falsity. ZAL imports
neither as an implicit default for observations: its evaluation contract requires
every declared Boolean value explicitly. Missing/extra keys or non-Boolean
values refuse. Negation is ordinary Boolean complement. The unmatched-input
default applies only after a valid complete input enables no rule.

Keep seven claims separate: valid syntax/typing; paired-surface correspondence;
declared finite model properties; correctness of the checker implementation;
qualified factory correspondence; adequacy to human intent; and authenticity
of observations. Tests or evidence for one must not be relabeled as another.
Reachability computes the entire finite control-state closure, so its modeled
state-invariant scope is not restricted to the interactive 64-step replay cap.
It does not establish liveness, fairness or unbounded data behavior, and there
is no machine-checked general theorem of this Python implementation.

The seed's intent gap is deliberately inspectable. It has no application
invariants. Weakening `authorized` to `true` on approval passes the declared
finite checks, while semantic diff exposes unauthorized approval under the
assumed input `pending / approve / authorized=false`. A named Boolean does not
authenticate authorization, and consistency cannot recover an unstated
requirement. Independently chosen expected/forbidden scenarios remain essential.
The [README walkthrough](README.md#check-scope-and-the-intent-boundary) reproduces
this without a provider or workspace acceptance.

The checker now emits separate advisories for no application invariants,
guards never true at their declared source, sources unreachable from the initial
state, and invariants true on every declared state. A guard incompatible with
its source is not necessarily globally contradictory. A reachable invariant
pass is not itself a tautology warning; the latter ranges over the entire
declared state domain. These diagnostics do not implement temporal vacuity
analysis, add transition requirements or change acceptance failures.

## Contextual help without an LLM

The following primary sources motivate interface choices, not claims that ZAL
implements their verification engines or inherits their usability results:

| Source | Design inference and limit |
| --- | --- |
| [Leino and Wüstholz, The Dafny Integrated Development Environment, 2014](https://arxiv.org/html/1404.6602v1) | Associate computed help with typed objects and distinguish a selected evaluation context. ZAL uses its explicit finite input rather than Dafny's solver machinery; the historical IDE paper does not certify this implementation. |
| [Microsoft LSP 3.17 hover definition](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.17/language/hover.md) | Separate help content from a position-based adapter. ZAL currently resolves canonical typed IDs; it has no language server or arbitrary editor-source hover. A future adapter needs buffer/version, span and occurrence-role bindings. |
| [W3C, Understanding WCAG 2.2 SC 1.4.13](https://www.w3.org/WAI/WCAG22/Understanding/content-on-hover-or-focus.html) | Hover/focus content must meet applicable dismissible, hoverable and persistent conditions. Keep visible keyboard/click/tap access to help; a title attribute or DOM-double test cannot establish accessibility conformance. |

The terminal's `?`, `man`, `help TOPIC` and `legend TOPIC`, MCP's `zal_help`
and the browser all call the same deterministic resolver. Explicit aliases and
typed IDs avoid guessing across overlapping namespaces. Canonical paired
examples come from model/formula renderers; semantic descriptions remain reviewed
metadata, not meaning automatically inferred from syntax. The `zal/help/1`
payload records revision, profile and renderer source identity. Read-only help
does not become candidate review or alter its displayed-review binding.

Four questions stay distinct: what a construct means; what the current object
declares; what happens for one fully supplied input; and what a checker report
establishes. `translate` is a canonical projection. Selected-object `why` is
static help. Input-specific `why`/`zal_explain` evaluates the typed AST and shows
source/event matches, recursive guard truth, the actual enabled rule/default,
outcome and frame. A disabled selected rule alone does not imply rejection.
The supplied facts are assumptions, and a truth tree is no external causal
history. None of these paths calls a provider or repairs an invalid draft.

Pointer/focus previews and the persistent help dialog address canonical objects
and explicit grammar topics. English `and`/`is` also appear as fixed grammar
text; visible text alone cannot identify an operator occurrence. Exact source
hover, parser-derived completion and generalized causal explanations are deferred.
Late replies must be rejected on request generation, selection and revision,
including edits and dismissal. Native keyboard, pointer, touch and assistive-
technology validation remains separate from deterministic UI state checks.

## Existing terminal harnesses and the common bridge

Official [Codex MCP documentation](https://learn.chatgpt.com/docs/extend/mcp)
and [Claude Code MCP documentation](https://code.claude.com/docs/en/mcp) describe
local stdio servers. The smallest shared seam is a provider-independent
read/check/help/explain/compare/trace/propose service over one workspace. The existing
harness owns its conversation and tool permissions; ZAL owns deterministic
meaning, evidence and candidate validation. The human terminal owns the intended
accept/reject workflow. Actual tool names and setup are in the [README](README.md).

The [Codex app-server protocol](https://learn.chatgpt.com/docs/app-server) is a
separate custom-client route: the companion owns threads, turns and UI. It is
not evidence of arbitrary custom panes inside stock Codex, or of attaching to
an existing stock CLI conversation. Pin the generating binary's schema instead
of assuming current online fields exist in an older installation.

Claude's [CLI reference](https://code.claude.com/docs/en/cli-reference) documents
per-session MCP configuration. Claude support here is a documented adapter
contract only. Its installed interface and live behavior remain unvalidated.
Optional skills, hooks and provider-specific push channels are not part of this
prototype. A normal tool catalog does not imply unsolicited message injection
or a continuously synchronized conversation.

The shared JSON store is an intentionally small cooperative same-user workflow.
Advisory locks and exact revision checks are useful consistency mechanisms, but
cannot stop a same-user agent with direct filesystem access from bypassing
review. An enforceable malicious-agent boundary would require independently
protected storage and reviewer authorization. No such guarantee is made here.

## Supplied prototype provenance

Two supplied packages informed the meaning-first layout, paired rule displays,
legend, visible frames and counterexample review. They remain separate designs
and evidence sources. Original archives were retained outside the repository;
their executable code and recorded results were not imported as this profile's
validation. These SHA-256 digests identify the inspected archive bytes:

| Retained archive | Design inspected | SHA-256 |
| --- | --- | --- |
| `meaning-workbench-original.zip` | `zal-review/`: ZAL/0 inventory model, bounded checked-i64 fields and inputs, Accept/Reject/CommittedFailure and effect data. Its specification inspects factory revision `f69a5653d0ca3c4ed722c8bec0a383b4dc5d738b`. | `06b54f7fd9423b33326f7e096c609a8df33c96bb4e934951fd23ccff11489f2f` |
| `rule-workbench-original.zip` | `zeno-abstraction-language/`: ZAL 0.1 RetryGate, checked numerical/Boolean state, committed failure/effect names and captured offline fixture checks; inspected factory revision `3245e62dc0b9fed5e87f4e55e9465fb44a03381c`. | `6643c14c9ab012808027c37fc2139b272f0926ab73d70eef30a416f315657b6d` |

Their grammar, Unicode aliases, numeric evaluation strategies, domain counts,
generated target files and captured browser reports do not apply to this
Boolean control-FSM dialect. In particular, selected-rule reference evaluation
does not establish correspondence to the factory's eager graph evaluator.
Their documentation reports browser-network restrictions; an injected page
or bridged fetch is different evidence from a native browser-to-loopback run.
Do not combine their test totals with this prototype's tests.

This implementation targets the factory source at
`3245e62dc0b9fed5e87f4e55e9465fb44a03381c`. The
[source map](../ZAL_SOURCE_MAP.md) distinguishes implemented F1–F7 seams from
the planned V2.2 G1–G14 work. A roadmap version is not an installed capability.

## Integration evidence and open work

Evidence is deliberately split by boundary; historical runs do not certify the
final tree. Final test/CI and independent-review results must identify their
exact source state separately.

- Domain/transport/shared-store unit tests exercise deterministic fixtures and
  simulated tool-to-terminal review. They do not invoke a provider.
- The registered `zal-dialogue` ATDD scenario combines those Python tests,
  deterministic Node UI checks, the pinned CLI build and actual sample-domain
  factory replay. It also runs through the repository's `--all` acceptance
  route. Registration alone is not a final-source passing result.
- `test_workspace.cjs` exercises the actual UI script with minimal DOM doubles
  and domain-generated fixtures. Its checks are not native browser rendering,
  layout, accessibility or end-to-end HTTP evidence.
  Help/explanation coverage must include alias/typed lookup, full-context
  refusal, whole-model outcome agreement, truth-tree agreement, state and
  displayed-review preservation, and stale-response handling. Consult the
  exact final test output rather than treating an earlier count as current.
- Historical Codex 0.146.0 app-server initialization and read-only thread
  creation succeeded. One configured-model turn failed; an available default
  model then completed a structured explanation with unchanged revision and
  no observed tool items. This is one explanation, not every lifecycle feature
  or a stock-CLI integration pass.
- Cloud Codex 0.159.2 is a separate executable/schema target. Its per-invocation
  MCP configuration was inspected without inference. Do not transfer a 0.146.0
  live result to 0.159.2 merely because method names match.
- An approved cloud stock-CLI retry used a synthetic workspace and temporary
  permissions for model reading and one exact draft proposal. Runtime startup
  failed with a read-only-filesystem error; no model turn or MCP tool execution
  was observed, no proposal was staged, and the workspace/configuration stayed
  unchanged. This is an infrastructure blocker, not a live interoperability pass.
- The historical stock Codex CLI discovered `zal_read`, but its permission
  layer cancelled both calls. No read/check/proposal completed. Discovery is
  not successful interoperability; permission failure is distinct from domain
  or adapter failure.
- Claude was neither invoked nor registered. No live Claude result is claimed.
- The browser can use the terminal/MCP store with `ui --workspace PATH`.
  Without that option it remains an isolated in-memory demo. Sharing a domain
  module alone is not evidence of sharing state; cross-adapter tests must use
  the explicit shared-workspace mode.
- Final native browser QA in the cloud was blocked before page creation by a
  local socket restriction; the supported browser also refused loopback access.
  Deterministic DOM-state checks do not substitute for fresh rendered, keyboard,
  HTTP or CSP evidence. Earlier screenshots are historical checkpoint views.
- Factory qualification compares every reference tuple with the actual
  generated application's Authority. That is tested correspondence for
  class/reason/successor and empty deliveries, not a theorem, resource-aware
  equivalence receipt, publication authority or application adoption.
  The current lowering requires at least two states for the unchanged
  scaffold's rejected-genesis check; a valid one-state language model is refused
  by that narrower factory-qualified subset.

Before broader claims: complete exact-source repository checks and independent
review; establish an approved end-to-end stock-harness tool exchange; test
comprehension with deliberately ambiguous requests and held-out traces; and
qualify each wider profile and its lowering separately. No measured usability
benefit, general synthesis, optimization convergence, unbounded liveness,
production security or deployment readiness is established by this design.
