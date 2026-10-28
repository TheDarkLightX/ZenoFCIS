# Experimental finite-local compositional safety checking (2.7 research)

**Status: implementation prototype, not production admission.** This is a separately
executable, zero-package-dependency Node.js checker of a *new, narrow abstract
finite-table model*. It does not parse `.zeno`, invoke the ZenoFCIS Authority,
verify Rust, construct an independently kernel-checked certificate, qualify a
generated application, or authorize installation. Do not interpret its `proved`
verdict as applying to arbitrary programs or the 2.6 release.

**Purpose.** Demonstrate a precise way to prove invariants of many components
without constructing the global Cartesian state table. Once the native
parser/Authority, contract-composition and runtime-refinement bridges are
machine-checked, this can become an acceptance technique rather than a
standalone mathematical model experiment.

## Run

Node.js 22 recommended; no third-party packages, network, Rust or SMT solver.

```sh
node --test experiments/proof-composition-27/test.cjs
node experiments/proof-composition-27/make_examples.cjs /tmp/zfcis-proof-fixtures
node experiments/proof-composition-27/run.cjs /tmp/zfcis-proof-fixtures/chain3.json
node experiments/proof-composition-27/run.cjs /tmp/zfcis-proof-fixtures/pairs25.json
node experiments/proof-composition-27/propose.cjs /tmp/zfcis-proof-fixtures/chain3-unproven.json /tmp/zfcis-proof-fixtures/chain3-candidate.json
node experiments/proof-composition-27/run.cjs /tmp/zfcis-proof-fixtures/chain3-candidate.json
```

The CLI accepts *canonical* UTF-8 JSON only: object keys recursively sorted,
two-space indentation and one final newline. The example producer emits that
format. All model objects have closed field sets. In particular, the canonical
byte check rejects duplicate JSON object keys that standard `JSON.parse`
would otherwise silently collapse. A fresh result carries SHA-256 hashes of
the complete input bytes and the JavaScript checker source, not authenticated
provenance or proof of what runtime was executed.

Exit codes: 0 `proved` (model-only), 1 `violated` (counterexample),
2 `inconclusive` (no checked claim), 65 `refused` (malformed or unsupported
input). All results have `publication_authority:false`.

## Exact semantics and soundness argument

Let `V` be a finite set of named state variables, each with a nonempty finite
integer domain and a declared initial value. An **action** has a fixed read set
`R(a)`, a write set `W(a) ⊆ R(a)`, and a **complete** transition table for
*every* valuation of `R(a)`. A table row either stutters (`next:null`) or
atomically supplies the new values for precisely `W(a)`; all other variables
are unchanged. The next values must lie in the declared domains. This
abstract system has no concurrency inside one action, no effects, and no
undocumented state. The checker enforces table completeness and write closure.

Each invariant `P` has a support `S(P)` and an explicit finite set of
allowed valuations over that support. It asserts exactly that the current
projection is in that set; it has no access to state outside its support.

For each invariant, the checker verifies its declared genesis valuation is
allowed. For each action whose writes intersect its support, it enumerates
**every** valuation of

```
U(P, a) = S(P) ∪ R(a)
```

and checks the following implication under the exact transition table:

```
P(s|S(P)) ∧ action_a(s|R(a)) = s'
    ⇒ P(s'|S(P))
```

Every action whose write set is disjoint from `S(P)` preserves `P` by
the structural frame rule, without enumerating any state. The conjunction of
all invariants therefore holds initially and is preserved by every action.
Ordinary simultaneous induction proves safety over executions of **any finite
length**, and hence every finite prefix of an infinite execution. This
argument is independent of the number of *unmentioned* state variables.

This is a **mathematical induction argument for the defined abstract
semantics**, not a Lean-kernel-checked theorem or a verified implementation
of this JavaScript checker. The latter remain explicit future proof tasks.
The invariant must accurately capture the human-intended property;
conservation of a model's integers does not automatically imply a real
system's business correctness.

