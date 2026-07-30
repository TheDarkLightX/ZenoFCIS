# Authenticated-state adapter

## Boundary

`zeno-fcis-authenticated` plans a versioned sparse authenticated index behind
an already-authoritative `CanonicalPatch`. The semantic state, ZCVE bytes, and
semantic pre/post roots remain authoritative. Tree roots are a separate,
explicit dual-root profile and cannot replace an existing ZenoDEX root.

The public boundary contains `TreeReader`, `TreeWriter`,
`PlannedAuthenticatedCommit`, `DecodedAuthenticatedPlan`, `NodeBatch`,
`StaleNodeCandidate`, bounded membership/absence proofs, caller-supplied
`SparseProofContext`, and the
private-construction `ContextVerifiedSparseProof` witness. A configured
`AuthenticatedStatePlanner` owns one `StateProjector` across requests so a
request-time caller cannot substitute another implementation. Its declared
commitment is part of `AuthenticatedProfile`.

## Inputs and outputs

- configured input: authenticated profile and setup-selected projector;
- request input: immutable pre-state, expected tree snapshot/version,
  canonical patch, and state domain;
- output: applied semantic post-state plus a plan binding tree/profile/version,
  semantic roots, patch hash, authenticated roots, logical node batch, stale
  candidates, and the complete projected post-state;
- commit: compare expected profile/version/root, then publish the complete plan.

## Laws

- applying a plan to the expected snapshot equals a full rebuild from the
  projected post-state;
- update planning rejects a projector whose declared commitment differs from
  the mounted authenticated profile;
- equal logical leaves have equal roots regardless of insertion history;
- stale version, root, profile, semantic pre-root, or incomplete projection
  fails without mutation;
- membership and absence witnesses bind the exact key, value, tree, profile,
  declared projector commitment, root, and version supplied to the verifier;
- deleted leaves become explicit stale-node candidates;
- semantic root and authenticated root are always separate fields;
- sparse proofs and authenticated plans admit persisted bytes only through
  strict bounded, complete, canonical decoders;
- decoded plans remain non-authoritative transport and cannot be applied.

## Bounds

The reference backend admits at most 4,096 projected leaves. Proofs have a
fixed 256 sibling hashes. Projection, rebuild, and differential planning are
bounded by the declared leaf count; the reference implementation favors
inspectability over production throughput.

## Trusted dependencies

The tree uses the pinned RustCrypto SHA-256 provider through the ZenoFCIS
domain-separation API. Internal tree shape is not consensus encoding. A
production adapter must keep any external JMT node types private and prove that
its incremental roots and proofs match this logical reference profile.

Authenticated update-plan canonical encoding is version 2. Sparse-proof
canonical encoding is version 1. Both have strict bounded decoders. The plan
includes the projector commitment and is intentionally incompatible with the
prior pre-release encoding. See
[Authenticated sparse-proof context](AUTHENTICATED_PROOF_CONTEXT.md).
Candidate-bound production use is specified by
[Candidate-bound authenticated authority](AUTHENTICATED_AUTHORITY_BOUNDARY.md).

## Nonclaims

The included sparse-tree backend is a deterministic reference and test oracle,
not a vetted production JMT datastore. It performs full logical rebuilds when
planning. It does not provide pruning authority, crash-atomic database commit,
or migration for an existing single-root profile. Those require a mounted JMT
implementation, the concrete shell transaction, and explicit activation of a
dual-root profile.

The projector commitment alone is a declared identity, not implementation
attestation. The higher `zeno-fcis-authenticated-authority` ring requires exact
retained qualification evidence, an independently selected verifier, and a
per-transition project relation before it creates nominal publication
authority.
A context-verified proof still does not attest that its expected context came
from production authority. The bounded reference tree is not a production JMT,
and verifier or relation-engine soundness remains a deployment trust boundary.
