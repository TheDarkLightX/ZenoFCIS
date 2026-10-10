# ZenoFCIS quickstart

ZenoFCIS helps a project express authoritative state changes as deterministic,
inspectable values:

```text
ProjectProfile + ProjectCatalog
    -> generated transition or typed domain machines
    -> ComposedDomainProgram
    -> verified project laws
    -> CatalogCommitAuthority
    -> authorized shell publication
```

The workspace is currently version `0.1.0` and remains pre-release. The APIs
below are implemented, but no Cargo API stability or general production
qualification is claimed yet.

## 1. Choose the smallest feature set

Use the umbrella crate when starting a project:

```toml
[dependencies]
zeno-fcis = { version = "=0.1.0", default-features = false, features = [
    "composed-program",
] }
```

Use the curated imports in application code:

```rust
use zeno_fcis::prelude::*;
```

Run the two compiled introductory examples:

```bash
cargo +1.97.1 run -p zeno-fcis --example minimal_core --locked
cargo +1.97.1 run -p zeno-fcis --example checked_backend --features backend --locked
```

`composed-program` enables the schema, catalog, transition, laws, authority,
fixed-domain-machine, and composed-program layers. The semantic path supports
`no_std + alloc`.

Add features only at the boundary that needs them:

```toml
# Host-side starter generation
zeno-fcis = { version = "=0.1.0", features = ["bootstrap"] }

# Crash-atomic SQLite publication
zeno-fcis = { version = "=0.1.0", features = ["sqlite-shell"] }

# Checked external engines such as an SMT, Lean, CVC5, or private ESSO adapter
zeno-fcis = { version = "=0.1.0", default-features = false, features = ["backend"] }
```

Avoid `full` in reusable libraries. It includes project-specific and reference
surfaces that most consumers do not need. See [Feature matrix](FEATURE_MATRIX.md).

## 2. Define reviewed project meaning

Start with closed values and identifiers:

1. Define a `Schema` for the root state, command, authenticated context, effect
   payloads, channel destinations, and channel payloads.
2. Define a `ProjectProfile` with stable registry entries and commitments for
   the schema, algorithms, rejection precedence, policies, effects, and
   channels.
3. Construct a `ProjectCatalog` with:
   - every rejection and committed-failure reason;
   - every authoritative effect and its authority/subject requirements;
   - every outbox channel and its destination and payload types;
   - deterministic plan and value limits.

These are protocol decisions. A generator, LLM, solver, or runtime must not
invent or renumber them.

For a new project, `zeno-fcis-bootstrap::generate_project` can turn an already
reviewed catalog into inspectable Rust/Python starter files, typed
reason/effect/channel APIs, negative vectors, adapter skeletons, and a CI
template. It does not choose the catalog or business rules.

Read:

- [Universal project profiles](UNIVERSAL_PROJECT_PROFILES.md)
- [Schema-bound catalog](SCHEMA_BOUND_CATALOG.md)
- [Project bootstrap generator](PROJECT_BOOTSTRAP_GENERATOR.md)

## 3. Implement the transition

For one bounded domain, use the generated private-inner transition API or
`CataloguedTransitionBuilder`. The generated API is preferred because it fixes
the reviewed paths, reason dispositions, effect IDs, channel IDs, and payload
types in ordinary Rust types.

For several domains:

1. Give each domain one `DomainMachine`.
2. Define its narrow `MachineInterface`, owned state cells, typed ports,
   footprints, reasons, and deterministic budget.
3. Define one `CompositionSpec` with explicit wiring and canonical merge order.
4. Construct an `ExecutableComposition`.
5. Construct one authority-owned `ComposedDomainProgram`.

`ComposedDomainProgram` projects the exact root state, command, and context into
fixed machine rows, executes the canonical sequential composition, and projects
one successor patch, commit plan, and outbox plan. It returns an ordinary
`TransitionDecision`; it grants no commit authority.

Read:

- [Fixed-size domain machines](FIXED_STATE_DOMAIN_MACHINES.md)
- [Composed domain program](COMPOSED_DOMAIN_PROGRAM.md)
- [Formal composition](FORMAL_COMPOSITION_V2.md)

