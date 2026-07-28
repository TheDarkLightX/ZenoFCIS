# On-chain FCIS generation

ZenoFCIS now defines one closed semantic machine that can be rendered by chain-specific shells:

```text
stable schema IDs + immutable pre-state + command + authenticated context
    -> pure total decision
    -> Accept(next state, bounded observations, bounded capabilities)
     | Reject(stable reason)
    -> shell validation
    -> atomic state commit
    -> closed effect interpretation
    -> content-addressed receipt
```

The shared `OnchainMachineSpec` fixes state fields, command fields, rejection reasons, public event schemas, capability grants, plan bounds, and observation-shape policy. Declaration order is normalized by stable numeric IDs before the machine hash is calculated.

## Why this is safer for agent-authored code

Generated shells retain effect authority. An agent implements only pure admission, invariant, and decision functions. The generated layer captures caller/signer and chain context, checks optimistic-concurrency preconditions, validates every accepted plan, commits state, and interprets only capabilities already present in the reviewed machine catalog.

Arbitrary EVM calls, arbitrary Solana CPIs, arbitrary calldata, delegate execution, dynamic account lists, and unreviewed token bindings are not generic capabilities. Adding one requires a new reviewed backend profile.

These restrictions constrain what generated project logic may express. They do not sandbox the agent, compiler, build scripts, dependencies, or analyzers. Dedicated temporary build paths provide workspace separation only. See the [execution and sandbox boundary](EXECUTION_SANDBOX_BOUNDARY.md).

## Solidity v2

`generate_advanced_solidity` emits:

- an abstract base contract pinned to Solidity `0.8.36`;
- an exact machine-hash constant;
- private generated storage and sequence state;
- expected-state-hash and expected-sequence checks;
- recomputed state-root corruption checks;
- compiler-enforced `internal pure` hooks;
- typed `_event<Name>` and `_effect<Name>` helpers;
- bounded and canonically ordered event/effect plans;
- zero checks on unused fixed-array plan slots;
- exact capability, asset, recipient, amount, and per-transition use validation;
- exact token address and runtime-code-hash bindings;
- OpenZeppelin `SafeERC20` transfer interpretation;
- checks-effects-interactions ordering and a reentrancy gate;
- SHA-256 machine, state, command, context, plan, and candidate receipts;
- a generation manifest and agent-editing policy.

The original effect-free Solidity scaffold remains available for small local-state machines. New value-moving applications should start from the shared on-chain model and the capability-bound v2 generator rather than widening the v1 shell by hand.

The generator expects OpenZeppelin Contracts `5.6.1`. The generated import path is conventional; dependency locking, source digest retention, and compiler invocation belong in the consuming project and its CI evidence.

## Observation policy

`PublicVariableShape` allows public event/effect counts to vary inside reviewed bounds. `FixedShape` requires the configured number of active slots. Fixed shape reduces one obvious public length channel, but it is **not** a claim of constant-time execution or covert-channel freedom. Gas/compute usage, branches, state access, transaction inclusion, and external protocols remain observable.

## Capability boundary

The initial shared capability is `FungibleTransfer`. It binds:

- a stable capability code;
- a stable cross-chain asset ID;
- a recipient policy derived from caller, fixed identity, command field, or pre-state field;
- an amount ceiling;
- a per-transition use ceiling.

The Solidity backend additionally binds each capability to an exact token address and expected runtime code hash. The Solana backend should bind the same semantic capability to exact mint, token program, authority PDA, vault, and destination-account constraints.

## Required assurance before production

Generated architecture is evidence, not production authorization. A production release still requires:

1. exact compiler and dependency pins;
2. compiler-known-bug review;
3. retained source and generated-bundle digests;
4. unit, property, invariant, and mutation tests;
5. gas or compute ceilings and failure-path tests;
6. static analysis and formal analysis proportional to value at risk;
7. independent review and deployment-specific binding verification;
8. upgrade-authority and governance review;
9. post-deployment bytecode or verified-build confirmation.

No generated backend claims economic correctness, oracle correctness, MEV resistance, constant-time execution, covert-channel elimination, or audit completion.
