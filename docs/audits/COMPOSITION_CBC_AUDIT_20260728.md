# ZenoFCIS composition and correctness-by-construction audit

**Audit date:** 2026-07-28  
**Reviewed baseline:** `64c53e43c8110a441cf9a72174efb599030dccf6`  
**Scope:** generic composition, transition construction, refinement/evidence promotion, authenticated-state context, and catalog-authorized publication  
**Relationship to prior work:** extends the value-movement audit in PR #59 and retains its finding identifiers and closure issues where applicable.

## Executive conclusion

ZenoFCIS has substantial composition and correctness-by-construction machinery, but the two claims must be scoped precisely.

### Existing composition capability

The baseline already contains:

- hierarchical read/write/context/effect paths;
- component assumptions, guarantees, frames, and typed wiring;
- directional frame authorization after PR #60;
- deterministic merge-order declarations;
- an external evidence-verifier boundary;
- state read/write conflict detection.

This is a real compositional-contract layer, not only documentation.

### Existing correctness-by-construction capability

The baseline also contains a strong structural construction pipeline:

```text
closed schema and stable identifiers
-> schema-admitted command/context/state witnesses
-> generated typed reason/effect/channel/read/update APIs
-> catalog-aware transition builder
-> preconditioned canonical patch and closed plans
-> same-candidate receipt and CommitBundle
-> catalog-authorized production publication in PR #63
```

These layers prevent many malformed or cross-boundary states from reaching production authority.

### Missing claim

The baseline does not yet establish the stronger theorem:

```text
Every authorized transition preserves every required project invariant,
conserves value according to the reviewed economic laws,
and composes equivalently under every promoted execution strategy.
```

The largest blockers are proof-carrying composition evidence, complete effect/outbox conflict semantics, first-class relational laws, exhaustive refinement evidence, and exact authenticated-state projection/proof context.

## Guarantee under review

The narrow value-moving correctness goal should be stated as:

For every admitted pre-state, command, authenticated context, principal, replay identity, catalog, provider, transition program, law set, interpreter, and deployment:

1. the decision is deterministic under the exact bound inputs;
2. ordinary rejection changes no authoritative state and creates no effect/outbox authority;
3. an accepted or committed-failure result has one internally consistent candidate identity;
4. the post-state satisfies the complete required invariant set;
5. every state delta and external effect satisfies the complete relational law set;
6. every composed or parallel execution equals the canonical sequential semantics;
7. authenticated roots and proofs refer to the exact projector/tree/profile/version/candidate context;
8. the imperative shell publishes and interprets only a nominal authorized witness;
9. replay, crash recovery, and outbox delivery preserve those identities;
10. promotion evidence is bound to exact source, tools, coverage, artifacts, and deployment.

No existing single API proves all ten clauses.

## Findings

### CMP-001 — Effects omitted from default parallel conflict authorization

**Severity:** P1  
**Status on baseline:** open; tracked by issue #56  
**Affected area:** `zeno-fcis-compose::conflicts`

The baseline conflict relation checks state write/write and write/read overlap only. Two components with disjoint state paths but nonempty value-moving effects can be promoted as conflict free.

**Exploit sketch:**

- component A emits a debit or transfer;
- component B emits a mint, fee, callback, or second transfer;
- state footprints are disjoint;
- effect interpretation is order sensitive;
- the parallel verifier sees no conflict and accepts equal caller-supplied result hashes.

**Blast radius:** any application treating the composition report as authority for concurrent effects, especially financial, storage settlement, messaging, workflow execution, or OS device operations.

**Required repair:** effects conflict by default whenever both sides are nonempty. One exact pair/kind may be waived only by a reviewed commutativity law and independent evidence.

### CMP-002 — Outbox semantics absent from component footprint and conflict model

**Severity:** P1  
**Status on baseline:** open; tracked by issue #56

Outbox channels, destinations, ordering, retry identities, and acknowledgement behavior are not represented in component composition footprints.

**Exploit sketch:** two state-disjoint transitions enqueue order-sensitive instructions to the same destination. A parallel result may have the same state root while producing a different externally observed order or idempotency interaction.

**Required repair:** add an explicit outbox channel/destination footprint. Default to conflicts for outbox/outbox and effect/outbox combinations unless a verified law establishes canonical equivalence.

