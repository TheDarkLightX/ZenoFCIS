# ZenoFCIS release assurance

This document defines the release gate for the complete ZenoFCIS library family. A passing build is necessary evidence. It is not an audit, a deployment authorization, or proof that a downstream profile is economically correct.

The [V1 release checklist](V1_RELEASE_CHECKLIST.md) gives the ordered owner
procedure for freezing one exact commit, rerunning this gate, creating the
signed tag, publishing crates, retaining evidence, and responding to a partial
or failed release.

## Authority boundary

The pure crates define decisions, canonical values, patches, plans, receipts,
composition, refinement, authenticated-state planning, and bounded synthesis.
They cannot perform ambient I/O. Concrete effects enter through explicit
adapters and the SQLite shell. The authenticated-authority ring qualifies one
projector and per-transition projection relation, then exposes a nominal
candidate-bound publication value. The included sparse tree remains a bounded
reference writer.

The authority rule is:

```text
candidate generator or mounted runtime proposes data
    -> strict decoder reconstructs exact invocation-bound artifacts
    -> independent checker validates exact data and claimed coverage
    -> shell validates the complete candidate again
    -> one database transaction publishes state, replay binding, receipt, and outbox
```

Persistent collection backends are sealed implementations of one logical map interface. Equality and canonical materialization are defined by logical entries, not by backend nodes, allocation identity, or mutation history. Old versions remain usable because updates return a structurally shared successor.

## Version policy

Cargo package versions follow semantic versioning for the Rust API. Protocol compatibility is governed separately by explicit identifiers embedded in canonical data:

- domain name and domain version;
- codec and schema version;
- profile and algorithm hashes;
- generator and formatter identifiers;
- synthesis grammar, checker, and algorithm hashes;
- authenticated tree and activation profile identities.

A Rust patch release must not change canonical bytes, commitment preimages, stable reason precedence, or acceptance semantics under an existing protocol identifier. Such a change requires a new protocol identifier and migration evidence even when the Rust signature is unchanged.

During the `1.0.0-rc.*` series, ordinary Rust APIs may change between release
candidates. Existing protocol identifiers remain immutable. Final Cargo API
stability begins at `1.0.0`.

## Required release gate

Run from a clean checkout of the exact release commit:

```bash
python3 tools/check_assurance.py --self-test
python3 tools/check_assurance.py
python3 tools/rc_package.py self-test
python3 tools/rc_package.py check
python3 tools/atdd.py self-test
python3 tools/atdd.py check
npm ci --ignore-scripts
npm audit --audit-level=high
NODE_BIN=<exact-node-22.23.1> python3 tools/check_probity.py
cargo +1.97.1 fmt --all -- --check
cargo +1.97.1 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.97.1 test --workspace --all-features --locked
cargo +1.97.1 test --workspace --doc --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo +1.97.1 doc --workspace --all-features --locked --no-deps
cargo +1.97.1 deny check
cargo +1.97.1 audit --ignore RUSTSEC-2026-0173 --deny warnings
python3 tools/release_manifest.py --require-clean > SOURCE-MANIFEST.json
python3 tools/rc_package.py build --output /tmp/zeno-fcis-rc2
python3 tools/atdd.py run --all
```

The CI workflows add:

- `wasm32-unknown-unknown` checks for every `no_std + alloc` crate;
- independent SHA-256 provider and provider-parity checks;
- Miri interpretation of the semantic boundary tests;
- compilation of the codec and candidate-bundle fuzz targets;
- focused crash-atomic SQLite, authenticated-state, synthesis, mounted-adapter, and collection-backend tests;
- strict validated-decision, canonical-domain-manifest, and exhaustive-coverage promotion tests;
- strict sparse-proof and authenticated-plan decoding, projector qualification,
  candidate-bound projection-relation, and nominal publication tests;
- two independently generated source manifests compared byte-for-byte.
- exact BDD-to-ATDD registry equality, the complete adopter portfolio, and a
  hostile/permitted Probity command corpus under Node `22.23.1`.

## Static policy

`tools/check_assurance.py` fails when:

- a workspace library omits `#![forbid(unsafe_code)]`;
- a semantic crate directly accesses filesystems, networking, processes, environment variables, wall clocks, threads, async runtimes, or randomness;
- a semantic crate introduces unsafe or foreign-function code, mutable statics, or shared interior-mutability primitives;
- a semantic crate introduces floating-point types;
- a lower dependency ring imports a higher ring;
- an external Cargo dependency lacks an exact `=version` pin;
- a workflow requests write permissions;
- a third-party workflow action is not pinned to a full commit.

