# Generated typed effect and channel staging

The project bootstrap generator exposes one high-level staging method for every effect and channel already present in a reviewed `ProjectCatalog`. `GeneratedTransition` no longer accepts a caller-built `Effect` or `OutboxEntry`. Each generated method fixes the stable operation or channel identifier and accepts only the schema-generated payload and destination types selected by the catalog.

## Inputs

- one already reviewed `ProjectCatalog`;
- its existing effect and channel definitions, stable identifiers, payload and destination schema types, authority and subject requirements, and policy commitments;
- the existing generated transition inputs and explicit resource limits.

## Outputs

For each catalog effect, generated Rust source contains `emit_effect_<id>`. Its payload parameter uses the exact schema-generated payload type. Authority and subject parameters follow the reviewed `HashRequirement`:

- `Any` accepts `Hash32`;
- `Present` accepts `NonZeroHash`;
- `Absent` exposes no parameter and stages `Hash32::ZERO`;
- `Exact` exposes no parameter and stages the exact catalog commitment.

For each catalog channel, generated source contains `enqueue_channel_<id>` with the exact schema-generated destination and payload types. These methods reuse the existing generated smart constructors and stage their closed values through the private generic builder.

The generated high-level wrapper exposes no raw `emit(Effect)` or `enqueue(OutboxEntry)` method. The generic low-level builder and the standalone smart constructors remain available for explicitly lower-level integrations.

## Authority boundary

The reviewed catalog remains authoritative for every stable identifier, schema type, authority rule, subject rule, operation-policy commitment, delivery-policy commitment, and plan limit. Generation renders those existing values into nominal method signatures and fixed arguments. It does not create, remove, renumber, reinterpret, or reorder a protocol value.

Typed conversion rejects malformed generated payloads and destinations before they are staged. Final `ProjectCatalog` validation at sealing remains authoritative for complete effect and outbox admission, aggregate limits, and cross-bindings.

## Trusted dependencies

No new dependency is introduced. The generated methods use the existing catalog, schema-codegen adapters, closed plan values, transition builder, codec, and commitment-provider boundaries.

## Deterministic resource bounds

- The number of generated staging methods is bounded by the already validated catalog effect and channel counts.
- Each typed payload and destination conversion retains its schema bounds.
- `CatalogLimits::max_effects` and `max_outbox_entries` bound staged plan cardinality.
- Catalog value and aggregate metrics continue to bound depth, nodes, collection length, and payload bytes.
- `TransitionLimits::max_observed_paths` continues to bound the recorded effect footprint.
- Existing bootstrap file-count, per-file-byte, and aggregate-byte limits are unchanged.

## Laws and negative cases

- Every generated effect method fixes the exact existing effect identifier and payload type.
- Every generated channel method fixes the exact existing channel identifier, destination type, and payload type.
- `Present` commitments cannot be zero through the generated signature.
- `Absent` and `Exact` commitments cannot be replaced by caller input through the generated signature.
- `Any` commitments remain explicit caller inputs.
- Invalid typed payloads and destinations fail before staging and leave both plans unchanged.
- Successfully staged typed values survive full decision sealing and complete decision revalidation.
- Declaration-order independence and byte-identical repeated generation remain covered by existing bootstrap laws.
- Generated high-level source contains no raw effect or outbox staging method.

## Assumptions

- The reviewed catalog assigns the intended schema types and authority rules to every effect and channel.
- Caller-supplied `Any` and `Present` commitments have the intended project meaning.
- Generated source is used at its retained digest or reviewed again after modification.
- The selected commitment provider and generated catalog reconstruction are correct.

## Explicit nonclaims

- Rust type selection does not prove that a catalog policy is economically or operationally correct.
- `NonZeroHash` proves only that a commitment is not the zero sentinel; it does not prove authority provenance.
- This package does not execute effects, deliver outbox entries, mount a runtime, or authorize a destination.
- It changes no schema, stable identifier, authority rule, policy commitment, precedence position, profile version, candidate identity, or canonical codec byte.
- It provides no formal proof, independent audit, side-channel result, or production authorization.
- Bounded compiled tests are not unbounded proofs.
