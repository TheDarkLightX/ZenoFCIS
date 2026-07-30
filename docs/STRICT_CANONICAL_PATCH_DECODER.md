# Strict canonical patch decoder

`zeno-fcis-patch` exposes `decode_canonical_patch(bytes, limits)` for admitting
untrusted wire bytes into the existing `CanonicalPatch` type. The decoder is a
pure `no_std + alloc` boundary. It does not perform I/O or change the canonical
patch format.

## Inputs and outputs

Inputs are one immutable byte slice and one explicit `PatchDecodeLimits` value.
The successful output is a `CanonicalPatch` reconstructed through the existing
public constructor. Failure returns a typed `PatchDecodeError` and no partial
patch.

The limits bind:

- complete input bytes;
- operation count;
- path segments per operation;
- encoded bytes per map-key path segment;
- aggregate decoded value nodes;
- aggregate decoded value payload bytes;
- the existing per-value ZCVE depth, node, payload, collection, and input bounds.

The reviewed defaults admit at most 64 MiB of complete input, 4,096 operations,
256 segments in any path, 65,536 bytes in one encoded map key, 1,000,000 decoded
value nodes, and 64 MiB of decoded value payload. Callers may supply tighter
limits for a deployment or profile.

Initial vector reservation is also bounded by wire evidence. The decoder
reserves no more operation slots than the remaining bytes can contain as
length-prefixed operation blobs, and no more path segments than the path blob
can contain as tagged segments. A short count-only input cannot trigger a
reservation proportional to the declared maximum.

## Authority boundary

Raw bytes have no patch authority. The decoder first admits every nested value
and map key through the strict ZCVE/1 decoder, then reconstructs the patch through
`CanonicalPatch::try_new`. That constructor remains authoritative for insert
shape, map-key equality, canonical operation order, and ancestor/descendant
non-overlap.

Finally, the decoder canonically re-encodes the reconstructed patch and requires
exact equality with the complete input. Alternate operation order and any other
wire alias fail closed.

The decoder does not apply the patch. State authority still crosses the shell
boundary only through the existing expected-pre-root and old-value preconditions.

## Trusted dependencies

No dependency is added. The boundary trusts the existing:

- `zeno-fcis-codec` ZCVE/1 decoder and canonical encoder;
- `zeno-fcis-value` structural validation and metrics;
- `zeno-fcis-patch` constructors and invariants.

External database formats, Serde layouts, collection internals, and runtime
representations do not define patch bytes.

## Laws and negative cases

The permanent tests establish the following executable laws within their
declared examples and bounds:

1. Decoding canonical bytes reconstructs the original patch.
2. Re-encoding a successful decode reproduces the exact input.
3. Exact byte, operation, path, map-key, node, and payload boundaries are
   accepted; one-over inputs fail.
4. Declaration order that differs from canonical operation order fails.
5. Duplicate or ancestor/descendant paths fail through the constructor.
6. A semantic map key that differs from its encoded path key fails.
7. Unknown operation tags, unknown path tags, invalid flags, trailing bytes,
   truncation, and nested ZCVE limit violations fail.
8. Count-only operation declarations are rejected without reserving the
   declared number of elements.

## Assumptions

- The caller selects limits appropriate for the deployment before allocating
  or processing untrusted input.
- ZCVE/1 and the current patch encoding remain bound by their existing protocol
  identities.
- Successful decoding establishes syntactic and structural admission only;
  patch application still requires the correct state and commitment provider.

## Explicit nonclaims

- This does not change ZCVE/1 or canonical patch bytes.
- This does not decode `CommitPlan`, `OutboxPlan`, receipts, or bundles.
- This does not authenticate transport bytes or choose a commitment provider.
- This does not prove unbounded parser correctness or memory safety beyond the
  Rust and test/tool evidence retained for the exact source.
- This does not make ZenoFCIS production-authorized or complete the Core V1
  release gate.
- This does not impose a process-wide allocator quota. A valid large patch can
  still require memory proportional to its admitted input and logical values.
