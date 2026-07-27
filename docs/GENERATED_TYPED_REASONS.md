# Generated typed reason application

The project bootstrap generator separates reviewed ordinary-rejection reasons from reviewed committed-failure reasons in the generated Rust API. `GeneratedProject::begin_transition` returns a private-inner `GeneratedTransition` wrapper. Its `require` method accepts only `RejectReasonId`, and its `fail_if` method accepts only `CommittedFailureReasonId`.

## Inputs

- one already reviewed `ProjectCatalog`;
- its existing reason definitions, stable identifiers, dispositions, names, predicate commitments, and total precedence;
- the existing generated transition inputs and explicit resource limits.

## Outputs

Generated Rust source contains:

- the existing general `ReasonId` enum for catalog inspection;
- `RejectReasonId`, containing exactly the catalog reasons whose disposition is `Reject`;
- `CommittedFailureReasonId`, containing exactly the catalog reasons whose disposition is `CommittedFailure`;
- a private-inner `GeneratedTransition<'a, H>` that delegates state, context, catalog-typed effect/channel staging, and sealing operations to `CataloguedTransitionBuilder`;
- disposition-typed `require` and `fail_if` methods;
- no public escape method that returns the generic builder.

An empty disposition class produces an uninhabited generated enum. This keeps that reason class unavailable without inventing a placeholder identifier.

## Authority boundary

The input catalog remains authoritative for every reason identifier, disposition, predicate commitment, readable name, and precedence position. Generation partitions those existing definitions by disposition and renders nominal Rust types. It does not create, remove, renumber, reinterpret, or reorder a reason.

The caller still computes each business predicate and supplies the resulting boolean. The generic transition builder still validates the reconstructed semantic identifier against the exact catalog and selects applicable reasons by the catalog's total precedence. Candidate sealing and three-way decision construction remain authoritative.

## Trusted dependencies

No new external library is introduced. The generated wrapper uses the existing catalog, project, transition, compose, patch, plan, value, schema, codec, and commitment-provider boundaries. The compiled fixture directly uses the existing workspace `zeno-fcis-compose` crate to inspect generated context footprints; the generated high-level API exposes no caller-supplied `AccessPath`.

## Deterministic resource bounds

- The number of variants in each typed enum is bounded by the already validated catalog reason count.
- `TransitionLimits::max_applicable_reasons` continues to bound retained applicable reasons.
- Duplicate applications of the same reason remain set-like and consume no additional retained-reason slot.
- Existing patch, footprint, effect, outbox, schema-validation, and bootstrap-output limits are unchanged.
- Generation and transition construction allocate only fresh execution-local bounded values.

## Laws and negative cases

- Every generated typed reason maps to the same general `ReasonId` and nonzero `SemanticId` already present in the catalog.
- `RejectReasonId` contains only ordinary-rejection reasons.
- `CommittedFailureReasonId` contains only committed-failure reasons.
- A satisfied `require` condition records no reason and preserves `Accept` when no other reason applies.
- A failed `require` condition produces the exact catalogued rejection and revalidates as a complete decision.
- Applying the same typed reason twice produces the same decision as applying it once.
- Generated high-level transition methods expose no raw `SemanticId` parameter.
- The generic builder is stored in a private field and is not returned by the generated entry point.

## Assumptions

- The reviewed catalog assigns the intended disposition and predicate commitment to each reason.
- Caller-supplied predicate booleans faithfully represent the reviewed predicate identified by the catalog commitment.
- Generated source is used at its retained digest or reviewed again after modification.
- The selected commitment provider and generated catalog reconstruction are correct.

## Explicit nonclaims

- Rust type separation does not prove that a caller computed a business predicate correctly.
- This package does not change generic low-level `CataloguedTransitionBuilder` APIs.
- This package does not change rejection precedence, schemas, stable identifiers, candidate identity, codec bytes, or profile versions.
- It does not mount a runtime, execute effects, prove noninterference, provide an independent audit, or authorize production use.
- Bounded compiled tests are not an unbounded proof.
