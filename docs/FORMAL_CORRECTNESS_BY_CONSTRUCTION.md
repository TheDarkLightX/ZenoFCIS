# Formal correctness by construction

## Terminology

In this document, **CbC** means correctness by construction: a program or transition is assembled through rules whose successful outputs satisfy a declared specification, rather than being constructed first and checked only afterward.

ZenoFCIS already implements important structural CbC rules:

```text
closed schemas
+ stable identifiers
+ generated typed APIs
+ catalog admission
+ preconditioned patches
+ closed effect/outbox plans
+ same-candidate receipts and bundles
+ nominal catalog authorization
```

Those mechanisms establish shape, identity, and boundary consistency. They do not automatically establish project-specific relations such as conservation, debit/credit equality, mint/burn authority, or fee rounding.

`zeno-fcis-cbc` supplies that missing relational-law layer.

## Construction judgment

The intended production judgment is:

```text
reviewed catalog
+ exact transition build
+ complete required LawSet
+ admitted pre-state, command, and context
+ reviewed transition program
+ project LawChecker
+ independent LawEvidenceVerifier
    -> LawVerifiedTransition
```

A `LawVerifiedTransition` has no public constructor. It can exist only after:

1. the complete three-way transition is revalidated;
2. the authoritative post-state is reconstructed by applying the exact sealed bundle;
3. the required law set is shown to match every `Claim` registry entry in the profile;
4. every law applicable to the decision class is evaluated;
5. all required executable checks succeed;
6. all required exact evidence claims are independently accepted;
7. the successful evaluation report is content addressed.

A production commit authority should accept the nominal witness rather than a raw `TransitionDecision` or `CommitBundle`.

## Profile-registered law completeness

A `LawSet` is not an arbitrary subset chosen by a caller. Its definitions must exactly reconstruct all profile entries in `RegistryKind::Claim`:

```text
law id == registry id
law stable name == registry stable name
law statement hash == registry definition hash
number of laws == number of profile Claim entries
```

This prevents a caller from silently omitting an inconvenient invariant while retaining the same project profile.

The law set additionally binds:

- exact profile commitment;
- exact schema commitment;
- exact catalog commitment;
- profile policy/invariant commitment;
- exact reviewed transition-build commitment.

## Law classes

The common registry classifies laws without defining project mathematics:

- state invariant;
- transition relation;
- value conservation;
- mint/burn and total-supply relation;
- debit/credit/effect equality;
- fees, fixed-point arithmetic, dust, and rounding;
- capability or principal authority;
- ordinary rejection purity;
- permitted committed-failure behavior;
- composition and sequential/parallel equivalence;
- custom project relation.

A project should use the narrowest applicable class and retain the exact mathematical or executable statement identified by `statement_hash`.

## Decision scope

Laws explicitly apply to:

- every decision;
- accept only;
- reject only;
- committed failure only;
- every committed decision.

Evaluation fails closed when no law applies to the current decision class. A project therefore cannot define accepted-value laws while leaving rejections or committed failures unconstrained.

Recommended minimum coverage:

```text
Reject:
    unchanged state/root
    no candidate
    empty commit and outbox plans
    stable reason/precedence

Accept:
    all state invariants
    command-specific transition relation
    value conservation
    effect/state equality
    authority law
    arithmetic/rounding laws

CommittedFailure:
    all post-state invariants
    exact allowed failure transition
    exact allowed effects/outbox
    consumed one-shot authority rules
```

## Complete law subject

`LawSubject` is privately reconstructed from a validated transition and includes:

- profile, schema, catalog, and transition-build identities;
- decision class and stable reason identity;
- command and authenticated-context commitments;
- pre-root and post-root;
- candidate identity;
- complete pre-state and reconstructed post-state;
- exact canonical patch;
- exact authoritative commit plan;
- exact outbox plan.

Evidence is therefore bound to the complete relation, not only to a post-state hash or receipt.

## Executable checks

A project-owned pure `LawChecker` receives the exact law definition and `LawSubject`. It returns:

```text
Satisfied
Violated(counterexample commitment)
Indeterminate(reason commitment)
```

Violation and indeterminate results block promotion. Zero placeholder commitments also block promotion.

