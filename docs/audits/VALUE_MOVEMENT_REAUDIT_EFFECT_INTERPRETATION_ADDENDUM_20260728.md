# Value-Movement Re-audit Addendum — CommitPlan Interpretation

## Finding RA-010 — P1

At integrated PR #70 head `aea9d9db3ff596ba09afed1f2f1b9b13c914f651`, `CommitPlan` effects are validated, candidate-bound, law-visible, and persisted inside the bundle. They are not interpreted by the authorized SQLite commit protocol.

The SQLite commit path performs:

```text
validate nominal authorization
validate and apply semantic patch
write state/root/version/write authorization, bundle, receipt, replay
write OutboxPlan rows
commit SQLite transaction
```

The bound `I` value is used only under `I: IdempotentDestination` by `deliver_next` for outbox delivery. The code exposes no corresponding production `CommitPlanInterpreter` or closed operation-registry execution step.

## Why this matters

The plan and catalog documentation describe effects as authoritative operations. Project examples include:

- mint;
- burn;
- collateral transfer;
- fee allocation;
- settlement;
- liquidation compensation;
- provider payment/obligation actions.

If these effects are executable authority, a successful SQLite commit can update semantic state while merely retaining the required operation as bytes. The state/effect relation may be mathematically correct in the candidate yet operationally incomplete.

If effects are not executable authority, their names and documentation must not imply that successful shell commit has performed them.

## Three acceptable semantic models

### 1. Atomic local commit effects

The shell owns an exact closed operation registry and interprets every effect inside the same transaction or another formally refined atomic substrate.

Required properties:

- exact registry and interpreter identity;
- all-or-nothing state/effect publication;
- no hidden imperative operation;
- exact result validation;
- crash/retry proof;
- complete receipt status.

### 2. Durable effect obligations

Effects become durable acknowledged obligations similar to an outbox, with:

- stable idempotent identity;
- pending/delivered/failed state;
- exact bundle membership;
- retry and committed-failure semantics;
- ordering and conflict policy;
- destination/interpreter binding.

A shell commit means the obligation was durably scheduled, not necessarily completed.

### 3. Evidence-only effect declarations

Effects are explicitly non-executable evidence. Value-moving profiles must use another qualified operation surface, and APIs/documentation must not call the plan authoritative shell operations.

## Tests required

For every effect operation:

1. crash before any effect;
2. crash between effects;
3. crash after effects before state publication;
4. crash after state publication before effect status publication;
5. replay exact authorization;
6. replay same candidate under another authorization;
7. interpreter returns success with wrong observed result;
8. interpreter applies partial result then errors;
9. same-type interpreter configuration changes across restart;
10. reference/SQLite effect status and identities compare exactly.

## Guarantee impact

Until one model is selected and implemented, the safe statement is:

> ZenoFCIS can bind and persist a structurally and relationally validated CommitPlan.

It cannot yet claim:

> The authorized SQLite shell atomically performs every authoritative CommitPlan effect.

Tracked in issue #76.
