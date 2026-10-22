# Composed-domain composition

`zeno-fcis contract check-composition` reads a third kind of manifest,
`zeno-fcis/checked-composition/3`. It checks several contracts that act
together on every request, in a fixed order, and it proves properties of the
whole without enumerating every combination of their states.

The code for this profile is LLM-generated, like the rest of ZenoFCIS. Its
verdicts come from ZenoFCIS's own Rust checker, which runs each component's
library Authority. A separately written Python reference and the controls
described below test it.

## When to use it

The `/1` and `/2` profiles run one selected component per command, which reads
its peers' state through views. The composed-domain profile runs every
component on every request:

| | `/1` and `/2` | `/3`, composed domain |
|---|---|---|
| Who decides a request | One selected leaf | Every participating component, in merge order |
| How components connect | A consumer reads a provider's view | A component's inputs are expressions over the request and earlier components' state |
| A component that owns no state | Refused | Accepted, as a read-only component |
| Aggregate contract | Generated and validated | None; `--aggregate` and `--write-aggregate` are refused |
| What is checked | Each leaf, each interface implication, each group | `local`, `assume` and `couple` obligations, planned before any evaluation |

## The joint step

One request moves the whole composition by one joint step. All components
decide from the same pre-state:

1. Components run in merge order, the order the manifest lists them.
2. A component whose `participates` formula is false skips this request.
3. A participating component's command and context fields are the values of
   its bindings. A binding reads request fields and earlier components'
   pre-state, never a provisional successor from this step.
4. The component's Authority decides. An Accept stays provisional.
5. A Reject ends the step, and nothing changes. So does any refusal:
   - the Authority refuses (an input or successor outside its field domains, a
     violated law, an arithmetic or meter refusal);
   - a binding has no value, or its value lies outside the field's domain;
   - a read-only component's committing decision changes its state.
6. The first CommittedFailure commits the provisional successors up to and
   including its own, then ends the step. Later components do not run.
7. If every participant accepts, every successor commits.

This follows the composed-domain oracle retained in
`verification/kernel-laws/src/oracle/domain` in four rules: merge order,
provisional Accepts, a Reject that changes nothing, and a CommittedFailure
that commits its prefix. It differs in three ways:

- that oracle has no participation formula;
- it reports a refusal as an error, where `/3` ends the step with no change;
- it routes one component's outputs to later components in the same step
  through ports, where a `/3` binding reads earlier components' pre-state.

## The manifest

```json
{
  "schema": "zeno-fcis/checked-composition/3",
  "checker": "zeno-fcis/composed-domain-checker/1",
  "name": "guarded-budget",
  "request": [
    {"name": "amount", "domain": {"kind": "int", "min": 0, "max": 3}},
    {"name": "authorized", "domain": {"kind": "bool"}}
  ],
  "components": [
    {
      "name": "guard", "path": "guard",
      "project_sha256": "…", "policy_sha256": "…", "program_sha256": "…",
      "owns": [],
      "participates": "true",
      "bind": {"mark": "0", "request": "0", "authorized": "authorized"},
      "assume": "true",
      "invariant": "mark == 0"
    },
    {
      "name": "budget", "path": "budget",
      "project_sha256": "…", "policy_sha256": "…", "program_sha256": "…",
      "owns": ["budget.remaining"],
      "participates": "true",
      "bind": {"amount": "amount", "authorized": "true"},
      "assume": "authorized == 1",
      "invariant": "remaining <= 3"
    }
  ],
  "couplings": []
}
```

The complete files are under
`crates/zeno-fcis-contract/tests/fixtures/checked-composition-3/guarded/`.

Each component names its contract directory (`project.zeno` and
`v2/policy.json`) and pins three SHA-256 values: the two files and the bound
program's identity, as a `/1` leaf does. The checker compiles each contract
through the ordinary contract route and binds its library Authority.

For each component, in merge order:

