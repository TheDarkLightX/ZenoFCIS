# ZAL: a behavior dialogue layer for a future ZenoFCIS 2.3/2.4

Status: design proposal and additive research prototype, 2026-10-06.
Executable profile: `zal/finite-fsm/1`. Repository baseline:
`3245e62dc0b9fed5e87f4e55e9465fb44a03381c` on
`agent/v21-factory-20261004`. This is an unreleased development branch.
The actual source and capability map is [ZAL_SOURCE_MAP.md](../ZAL_SOURCE_MAP.md).
V2.2 G1–G14 remain planned at this inspected revision.

## Purpose and research basis

ZAL lets a person inspect and discuss one precise software behavior through
controlled English, formulas and a state machine, with optional LLM assistance.
Language help, canonical translation and concrete explanations need no LLM.
Its deliverable is a reviewed meaning and source-bound evidence that a
particular realization matches a
particular profile. Free conversation helps discover intent; deterministic
parsing defines the candidate meaning. An LLM cannot resolve ambiguity silently.

The design follows controlled-language work on explicit interpretation and
canonical paraphrase, atomic transition-system modeling, finite counterexample
search and human review. [RESEARCH.md](RESEARCH.md) records primary sources,
design inferences, limits and the two provided prototype packages. No study has
established that ZAL improves comprehension or synthesis quality.

## One model, two surfaces

Both parsers build an owned, frozen typed model containing a profile, machine
name, states, events, fresh context observations, initial state, default
rejection, transitions, invariants and unresolved obligations. Rule order does
not grant precedence. Collections are canonically sorted; total Boolean
conjunction/disjunction are flattened, deduplicated and sorted. This licenses
no reordering of trapping arithmetic in a future profile.

For every admitted model M:

```
parse_symbolic(render_symbolic(M)) = M
parse_english(render_english(M)) = M
revision(M) = SHA256(canonical_owned_AST(M))
```

Round trips preserve meaning and stable declared IDs, not comments, whitespace
or original source locations. The digest identifies a value; it is not proof,
authentication, publication identity or a claim of human intent.
Malformed source, unknown references, extra statements, Unicode lookalike
operators and arbitrary prose are refused. The user must clarify them explicitly.

Example of the same transition:

```
rule approve_order: pending + approve [authorized] -> accept approved;
```

```
Rule approve_order: when the state is pending and the event is approve,
if authorized, move to approved.
```

The actual controlled-English parser requires that rule on one line. Ordinary
conversation is not accepted by either parser. The CLI/UI legend uses the active
statement registry and paired canonical model renderings. Help explanations are
explicit metadata, checked against the interpreter; syntax alone does not derive
semantic prose or establish its correctness.

## Normative finite-FSM semantics

State is one control label. Events are enumerated input labels. Context facts
are Boolean observations supplied afresh on each invocation; they are neither
stored state nor certified external truth. Every guard reads the same immutable
pre-state and supplied context. Every evaluation requires exactly the declared
context keys with Boolean values. Missing information is refused, not treated
as false; negation is Boolean complement, not negation-as-failure. Each
transition is atomic. Accept changes only the control state to the declared
fixed target; Reject keeps it unchanged and
retains a declared reason. There are no effects or other implicit writes.

Let s be the pre-state, e the event and c the context. A rule is enabled iff its
source equals s, its event equals e, and its guard evaluates true. Exactly one
enabled rule yields its declared outcome. More than one produces a determinism
counterexample; no first-match priority is inferred. No enabled rule yields the
visible default Reject with unchanged state and its declared reason.

Thus the relation is the disjunction of each enabled guard **and** its poststate,
plus the explicit unmatched-input default. A guard⇒post formula can explain a
rule, but the conjunction of implications is not the transition relation:
all-false guards do not permit an arbitrary successor.

The initial set is one declared state. State invariants use only state predicates.
Complete finite exploration computes reachable states from that initial state
under every event and Boolean context valuation, including noncommitting
rejections. Reachable invariant violations include a shortest BFS trace. The
checker separately checks all raw Accept targets, because a factory caller may
supply a declared but unreachable pre-state. A raw Reject may preserve an
invariant-violating pre-state without committing it. These claims have separate
report fields. This explores the entire finite reachable graph, not only traces
of length 64. Subject to the model's fresh-input semantics and the correctness
of the checker implementation, the state-invariant result covers every execution
length in that finite model. It establishes neither liveness nor fairness.

