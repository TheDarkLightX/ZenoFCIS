# ZAL: a behavior dialogue layer for ZenoFCIS (research prototype)

Status: design proposal and additive research prototype, 2026-10-06.
Executable profiles: `zal/finite-fsm/1` and, from 2.6, `zal/finite-fsm/2`.
The actual source and capability map is [ZAL_SOURCE_MAP.md](../ZAL_SOURCE_MAP.md).

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
search and human review. No study has
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
outcomes. No new requirement semantics are implied by the diagnostic. Profile 2
adds transition requirements, and its seed fails that weakening (see below).

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

## Why-not, what-if, narratives and the construct guide

These read-only explanations (`explain.py`, `constructs.py`) use the admitted
typed model only: no provider, eval or authority. Each result names its schema
and the exact revision and checker identity.

`zal/why-not/1` takes one complete input and a wanted outcome: any of `class`,
`state`, `reason` and `rule`, all declared. It enumerates the stated scope:
`context` keeps the state and event and tries every context valuation, and
`input` tries every declared input. Inputs with overlapping enabled rules are
skipped and counted. Minimality is the fewest changed inputs, where the state,
the event and each fact count one each. The result lists every minimal input
(capped at 32, with the full count), its changes as typed IDs, the deciding
rule and its canonical lines, and whether its source state is reachable from
the initial state. With nothing found, it says so for the scope and reports
how many inputs outside a `context` scope would give the outcome. Rules whose
own outcome matches are listed with their source and event matches and their
guard's value at the given input.

`zal/what-if/1` applies a `change` of `state`, `event` and named facts.
Undeclared facts and empty changes are refused. Every requested change is
reported, including changes that keep a value. The result checks that each
one reached the second input, then compares both whole-model decisions and,
rule by rule, source/event matches, guard value and enablement. It also names
the changed facts each guard reads.

`zal/narrative/1` tells a determinism, invariant or invariant-preservation
counterexample, or an assumed trace, as numbered sentences. Each sentence
records its registered template, its facts and a witness path per fact. Its
English and symbolic text are that template filled from those facts. Before
returning, the narrator re-derives every sentence. It checks the witness
paths, the template text, the declared initial state, each step's decision
and continuity, the enabled rules, the accepting rule, the invariant's
falsity and the final state. Any mismatch refuses the whole narrative.
Obligations without templates are refused by name; the structured witness
remains authoritative.

`zal/help/2` (`zal_guide`, terminal `guide`) resolves topics exactly as
`zal/help/1` and assembles each answer from the construct registry. Forms for
statements with a parser pattern are derived from `grammar_patterns`. The
registry covers every statement kind, Boolean node, rule outcome, rule syntax
mark and unsupported kind. A drift test requires that coverage, and requires
the live parser to accept every registry form as that construct. `zal/help/1`
output is unchanged.

Profile 2 has its own schemas, from `explain2.py`: `zal/why-not/2`,
`zal/what-if/2` and `zal/narrative/2`. An input adds `variables`, and context
values include integer inputs. Why-not counts the state, the event, each
variable, each fact and each integer input as one change each. A wanted
outcome may name `class` `Trap`, the trapped `variable` and post-state
`variables`. Each witness reports configuration reachability and
admissibility. What-if applies variable and integer changes and compares rule
updates, requirement values and admissibility. Its outcome may be a Trap.
Narratives cover determinism, range, invariant, invariant-preservation,
requirement and property counterexamples:
- A range story ends with the update that would leave its range, and a
  requirement story names the failing step. When the requirement is an
  implication, it says that the condition holds and the conclusion does not.
- Safety, next and within-k stories name the triggering step and the steps
  where the goal fails.
- A liveness story is a lasso: the prefix, the cycle that repeats forever,
  where each fairness declaration holds on the cycle, and the trigger whose
  goal never happens, or the step at which an until breaks first.

The verifier re-derives continuity through the cycle, the cycle's return to
its first configuration, each fairness position, the trigger and every goal
judgment from the model. `explain.narrate_check` and `narrate_trace` dispatch
to profile 2 by model type. `run.py story`, `why-not` and `what-if` expose
both profiles for a model file.

## Ladder statements in the tools