| Key | Meaning |
|---|---|
| `name` | A lowercase identifier, unique in the manifest |
| `path` | The contract directory, relative to the manifest |
| `owns` | Every state field as `component.field`, or `[]` for a read-only component |
| `participates` | When the component takes part; reads request fields and earlier components' state |
| `bind` | One expression per command and context field; for a read-only component, also one per state field |
| `assume` | A formula over the component's own command and context fields, by bare name |
| `invariant` | A formula over the component's own state fields, by bare name |

At the top level:

- `request`: the request fields, each `bool` or `int` with `min` and `max`;
- `couplings`: each with a `name`, its `members` in merge order, and an
  `invariant` over the members' state fields, written `component.field`;
- `limits` (optional): `max_obligation` and `max_total`, which may only lower
  the compiled ceilings of 2^22 and 2^25.

A scalar command or context root is one field named `command` or `context`.
Formulas use the contract rules' expression language
([contract rules](CONTRACT_RULES.md)). A number used as a boolean is true
exactly when it is 1. A formula with no value, such as a division by zero,
counts as false. A binding is a rule expression, as a decision rule is: no
multiplication or division, and checked 128-bit arithmetic, so a binding
that overflows has no value. The library's program evaluator computes each
binding, reading every field it names as the number it is (a boolean as 0 or
1), so a component whose fields are all boolean needs no integer declaration;
the library's law evaluator decides every other formula.

The checker refuses, before anything is evaluated:

- duplicate component names, and two components with one directory;
- a binding or `participates` formula that reads a later component, the
  component itself, or a component that owns no state;
- unknown fields, components or names;
- an assumption or invariant that reads outside its own fields;
- a coupling formula that reads a field of a non-member;
- two components owning one field, a component owning another's field, or one
  owning only part of its state;
- a formula longer than 4,096 bytes, with more than 1,024 expression nodes,
  or nested more than 128 deep (a flat chain such as `a && b && …` counts
  one level per operator);
- unknown keys, repeated keys, and non-canonical values: names that are not
  lowercase identifiers, formulas with leading or trailing space, pins that are
  not lowercase hexadecimal, couplings out of name order, limits above the
  ceilings.

## The obligations

Each obligation is an exhaustive finite check that never enumerates the global
product.

| Obligation | What it checks | Size |
|---|---|---|
| `local`, per component | The declared genesis lies in the field domains, the Authority commits it (its genesis laws hold), and it satisfies the invariant. From every state that satisfies the invariant, on every input that satisfies the assumption, every committing decision keeps the invariant. A read-only component has no genesis of its own, and every committing decision must leave its state unchanged. Decisions the Authority refuses commit nothing: they are counted and the first is reported, but they are not failures | The component's local domain: state × command × context |
| `assume`, per component | For every value of the request fields its participation and bindings read, and every state of the components they read that satisfies those components' invariants: where it participates, every binding has a value inside its field's domain and the delivered command and context satisfy its assumption. For a read-only component, the delivered state also lies in its domains and satisfies its invariant | Those request fields × those components' state domains |
| `couple`, per coupling | The coupling holds at the members' genesis. From every state of the members and of the components they read that satisfies every invariant and the coupling, on every request, the joint step simulated over the members in merge order keeps the coupling | Those states × the members' request fields |

A component outside a coupling is never enumerated. If it rejects or refuses,
nothing changes. If its contract has a CommittedFailure case, the step may end
there. Wherever such a component sits between two members, the `couple` check
also requires the coupling to hold at that point. This is an
over-approximation: it may report a violation that no real execution reaches,
never the reverse.

## The planner: over budget is refused, never sampled

The checker computes every obligation's exact size first, as a 128-bit
integer, and refuses a size that overflows. It also plans the tables the
check builds: each component's decisions over its local domain and each
formula's values over the domains it reads, one evaluation per cell. If one
obligation or table exceeds `max_obligation`, or the obligations' sum or the
tables' sum exceeds `max_total`, the verdict is `refused`.
The refusal names the first offending obligation or table and the factor by which it
exceeds the limit. Nothing is evaluated: the result records `evaluated: 0`,
no Authority decision and no formula value.