Limits: ASCII source and both canonical projections ≤64 KiB; ≤80 statements;
names ≤32 characters; 1–8 states, 1–8 events, 0–4 context facts, ≤32 rules,
≤8 invariants, ≤8 unresolved obligations. This gives ≤1024 raw input tuples.
Formula input ≤2048 characters/512 lexical tokens; canonical formula ≤128 nodes,
depth 16 and 2048 characters in either rendering. Trace ≤64 steps; discussion
≤256 retained events. Limits refuse work; they never report partial success.

## Checks and comparison

Reports separate admission/typing, nonempty initial state, determinism, coverage
with the explicit default, reachable invariants and raw commit invariants.
`pass-with-scope` means those exact finite obligations completed. Reports that
complete the finite checks expose `invariant_count`; zero produces a
`no-application-invariants` advisory and no
application invariant claim. Additional advisories are:

- `guard-never-true-at-source`: no complete Boolean context makes a rule's guard
  true at its declared source. This need not be a contradiction at other states.
- `unreachable-source`: a rule's source is outside the initial reachable set,
  independently of whether its guard is satisfiable there.
- `invariant-domain-tautology`: the invariant holds on every declared state,
  not merely every reachable state, and excludes none of that domain.

These are informative warnings, not new acceptance obligations or general
temporal-vacuity analysis. `counterexample` identifies the failed obligation
and concrete input/trace. `unsupported` preserves explicit
future obligations. Factory qualification and realization are separate fields.

The `order.zal` seed declares no application invariants. Replacing its approval
guard `authorized` with `true` still passes these finite consistency checks;
comparison exposes `pending / approve / authorized=false` changing from
default Reject to Accept/`approved`. This demonstrates an intent gap, not a
proof that authorization is enforced. The current state-only invariant grammar
does not express a general transition requirement relating observations and
outcomes. No new requirement semantics are implied by the diagnostic.

Comparison enumerates the complete common declared domain and compares business
class, successor control label and rejection reason, plus the initial state as a
zero-step observation. It also lists changed rule/initial/default/invariant/
obligation objects. A changed domain receives `domain-changed`; no equivalence
is inferred across it. Equal decision observations do not establish equal
requirements, Step usage, budget refusals, publication identity or deliveries.
Invariant/obligation metadata still requires review even when decisions match.

## Deterministic help and concrete explanations

`help_topic` resolves the active grammar and exact current-model objects. The
terminal commands `?`, `man`, `help TOPIC` and focused `legend TOPIC`, the MCP
tool `zal_help` and the browser help endpoint share this resolver. Topic aliases
are explicit, including `not`/`!`, `and`/`&`, `or`/`|`, `is`/`==` and
`context`/`facts`; there is no approximate or free-prose interpretation.
Exact typed IDs and built-in object IDs `initial`, `default` and `frame` take
precedence. Other bare names resolve to an object only when unique and not
colliding with a grammar topic; otherwise they are refused as ambiguous.
`grammar:TOPIC` selects only a grammar meaning, never a typed object ID.
Unknown or stale object IDs are refused.

The `zal/help/1` response contains `profile`, full model `revision`, `renderer`
source identity, resolved `topic`, entry kind/title/meaning, paired canonical
`entries`, assumption/scope `notes` and `available_topics`. Adapter responses
bind `renderer` to the source closure used by the checker. Changing help code
can change that source identity without changing the canonical model revision.
Help grants `authority: none`. A fragment example is not a complete model;
whole-model translation comes from `Model.render` and preserves its revision.

Static help states what an object means without claiming it is enabled. It does
not read arbitrary editor text, resolve source spans or inherit context from a
trace. The English `and` and `is` in fixed statement text have different roles
from operators inside predicates. Symbolic `+` is a state/event separator and
`->` introduces an outcome, not arithmetic or a general implication operator.
The modeled Accept outcome is distinct from the terminal's human `accept`
gesture. A future source-position adapter would need buffer-version and
occurrence/role bindings beyond the current model digest.

`why` without inputs shows selected-object help. `why STATE EVENT fact=yes`
evaluates one fully specified hypothetical input; `zal_explain` and the browser
explanation endpoint use the same typed-AST evaluation. They require a declared
state/event and every declared Boolean fact, with no extra keys. The result
includes the exact revision/profile/checker identity, assumed input, each rule's
source/event match, recursive guard truth tree, enabled status, actual whole-model
outcome, default use/reason, frame and absence of effects. The readable terminal
view summarizes the tree; structured output retains its nodes. Invalid inputs
and overlapping enabled rules refuse rather than invent a result.

A disabled selected rule does not imply rejection: another rule may be enabled.
Default rejection means no rule is enabled for these inputs. An evaluation
explanation is neither an external causal history nor evidence authenticating
the supplied facts. Help, translation and explanations invoke no provider,
change no model/proposal/selection/agreement, and neither create nor invalidate
the client's successfully displayed candidate binding. Storage adapters may
rewrite snapshots while preserving this semantic state.

