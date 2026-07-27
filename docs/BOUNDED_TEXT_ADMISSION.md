# Bounded Text Admission

## Scope

This package closes the length-admission gap in `Value::text_ascii`. A
successful call now guarantees that its one ASCII text leaf does not exceed
the existing default payload ceiling. ZCVE/1 bytes, tags, decode behavior,
schema identifiers, and profile versions do not change.

The public `Value` algebra and generated schema adapters are not redesigned in
this package.

## Inputs and outputs

`Value::text_ascii` accepts one owned Rust `String` and returns either:

- `Value::Text` containing the same bytes;
- `TextError::NonAscii`; or
- `TextError::TooLong` carrying exact minimum, maximum, and actual byte lengths.

The maximum is `ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES`, currently 64 MiB.
ASCII byte length and character count are identical.

## Authority boundary

The constructor owns admission of a single text leaf. It does not own an
enclosing value's aggregate resource decision. Callers must still invoke
`Value::validate_limits` or canonical encoding after composing several leaves.

Schema-specific minimum and maximum lengths remain owned by the reviewed
schema and its generated adapters. This constructor cannot widen those bounds.

## Trusted dependencies

No dependency is added or changed. The implementation uses Rust `String`
ASCII and length operations, `TextError`, `LengthError`, and the existing
default `ValueLimits` payload ceiling.

## Deterministic resource bounds

- one text leaf: at most 64 MiB under the default constructor;
- aggregate owned byte and text payload: at most 64 MiB under default value
  validation;
- ZCVE/1 length encoding: unchanged `u32` length;
- admission work: one ASCII scan and one constant-time length comparison;
- no ambient clock, randomness, filesystem, network, thread, process, or
  mutable global state.

The input `String` is already allocated by the caller. Rejection returns no
partial `Value`.

## Laws and positive cases

1. Every successful `Value::text_ascii` result is ASCII.
2. Every successful text leaf is within the default payload ceiling.
3. An exact-bound ASCII input succeeds unchanged.
4. Existing admitted strings produce byte-identical ZCVE/1 output.
5. The public default text ceiling and `ValueLimits::default()` payload limit
   are derived from the same constant.

## Negative cases

- one byte above the selected ceiling returns exact `TextError::TooLong`;
- non-ASCII input returns `TextError::NonAscii`;
- when both conditions apply, the existing non-ASCII error precedence is
  preserved;
- aggregate overflow from several admitted leaves is rejected later by
  `Value::validate_limits` or canonical encoding.

Small-limit unit tests exercise exact-bound and one-over behavior through the
same internal routine used by the public default constructor, avoiding a 64
MiB allocation in Miri and routine unit tests.

## Assumptions

- `ValueLimits::DEFAULT_MAX_PAYLOAD_BYTES` retains its reviewed default meaning;
- ASCII remains the only text profile admitted by ZCVE/1;
- callers validate the complete enclosing value before it becomes protocol
  authority.

## Explicit nonclaims

- This does not make every raw `Value` enum variant unrepresentable when
  invalid; complete-value validation remains authoritative.
- It does not add schema-specific text types or alter generated adapter layout.
- It does not bound caller allocation before the `String` reaches the
  constructor.
- It does not change ZCVE/1, schemas, roots, commitments, or migrations.
- Bounded tests are not an unbounded proof or a production-readiness claim.
