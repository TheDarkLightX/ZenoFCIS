# Catalog Authorization Boundary

Status: design and implementation boundary for issues #54 and #57.

## Purpose

ZenoFCIS distinguishes an internally consistent `CommitBundle` from an
artifact authorized for one exact production invocation and deployment. A raw
bundle remains useful as immutable protocol data and as input to the pure
reference shell. It does not confer production commit authority.

The production boundary is:

```text
approved provider token
+ exact ProjectCatalog
+ state-domain binding
+ transition/interpreter/deployment/replay-policy bindings
+ schema-admitted pre-state, command, and authenticated context
+ principal and authentication-evidence commitments
+ replay identity
+ execution by the shell-owned reviewed transition program
    -> CatalogAuthorizationDecision
    -> CatalogAuthorizedTransition for Accept or CommittedFailure only
```

Only `CatalogAuthorizedTransition` may cross a production commit port.

## Inputs

- A `ProjectCatalog` reconstructed under a sealed approved commitment provider.
- A known-answer-verified provider token.
- An owned state-domain name and version.
- Nonzero commitments to the reviewed transition build, provider build
  evidence, effect interpreter, deployment, and replay policy.
- Schema-admitted pre-state, command, and authenticated-context envelopes.
- Nonzero principal, authentication-evidence, and replay commitments.
- The exact nominal transition-program and interpreter types owned by the
  commit authority. Callers cannot submit a prebuilt `TransitionDecision` for
  authorization.

The command and context commitments are rederived from the externally supplied
envelopes. They are never copied from the decision being validated.

## Outputs

- `CatalogAuthorizedTransition<H, P, I>` for `Accept` and
  `CommittedFailure`.
- A noncommittable catalog-authorized rejection value for `Reject`.
- A versioned `AuthorizationId` that binds the exact policy, invocation,
  candidate, bundle, decision class, reason, and roots.
- Exact canonical authorization bytes for persistence and replay comparison.

The existing `CandidateId` remains the implementation-neutral semantic
identity. `AuthorizationId` is the deployment-specific production identity.

## Authority Boundary

`CatalogAuthorizedTransition` has private fields and has no `Default`, decoder,
`Deref`, public from-parts constructor, or conversion from `CommitBundle` or
`NormalizedDecision`. Construction is possible only after complete validation
under an `AuthorizationPolicy<H, P, I>` created with a
`VerifiedProvider<H>`. `P` is invoked inside `CatalogCommitAuthority::execute`;
the decision is never an external constructor input.

The generic `CommitmentHasher` trait remains available to reference and
research APIs. Production authorization accepts only the sealed
`ApprovedCommitmentProvider` implementations supplied by `zeno-fcis-crypto`.

`zeno-fcis-shell` exposes executable reference semantics under the explicit
name `apply_reference_bundle`. It proves atomic structural application and
idempotent reference replay, not production authorization. Concrete commit
ports accept the nominal authorized value.

## Trusted Dependencies

- ZCVE/1 canonical encoding and domain separation.
- The exact pinned SHA-256 provider implementation selected by the policy.
- Schema, project-profile, catalog, transition, patch, plan, receipt, and pure
  reference-shell validation.
- For SQLite, SQLite transaction and durability behavior under the documented
  configuration.
- The ingress authenticator, reviewed transition implementation, effect
  interpreter, deployment configuration, and replay policy identified by their
  commitments.

External-library types do not appear in protocol-facing authorization values.

## Deterministic Resource Bounds

- State, command, and context values retain their schema and canonical envelope
  bounds.
- Transition patches, footprints, reasons, effects, outbox entries, state
  depth, and state nodes retain `TransitionLimits` and `CatalogLimits`.
- Domain names are nonempty ASCII and bounded by the canonical `u16` length.
- Authorization encoding adds a fixed number of hashes and bounded canonical
  blobs; it does not introduce an unbounded collection.
- SQLite authorization publication is one transaction containing one
  authorization, one replay binding, one receipt, and the already bounded
  outbox plan.

Wall-clock timeout is not protocol evidence.

## Laws

1. Raw bundle non-authority: no production commit API accepts `CommitBundle`.
2. Exact invocation: authorization compares decision command and context
   commitments with commitments rederived from the supplied envelopes.
3. Exact state: the admitted pre-state root equals the candidate pre-root and
   patch expected root.
4. Exact policy: catalog, profile, precedence, provider, state domain,
   transition build, interpreter, deployment, and replay policy all match the
   shell-pinned authorization policy.
5. Nominal provider: a third-party hasher cannot impersonate an approved
   provider by copying its textual algorithm identifier.
6. Decision closure: ordinary rejection yields no committable authorization.
7. Deployment separation: changing any policy or invocation field changes the
   `AuthorizationId` or fails validation.
8. Replay identity: the replay identity comes from the invocation witness and
   cannot be replaced at commit time.
9. Reference refinement: authorized publication applies the exact inner bundle
   through the existing expected-root reference semantics.
10. Limit ownership: the produced transition must bind the exact
    shell-selected `TransitionLimits`.
11. SQLite identity: schema version, policy, state domain, authorization,
    invocation, replay, candidate, bundle, receipt, and outbox rows publish in
    one transaction; the shell rechecks policy identity before later access.
12. Interpreter ownership: a concrete interpreter enters SQLite only through a
    private-construction `BoundInterpreter` minted by the same authority, then
    remains owned by that shell for delivery.

## Negative Cases

Validation fails for a changed command, context, principal, authentication
evidence, replay identity, pre-state, catalog, profile, precedence, state
domain, provider, provider-build evidence, transition build, interpreter,
deployment, replay policy, candidate, bundle, decision class, reason, or root.

SQLite additionally fails closed for a policy mismatch, replay collision,
candidate-to-authorization collision, populated legacy database without
authorization records, partial transaction, or corrupted stored identity.

## Assumptions

- The ingress layer has authenticated the context and principal represented by
  the supplied commitments.
- Owner review selected the exact transition, interpreter, deployment, and
  replay-policy commitments.
- Collision resistance holds for the approved SHA-256 provider.
- The concrete shell protects its database and process authority according to
  its deployment threat model.

## Explicit Nonclaims

- A nonzero authentication-evidence hash is a binding, not proof that ingress
  authentication was correct.
- A build, interpreter, or deployment hash is a binding, not binary attestation
  or refinement proof.
- Known-answer verification identifies a sealed provider and checks fixed
  vectors; it does not prove the complete compiled binary or hardware.
- This boundary does not yet prove project invariants or value conservation;
  issue #58 remains required before value-moving production promotion.
- This boundary does not reconstruct exact decoded SQLite bundle/outbox set
  equality; issue #55 remains required before delivery qualification.
- [Flux: Liquid Types for Rust](https://doi.org/10.1145/3591283) is relevant
  to future refinement checking inside reviewed Rust implementations. A Flux
  proof would not by itself establish ingress authentication, runtime
  provenance, interpreter identity, deployment binding, or SQLite row-set
  completeness.
- Composition, exhaustive-refinement, authenticated-projector, and concrete
  Solidity/Solana deployment findings remain open under issues #56, #61, and
  #62 and the chain-specific reviews.
- Passing bounded tests is not an unbounded proof, independent audit, or a
  production-readiness claim.
