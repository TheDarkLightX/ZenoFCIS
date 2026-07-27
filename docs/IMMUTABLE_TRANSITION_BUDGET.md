# Immutable Transition Budget Boundary

## Scope

`zeno-fcis-core::Transition::step` accepts immutable `BudgetLimits` and returns a
`BudgetedDecision`. Implementations create a fresh execution-local `Budget`,
charge modeled work, and consume that meter with `Budget::finish`. Caller-owned
mutable budget state no longer crosses the transition boundary.

This is a pre-1.0 source-API correction. It changes no protocol encoding,
schema, profile identity, precedence identity, candidate identity, receipt,
bundle, or budget hash.

## Inputs and outputs

The transition inputs are shared references to immutable state, command, and
context plus copied immutable `BudgetLimits`. The output contains:

- the complete `Accept | Reject | CommittedFailure` decision;
- the exact limits supplied to the transition; and
- the exact `BudgetUsed` charged by its execution-local meter.

`BudgetedDecision` has private fields and no public arbitrary constructor.
`Budget::finish` is its construction boundary and consumes the meter. `Budget`
is intentionally not `Clone`, so one execution-local meter cannot be forked
into conflicting usage reports.

## Authority boundary

The functional core owns decision semantics and deterministic logical-resource
accounting. A caller may inspect or bind the returned report, but cannot reuse a
partially consumed budget as hidden input to a later transition.

Implementations remain responsible for charging every modeled operation. This
API makes reported usage explicit and prevents the meter itself from escaping;
it does not independently prove accounting completeness.

## Trusted dependencies

This package adds no dependency. It relies only on the existing
`zeno-fcis-core` decision algebra and integer arithmetic.

## Deterministic resource bounds

The existing seven resource classes and `u64` limits are unchanged. Charges are
atomic: an overflow or limit violation leaves `BudgetUsed` unchanged. The
returned usage is bounded componentwise by the returned limits.

## Laws and negative cases

- Equal immutable inputs and equal limits produce equal reports for a
  deterministic transition implementation.
- A successful charge appears in the returned `BudgetUsed`.
- A rejected over-limit charge contributes no partial usage.
- Consuming a report through `into_parts` preserves the exact decision, limits,
  and usage.
- The transition receives no caller-owned mutable budget reference.

## Assumptions

- Transition implementations observe no ambient clock, randomness, filesystem,
  network, database, scheduler, thread state, or mutable global state.
- Implementations charge logical work according to their reviewed algorithm.
- Callers bind the returned report when a higher-level candidate or receipt
  requires a budget commitment.

## Explicit nonclaims

- This is not a proof that an implementation charged every operation.
- Logical budgets are not wall-clock, energy, allocation, or WCET evidence.
- This package does not validate higher-level `TransitionResourceReport`
  construction.
- This package makes no production-readiness or unbounded-proof claim.
