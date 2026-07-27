# Bounded Byte Admission

## Scope

This package gives `Value::bytes` the same default payload admission boundary
as `Value::text_ascii`. A successful call guarantees that its one byte leaf
does not exceed the existing 64 MiB default ceiling. ZCVE/1 bytes, tags,
decoder behavior, schemas, identifiers, and profile versions do not change.

The public `Value` algebra and generated schema adapters are not redesigned in
this package.

## Inputs and outputs

`Value::bytes` accepts one owned `Vec<u8>` and returns either:

- `Value::Bytes` containing the same bytes; or
- `LengthError` carrying exact minimum, maximum, and actual byte lengths.

The maximum is `ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES`, currently 64 MiB.

## Authority boundary

The constructor admits one byte leaf. It does not own an enclosing value's
aggregate resource decision. Callers must still invoke `Value::validate_limits`
or canonical encoding after composing several leaves.

Schema-specific byte minima and maxima remain owned by reviewed schemas and
generated adapters. The code generator maps default-admission failure to its
existing `VectorConstruction` failure. The backend's fixed 32-byte certificate
binding fails closed to `Indeterminate` if admission ever becomes impossible.

## Trusted dependencies

No dependency is added or changed. The implementation uses `Vec::len`,
`LengthError`, and the existing `ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES`
constant.

## Deterministic resource bounds

- one helper-constructed byte leaf: at most 64 MiB;
- aggregate default byte/text payload: at most 64 MiB;
- ZCVE/1 length field: unchanged `u32`;
- admission work: one constant-time length comparison;
- no ambient clock, randomness, filesystem, network, thread, process, or
  mutable global state.

The input vector is already allocated by the caller. Rejection returns no
partial `Value`.

## Laws and positive cases

1. Every successful `Value::bytes` leaf is within the default payload ceiling.
2. An exact-bound input succeeds with identical bytes.
3. Existing admitted byte strings produce byte-identical ZCVE/1 output.
4. Several admitted leaves remain subject to aggregate validation.
5. Codegen and backend callers propagate or fail closed on admission failure.

## Negative cases

- one byte above the selected ceiling returns exact `LengthError` fields;
- aggregate overflow from several admitted leaves is rejected later by
  `Value::validate_limits` or canonical encoding;
- a backend certificate leaf that cannot be admitted produces
  `CheckResult::Indeterminate`, never an uncertified counterexample.

Small-limit unit tests exercise exact-bound and one-over behavior through the
same internal routine as the public constructor, avoiding a 64 MiB allocation
in Miri and routine unit tests.

## Compatibility

This is an intentional pre-1.0 source compatibility break:

```text
Value::bytes(bytes) -> Value::bytes(bytes)?
```

Existing inputs within the default ceiling keep identical values and canonical
bytes.

## Assumptions

- the default payload ceiling retains its reviewed meaning;
- callers validate a complete enclosing value before it becomes protocol
  authority;
- a SHA-256 certificate remains exactly 32 bytes.

## Explicit nonclaims

- This does not make every raw `Value` enum variant unrepresentable when
  invalid; complete-value validation remains authoritative.
- It does not alter generated adapter layout or schema-specific byte bounds.
- It does not bound allocation before the input vector reaches the constructor.
- It does not change ZCVE/1, schemas, roots, commitments, or migrations.
- Bounded tests are not an unbounded proof or a production-readiness claim.