The method is sound but incomplete. By default, each invariant is checked
using only itself as a pre-state hypothesis. An optional **separate strengthening
manifest** selects other invariants as assumptions for particular action/goal
obligations. The verifier checks **every** invariant's genesis and preservation
obligations together: this is *mutual induction*, so circular proof dependencies
are sound when the whole conjunction is inductive. Hypotheses affect pre-state
filtering only and can increase the checked local support; an oversized union
returns `inconclusive`, not a partial proof. PDR, IC3, CHC solving, CEGAR and
symbolic arithmetic are still unimplemented.
The amount of work is still exponential in the *local* union `U(P,a)`.
A genuinely global invariant involving all accounts can exceed the budget,
and the checker returns `inconclusive`, never a sampled success.

## Simultaneous induction and independently checked reference

The opt-in strengthening file (`zeno-fcis/local-strengthening/experimental-1`)
lists a sorted set of `(target invariant, action, assumed invariants)` bindings.
Every named candidate lemma is also a separate, fully checked target.
Assumptions are *only* active for its named transition obligation and only
in the pre-state. No assumptions are discharged from a previously issued
receipt, AI proposal or hash. Proofs remain conditional on this model's
precise read/write/frame semantics.

The `mutual.json` example has two variables, `x=y=0` initially, and actions
`x := y` and `y := x`. Each candidate `x=0` and `y=0` is **not** inductive
alone. A complete simultaneous induction is possible using `y=0` as an
assumption when proving `x=0`, and conversely. A deliberately incorrect
transition from `(0,0)` must still fail the strengthened checker.

```sh
node experiments/proof-composition-27/run.cjs /tmp/zfcis-proof-fixtures/mutual.json
# Returns exit 1: supplied invariants are not individually inductive.
node experiments/proof-composition-27/run.cjs /tmp/zfcis-proof-fixtures/mutual.json /tmp/zfcis-proof-fixtures/mutual-strengthening.json
node experiments/proof-composition-27/run_goals.cjs /tmp/zfcis-proof-fixtures/mutual.json /tmp/zfcis-proof-fixtures/mutual-goals.json /tmp/zfcis-proof-fixtures/mutual-strengthening.json
```

The first CLI argument in the optional strengthening position is a separate
canonical file; its byte hash is included in the result alongside the model,
goals and checker source identities. This is a reproducible identity binding,
not authentication of the owner or proof of binary provenance.

`global_oracle.cjs` is a separately authored, deliberately expensive
**full Cartesian product** reference. For small models it checks every
global induction pair and reachable states separately, allowing the test
suite to distinguish noninductive-but-reachable-safe candidate lemmas from
actual unsafe behavior. It is capped at 4,096 global states and 50,000
transitions; larger models return `inconclusive`. The local checker and
global oracle are compared on the reference models and seeded transition
mutations. The reference is a cross-check, **not a machine-checked
refinement theorem**, and runs in the same JavaScript runtime.

## Native parser and Authority differential bridge (unqualified)

The same draft PR now carries **four Rust tests in the actual
`zeno-fcis-contract` crate**, rather than only JavaScript abstract models.
These test sources have been committed, but a pinned Rust compiler is
unavailable in the current execution environment and the corresponding GitHub
workflow is queued. No native PASS or fully proved refinement is claimed.

The focused tests are:

- `native_bridge_27_guarded_budget_matches_independent_complete_reference`:
  the real `check_domain` checker and `JointComposition::joint_step` bind
  the committed two-component `.zeno` guarded-budget fixture. An independently
  authored Rust reference compares **all 32** declared state/request tuples,
  including authorization-first rejection, insufficient-budget rejection,
  unchanged state on rejection and exact accepted decrements. The separate
  native reachability enumeration must close at 32 rows; a cap of 31 refuses.
- `native_bridge_27_tampering_one_component_source_fails_closed`: changing
  the guard's source after its manifest was pinned must be rejected by both
  native binding and the composed-domain checker.
- `native_27_fifty_source_bound_components_keep_a_coupling_without_global_product`:
  instantiate **25 and 50 independently named native Zeno contract instances**
  (reusing the same reviewed bounded-counter source), with an actual
  two-component equality coupling. The native checker must prove all local,
  assumption and coupling obligations without enumerating the full product.
  The same source-bound joint-step API must accept a valid simultaneous
  increment and roll back all provisional Accepts if a late component rejects.