## Dialogue, agreement and realization

The client owns the current revision, selected typed ID, pending candidate,
checks and append-only session events. IDs are `state:name`, `event:name`,
`context:name`, `rule:name`, `invariant:name`, plus `default`, `initial`, `frame`.
Selection, questions and traces are revision-bound. Model responses are exact
structured acts: ask, explain, clarify, compare, counterexample or propose.
Every act cites the exact base revision and declared typed object IDs.
Only propose may carry a complete normative surface.

Explanations and questions never mutate the model. A proposal parses anew,
produces canonical paired views, runs independent deterministic checks and
shows changed objects and a distinguishing input. The client validates the
JSON fields, IDs, bounds and base; `agree` is absent from the model schema.

Human acceptance binds both base and candidate revisions, reruns checks against
the current checker source closure and refuses stale or already consumed
candidates. Long-lived clients pin that source closure at process start and
refuse further checking/acceptance if it changes; restart and restage/review
the candidate rather than relabel old evidence. A known counterexample blocks
acceptance in this prototype.
A well-typed unresolved requirement may be accepted as a meaning while its
evidence remains `unsupported` and realization remains `not-run`; lowering
still refuses it. Human agreement, evidence and realization are orthogonal.

The human terminal defaults to controlled-English meaning, selected-rule
context, readable state traces, candidate witnesses and a built-in legend.
Its `accept`/`reject` convenience commands bind the exact candidate and event
cursor previously displayed by that client and refuse intervening changes;
the person does not need to type digests or JSON. Structured output and explicit
full-hash commands remain optional low-level interfaces.

The 2.3 addon adds `show` / `accept-current` for an initial or unchanged model.
It requires successful rendering of that exact model/checker/event cursor and
no pending proposal. The cursor is captured with the displayed snapshot under
the same workspace lock; an intervening event requires another display.
Agreement records bind the candidate revision, canonical English and checker.
The checker identity version `zal/checker/2` hashes `behavior.py`, `workflow.py`,
`factory.py`, `shared.py`, `terminal.py`, `mcp.py` and `workspace.py`.

Reviewed workspace export reruns supported-profile checks and requires the last
human agreement to match the current revision, meaning and checker, with no
pending proposal. Its `zal/reviewed-declarations/1` receipt names the agreement
sequence and every generated file digest. Verification independently regenerates
the declarations, compares the complete file/directory inventory and exact bytes,
and refuses unreadable content or symlinks. This is a cooperative review gate,
not authenticated approval, a proved lowering checker or factory qualification.

Acceptance is a trusted local client event. It is not a cryptographic human
attestation: a malicious process with the same local access can operate the
client. The transport has no acceptance tool; command approval is a separate
protocol and never grants agreement or factory authority.

## Terminal workflow and shared state

The primary workflow uses the human review terminal and a provider-independent
MCP bridge against one existing `zal/shared-workspace/1` file. The bridge exposes
`zal_read`, `zal_check`, `zal_help`, `zal_explain`, `zal_propose`, `zal_compare`
and `zal_trace`; there is no acceptance tool. The harness must refresh the
exact revision before continuing
on changed meaning; the human separately reviews the current candidate. The bridge
does not start inference. Setup and exact commands are in [README.md](README.md).

Cooperative advisory locking and atomic snapshots preserve ordinary consistency
between clients, not isolation against malicious same-user processes. The
optional browser shares this file only when started with `ui --workspace PATH`;
without that option it is an isolated in-memory demo. A browser refresh does not
inject a turn into the coding harness: it must call `zal_read` again. Ordinary
MCP, same-user storage and tool approval do not authenticate a human decision.

## Separate Codex custom-client connection

The companion uses Codex `app-server --listen stdio://` and JSONL requests.
The historical local probe used 0.146.0; cloud 0.159.2 has a separately generated
schema and must be validated independently. It initializes, lists available
models, creates an ephemeral read-only thread using the available default, and
starts a turn with the proposal JSON output schema. It does not alter persisted
account, model configuration or credentials. The default replay uses no model.

The client handles exact turn steering/interruption and explicit human questions.
It refuses execution/approval requests. It buffers notifications while a start
response is unresolved and binds only that response's exact turn and connection
generation. Interrupting a pending start revokes proposal import immediately
and keeps the workspace busy until that start settles. Terminal/disconnected
turns retire questions and do not import partial, stale or malformed output.
Message bounds are 512 KiB/2048 events; pending questions are bounded to 8.