`ladder_tools.py` exposes the shipped ladder examples and optional
`ZAL_LADDER_DIR` statements. Each statement revision is the SHA-256 of
`zal/ladder-statement/1`, a NUL byte and the statement's canonical JSON. Every
render, parse, same, lower and explain call rereads the file, recomputes the
revision and refuses a mismatch before parsing. Files must be regular, at most
256 KiB and named by a lowercase stem; at most 64 statements are listed.
Rendering is `ladder_text` and `render`; parsing is `ladder.parse`, with
uniqueness counts recomputed from the same Earley grammar and cross-checked
against admission. `same` compares expanded normal forms and the temporal scope
identity, or the declared vocabulary for `zal/ladder/0`. Different normal forms
are not a proof of different meaning. Lowering is the existing inert
`temporal_lowering.lower`. All of these results have authority none.

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
The checker identity version `zal/checker/3` hashes `behavior.py`,
`behavior2.py`, `workflow.py`, `factory.py`, `shared.py`, `terminal.py`,
`mcp.py`, `workspace.py`, `server.py`, `workspace.html`, `explain.py`, `explain2.py`,
`constructs.py`, `ladder_tools.py`, `ladder.py`, `ladder_en.json`,
`temporal_lowering.py`, the withdrawal pilot's checker `withdrawal_pilot.py`
with its Lean templates `withdrawal_pilot.lean` and
`withdrawal_pilot_refutation.lean`, and the finite types, typed-machine and
effects modules `adt.py`, `typed2.py`, `effects.py` and `effects_guide.py`.

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
and `zal_trace`, the explanation tools `zal_why_not`, `zal_what_if`,
`zal_narrate` and `zal_guide`, and the revision-bound `zal_ladder_*` tools;
there is no acceptance tool. The harness must refresh the
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

## Profile `zal/finite-fsm/2`: integers, requirements and temporal properties