- `native_27_noninductive_coupling_refutes_even_with_fifty_valid_components`:
  the native coupling checker must refute a deliberately noninductive global
  claim while all 50 component-local checks remain valid.

**Do not confuse 50 instances of one small contract with 50 distinct
codebases.** This is a first production-API scaling and correspondence
*regression test*, not a theorem connecting the parser, Authority, proof
obligations and full application shell. In particular, the actual 2.6
composed-domain proof still has tested bridges and the full production
implementation-to-Verus refinement remains open.

The companion CI workflow has a new isolated pinned-Rust job:

```sh
cargo +1.97.1 test --locked --offline -p zeno-fcis-contract --lib native_bridge_27_ -- --nocapture
cargo +1.97.1 test --locked --offline -p zeno-fcis-contract --lib native_27_ -- --nocapture
```

Hosted compilation, complete source-bound qualification, independent review,
and the implementation-to-proof theorem are still required.

## Global conservation across many connected components

A second independent checker, `linear_checker.cjs`, proves a **single global**
weighted-sum safety property using only local action tables and their exact
frame conditions. The separately frozen goal lists every variable with a
bounded integer coefficient and an exact decimal-string expected total.

Let `S(s) = Σᵢ wᵢ·sᵢ`. For each action row, the checker establishes:

```text
    Σᵢ∈writes(action) wᵢ · (postᵢ − preᵢ) = 0
```

Since the complete finite-table transition updates **only** the declared
write set, all other summands cancel. The goal also checks `S(genesis)`
equals the frozen expected total. Induction therefore establishes this
**specific model property** for every execution history, without exploring
the Cartesian product of the global state. Both sides use exact `BigInt`
arithmetic; model admission validates all row domains and framing.

The connected path ledger has transfers between adjacent accounts, not
independent pairs. For 50 accounts it uses **882** local action-table rows
rather than checking 3⁵⁰ global states. For 200 accounts it uses **3,582**
local rows rather than 3²⁰⁰ global states. These are bounded toy ledger
models, **not** 50 or 200 source modules, native `.zeno` contracts, or
executable benchmarks. The model-admission invariant is intentionally
tautological: the *global* property is proved by the new delta checker.

```sh
node experiments/proof-composition-27/run_linear.cjs /tmp/zfcis-proof-fixtures/ledger50.json /tmp/zfcis-proof-fixtures/ledger50-goal.json
node experiments/proof-composition-27/run_linear.cjs /tmp/zfcis-proof-fixtures/ledger200.json /tmp/zfcis-proof-fixtures/ledger200-goal.json
```

Faults that leak or create tokens, omit coefficients, supply an incorrect
genesis total, or change the meaning of the goal are refused or refuted.
Future work is proving that native composition/collection/SQLite operations
actually refine this restricted model. ZenoFCIS 2.6 already has related
collection summary-delta and Lean/Verus work; this standalone experiment
neither replaces nor completes those existing bridges.

## Bounded automatic proposal (not an acceptance algorithm)

`propose_invariants.cjs` builds an *untrusted* graph from each action's declared
read footprint. For dependency components of at most three variables and at
most 4,096 finite domain tuples, it proposes a conserved-integer-sum invariant
using the declared genesis as the candidate total. It does not infer that
conservation is true. The separate checker is the only component that can
return a model-level `proved` result. A modified transition is still refuted;
an oversized connected dependency component returns `inconclusive`/a proposal
refusal, not a silently partial claimed proof.

This is a bounded seed for future CEGAR/PDR/CHC, abstraction libraries and
Crucible search. It is not a general invariant synthesizer, and the proposal
algorithm itself is neither verified nor trusted.

## Separately frozen safety goals

To prevent an untrusted invariant proposer from deciding what is worth proving,
the independent `goal_checker.cjs` takes a **separate canonical goal packet**.
Each goal is an owner-selected Boolean property represented by allowed tuples
over a declared support and an explicit set of candidate invariant premises.
It first independently checks induction of the candidate invariants and then
exhaustively checks that the selected premises imply the goal over their combined
local support. A failing implication returns a countermodel (not necessarily a
reachable execution); an excessive support union returns `inconclusive`.

