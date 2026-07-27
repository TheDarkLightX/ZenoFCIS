# Generated root-envelope smart constructors

## Inputs

- A closed, canonical `zeno_fcis_schema::Schema` accepted by the existing
  generator.
- The generator's pinned RustCrypto SHA-256 provider, which computes the exact
  schema commitment embedded in the generated Rust source.
- At runtime, a generated root value, caller-selected `CommitmentHasher`, and
  explicit `ValidationLimits`.

The generator does not infer a schema from Rust layout. The reviewed schema
remains the source of type, field, variant, bound, root, profile, and version
meaning.

## Outputs

The generated Rust root type exposes two associated functions:

```rust
Root::zfcis_schema() -> Result<Schema, SchemaError>

root.to_root_envelope::<H>(limits)
    -> Result<SchemaAdmittedEnvelope, AdapterError>
```

`zfcis_schema` reconstructs the complete closed schema as ordinary,
inspectable Rust. `to_root_envelope` converts the typed root to `Value`, admits
it against that reconstructed root schema, binds the successful validation
report and canonical envelope, and checks that the resulting schema commitment
equals the commitment embedded at generation time.

## Authority boundary

- Schema authors and reviewers choose the schema and its stable identifiers.
- The generator renders the already accepted schema; it cannot select or
  promote identifiers, bounds, profiles, versions, or hash providers.
- The caller supplies the runtime validation limits and commitment provider.
- The generated helper accepts only a provider whose result matches the exact
  schema commitment embedded in the generated source.
- `SchemaAdmittedEnvelope` remains the authority for schema validation,
  structural value admission, canonical envelope admission, and retained
  metrics.

Raw `Value` and low-level schema APIs remain available. The generated helper is
the smallest schema-bound path for generated roots; it does not redefine those
lower layers.

## Trusted dependencies

No new external dependency is added. Generation uses the existing pinned
RustCrypto provider. Generated code uses the existing `zeno-fcis-codec`,
`zeno-fcis-schema`, `zeno-fcis-value`, and `zeno-fcis-patch` public APIs. The
compiled fixture uses the existing workspace `zeno-fcis-crypto` package to
exercise the expected provider.

## Deterministic resource bounds

- Schema reconstruction uses exact generated `SchemaLimits`: the schema's type
  count and the largest record, enum or sum, and tuple cardinalities.
- Value validation uses the caller's explicit `ValidationLimits`.
- Structural value and complete-envelope admission retain the existing default
  value limits and 64 MiB complete-envelope byte bound.
- Every call reconstructs one finite schema and allocates one owned value and
  envelope. There is no cache, global mutable state, I/O, clock, randomness,
  thread, or async runtime.

## Admission order

Local diagnostic order is deterministic:

1. Convert the generated typed root to `Value`.
2. Reconstruct the generated schema.
3. Perform root schema validation, structural value admission, schema hashing,
   and complete-envelope admission through `SchemaAdmittedEnvelope`.
4. Compare the admitted schema hash with the embedded generation-time hash.

This order is local API behavior. It does not create or change protocol
rejection precedence.

## Laws and negative cases

- Reconstructed schema equality: the generated schema equals the source schema.
- Commitment equality: the reconstructed schema hashes to the embedded
  `SCHEMA_HASH_HEX` with the expected provider.
- Root binding: a successful helper result binds the generated root type, exact
  schema commitment, admitted value, and exact validation metrics.
- Provider parity: a hash-compatible independent provider may construct the
  same envelope.
- Provider mismatch rejection: a provider producing a different schema hash
  fails with `AdapterError::SchemaHashMismatch`.
- Failure precedence: an invalid generated field fails typed conversion before
  a later provider mismatch can be observed.
- Byte-identical regeneration and manifest binding continue to cover the
  changed generated source.

## Assumptions

- The reviewed schema and generation inputs are correct for the project.
- A provider intended for the generated helper implements the reviewed
  commitment algorithm and domain separation correctly.
- Callers choose validation limits appropriate for their deployment.
- Existing canonical encoding and schema-envelope laws hold.

## Explicit nonclaims

- This work does not choose a schema, stable identifier, provider, protocol
  version, generator identity, formatter identity, or migration.
- It does not prove the cryptographic provider, canonical codec, generated
  source compiler, or business invariants.
- It does not remove every low-level way to construct a `Value`, `Schema`, or
  envelope.
- It does not make generated adapters zero-copy, constant-time, allocation-free,
  or production-authorized.
- The compiled fixture and negative tests are bounded executable evidence, not
  an unbounded proof.
