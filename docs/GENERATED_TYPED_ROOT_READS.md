# Generated typed root reads

The project bootstrap generator exposes one high-level read method for every direct field of a reviewed record root. Each `read_<field>` method fixes the existing generated field path, returns the exact schema-generated field type, and records the same complete read footprint as the generic transition builder. `GeneratedTransition` no longer accepts an arbitrary `ValuePath` for reads.

## Inputs

- one already reviewed `ProjectCatalog`;
- its existing root type, direct record fields, stable field identifiers, field names, and field schema types;
- one schema-admitted immutable pre-state envelope;
- the existing explicit transition limits.

## Outputs

For every direct field in a record root, generated Rust source contains:

```rust
pub fn read_<field>(
    &mut self,
) -> Result<crate::generated::<FieldType>, GeneratedProjectError>
```

The method uses the existing `RootType::<field>_path()` constructor, delegates observation and footprint recording to the private generic builder, clones the bounded observed value, and converts it through the existing checked generated adapter.

If the reviewed root is not a record, the high-level wrapper generates no direct-field read methods. Lower-level integrations may still use `CataloguedTransitionBuilder` explicitly.

## Authority boundary

The reviewed schema remains authoritative for the root type, field names, field identifiers, field types, and value bounds. Generation copies those facts into nominal return types and fixed paths. It does not create a field, select a path, authorize an observation, infer nested traversal, or change footprint semantics.

The transition builder remains authoritative for path lookup, read-footprint capacity, namespace binding, canonical path hashing, decision construction, and sealing. Schema admission establishes that a field read from the admitted root can be converted to its generated nominal type.

## Trusted dependencies

No new dependency is introduced. The generated methods use the existing schema model, schema-codegen adapters and paths, transition builder, catalog, value representation, and selected commitment provider.

## Deterministic resource bounds

- The number of generated methods is bounded by `SchemaLimits::max_fields`.
- Each read consumes one entry from `TransitionLimits::max_observed_paths`.
- The returned owned value is bounded by the reviewed field schema and the pre-state admission limits.
- Existing bootstrap file-count, per-file-byte, and aggregate-byte limits are unchanged.

## Laws and negative cases

- Every generated read method fixes the exact existing direct-field path.
- Every generated read method returns the exact schema-generated field type.
- A successful read records exactly one read footprint at the expected state namespace and field identifier.
- A read alone stages no patch or write footprint.
- The generated high-level wrapper contains no raw `read(ValuePath)` method.
- Repeated generation and catalog declaration-order independence remain byte-for-byte deterministic.

## Assumptions

- The reviewed schema gives every root field its intended project meaning and bounds.
- The pre-state envelope was admitted under the exact generated schema and provider binding.
- Generated output is used at its retained digest or reviewed again after modification.
- Project policy decides whether exposing a read result to a caller is authorized.

## Explicit nonclaims

- This package does not generate nested record, tuple, vector, sum-payload, or map-entry read methods.
- It does not remove the explicit lower-level generic transition API.
- A schema-typed read does not establish information-flow, declassification, or side-channel safety.
- It changes no schema, stable field identifier, path encoding, precedence position, profile version, candidate identity, or canonical codec byte.
- It provides no formal proof, independent audit, mounted-runtime evidence, or production authorization.
- Bounded compiled tests are not unbounded proofs.
