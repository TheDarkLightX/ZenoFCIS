# Formal composition v2

## Purpose

ZenoFCIS composition combines independently specified functional-core components without turning integration order, concurrency, or external effects into hidden semantics.

The v2 composition boundary is proof carrying:

```text
canonical component contracts
+ complete declared state/context/effect/outbox footprints
+ assumptions, guarantees, frames, and wiring
+ exact deterministic merge order
+ conservative conflict detection
+ independently verified conflict laws
+ independently verified sequential/parallel parity
    -> verified composition report
```

A successful report is a statement about the exact supplied composition specification and evidence. It is not a theorem about other source revisions, domains, partition plans, merge orders, compilers, or deployments.

## Composition identity

A `CompositionSpec` has canonical bytes and a domain-separated commitment. The commitment includes:

- composition format and project-selected specification versions;
- every component identifier and profile commitment;
- every declared read, write, context, effect, and outbox path;
- assumptions, guarantees, frame rules, and allowed writers;
- all typed wiring and wiring schemas;
- global coupling claims;
- the exact deterministic merge order;
- every declared parallel conflict law.

Every proof statement includes the exact specification commitment. An artifact accepted for one specification therefore cannot be replayed for a specification with a changed component, provider, wiring, footprint, merge order, conflict law, or profile.

## Paths and directional authorization

`AccessPath::overlaps` is symmetric and is used only for conflict detection.

`AccessPath::covers` is directional and is used for authorization. A frame declaration authorizes a destination only when the protected path covers the complete destination path and the source component is in the exact sorted writer set.

This distinction prevents a narrow descendant frame from authorizing a broader ancestor write.

## Footprints

A component contract declares:

- state reads;
- state writes;
- authenticated-context reads;
- authoritative effects;
- outbox channel/destination paths.

The declaration is intended to be a static complete over-approximation. A transition's execution-observed footprint is useful evidence but is not, by itself, a complete static footprint. Production parallel authorization additionally requires one of:

1. generated control flow whose complete footprint is derivable from the reviewed transition definition;
2. a static analysis or proof that every possible observed footprint is covered by the declaration;
3. exhaustive finite enumeration under a verified domain manifest;
4. a separately verified footprint-completeness theorem.

Composition v2 binds the declared footprint into the specification. A later implementation package must connect generated/project transition definitions to a nominal footprint-completeness witness before a production authority may use parallel promotion.

## Default conflicts

The default conflict relation includes:

```text
write / write
left write / right read
right write / left read
both components stage effects
both components stage outbox obligations
left effects / right outbox
right effects / left outbox
```

State paths may be disjoint while effects remain order sensitive. Transfers, mints, burns, callbacks, logs, messages, retries, and external destinations are therefore not assumed to commute.

For effects and outbox, the default is intentionally conservative: nonempty footprints conflict even when their path namespaces differ. Project semantics may waive one exact component-pair/conflict-kind combination only through a `ParallelConflictLaw` and independently accepted evidence for the exact `CompositionClaim::ConflictLaw`.

A commutativity law should normally state all of the following:

- the two operations are enabled under the same preconditions in either order;
- both orders produce the same authoritative post-state;
- both orders produce canonically equal commit and outbox plans, or a separately specified canonical normalization;
- receipts, failures, resource limits, and rejection precedence are equal;
- external interpreter behavior is idempotent or commutative under the same identities;
- no omitted observer can distinguish the orders except through explicitly declassified behavior.

## Assume-guarantee closure

An assumption discharge binds:

- the exact composition specification;
- the consuming component;
- the exact assumption claim;
- an exact sorted set of `(provider component, guarantee claim)` pairs;
- the retained evidence artifact.

A provider claim is valid only when that exact component declares that exact guarantee. The evidence verifier receives the complete structured statement, not only the assumption hash. Substituting a provider, guarantee, component, specification, or artifact invalidates the discharge.

Every local guarantee, frame theorem, and global coupling theorem is similarly wrapped in a structured `CompositionClaim` containing the exact specification identity and relevant component/path fields.

## Deterministic parallel equivalence

Parallel promotion requires a `ParallelVerificationContext` binding:

- exact `CompositionSpec` commitment;
- exact source revision;
- input-domain definition;
- coverage or theorem identity;
- partition plan and reduction topology;
- algorithm/codec versions;
- proof, replay, or translation-validation toolchain;
- deterministic merge order.

`ParallelParityEvidence` additionally binds the normative sequential result, composed result, and retained artifact. Promotion requires:

```text
context == externally expected context
context.spec == exact supplied CompositionSpec
context.merge_order == spec.merge_order
sequential_result == composed_result
independent verifier accepts the complete structured parity claim
```

Raw equal hashes without the complete context and verifier decision are insufficient.

The intended semantic theorem is:

```text
ParallelStep(spec, input, context)
    == SequentialStep(spec.merge_order, input, context)
```

where equality covers the complete authority-bearing result:

- decision class and stable reason;
- post-state and root;
- canonical patch;
- commit plan;
- outbox plan;
- receipt and candidate identity;
- resource report;
- observed footprint;
- committed-failure behavior.

## Evidence verifier boundary

`EvidenceVerifier` receives a `CompositionClaim` and artifact commitment. A verifier implementation must pin its toolchain and validate the retained artifact against the exact canonical claim.

An implementation that merely returns true, compares unrelated hashes, or checks only artifact existence is not production evidence. Production integrations should adapt canonical evidence envelopes or checked backend certificates and should retain:

- tool and binary identity;
- source revision;
- exact claim bytes/commitment;
- assumptions;
- coverage mode;
- artifact digest;
- independent replay or kernel-checking result.

## Deterministic resource bounds

The v2 model retains hard bounds for:

- path depth;
- paths per set;
- components;
- contract claims/evidence items;
- conflict laws;
- canonical length fields.

No wall clock, thread scheduler, filesystem, network, randomness, database, or mutable global state enters composition verification.

## Correctness-by-construction relationship

Composition v2 is one layer of correctness by construction:

1. closed schemas make malformed values unrepresentable or inadmissible;
2. generated typed APIs remove raw reason/effect/channel/path choices;
3. catalogued transition sealing binds one complete candidate;
4. composition contracts restrict legal wiring and concurrency;
5. structured evidence must discharge every declared theorem;
6. authorization must refuse transitions lacking required project invariants and law evidence.

Layer 6 is not completed by this composition package. First-class project invariant and conservation laws are specified separately and must be integrated with catalog authorization before a value-movement correctness claim.

## Explicit nonclaims

A verified composition report does not establish:

- completeness of a handwritten footprint declaration;
- truth of a claim when the supplied verifier is unsound;
- value conservation or economic correctness unless separately declared and proved;
- exhaustive input coverage without an exact domain manifest;
- runtime, compiler, database, chain, or hardware refinement;
- liveness, fairness, side-channel freedom, or availability;
- production authorization.
