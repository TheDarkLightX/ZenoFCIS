# Correct-by-Construction Map Entries

## Scope

This package removes the public construction path that accepted encoded map-key
bytes independently from the semantic key. It changes only map-entry
construction and the location of the existing ZCVE/1 value encoder. It does not
change ZCVE/1 bytes, map ordering, state roots, schemas, stable identifiers, or
patch semantics.

## Inputs and outputs

`MapEntry::try_new(key, value)` accepts two owned immutable `Value` objects. It
derives the exact ZCVE/1 bytes of `key`, stores those bytes with the semantic
key, and returns either one immutable `MapEntry` or a `ValueError`. No caller can
supply the stored ordering bytes.

`LogicalEntry::try_new(encoded_key, key, value)` remains an explicit boundary
for persistent collection adapters that receive encoded keys. It returns an
entry only when the supplied bytes exactly equal the canonical encoding of the
semantic key. Its previous unchecked public constructor is removed.

## Authority boundary

Canonical key bytes are protocol meaning. The single low-level value encoder
now lives in `zeno-fcis-value`, where `MapEntry` can use it without a dependency
cycle. `zeno-fcis-codec::CanonicalEncode for Value` delegates to that same
implementation. The codec remains the public encoding, decoding, envelope, and
commitment interface.

Persistent backends may reconstruct `LogicalEntry` values only from parts that
entered through the checked constructor. Their internal node shapes, insertion
history, and allocation identity remain non-authoritative.

## Trusted dependencies

No dependency is added. The implementation uses only `core`, `alloc`, and the
existing ZenoFCIS crates. External collection types remain hidden behind the
existing sealed persistent-map adapters.

## Deterministic resource bounds

Key encoding uses `ValueLimits::default()`:

- maximum depth: 64;
- maximum nodes: 1,000,000;
- maximum aggregate payload bytes: 64 MiB;
- maximum children in one collection: 1,000,000.

Construction returns no entry when the key exceeds those bounds. Map ordering
and duplicate rejection remain bounded by the enclosing `Value::map_canonical`
or `Value::normalize_map` call. No ambient clock, randomness, filesystem,
network, process, thread, or mutable global state is consulted.

## Laws and evidence

1. `MapEntry::try_new(k, v).encoded_key == ZCVE1(k)`.
2. The public codec and map-entry construction use the same encoder function.
3. The existing ZCVE/1 map golden bytes remain exact.
4. Duplicate semantic keys remain duplicate encoded keys and are rejected.
5. Decode rejects a wire key whose decoded value does not re-encode to the
   exact admitted bytes.
6. Patch insert rejects a semantic key that does not match its encoded path.
7. Generated Rust adapters construct map entries only through `try_new`.
8. Every persistent backend materializes the same checked `Value::Map`.
9. Materialization rejects a backend-stored key that differs from the semantic
   key's derived ZCVE/1 bytes.

Compile-fail doctests prevent reintroduction of the former public
`MapEntry::new(encoded_key, key, value)` and
`LogicalEntry::new(encoded_key, key, value)` APIs. Existing differential tests
cover insertion-history independence, snapshot retention, removal, and parity
between the reference, `rpds`, and `imbl` backends.

## Negative cases

- non-ASCII, structurally noncanonical, or over-limit keys fail construction;
- duplicate encoded keys fail map construction;
- mismatched explicit bytes fail `LogicalEntry::try_new`;
- mismatched backend-stored bytes fail materialization;
- noncanonical or trailing wire bytes fail decode;
- a patch map-key path that differs from the semantic key fails closed;
- generated adapters propagate `ValueError` instead of creating a partial map.

## Assumptions

- ZCVE/1 tags and field widths retain their existing protocol assignments;
- `ValueLimits::default()` remains the canonical admission envelope used by the
  public codec;
- persistent backends receive entries only through the sealed public trait;
- stored backend entries originated from `LogicalEntry::try_new`.

## Explicit nonclaims

- this is not a proof that ZCVE/1 is injective;
- compile-fail tests and bounded runtime tests are not unbounded verification;
- no collection backend is promoted to a production default;
- no authenticated-tree, database, migration, or project-profile claim is
  added.

The subsequent fallible-only materialization boundary is documented in
`FALLIBLE_PERSISTENT_MAP_API.md`.
