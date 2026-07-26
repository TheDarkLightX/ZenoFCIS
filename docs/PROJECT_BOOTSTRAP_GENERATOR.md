# Project bootstrap generator

`zeno-fcis-bootstrap` is a deterministic tooling layer that converts one reviewed `ProjectCatalog` into an inspectable starter bundle. It does not infer a schema, allocate identifiers, choose precedence, or invent policy.

## Inputs

- one immutable `ProjectCatalog`;
- the exact `CommitmentHasher` used by that catalog;
- validated package, Rust module, and Python module names;
- explicit file-count, per-file-byte, and aggregate-byte limits.

## Outputs

The generated bundle contains:

- the complete existing schema-codegen bundle, including typed Rust/Python values and positive/negative vectors;
- canonical profile and catalog bytes;
- the complete stable registry as a readable table;
- typed Rust reason, effect, and channel identifiers;
- effect and outbox smart constructors with schema-generated payload and destination types;
- a typed transition-construction entry point;
- Rust and Python mounted-runtime skeletons pointing at the common adapter boundary;
- a migration stub that grants no migration authority;
- an evidence manifest whose requirements begin unsatisfied;
- a pinned, read-only CI template;
- an architecture document;
- a content-addressed bootstrap manifest.

## Authority boundary

The catalog remains authoritative for schema types, stable identifiers, reason disposition and precedence, effect authority rules, channel shapes, registry commitments, and resource limits. The generator controls only deterministic file selection and rendering.

Generated smart constructors enforce Rust-level payload/destination type selection. Final `ProjectCatalog` validation remains mandatory because authority and subject commitments, aggregate limits, and project policy are catalog obligations.

The migration and evidence files are explicit empty obligations. They cannot promote a profile, prove a theorem, authorize an incompatible migration, or attest runtime refinement.

## Deterministic bounds

`BootstrapLimits` bounds file count, each file's bytes, and aggregate bytes. Each bound has a hard library maximum. Existing schema codegen separately bounds schemas, generated adapters, and vector construction. Its fixed provider identity remains visible in the nested generation manifest; the bootstrap manifest binds both that identity and the selected catalog provider.

Generation fails atomically. No partial bundle is returned after a collision, hash-provider mismatch, invalid name, unknown referenced schema type, or resource overflow.

## Laws and negative cases

- repeated generation from equal catalog/spec inputs is byte-identical;
- catalog declaration order cannot change output;
- generated paths are strictly ordered and unique;
- the manifest binds every non-manifest file, the exact catalog/profile/schema-generation identities, generator version, and hash-provider identity;
- typed constructors use the exact stable effect/channel IDs and schema type names;
- evidence and migration output starts explicitly incomplete;
- invalid package/module names, wrong hash provider, one-over file limits, and file collisions fail closed;
- generated source is compiled in the permanent fixture crate before review.

## Trusted dependencies

No new external library is used. The generator trusts the existing catalog, project, schema, codec, and schema-codegen crates plus the selected commitment provider.

## Assumptions

- the input catalog was reviewed and constructed with the selected provider;
- generated output is installed without modification, or modifications receive a new retained artifact digest;
- downstream projects commit a reviewed lockfile before enabling the generated locked CI template;
- downstream projects supply business transition logic and real mounted runtimes.

## Explicit nonclaims

- A generated starter is not a complete application or production deployment.
- Generated CI does not prove business correctness.
- A typed constructor does not satisfy authority policy until catalog validation succeeds.
- Runtime, migration, and evidence skeletons contain no implementation or proof.
- Bounded regeneration and fixture tests are not unbounded formal verification.