Profile 2 is a separate executable profile in `behavior2.py`. The header
statement selects it: `zal 2;` or `ZAL version 2.`. Profile 1 sources keep
their parser, revision, verdicts and renderings; `behavior.py` is unchanged.
The grammar is in [GRAMMAR.md](GRAMMAR.md#profile-zalfinite-fsm2). Both
surfaces build one frozen typed model, with the same round-trip laws and a
revision over its canonical AST, which now names profile 2.

### Configurations and steps

A configuration is the control state plus the value of every declared
variable. A variable is a stored integer with an inclusive range and an
initial value; the initial state and those values form the single initial
configuration. Context facts remain fresh Booleans. An input is a fresh
integer from its inclusive range on every step. Neither is authenticated.

A rule is enabled exactly as in profile 1: its source, event and guard. The
same determinism, no-precedence and explicit-default rules apply. An Accept
moves to its target and applies its updates: `x = TERM`, `x += TERM` or
`x -= TERM`, where TERM is a literal, a variable or an input. Updates read
the pre-state and apply together; a rule updates a variable at most once. A
Reject keeps the whole configuration. There are still no effects.

Formulas compute exact integers. Every value they can read is bounded, so no
formula can overflow. Only an update can leave a declared range. Such a step
is refused: `decide` returns a named refusal, and `check` reports a `range`
counterexample. A step never wraps around.

### Formulas

Guards read facts, inputs, the pre-state label and variables. Invariants read
the control state and variables only. Step formulas, used by requirements,
fairness declarations and property operands, read the whole step: the
pre-state, `event == E`, facts and inputs, `accepted`, and the post-state as
`state'` and `x'` ("after this event"). Comparisons are `==`, `!=`, `<`,
`<=`, `>` and `>=` over sums and differences of literals, variables and
inputs.

Implication `A => B` ("if A, then B") is material: a false premise makes it
true. Equivalence `A <=> B` reads "A if and only if B". Chains of either
need parentheses. Conjunction and disjunction are canonically sorted as in
profile 1; arithmetic keeps its written order.

### Checks

`check` enumerates every declared input tuple and stops at the first
counterexample, in this order:

1. determinism over every declared configuration, event and context;
2. range traps reachable from the initial configuration, with a shortest trace;
3. reachable invariants, with a shortest trace;
4. range traps from admissible configurations;
5. invariant preservation from admissible configurations;
6. transition requirements on every admissible step, reporting a reachable
   step with its shortest trace before an unreachable one;
7. temporal properties.

An admissible configuration is a declared configuration that satisfies every
invariant: the states a lowered contract can hold, since its genesis and
commit laws enforce the invariants. Profile 1 checks every raw Accept target
instead. Profile 2 invariants read variables, so preservation is relative to
admissible pre-states, and together with reachable invariants it makes the
invariant inductive. A requirement counterexample carries the step, whether
it is reachable, the trace and the requirement's truth tree.

The `order2.zal` seed states "approve only if authorized" as a requirement:
an accepted `approve` that reaches `approved` needs `authorized`. Weakening
its approval guard to `true` now fails with a reachable requirement
counterexample at `pending / approve / authorized=false`, which closes the
intent gap that profile 1 can only expose by comparison.

### Temporal properties

An execution is an infinite sequence of steps from the initial
configuration. Every configuration has a successor for every event, because
the default rejection keeps it, so every execution is infinite and always has
a next step. Positions are steps, called events as in ZAL-T. A prime reads
after this event, and next advances one event. Events are not seconds.

The operators have the ZAL-T meanings and its intuitive-rung English:

| Symbolic | English | Meaning at a step |
| --- | --- | --- |
| `always(P)` | `always (P)` | P holds now and at every later step. |
| `never(P)` | `never (P)` | P holds at no step from now on. |
| `next(P)` | `at the next event, which must exist, (P)` | Strong next. |
| `within(k, P)` | `now or within the next k events, (P)` | P holds at an offset 0 through k; `within(0, P)` means now. |
| `eventually(P)` | `eventually (P)` | P holds now or later. |
| `until(P, Q)` | `(P) holds until (Q) happens, and that must happen` | Strong until: Q occurs, possibly now, and P holds at every step before it. |

The supported shapes are `always(P)`, `never(P)`, `always(P => T)`,
`always(T)` and `T` from the initial configuration, where `T` is `next`,
`within`, `eventually` or `until` over step formulas. Other shapes, such as
nested temporal operators, are refused by name. The bound k is a literal from
0 to 64.

Safety, `next` and `within` properties are checked exactly over every
infinite execution, by reachability and a bounded search. A counterexample is
a shortest trace: for `within`, the triggering step and k further steps that
all miss the goal.

`eventually` and `until` are liveness. A model with such a property and no
fairness declaration is refused: "liveness needs a fairness declaration". A
fairness declaration `fairness ID: infinitely often (P);` assumes that P holds
at infinitely many steps; liveness is checked only over executions that
satisfy every declaration. Any event can be offered at any time, so weak
fairness on an event `e` is `infinitely often (event == e)`. The check is
exact over the finite configuration graph, using its strongly connected
components. A counterexample is a fair lasso: a prefix and a nonempty cycle
that returns to its start and contains a step satisfying each fairness
declaration. A finite violation of `until` sits inside the prefix of such a
lasso. Fairness restricts liveness only; safety and bounded properties range
over every execution. If no reachable execution is fair, every liveness
property holds vacuously and the `no-fair-execution` advisory says so.

These results are finite checked over the declared machine. They are not
proofs, and they say nothing about a contract or a real trace until a
separate binding establishes that correspondence.

`trace_properties` judges one supplied trace of at most 64 events with the
ZAL-T finite-word meaning. Strong next is false at the last event; eventually
and until need their witness before the trace ends; `within(k, P)` needs k
less than the trace length and is refused for that trace otherwise; fairness
does not apply to a finite word. A pending obligation at the end of a trace
is a finite counterexample, not evidence of an infinite one.

### Advisories, explanation and comparison

Profile 2 keeps `guard-never-true-at-source`, `unreachable-source` and
`invariant-domain-tautology`, over configurations. It adds:

- `no-application-claims`: no invariants, requirements or properties;
- `requirement-premise-never-true`: an implication-shaped requirement whose
  premise is false on every admissible step;
- `property-trigger-never-reached`: a response property whose premise is
  false on every reachable step;
- `no-fair-execution`: liveness holds only vacuously.

`explain` evaluates one exact input: state, variables, event and every context
value. It returns each rule's matches, guard truth tree with term values,
enabled status and updates (before, after, in range), the outcome or a
`Trap` outcome naming the variable and value, each requirement's value and
tree, and each invariant before and after. Temporal properties are judged over
executions, so a single-step explanation does not evaluate them.
`semantic_diff` compares class, control state, variables, reason and traps on
the common declared domain, plus the initial configuration. A changed
variable range is a changed domain.

### Limits

| Item | Limit |
| --- | --- |
| Statements | 96 |
| States / events / facts / inputs / variables | 1-8 / 1-8 / 0-4 / 0-2 / 0-4 |
| Integer literals and range bounds | -32767 to 32767 |
| Values in one range | 1-64 |
| Configurations / input tuples | 4,096 / 16,384 |
| Rules / invariants / requirements / fairness / properties / unresolved | 32 / 8 / 8 / 4 / 8 / 8 |
| Formula input | 4,096 characters; 1,024 tokens |
| Canonical formula | 128 nodes; depth 16; 4,096 characters per surface |
| `within` bound | 0-64 |
| Replayed or judged trace | 64 events |

A domain above these limits is refused before any check; there is no
sampling or partial pass.

### Lowering

`factory.lower` lowers profile 2 to `project.zeno` and `v2/policy.json`.
Variables become integer state fields with their declared ranges; inputs
become integer context fields. Updates become case post-state expressions.
Invariants become `StateInvariant` laws on commit and genesis. Each
requirement becomes an `on accept` law (kind `AuthoritySubjectRecipient`) and
an `on reject` law (kind `RejectNoAuthority`), with the outcome class folded
in and, for a rejection, post-state reads replaced by the unchanged
pre-state; a side that folds to true is omitted. The law kinds are review
categories, not inferred meaning. Temporal properties, fairness declarations
and unresolved obligations have no contract form and refuse as
`unsupported lowering: temporal property ID`, `unsupported lowering: fairness
assumption ID` or `unsupported lowering: unresolved KIND`.

The reference table covers every input tuple from an admissible
configuration. Pre-states that break an invariant are excluded, because the
contract's genesis and commit laws keep every state it holds admissible. The
correspondence is a tested correspondence, not a theorem: qualification runs
the actual generated Authority on that table.

## Finite types, typed machines and effect signatures

Three 2.6 profiles are separate modules with their own profile IDs:
`zal/finite-types/1` (`adt.py`), `zal/typed-fsm/1` (`typed2.py`) and
`zal/effects/1` (`effects.py`, with `effects_guide.py` for help and
explanations). Profiles 1 and 2 keep their parsers, revisions, verdicts and
renderings. [Types and effects](EFFECTS.md) has the forms, the checks and the
evidence; the grammar is in [GRAMMAR.md](GRAMMAR.md#profiles-zalfinite-types1-zaleffects1-and-zaltyped-fsm1).

A type is an alias, a record or a variant over bounded integers, Booleans,
byte-capped and exact bytes and bounded lists. Recursion is refused, so every
type is finite. Each type's count of values is exact: records multiply,
variants add. A typed machine elaborates exactly to a `zal/finite-fsm/2`
machine. A name of a non-integer type holds the index of its value in the
type's canonical enumeration, which must have at most 64 values, and is used
only with `==`, `!=` and assignment. Verdicts are profile 2 verdicts on the
elaborated machine, with values decoded in witnesses.

An effect signature source declares resources, capabilities, answer shapes
and their frames, the port list, types, laws and one operation and footprint
per port. `effects.check` runs the named checks `record_schema`,
`port_coverage`, `frames`, `footprints`, `laws`, `v2_mapping` and
`record_kinds`. `effects.lower` emits the `zeno-fcis/shell-effects/1` record's
canonical bytes. The shipped source `examples/shell_effects.zal` lowers to the
shell registry's bytes and identity; this is a tested correspondence against
the built library, not a proof. Laws are named and checked for existence and
use, not against any handler. These profiles have no review-terminal, MCP or
browser surface yet; `run.py` selects them by header.

## Future profiles and release gates

`unresolved effects`, `liveness`, `fairness`, `arithmetic` and `concurrency`
preserve readable obligations and block realization. They are not executed.
Safety, eventuality and timed guarantees must remain distinct. External context
truth and environment assumptions need explicit responsibility and realizability
review. Hierarchical/concurrent state machines need an independently defined
step/event scheduling semantics. Integer arithmetic must preserve actual widths,
eager evaluation, traps and inactive-expression behavior before any lowering.

Profile 2 adds bounded integers whose only trap is leaving a declared range;
it does not qualify a wider checked-integer application language, effects or
concurrency.

A later release gate should qualify profiles and lowering on actual source, add
revision-bound evidence replay and test human comprehension against canonical
paraphrases/witnesses. A further gate may add solver-assisted synthesis or
proof backends only behind explicit scopes, budgets, counterexample replay,
independent checks and human agreement. No convergence, optimality, intent
certification, unbounded liveness or neurosymbolic synthesis guarantee is made
by the current prototype.
