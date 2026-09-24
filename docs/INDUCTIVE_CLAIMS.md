# Inductive claims

## Purpose

Every other claim is proved over observations that nothing constrains. Such a
proof holds for every system alike and says nothing about a particular
application (see [law and claim substance](CLAIM_SUBSTANCE.md)). A property
over the full integer range of an application's state therefore had no proof
route, and neither did any property of a hand-written program.

An inductive claim uses the application's laws as its model of how the state
can change. The authority refuses every decision that breaks a law enforced on
it, so the laws enforced on accepts and on committed failures bound every
transition the application can commit, whether its program is hand-written or
synthesized. An invariant whose induction step holds relative to those laws
holds on every state the application commits, provided the application checks
the base case and observes every value the invariant reads.

A solver's `unsat` for the step is Attested, not Proved: its proof text is not
independently checked. See [design record 0003](adr/0003-epistemic-status.md).

## Syntax

```text
claim ID name BACKEND inductive [assume [LAW, ...]] [accept [LAW, ...]] [failure [LAW, ...]] = INVARIANT;
```

- `INVARIANT` is a relational formula that reads only `pre.` state paths, which
  here stand for "the state".
- The three groups are optional, but must name at least one law in total, and
  must appear in the order shown. Each law is named once, and must be declared
  in the project:
  - `assume` lists laws enforced on every committing decision;
  - `accept` lists laws enforced on accepts;
  - `failure` lists laws enforced on committed failures.
- `BACKEND` is `cvc5`, `z3`, or `all`. There is no Lean export for inductive
  claims yet, so `prove --backend lean` reports that the mode is unsupported.

The groups follow the decision scopes in the law manifest. A state invariant
is usually scoped to every committing decision, an action law to accepts, and
a failure law to committed failures. Every template uses this pattern.

The claim and its three groups are part of the canonical project bytes. A
project without inductive claims encodes exactly as before.

Example, from the durable-counter template:

```text
claim 600 counters_never_negative all inductive accept [501] failure [502] = pre.100.110 >= 0 && pre.100.111 >= 0;
```

## What is checked, and where

The argument has three parts. `prove` checks only the first.

1. **The step**, exported to SMT by `export_inductive_smt`. It asks for an
   assignment to every observed `pre.`, `post.`, `command.`, and `context.`
   value, each within the i128 range, together with a decision kind (1 for an
   accept, 2 for a committed failure) when the claim names per-kind laws. The
   assignment must make each of these true:
   - every law in `assume`;
   - every law in `accept` when the kind is 1, and every law in `failure` when
     the kind is 2;
   - the invariant over `pre.`;
   - the invariant restated over `post.` (`invariant_at`), *not* true.

   Each observed value whose type is enumerated, or bool, is also asserted to
   be one of its declared values: its variants' IDs, or 0 and 1
   (`declared_domain`). Admission refuses anything else, so this excludes
   only inputs the application never decides. Integer ranges are supplied by
   `build.rs`, not by `project.zeno`, so the step does not assume them.

   An unsatisfiable answer means that no transition the assumed laws admit,
   over declared values, leaves the invariant.
2. **The base case and the observer**, checked by the application.
   `evaluate_invariant` must return `EvalOutcome::True` on the exact genesis
   state, as the law checker's own observer sees it. That observer must also
   read every path the invariant reads (`claim_paths`), so that the invariant
   has a value on every state the application commits. It is not enough to
   build a complete observation by hand for the test.
3. **The assumptions**, checked by the application.
   `LawManifest::check_step_assumptions` must accept the claim's three groups,
   mapped to the manifest's identifiers. It refuses a law the manifest does
   not define, and a law whose scope does not cover every decision of the kind
   the claim assumes it on. Assuming an accept-only law in `assume` is refused,
   for example.

The durable-counter template's `tests/induction.rs` runs all of these checks
against a real shell, its law checker's observer, and its manifest. Its law
checker observes the state before and after a decision through one function,
`state_observations`, so assumption (f) holds by construction.

## Why it is sound

Suppose all of the following hold:
- (a) the step is unsatisfiable;
- (b) the base case passes;
- (c) the assumption check passes;
- (d) the authority enforces its manifest: a decision commits only if every
  law whose scope covers its kind is reported satisfied;
- (e) the application's law checker reports a formula law satisfied only when
  the formula evaluates to true on the transition's observations, which is how
  every template's checker works;
- (f) the application observes the state before and after a step through the
  same projection paths;