The report also gives the monolithic size, global states times requests, as
an exact decimal. That is the space the checker did not enumerate.

For example, the ten-component fixture `ladder-10/composition.json` is proved
with 70,508 evaluated rows, against a monolithic product of
172,179,890,733,018,470,400 pairs. Adding one coupling over three components
with large state, in `ladder-10/wide.json`, plans an obligation of
86,782,189,536 rows, 20,691 times the limit, and the check is refused with
nothing evaluated.

## Verdicts

- `proved`: every obligation holds. Exit 0.
- `violated`: an obligation fails. The result gives the first witness: the
  obligation, the states and request, and what failed. A `local` witness is a
  counterexample to induction. Exit 1.
- `refused`: the planner refused the plan. Exit 1.

A manifest the checker refuses at validation exits 1 with a refusal and no
result. A plan within budget is never reported as passed on a sample.

The JSON result (`zeno-fcis/composed-domain-result/2`) lists every obligation
with its size, status, `evaluated` count and `admitted` count. A proved
obligation evaluates every row of its planned space, including rows a
premise makes vacuous; a violated one stops at its first witness. The
result also gives the planned table values and the evaluations actually
made: Authority decisions and formula values. It also gives
the SHA-256 of each component's decision table, the checker's source-bound
identity, the claim, the trusted base and the non-claims.
These work counters exclude genesis publication.

## Components that own no state

**`/1` and `/2` refuse them.** A leaf must list at least one owned state
field: the manifest refuses `owns: []` with "must list each owned state field
once, and at least one". The `/2` hierarchy reads the same leaves.

