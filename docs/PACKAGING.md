# Release packaging

This document describes the release packaging tooling and the `2.4.0`
artifact set it prepares. Final release qualification and publication remain
pending.

## Package set

`release/package-set.toml` is the reviewed package authority. It contains:

- 39 public crates in dependency-first publication order;
- one private crate for compiled generated-code tests, for 40 workspace packages;
- the `zeno-fcis` specification CLI;
- the exact Cargo version and Rust toolchain.

`tools/rc_package.py check` compares that manifest with Cargo metadata. It
fails on missing packages, hidden public packages, duplicate entries, version
drift, non-exact internal dependency pins, missing package metadata, or an
invalid publication order. The publication order includes internal development
dependencies, because crates.io must resolve them while it checks a published
crate.

## Artifact set

`tools/rc_package.py build --output <directory>` creates:

```text
packages/*.crate
binaries/zeno-fcis-<version>-<target>.tar.gz
hosts/<target>/HOST-BINARY.json
hosts/<target>/COMPILED-INPUTS-<target>.json
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

The tooling builds the full list above locally. The current public candidate
download pages carry a smaller set: the Linux x86-64 GNU CLI archive, the
offline Rustdoc archive, `SBOM.cdx.json`, signed checksums, and a commentless
`RELEASE-SIGNING-KEY.pub`. A candidate's source is its matching public Git
tag. The source archive, full bundle, and `.crate` archives are local build
outputs, not published downloads.

`tools/rc_package.py downloads-list --release <directory>
[--signing-key RELEASE-SIGNING-KEY.pub] --out DOWNLOADS.sha256` writes the flat
list that users check, sorted by name. The list has one `<sha256>  <name>` line
for each host archive, the Rustdoc archive and `SBOM.cdx.json`, each with its
`SHA256SUMS` hash. With `--signing-key`, it also has one line for the key file,
with that file's own hash. Two assets may not share a name, and the key file
must be named `RELEASE-SIGNING-KEY.pub`.

The repository does not hold the key, so the release-set job writes the list
without it. The owner writes the list to be signed with `--signing-key`, and
the custody check, given the same key, checks that list. The installed-binary
journeys check archives against a list made by the same function.

The command requires a clean exact commit. Run it, `build-host-binary`,
`merge-hosts` and the custody check from a fresh clone of that commit. They
refuse untracked files and any ignored file outside `target/`, `node_modules/`
and `__pycache__/` folders, and a working checkout usually holds such files.
For example, a workspace test run writes
`crates/zeno-fcis-generated-code-tests/python/`.

The build uses pinned Rust `1.97.1`, fetches the locked external graph,
packages every public crate with `--locked`, then unpacks the complete crate
set into a temporary resolver-3 workspace. It reconciles a copy of the
reviewed lock against only the unpacked internal packages, and rejects
external identity or checksum drift and internal source fallback. It then runs
two compilations with all features and `--locked --offline`:

1. **Consumer build, archives only.** With each crate unpacked at
   `sources/<crate>-<version>/`, it builds every library, binary, example and
   build script, so published crates build from their archives alone. A
   non-test target that reads a file outside its own package fails here.
2. **Packaged tests, repository layout.** The same unpacked archives are
   placed at `crates/<crate>/`, and the repository files named in
   `release/packaged-test-inputs.json` are copied from the exact commit: nine
   from `verification/`, and the benchmark artifacts under `docs/benchmarks/`
   that the CLI's checker, optimizer and loop tests include. Every target,
   tests included, is compiled with `--no-run`. Rustc's dep-info then lists
   every file that each target read outside its own package. Those reads must
   equal the committed manifest exactly, by test target, path and SHA-256, for
   sibling-archive files and repository files alike. Any non-test target with
   such a read fails. Only `verification/` and `docs/benchmarks/` may hold a
   pinned repository file.

Packaged tests are not standalone. They compile only from the published
archives laid out as in the repository, plus those pinned repository files. A
crate's tests cannot be compiled from its own archive or from crates.io; the
whole-repository source archive is what runs every test. Two frozen, pinned
references force this allowance:

- the `#[cfg(test)]` `#[path]` include in
  `crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs`, which reads the
  CLI's order-fulfillment `synthesized/transition.rs`;
- `crates/zeno-fcis-synthesis/tests/v2_evaluator_identity.rs`, which includes
  two files from `verification/verus/`.

Open item: restore standalone packaged tests at the next identity
regeneration, when the verified sources next change.

The same run builds both declared binaries in release mode, generates
warning-denied Rustdoc, records the Cargo dependency graph as CycloneDX 1.6,
and content-addresses every retained artifact. The binaries are built with
`ZENO_FCIS_BUILD_TREE` empty, so `zeno-fcis` holds no build directory.
`zeno-fcis new` from an installed binary therefore needs `--source
<extracted source tree>`, for example the extracted
`source/zeno-fcis-<version>-source.tar.gz`.

