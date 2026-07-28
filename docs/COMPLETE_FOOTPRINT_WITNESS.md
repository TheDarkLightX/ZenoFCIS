# Complete static footprint witnesses

## Purpose

An execution-observed footprint describes one transition run. A deterministic-
parallel contract needs a static declaration that covers every admitted run,
including rare rejection and committed-failure branches. ZenoFCIS now makes
that distinction explicit:

```text
authority-owned component binding
+ closed proof method
+ explicit Accept / Reject / CommittedFailure coverage
+ retained artifact
+ pinned verifier
    -> CompleteFootprintWitness

CompositionSpec
+ exact authority bindings
+ one untrusted footprint-evidence item per component
+ authority-selected footprint verifier
+ ordinary composition evidence
+ sequential/parallel parity
    -> DeterministicParallelAuthorization
```

The authorization is a planning and promotion value. It does not create
threads, schedule work, or interpret effects.

## Inputs

`FootprintAuthorityBinding` is selected by the production authority and binds:

- component and project-profile identity;
- transition-program build;
- declared read, write, authenticated-context, effect, and outbox footprints;
- schema, catalog, transition-algorithm, and source-revision identities;
- approved proof/checker toolchain;
- approved verifier identity.

`FootprintCompletenessClaim` adds:

- one closed proof method;
- an explicit status for `Accept`, `Reject`, and `CommittedFailure`;
- the exact theorem, query, derivation, or coverage identity.

The supported proof methods are:

1. generated closed control flow with a mechanically derived footprint;
2. pinned static analysis;
3. exhaustive finite enumeration under `ExhaustiveFootprintDomain`;
4. an independently checked theorem.

There is no bounded-test proof method. Bounded tests can produce useful
evidence, but they cannot be labeled exhaustive through this API.

An exhaustive domain contains the reviewed domain definition, exact enumeration
algorithm, and a sorted unique nonzero set of canonical input commitments.
Duplicate case identifiers cannot pad the cardinality.

## Outputs

`verify_complete_footprint` returns `CompleteFootprintWitness` only when:

```text
evidence.claim.binding == authority-selected binding
evidence.verifier == authority-approved verifier
running verifier identity == authority-approved verifier
verifier accepts the exact claim and artifact
```

The witness has private fields and canonical bytes under
`FOOTPRINT_WITNESS_FORMAT_VERSION = 1`.

`authorize_deterministic_parallel` accepts untrusted footprint evidence, invokes
the authority-selected verifier, and returns `DeterministicParallelAuthorization`
only when:

- the authority binding set is an exact duplicate-free component set;
- the evidence set is an exact duplicate-free component set;
- every authority binding exactly matches its `CompositionSpec` component;
- every evidence claim exactly matches its authority binding;
- every verified claim mints exactly one private-field witness;
- ordinary assume-guarantee, frame, wiring, conflict-law, and parity checks pass.

The authorization commitment includes the exact composition identity, parallel
verification context, and canonical component-ordered witness set.

## Authority boundary

The project or deployment authority owns:

- the expected `FootprintAuthorityBinding` values;
- the approved verifier implementation and identity;
- the source, program, schema, catalog, algorithm, and toolchain commitments;
- the `CompositionSpec`, parallel context, and evidence policy.

Callers may propose evidence. They do not choose the authority bindings,
approved verifier, minted witnesses, or successful authorization. The parallel
authorization API does not accept caller-supplied witnesses.

ESSO is an optional private checker and can implement
`FootprintEvidenceVerifier` in a private adapter. Lean, SMT/Z3, CVC5, Kani,
Flux, static analyzers, and generated-control-flow checkers can implement the
same public trait. ZenoFCIS does not require or embed any one checker.

The trait boundary cannot prove that an external verifier implementation is
sound or that its identity honestly describes its binary. Production
deployments must pin, attest, and independently qualify the selected verifier.

## Trusted dependencies

This package adds no dependency. It uses:

- `zeno-fcis-codec` for canonical encoding and domain-separated commitments;
- the existing immutable composition values and `CommitmentHasher` boundary;
- the authority-selected external verifier for proof truth.

External verifier types never become protocol-facing ZenoFCIS types.

## Deterministic resource bounds

- At most 4,096 component bindings and witnesses, inherited from
  `CompositionSpec`.
- At most 4,096 paths per read, write, context, effect, or outbox set.
- At most 64 atoms per path.
- At most 16,384 exact inputs in an exhaustive finite-domain manifest.
- Witness and authority sets are sorted once and compared linearly after sort.
- Per-execution `covers_observed` checks are bounded by declared-path count
  multiplied by observed-path count.
- No clock, randomness, filesystem, network, database, thread, async runtime,
  mutable global state, or interior mutability enters verification.

## Laws

For every component `c` and admitted input `i`, the claimed theorem is:

```text
observed_reads(c, i)    subset_of declared_reads(c)
observed_writes(c, i)   subset_of declared_writes(c)
observed_context(c, i)  subset_of declared_context(c)
observed_effects(c, i)  subset_of declared_effects(c)
observed_outbox(c, i)   subset_of declared_outbox(c)
```

Coverage includes ordinary rejection and committed failure. A decision class
may be marked `ProvedUnreachable`, which itself is part of the verified claim.

Changing any bound component, profile, program, path, map-key/destination,
schema, catalog, algorithm, source revision, toolchain, verifier, method,
coverage identity, decision-class status, or artifact changes or invalidates
the witness.

Production deterministic-parallel authorization requires:

```text
for every component in CompositionSpec:
    exactly one authority binding
    exactly one CompleteFootprintWitness
    witness.binding == authority binding
    binding.profile == component.profile
    binding.footprint == component.footprint
    binding.outbox == component.outbox
```

## Negative cases

Permanent tests reject:

- a rare effect absent from the declaration;
- a committed-failure outbox destination absent from the declaration;
- a changed map-key or destination identity;
- duplicate or empty exhaustive input domains;
- changes to component, profile, program, footprint, outbox, schema, catalog,
  algorithm, source revision, proof toolchain, or verifier;
- a verifier-identity mismatch or rejected proof;
- omission, duplication, or staleness in the footprint-evidence set;
- an authority binding that differs from the exact composition contract;
- mutation of one decision class from covered to proved unreachable without
  changing the claim identity.

## Assumptions

- The authority supplies independently selected expected bindings.
- Canonical input commitments in an exhaustive manifest represent the complete
  reviewed finite domain under the named enumeration algorithm.
- The chosen verifier checks the exact proof-method semantics and retained
  artifact, including all operation sites and all decision classes.
- `AccessPath::covers` remains the normative directional containment relation.
- The selected `CommitmentHasher` satisfies its declared algorithm contract.

## Explicit nonclaims

- A witness is not a mechanized theorem unless its selected proof method and
  verifier actually provide one.
- An execution-observed footprint or a passing bounded test suite is not a
  complete static footprint proof.
- ZenoFCIS does not ship ESSO, Lean, Z3, CVC5, Kani, Flux, or another checker.
- ZenoFCIS does not establish soundness of an external verifier or authenticate
  its binary by itself.
- The authorization does not prove conflict commutativity or sequential parity;
  those remain separate mandatory composition obligations.
- The authorization does not provide a concurrent scheduler, threaded shell,
  liveness, fairness, side-channel resistance, deployment qualification, or
  project business-law correctness.
- This package does not change ZCVE/1, existing composition-v2 bytes, schemas,
  stable identifiers, or rejection precedence. RC packaging changes Cargo
  package versions only.