The check has hostile witnesses for every forbidden-pattern rule. `--self-test` proves those witnesses are rejected before repository scanning begins.

## Supply-chain policy

`Cargo.lock` is committed and every external direct dependency is exactly pinned. `deny.toml` makes unknown registries, unknown Git sources, wildcard dependencies, disallowed licenses, and RustSec findings release blockers. CI installs exact versions of `cargo-deny`, `cargo-audit`, and `cargo-fuzz`.

Optional developer guardrails use a private npm package with exact Probity
`1.10.0`, exact Node `22.23.1`, lockfile version 3, and a retained npm integrity
digest for the complete canonical lock graph. CI installs that graph with
lifecycle scripts disabled and runs `npm audit`. It is excluded from Rust
runtime and protocol authority.

The source manifest binds the full commit, clean-tree status, pinned Rust toolchain file, every tracked path, file kind, byte length, and SHA-256 digest. It is deterministic and contains no timestamps or host-specific paths.

Every advisory exception requires a repository-local disposition. See [supply-chain exceptions](SUPPLY_CHAIN_EXCEPTIONS.md). An exception suppresses only its named advisory; all other vulnerabilities, yanks, unsound notices, and unmaintained warnings remain blockers.

## Evidence retention

For a release candidate, retain:

1. the source commit and tag;
2. `SOURCE-MANIFEST.json` generated with `--require-clean`;
3. complete CI logs for stable, Miri, fuzz-build, feature-matrix, and supply-chain jobs;
4. generated schema/codegen manifests and cross-language replay output for every promoted profile;
5. mounted-runtime refinement fixtures for the exact external runtime build;
6. any proof/checker evidence referenced by a promotion certificate;
7. all public `.crate` files, rustdoc, source and diagnostic binary archives,
   CycloneDX SBOM, provenance inputs, RC manifest, and checksums;
8. a signed audit or review report when the deployment policy requires one.

The retained rustdoc archive includes every public API and source page but
excludes Rustdoc `1.97.1`'s nondeterministic merged cross-crate search index.
`OFFLINE_SEARCH_DISABLED.txt` inside the archive records this exact
reproducibility boundary.

Evidence is additive. A newer test run does not retroactively validate older source or external runtime artifacts.

## Failure and recovery

Any failed gate blocks release. Repair occurs in a new commit, followed by a complete rerun from a clean checkout. Do not reuse a source manifest, generated artifact, refinement fixture, or checker certificate across changed source unless its content address and all bound identifiers are unchanged and independently verified.

SQLite schema v5 creates a store only from nominal `CatalogAuthorizedGenesis`, persists the exact initial state/root/policy/law-evaluation/authorization identity, and revalidates that record on reopen without caller-supplied state. It consumes nominal `CatalogAuthorizedTransition` values and stores the exact policy, invocation, replay, authorization, candidate, bundle, receipt, and outbox identities in one transaction. Reopen strictly decodes and reauthorizes the gap-free transition sequence, reconstructs exact row-set equality, and requires the resulting state/root/version to equal the current semantic row. Replay and pending delivery repeat exact persisted-candidate validation. Delivery identities are derived from the implementation-neutral candidate and canonical outbox entry in both reference and SQLite shells. A crash before commit leaves no publication. A crash after commit is recovered by exact idempotent replay and delivery acknowledgement. Schema v4 and earlier or populated unversioned stores fail closed pending explicit migration. Operators must never edit genesis, policy, authorization, replay, receipt, or outbox rows to force progress.

## Explicit non-claims

The repository currently supplies reviewed reference implementations and strict mounting boundaries. It does not claim:

- that an external ZenoDEX executable or Python runtime is bundled or production-approved;
- that the reference sparse Merkle tree is a production database or a vetted Jellyfish Merkle Tree implementation;
- that projector evidence or a relation witness is sound without the
  deployment-selected independent verifier and reviewed relation engine;
- that a concrete ESSO, SMT solver, theorem prover, compiler, or LLM is bundled as a synthesis backend;
- that generated or imported evidence is true without its independent checker;
- that equality between untrusted `NormalizedDecision` values grants
  production promotion authority;
- that SQLite durability settings replace deployment-specific storage validation;
- audit completion, project-specific economic correctness, side-channel resistance, or production qualification of a value-moving profile.

Those claims require separately bound evidence and an explicit deployment decision.
