# Temporal ladder contract

`zal/ladder-temporal/1` uses `ordered-events-over-integers/1`. It is an
experimental presentation and inert lowering profile with authority none.
It adds no temporal evaluator, model checker or proof engine.

A trace binder selects one ordered observation stream. A finite word contains
1 through N events, with 1 <= N <= 256. Evaluation starts at position zero.
One event records declared pre-state, input, outcome and post-state
observations. Preserve authoritative order, rejections, retries and repeated
values. Events are not seconds. A prime reads after this event; next advances
to the following event. Check zero-operation initialization separately rather
than inventing an event.

| Form | Finite meaning at position i |
| --- | --- |
| `always(P)`, `□(P)` | P holds at every remaining event, including now |
| `eventually(P)`, `◇(P)` | P holds now or at some later event in this word |
| `next(P)`, `○(P)` | The next event exists and satisfies P; false at the endpoint |
| `until(P,Q)`, `((P) U (Q))` | Q must occur; P holds strictly before its witness |
| `never(P)` | `always(not P)` |
| `within(k,P)`, `◇[0,k] (P)` | P holds at an offset from 0 through k inclusive |

`k` is a literal integer between 0 and 255 and must be less than N in finite
mode. No clamping occurs. The bounded macro expands by W0(P)=P and
W(k+1)(P)=P or next Wk(P). Expansion and native export retain separate budgets.
A grouped formula is the general operand; symbolic unary aliases also accept
an atomic operand when complete parsing yields one meaning.

The macro correspondence follows induction on k. Its base is the current
position. Its step adds the current witness or advances one existing event to
a witness at offsets 0 through k. Strong next excludes unavailable positions.
This establishes the stated finite endpoint convention; it supplies no machine
membership, real-time bound or liveness theorem. Explicit negation is retained
in temporal counterexample tasks. At the endpoint, not-next-P differs from
next-not-P.

Every statement includes its source revision, event schema, projection,
authoritative order, model and environment, initialization/transition/
continuity obligations, mode, horizon, assumptions, and versioned observation
and definition meanings. Every exported rung carries that context. Scope
identity changes when any of these dependencies change. Event atoms read the
active position; trace-wide facts are captured once. Request/action variables
are rigid across time. Nested trace binders and quantifiers inside a temporal
operand are refused. Fixed finite object binders outside time expand exactly;
unbounded object domains remain unsupported lowering obligations.

Registered readings declare their temporal mode and may restrict the finite
horizon. Both rendering and parsing enforce that applicability. A finite
withdrawal demonstration says that completion occurs before this trace ends;
the unbounded reading retains the separate eventual-progress obligation.

Eventually, until and bounded response require an explicit assumptions
declaration. An unconditional finite statement can declare “provided true”.
A named assumption must appear as the visible premise of the trace property;
its complete versioned definition stays attached. `provided A, B` is material
implication, with no extra causal meaning.

Lowering emits an inert `.zeno` claim for the existing Rust parser and temporal
constructors. Its scalar observation layout declares Boolean and integer
fields. Checked i128 arithmetic remains in the native relation; no Python
calculation replaces it. A leading zero addition makes a compound scalar
unambiguous to the existing relational parser, preserving its value and
arithmetic definedness while consuming native evaluation work. Unsupported
observation mappings and fragments produce readable obligations, not a weaker
claim. The lowering subject also binds the exact renderer, templates and lowering
source. A native parser/export pass qualifies the generated native syntax;
it does not qualify Python controlled-language admission for authority.

Finite-domain conjunctions and disjunctions are balanced while retaining the
declared member order. Lowering checks the expanded native tree against depth
256 and 4,096 nodes, plus conservative operation and intermediate-string bounds
within the native export envelope of 1,000,000 operations and 16 MiB. These
producer bounds may refuse a claim that a native exporter could handle; named
refusals preserve that distinction. Native parsing, elaboration and export
still check independently. Diagnostic fixture failures return named results.

A finite replay can yield Satisfied, Counterexample with a zero-based step,
or Indeterminate. Missing predicates/observations, undefined arithmetic,
empty/oversize words and exhausted budgets stay indeterminate. A replay judges
one exact word. Finite universal checking must include every length 1 through
N under exactly encoded restrictions; export alone performs no such check.

Unbounded mode is distinct and has no final event. It remains a Lean proof
obligation until the exact proposition, model restrictions, fairness,
toolchain and allowed axioms are kernel checked. A named unbounded run assumption that cannot be represented exactly blocks
lowering; substituting a stipulated finite Boolean cannot encode fairness.
A finite pending request
fails “complete before this trace ends”; it does not establish an infinite
freeze. Stipulating a trace-wide assumption for a fixture proves no fairness.

State induction or complete finite-state reachability can establish supported
state invariants after initialization and preservation checks. An arbitrary
temporal formula cannot be submitted as a state lemma. A history monitor
needs its own initialization, update and observation correspondence. Invariants
alone do not establish progress. Refinement carrying next or within-k also
needs the same event count and order; hiding/stuttering and liveness require
separate correspondence and progress arguments.

The [worked examples](EXAMPLES.md) cover loss absence, alarm-safe withdrawal
response and UI discoverability. The [withdrawal pilot](WITHDRAWAL_PILOT.md)
binds the withdrawal statements to the withdrawal-queue contract through a
checked observation adapter. Its bounded response is finite checked over every
tick history through length 12, and its unbounded response is kernel checked
by Lean under the stated fairness premise. The vault and interface
interpretations remain proposals awaiting owner confirmation.
An abstract UI invariant or observation table is no browser or accessibility
result. Machine membership and concrete refinement remain obligations.