Read-only instructions are a policy, not a physical ban on every possible
read-only tool. Live probes report observed tool items separately. Installed
handshake, mocked lifecycle tests and actual live-model completion are distinct
evidence. This is a local companion, not a custom pane embedded in the desktop.
The earlier stock Codex MCP probe discovered `zal_read` but both calls were
cancelled by its permission layer; no read/check/proposal completed. That is
not a stock-CLI interoperability pass. Claude's bridge setup is documentation-
backed only; no live Claude model was invoked.

## Accessible meaning workbench

The interface leads with selected objects and paired rule cards. A person can
select a state/arrow, ask why, inspect the guard/reason, scrub a recorded trace,
edit a transition into a candidate, inspect a semantic witness, and accept or
reject the exact revision. Full-source editing is secondary and defaults to
controlled English. The equivalent transition table and navigation retain all
declared objects when a graph is difficult to read.

The visible `Help ?` and `Meaning of this object ?` controls open persistent
help. Pointer/focus on declared objects or grammar topics previews local help;
click/tap and keyboard activation do not require hovering. `?` uses the focused
topic or selected object, without intercepting typing in text controls or the
editor. Help dismissal restores invoking focus. The local `Explain next input`
and `Why this recorded step?` controls use explicitly identified input values;
`Ask Codex` remains a separate provider action.

Meaningful status updates preserve selection, keyboard focus, draft text and
scrub position. A delayed candidate check cannot replace a later edit. Help
replies/errors are checked against request generation, revision and selection;
newer requests, edited queries or dismissed dialogs retire older requests.
There is no arbitrary editor-source hover or installed language server.
Legend examples derive from the same canonical model as the parsers. Text labels
accompany semantic colors. Focus is visible, motion defaults off, and reduced-motion
preferences override optional transitions. Narrow layouts stack the panels.
DOM-double regressions check scripted state behavior, not native focus, touch,
layout, screen-reader behavior or WCAG conformance. Fresh cloud browser QA is
blocked by the recorded socket/loopback restrictions; prior screenshots do not
validate these help changes.

## Actual factory integration and trust boundary

Lowering emits `project.zeno`, `v2/policy.json`, a feasible persistent journey,
an exhaustive reference table, a source map and a generated Authority test.
The existing factory requires ordered cases and a literal-true fallback.
ZAL first proves finite disjointness by exhaustive checking, then uses canonical
case order plus explicit default. That is a tested correspondence, not a theorem.

Qualification runs the existing CLI's generate, drift check, advisory review,
and scaffold against the exact repository source. It normalizes only the
temporary application's lockfile offline, then executes every reference tuple
against the actual generated library Authority and the scaffold's tests.
The root Cargo manifest, lockfile, evaluator and production public API remain
unchanged. The current factory-qualified subset requires at least two states
for the unchanged scaffold's independent rejected-genesis check. One-state
models remain supported by the language and finite checker. Factory node/shape
limits may also refuse a valid model; profile admission does not promise every
maximum-size model can lower.

Reference checking cannot manufacture Authority, Publication or equivalence
receipts. Factory review remains advisory. Replacement checking, adoption,
publication, persistence, upgrades and delivery remain the existing separate
routes. This prototype calls no adoption/publication/upgrade/release command.
Its correspondence scope is decisions/reasons/successors and empty deliveries,
not sealed resource usage, budget-refusal or publication-identity equivalence.

## Future profiles and release gates

`unresolved effects`, `liveness`, `fairness`, `arithmetic` and `concurrency`
preserve readable obligations and block realization. They are not executed.
Safety, eventuality and timed guarantees must remain distinct. External context
truth and environment assumptions need explicit responsibility and realizability
review. Hierarchical/concurrent state machines need an independently defined
step/event scheduling semantics. Integer arithmetic must preserve actual widths,
eager evaluation, traps and inactive-expression behavior before any lowering.

The two provided broader reference packages remain distinct designs (checked
i64 state/command inputs, failure/effect data and a RetryGate example). Their
syntax, evaluation strategy and captured check reports are not silently merged
with this Boolean control-FSM profile. Their visual patterns are reused with
provenance in [RESEARCH.md](RESEARCH.md).

A future 2.3 gate should qualify profiles and lowering on actual source, add
revision-bound evidence replay and test human comprehension against canonical
paraphrases/witnesses. A future 2.4 gate may add solver-assisted synthesis or
proof backends only behind explicit scopes, budgets, counterexample replay,
independent checks and human agreement. No convergence, optimality, intent
certification, unbounded liveness or neurosymbolic synthesis guarantee is made
by the current prototype.