The example generator freezes `chain3-owner-goals.json` independently of
the proposer. This demonstrates the separation, **not** cryptographic
authentication of a human's intent or a signed owner approval. The accepted
claim is still for the declarative finite model, never the generated executable.

```sh
node experiments/proof-composition-27/propose.cjs /tmp/zfcis-proof-fixtures/chain3-unproven.json /tmp/zfcis-proof-fixtures/chain3-candidate.json
node experiments/proof-composition-27/run_goals.cjs /tmp/zfcis-proof-fixtures/chain3-candidate.json /tmp/zfcis-proof-fixtures/chain3-owner-goals.json
```

## Baseline examples and negative controls

`pairs25.json` represents 50 variables / 50 transfer actions / 25 local
conservation invariants. The global state space has `3^50`
(717897987691852588770249) assignments, but the checker needs 450
small local implication rows plus 450 action-table admission rows.
The test suite also builds a 200-variable version (1,800 local rows);
neither case enumerates global states. These are *state count* experiments,
not Rust codebase-size benchmarks or measured solver speedups.

`chain3.json` has three coupled accounts, four transfer actions and one
three-account conservation property. It discharges four 27-row
obligations (108 local rows), even though each individual action reads
only two accounts.

The test suite also validates automatic candidate discovery for both fixture
families, and rejects incorrect proposals through the independent checker.

The test suite deliberately corrupts a transfer, the genesis, table ordering
and completeness, successor ranges, declared write/read footprints and model
shape. It also verifies that an oversized global invariant and an overly
tight work budget return `inconclusive` before any proof-obligation
evaluation. The CLI tests canonical-byte admission, exit statuses and
input/source identity hashes.

## Claimed and unclaimed evidence

| Claim | Evidence in this PR |
|---|---|
| Correct finite-table model validation on the checked fixtures | Standalone JavaScript tests and negative controls |
| Compositional invariant proof *under exactly this model semantics* | Complete local enumerations plus mathematical induction argument |
| Optional mutual induction with circular lemma dependencies | Every inductive lemma is checked against every relevant action under selected pre-state hypotheses |
| Global weighted conservation across a connected ledger | Exact per-action delta proof, separate frozen total, and 50/200-variable bounded experiments |
| Differential global-state model oracle (small cases only) | Independent full-product enumeration, reachability and seeded mutants |
| Arbitrarily many history steps in the fixed model | Conditional mathematical induction |
| Ability to verify 50 or 200 variable fixtures without a global product | Reproducible model-specific row counts |
| Independently checked goal entailment under finite-table semantics | Local complete implication check following verified candidate induction |
| Native parser/Authority finite bridge | Four Rust regressions pass locally with Rust 1.97.1; hosted qualification remains open |
| Native `zeno-fcis contract check-composition /3` refinement | **NOT ESTABLISHED** |
| Native `.zeno` parser/decision/collection/Authority equivalence | **NOT ESTABLISHED** |
| Lean/Verus proof of this new checker's soundness or source code | **NOT ESTABLISHED** |
| SMT, CHC, IC3, CEGAR or machine-checked proof certificates | **NOT IMPLEMENTED** |
| Executable, SQLite, outbox, adapter or isolated-worker refinement | **NOT ESTABLISHED** |
| Production adoption or release qualification | **NONE** |

The existing composed-domain Verus draft
[ZenoFCIS #137](https://github.com/TheDarkLightX/ZenoFCIS/pull/137)
is a related but different abstraction/proof stack. This experiment does
not inherit that proof automatically. Connecting them requires source-
and semantics-checked refinement, not a matching SHA or favorable example.

See [NEXT.md](NEXT.md) for the independently assignable integration gates.

Local qualification of starting revision `b6fd7b4` on 2026-10-26 ran all 52
Node tests (Node 20.19.2) and the native Rust bridge/scaling tests. The native
guarded-budget reference compares all 32 tuples. The 25/50 counter instances
check 504/954 local and coupling rows, using 300/600 Authority decisions and
209/409 formula cells; neither enumerates the global state product. These
counts characterize the supplied repeated-counter fixture, not a competitive
performance benchmark or a general implementation proof. The workflow's native
job placement and Rust formatting were repaired after that starting revision.
