# Closed schema and code-generation boundary

ZenoFCIS schemas are immutable protocol values. They fix the profile name and version, root type, type identifiers, field identifiers, variant identifiers, bounds, and the complete nonrecursive value graph. Construction sorts unordered declarations, rejects duplicates and unknown references, and rejects recursive cycles.

A schema is not inferred from Rust layout, Serde behavior, source declaration order, or a procedural macro expansion. It has its own canonical bytes and domain-separated commitment.

## Value admission

`Schema::validate_value` checks the complete owned value recursively under an explicit node/depth budget. Validation covers integer intervals, byte and text lengths, tuple and record shape, enum and sum identity, payload presence, vector bounds, and map key/value types.

Successful validation does not authenticate provenance or establish a business invariant. Those remain transition/profile obligations.

## Inspectable generation

`zeno-fcis-codegen` emits:

- ordinary Rust constants;
- ordinary Rust schema reconstruction and a root-only schema-bound envelope
  smart constructor;
- ordinary Python constants;
- exact canonical schema bytes;
- a content-addressed generation manifest.

The output exposes every type, field, and variant identifier and embeds the schema commitment. The generated root helper converts the typed root, reconstructs the exact source schema under finite generated limits, performs complete `SchemaAdmittedEnvelope` admission under caller-supplied validation limits, and rejects a runtime provider whose schema commitment differs from the embedded value. It contains no hidden source transformation and can be regenerated and diffed independently.

Generator semantics are versioned separately from schema semantics. Changing formatting without changing identifiers or schema bytes changes generated-file evidence but not protocol meaning. Changing a schema identifier, bound, or type graph changes the schema commitment and requires an explicit profile migration.

The helper is an admission path, not schema-selection or promotion authority. See [generated root-envelope smart constructors](GENERATED_ROOT_ENVELOPE.md) for its bounds, laws, and explicit nonclaims.