### CMP-003 — Sequential parity is self-asserted hash equality

**Severity:** P1  
**Status on baseline:** open; tracked by issue #56

`CompositionEvidence` accepts caller-supplied sequential and composed commitments. `verify_deterministic_parallel` checks only their equality. The hashes are not bound to a composition specification, source revision, input domain, coverage, partition plan, merge order, result artifact, or verifier.

**Exploit:** provide the same arbitrary nonzero hash in both fields.

**Required repair:** replace raw hashes with a canonical parity claim and independently verified artifact binding the complete execution and coverage context.

### CMP-004 — Assumption-discharge evidence is replayable across provider sets

**Severity:** P1  
**Status on baseline:** open; tracked by issue #56

The baseline verifier checks the discharge artifact against only the assumption claim. Provider guarantee hashes are checked for global existence but are not part of the statement passed to the verifier, and provider component identities are absent.

**Exploit:** retain the old artifact while substituting another existing guarantee list.

**Required repair:** bind the exact sorted `(provider component, guarantee claim)` set and exact composition specification into the verified statement.

### CMP-005 — Composition specification lacks a canonical authority identity

**Severity:** P1  
**Status on baseline:** open

The baseline composition values do not all implement canonical encoding, and the verifier does not bind every theorem to an exact specification commitment.

**Impact:** evidence reuse after changes to footprints, wiring, components, merge order, or conflict waivers.

**Required repair:** canonical specification bytes and a domain-separated commitment covering every authority-bearing field.

### CMP-006 — Observed footprints can be mistaken for static complete footprints

**Severity:** P1 for production parallel authorization  
**Status:** open after composition v2

The transition builder returns an execution-observed footprint. A component contract is intended to contain a static complete over-approximation. The baseline type system does not prove the relationship:

```text
observed_footprint(input) subset_of declared_complete_footprint
```

for every legal input.

**Exploit:** a retained test case does not execute a rare effect/path; the observed footprint is used as though it were complete; two tasks are promoted in a case where the rare path conflicts.

**Required repair:** a nominal footprint-completeness witness generated from closed control flow, statically verified, exhaustively enumerated under an exact domain manifest, or theorem-checked.

### CBC-001 — Project invariants and conservation laws are identifiers, not enforced relations

**Severity:** P1  
**Status:** open; tracked by issue #58

Schemas and catalog policy hashes identify shape and reviewed policy, but no generic layer checks relations among:

```text
pre-state
command/context
post-state
patch
commit effects
outbox entries
decision/reason
```

A transition can be type correct while transferring an amount that differs from the state debit, minting without the matching supply delta, using a different asset or recipient, or losing value through an unmodeled rounding remainder.

**Required repair:** first-class canonical law definitions, a complete required law set bound to the catalog/profile, executable and proof-backed checkers, and a nominal law-verified transition required by the production authorization port.

### CBC-002 — No mechanized soundness theorem for the construction calculus

**Severity:** P2  
**Status:** open

ZenoFCIS has many correct-by-construction mechanisms, but their soundness is presently established by Rust encapsulation, tests, differential evidence, and review rather than one mechanized theorem connecting generated APIs, catalog validation, transition sealing, law checking, authorization, and shell publication.

A desirable theorem is:

```text
construct(program, reviewed_spec, admitted_input) = Authorized(t)
    implies
well_formed(t)
and catalog_valid(t)
and laws_hold(t)
and invocation_bound(t)
and shell_safe(t)
```

**Required repair:** define the construction judgment and refinement relations in a proof assistant or proof-oriented IR, then translation-validate the Rust implementation.

### REF-001 — Exhaustive refinement can be fabricated by cardinality

**Severity:** P1  
**Status:** open; tracked by issue #61

The baseline exhaustive promotion checks case count against cardinality but does not verify exact unique domain enumeration. Repeated inputs can satisfy the count. Raw normalized decisions can also be self-consistently fabricated without strict artifact reconstruction.

**Required repair:** canonical domain manifests, unique exact input membership, deterministic enumeration identity, independently verified coverage, and privately constructible validated normalized decisions.

### AUTH-001 — Production authorization is not yet law aware

**Severity:** P1  
**Status:** open relative to PR #63

