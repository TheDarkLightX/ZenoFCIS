# Installation

ZenoFCIS `1.0.0-rc.2` is a Rust library release candidate. Rust `1.97.1` is
the minimum supported toolchain for this candidate.

## Application dependency

Use the umbrella crate and select the smallest feature set needed by the
application:

```toml
[dependencies]
zeno-fcis = { version = "=1.0.0-rc.2", default-features = false, features = [
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
zeno-fcis-core = { version = "=1.0.0-rc.2", default-features = false }
zeno-fcis-codec = { version = "=1.0.0-rc.2", default-features = false }
```

All ZenoFCIS crates in one dependency graph should use the same exact release
candidate version.

## Source checkout

```bash
git clone https://github.com/TheDarkLightX/ZenoFCIS.git
cd ZenoFCIS
git checkout v1.0.0-rc.2
cargo +1.97.1 test --workspace --all-features --locked
```

The tag is created only after the exact release-candidate commit passes every
required gate. Until that tag exists, use the reviewed branch commit named in
the RC pull request rather than an unpinned branch dependency.

## Diagnostic binary

The core library does not require a daemon or executable. The RC includes one
host diagnostic tool:

```bash
cargo +1.97.1 install zeno-fcis-adapter-zenodex \
  --version 1.0.0-rc.2 --locked
```

`mount-zenodex-zusd` compares the pinned ZenoDEX Python and Rust transitions.
It requires a clean checkout of the exact pinned ZenoDEX revision and that
repository's Rust binary. It does not run a production shell or authorize
value movement.

## Offline verification

The RC artifact set contains `SHA256SUMS`, `RC-MANIFEST.json`,
`SOURCE-MANIFEST.json`, `SBOM.cdx.json`, and `PROVENANCE-INPUTS.json`.

```bash
sha256sum --check SHA256SUMS
```

GitHub-hosted provenance or signatures must be verified separately against the
exact release tag. `PROVENANCE-INPUTS.json` records inputs for attestation; it
is not itself a signature or SLSA attestation.
