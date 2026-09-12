# Release packaging

This document describes the ZenoFCIS `1.0.0` artifact set.

## Package set

`release/package-set.toml` is the reviewed package authority. It contains:

- 36 public crates in dependency-first publication order;
- one private crate for compiled generated-code tests;
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
PACKAGED-APPLICATION.json
RC-MANIFEST.json
SHA256SUMS
zeno-fcis-<version>-rc-bundle.zip
```

The command requires a clean exact commit. It uses pinned Rust `1.97.1`, fetches
the locked external graph, packages every public crate with `--locked`, then
unpacks the complete crate set into a temporary resolver-3 workspace. It
reconciles a copy of the reviewed lock against only the unpacked internal
packages, rejects external identity/checksum drift and internal source fallback, then
compiles all features across every public library, test, example, benchmark,
and binary target with `--locked --offline`. This catches source, build, binary,
or test files that exist in the repository and are absent from a packaged
archive. The same run builds both declared binaries in release mode,
generates warning-denied rustdoc, records the Cargo dependency graph as
CycloneDX 1.6, and content-addresses every retained artifact.

Before deleting the unpacked workspace, the packager also builds its CLI and
uses that executable to emit a fresh durable-counter application. The separate
consumer resolves internal dependencies only from the extracted archives and
external dependencies from the reviewed lock. Formatting, Clippy, tests, and
the durable demonstration must all pass. `PACKAGED-APPLICATION.json` retains
archive and generator hashes, the original emitted-file hashes, admitted
dependency identities, compiler identity and argument vectors, commands, and
outcomes. It is included in the release manifest, checksums, and bundle. See
[packaged application qualification](PACKAGED_APPLICATION_QUALIFICATION.md).

To replay this check on the exact checkout that produced an existing crate set:

```bash
python3 tools/rc_package.py verify-packaged \
  --packages /tmp/zeno-fcis-rc/packages \
  --output /tmp/zeno-fcis-packaged-check
```

The current HEAD must equal the commit recorded in every archive. This replays
the same checker; it is not an independent review. The output directory must be
new. This command reuses downloaded dependencies,
creates one temporary compilation target, retains a JSON receipt on success, and
removes its owned staging directory on success or failure. Development archives
are permitted by this standalone check and are explicitly marked through their source status; the
normal release build still requires a clean exact commit. The resolver overlay
points to archive contents. A registry-only smoke test remains a separate
post-publication check.

Compiler arguments use Cargo's encoded variables so paths containing spaces
remain intact. Assembly discards inherited compiler, wrapper, target, and flag
overrides and records the selected argument vectors. This does not isolate
Cargo configuration files or establish a hermetic build. Local receipt equality
has been checked on one host with one toolchain installation and dependency
cache; Cargo home and rustup paths are not remapped. Fresh consumers also
resolve external dependencies under their own lockfiles.

Archive VCS fields describe the packaging source; they are not an attestation.
Match archive hashes against the selected release manifest and its independent
source evidence. Receipts also hash the checker scripts, reviewed lockfile, and
package-set configuration, and reject changes to those inputs during a run.

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
generated-code test crate has `publish = false`. Publishing, signing, creating a Git tag,
or creating a GitHub release remains an owner action after review.

The repository's permanent workflows are read-only. This package does not add
a write-enabled release workflow or weaken that policy.

The complete owner procedure for exact-head review, signed tagging,
dependency-ordered crates.io publication, release evidence, and failure
recovery is the [V1 release checklist](V1_RELEASE_CHECKLIST.md). The permanent
release-candidate workflow also runs on `v1.0.0-rc.*` and `v1.0.0` tags so the immutable tag
is packaged through the same read-only gate used during review.

## Nonclaims

Successful packaging records artifact identities, checks Cargo package metadata,
compiles the package contents, and executes the bounded generated application.
Reproducibility requires matching retained checksums from independent clean
builders. It does not claim byte reproducibility for Rustdoc's excluded global
search index, and it does not constitute an independent audit, proof of
downstream project laws, deployment qualification, signature, or SLSA
attestation.
