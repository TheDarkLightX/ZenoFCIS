# ZenoFCIS crate map

ZenoFCIS is a workspace of small crates arranged around a pure semantic core.
The classes below describe architectural role, not current Cargo stability.
All workspace crates are version `1.0.0-rc.1`; the project has not published a
stable V1 API.

## Dependency direction

```text
values and canonical bytes
    -> patches, plans, receipts, profiles, schemas
    -> catalogs, transitions, laws, composition
    -> domain programs and authorization
    -> reference or concrete shells

external tools and runtimes
    -> bounded adapters and evidence
    -> pure validators and promotion policy
```

Semantic crates must not depend on databases, networks, clocks, randomness,
threads, or process state. Shell and tooling crates may perform effects but may
not redefine semantic decisions.

## Foundational semantic crates

These are project-neutral protocol primitives and the intended core library
surface.

| Crate | Responsibility |
|---|---|
| `zeno-fcis-core` | Three-way decisions, stable reasons, immutable logical budgets, transition trait |
| `zeno-fcis-value` | Transitively owned closed values and bounded admission |
| `zeno-fcis-codec` | ZCVE/1 canonical encoding, domain separation, commitments, strict value decoding |
| `zeno-fcis-patch` | Expected-root, preconditioned, canonical nonoverlapping state patches |
| `zeno-fcis-plan` | Closed authoritative effects and outbox obligations |
| `zeno-fcis-receipt` | Candidate sealing, receipts, reject receipts, and complete bundles |
| `zeno-fcis-project` | Project profiles, stable registries, compatibility, and migration bindings |
| `zeno-fcis-schema` | Closed schemas, type IDs, and schema-bound value admission |
| `zeno-fcis-catalog` | Executable reason, effect, channel, authority, schema, and limit rules |
| `zeno-fcis-transition` | Catalog-aware transition construction and complete decision validation |

Most foundational semantic crates support `no_std + alloc`.

## Project architecture and authority

These crates turn the foundational values into a safe application path.

| Crate | Responsibility |
|---|---|
| `zeno-fcis-compose` | Component contracts, footprints, complete-footprint witnesses, frames, wiring, conflicts, and proof-carrying parallel authorization |
| `zeno-fcis-domain` | Fixed-size typed domain machines and canonical sequential execution |
| `zeno-fcis-composed-program` | Root projection, nominal machine ownership, catalogued plan projection, and transition-program identity |
| `zeno-fcis-laws` | Complete relational-law manifests, executable checks, retained evidence, and verified law sets |
| `zeno-fcis-authority` | Exact invocation, provider, program, law, interpreter, deployment, replay, and commit authorization |

The recommended production path ends in a privately constructed
`CatalogAuthorizedTransition`. Raw bundles remain inspectable data.

## Formal, refinement, and synthesis surfaces

These are tool-neutral assurance protocols. They do not make a tool result
authoritative without an independent checker and release policy.

| Crate | Responsibility |
|---|---|
| `zeno-fcis-refine` | Exact normalized-decision comparison and promotion reports |
| `zeno-fcis-evidence` | Source/tool/query/assumption/coverage-bound evidence envelopes and importers |
| `zeno-fcis-synthesis` | Canonical bounded candidate enumeration and honest incomplete-search results |
| `zeno-fcis-backend` | Checked engine/verifier protocol for Lean, SMT/Z3, CVC5, Kani, private ESSO, compilers, and other tools |

Concrete tool clients normally live in a separate `std` shell crate. ESSO is
private and optional; it is not a public dependency or universal requirement.

## Security-support surfaces

| Crate | Responsibility |
|---|---|
| `zeno-fcis-crypto` | Pinned SHA-256 providers, known-answer checks, and provider parity |
| `zeno-fcis-secret` | Zeroizing secret containers, constant-time comparison primitives, and explicit exposure permits |
| `zeno-fcis-security` | Information-flow labels, leakage policies, side/covert-channel evidence, and promotion reports |

These APIs express boundaries and evidence requirements. They do not establish
compiled-code constant time, hardware isolation, or deployment security by
themselves.

## Reference implementations

Reference crates provide executable or differential oracles. They are useful
for tests, refinement, and adapter development. They are not automatically
production backends.

| Crate | Responsibility |
|---|---|
| `zeno-fcis-shell` | Pure atomic commit, replay, and acknowledgement reference semantics |
| `zeno-fcis-authenticated` | Configured projector-bound authenticated-state reference and context-verified sparse-proof witnesses |
| `zeno-fcis-collections` | Logical persistent-map interface plus reference, `rpds`, and `imbl` implementations |

`apply_reference_bundle` intentionally accepts raw structural data. Production
publication uses the nominal authorization boundary instead.

## Effectful shells and host tooling

| Crate | Responsibility |
|---|---|
| `zeno-fcis-shell-sqlite` | Crash-atomic, policy-pinned SQLite authorized publication and outbox delivery |
| `zeno-fcis-adapter` | Callable and strict JSON-line mounted-runtime comparison |
| `zeno-fcis-codegen` | Deterministic Rust/Python schema and vector generation |
| `zeno-fcis-bootstrap` | Deterministic project starter generation from a reviewed catalog |

These crates may use `std` because they access a host runtime, database, files,
or generated source. They must preserve the decisions produced by the semantic
core.

## Project-specific and fixture crates

| Crate | Responsibility |
|---|---|
| `zeno-fcis-profile-zenodex` | Initial ZenoDEX single-vault zUSD profile values and stable identifiers |
| `zeno-fcis-adapter-zenodex` | Mounted ZenoDEX Python/Rust runtime parity fixtures |
| `zeno-fcis-codegen-fixture` | Compiled fixture for generated-source assurance |

Applications should define their own profile, catalog, laws, machines, mounted
runtime, and interpreter. ZenoDEX behavior is not part of the generic
framework's meaning.

## Umbrella crate

`zeno-fcis` re-exports the project-neutral primitives and gates higher layers
behind features. Prefer it for application dependencies. Depend on individual
crates when publishing a narrowly scoped adapter or when a dependency ring
must remain explicit.

See [Feature matrix](FEATURE_MATRIX.md) and [Quickstart](QUICKSTART.md).