- (h) that observer reads every path the invariant reads, on every state the
  application commits, and evaluating the invariant on an observed state stays
  within the evaluation limits. Otherwise the invariant has no value on some
  committed state. A partial observer could pass the base case and the
  assumption check, and still give `MissingProjection` later.
- (g) the law checker observes an enumerated field as its variant ID and a
  bool as 0 or 1, and every value it observes is admissible under the
  authority's schema, which refuses undeclared variants. The authority checks
  admissibility itself, against its own catalog's schema:
  - the command and context when it admits an invocation;
  - the initial state at genesis;
  - the pre-state and post-state of every transition before it can commit.

  The templates' conformance tests, which read fields by numeric ID, check
  the observation binding.

Then every state the application commits, as its law checker observes it,
satisfies the invariant. The argument is by induction over the committed
history:
- genesis satisfies it, by (b), (f), and (h);
- a rejection leaves the state unchanged;
- an accept, or a committed failure, satisfies every law assumed for its kind,
  by (c), (d), and (e). Its pre-state satisfies the invariant by the induction
  hypothesis, so its post-state does too, by (a).

**Arithmetic.** Evaluation is strict. An overflow, a division by zero, or an
inexact exact division anywhere in a formula makes the formula have no value,
and the law checker then does not report the law satisfied. So on a committed
transition, every assumed law was defined and true. The step therefore asserts
each law as "defined and true" on its own, and asserts that the invariant over
`post.` is not "defined and true". An undefined subterm is never hidden inside
a larger implication.

## Results

| Result | Meaning |
| --- | --- |
| `ProposedUnsat` (CVC5) | The step holds, attested by the solver's `unsat`. Its proof text is not checked. As for every solver result, `prove` exits 2. |
| `Blocked(UnsupportedEvidence)` (Z3) | Z3 answered `unsat`. Z3 gives no proof object, so it is not retained as evidence. |
| `Refuted` | A replayed transition satisfies the assumed laws and leaves the invariant. |
| `Undefined(reason)` | A replayed transition satisfies the assumed laws, and the invariant over `post.` has no value there. |

A counterexample is replayed through the library evaluator before it is
reported. The model is confirmed only when all of these hold:
- every observed value is present, and within its declared domain;
- every law assumed for the model's decision kind evaluates to true;
- the invariant over `pre.` evaluates to true;
- the invariant over `post.` does not.

Any other model is `ModelReplayFailed`, never a verdict. The retained
`counterexample.json` includes `zeno_decision_kind` when the step is split.

A counterexample to induction may start in a state the application never
reaches. The scope line says so, and the remedy is to strengthen the invariant
or assume more laws.

## Worked examples

From the tests and from checks on scratch copies with the pinned CVC5 1.3.3
and Z3 4.16.0, all run on 2026-09-24:

- A counter whose accept law is `post == pre + amount` and whose every-commit
  law bounds `0 <= amount <= 2`:
  - for `pre >= 0`, CVC5 answers `unsat` (attested).
  - `pre <= 9` is refuted at 9 + 1 = 10. Adding the bound law `post <= 9` to
    `assume` makes it provable.
  - `pre * pre >= 0` is `undefined (overflow)` near √(i128::MAX). The
    invariant itself overflows there, although it holds wherever it is
    defined.
- The same counter with a committed-failure law `post == 0`: for `pre >= 0`,
  with `accept [401] failure [403]`, CVC5 answers `unsat` (attested). Without the `failure` group, a
  committed failure is unconstrained, and the counterexample carries
  `zeno_decision_kind 2`.
- The durable counter: for `counters_never_negative`, CVC5 answers `unsat`
  (attested) from its action laws alone. `pre.count <= 3` is refuted at 3 + 1 = 4, which shows the cap
  comes from the bound law and the program's capacity check, not from the
  action laws.
- The account-lockout template's lock invariant, the formula of its law 500,
  assumed only its action laws. Before declared domains, the first
  counterexample was an "accept" with command 0: the action laws never state
  that an accept is a login or an unlock, because admission guarantees it.
  Declared domains now exclude it without changing the laws. The remaining
  counterexamples start at times near the top of the i128 range, or at
  `failed_attempts = 3`. The integer ranges in `build.rs` exclude both, which
  is the first limit below.
- The compliance-gateway template's strikes invariant, `0 <= strikes <= 3`,
  with `accept [501, 502] failure [503]`, gave the same command-0
  counterexample before declared domains. With them, CVC5 answers `unsat`
  (attested) from the gateway's own laws. The template ships this as its
  claim 600, with `tests/claims.rs` for the application's checks.