Before it deletes the unpacked workspaces, the packager also builds its CLI in
the archives-only layout. It uses that executable to emit fresh
durable-counter, prepared-counter, account-lockout, order-fulfillment,
inventory-reservation, compliance-gateway, withdrawal-queue, and
agent-treasury-guard applications. Each separate consumer resolves internal
dependencies only from the extracted archives, and external dependencies from
the reviewed lock. Formatting, Clippy, tests, and the complete application
demonstration must all pass. The same check runs the unchanged V1.0 consumer
and verifies the retained foundational source baseline against the extracted
package files.

`PACKAGED-APPLICATION.json` retains the archive and generator hashes, the
original emitted-file hashes, the admitted dependency identities, the compiler
identity and argument vectors, the commands, and the outcomes. It is included
in the release manifest, checksums, and bundle. See
[packaged application qualification](PACKAGED_APPLICATION_QUALIFICATION.md).

To replay this check on the exact checkout that produced an existing crate set:

```bash
python3 tools/rc_package.py verify-packaged \
  --packages /tmp/zeno-fcis-rc/packages \
  --output /tmp/zeno-fcis-packaged-check
```

The current HEAD must equal the commit recorded in every archive. This replays
the same checker; it is not an independent review. The output directory must
be new. The command reuses downloaded dependencies, creates one temporary
compilation target, retains a JSON receipt on success, and removes its own
staging directory on success or failure. If verification or receipt writing
fails, it also removes the new output directory so that the same path can be
retried. It rejects and preserves an output directory that already exists.

This standalone check permits development archives and marks them explicitly
through their source status; the normal release build still requires a clean
exact commit. The resolver overlay points to archive contents. A registry-only
smoke test remains a separate post-publication check.

Compiler arguments use Cargo's encoded variables, so paths that contain spaces
stay intact. Assembly discards inherited compiler, wrapper, target, and flag
overrides and records the selected argument vectors. This does not isolate
Cargo configuration files or establish a hermetic build. Local receipt equality
has been checked on one host, with one toolchain installation and dependency
cache; Cargo home and rustup paths are not remapped. Fresh consumers also
resolve external dependencies under their own lockfiles.

Archive VCS fields describe the packaging source; they are not an attestation.
Match the archive hashes against the selected release manifest and its
independent source evidence. Receipts also hash the checker scripts, the
reviewed lockfile, and the package-set configuration, and they reject changes
to those inputs during a run.

The source archive also retains `package.json`, `package-lock.json`,
`.node-version`, and `probity.config.ts` for the optional development
guardrails. It retains the isolated Mini Determinator QEMU kernel source, its
locked nested workspace, the fixed-argument capture runner, and the validated
marketing capture, serial transcript, and metadata. These demo artifacts are
source and integration evidence. They are not additional publishable crates or
binaries in the 39-public-package release set (40 workspace packages including
the private test crate).

The release packager validates the exact Node and Probity identities and binds
the complete canonical npm lock graph, including every transitive package
entry. `SBOM.cdx.json` describes the shipped Rust crate graph. The
development-only npm graph is locked and audited separately, and it is not a
runtime dependency of any published crate.

The offline Rustdoc archive retains every public crate API and source page.
Pinned Rustdoc `1.97.1` does not produce a byte-identical merged cross-crate
search index across independent clean builds, so packaging removes
`search.index` and records that boundary in `OFFLINE_SEARCH_DISABLED.txt`.
The generated help and settings pages are normalized to the `zeno_fcis`
umbrella crate, because Rustdoc otherwise records the crate that finishes
last. Use docs.rs or locally generated documentation when you need global
search.

Each generated binary archive is specific to its host target. The archive
name, top-level directory, executable member, and provenance command are
derived from the host target that `rustc -vV` reports, so a machine builds
only its own executable. Beside the archive, the build records two files:

- `hosts/<target>/HOST-BINARY.json` holds the version, the source commit, the
  full `rustc -vV` text, the macOS deployment target, the archive name, hash,
  size and members, the executable hash, and the compiler flags and commands.
- `hosts/<target>/COMPILED-INPUTS-<target>.json` lists every file that the
  release build's rustc dep-info names, and the environment variables that the
  build's crates read. Each file is classified as a workspace source, a member
  of a locked registry package, or a build-script output. The target, source
  and home folders are shown as `<target>`, `<source>` and `<home>`. The record
  is written before the build directory is deleted.

A compiled input outside those three classes fails the build. So does a
variable that names the checkout, the target folder or the home folder,
except that Cargo's `CARGO_MANIFEST_DIR` and `OUT_DIR` may name a folder
inside the checkout or the target folder. rustc records such a variable even
when the code that reads it is not compiled in, so reading a path is not the
same as embedding it. The build refuses an executable that holds the
checkout's path, and `merge-hosts` and the custody check refuse one that holds
a build or home path.

`tools/rc_package.py build-host-binary --output <directory>` builds, archives
and records only this host's executable, with the same remapped compiler
environment and an empty `ZENO_FCIS_BUILD_TREE`, then runs the privacy scan.

