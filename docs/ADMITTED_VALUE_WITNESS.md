# Admitted Value Witness

## Scope

This package adds an owned `AdmittedValue` wrapper for callers that encode the
same immutable `Value` repeatedly. Construction validates the exact reviewed
`ValueLimits::default()` envelope once. Canonical encoding through the witness
then emits the same ZCVE/1 bytes without repeating that separate validation
tree walk.

Raw `Value` canonical encoding remains unchanged and fail-closed. ZCVE/1,
schemas, identifiers, roots, commitments, and profile versions do not change.

## Inputs and outputs

`AdmittedValue::try_new` consumes one `Value` and returns either:

- an immutable owned witness containing the value and exact `ValueMetrics`; or
- the existing `ValueError` selected by default-limit validation.

The witness exposes immutable value access, exact metrics, consuming recovery
of the value, and append-only ZCVE/1 encoding. Its fields are private and there
is no mutable accessor.

## Authority boundary

`AdmittedValue` proves only that its owned value passed the current reviewed
default canonical admission envelope. Callers cannot construct a witness from
invented metrics or from a value admitted only under more permissive custom
limits.

The witness and metrics are not encoded. `CanonicalEncode for AdmittedValue`
encodes only the underlying value, so its output and commitments are identical
to canonical encoding of the admitted raw `Value`.

Raw `Value` variants remain publicly constructible. Their existing
`CanonicalEncode` implementation continues to validate before emitting bytes.

## Trusted dependencies

No dependency is added or changed. The implementation uses `core`, `alloc`,
the existing default `ValueLimits`, exact `ValueMetrics`, and the single shared
ZCVE/1 encoder in `zeno-fcis-value`.

## Deterministic resource bounds

Witness admission performs one complete deterministic validation bounded by:

- maximum depth: 64;
- maximum nodes: 1,000,000;
- maximum aggregate byte and text payload: 64 MiB;
- maximum children in one collection: 1,000,000.

Each subsequent encoding still traverses the admitted tree once to emit its
bytes. It omits the additional pre-encoding validation traversal. The witness
does not cache canonical bytes or add mutable global state.

No ambient clock, randomness, filesystem, network, process state, thread, or
interior mutability is consulted.

## Laws and positive cases

1. Witness construction succeeds exactly when the value passes
   `ValueLimits::default()` validation.
2. Stored metrics equal the exact metrics returned by that validation.
3. The witness owns the exact admitted value and exposes no mutable access.
4. Raw and witnessed canonical encoding produce byte-identical ZCVE/1 output.
5. Repeated witnessed encoding is byte-identical.
6. Consuming a witness recovers the exact original value.
7. Raw `Value` encoding retains its existing validation behavior.

## Negative cases

- raw non-ASCII `Value::Text` cannot become a witness;
- a structure admitted only by a more permissive depth limit cannot become a
  default witness;
- callers cannot construct `AdmittedValue` from public fields;
- malformed records, maps, excessive collections, nodes, payloads, or depth
  fail through the existing deterministic `ValueError` precedence.

## Assumptions

- `ValueLimits::default()` remains the reviewed canonical admission envelope;
- `Value` remains transitively owned and exposes no interior mutation;
- the single ZCVE/1 encoder remains shared by raw and witnessed encoding;
- witness values are reconstructed after a software upgrade that changes the
  default admission envelope.

## Explicit nonclaims

- This does not remove validation from raw `Value` encoding.
- It does not admit values under custom or more permissive limits.
- It does not cache bytes, promise constant-time execution, or establish a
  wall-clock performance result.
- It does not make every invalid raw `Value` unrepresentable.
- It does not change ZCVE/1, schemas, state roots, commitments, migrations, or
  production authority.
- Bounded tests and structural review are not an unbounded proof.
