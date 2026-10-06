# `zal/finite-fsm/1` grammar and limits

This is the implemented Boolean control-FSM dialect in
[`behavior.py`](../../integrations/zal/behavior.py), not the earlier numerical
ZAL/0 or ZAL 0.1 designs. See the [specification](SPECIFICATION.md) for semantics
and the [README](README.md) for commands. The parser and live legend share the
statement registry; parsing never invokes an LLM.

## Lexical rules

- ASCII source only. Names match `[a-z][a-z0-9_]{0,31}` and are case-sensitive.
- Reserved names: `none`, `true`, `false`, `state`, `not`, `and`, `or`, `is`,
  `default`. This also applies to machine, rule, invariant and reason names.
- Blank lines and whole-line `#` comments are ignored. No inline comments.
- Leading/trailing line whitespace is ignored. Each declaration occupies one
  line. Literal internal spaces, capitalization and punctuation below matter.
  In particular, the atomic-step and frame declarations share one line.
- The eight required declarations occur exactly once each, in any order. Rules,
  invariants and unresolved obligations are optional. Names may be referenced
  before declaration; all references are resolved before admission.
- Comma-separated name lists trim whitespace and must be nonempty and distinct,
  except that `context none;` denotes zero context facts. Rule and invariant
  IDs must be distinct from one another. Typed state/event/context namespaces
  may reuse a spelling. Reasons are names on outcomes, not separate declarations.

## Complete paired example

Each fenced block below is one complete model, with the same canonical revision.
The invariant is a state-domain property, not a claim about authorization truth.
It is true on every declared state and therefore produces the advisory
`invariant-domain-tautology`; it adds no restriction to that state domain.

```zal
zal 1;
machine approval;
states approved, pending;
events approve;
context authorized;
initial pending;
step atomic; frame control_only;
default reject not_enabled unchanged;
rule approve_order: pending + approve [authorized] -> accept approved;
rule refuse_order: pending + approve [!authorized] -> reject unauthorized unchanged;
invariant declared_state: state == approved | state == pending;
```

```text
ZAL version 1.
The machine is approval.
The states are approved, pending.
The events are approve.
The context observations are authorized.
Initially the state is pending.
Each step is atomic. Only the control state may change.
Otherwise reject because not_enabled and keep the state unchanged.
Rule approve_order: when the state is pending and the event is approve, if authorized, move to approved.
Rule refuse_order: when the state is pending and the event is approve, if (not authorized), reject because unauthorized and keep the state unchanged.
Invariant declared_state: (state is approved or state is pending).
```

## Statement forms

The following are templates: substitute declared names for uppercase slots.
`GUARD` is a Boolean formula; `STATE_FORMULA` cannot mention context facts.

| Symbolic | Controlled English |
| --- | --- |
| `zal 1;` | `ZAL version 1.` |
| `machine NAME;` | `The machine is NAME.` |
| `states NAME, NAME;` | `The states are NAME, NAME.` |
| `events NAME, NAME;` | `The events are NAME, NAME.` |
| `context NAME, NAME;` | `The context observations are NAME, NAME.` |
| `context none;` | `The context observations are none.` |
| `initial STATE;` | `Initially the state is STATE.` |
| `step atomic; frame control_only;` | `Each step is atomic. Only the control state may change.` |
| `default reject REASON unchanged;` | `Otherwise reject because REASON and keep the state unchanged.` |
| `rule ID: STATE + EVENT [GUARD] -> accept TARGET;` | `Rule ID: when the state is STATE and the event is EVENT, if GUARD, move to TARGET.` |
| `rule ID: STATE + EVENT [GUARD] -> reject REASON unchanged;` | `Rule ID: when the state is STATE and the event is EVENT, if GUARD, reject because REASON and keep the state unchanged.` |
| `invariant ID: STATE_FORMULA;` | `Invariant ID: STATE_FORMULA.` |
| `unresolved KIND: TEXT;` | `Unresolved KIND: TEXT.` |

`KIND` is exactly `effects`, `liveness`, `fairness`, `arithmetic`, or
`concurrency`. `TEXT` is 1–200 characters matching
`[a-zA-Z0-9_ ,!?()/-]`. The terminator is separate. Such an obligation is retained
and yields `unsupported` if the executable checks otherwise pass; it has no
executable interpretation. Unsupported requirements written as ordinary rules
are refused, not silently converted into obligations.

## Boolean formulas

Canonical symbolic formula grammar, with whitespace permitted between tokens:

```ebnf
formula     = disjunction ;
disjunction = conjunction, { "|", conjunction } ;
conjunction = unary, { "&", unary } ;
unary       = "!", unary | "(", formula, ")" | "true" | "false"
            | CONTEXT_NAME | "state", "==", STATE_NAME ;
```

The English parser additionally maps the exact words `not`, `and`, `or`, `is`
to `!`, `&`, `|`, `==`. It also accepts the symbolic operators; the canonical
English renderer uses words. Precedence is negation, conjunction, disjunction,
from strongest to weakest. Parentheses override grouping. There are no Unicode
operator aliases, implication, temporal operators, arithmetic, function calls,
general equality or post-state reads. For example, `authorized == true` is
invalid; use `authorized`.

Guards can read supplied Boolean facts and the pre-state label. Invariants can
use `true`, `false`, state predicates and Boolean connectives only. All operands
are total in this profile. Context is supplied afresh for every step; its name
does not authenticate the observation.

## Built-in help and canonical translation

These commands belong to the review terminal, not the model grammar:

| Command | Meaning |
| --- | --- |
| `?`, `man` | Current topic index. |
| `help` | Review command overview and topic index. |
| `? TOPIC`, `man TOPIC`, `help TOPIC`, `legend TOPIC` | The same built-in topic or exact-object help. |
| `legend` | All paired canonical statements and operator spellings. |
| `translate english`, `translate symbolic` | Complete current canonical model in the chosen surface. |
| `why` | Help for the currently selected object, without assumed input values. |
| `why STATE EVENT FACT=yes ...` | One exact decision explanation; supply every declared fact once. |

`yes`/`true` and `no`/`false` are Boolean values in terminal input commands.
MCP/HTTP input uses actual JSON Booleans. Missing or extra keys and non-Boolean
values are refused; omitted observations do not default to false. An empty
context is valid only for a model declaring `context none;`.

Help accepts explicit aliases such as `machine`/`name`, `state`/`states`,
`event`/`events`, `context`/`facts`, `not`/`!`, `and`/`&`, `or`/`|` and `is`/`==`.
Model topics use `state:NAME`, `event:NAME`, `context:NAME`, `rule:ID` or
`invariant:ID`, together with `initial`, `default` and `frame`. These exact
object IDs take precedence. Other bare names resolve only when unique and not
colliding with a grammar topic. Use a typed ID to disambiguate such names, or
`grammar:TOPIC` to select only a grammar meaning. A typed ID after `grammar:`
is refused. There is no fuzzy matching or natural-language fallback.

The role matters: `and`/`is` in fixed English statement text are not necessarily
predicate operators. `+` separates a source state and event; `->` separates a
guard from an outcome. Neither is an arithmetic or implication operator here.
An `accept` outcome changes the modeled state; the review terminal's `accept`
command records human agreement to a displayed candidate. These are separate.

Help entries pair canonical fragments from the typed model or typed Boolean
examples. Fragments require their declared context and are not standalone
models. The response binds profile, model revision and renderer source identity
using `zal/help/1`. It describes canonical objects, not unparsed editor text or
literal source spans. Canonical translation, help and concrete `why` evaluation
are deterministic and need no LLM. They add no executable constructs.

## Decision and evidence scope

An enabled rule matches the pre-state, event and true guard. Exactly one enabled
rule determines Accept with its fixed target or Reject with unchanged state and
a reason. Multiple enabled rules produce a determinism counterexample, even
when they would return the same result. Rule order is never priority. No enabled
rule produces the explicitly declared default rejection.

Checks enumerate every raw state/event/Boolean-context tuple. Reachability starts
at the one declared initial state and explores the complete finite graph;
reachable invariant counterexamples retain a shortest BFS trace. Raw Accept
targets are checked separately because an external factory caller can supply
an unreachable declared pre-state. A raw Reject does not commit a new state.
A trace is a supplied input sequence replayed from initialization, not a claim
that these events happened in an application.
The 64-input trace cap does not bound the checker's complete finite reachable
graph exploration. Concrete explanations also describe supplied assumptions,
not the cause of a real external event.

`pass-with-scope`, `counterexample`, and `unsupported` are distinct results.
Malformed or over-limit models are refused before a success report. With no
declared invariants, no application invariant has been established and no
business requirement is inferred. The no-invariant advisory makes that
absence explicit. Impossible-at-source guards, unreachable sources and
invariants true throughout the declared state domain have separate advisories;
they do not change the executable grammar or become new rejection conditions.
Comparison uses the full common declared domain and separately compares initialization;
changed domains return `domain-changed`. The compared observations exclude Step
usage, budget refusals, deliveries and publication identity.

## Admission and operation bounds

| Item | Limit |
| --- | --- |
| Input source and each canonical surface | 64 KiB ASCII each |
| Nonblank, noncomment statement lines | 80 |
| Name | 1–32 characters |
| States / events / context facts | 1–8 / 1–8 / 0–4 |
| Rules / invariants / unresolved obligations | 0–32 / 0–8 / 0–8 |
| Raw input tuples | At most 8 × 8 × 16 = 1,024 |
| Formula input | 2,048 characters; 512 lexical tokens |
| Formula parser recursion | At most 64 nested atom/parenthesis levels |
| Canonical formula | 128 nodes; depth 16; 2,048 characters in each surface |
| Trace | 0–64 inputs, each with the exact declared context keys and Boolean values |
| Retained discussion | 256 events |
| Model act object IDs / explanation | 32 / 8,192 characters |
| Built-in help topic | 128 characters |
| Concrete decision-explanation output | 512 KiB canonical JSON |

Formula size is also checked after canonicalization. Limits refuse work rather
than substitute sampling or a partial pass. Factory expression/shape limits are
independent: a valid model at these limits is not guaranteed to lower. The
current factory-qualified subset also requires at least two states for the
unchanged scaffold's independent rejected-genesis check. This does not remove
one-state models from the language or its finite checker.

## Canonical identity

Both parsers construct the same frozen typed model. State/event/context names,
rules by ID, invariants by ID and unresolved obligations are sorted. Nested
conjunctions/disjunctions are flattened, deduplicated and sorted. This does not
perform arbitrary logical simplification: `authorized & true` need not have the
same revision as `authorized`. It grants no right to reorder trapping arithmetic
in a future dialect.

The revision is SHA-256 of sorted-key, compact ASCII JSON from `Model.data()`,
including the profile, declarations, initial state, default, formulas, outcomes,
obligations, atomicity and frame. Comments, formatting and source positions are
not retained. A rename can change both revision and typed IDs; graph layout
does not. The checker source identity is separate from the model revision.
Hashes are identities, not proofs, signatures, human attestations or factory
publication capabilities.
