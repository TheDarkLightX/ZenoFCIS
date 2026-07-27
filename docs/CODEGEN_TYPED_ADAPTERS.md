# Codegen Typed Adapters — Design Document

## Work Package A (Issue #6)

### Inputs

- A closed `zeno_fcis_schema::Schema` with stable type IDs, field IDs, and
  variant IDs. Source declaration order and names never define protocol
  meaning; numeric identifiers do.
- A `GenerationSpec` providing the Rust and Python module names.

### Outputs

The generator produces a `GeneratedBundle` containing:

1. **Rust typed adapters** (`rust/<module>.rs`): ordinary structs, enums, and
   newtypes with strict `to_value`/`try_from_value` conversions, typed
   patch-path constructors, exact schema reconstruction on the generated root,
   and a root-only `to_root_envelope` smart constructor. No procedural macros.
2. **Python typed adapters** (`python/<module>.py`): ordinary Python classes
   with strict `to_value`/`try_from_value` conversions and a `replay()`
   function.
3. **Python ZCVE/1 codec** (`python/zcve.py`): a fixed encoder/decoder module
   used for vector replay parity.
4. **Codec vectors** (embedded in both Rust and Python): positive, boundary,
   malformed, non-canonical, unknown-field, unknown-variant, and trailing-byte
   vectors with expected decode/validate outcomes.
5. **Canonical schema bytes** (`schema.zcve`): exact ZCVE/1 encoding of the
   source schema.
6. **Manifest** (`MANIFEST.zfcis`): content-addressed binding of generator ID,
   formatter ID, schema hash, vector-set hash, per-file hashes, and per-vector
   hashes.

### Authority

- The **schema** is the sole authority for type structure, field admission, and
  variant admission.
- The **generator** (`zeno-fcis-codegen/1`) is the sole authority for source
  text formatting and manifest construction.
- The **formatter** (`zeno-fcis-codegen-renderer/1`) is the sole authority for
  rendering style (indentation, ordering, naming conventions within generated
  text).
- Stable numeric identifiers (type IDs, field IDs, variant IDs) drive every
  emitted reference. Source names are cosmetic.
- The generated root helper may admit a value under caller-supplied validation
  limits and commitment provider. It cannot choose the schema or silently
  accept a schema commitment different from the value embedded at generation.

### Bounds

- Maximum recursive nesting depth: 64 (codec decode), 128 (schema validation).
- Maximum collection length: enforced by `DecodeLimits` and `ValidationLimits`.
- All generated constants are `u32` (type IDs) or `u16` (field/variant IDs).
- Generated text is ASCII-only.
- Generated schema reconstruction uses exact finite limits derived from the
  source schema's type and maximum field, variant, and tuple cardinalities.
- Root-envelope value validation uses explicit caller-supplied
  `ValidationLimits`; later structural and complete-envelope admission retains
  the lower layers' existing bounds.

### Laws

1. **Byte-identical regeneration**: `generate(schema, spec)` is a pure
   function. Repeated calls produce byte-identical bundles.
2. **Manifest binding**: the manifest binds the schema hash, vector-set hash,
  and per-file/per-vector SHA-256 commitments. Any change to inputs changes
  the manifest hash.
3. **Positive vectors round-trip**: every positive and boundary vector decodes
   successfully and validates against its declared type.
4. **Negative vectors reject**: every negative vector is rejected at the decode
   stage (malformed, non-canonical, trailing) or at the validation stage
   (unknown field, unknown variant) with the expected error category.
5. **Cross-language parity**: the Python ZCVE/1 codec replays all vectors with
   the same decode outcomes as the Rust codec.
6. **No hidden macros**: generated Rust contains only ordinary items. No
   procedural macros, no `include_str!` of generated content, no build-time
   code execution beyond the generator itself.
7. **Exact schema reconstruction**: the generated root reconstructs the same
   canonical `Schema` as the generator input.
8. **Bound root admission**: a successful generated root smart constructor
   binds the exact root type, schema hash, value, and validation metrics.
9. **Fail-closed provider check**: a runtime hasher whose schema commitment
   differs from the embedded generation-time commitment is rejected.

### Non-Claims

- The generator does **not** verify schema correctness; that is the schema
  crate's responsibility.
- The generator does **not** enforce protocol semantics (e.g., business logic
  constraints); it only enforces structural typing.
- The Python codec is **not** a production implementation; it is a minimal
  replay parity tool. Production systems use the Rust codec.
- The generated adapters are **not** zero-copy; they own their data.
- The root smart constructor does **not** select a hash provider, schema,
  profile version, or stable identifier, and it does not establish business
  invariants or production authority.
