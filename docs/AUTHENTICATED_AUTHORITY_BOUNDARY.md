# Candidate-bound authenticated authority

## Purpose

`zeno-fcis-authenticated-authority` joins an already authorized semantic
transition to one exact authenticated-state update. It prevents a raw sparse
tree plan, a self-declared projector identity, or an internally consistent
proof from becoming production publication authority by itself.

The supported flow is:

```text
CatalogAuthorizedTransition
+ setup-qualified StateProjector
+ exact ReferenceSparseTree snapshot
+ required projection-relation evaluation
    -> CatalogAuthorizedAuthenticatedCommit
    -> ProductionAuthenticatedCommitPort
```

The crate is a higher dependency ring. The generic semantic core and reference
authenticated tree remain reusable without depending on production authority.

## Inputs

Authority setup owns:

- an `AuthenticatedProfile` containing the exact tree, profile, and declared
  projector commitments;
- the concrete `StateProjector` implementation;
- the semantic `StateDomainBinding`;
- a `ProjectorQualificationClaim` binding the reviewed projector
  specification, implementation, retained evidence, and toolchain;
- the exact retained qualification-evidence bytes;
- a `ProjectorQualificationVerifier` selected independently by deployment
  setup;
- a reviewed `ProjectionRelationEngine` for per-transition completeness and
  state/projection agreement;
- a nominal `VerifiedProvider<H>`.

Each request supplies only:

- a privately constructed `CatalogAuthorizedTransition`;
- the exact current `ReferenceSparseTree` snapshot; and
- for persisted re-entry, strict canonical plan bytes plus explicit decode
  limits.

The request cannot substitute the mounted projector, verifier identity,
relation engine, state domain, provider, or authenticated profile.

## Outputs

Successful setup produces an `AuthenticatedCommitAuthority`. Its configuration
identity commits to the state domain, complete projector qualification,
relation-engine identity, and nominal approved provider.

Successful request authorization produces
`CatalogAuthorizedAuthenticatedCommit`. This private-construction value binds:

- the exact semantic authorization and candidate;
- the exact semantic pre-state and locally reconstructed post-state;
- the candidate patch and semantic roots;
- the authenticated profile, expected version/root, successor version/root,
  node batch, and stale-node set;
- the projector qualification;
- the complete per-transition projection subject, engine, and successful
  witness;
- the authenticated-authority configuration.

Only that nominal value is accepted by `ProductionAuthenticatedCommitPort`.
Raw `PlannedAuthenticatedCommit` and `DecodedAuthenticatedPlan` values remain
inspectable reference or transport data.

## Strict persisted admission

`zeno-fcis-authenticated` now defines canonical sparse-proof format version 1
and keeps authenticated-plan format version 2. The strict decoders are:

```rust
decode_sparse_proof(bytes, limits)
decode_authenticated_plan(bytes, limits)
```

They enforce complete-input byte limits, nested ZCVE limits, fixed proof depth,
known tags, exact successor versions, bounded writes and stale nodes, unique
canonical key order, complete consumption, and byte-for-byte canonical
reconstruction.

A decoded plan has no conversion into an applicable reference plan. Persisted
reauthorization decodes it, independently reruns semantic patch application and
projection under the mounted authority, and requires exact equality with the
locally reconstructed plan.

## Authority boundary

The retained projector evidence and verification result are data. The setup
owner decides which concrete verifier and relation engine are trusted for a
deployment. A request-time caller cannot obtain a nominal authenticated commit
by supplying an arbitrary verifier result or by copying hashes into a raw plan.

`TreeWriter` remains the explicit reference/testing interpreter boundary.
Production application code should expose only
`ProductionAuthenticatedCommitPort`, whose `publish` method accepts the nominal
candidate-bound type.

## Deterministic resource bounds

- qualification evidence defaults to at most 16 MiB;
- persisted authenticated input defaults to at most 4 MiB;
- one plan admits at most 4,096 logical writes;
- one plan admits at most 4,096 stale-node candidates;
- one proof contains exactly 256 sibling hashes;
- nested values use explicit `DecodeLimits`;
- collection preallocation is bounded by remaining wire bytes;
- the reference tree admits at most 4,096 leaves.

All limits are checked before authority is created or publication occurs.

## Laws

The implementation enforces these generic laws:

1. Projector declaration equals the profile projector commitment.
2. Retained evidence bytes equal the claim's evidence commitment.
3. A nonzero, setup-selected verifier attests the exact claim and evidence.
4. The projected pre-state equals the mounted tree's complete logical leaves.
5. The semantic candidate patch applies to the exact admitted pre-state and
   yields the semantic authorization's post-root.
6. The authenticated successor equals a full reference rebuild from the
   mounted projector's post-state output.
7. The project relation engine returns `Satisfied` with a nonzero witness for
   the complete semantic/authenticated subject, and its declared identity
   remains equal to the setup-bound identity.
8. Persisted plan bytes equal strict decoding and exact local reconstruction.
9. Publication profile, configuration, expected version, and expected root
   equal the mounted port.
10. Changing the candidate, semantic authorization, patch, roots, profile,
    projector qualification, relation engine, provider, or plan changes or
    invalidates the authenticated authorization.

Project-specific relation engines must state the additional completeness and
invariant laws required by their profile. For example, a root projector may
require every accepted semantic update to produce exactly one corresponding
authenticated leaf update. A projector that omits the changed value then fails
authorization even if its self-declared hash matches the profile.

## Negative cases

Tests and compile-fail examples cover:

- wrong projector identity;
- wrong qualification profile;
- wrong evidence bytes and evidence above the configured limit;
- rejected or indeterminate qualification;
- zero verifier, engine, attestation, or projection witness identity;
- projector output that omits a changed semantic value;
- semantic pre/post-root disagreement;
- stale tree profile, version, or root;
- malformed, truncated, trailing, unknown-tag, noncanonical, over-limit, and
  non-successor persisted plans and proofs;
- persisted plan substitution after strict decoding;
- raw-plan conversion into nominal authenticated authority.

## Trusted dependencies and assumptions

The boundary assumes the selected approved provider implementation, canonical
codec, semantic authorization, reference sparse-tree model, qualification
verifier, relation engine, and deployment setup are correct for their stated
claims. Evidence identity is not evidence soundness. A verifier that attests a
false qualification or a relation engine that returns `Satisfied` for an
incorrect relation remains a trusted-component failure.

## Explicit nonclaims

This package does not provide:

- an unbounded proof that an arbitrary projector is complete;
- an automatic proof of project-specific economic or domain invariants;
- a production JMT implementation, pruning policy, migration engine, or
  database recovery protocol;
- atomic publication spanning an independent semantic database and an
  independent authenticated database;
- soundness of a caller-selected external theorem prover, verifier, or retained
  artifact;
- compiled-code, hardware, side-channel, or operational deployment
  qualification;
- production readiness for a downstream project merely because it enables the
  feature.

Those claims require concrete deployment adapters and retained, independently
reviewed evidence.
