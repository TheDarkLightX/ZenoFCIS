# Temporal logic version 1

ZenoFCIS `1.0.0-rc.3` gives every temporal claim an explicit mode.

## Modes

`finite N`, where `1 <= N <= 256`, evaluates a nonempty trace containing at most
`N` bounded logical steps. Every length from one through `N` is part of the claim. A logical step
is an event observation, not a wall-clock duration. A trace beyond the horizon,
an empty trace, missing observation or predicate, arithmetic error, or resource
limit blocks the claim.

`unbounded` has no built-in evaluator. It creates a proof obligation for the
Lean adapter. A finite result or SMT result cannot be relabeled as an
unbounded theorem.

## Operators

Version 1 supports atoms, Boolean negation/conjunction/disjunction, strong
`next`, `always`, `eventually`, and strong `until`.

- `next p` is false at the final finite step;
- `always p` requires `p` at every remaining step;
- `eventually p` requires one remaining witness;
- `p until q` requires a future `q` and `p` at every earlier step.

Example:

```text
claim 500 stable cvc5 finite 4 = always atom(post.100 >= 0);
claim 501 stable_forever lean unbounded = always atom(post.100 >= 0);
```

These claims are deliberately distinct, even when their formula text is
similar.

## Evaluation outcomes

The finite evaluator returns `Satisfied`, `Counterexample { step }`, or
`Indeterminate(reason)`. Counterexample steps are zero-based logical trace
positions. Unbounded evaluation returns `ProofObligation` without inspecting a
finite trace.

Relational atoms share the checked `i128` evaluator. `div_floor` and
`div_ceil` use mathematical floor and ceiling for every sign combination.
Overflow, divide by zero, non-exact `div_exact`, absent named predicates,
missing observations, and iteration exhaustion remain indeterminate and block
promotion.

## Translation contract

CVC5 and Z3 receive only relational and finite-trace obligations through
deterministic SMT-LIB. The translation carries checked `i128` definedness,
signed division semantics, and an explicit trace-length choice from one through
the declared horizon. An undefined arithmetic result is a counterexample
condition. CVC5 is asked for proof output in safe mode when the supported
fragment allows it. Z3 is used for differential checking and replayable
models; unsupported Z3 UNSAT evidence remains blocked. SAT models must replay
against the exact typed claim in the built-in evaluator.

Lean receives unbounded temporal proof obligations. The generated source
preserves every projection path, relational atom, bounded sum and quantifier,
checked arithmetic condition, and temporal operator. Evidence binds the exact
claim ID, generated source, Lean toolchain identity, successful kernel check,
and exact allowed-axiom set. A process exit code or parsed word alone is never
a certificate.

## Nonclaims

Finite execution is bounded model evidence. It is not induction, liveness over
infinite traces, fairness, real-time reasoning, or deployment authority. Lean
source generation is an obligation until the exact source is kernel checked
under the configured axiom policy.
