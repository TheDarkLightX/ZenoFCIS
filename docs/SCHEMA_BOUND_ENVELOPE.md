# Schema-Bound Admitted Envelope

## Scope

This package supplies immutable schema-admission witnesses in `zeno-fcis-schema`.
`SchemaAdmittedEnvelope` remains root-only. `SchemaAdmittedTypeEnvelope` binds an
explicit selected type for generated command, context, and other non-root values.

Both constructors validate one owned `Value` against the selected schema type,
perform the existing default structural value admission, compute the schema
commitment using the caller-selected `CommitmentHasher`, and admit the complete
envelope under the existing default decoder input limit.

Raw `Value`, `Envelope`, `AdmittedValue`, and `AdmittedEnvelope` behavior stays
unchanged. ZCVE/1 bytes, envelope framing, schema bytes, domains, type
identifiers, commitments, and protocol versions do not change.

## Inputs and outputs

`SchemaAdmittedEnvelope::try_new::<H>` consumes:

- a borrowed, already closed and normalized `Schema`;
- an owned root `Value`;
- explicit deterministic `ValidationLimits`;
- a compile-time `CommitmentHasher` selected by the integrating project.

It returns either:

- a sealed root envelope retaining the exact successful `ValidationReport`;
  or
- a closed local error identifying schema validation, default value admission,
  schema commitment, or complete-envelope admission.

The caller cannot supply a different type identifier or schema hash. The
constructor takes the root type from `Schema::root_type()` and computes the
schema hash from the exact supplied schema.

`SchemaAdmittedTypeEnvelope::try_new::<H>` additionally consumes an explicit
`TypeId`. It validates against that exact type and binds it into the canonical
envelope. Higher-level generated wrappers fix this argument to the reviewed
command or context type so application callers cannot substitute it.

## Authority boundary

The caller or integrating project retains authority to select:

- the reviewed schema;
- the hash provider;
- the schema-validation resource limits;
- the value to propose for admission.

The witness decides none of those inputs. It only checks and binds them.

Private fields prevent a caller from pairing an envelope with invented
validation metrics. The owned admitted value exposes no mutable access.
`CanonicalEncode for SchemaAdmittedEnvelope` delegates to the existing
`AdmittedEnvelope` bytes, so the wrapper adds no protocol field.

Local constructor failures have this explicit order:

1. root-schema validation;
2. default structural value admission;
3. schema commitment;
4. complete-envelope size admission.

This is an API diagnostic contract. It is not an application rejection
registry, does not alter stable reason precedence, and is not canonically
encoded as protocol meaning.

## Trusted dependencies

No dependency is added or changed. The implementation uses:

- `Schema::validate_root` and `Schema::validate_value`;
- `Schema::schema_hash`;
- `AdmittedValue::try_new`;
- `AdmittedEnvelope::try_new`;
- the caller-selected existing `CommitmentHasher`.

External hash-provider types remain hidden behind the existing provider trait.

## Deterministic resource bounds

Root-schema validation is bounded by the exact caller-supplied:

- maximum schema traversal depth;
- maximum visited value nodes.

The successful report retains:

- visited value nodes;
- maximum observed schema-validation depth.

Default structural value admission remains bounded by:

- maximum value depth: 64;
- maximum value nodes: 1,000,000;
- maximum aggregate byte and text payload: 64 MiB;
- maximum children in one collection: 1,000,000.

The complete envelope remains bounded to 64 MiB, including its 48-byte header.
Schema construction and canonical schema encoding retain their existing
bounds.

No ambient clock, randomness, filesystem, network, database, process state,
thread, global mutation, or interior mutability is consulted.

## Laws and positive cases

1. A witness exists only after successful validation against the selected
   schema type.
2. Root admission binds `Schema::root_type()`; selected-type admission binds
   the exact supplied `TypeId`.
3. The bound schema hash equals `Schema::schema_hash::<H>()`.
4. The retained validation report is the exact report from the successful
   root validation.
5. The bound value also satisfies `ValueLimits::default()`.
6. The complete envelope satisfies the default decoder input limit.
7. Raw admitted-envelope bytes and schema-bound envelope bytes are identical.
8. Default decoding recovers the exact raw envelope.
9. Reordering schema declarations does not change the envelope bytes.
10. Changing the schema version changes the schema commitment and envelope
    bytes.

## Negative and exact-boundary cases

- a value outside the root integer range is rejected;
- a root kind mismatch is rejected;
- exhausted schema-validation depth or node budget is rejected before later
  stages;
- non-ASCII text that satisfies the schema's length shape is rejected by
  default structural admission;
- a complete envelope beyond the default input ceiling is rejected by the
  existing admitted-envelope boundary;
- callers cannot inject an arbitrary type ID, schema hash, or validation
  report through public fields.

## Assumptions

- the supplied schema was selected and reviewed outside this constructor;
- the selected `CommitmentHasher` is appropriate for the integrating profile;
- the schema's stable identifiers and version were assigned by project
  authority;
- `Value`, `Schema`, and admitted wrappers remain transitively owned without
  interior mutation;
- admitted wrappers are reconstructed after an upgrade that changes any
  applicable default limit.

## Explicit nonclaims

- This does not choose or approve a schema, type ID, stable identifier, hash
  provider, migration, or release profile.
- It does not prove the selected hash provider cryptographically correct.
- It does not add application invariants beyond the supplied closed schema.
- It does not cache canonical bytes or promise constant-time execution.
- It does not establish wall-clock or allocation performance.
- It does not change protocol rejection precedence.
- Bounded tests are not an unbounded proof.
- This grants no production authority.
