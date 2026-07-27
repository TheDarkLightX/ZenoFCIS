# Admitted Envelope

## Scope

This package adds an immutable `AdmittedEnvelope` for callers that repeatedly
encode the same typed `Envelope`. It owns an `AdmittedValue`, measures the
canonical payload once, and admits the complete envelope only when its exact
encoded length fits `DecodeLimits::DEFAULT_MAX_INPUT_BYTES`.

Raw `Envelope` construction, encoding, decoding, and compatibility remain
unchanged. Envelope magic, ZCVE/1 payload bytes, type identifiers, schema
commitments, and length fields do not change.

## Inputs and outputs

`AdmittedEnvelope::try_new` consumes:

- a caller-selected `u32` type identifier;
- a caller-supplied schema commitment;
- an immutable `AdmittedValue` created under `ValueLimits::default()`.

It returns either:

- a sealed envelope retaining the exact canonical payload length; or
- an existing encoding failure, including `EnvelopeInputLimit` when the
  complete envelope exceeds the reviewed default input bound.

`AdmittedEnvelope::try_from_envelope` consumes a raw envelope, performs default
value admission, and applies the same complete-envelope bound.

## Authority boundary

The wrapper proves only structural default admission and complete encoded-size
admission. Its fields are private, its value is owned, and it exposes no
mutable access. A caller cannot replace the value after its payload length is
retained.

The wrapper does not decide whether the caller-selected type identifier and
schema hash are correct. That remains the responsibility of a reviewed schema
or generated adapter in a higher dependency ring.

`CanonicalEncode for AdmittedEnvelope` emits the existing envelope header and
the exact underlying ZCVE/1 value bytes. The retained payload length is the
same existing four-byte canonical length field, not new protocol data.

## Trusted dependencies

No dependency is added or changed. The implementation uses:

- the existing `AdmittedValue` invariant;
- the existing shared ZCVE/1 encoder;
- the existing raw `Envelope` format;
- `DecodeLimits::DEFAULT_MAX_INPUT_BYTES`.

## Deterministic resource bounds

Inner value admission remains bounded by:

- maximum depth: 64;
- maximum nodes: 1,000,000;
- maximum aggregate byte and text payload: 64 MiB;
- maximum children in one collection: 1,000,000.

The complete admitted envelope is bounded to 64 MiB, including:

- 8 bytes of envelope magic;
- 4 bytes of type identity;
- 32 bytes of schema commitment;
- 4 bytes of payload length;
- the exact canonical payload.

Construction performs one canonical payload encoding into a temporary owned
buffer to retain its length. Each later envelope encoding performs one direct
payload traversal, does not repeat value validation, and does not allocate a
separate payload buffer. No canonical bytes are cached.

No ambient clock, randomness, filesystem, network, process state, thread,
global mutation, or interior mutability is consulted.

## Laws and positive cases

1. Construction succeeds only for an already default-admitted value whose
   complete envelope fits the default decoder input limit.
2. The retained payload length equals the exact canonical value byte length.
3. The reported encoded length equals the 48-byte header plus that payload.
4. Raw and admitted envelope encodings are byte-identical.
5. Repeated admitted encoding is byte-identical.
6. Default decoding of admitted bytes recovers the exact raw envelope.
7. Consuming the wrapper recovers the exact raw envelope or admitted value.
8. Raw `Envelope` behavior remains unchanged.

## Negative and exact-boundary cases

- an envelope with raw non-ASCII text cannot be admitted;
- a complete envelope one byte above its admitted input limit is rejected;
- a complete envelope exactly at its admitted input limit is accepted;
- callers cannot forge a retained payload length through public fields;
- an `AdmittedValue` near its own payload ceiling may still be rejected when
  envelope framing would exceed the complete-input ceiling.

## Assumptions

- `ValueLimits::default()` remains the reviewed inner admission envelope;
- `DecodeLimits::DEFAULT_MAX_INPUT_BYTES` remains the reviewed complete-input
  ceiling;
- `Value` remains transitively owned without interior mutation;
- raw and admitted envelopes continue to share the same canonical format and
  value encoder;
- admitted wrappers are reconstructed after an upgrade that changes either
  default limit.

## Explicit nonclaims

- This does not validate the type identifier against a schema.
- It does not prove that the schema commitment names a supplied schema.
- It does not remove validation from raw `Envelope` or raw `Value` encoding.
- It does not cache canonical bytes or promise constant-time execution.
- It does not establish a wall-clock or allocation performance result.
- It does not change ZCVE/1, stable identifiers, roots, commitments,
  migrations, or protocol versions.
- Bounded tests and structural review are not an unbounded proof.
- This grants no production authority.