## 4. Define and verify project laws

Schemas establish shape. Project laws establish relationships across the
complete transition, such as:

```text
post supply = pre supply + minted - burned
semantic debit = external transfer + explicit fee
Reject has no patch, effect, outbox entry, or candidate
the command asset equals the effect asset
the authority and recipient are the reviewed ones
```

Create a complete `LawManifest`, implement a pure `ProjectLawEngine`, retain
the evidence required by the manifest, and call `verify_project_laws`.
Successful verification produces `VerifiedProjectLaws`; each invocation still
receives a fresh bounded law evaluation.

Formal tools are optional adapters:

- public users can mount Lean, SMT/Z3, CVC5, Kani, Flux, or another checker;
- private ESSO users can implement the same public verifier/backend traits in a
  private crate;
- timeout, crash, unsupported input, disagreement, and `unknown` grant no
  authority.

Read [Project relational laws](PROJECT_RELATIONAL_LAWS.md) and
[Generic backend protocol](GENERIC_BACKEND_PROTOCOL.md).

## 5. Own production authority

Construct one `CatalogCommitAuthority` at application startup. The authority,
not the request caller, owns:

- the exact `ProjectCatalog`;
- approved commitment provider;
- transition program and build identity;
- verified law set and exact law-engine type;
- state domain, interpreter, deployment, and replay-policy bindings;
- transition resource limits.

For each request, admit the exact pre-state, command, authenticated context,
principal, authentication evidence, and replay identity into an
`InvocationWitness`. Call `CatalogCommitAuthority::execute`.

The result is:

- an authorized rejection, which cannot be committed; or
- a privately constructed `CatalogAuthorizedTransition` for `Accept` or
  `CommittedFailure`.

Do not accept a caller-created `CommitBundle`, `TransitionDecision`,
`NormalizedDecision`, law result, provider, or interpreter at this boundary.

Read [Catalog authorization boundary](CATALOG_AUTHORIZATION_BOUNDARY.md).

## 6. Publish through an authorized shell

Use `AuthorizedShellState` for the pure authorized publication model or enable
`sqlite-shell` for the crash-atomic SQLite adapter. The shell publishes the
exact authorized state, receipt, replay binding, and outbox obligations under
expected-root compare-and-swap.

`zeno-fcis-shell::apply_reference_bundle` accepts raw `CommitBundle` data only
as reference semantics. It is not a production commit port.

External delivery stays in the imperative shell. It must interpret the exact
committed outbox entry idempotently and under the authority-owned interpreter.

## Project adoption checklist

- [ ] Closed schema and stable registries are owner reviewed.
- [ ] Rejection precedence is total and versioned.
- [ ] Every effect and channel has schema, authority, and deterministic limits.
- [ ] Commands and context are admitted under exact role-separated bindings.
- [ ] Domain machines expose narrow typed interfaces and complete footprints.
- [ ] The canonical sequential composition is deterministic and budgeted.
- [ ] Required invariants, conservation, authority, and failure laws are complete.
- [ ] Formal evidence is checked by the release-selected verifier.
- [ ] The authority owns the program, laws, provider, interpreter, and deployment.
- [ ] Only `CatalogAuthorizedTransition` reaches the production commit port.
- [ ] Mounted implementations are compared over complete normalized decisions.
- [ ] Crash, replay, CAS-conflict, and outbox behavior are tested.

## Current nonclaims

ZenoFCIS does not currently provide:

- Cargo API stability or a completed official V1 release;
- a concurrent scheduler, threaded shell, or production parallel runtime;
- proof that handwritten footprints are complete;
- automatic invention of project schemas, identifiers, laws, or policies;
- a bundled universal solver, theorem prover, ESSO installation, or proof of a
  checker implementation;
- automatic economic correctness, requirement completeness, or deployment
  qualification;
- general production qualification for authenticated state, SQLite delivery,
  Solidity, Solana, operating systems, or hardware;
- an end-to-end mechanized proof for arbitrary projects.

The deterministic-parallel APIs are proof-carrying planning and promotion
surfaces. Canonical sequential execution remains the normative runtime oracle.