### Two-host release set

The read-only release-candidate workflow builds two host executables:
`x86_64-unknown-linux-gnu`, in the complete Linux set on `ubuntu-24.04`, and
`aarch64-apple-darwin`, on GitHub's `macos-15` arm64 image with
`MACOSX_DEPLOYMENT_TARGET=11.0`. There is no Intel macOS executable and no
universal binary. The workflow has three jobs:

1. **`assemble`** builds the complete Linux set and reports the hashes of its
   `SHA256SUMS`, archive, host record and compiled-inputs record as job outputs.
2. **`macos-host-binary`** runs `build-host-binary`, then installs and uses the
   executable with `tools/check_installed_cli.py` (below), and reports the
   hashes of its archive, records and journey report.
3. **`release-set`** downloads both artifacts and checks each file against the
   hash its producing job reported. It runs the Linux installed-binary journey,
   merges the macOS host into the set, writes `DOWNLOADS.sha256` and runs the
   custody check.

`tools/rc_package.py merge-hosts --release <directory> --host <host folder>...
--journey <target>=<report>` first checks the set's index, and changes nothing
when the index is stale. It refuses:

- a host record from another commit, version or `rustc` commit-hash;
- a duplicate host, or a host the set already holds;
- an archive other than the recorded one;
- an extra or missing archive member;
- a document member that differs from the committed file;
- an executable other than the recorded one.

Every host needs an installed-binary journey report that passed on that host's
archive. The release privacy scan must find nothing in the journeys and
records that join the set. Only after every check passes does the command copy
the archive, the host's records and the journeys into the set, add the host to
`PROVENANCE-INPUTS.json`, and rewrite `RC-MANIFEST.json`, `SHA256SUMS` and the
bundle with the same code that `build` uses.

`tools/check_installed_cli.py --release <directory> [--source <tree>]
--work-dir <new directory> --report <file>` runs the marked install block of
the [installation guide](INSTALLATION.md) for this host as written; only the
release URL is replaced by a local folder. It then:

- checks the installed executable's platform facts: the glibc 2.39 floor on
  Linux, and arm64, macOS 11.0 and an ad-hoc signature on macOS;
- creates and builds generated applications with `--source`;
- runs a store journey and the serial checks;
- checks that the Linux-only routes fail closed on macOS.

Each step records its arguments, exit code and output hashes. The store
journey runs the SQLite shell with its shipped settings. `synchronous=EXTRA`
applies on both hosts, so each commit also syncs its rollback-journal
deletion. `fullfsync` applies on macOS, so SQLite's file syncs use
`F_FULLFSYNC`. The journey checks the store's results, not durability under
power loss.

`tools/check_release_custody.py --release <directory> --report <file>
[--bindings <job outputs>] [--downloads DOWNLOADS.sha256
[--signing-key RELEASE-SIGNING-KEY.pub]] [--tag v<version>]` binds the merged
set to its source commit by content. Its eleven checks cover:

1. the working tree, index and ignored files (without hashing `.git/index`);
2. the tag and versions;
3. the source manifest;
4. the source archive;
5. every `.crate` member;
6. every binary archive member, executable header and journey report;
7. each executable's compiled inputs;
8. the packaged-application record;
9. the release index;
10. the producing jobs' hashes;
11. the download list.

A missing input fails its check, and the bindings must name every host of the
set with its archive and all four hashes. The tag check confirms that the tag
names the commit. It does not authenticate the tag: on a tag run, the checkout
itself creates the tag at the run's commit.

The release-set artifact keeps the job outputs as `JOB-OUTPUTS.json`, beside
`DOWNLOADS.sha256` and `CUSTODY.json`. On a fresh clone at the commit, after
`cargo fetch --locked`, the same check can therefore be repeated offline
against the downloaded set. It rebuilds nothing, so it is not a
reproducible-build proof, and it does not authenticate the runner, compiler or
linker.

## Publication

Crates are published in the exact order in `release/package-set.toml`. The
generated-code test crate has `publish = false`. Publishing, signing, Git
tagging, and GitHub release creation happen only after review, and outside the
repository's permanent workflows, which stay read-only. This package adds no
write-enabled release workflow and does not weaken that policy.

The read-only release-candidate workflow also runs on release tags, so each
published tag passes through the same packaging gate used during review.
[Release candidates](RELEASE_CANDIDATES.md) lists the pinned public
candidates and the assets each download page carries.

## Nonclaims

Successful packaging records artifact identities, checks Cargo package
metadata, compiles the package contents, and runs the bounded generated
application. Reproducibility requires matching retained checksums from
independent clean builders. Packaging does not claim byte reproducibility for
Rustdoc's excluded global search index. It is not an independent audit, a
proof of downstream project laws, deployment qualification, a signature, or an
SLSA attestation. A stored certificate or key identity records which key
produced a signature; it is not proof of an artifact's origin. The signing key
published with a release's checksums shares that release's distribution
channel and does not establish an independent trust root.