**A contract with no state at all has no route to an Authority.** The
contract compiler refuses an empty state record ("needs fields, variants, a
declared range or a leaf binding", at `project.zeno type 100`). It refuses a
scalar state root too ("the state root, type 100, must be a record"). The
library's record encoding admits an empty record, but no contract reaches the
library that way. So `/3` does not offer a zero-field form.

**`/3` accepts read-only components.** A component with `owns: []` keeps its
state fields, and the manifest binds each one to earlier components'
pre-state or to a constant. Its committing decisions must leave that state
unchanged: the `local` obligation checks this, and a change is `violated`. In
the joint step, a change is a refusal. This matches the "memo" of the coupled
core applications, which owns no state and whose view reads another
component's state.

An authorization guard is the simplest case. Its contract has one state field
with a single value, so binding it to `0` makes it read-only. It accepts an
authorized request and rejects any other. See
`checked-composition-3/contracts/guard` and `guarded/composition.json`, and
`guarded/v1-guard.json` for the same guard refused as a `/1` leaf.

## What a proved verdict means

> If every obligation holds, every reachable global state satisfies every
> component invariant and coupling invariant, and every input delivered to a
> component, with a read-only component's delivered state, is inside its
> domains and satisfies its assumption and invariant.

The argument is an induction over executions from genesis:

- **Domains.** Every reachable state lies inside the field domains. Each
  owning component's genesis does, by `local`. A step commits only successors
  the Authority decided, and the Authority refuses a successor outside its
  field domains; a refusal changes nothing.
- **Base.** `local` shows each owning component's genesis satisfies its
  invariant; `couple` shows each coupling holds at genesis.
- **Step.** Take a reachable state where every invariant and coupling holds,
  and any request.
  - Each participating component's bindings read only the request and earlier
    components' state, and those components satisfy their invariants. So
    `assume` shows the delivered inputs lie in the domains and satisfy the
    assumption, and a read-only component's delivered state also satisfies
    its invariant.
  - A Reject or refusal changes nothing.
  - Otherwise a prefix of the participants commits: every one, or those up to
    the first CommittedFailure. Each committed owning component decided from a
    state satisfying its invariant, on inputs satisfying its assumption, so
    `local` shows its successor satisfies its invariant. A read-only component
    changes nothing.
  - For a coupling, the members that commit are either all members as the
    `couple` simulation runs them, or the members before the point where a
    CommittedFailure ends the step. The `couple` check covers both: a member's
    own CommittedFailure ends the simulation there, and an outside component's
    possible failure between members is checked at that point.
  - So every invariant and coupling holds after the step.

This argument is stated here, not machine-checked. The claim also rests on
the exhaustive finite checks of each obligation. `DOMAIN_TRUSTED_BASE` in
`crates/zeno-fcis-contract/src/checked_composition.rs` lists what the claim
rests on:

- the induction argument above;
- the planner's sizes and the exhaustive enumeration;
- each component's decisions, made once by its bound Authority on its
  complete local domain;
- each formula's table, decided by the library's law evaluator;
- the joint step's implementation;
- the coupling check's treatment of components outside the coupling.

It does not claim:

- anything machine-checked;
- contextual refinement, an adoption path, or an aggregate contract;
- anything about storage atomicity, concurrency, delivery, effects or outbox;
- any runtime authority;
- anything outside the finite declared domains;
- any liveness, fairness or trace property.

## Receipts

`--receipt FILE` writes the result's canonical receipt,
`zeno-fcis/composed-domain-receipt/2`, for any verdict. `--replay FILE`
runs every check again and requires the same receipt byte for byte. A changed
receipt, or a source that changed since, is refused.

## The joint step as a library API

`zeno_fcis_contract::checked_composition::JointComposition` builds a
composition from in-memory sources: the manifest text and each component's
`project.zeno` and `v2/policy.json`, in merge order. It binds every component
Authority once and decides every input of each component's local domain.

- `joint_step(global_state, request)` returns the outcome (`Accept`,
  `Reject`, `Refused` with its class, or `CommittedFailure`) and the committed
  successor. The successor equals the pre-state after a Reject or refusal.
- `state_layout`, `request_layout` and `genesis` give the layout: the global
  state is each owning component's state fields, in merge order, then field
  order.
- `reachable(max_pairs)` lists the joint step on every reachable pair from
  genesis, or refuses over the limit.

It is pure and deterministic and checks no obligation; `check_domain` does.
The constructor plans every table's size before it builds anything, and
refuses one over the obligation ceiling. `reachable` enumerates requests as
it uses them and stops at `max_pairs`, so a wide request field is refused,
never collected. `check-composition --joint-table MAX_PAIRS` prints the same
reachable table.

## How it is tested

- Unit tests cover every validation refusal, every joint-step rule, the
  planner's sizes on hand-computed cases, and the behaviours above: a refused
  decision is not a failure, and a failure between coupling members can end
  the step.
- `tools/composition_domain_reference.py` is an independent Python reference.
  It takes component decisions from `tools/contract_judges_reference.py`, and
  implements the joint step, a breadth-first oracle over the reachable global
  product, and the three obligations by direct enumeration.
- `tools/composition_domain_soundness.py` generates random compositions of
  the declared core-component instances and the fixture contracts. For each,
  it compares the checker's verdict with the oracle (a `proved` verdict with an
  oracle violation would be unsound). It also compares the joint step on every
  reachable pair. Its self-test shows that the run fails when either
  comparison is stubbed. A wrong joint step planted in the checker's table (a
  CommittedFailure that commits only its own successor) makes the comparison
  report differences; the wrong step is never part of a build.

These are tests over the generated cases and the fixtures, not a proof about
other compositions.

Scalar coordinates in current composition JSON results and joint tables use
`safe-number-or-decimal-string/1`; see the [integer report encoding](CONTRACT_JUDGES.md#integer-values-in-json-reports).
Fresh composition receipts use `/2` envelopes for the versioned result. Existing
receipt files are unchanged; replay requires the current exact schema and result.
