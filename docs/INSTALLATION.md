# Installation

This source packages ZenoFCIS `2.3.0-rc.1`, a release candidate. Rust `1.97.1`
is the minimum supported toolchain. The stable `1.1.0` release has the older API;
it does not provide the candidate's breaking V2 application path.

## Candidate binary and pinned source

Download the Linux x86_64 GNU CLI archive and checksum bundle from the matching
[GitHub prerelease](https://github.com/TheDarkLightX/ZenoFCIS/releases/tag/v2.3.0-rc.1).
Check `SHA256SUMS`, extract the binary archive, and run `zeno-fcis --version`;
it must print `zeno-fcis 2.3.0-rc.1`. The CLI has no daemon. The candidate
does not advertise macOS or Windows binaries.

For library use, clone the repository and select that exact tag:

```bash
git clone https://github.com/TheDarkLightX/ZenoFCIS.git
cd ZenoFCIS
git checkout v2.3.0-rc.1
git verify-tag v2.3.0-rc.1
```

Verify the tag against the signer's independently obtained public key. Use the
normal [V2 program API](V2_PROGRAM_API_MIGRATION.md), or scaffold an application
with `zeno-fcis new APP --contract CONTRACT --source /absolute/path/to/ZenoFCIS`.
The command's `--source` option binds generated dependencies to this checkout.
This candidate is not claimed to have been published to crates.io.

## Application dependency

Use the umbrella crate and select the smallest feature set needed by the
application:

```toml
[dependencies]
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", default-features = false }
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
zeno-fcis-core = { path = "../ZenoFCIS/crates/zeno-fcis-core", default-features = false }
zeno-fcis-codec = { path = "../ZenoFCIS/crates/zeno-fcis-codec", default-features = false }
```

All ZenoFCIS crates in one dependency graph should use the same exact release
version.

## Source checkout

```bash
git clone https://github.com/TheDarkLightX/ZenoFCIS.git
cd ZenoFCIS
git checkout v2.3.0-rc.1
cargo +1.97.1 test --workspace --all-features --locked
```

The signed release tag identifies the source used to build the published
packages. See the [release notes](RELEASE_NOTES.md) for compatibility and
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
cargo +1.97.1 install --path crates/zeno-fcis-cli --locked
zeno-fcis check project.zeno
```

V1's `mount-zenodex-zusd` diagnostic parity tool is not shipped in V2. Its
pinned assertions remain in the private kernel-law oracle, and a checked
full-width zUSD profile is still unfinished and is not advertised by this candidate.

## Offline verification

The release artifact set contains `SHA256SUMS`, `RC-MANIFEST.json`,
`SOURCE-MANIFEST.json`, `SBOM.cdx.json`, and `PROVENANCE-INPUTS.json`.

```bash
sha256sum --check SHA256SUMS
```

GitHub-hosted provenance or signatures must be verified separately against the
exact release tag. `PROVENANCE-INPUTS.json` records inputs for attestation; it
is not itself a signature or SLSA attestation.
