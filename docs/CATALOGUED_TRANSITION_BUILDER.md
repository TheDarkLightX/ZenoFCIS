# Catalogued transition builder

`zeno-fcis-transition` is a pure `no_std + alloc` construction layer above the schema-bound catalog, preconditioned patch, closed plans, composition footprint, deterministic budget, and candidate-sealing crates.

## Scope

The builder owns one execution-local proposal. It reads one immutable pre-state, accumulates fresh private vectors, canonicalizes them at the boundary, validates committed plans through one `ProjectCatalog`, derives candidate bindings from that catalog, and returns one of the existing three decisions:

```text
Accept(TransitionArtifacts)
Reject(TransitionReject)
CommittedFailure(TransitionArtifacts, SemanticId)
```

An ordinary rejection returns no patch, `CommitPlan`, `OutboxPlan`, receipt candidate, or `CommitBundle`. An accepted or committed-failure result owns one complete `CommitBundle` plus the exact observed footprint and deterministic resource report bound by that bundle.

## Inputs and outputs

Construction inputs are:

- one immutable `ProjectCatalog`;
- one immutable pre-state `Value`;
- one explicit versioned state commitment domain;
- nonzero command and authenticated-context commitments;
- exact caller-supplied `BudgetUsed`, normally extracted from the pure
  transition's returned `BudgetedDecision`;
- one explicit `TransitionLimits` envelope.

The generic builder retains this low-level interface. Bootstrap-generated projects expose the narrower path documented in `GENERATED_CATALOG_TRANSITION.md`, `GENERATED_COMMAND_CONTEXT_ENVELOPES.md`, `GENERATED_TYPED_ROOT_READS.md`, `GENERATED_NO_RAW_MUTATIONS.md`, `GENERATED_TYPED_CONTEXT_OBSERVATIONS.md`, and `GENERATED_TYPED_REASONS.md`: a private reconstructed catalog plus schema-admitted root, command, and context witnesses and a private-inner transition wrapper. That path has no caller-supplied raw catalog, state `Value`, command hash, context hash, reason `SemanticId`, direct-field read path, raw insertion/deletion method, or raw context-observation path.

Builder operations may:

- observe a `ValuePath` in the immutable pre-state;
- add preconditioned insert, update, or delete operations;
- add closed `Effect` and `OutboxEntry` values;
- record explicit context paths;
- register applicable reject or committed-failure reasons from the catalog.

The builder derives the profile, precedence, and algorithm bindings from the catalog. It derives the budget binding from a canonical `TransitionResourceReport` containing the catalog commitment, caller budget usage, catalog metrics, and observed-footprint commitment. Callers cannot supply those four candidate bindings independently.

## Decision and error precedence

Applicable catalog reasons are selected by the catalog's unique total precedence, independent of method-call order. The selected reason's disposition determines `Reject` or `CommittedFailure`. If no reason applies, the result is `Accept`.

For a committing result, construction order is fixed:

1. normalize the observed footprint;
2. canonicalize plan ordinals and reject duplicates;
3. validate both plans against the catalog;
4. canonicalize the patch and reject overlap or invalid insertion shape;
5. derive the resource-report and budget commitments;
6. seal exactly one candidate through `CandidateBuilder`.

For ordinary rejection, staged patch/effect/outbox proposals and their proposed write/effect footprints are discarded before output construction. Observed read and context paths remain as evidence of the rejection computation. The rejection receipt binds the same catalog-derived profile, precedence, algorithm, command, context, and resource-report identities, while carrying no candidate.

## Footprint semantics

State observations and proposed writes are converted from `ValuePath` to `AccessPath` under the profile state-type namespace. Map-key atoms use a domain-separated commitment to the exact encoded map-key bytes. Explicit context paths must use the profile context-type namespace. Each non-executable commit-evidence record contributes the empty path in the effect operation's namespace. Duplicate observed paths are canonicalized as set membership.

The returned footprint is execution-observed evidence. It does not establish that all paths reachable for other inputs were declared. It cannot authorize deterministic parallel composition by itself. Parallel promotion still requires a separately reviewed complete static component footprint and equality with the canonical sequential result.

## Deterministic bounds

`TransitionLimits` bounds:

- patch proposals;
- observed state paths in each read/write set;
- explicit context paths;
- applicable reasons;
- encoded bytes in any one map-key path atom.
- recursive pre-state and successor depth;
- pre-state and successor node count.

Each bound also has a hard library maximum. Both the supplied pre-state and every committing successor must satisfy the catalog's root schema under the state-validation bounds. Effect and outbox cardinality and payload structure remain bounded by `CatalogLimits`. Existing `AccessPath`, `PathSet`, `CanonicalPatch`, `CommitPlan`, and `OutboxPlan` constructors apply their own exact canonical bounds.

## Laws and negative cases

The implementation must test:

- method-call order cannot change reason selection, plan order, patch order, or footprint bytes;
- rejected decisions expose no candidate or authoritative plan;
- committed failure uses a catalogued committed-failure reason and a complete candidate;
- profile, precedence, algorithm, catalog, footprint, and budget identities recompute from output values;
- stale pre-root, wrong state type, wrong reason disposition, unknown plan ID, duplicate ordinal, overlapping path, wrong payload schema, and wrong candidate binding fail closed;
- exact boundary values pass and one-over-limit values fail without partial output;
- output validation replays candidate sealing and catalog admission against the same pre-state.

## Authority boundary and trusted dependencies

The builder decides construction consistency and catalog admission. It does not decide business predicates: callers supply booleans only to select already reviewed catalog reasons. It does not execute effects or outbox delivery, acquire time or randomness, perform I/O, choose schemas, choose stable identifiers, or promote evidence.

The crate adds no external dependency. It trusts the existing value, codec, core, patch, plan, receipt, composition, project, schema, and catalog invariants plus the selected commitment provider. It assumes caller `BudgetUsed` is taken from the exact `BudgetedDecision` for the transition being sealed and that the implementation charged all modeled work. The builder binds that report but does not reconstruct hidden computation or independently prove accounting completeness.

## Explicit nonclaims

- An observed footprint is not a complete static component contract.
- Builder validation is not a proof of business correctness or invariant preservation.
- A policy or predicate commitment is not proof that its referenced rule is safe.
- Low-level map entries now derive their ordering bytes from their semantic key; see
  `CORRECT_BY_CONSTRUCTION_MAP_ENTRIES.md`.
- It does not add runtime refinement, shell atomicity evidence, mechanized proofs, or production authorization.