PR #63 closes the raw-bundle publication path by minting a nominal catalog-authorized transition. Its stated nonclaim is that it does not prove business invariants or conservation.

**Required repair:** the authorization policy must own the exact required law set and verifier/checker identity and mint authority only after all laws hold for the complete transition artifact.

### AST-001 — Authenticated projector and sparse-proof context are under-bound

**Severity:** P2, P1 when proofs authorize value movement  
**Status:** open; tracked by issue #62

The authenticated profile does not bind an exact projector implementation/semantic commitment. Sparse proofs omit complete tree/projector context and return a boolean rather than a nominal context-bound witness.

**Required repair:** projector identity, exact tree/profile/version/root/key context, privately constructed verified proof witnesses, and binding of authenticated update plans to the exact authorized candidate.

### EVD-001 — Structural evidence adapters remain weaker than proof checking

**Severity:** P2  
**Status:** partially open

Canonical evidence envelopes and checked backend protocols are strong transport/identity boundaries. Production use still requires concrete artifact checkers for each proof/replay tool. A nonzero artifact hash or structural checker does not establish theorem truth.

## Composition v2 implemented by the companion branch

The companion implementation introduces:

- canonical composition specification and claim commitments;
- exact provider-component guarantee bindings;
- effect/outbox footprints and conservative conflicts;
- exact conflict-waiver laws;
- structured statements delivered to the evidence verifier;
- full parallel verification context;
- independently verified sequential/parallel parity;
- negative tests for provider substitution, missing parity, effect/outbox conflicts, and context mutation.

It closes CMP-001 through CMP-005 within the generic composition package. CMP-006 remains a separate static-completeness obligation.

## Required closure order

1. Merge and validate catalog-authorized production authority, PR #63.
2. Merge proof-carrying composition v2 and close the remaining issue #56 items.
3. Add first-class invariant/conservation law definitions and checkers, issue #58.
4. Stack law-aware authorization on PR #63 so the production witness cannot exist without required laws.
5. Repair exhaustive refinement and validated decision reconstruction, issue #61.
6. Bind authenticated projectors and proof context, issue #62.
7. Complete strict decoding and SQLite row-set cross-validation, issue #55.
8. Mount project-specific laws and full-decision parity across Rust, Python, and selected chain interpreters.
9. Produce independent proof/audit evidence and rerun the audit at one exact release head.

## Adversarial and mutation test backlog

### Composition mutations

- Remove one effect or outbox path from a declared complete footprint.
- Substitute a provider component while retaining the same guarantee hash.
- Substitute a guarantee while retaining the discharge artifact.
- Change wiring schema, destination, or source effect.
- Change merge order without changing result hashes.
- Change source revision, domain, partition plan, algorithm, or toolchain in parity context.
- Supply equal arbitrary result hashes without parity evidence.
- Introduce two disjoint state tasks with order-sensitive effects.
- Introduce same-destination outbox entries with different payload/order.
- Declare a commutativity law without evidence or for the wrong conflict kind.

### Relational-law mutations

- Change one debit, credit, transfer, fee, mint, burn, or supply delta.
- Change asset, subject, authority, or recipient independently.
- Remove one aggregate effect while preserving individual validity.
- Move rounding remainder to an unapproved sink.
- Add an effect to ordinary rejection.
- Add an unapproved effect to committed failure.
- Use a law artifact from another profile, source revision, or transition candidate.

### Refinement/authenticated-state mutations

- Repeat one input under multiple case IDs in exhaustive evidence.
- Replace the domain hash while preserving cardinality.
- Mutate candidate, patch, plan, receipt, and bundle consistently on both sides.
- Replay a sparse proof under another tree ID, projector, profile, version, root, or key.
- Use a projector that omits one economically relevant field.

## Claim posture after closure

Even after all source packages are implemented, the strongest honest claim is scoped:

```text
For the exact reviewed project profile, transition program, law set,
composition spec, provider, source revision, toolchain, runtime,
interpreter, and deployment represented by the retained evidence,
every admitted value-moving transition satisfies the stated invariants,
relational laws, composition theorem, and publication refinement.
```

It is not a claim about unspecified projects, arbitrary integrations, unmodeled hardware behavior, an unsound verifier, or future code revisions.
