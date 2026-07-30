# Release-candidate packaging

This document describes the ZenoFCIS `1.0.0-rc.3` artifact set.

## Package set

`release/package-set.toml` is the reviewed package authority. It contains:

- 36 public crates in dependency-first publication order;
- one private compiled code-generation fixture;
- the `zeno-fcis` authoring CLI and `mount-zenodex-zusd` diagnostic target;
- the exact Cargo version and Rust toolchain.

`tools/rc_package.py check` compares that manifest with Cargo metadata and
fails on missing packages, hidden public packages, duplicate entries, version
drift, non-exact internal dependency pins, missing package metadata, or an
invalid publication order. Publication order includes internal development
dependencies because crates.io must resolve them while checking a published
crate.

## Artifact set

`tools/rc_package.py build --output <directory>` creates:

```text
packages/*.crate
binaries/zeno-fcis-<version>-<target>.tar.gz
binaries/mount-zenodex-zusd-<version>-<target>.tar.gz
docs/zeno-fcis-rustdoc-<version>.tar.gz
source/zeno-fcis-<version>-source.tar.gz
SOURCE-MANIFEST.json
SBOM.cdx.json
PROVENANCE-INPUTS.json
RC-MANIFEST.json
SHA256SUMS
zeno-fcis-<version>-rc-bundle.zip
```

The command requires a clean exact commit. It uses pinned Rust `1.97.1`, fetches
the locked external graph, packages every public crate with `--locked`, then
unpacks the complete crate set into a temporary resolver-3 workspace. It
generates an offline lock against only the unpacked internal packages, then
compiles all features across every public library, test, example, benchmark,
and binary target with `--locked --offline`. This catches source, build, binary,
or test files that exist in the repository and are absent from a published
archive. The same run builds both declared binaries in release mode,
generates warning-denied rustdoc, records the Cargo dependency graph as
CycloneDX 1.6, and content-addresses every retained artifact.

The source archive also retains `package.json`, `package-lock.json`,
`.node-version`, and `probity.config.ts` for optional development guardrails.
It retains the isolated Mini Determinator QEMU kernel source, its locked nested
workspace, the fixed-argument capture runner, and the validated marketing
capture, serial transcript, and metadata. These demo artifacts are source and
integration evidence; they are not additional publishable crates or binaries
in the 36-package release set.
The release packager validates their exact Node/Probity identities and binds
the complete canonical npm lock graph, including every transitive package
entry. `SBOM.cdx.json` describes the shipped Rust crate graph; the
development-only npm graph is separately locked and audited and is not a
runtime dependency of any published crate.

The offline rustdoc archive retains every public crate API and source page.
Pinned Rustdoc `1.97.1` does not produce a byte-identical merged cross-crate
search index across independent clean builds, so packaging removes
`search.index` and records that boundary in `OFFLINE_SEARCH_DISABLED.txt`.
The generated help and settings pages are normalized to the `zeno_fcis`
umbrella crate because Rustdoc otherwise records the crate that finishes last.
Use docs.rs or locally generated documentation when global search is required.

Each generated binary archive is host-target-specific. The archive name, top-level directory, executable member, and provenance command are derived from the declared target. The read-only release candidate workflow currently qualifies the Linux x86-64 archive. Additional
targets require their own exact-head workflow evidence before attachment to a
release.

## Publication

Crates are published in the exact order in `release/package-set.toml`. The
fixture crate has `publish = false`. Publishing, signing, creating a Git tag,
or creating a GitHub release remains an owner action after review.

The repository's permanent workflows are read-only. This package does not add
a write-enabled release workflow or weaken that policy.

The complete owner procedure for exact-head review, signed tagging,
dependency-ordered crates.io publication, release evidence, and failure
recovery is the [V1 release checklist](V1_RELEASE_CHECKLIST.md). The permanent
release-candidate workflow also runs on `v1.0.0-rc.*` tags so the immutable tag
is packaged through the same read-only gate used during review.

## Nonclaims

Successful packaging proves that the declared artifacts are reproducibly
assembled from an exact source revision and that Cargo package metadata is
coherent. It does not claim byte reproducibility for Rustdoc's excluded global
search index, and it does not constitute an independent audit, proof of
downstream project laws, deployment qualification, signature, or SLSA
attestation.
