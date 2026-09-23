# Law and claim substance

## Purpose

A law or claim whose value cannot change with a system's behavior gives no
evidence about that behavior, even when a checker accepts it or a prover
proves it. `pre.100 == pre.100` can never fail, so a runtime law check of it
never detects a faulty transition, and a proof of it says nothing about the
project it appears in.

This package reports such formulas before anyone relies on them, and it states
what a solver result means when the exported obligation contains no model of
the system.

## Inputs and outputs

`zeno_fcis_spec::law_substance` and `claim_substance` take an elaborated law or
claim and return one `Substance`:

| Code | Meaning |
| --- | --- |
| `constant-true` | True for every assignment of the observations it reads. It can never fail. |
| `constant-false` | False for every assignment. Any effect depends on which decisions it is applied to, which `.zeno` does not state. |
| `ignores-transition` | A single-step formula that reads no post-state, effect, outbox, or event observation, or a temporal formula that reads no observation at all. No transition outcome can change it. |
| `may-constrain-transition` | Reads an observation that a transition can change. Necessary for a meaningful constraint; not sufficient. |

`zeno-fcis check` reports the classification of every law and claim:

- Human output prints one `warning:` line on stderr for each formula that is
  not `may-constrain-transition`. The summary line on stdout is unchanged.
- JSON output adds a `substance` object with `laws` and `claims` arrays of
  `{code, id, name}` entries.
- `--require-substantive` exits with code 1 when any law or claim is vacuous.
  JSON output then reports `status: "vacuous"`.

`zeno_fcis_formal_tools::ExportedObligation::scope` returns the
`ObligationScope` of an exported obligation. Every current SMT and Lean export
has scope `without-system-model`: the obligation contains the claim over
bounded, otherwise unconstrained observations, with no transition relation,
initial state, schema bound, or law. `zeno-fcis prove` prints a `scope:` line
after each accepted or refuted result:

- An accepted result (`ProposedUnsat` or `KernelChecked`) holds for every
  bounded observation assignment and says nothing specific about the system.
- A refuted result's counterexample may be unreachable in the system.

## Authority boundary

The classification is diagnostic. It creates no evidence, grants no authority,
and changes no canonical project bytes, semantic program hash, exit code for
existing invocations, or existing JSON field. Only `--require-substantive`
changes an exit code, and only when a caller asks for it.

## Trusted dependencies

The analysis uses only the typed `.zeno` AST in `zeno-fcis-spec`. It runs in
`no_std + alloc` without clocks, randomness, I/O, or solvers.

## Deterministic resource bounds

Classification is one traversal of each formula. It is linear in the formula
nodes that elaboration already bounds.

## Laws

1. A formula is reported as constant only when its value is fixed for every
   assignment of present observations and no subterm can be indeterminate.
   The checked evaluator's connectives are strict: an indeterminate operand
   makes `&&`, `||`, and `->` indeterminate. So a constant operand fixes a
   connective only when the other operand is always defined.
2. Arithmetic, host predicates, and non-empty quantifiers are never folded.
   Arithmetic can overflow or divide inexactly, predicates can be missing, and
   quantifier iterations are budgeted, and each of these fails closed.
3. A single-step formula that reads a post-state, effect, outbox, or event
   observation and is not constant is never reported.
4. Temporal formulas that read pre-state observations are never reported,
   because a later pre-state is an earlier post-state.
5. `next` is false at the final step, so only a false body makes `next`
   constant. Every remaining suffix of a trace is non-empty, so a constant body
   fixes `always` and `eventually`.

## Negative cases

The tests keep these outcomes:

- The shipped Mini Determinator law 400 and claims 500 and 501 are
  `constant-true`.
- The template placeholders `= true` and `= false` are constant.
- The durable-counter transition laws are `may-constrain-transition`.
- `false && div_exact(post.x, 2) == 1` and `post.x + 1 == post.x + 1` are not
  folded.
- `always atom(pre.x <= 3)` is not reported.
- A project with only transition laws passes `--require-substantive` with no
  warnings.

## Assumptions

- Every observation a formula reads is present during evaluation. A missing
  observation is a projection error, not a transition error.
- Evaluation stays within its configured operation limits.

## Explicit nonclaims

- `may-constrain-transition` does not establish that a formula states a useful
  requirement, or that the requirement is correct.
- The analysis is syntactic. It does not detect formulas that are implied by
  schema bounds or other laws, such as a law that restates a field's declared
  range.
- `without-system-model` results remain model-free. A proof about a transition
  needs an obligation that includes the transition relation, initial state,
  and domain. That is separate, later work.
