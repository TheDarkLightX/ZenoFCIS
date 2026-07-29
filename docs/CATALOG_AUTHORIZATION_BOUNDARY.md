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
+ verified project-law set and exact runtime law-engine type
+ schema-admitted pre-state, command, and authenticated context
+ principal and authentication-evidence commitments
+ replay identity
+ execution by the shell-owned reviewed transition program
    -> complete structural transition validation
    -> fresh evaluation of every applicable project law
    -> CatalogAuthorizationDecision
    -> CatalogAuthorizedTransition for Accept or CommittedFailure only
```

Only `CatalogAuthorizedTransition` may cross a production commit port.

Store creation is a separate authority path and does not add a fourth decision
kind:

```text
exact ProjectCatalog/provider/state domain/execution policy
+ GenesisPolicyBinding
+ complete verified project-law set
+ schema-admitted reviewed initial state
    -> exact root comparison
    -> complete genesis-law evaluation
    -> private CatalogAuthorizedGenesis
    -> one-time authorized shell creation
```

Existing SQLite stores reopen without accepting caller-supplied initial state.

## Inputs

- A `ProjectCatalog` reconstructed under a sealed approved commitment provider.
- A known-answer-verified provider token.
- An owned state-domain name and version.
- Nonzero commitments to the reviewed transition build, provider build
  evidence, effect interpreter, deployment, and replay policy.
- A `GenesisPolicyBinding` containing the expected initial root, reviewed
  source/configuration/evidence commitments, and unique deployment instance.
- Schema-admitted pre-state, command, and authenticated-context envelopes.
- Nonzero principal, authentication-evidence, and replay commitments.
- A complete `VerifiedProjectLaws` value binding the catalog, law manifest,
  retained formal evidence, runtime checker build, and independent evidence
  verifier.
- The exact nominal transition-program, project-law engine, and interpreter
  types owned by the commit authority. Callers cannot submit a prebuilt
  `TransitionDecision` or select a checker per invocation.

The command and context commitments are rederived from the externally supplied
envelopes. They are never copied from the decision being validated.

## Outputs

- `CatalogAuthorizedTransition<H, P, L, I>` for `Accept` and
  `CommittedFailure`.
- A noncommittable catalog-authorized rejection value for `Reject`.
- A versioned `AuthorizationId` that binds the exact policy, invocation, law
  set, law evaluation, candidate, bundle, decision class, reason, and roots.
- Exact canonical authorization bytes for persistence and replay comparison.
- `CatalogAuthorizedGenesis<H, P, L, I>` for one exact reviewed initial state,
  root, policy, and complete genesis-law evaluation.
- A content-addressed `GenesisId` and canonical genesis authorization bytes for
  persistent reopen validation.

The existing `CandidateId` remains the implementation-neutral semantic
identity. `AuthorizationId` is the deployment-specific production identity.

## Authority Boundary

`CatalogAuthorizedTransition` has private fields and has no `Default`, decoder,
`Deref`, public from-parts constructor, or conversion from `CommitBundle` or
`NormalizedDecision`. A raw normalized decision is untrusted transport;
`ValidatedNormalizedDecision` can be constructed only through strict
receipt/bundle reconstruction against an exact invocation, pre-state, state
domain, and approved provider. Commit authority is possible only after complete validation
under an `AuthorizationPolicy<H, P, L, I>` created with a
`VerifiedProvider<H>` and `VerifiedProjectLaws<H, L>`. `P` is invoked inside
`CatalogCommitAuthority::execute`; the decision is never an external
constructor input. `L` is carried nominally through the policy, invocation,
authorization, bound interpreter, and shell, preventing a different checker
type from minting a value accepted by that commit port.

The law manifest and evidence protocol are tool-neutral. Public deployments
can mount Lean, Z3, CVC5, Kani, Flux, or other checked adapters. An owner with
private ESSO access can mount an ESSO checker in a private crate through the
same interfaces; public ZenoFCIS has no ESSO dependency or universal ESSO
requirement.

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
- The reviewed project-law definitions, per-invocation law engine, and any
  independently mounted retained-evidence verifier selected by the release.
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
- Law definitions, retained evidence bytes, and per-invocation observations
  retain `LawLimits`; the default maximum retained artifact volume is 64 MiB
  and the hard maximum law/observation count is 4,096.
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
13. Law completeness: every law family is required or explicitly inapplicable;
    every applicable law is evaluated exactly once for the complete invocation
    and decision surface.
14. Law fail-closure: a missing, extra, duplicate, violated, indeterminate, or
    engine-failed observation prevents construction of authorization.
15. Law identity: the policy and authorization body bind the exact law-set,
    runtime engine build, evidence verifier, and per-invocation evaluation.
16. Nominal checker: a different law-engine type cannot mint an authorization,
    interpreter token, or shell state of the selected production type.
17. Genesis non-bypass: no authorized shell creation API accepts a raw admitted
    state without `CatalogAuthorizedGenesis`.
18. Genesis exactness: initial root, policy, source/configuration/evidence,
    deployment instance, law set, and complete genesis evaluation are bound by
    the `GenesisId`.
19. Create/reopen separation: creation consumes the nominal genesis witness;
    SQLite reopen accepts no initial-state replacement and reconstructs the
    expected witness from persisted canonical state under the current authority.

## Negative Cases

Validation fails for a changed command, context, principal, authentication
evidence, replay identity, pre-state, catalog, profile, precedence, state
domain, provider, provider-build evidence, transition build, interpreter,
deployment, replay policy, candidate, bundle, decision class, reason, or root.
Validation also fails for a mismatched law catalog or source binding, changed
law manifest, changed engine/verifier identity, incomplete observation set, or
any violated or indeterminate law.

Genesis authorization additionally fails for a changed initial root or state,
source/configuration/evidence/deployment-instance binding, missing or extra
genesis law observation, or violated/indeterminate genesis law.

SQLite additionally fails closed for a policy mismatch, replay collision,
candidate-to-authorization collision, populated legacy database without
authorization records, partial transaction, or corrupted stored identity.

## Assumptions

- The ingress layer has authenticated the context and principal represented by
  the supplied commitments.
- Owner review selected the exact transition, project-law engine, retained
  evidence verifier, interpreter, deployment, and replay-policy commitments.
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
- The framework requires relational laws before authorization, but it does not
  invent a project's conservation equations or prove that a dishonest checker
  implementation matches its manifest. Each promoted profile still needs
  independently reviewed definitions, checkers, and retained proof evidence
  at the coverage level it claims.
- SQLite schema v5 reconstructs exact decoded authorization, bundle, receipt,
  replay, and outbox set equality before replay or delivery. This does not
  qualify the host filesystem or a production destination.
- [Flux: Liquid Types for Rust](https://doi.org/10.1145/3591283) is relevant
  to future refinement checking inside reviewed Rust implementations. A Flux
  proof would not by itself establish ingress authentication, runtime
  provenance, interpreter identity, deployment binding, or SQLite row-set
  completeness.
- The composition and exhaustive-refinement structural findings are closed by
  proof-carrying composition and validated manifest-backed promotion. Strict
  authenticated transport, projector qualification, context-bound proofs, and
  candidate-bound authenticated publication are implemented by the
  authenticated authority layer. Production datastore qualification and the
  concrete Solidity/Solana deployment findings remain project-specific work.
- Passing bounded tests is not an unbounded proof, independent audit, or a
  production-readiness claim.
