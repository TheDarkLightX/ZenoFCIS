# Installation

ZenoFCIS `1.1.0` is the stable Rust library release. Rust `1.97.1` is
the minimum supported toolchain.

## Application dependency

Use the umbrella crate and select the smallest feature set needed by the
application:

```toml
[dependencies]
zeno-fcis = { version = "=1.1.0", default-features = false, features = [
    "composed-program",
] }
```

The default feature supplies the foundational `std` surface. Semantic users
can disable default features for `no_std + alloc`. See the
[feature matrix](FEATURE_MATRIX.md) before enabling shell or project-specific
features.

## Narrow crate dependency

Libraries that must preserve a dependency ring can depend on an individual
crate:

```toml
[dependencies]
zeno-fcis-core = { version = "=1.1.0", default-features = false }
zeno-fcis-codec = { version = "=1.1.0", default-features = false }
```

All ZenoFCIS crates in one dependency graph should use the same exact release
version.

## Source checkout

```bash
git clone https://github.com/TheDarkLightX/ZenoFCIS.git
cd ZenoFCIS
git checkout v1.1.0
cargo +1.97.1 test --workspace --all-features --locked
```

The signed release tag identifies the source used to build the published
packages. See the [release notes](V1_RELEASE_NOTES.md) for compatibility and
assurance boundaries.

## Optional coding-agent guardrails

Contributors using coding agents can install the exact locked development
tooling under Node `22.23.1`:

```bash
npm ci --ignore-scripts
python3 tools/check_probity.py
```

Probity is not a Rust, runtime, protocol, or published-crate dependency. See
[deterministic developer guardrails](DEVELOPER_GUARDRAILS.md) for opt-in Codex
hook setup and explicit nonclaims.

## Host binaries

The core library does not require a daemon. V2 ships one host binary, the
authoring CLI:

```bash
cargo +1.97.1 install zeno-fcis-cli --version 1.1.0 --locked
zeno-fcis check project.zeno
```

V1's `mount-zenodex-zusd` diagnostic parity tool is not shipped in V2. Its
pinned assertions remain in the private kernel-law oracle, and a checked
full-width zUSD profile is planned for V2.1.

## Offline verification

The release artifact set contains `SHA256SUMS`, `RC-MANIFEST.json`,
`SOURCE-MANIFEST.json`, `SBOM.cdx.json`, and `PROVENANCE-INPUTS.json`.

```bash
sha256sum --check SHA256SUMS
```

GitHub-hosted provenance or signatures must be verified separately against the
exact release tag. `PROVENANCE-INPUTS.json` records inputs for attestation; it
is not itself a signature or SLSA attestation.