- The withdrawal-queue template's solvency invariant, the formula of its law
  500, assumed on every commit its conservation, authorization, and tick laws,
  which its law checker enforces on every committing decision because the
  application never commits a failure. The draft's formulas under-described
  the program, and CVC5 refuted the step four times, each with a transition
  the README's rules forbid: a deposit from 4 to 5, a deposit of −1, a tick
  from a pause outside the schema's range that set must-serve, and a tick
  that emptied a lane while keeping its amount. Each was a rule the Rust part
  of the law checker enforced but no formula stated. With the deposit's
  capacity, positive amounts, the must-serve rule, and the lane-and-amount
  coupling stated as laws, CVC5 answers `unsat` (attested). The pause's range,
  `0 <= pause <= 2`, was attested from the tick law on the first run. The
  template ships both as claims 600 and 601, with `tests/claims.rs`.

## Limits

- **No integer ranges.** Variant domains and bools come from `project.zeno`,
  but integer ranges are supplied by `build.rs`, so the step cannot assume
  them. An invariant whose proof needs a range must get it from an assumed law,
  or strengthen itself with the range and prove that it is preserved. Stating
  scalar bounds in `project.zeno`, or exporting the schema's bounds into the
  step, is the planned next improvement.
- **Scopes are not in `.zeno`.** The claim states which decisions each law is
  assumed on, and the application confirms it against its manifest.
- **No Lean export yet.** Solver results are Attested at most.
- **Named predicates** make replay impossible, and are refused at replay, as
  for other claims.

## Controls

Each check below was removed in a working tree, and the named test failed. The
edits were reverted; none is committed.

| Check removed | Detected by |
| --- | --- |
| Replay confirms undefined models (always `ModelReplayFailed` instead) | `replay_confirms_models_where_strict_evaluation_has_no_value` |
| Replay requires every assumed law to hold | `inductive_replay_confirms_only_transitions_the_laws_admit` |
| Replay requires the invariant over `pre.` to hold | `inductive_replay_confirms_only_transitions_the_laws_admit` |
| Replay uses the model's decision kind | `a_split_step_uses_each_decision_kinds_own_laws` |
| Per-kind laws are guarded by the decision kind in the export | `a_split_step_uses_each_decision_kinds_own_laws` |
| The export asserts every assumed law | `the_inductive_step_asserts_each_law_and_the_invariant_separately` |
| `invariant_at` rewrites `pre.` to `post.` | `the_invariant_is_restated_over_the_post_state_and_nothing_else` |
| Elaboration refuses non-`pre.` paths in the invariant | `elaboration_refuses_inductive_claims_it_cannot_ground` |
| Elaboration refuses undeclared laws | `elaboration_refuses_inductive_claims_it_cannot_ground` |
| The groups are part of the canonical bytes | `inductive_claims_keep_their_assumed_laws_in_the_canonical_project` |
| The manifest check refuses accept-only laws in `assume` | `step_assumptions_must_be_enforced_on_the_decisions_they_are_assumed_on` |
| Inductive invariants are classified by `invariant_substance` | `an_invariant_that_reads_state_is_substantive` |
| The export asserts declared domains | `declared_domains_bound_enumerated_and_bool_observations` |
| Replay refuses values outside a declared domain | `declared_domains_bound_enumerated_and_bool_observations` |
| `declared_domain` reads variants and bools, and nothing else | `declared_domains_come_from_variants_and_bools_only` |
| Model values down to `i128::MIN` are parsed, so a counterexample there is replayed | `model_values_span_the_whole_i128_range` |
| In the durable counter, the law checker's observer reads every field the invariant reads (planted by observing `failures` under another path) | `the_law_checker_observes_every_field_the_invariant_reads`, `the_invariant_holds_on_the_exact_genesis_state` |

The pinned test `pinned_inductive_steps_agree_with_exhaustive_replay` checks
the SMT encoding itself. It uses eight claims over a box that the assumed laws
bound. For each claim, CVC5 and Z3 must agree with an exhaustive replay of
every transition in the box. The claims cover:
- a step that holds;
- a refuted step;
- a step that is undefined only;
- a step with both kinds of counterexample;
- a split step with a missing failure group.

The test runs in the formal-tools workflow and in `tools/record_gate_evidence.py`.

## Explicit nonclaims

- A result for the step alone says nothing about the application. The base
  case, the observer check, and the assumption check must also pass in the
  application's own tests.
- The result rests on the solver's unchecked `unsat` and on the SMT encoding.
  The pinned differential test checks the encoding only on its collection.
- An invariant with an attested step is only as meaningful as the laws it
  assumes. Laws that
  under-describe the program give counterexamples, and laws that
  over-describe it refuse decisions at run time.
