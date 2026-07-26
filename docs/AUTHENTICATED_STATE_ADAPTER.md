# Authenticated-state adapter

## Boundary

`zeno-fcis-authenticated` plans a versioned sparse authenticated index behind
an already-authoritative `CanonicalPatch`. The semantic state, ZCVE bytes, and
semantic pre/post roots remain authoritative. Tree roots are a separate,
explicit dual-root profile and cannot replace an existing ZenoDEX root.

The public boundary contains `TreeReader`, `TreeWriter`,
`PlannedAuthenticatedCommit`, `NodeBatch`, `StaleNodeCandidate`, and bounded
membership/absence proofs. A reviewed `StateProjector` defines the complete
mapping from semantic state to logical authenticated leaves.

## Inputs and outputs

- input: immutable pre-state, expected tree snapshot/version, canonical patch,
  state domain, reviewed projector, and authenticated profile;
- output: applied semantic post-state plus a plan binding tree/profile/version,
  semantic roots, patch hash, authenticated roots, logical node batch, stale
  candidates, and the complete projected post-state;
- commit: compare expected profile/version/root, then publish the complete plan.

## Laws

- applying a plan to the expected snapshot equals a full rebuild from the
  projected post-state;
- equal logical leaves have equal roots regardless of insertion history;
- stale version, root, profile, semantic pre-root, or incomplete projection
  fails without mutation;
- membership and absence proofs bind the exact key, value, root, profile hash,
  and version;
- deleted leaves become explicit stale-node candidates;
- semantic root and authenticated root are always separate fields.

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

## Nonclaims

The included sparse-tree backend is a deterministic reference and test oracle,
not a vetted production JMT datastore. It performs full logical rebuilds when
planning. It does not provide pruning authority, crash-atomic database commit,
or migration for an existing single-root profile. Those require a mounted JMT
implementation, the concrete shell transaction, and explicit activation of a
dual-root profile.
