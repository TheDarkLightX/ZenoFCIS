# Installation

Public signed `2.2.0-rc.1` and `2.3.0-rc.1` candidates are available for the
**Linux x86_64 GNU CLI**. They are prereleases with matching source, crate
archives, Rustdoc and manifests. See [release candidates](RELEASE_CANDIDATES.md)
for the exact source pins, signed checksum assets and qualification limits.
Stable V2.2/V2.3 scope remains unfinished; macOS/Windows binaries and candidate
crates.io publication are not claimed. Rust `1.97.1` is the supported toolchain
for building the matching source and generated applications.

## Install the V2.3 candidate CLI

From a directory where you want to retain the archive and executable:

```bash
curl --fail --location --output zeno-fcis-2.3.0-rc.1-x86_64-unknown-linux-gnu.tar.gz \
  https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/zeno-fcis-2.3.0-rc.1-x86_64-unknown-linux-gnu.tar.gz
curl --fail --location --output DOWNLOADS.sha256 \
  https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/DOWNLOADS.sha256
sha256sum --ignore-missing --check DOWNLOADS.sha256
tar -xzf zeno-fcis-2.3.0-rc.1-x86_64-unknown-linux-gnu.tar.gz
export PATH="$PWD/zeno-fcis-2.3.0-rc.1-x86_64-unknown-linux-gnu:$PATH"
zeno-fcis describe
```

The executable is at the archive's top-level prefix, beside its README and
licenses. Retain that directory and add it to your shell's PATH to use it in
later sessions. `--ignore-missing` checks the downloaded assets listed in the
flat `DOWNLOADS.sha256`; it does not check assets you have not downloaded.
Verify the [detached checksum signature](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/DOWNLOADS.sha256.sig)
separately using your trusted signing-key policy. The release also provides the
[owner public key](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/OWNER-SIGNING-KEY.pub);
downloading it beside the artifacts does not independently establish trust.

## Install the V2.2 candidate CLI

Use the corresponding archive and checksums for the older candidate:

```bash
curl --fail --location --output zeno-fcis-2.2.0-rc.1-x86_64-unknown-linux-gnu.tar.gz \
  https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/zeno-fcis-2.2.0-rc.1-x86_64-unknown-linux-gnu.tar.gz
curl --fail --location --output DOWNLOADS.sha256 \
  https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/DOWNLOADS.sha256
sha256sum --ignore-missing --check DOWNLOADS.sha256
tar -xzf zeno-fcis-2.2.0-rc.1-x86_64-unknown-linux-gnu.tar.gz
export PATH="$PWD/zeno-fcis-2.2.0-rc.1-x86_64-unknown-linux-gnu:$PATH"
zeno-fcis describe
```

Its [signature](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/DOWNLOADS.sha256.sig)
and [public key](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/OWNER-SIGNING-KEY.pub)
have the same separate trust requirement.

## Matching source and current library API

The current V2 API requires source/path dependencies. A dependency on the
historical `1.1.0` package does not supply it. Use the source tag matching your
CLI; a plain `main` checkout still declares `1.1.0` and is a different source.

For V2.3, clone into a fresh directory:

```bash
git clone --branch v2.3.0-rc.1 --depth 1 https://github.com/TheDarkLightX/ZenoFCIS.git ZenoFCIS
git -C ZenoFCIS rev-parse HEAD
# Expected: dd04cbcb3f7d8694de5f79bc1280b9252fdd147b
zeno-fcis new my-counter --template durable-counter --source "$PWD/ZenoFCIS"
```

The installed CLI records no build-directory location, so `new` needs an
explicit `--source`. Its generated application binds dependencies and the
lockfile to that tree; retain the source and follow the generated README.
For V2.2, use tag `v2.2.0-rc.1` and expected source
`c94c962f4d223f76dbc9ba9cb332cffd4b25436b` instead. Matching source tarballs are
also linked from the candidate record.

An application beside that V2.3 source checkout can use:

```toml
[dependencies]
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", default-features = false }
```

Select optional features from the [feature matrix](FEATURE_MATRIX.md).
Individual current crates likewise require paths to the same pinned tree;
do not mix historical and candidate APIs in one graph.

## Historical 1.1.0 application dependency

The stable `1.1.0` instructions below retain their older API scope, documented
in the [V1.1 release notes](V1_1_RELEASE_NOTES.md).

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

## Historical narrow crate dependency

Libraries that must preserve a dependency ring can depend on an individual
crate:

```toml
[dependencies]
zeno-fcis-core = { version = "=1.1.0", default-features = false }
zeno-fcis-codec = { version = "=1.1.0", default-features = false }
```

All ZenoFCIS crates in one dependency graph should use the same exact release
version.

## Historical source checkout

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

## Historical host binary

The core library does not require a daemon. The earlier `1.1.0` authoring CLI
can be installed for its historical workflows:

```bash
cargo +1.97.1 install zeno-fcis-cli --version 1.1.0 --locked
zeno-fcis check project.zeno
```

V1's `mount-zenodex-zusd` diagnostic parity tool is not shipped in V2. Its
pinned assertions remain in the private kernel-law oracle. The checked
full-width zUSD profile retains its qualification obligations in the
[factory feature plan](V2_1_V2_2_PLAN.md).

## Bundle verification

The retained bundle contains `SHA256SUMS`, `RC-MANIFEST.json`,
`SOURCE-MANIFEST.json`, `SBOM.cdx.json`, and `PROVENANCE-INPUTS.json`. Its
`SHA256SUMS` names paths inside the extracted bundle; run this there:

```bash
sha256sum --check SHA256SUMS
```

GitHub-hosted provenance or signatures must be verified separately against the
exact release tag. `PROVENANCE-INPUTS.json` records inputs for attestation; it
is not itself a signature or SLSA attestation.
