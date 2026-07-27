# Generated catalog-bound transitions

The project bootstrap generator emits an exact `GeneratedProject` binding between the reviewed `ProjectCatalog`, schema-admitted inputs, and a private-inner `GeneratedTransition`. The generated project owns a private reconstructed catalog, and the generated transition owns the generic `CataloguedTransitionBuilder` privately. Callers cannot start this path with an arbitrary catalog or raw `Value`, stage an update with an arbitrary `ValuePath` and raw `Value`, apply a raw reason `SemanticId`, or stage a raw `Effect` or `OutboxEntry` through the high-level wrapper.

## Inputs

Generation receives:

- one already reviewed `ProjectCatalog`;
- the exact `CommitmentHasher` used by that catalog;
- the existing schema-codegen root type and exact generated schema reconstruction;
- the existing bootstrap output limits and module names.

Runtime construction receives only:

- the same selected commitment provider;
- a `SchemaAdmittedEnvelope` produced under the exact generated schema commitment and root type;
- nominal generated command and context witnesses admitted under the profile's exact non-root schema types;
- an explicit versioned state commitment domain;
- explicit `BudgetUsed` and `TransitionLimits` values.

## Outputs

Generated source contains:

- exact catalog, profile, schema, and hash-provider constants;
- deterministic reconstruction of all catalog definitions, profile registry entries, bindings, and `CatalogLimits`;
- a private-field `GeneratedProject` that owns the reconstructed catalog;
- a typed-root `GeneratedProject::admit_root` method that uses the catalog provider even when schema code generation used a different provider;
- typed `admit_command` and `admit_context` methods that derive visible role-separated commitments;
- an envelope-bound `GeneratedProject::begin_transition` method;
- separate `RejectReasonId` and `CommittedFailureReasonId` enums copied from the exact catalog dispositions;
- a `GeneratedTransition` wrapper with schema-typed direct root-field update methods, disposition-typed reason methods, and per-catalog typed effect/channel staging methods;
- local fail-closed diagnostics for provider, catalog, profile, schema, root-type, and transition failures.

Successful startup returns `GeneratedTransition`, which retains the existing generic builder in a private field. Sealing and three-way decision semantics are unchanged.

## Authority boundary

The input catalog remains authoritative. Generation copies existing schema types, stable identifiers, reason dispositions and precedence, registry commitments, policy commitments, provider identity, and resource limits. It does not select or alter any of them.

`GeneratedProject::try_new` reconstructs those values, recomputes the schema commitment, validates all catalog cross-bindings, and compares the exact profile and complete catalog commitments with the generation-time constants. `admit_root` converts only the generated typed root and admits it with the catalog's schema and selected catalog provider. Command and context admission use the profile's exact type IDs and derive commitments over their complete schema-bound envelope bytes under `<domain-prefix>/command` and `<domain-prefix>/context`. `begin_transition` rechecks the provider, complete stored catalog identity, root witness, command witness, context witness, and both derived commitments before constructing the private-inner wrapper. Direct root-field update methods reuse existing schema-generated types and paths; generic nested mutation remains an explicit lower-level API. Reason types are a deterministic partition of existing catalog definitions. Effect and channel methods reuse existing catalog definitions and generated smart constructors; their signatures reflect the reviewed schema and hash-requirement shape. The generator chooses no field, path, identifier, precedence, authority rule, or destination.

The local error order is:

1. commitment-provider identity;
2. reconstructed schema commitment;
3. catalog construction and cross-bindings;
4. embedded schema, profile, and complete catalog commitments;
5. envelope schema commitment;
6. envelope root type;
7. generic transition admission.

This is an API diagnostic order. It does not modify application rejection or committed-failure precedence.

## Trusted dependencies

No new external dependency is introduced. This layer trusts the existing schema, generated root-envelope, project-profile, catalog, transition, codec, core, plan, and selected commitment-provider invariants.

## Deterministic resource bounds

- Catalog reconstruction is bounded by the already validated schema, catalog-definition, registry-entry, stable-name, and `CatalogLimits` maxima.
- Root admission retains its exact caller-supplied `ValidationLimits` and report.
- Transition construction retains the explicit `TransitionLimits`, `CatalogLimits`, `BudgetUsed`, patch, footprint, effect, and outbox bounds.
- Bootstrap file count, per-file bytes, and aggregate bytes remain bounded by `BootstrapLimits`.
- Reconstruction fails atomically and returns no partial `GeneratedProject`.

## Laws and negative cases

The compiled generated fixture verifies:

- reconstructed catalog equality with the exact generation input;
- equality of embedded schema, profile, and catalog commitments;
- successful envelope-bound startup, sealing, and full decision revalidation;
- typed-root admission under the catalog provider, including provider-before-conversion diagnostic precedence;
- rejection of a different hash-provider identity;
- rejection of an envelope with a different schema commitment;
- rejection of a different root type even when a test provider forces the expected schema commitment;
- provider and generated-binding failures precede generic transition-input failures;
- command and context commitments are deterministic, value-sensitive in bounded fixtures, role-domain separated, and nonzero;
- candidate bindings carry the exact generated command and context commitments;
- generated source contains no raw-`Value` or caller-supplied-catalog transition entry point;
- generated record-root update methods use the exact field path and type, reject malformed field values before staging, and expose no raw update path/value pair;
- generated high-level reason methods accept only the catalog-disposition-specific nominal reason types;
- generated high-level effect and channel methods fix existing IDs and schema-generated value types while exposing no raw plan value;
- invalid typed payloads and destinations fail before staging and leave the plans unchanged;
- repeated application of the same typed reason is decision-idempotent.

## Assumptions

- The generation input catalog and selected commitment provider were reviewed together.
- The generated schema reconstruction preserves the input schema exactly.
- The selected `CommitmentHasher` implementation correctly implements its advertised identity.
- State-domain selection, business predicates, authenticated-context provenance, and reported logical budget usage remain caller responsibilities.
- Generated output is used at its retained digest or reviewed again after modification.

## Explicit nonclaims

- Reconstructing an exact catalog is not proof that the catalog's business policy is correct.
- A schema-admitted root does not prove an application invariant beyond the closed schema.
- A generated transition is not a mounted-runtime refinement result, formal proof, audit, or production authorization.
- The bounded fixture suite is not an unbounded proof of commitment collision resistance or semantic correctness.
- This package changes no stable identifier, precedence position, protocol version, schema, or authority policy.
