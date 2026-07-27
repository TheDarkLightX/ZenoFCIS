# Fallible Persistent-Map API

## Scope

This package removes the two public `PersistentMap` convenience methods that
converted materialization errors into panics. The checked
`try_to_value_map()` and `try_canonical_bytes()` methods remain unchanged.
No backend algorithm, logical entry, canonical byte, protocol identifier,
state root, or dependency changes.

## Inputs and outputs

`PersistentMap::try_to_value_map(&self)` accepts one immutable map snapshot and
returns either its canonical `Value::Map` or one explicit `MapError`.

`PersistentMap::try_canonical_bytes(&self)` accepts the same immutable snapshot
and returns either the exact existing ZCVE/1 bytes or one explicit `MapError`.
Neither method publishes partial output.

## Authority boundary

Persistent collection backends remain sealed. Backend node shape, allocation
identity, and insertion history remain non-authoritative. Materialization
revalidates the stored encoded key against the semantic key before producing a
protocol value. A backend invariant violation crosses the public boundary only
as `MapError`; library code cannot convert it into process termination.

Tests and benchmarks may terminate on an unexpected `Err`, but those helpers
are outside the library API and grant no protocol or release authority.

## Trusted dependencies

No dependency is added or changed. The package trusts the existing checked
`LogicalEntry`, `MapEntry`, `Value::map_canonical`, and `CanonicalEncode`
boundaries, plus the sealed reference, `rpds`, and `imbl` adapters.

## Deterministic resource bounds

The existing bounds remain unchanged:

- maximum value depth: 64;
- maximum value nodes: 1,000,000;
- maximum aggregate payload bytes: 64 MiB;
- maximum children in one collection: 1,000,000;
- canonical lengths are encoded as `u32` and fail on overflow.

Materialization performs one deterministic traversal of the snapshot entries
and one bounded value encoding. It observes no ambient time, randomness,
filesystem, network, thread, process, or mutable global state.

## Laws and positive evidence

1. For every admitted snapshot `m`, the retained fallible methods return the
   same values and bytes as before this package.
2. Equal logical snapshots return equal successful materializations regardless
   of insertion history or selected backend.
3. Snapshot retention and removal laws remain unchanged.
4. Exact ZCVE/1 golden and round-trip evidence remains unchanged.
5. Library compilation denies `clippy::panic`.

Test helpers unwrap successful results before comparing them. Two identical
errors therefore cannot satisfy a canonical-equality law accidentally.

## Negative cases

- a backend-stored key mismatch returns `MapError::KeyEncodingMismatch`;
- duplicate or unsorted materialized keys return `MapError::ValueMap`;
- canonical encoding failure returns `MapError::Encoding`;
- compile-fail doctests reject the removed `to_value_map()` and
  `canonical_bytes()` convenience calls;
- one-over-limit values return an error without a partial protocol value.

## Compatibility

This is an intentional pre-1.0 source compatibility break. Downstream code must
replace:

```text
map.to_value_map()       -> map.try_to_value_map()?
map.canonical_bytes()    -> map.try_canonical_bytes()?
```

The return types and behavior of the retained `try_*` methods do not change.

## Assumptions

- callers do not rely on process termination as protocol behavior;
- ZCVE/1 and `ValueLimits::default()` retain their reviewed meanings;
- external backend types remain sealed behind this crate;
- tests and benchmarks are non-authoritative evidence consumers.

## Explicit nonclaims

- removing library panics is not a proof that allocation failure cannot abort;
- this package does not prove every transitive dependency panic-free;
- it does not promote a persistent backend to production use;
- it adds no authenticated-state, database, migration, runtime-refinement, or
  production-readiness claim.
