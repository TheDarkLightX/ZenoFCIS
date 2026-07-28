# ZenoFCIS

ZenoFCIS is a high-assurance Rust library family for functional-core / imperative-shell systems.

Its primary design rule is:

```text
immutable state + command + policy + authenticated context
    -> pure total transition
    -> Accept | Reject | CommittedFailure
    -> exact immutable candidate
    -> atomic imperative-shell interpretation
```

The semantic kernel treats values, decisions, resource budgets, canonical bytes, and commitments as explicit protocol data. It forbids unsafe Rust and is designed for `no_std + alloc` use without clocks, randomness, networking, filesystems, databases, or executable effect closures.

## Start here

For a project-neutral multi-domain application, enable the composed-program
path and import the curated prelude:

```toml
[dependencies]
zeno-fcis = { version = "=1.0.0-rc.1", default-features = false, features = [
    "composed-program",
] }
```

```rust
use zeno_fcis::prelude::*;
```

The supported application path is:

```text
ProjectProfile + ProjectCatalog
    -> generated transition or typed domain machines
    -> ComposedDomainProgram
    -> verified project laws
    -> CatalogCommitAuthority
    -> authorized shell publication
```

Read the [installation guide](docs/INSTALLATION.md),
[quickstart](docs/QUICKSTART.md), [API reference](docs/API_REFERENCE.md),
[crate map](docs/CRATE_MAP.md), [feature matrix](docs/FEATURE_MATRIX.md), and
[LLM integration guide](docs/LLM_USAGE.md). The
[RC1 release notes](docs/RC1_RELEASE_NOTES.md) describe the exact candidate
surface and remaining final-release blockers.
Runnable examples are checked permanently:

```bash
cargo +1.97.1 run -p zeno-fcis --example minimal_core --locked
cargo +1.97.1 run -p zeno-fcis --example checked_backend --features backend --locked
```

The `full` feature is intended for workspace integration and exploration.
Reusable libraries should select only the features needed at their boundary.

## Implemented workspace

The workspace now includes the complete package ladder:

- semantic values with default-bounded text and byte helper admission, immutable value and envelope witnesses for repeat canonical encoding, decisions with explicit immutable budget reports, ZCVE/1 canonical encoding, exact commitments, preconditioned patches and closed commit/outbox plans with strict bounded canonical decoding, receipts, and complete candidate bundles;
- assume-guarantee composition, deterministic-parallel conflict checking,
  backend-neutral complete static footprint claims, nominal footprint witnesses,
  witness-gated parallel authorization, runtime refinement reports, promotion
  policy, canonical evidence envelopes, and the first ZenoDEX profile;
- fixed-size executable domain machines with schema-admitted state, command,
  context, and port matrices; narrow per-machine interfaces; routes derived
  exactly from global composition wiring; deterministic merge-order execution;
  global reject rollback; and terminal committed-failure preservation;
- one authority-owned composed-domain program with closed root-to-context maps,
  nominal machine ownership, complete state projection, fail-closed internal
  routing, and exact catalogued effect/outbox projection;
- vetted RustCrypto and libcrux SHA-256 providers with known-answer and cross-provider parity evidence;
- closed schema validation, root and selected-type schema-bound envelope admission, generated exact-schema and exact-catalog reconstruction, typed root/command/context smart constructors, derived command/context commitments, schema-typed direct root-field reads, updates, and context observations, raw-path-free generated mutation and context-observation surfaces, disposition-typed reason application, catalog-typed effect/channel staging, private-inner generated transitions, deterministic Rust/Python adapters, negative codec vectors, cross-language replay, and content-addressed generation manifests;
- project-neutral profiles with stable reason/effect/channel/capability/event registries, explicit evolution modes, and exact content-addressed migration evidence;
- tool-neutral, profile-bound relational-law manifests for state invariants, conservation, mint/burn authority, debit/credit-to-effect equality, fees and rounding, authority/subject/recipient relations, rejection purity, and committed-failure effects, with retained proof evidence and fresh bounded per-invocation evaluation;
- nominal catalog authorization that owns the reviewed transition program and exact project-law engine, admits an external command/context/principal/replay invocation, pins a sealed known-answer-verified provider plus exact interpreter/deployment/resource bindings, and creates a private-construction `CatalogAuthorizedTransition` only after every applicable law is satisfied;
- a reusable callable/strict JSON-line mounted-runtime adapter for complete normalized decisions from any project profile;
- strict JSON-line mounted-runtime adapters that compare complete normalized decisions and retain mismatch fixtures;
- a permanent exact-revision mount of the real ZenoDEX Python/Rust single-vault zUSD transitions, with a retained 17-case full-decision parity report;
- an explicit dual-root sparse authenticated-state reference with projector-bound profiles, context-verified membership/absence witnesses, expected-version publication, and full-rebuild equality checks;
- verifier-gated bounded synthesis over canonical closed candidate domains with content-addressed certificates and honest incomplete-search results;
- crash-atomic policy-pinned SQLite publication that consumes only nominally authorized transitions, persists exact authorization/invocation/replay/bundle identities, rejects legacy unversioned stores, owns a policy-bound interpreter instance for outbox delivery, and retains crash-point refinement tests;
- backend-independent persistent collections with reference, `rpds`, and `imbl` implementations, structural sharing, logical-entry equality, property tests, and benchmarks;
- release assurance with static effect-boundary checks, exact dependency and CI-action pins, RustSec/license/source policy, deterministic source manifests, Miri, and fuzz harnesses.

## Demonstrated ZenoDEX runtime mount

