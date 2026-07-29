# Contributing to ZenoFCIS

ZenoFCIS accepts small, bounded changes that preserve its functional-core /
imperative-shell architecture and explicit protocol identities.

## Architecture rule

```text
bytes -> bounded canonical values -> pure transition -> immutable decision
      -> nominal authorization -> atomic imperative interpreter
```

Semantic crates use `#![forbid(unsafe_code)]` and contain no ambient clock,
randomness, filesystem, network, database, process, thread, async runtime,
global mutable state, or executable effect closure. Operational mechanisms
belong in an explicit outer adapter or shell.

## Development setup

Use pinned Rust `1.97.1` and the committed `Cargo.lock`. Direct external
dependencies require exact `=version` pins. Before submitting a change, run:

```bash
python3 tools/check_assurance.py --self-test
python3 tools/check_assurance.py
python3 tools/check_library_docs.py
cargo +1.97.1 fmt --all -- --check
cargo +1.97.1 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.97.1 test --workspace --all-features --locked
cargo +1.97.1 test --workspace --doc --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo +1.97.1 doc --workspace --all-features --locked --no-deps
```

Changes to packaging or a public crate must also run:

```bash
python3 tools/rc1_package.py self-test
python3 tools/rc1_package.py check
```

Permanent CI adds `no_std`, Miri, fuzz-build, supply-chain, source-manifest,
mounted-runtime, persistence, and assurance-specific gates.

## Protocol-visible changes

Protocol-visible behavior requires:

- a stable schema and version;
- canonical ordering and encoding;
- typed rejection or committed-failure semantics;
- explicit deterministic resource bounds;
- complete candidate, invocation, profile, law, and deployment binding where
  the value can reach authority;
- positive, exact-boundary, negative, and metamorphic or differential tests;
- an honest list of assumptions and nonclaims.

Do not renumber or reinterpret a stable field, variant, reason, effect,
channel, domain, codec, schema, precedence, profile, or algorithm identifier.
A semantic change under an existing Rust API still requires a new protocol
identity and reviewed migration evidence.

## Pull requests

Keep one conceptual change per PR. Describe:

- exact base and head commits;
- public API and dependency changes;
- authority boundary and resource bounds;
- laws, negative cases, and validation performed;
- assumptions, nonclaims, and unresolved blockers.

Do not commit generated caches, target directories, diagnostic archives,
temporary assembly scripts, write-enabled workflows, credentials, or retained
CI payload fragments. Generated source that is part of the public contract must
have a deterministic generator, retained manifest, and compile/replay gate.

CODEOWNERS declares repository review ownership. Repository settings must
separately enforce protected branches and required approvals. A passing CI run
does not replace independent review for authority-bearing or protocol changes.

## Security reports

Do not open a public PR for a suspected exploitable vulnerability. Follow the
private process in [SECURITY.md](SECURITY.md).
