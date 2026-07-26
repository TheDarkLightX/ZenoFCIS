# Universal project profiles and mounted runtimes

ZenoFCIS is not a ZenoDEX framework. Its semantic kernel is project-neutral; this document defines the boundary by which any project gives domain meaning to the shared values, patches, plans, evidence, and refinement machinery.

## Common profile

Every bounded context declares one `ProjectProfile` containing:

```text
project + subsystem + stable profile family + version
root state / command / authenticated-context type IDs
domain prefix
schema / precedence / algorithm / codec / effect / channel / policy hashes
canonically ordered stable registry entries
```

Registry namespaces cover state types, commands, contexts, reasons, effects, channels, evidence, capabilities, events, claims, and migrations. Numeric IDs and definition commitments are authority. Readable names are normalized and retained for review.

## Evolution evidence

Additive evolution may add entries, but it may not delete or rebind an existing stable ID, change root types, or silently change codec, precedence, algorithm, or policy. A schema, effect-registry, or channel-registry commitment may change only when the successor adds an entry in that exact namespace and supplies a nonzero reviewed compatibility-evidence commitment. Evidence for an unchanged binding is rejected.

Incompatible changes require an explicit nonzero migration-specification commitment. `ProfileEvolution` is the authoritative evolution artifact. It binds:

```text
exact predecessor profile commitment
exact successor profile commitment
additive extension evidence or migration commitment
profile-evolution format version
```

A bare compatibility report is diagnostic. Promotion and migration records should retain the canonical `ProfileEvolution` commitment.

## Inputs, outputs, authority, and bounds

Inputs are two validated `ProjectProfile` values plus one explicit `EvolutionMode`. The diagnostic output is a deterministic blocker sequence. The authoritative output is a canonical `ProfileEvolution` or a typed incompatibility error.

The crate decides identifier continuity and whether required evidence is present and content-bound. It does not decide that a migration implementation, schema extension, or business invariant is correct. Those claims belong to independent evidence checkers.

Deterministic bounds are:

```text
maximum stable-name bytes: 64
maximum domain-prefix bytes: 160
maximum registry entries per profile: 65,536
fixed three optional additive evidence commitments
linear predecessor-entry and successor-addition scans
```

The compatibility laws are stable-ID preservation, no removal or rebinding under additive evolution, exact namespace matching for binding extensions, strict version increase, deterministic blocker order, and content separation for distinct migration commitments. Negative cases cover zero evidence, unused evidence, missing corresponding additions, root changes, removals, rebindings, non-increasing versions, and zero migration commitments.

Assumptions are collision resistance of the selected commitment provider and correctness of independently reviewed definition/evidence hashes. This layer does not prove schema compatibility, migration totality, semantic equivalence, business correctness, runtime refinement, or production readiness.

## Generic mounted runtime

`zeno-fcis-adapter` mounts any callable or canonical JSON-line runtime that emits a complete `NormalizedDecision`. The adapter is project-neutral and compares:

```text
decision kind and stable reason
profile, command, context, precedence, algorithm, and budget bindings
pre-root and post-root
candidate identity
patch, commit plan, outbox plan, receipt, and complete bundle
```

JSON is transport only. ZCVE component bytes remain authoritative. Alternate whitespace, field order, case, unknown fields, duplicate fields, malformed hex, incomplete candidates, and reject-with-state-change forms fail closed.

`zeno-fcis-adapter-zenodex` keeps the concrete pinned zUSD mount and reuses the generic transport. New integrations should depend on `zeno-fcis-adapter` and keep project-specific mapping code in the project repository or a dedicated profile crate.

## Project mapping

### ZenoDEX

Bounded contexts should be split by authority rather than placed in one giant profile:

- balances and replay protection;
- spot pools and LP positions;
- zUSD vaults, Stability Pool, liquidation, redemption, and shutdown;
- Oracle observation/finalization;
- perpetual markets and accounts;
- governance and breakers;
- cross-lane atomic settlement.

The runtime bridge must emit complete decisions, not only accept/reject, receipt hashes, or state roots.

### ZenoStorage

Profiles should cover:

- storage-agreement lifecycle;
- object/package identity and immutable metadata;
- provider selection and obligations;
- payment/settlement evidence admission;
- retrieval, repair, and availability evidence;
- exact-once repository publication;
- key-capsule and retry-material capabilities.

Random keys, nonces, provider observations, chain facts, and clocks enter as authenticated context. The storage core must not generate ambient randomness or query providers.

### ZenoMail

Profiles should cover:

- mailbox and conversation state;
- immutable message envelopes;
- multi-device membership and key epochs;
- send, receive, acknowledge, revoke, and rotate commands;
- spam, trust, and delivery-policy decisions;
- ZenoStorage placement/retrieval plans;
- notification outbox channels.

Cryptographic operations remain external providers whose exact inputs and outputs are bound into context and receipts.

### PopperPad

Profiles should cover:

- append-only claim, evidence, recipe, artifact, and relationship events;
- hostile bundle import;
- verifier-result admission;
- truth quarantine and local trust policy;
- supersession, narrowing, refutation, and reproduction relations;
- bounty/challenge lifecycle;
- anchor and publication effects.

Recipe execution is an imperative-shell backend. The semantic core decides whether the resulting evidence is admissible and what graph transition follows.

### Helix

Profiles should cover:

- source, watchlist, evidence, claim, case, and automation-rule lifecycles;
- deterministic ranking and credibility math;
- reasoning backend requests and checked results;
- autopilot proposal, confirmation, denial, escalation, and execution;
- credential capabilities and redacted audit events;
- on-chain dry-run and receipt plans;
- federation messages.

LLMs and neural services propose typed values. Policy and reasoning kernels decide authority.

### LucyOS

Lucy should use separate profiles for agent governance and, later, native microkernel semantics:

- generation-tagged objects and capabilities;
- threads, address spaces, IPC endpoints, notifications, and reply authority;
- scheduling contexts, criticality, and time domains;
- syscalls, faults, interrupts, timers, and IPIs;
- page-table, TLB, cache, IOMMU, DMA, and user-entry machine plans;
- static system descriptions and generated capability graphs;
- per-core ownership and explicit cross-core protocols.

Privileged instructions and unsafe architecture code remain in a tiny machine interpreter that must refine closed machine-operation values.

## Private and future backends

ESSO, Morph, theorem provers, solvers, compilers, LLMs, and project-specific optimizers must enter through a generic backend protocol rather than a crate named after a private engine. A backend proposes closed artifacts and evidence; independent validators decide whether they satisfy the reviewed project profile. Backend identity, exact source/tool hashes, bounds, traces, counterexamples, and claims must be content-bound.

## Cross-repository validation

The permanent read-only `universal-project` workflow checks out the exact ZenoDEX revision named by the mounted zUSD profile and runs formatting, warnings-denied Clippy, and tests for its complete Rust workspace under Rust 1.97.1. This is a build and regression integration gate, not a substitute for the separate full-decision runtime-refinement evidence.

## Nonclaims

A common profile format does not make unrelated projects share business semantics. It prevents them from reimplementing stable identity, migration, runtime transport, and evidence boundaries inconsistently. Project-specific invariants, cryptography, runtime refinement, storage guarantees, and operational security remain separate proof obligations.
