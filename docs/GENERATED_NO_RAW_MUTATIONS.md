# Generated high-level mutation boundary

The project bootstrap generator exposes schema-typed direct root-field updates and no arbitrary raw insertion or deletion method through `GeneratedTransition`. Raw `ValuePath`, `Value`, and map-key assembly remain available only through the explicit low-level `CataloguedTransitionBuilder` API.

## Inputs

- one already reviewed `ProjectCatalog` and closed schema;
- the existing schema-generated direct root-field update methods;
- the existing private-inner generated transition wrapper.

## Outputs

Generated high-level Rust source contains no:

```rust
insert(ValuePath, Option<Value>, Value)
delete(ValuePath)
```

It also does not import `ValuePath` or raw `Value` for state mutation. Direct record-root reads and updates, disposition-typed reasons, catalog-typed effects and channels, schema-fixed context-footprint observation, and sealing remain available.

## Authority boundary

The reviewed schema remains authoritative for generated direct-field paths and types. The generator does not infer whether an absent field, map key, vector element, or nested location is a valid mutation target. Projects that need those operations must define a reviewed typed API or deliberately use the low-level generic builder.

The low-level builder remains authoritative for insertion/deletion preconditions, expected absence, old-value hashes, overlap rejection, complete footprints, canonical ordering, successor validation, and candidate sealing.

## Trusted dependencies

No new dependency is introduced. This package removes generated high-level surface and relies on the existing schema-codegen, bootstrap, transition, patch, catalog, and fixture boundaries.

## Deterministic resource bounds

- No new runtime resource is introduced.
- Generated source becomes smaller by a fixed method/import set.
- Existing `TransitionLimits`, schema admission, patch-operation, path-depth, and bootstrap output bounds are unchanged.

## Laws and negative cases

- Generated source contains no raw insertion method.
- Generated source contains no raw deletion method.
- Generated source contains no `ValuePath` or raw `Value` state-mutation import.
- Existing schema-typed direct-field updates still compile, stage the exact patch, and pass complete decision revalidation.
- Existing low-level insertion/deletion laws remain unchanged.
- Repeated generation and declaration-order independence remain byte-for-byte deterministic.

## Assumptions

- Direct root-field updates cover the intended initial high-level mutation surface.
- Projects needing nested mutation will add separately reviewed schema-typed methods.
- Generated output is used at its retained digest or reviewed again after modification.

## Explicit nonclaims

- This package does not generate typed insertion or deletion methods.
- It does not remove low-level `CataloguedTransitionBuilder::insert` or `delete`.
- It does not prove that existing project-specific callers avoid the low-level API.
- It changes no schema, stable identifier, path encoding, precedence, profile version, candidate identity, or canonical codec byte.
- It provides no formal proof, mounted-runtime evidence, independent audit, or production authorization.
- Bounded compiled tests are not unbounded proofs.
