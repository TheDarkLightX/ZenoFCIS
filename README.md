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

## Implemented workspace

The workspace now includes the complete package ladder:

- semantic values with default-bounded text and byte helper admission, immutable value and envelope witnesses for repeat canonical encoding, decisions, deterministic budgets, ZCVE/1 canonical encoding, exact commitments, preconditioned patches, closed plans, receipts, and complete candidate bundles;
- assume-guarantee composition, deterministic-parallel conflict checking, runtime refinement reports, promotion policy, canonical evidence envelopes, and the first ZenoDEX profile;
- vetted RustCrypto and libcrux SHA-256 providers with known-answer and cross-provider parity evidence;
- closed schema validation, root and selected-type schema-bound envelope admission, generated exact-schema and exact-catalog reconstruction, typed root/command/context smart constructors, derived command/context commitments, schema-typed direct root-field reads and updates, a raw-path-free generated mutation surface, disposition-typed reason application, catalog-typed effect/channel staging, private-inner generated transitions, deterministic Rust/Python adapters, negative codec vectors, cross-language replay, content-addressed generation manifests, and a bounded effect-free Solidity FCIS scaffold generator;
- project-neutral profiles with stable reason/effect/channel/capability/event registries, explicit evolution modes, and exact content-addressed migration evidence;
- a reusable callable/strict JSON-line mounted-runtime adapter for complete normalized decisions from any project profile;
- strict JSON-line mounted-runtime adapters that compare complete normalized decisions and retain mismatch fixtures;
- a permanent exact-revision mount of the real ZenoDEX Python/Rust single-vault zUSD transitions, with a retained 17-case full-decision parity report;
- an explicit dual-root sparse authenticated-state reference with membership/absence proofs, expected-version publication, and full-rebuild equality checks;
- verifier-gated bounded synthesis over canonical closed candidate domains with content-addressed certificates and honest incomplete-search results;
- crash-atomic SQLite publication, exact replay binding, a transactional outbox, idempotent delivery, and crash-point refinement tests;
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

The `zeno-fcis` umbrella keeps the semantic kernel small by default. Enable the complete integration surface explicitly:

```toml
[dependencies]
zeno-fcis = { version = "=0.1.0", features = ["full"] }
```

Important optional features include `codegen`, `evidence`, `mounted-runtime`, `mounted-zenodex`, `authenticated-state`, `synthesis`, `sqlite-shell`, `collections`, and `persistent-collections`.

## Solidity FCIS scaffold

The `codegen` feature now includes a deterministic Solidity v1 scaffold for closed local-state machines. It generates an abstract shell that owns initialization, private storage capture, expected-root checks, a recomputed-root corruption check, shell-captured execution context, decision consistency, invariant validation, a reentrancy gate, atomic commit, and transition receipts.

Derived contracts implement only `internal pure` command-admission, invariant, and decision hooks. V1 accepts fixed-size ABI scalars and generates no arbitrary external calls, token transfers, oracle adapters, delegate calls, upgrade hooks, or effect interpreter. This is a deliberately fail-closed foundation for agent-assisted contract development, not a claim that arbitrary “vibe-coded” Solidity is safe.

See [`docs/SOLIDITY_FCIS.md`](docs/SOLIDITY_FCIS.md) for the generated boundary, source-policy checker, example workflow, limitations, and production roadmap.

## Architecture

The repository keeps computation and coordination separate:

```text
pure transition
    -> immutable, content-addressed candidate
    -> independent validation
    -> atomic shell publication
    -> idempotent outbox delivery
```

Persistent backends are sealed behind a pure logical-map interface. Updates return new structurally shared versions; equality and canonical bytes depend on logical entries only. Map-entry ordering bytes are derived from the semantic key, the explicit persistent-entry boundary rejects mismatched key bytes, and materialization exposes only fallible APIs.

Concrete runtimes, databases, and synthesis engines remain outside the semantic authority boundary. Their adapters propose or store data; the pure validators decide whether that data is admissible.

## Verification

The main local gate is:

```bash
python3 tools/check_assurance.py --self-test
cargo +1.97.1 fmt --all -- --check
cargo +1.97.1 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.97.1 test --workspace --all-features --locked
```

See [release assurance](docs/RELEASE_ASSURANCE.md) for the full stable, `no_std`, Miri, fuzz, supply-chain, and source-manifest gates. Package-specific boundaries are documented in `docs/`.

## Assurance posture

This repository is pre-release high-assurance research software. It provides concrete reference implementations, runnable cross-boundary tests, and fail-closed promotion rules. The pinned ZenoDEX single-vault zUSD mount is bounded executable refinement evidence. It does not claim audit completion, economic correctness, side-channel resistance, production authorization, full ZenoDEX coverage, or that an external ZenoDEX, JMT, ESSO, solver, prover, compiler, or LLM runtime is bundled and approved.

## License

Dual-licensed under Apache-2.0 or MIT, at your option.
