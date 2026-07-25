# Persistent Collection Adapters — Design Document

## Work Package G (Issue #12)

### Objective

Evaluate structural sharing and immutable collection backends while keeping
equality, ordering, encoding, and roots defined over logical entries.

### Architecture

The `zeno-fcis-collections` crate defines a `PersistentMap` trait and a
`LogicalEntry` type. All backends produce identical `Value::Map` values and
canonical bytes for the same logical entries, regardless of insertion history.

```
PersistentMap trait
├── BTreeMapBackend (reference, always available)
├── RpdsBackend (optional, behind "rpds-backend" feature)
└── ImblBackend (optional, behind "imbl-backend" feature)
```

### Authority Boundary

- The `LogicalEntry` is the sole authority for map equality, ordering, and
  canonical encoding. Backend-specific internal shapes are never used for
  protocol meaning.
- The `PersistentMap` trait defines the semantic interface. Backend types are
  never exposed in the public API of downstream crates.
- The `BTreeMapBackend` is the reference backend and ground truth for
  differential testing.

### Trusted Dependencies

- `zeno-fcis-codec` for `CanonicalEncode` (existing workspace crate).
- `zeno-fcis-value` for `Value`, `MapEntry` (existing workspace crate).
- `rpds = "=1.1.0"` (optional): MIT/Apache-2.0, no unsafe, no advisory.
- `imbl = "=3.0.0"` (optional): MPL-2.0+, no advisory.
- `criterion = "=0.5.1"` (dev-only): MIT/Apache-2.0.

All dependency versions are pinned exactly per ZenoFCIS dependency policy.

### Deterministic Resource Bounds

- No explicit size limits in the adapter layer; bounds are inherited from
  `zeno-fcis-value`'s `Value::Map` validation.
- `BTreeMapBackend`: O(n) clone per insert/remove (no structural sharing).
- `RpdsBackend`: O(n) filter + push per insert/remove (vector-based).
- `ImblBackend`: O(log n) insert/remove (tree-based structural sharing).

### Laws

1. **Insertion-history independence**: all backends produce identical entries
   and canonical bytes for the same set of entries, regardless of insertion
   order.
2. **Alias resistance**: `insert` and `remove` return new maps; the original
   is unchanged.
3. **Snapshot retention**: old versions remain valid after modifications to
   new versions.
4. **Old-version stability**: the canonical bytes of an old version do not
   change after modifications to new versions.
5. **Canonical encoding consistency**: `canonical_bytes()` is deterministic
   and equals the encoding of the materialized `Value::Map`.
6. **Deletion correctness**: removing a key eliminates it; removing a
   nonexistent key is a no-op.
7. **Zero-removal**: removing all entries yields an empty map with empty
   entries.

### Differential Tests

- **BTreeMap vs reference model**: insert/remove sequence with 10 entries,
  removing every other entry, asserting equality at every step.
- **Rpds vs BTreeMap** (with `rpds-backend`): insert 20 entries, remove every
  3rd, assert entry and canonical byte equality.
- **Imbl vs BTreeMap** (with `imbl-backend`): same differential sequence.

### Benchmarks

- `insert_dense`: insert 10, 50, 100, 200 entries in dense order.
- `insert_sparse`: insert entries with stride 5 (sparse set).
- `lookup`: get by encoded key in maps of size 10, 50, 100, 200.
- `canonical_iteration`: materialize entries in canonical order.
- `root_generation`: compute canonical bytes.
- `snapshot_retention`: retain all intermediate snapshots during insert.

### Dependency Assessment

| Dependency | Version | License | Advisory | Unsafe |
|------------|---------|---------|----------|--------|
| rpds | =1.1.0 | MIT/Apache-2.0 | None | No |
| imbl | =3.0.0 | MPL-2.0+ | None | Yes (internal) |
| criterion | =0.5.1 | MIT/Apache-2.0 | None | No |

The `imbl` crate contains internal `unsafe` code for its persistent vector
implementation. This `unsafe` code is never exposed through the adapter API.
The `rpds` crate is `no_std` and contains no `unsafe` code.

### Recommendation

**No default backend is selected.** The `BTreeMapBackend` is the reference
backend and is always available. The `rpds-backend` and `imbl-backend`
features are opt-in. A default backend should only be selected after
benchmark results demonstrate a clear performance advantage and the
assurance criteria (no `unsafe` in the adapter layer, no advisory
vulnerabilities, license compatibility) are met.

### Nonclaims

- Persistent asymptotics do not imply better latency, memory locality,
  canonical commitments, authenticated proofs, or economic correctness.
- The `BTreeMapBackend` has no structural sharing; it clones the entire map
  on every modification.
- The `RpdsBackend` uses a vector internally and filters on every insert,
  which is O(n) per operation.
- The `ImblBackend` provides true O(log n) structural sharing but depends on
  a crate with internal `unsafe` code.
- No backend is claimed to be production-ready until benchmark evidence and
  assurance criteria are met.
