# Generated typed root updates

The project bootstrap generator exposes one high-level update method for every direct field of a reviewed record root. Each `update_<field>` method accepts the exact schema-generated field type and fixes the existing generated field path. `GeneratedTransition` no longer accepts an arbitrary `ValuePath` plus raw `Value` for updates.

## Inputs

- one already reviewed `ProjectCatalog`;
- its existing root type, direct record fields, stable field identifiers, field names, and field schema types;
- one schema-admitted pre-state envelope;
- the existing explicit state domain, budget report, and transition limits.

## Outputs

For every direct field in a record root, generated Rust source contains:

```rust
pub fn update_<field>(
    &mut self,
    value: &crate::generated::<FieldType>,
) -> Result<&mut Self, GeneratedProjectError>
```

The method converts the nominal field value through its existing checked adapter, uses the existing `RootType::<field>_path()` constructor, and delegates precondition hashing, patch construction, footprint recording, canonical ordering, successor construction, and sealing to the private generic builder.

If the reviewed root is not a record, the high-level wrapper generates no direct-field update methods. Lower-level integrations may still use `CataloguedTransitionBuilder` explicitly.

## Authority boundary

The reviewed schema remains authoritative for the root type, field names, field identifiers, field types, and value bounds. Generation copies those existing facts into nominal method signatures and fixed paths. It does not create a field, choose a path, infer a nested mutation policy, or alter the canonical patch format.

The transition builder remains authoritative for expected-pre-root binding, expected-old-value hashes, complete read/write footprints, overlap rejection, canonical operation ordering, successor calculation, catalog validation, and candidate sealing.

## Trusted dependencies

No new dependency is introduced. The generated methods use the existing schema model, schema-codegen adapters and paths, patch crate, transition builder, catalog, codec, and selected commitment provider.

## Deterministic resource bounds

- The number of generated methods is bounded by `SchemaLimits::max_fields`.
- Field conversion retains the reviewed field schema's depth, node, collection, and byte bounds.
- `TransitionLimits` continues to bound patch operations and observed paths.
- `CatalogLimits` and schema admission continue to bound the complete successor.
- Existing bootstrap file-count, per-file-byte, and aggregate-byte limits are unchanged.

## Laws and negative cases

- Every generated update method fixes the exact existing direct-field path.
- Every generated update method accepts only the exact schema-generated field type.
- Invalid generated values fail before staging and leave the patch empty.
- A successful typed update produces the expected field path, replacement value, successor, and complete revalidated decision.
- The generated high-level wrapper contains no raw `update(ValuePath, Value)` method.
- Repeated generation and catalog declaration-order independence remain byte-for-byte deterministic.

## Assumptions

- The reviewed schema gives every root field its intended project meaning and bounds.
- Generated output is used at its retained digest or reviewed again after modification.
- The selected commitment provider and generated catalog reconstruction are correct.
- Business predicates decide whether an otherwise well-typed update is allowed.

## Explicit nonclaims

- This package does not generate nested record, tuple, vector, sum-payload, or map-entry update methods.
- It does not remove the explicit lower-level generic transition API.
- A schema-typed update does not prove a business invariant or authorize the mutation.
- It changes no schema, stable field identifier, path encoding, precedence position, profile version, candidate identity, or canonical codec byte.
- It provides no formal proof, independent audit, mounted-runtime evidence, or production authorization.
- Bounded compiled tests are not unbounded proofs.
