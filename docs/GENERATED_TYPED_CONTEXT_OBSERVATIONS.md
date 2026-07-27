# Generated typed context observations

The project bootstrap generator exposes only schema-fixed authenticated-context observation methods through `GeneratedTransition`. A record-shaped context root receives one `observe_context_<field>()` method per reviewed field. Any other context root receives `observe_context_root()`. The generated high-level API accepts no caller-supplied `AccessPath`.

## Inputs

- one already reviewed `ProjectCatalog` and closed schema;
- the profile's existing context-type identifier;
- the context root's existing kind and stable record-field identifiers;
- the existing private-inner generated transition wrapper.

## Outputs

For a record context, generated source contains methods such as:

```rust
observe_context_oracle_epoch()
```

Each method constructs exactly:

```text
AccessPath(context_type_id, [Field(reviewed_field_id)])
```

For unit, boolean, integer, bytes, text, enum, tuple, sum, vector, or map context roots, generated source contains `observe_context_root()`, which constructs the exact context namespace with an empty atom sequence.

## Authority boundary

The reviewed profile selects the context namespace. The reviewed schema selects whether the context root is a record and, for records, the available field names and stable field IDs. Generation copies those values into ordinary inspectable Rust. It does not infer a context field, accept a caller path, read ambient context, authenticate provenance, or decide whether a business computation actually depended on the declared field.

`CataloguedTransitionBuilder::observe_context` remains authoritative for namespace matching, wildcard rejection, retained-path limits, footprint normalization, resource binding, and candidate sealing. Its low-level raw-path API is unchanged.

## Trusted dependencies

No new dependency is introduced. The generated methods use the existing schema, profile, compose, transition, bootstrap, and compiled-fixture boundaries.

## Deterministic resource bounds

- A record context generates exactly one observation method per already bounded schema field.
- A non-record context generates exactly one root-observation method.
- Each call constructs zero or one `PathAtom`, below the existing `AccessPath` depth bound.
- Each successful call consumes one slot under `TransitionLimits::max_observed_paths` before canonical duplicate removal.
- Existing bootstrap file-count and byte limits remain authoritative.

## Laws and negative cases

- Every generated record-field method fixes the profile context namespace and exact stable field ID.
- The compiled record fixture records one exact context path, no state read, no state write, and no patch operation.
- A non-record context generates the fixed root-observation method.
- Generated high-level source exposes no `observe_context(AccessPath)` method.
- The generic builder still rejects wrong namespaces, wildcard observations, and one-over path limits.
- Repeated generation and declaration-order independence remain byte-for-byte deterministic.

## Assumptions

- The reviewed schema field describes the intended context dependency.
- The caller's business computation reports every context field it actually observes.
- Authenticated-context provenance is established outside this pure generated wrapper.
- Generated output is used at its retained digest or reviewed again after modification.

## Explicit nonclaims

- This package does not prove complete information-flow tracking or noninterference.
- It does not authenticate, fetch, or evaluate external context.
- It does not generate nested, tuple-element, vector-index, sum-payload, or map-key observation methods.
- It does not remove the low-level `CataloguedTransitionBuilder::observe_context` API.
- It changes no schema, stable identifier, path encoding, precedence, profile version, candidate identity, or canonical codec byte.
- It provides no formal proof, mounted-runtime evidence, independent audit, or production authorization.
- Bounded compiled tests are not unbounded proofs.