The checker is part of the trusted computing base unless its result is independently verified. High-assurance laws should use `ExecutableAndEvidence` so that a fast executable checker and an independent proof/replay surface must agree.

## Proof and replay evidence

`LawEvidence` binds:

- exact law identity;
- input-domain or theorem-coverage commitment;
- exact verifier/toolchain commitment;
- retained artifact commitment.

`LawClaim` additionally binds:

- complete law-set identity;
- complete law-subject identity;
- profile/schema/catalog/build identities;
- law class, scope, and mathematical statement;
- decision, roots, command/context, and candidate identity;
- coverage and toolchain.

Changing the pre-state, post-state, amount, asset, recipient, authority, effect, outbox entry, candidate, build, coverage, or verifier identity changes the exact claim.

`LawEvidenceVerifier` must validate the retained artifact against that complete claim. Artifact existence or structural JSON validation is insufficient.

## Nominal witness and authority integration

The successful result is a `LawVerifiedTransition`. The wrapper owns:

- the exact transition decision;
- law-set identity;
- subject identity;
- successful report identity.

The production stack should become:

```text
ReviewedTransitionProgram
    -> TransitionDecision
    -> verify_transition_laws
    -> LawVerifiedTransition
    -> CatalogCommitAuthority
    -> CatalogAuthorizedTransition
    -> ProductionShell
```

The generic law crate does not modify the authority API in its first package because the authority work is currently reviewed in a separate PR. A stacked integration package must make the authority own the exact law set, checker type, verifier type, and evidence source, and must remove any path that authorizes a raw decision.

## Project law examples

### ZenoDEX

```text
total_debt == free_debt + stability_pool_debt
collateral and debt deltas match the selected command
minted zUSD == approved debt/supply delta
burned zUSD == approved debt/supply delta
transfer effects equal account debits/credits
liquidation redistribution conserves claims
fees and rounding remainder flow only to approved sinks
oracle authority/freshness required for each price-dependent command
```

### ZenoStorage

```text
provider obligation cannot exceed paid/escrowed commitment
repair preserves object/package identity
availability evidence refers to the exact stored version
settlement effects equal agreement state deltas
key-capsule authority follows the reviewed ownership graph
```

### ZenoMail

```text
message envelope identity is immutable
mailbox counters and acknowledgement effects agree
key epoch is monotonic
revoked devices cannot emit authorized send/decrypt effects
storage and notification plans refer to the same message candidate
```

### PopperPad and Helix

```text
claim/evidence provenance is append only
supersession does not silently delete prior authority
quarantine/refutation state and publication effects agree
agent proposals cannot become effects without the required approval capability
redacted audit output preserves the approved disclosure policy
```

### LucyOS

```text
capability derivation cannot amplify rights
every mapped frame is owned and typed correctly
IPC transfer follows endpoint and grant authority
scheduler budgets and replenishments conserve time authority
machine effects match abstract kernel-state transitions
cross-core execution refines canonical sequential/distributed semantics
```

## Mutation obligations

Every project law set should be mutation tested by changing independently:

- amount;
- asset or resource identity;
- principal, capability, or recipient;
- state debit or credit;
- effect payload;
- outbox destination or payload;
- fee or rounding remainder;
- candidate identity;
- decision class or reason;
- source revision;
- coverage or toolchain binding.

A production suite should kill every mutation through the executable checker, evidence verifier, or both.

## Formal soundness target

The long-term mechanized theorem should connect the construction calculus to production authority:

```text
verify_transition_laws(spec, input, decision, evidence) = Verified(w)
and authorize(policy, w) = Authorized(a)
and publish(shell, a) = committed
    implies
well_formed(decision)
and complete_required_laws_hold(decision)
and invocation_is_bound(decision)
and publication_refines_reference_semantics(decision)
```

The Rust implementation and tests provide executable evidence for the design. They are not yet a mechanized proof of that theorem.

## Explicit nonclaims

This package does not:

- invent project economics or invariants;
- prove a project-supplied checker correct;
- make a hash commitment equivalent to a proof;
- establish exhaustive input coverage without a valid domain/theorem artifact;
- integrate automatically with the open production-authority PR;
- repair authenticated projector/proof context or runtime refinement coverage;
- grant production authority by itself.
