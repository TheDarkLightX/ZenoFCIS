# Installation

The public `2.2.0-rc.1` and `2.3.0-rc.1` candidates supply one host binary, the
**Linux x86_64 GNU CLI**, with offline Rustdoc. The
[release candidates](RELEASE_CANDIDATES.md) page lists the exact source pins,
signed checksum assets and qualification limits.

The stable V2.2/V2.3 scope is unfinished. No macOS or Windows binaries and no
crates.io publication of the candidates are claimed. Rust `1.97.1` is the
supported toolchain for building the matching source and generated
applications.

## Install the V2.3 candidate CLI

Run these commands in the directory where you want to keep the archive and
the executable:

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

The executable sits in the archive's top-level directory, beside its README
and licenses. Keep that directory and add it to your shell's `PATH` to use the
CLI in later sessions.

`--ignore-missing` checks the downloaded assets listed in the flat
`DOWNLOADS.sha256`; it does not check assets you have not downloaded. Verify
the [detached checksum signature](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/DOWNLOADS.sha256.sig)
separately, under your trusted signing-key policy. The release also provides
the [signing key](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/RELEASE-SIGNING-KEY.pub).
Downloading it beside the artifacts does not independently establish trust, so
confirm its fingerprint through a trusted channel.

## Install the V2.2 candidate CLI

The older candidate uses its own archive and checksums:

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
and [signing key](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/RELEASE-SIGNING-KEY.pub)
need the same separate trust check.

## Verify downloads

`DOWNLOADS.sha256` lists the flat release assets; the install commands check
only the files you downloaded. The detached signature signs the entire
`DOWNLOADS.sha256` file. Verify that signature with `RELEASE-SIGNING-KEY.pub`,
then compare the binary's hash with its entry in the signed file. Confirm the
key's fingerprint through a trusted channel: downloading it from the same
release does not independently establish trust.

## Matching source and current library API

The current V2 API requires source or path dependencies; a dependency on the
historical `1.1.0` package does not supply it. Use the source tag that matches
your CLI. A plain `main` checkout still declares `1.1.0` and is a different
source.

For V2.3, clone into a fresh directory:

```bash
git clone --branch v2.3.0-rc.1 --depth 1 https://github.com/TheDarkLightX/ZenoFCIS.git ZenoFCIS
git -C ZenoFCIS rev-parse HEAD
# Expected: cb813cca3ddc4a4129c26f09f0d53e4052893c06
zeno-fcis new my-counter --template durable-counter --source "$PWD/ZenoFCIS"
```

The installed CLI records no build-directory location, so `new` needs an
explicit `--source`. The generated application binds its dependencies and
lockfile to that tree; keep the source and follow the generated README.

For V2.2, use tag `v2.2.0-rc.1` and expected source
`66e8e05180e0883e570e5459a5dc94f99d8e0d9b` instead. These source checkout
revisions differ from the original binary build revisions, with matching
runtime files.

An application beside that V2.3 source checkout can use:

```toml
[dependencies]
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", default-features = false }
```

Select optional features from the [feature matrix](FEATURE_MATRIX.md).
Individual current crates likewise require paths to the same pinned tree;
do not mix historical and candidate APIs in one graph.

## Historical 1.1.0 application dependency

The stable `1.1.0` instructions below keep their older API scope, documented
in the [V1.1 release notes](V1_1_RELEASE_NOTES.md).

Use the umbrella crate and select the smallest feature set the application
needs:

```toml
[dependencies]
zeno-fcis = { version = "=1.1.0", default-features = false, features = [
    "composed-program",
] }
```

The default feature supplies the foundational `std` surface. Semantic users
can disable default features for `no_std + alloc`. Read the
[feature matrix](FEATURE_MATRIX.md) before you enable shell or
project-specific features.

## Historical narrow crate dependency

A library that must keep a dependency ring explicit can depend on an
individual crate:

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

The original artifact manifests identify the source used to build the
published packages. The [release notes](V1_RELEASE_NOTES.md) describe the
compatibility and assurance boundaries.

## Historical host binary

The core library does not require a daemon. You can install the earlier
`1.1.0` authoring CLI for its historical workflows:

```bash
cargo +1.97.1 install zeno-fcis-cli --version 1.1.0 --locked
zeno-fcis check project.zeno
```

V2 does not ship V1's `mount-zenodex-zusd` diagnostic parity tool, and the
supported scalar route does not qualify the checked full-width zUSD profile.

## Optional coding-agent guardrails

Contributors who use coding agents can install the exact locked development
tooling under Node `22.23.1`:

```bash
npm ci --ignore-scripts
python3 tools/check_probity.py
```

Probity is not a Rust, runtime, protocol, or published-crate dependency. The
[deterministic developer guardrails](DEVELOPER_GUARDRAILS.md) guide covers the
opt-in Codex hook setup and its explicit nonclaims.