ZenoFCIS mounts the existing ZenoDEX single-vault zUSD functional core through
two thin JSON-line entry points: one calls the Python transition and one calls
the independent Rust transition. Both receive the same exact state, command,
context, and policy. ZenoFCIS normalization additionally binds the profile,
algorithm, schema, codec, precedence, and budget identities.

The retained v1 corpus executes 17 state-threaded cases. It currently produces
9 accepted transitions and 8 rejections with no Python/Rust divergence. For
each case, the mount compares the complete normalized decision:

```text
decision kind and reason
+ pre-state and post-state roots
+ candidate identity and CanonicalPatch
+ CommitPlan and OutboxPlan
+ receipt and CommitBundle
+ complete decision commitment
```

The permanent `mounted-zenodex` workflow checks out the exact pinned ZenoDEX
revision, builds its Rust runtime, runs both implementations, and byte-compares
the new report with the retained
[`fixtures/mounted-zenodex/zusd-v1/report.json`](fixtures/mounted-zenodex/zusd-v1/report.json).
The runner can also be invoked directly:

```bash
cargo +1.97.1 run -p zeno-fcis-adapter-zenodex \
  --bin mount-zenodex-zusd --locked -- \
  <pinned-zenodex-checkout> <zenodex-rust-binary> <output-directory>
```

This establishes bounded executable integration for the mounted profile. It
does not authorize production effects, cover multiple vaults or other ZenoDEX
lanes, or replace an audit or unbounded refinement proof.

The `zeno-fcis` umbrella keeps the semantic kernel small by default. Application
code should enable the smallest explicit feature set, for example:

```toml
[dependencies]
zeno-fcis = { version = "=1.0.0-rc.1", default-features = false, features = ["composed-program"] }
```

The umbrella crate's default and `no_std` feature sets are project-neutral.
Enable `zenodex-profile` for the ZenoDEX profile exports or `mounted-zenodex`
for that profile plus its concrete mounted runtime.

Important optional features include `authority`, `domain-machines`, `composed-program`, `codegen`,
`evidence`, `mounted-runtime`, `zenodex-profile`, `mounted-zenodex`,
`authenticated-state`, `synthesis`, `sqlite-shell`, `collections`, and
`persistent-collections`.

## Architecture

The repository keeps computation and coordination separate:

```text
pure transition
    -> immutable, content-addressed candidate
    -> external invocation + catalog/provider/deployment validation
    -> complete project-law evaluation
    -> nominal CatalogAuthorizedTransition
    -> policy-pinned atomic shell publication
    -> idempotent outbox delivery
```

Persistent backends are sealed behind a pure logical-map interface. Updates return new structurally shared versions; equality and canonical bytes depend on logical entries only. Map-entry ordering bytes are derived from the semantic key, the explicit persistent-entry boundary rejects mismatched key bytes, and materialization exposes only fallible APIs.

Concrete runtimes, databases, and synthesis engines remain outside the semantic authority boundary. Their adapters propose or store data; pure validators decide whether that data is admissible. A structurally valid `CommitBundle` remains reference data and cannot enter the production SQLite commit port directly.

The `zeno-fcis-domain` layer makes global composition executable without
introducing hidden shared state. Every component receives only its fixed state
row, one command, one context, and fixed typed input ports. The complete route
matrix is derived from `CompositionSpec`; state and invocation matrices bind
the exact executable composition and cannot be replayed across another
same-shaped topology. See
[`docs/FIXED_STATE_DOMAIN_MACHINES.md`](docs/FIXED_STATE_DOMAIN_MACHINES.md).
The production bridge is documented in
[`docs/COMPOSED_DOMAIN_PROGRAM.md`](docs/COMPOSED_DOMAIN_PROGRAM.md).
Its aggregate-root projection paths are required to equal the exact state paths
declared by the corresponding machine interfaces; the bounded law and its
nonclaims are documented in
[`docs/COMPOSED_ROOT_PROJECTION_CONFORMANCE.md`](docs/COMPOSED_ROOT_PROJECTION_CONFORMANCE.md).

## Verification

The main local gate is:

```bash
python3 tools/check_assurance.py --self-test
cargo +1.97.1 fmt --all -- --check
cargo +1.97.1 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.97.1 test --workspace --all-features --locked
```

See [release assurance](docs/RELEASE_ASSURANCE.md) for the full stable, `no_std`, Miri, fuzz, supply-chain, and source-manifest gates. Package-specific boundaries are documented in `docs/`.

RC packaging is fail closed and reviewable:

```bash
python3 tools/rc1_package.py self-test
python3 tools/rc1_package.py check
python3 tools/rc1_package.py build --output /tmp/zeno-fcis-rc1
```

The build retains all public `.crate` packages, rustdoc, source and diagnostic
binary archives, checksums, a CycloneDX SBOM, and provenance inputs. See the
[packaging reference](docs/PACKAGING.md).

## Assurance posture

Version `1.0.0-rc.1` is the first public API and packaging candidate for the
reusable core library. It is ready for downstream API evaluation and
integration testing, while remaining a pre-release candidate until the
independent exact-head review and final release gates pass. The pinned ZenoDEX
single-vault zUSD mount is bounded executable refinement evidence. Production
value-moving promotion still requires each profile's independently reviewed
laws and evidence, strict decoded SQLite bundle/outbox set reconstruction,
deployment qualification, and an exact-head audit. This RC does not claim
audit completion, project-specific economic correctness, side-channel
resistance, full ZenoDEX coverage, or approval of an external JMT, ESSO,
solver, prover, compiler, or LLM runtime.

## License

Dual-licensed under Apache-2.0 or MIT, at your option.
