# Installation

Public `2.2.0-rc.1` and `2.3.0-rc.1` candidates are available for the
**Linux x86_64 GNU CLI**, with offline Rustdoc. See
[release candidates](RELEASE_CANDIDATES.md) for the exact source pins, signed
checksum assets and qualification limits. Stable V2.2/V2.3 scope remains
unfinished; candidate crates.io publication is not claimed. Rust `1.97.1` is the
supported toolchain for building the matching source and generated applications.

## Supported hosts

- **Linux x86_64 GNU** needs glibc 2.39 or later. Both published candidates'
  executables require the `GLIBC_2.39` symbol version (a non-weak version
  need); Ubuntu 24.04, where they were built, ships glibc 2.39. On an older
  glibc, such as Ubuntu 22.04's 2.35, the dynamic loader refuses to start the
  executable and names the missing `GLIBC_2.39` version.
- **macOS on Apple silicon** (`aarch64-apple-darwin`): the 2.3.0 release
  workflow builds this required archive on GitHub's macOS 15 arm64 image
  (`macos-15`) with a minimum-version field of macOS 11.0. Inclusion in the
  release set requires a passing installed-binary journey on that image.
  Qualification is bound to the matching workflow run; other macOS versions
  are not qualified.
- Intel Macs, Windows and other hosts have no executable and are not qualified.
  Building from the matching source tag there may work but is not qualified.

## Install the 2.3.0 CLI

From a directory where you want to retain the archive and executable, set
`export VERSION=2.3.0` for a published 2.3.0 release, then run:

<!-- installed-journey: x86_64-unknown-linux-gnu -->
```bash
: "${VERSION:?set VERSION to the release to install, without the leading v}"
ARCHIVE="zeno-fcis-$VERSION-x86_64-unknown-linux-gnu"
curl --fail --location --output "$ARCHIVE.tar.gz" \
  "https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v$VERSION/$ARCHIVE.tar.gz"
curl --fail --location --output DOWNLOADS.sha256 \
  "https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v$VERSION/DOWNLOADS.sha256"
sha256sum --ignore-missing --check DOWNLOADS.sha256
tar -xzf "$ARCHIVE.tar.gz"
export PATH="$PWD/$ARCHIVE:$PATH"
zeno-fcis describe
```

The executable is at the archive's top-level prefix, beside its README and
licenses. Retain that directory and add it to your shell's PATH to use it in
later sessions. `--ignore-missing` checks the downloaded assets listed in the
flat `DOWNLOADS.sha256`; it does not check assets you have not downloaded.
Verify the [detached checksum signature](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0/DOWNLOADS.sha256.sig)
separately under your trusted signing-key policy. The release also provides the
[signing key](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0/RELEASE-SIGNING-KEY.pub).
Downloading it beside the artifacts does not independently establish trust;
confirm its fingerprint through a trusted channel.

The older prerelease downloads, source pins and signatures remain in the
[release-candidate record](RELEASE_CANDIDATES.md).

## Install on macOS (Apple silicon)

For a published 2.3.0 release whose signed `DOWNLOADS.sha256` lists
`zeno-fcis-2.3.0-aarch64-apple-darwin.tar.gz`, set `export VERSION=2.3.0`
in your shell, then run:

<!-- installed-journey: aarch64-apple-darwin -->
```bash
: "${VERSION:?set VERSION to a release that lists the macOS archive}"
ARCHIVE="zeno-fcis-$VERSION-aarch64-apple-darwin"
curl --fail --location --output "$ARCHIVE.tar.gz" \
  "https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v$VERSION/$ARCHIVE.tar.gz"
curl --fail --location --output DOWNLOADS.sha256 \
  "https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v$VERSION/DOWNLOADS.sha256"
grep "  $ARCHIVE.tar.gz\$" DOWNLOADS.sha256 | shasum -a 256 -c -
tar -xzf "$ARCHIVE.tar.gz"
export PATH="$PWD/$ARCHIVE:$PATH"
zeno-fcis describe
```

`shasum -a 256 -c -` checks only the line `grep` selects, so this does not
depend on `--ignore-missing`. The executable is arm64 only and its
minimum-version field is macOS 11.0; the release workflow runs it only on the
macOS 15 arm64 image.

The executable carries only the linker's ad-hoc code signature. It is not
signed with a Developer ID and not notarized. Files that `curl` downloads carry
no quarantine attribute, so the commands above run without a Gatekeeper
prompt. A browser marks its downloads as quarantined, and Gatekeeper then
refuses to run the executable. For a browser download, verify the checksum and
the signature first, then remove the attribute before extracting:

```bash
xattr -d com.apple.quarantine "$ARCHIVE.tar.gz"
```

If you already extracted it, use `xattr -dr com.apple.quarantine "$ARCHIVE"`
on the extracted directory instead.

One `DOWNLOADS.sha256`, signature and signing key cover both platforms;
`ssh-keygen -Y verify` is available on macOS and Linux.

Generated applications on macOS need Rust `1.97.1` and the Xcode Command Line
Tools (`xcode-select --install`), whose C compiler builds the bundled SQLite.
On every platform the SQLite shell sets `synchronous=EXTRA`, so each commit
also syncs the folder after deleting its rollback journal, the step that makes
the commit final. On Apple platforms it also turns on SQLite's `fullfsync`
setting, so every SQLite file sync, checkpoints included, uses `F_FULLFSYNC`
(SQLite's folder syncs, including that one, use a plain `fsync`); the generated
application's journal and directory syncs use Rust's `sync_all`, which also
uses `F_FULLFSYNC` there. `F_FULLFSYNC` asks the drive to flush its write
cache. On every platform, durability against power loss still depends on the
drive honouring the syncs it is asked for.

### Linux-only features on macOS

On macOS these features are unavailable or unqualified:

- **Contained checker runtime** (`loop` commands and
  `transform check --offline-parallel`): exits 2 with an inconclusive result
  and does nothing; see [contained checker runtime](#contained-checker-runtime).
- **`synth verify` execution of emitted code**: exits 2 with
  `conformance-unknown` and code `unsupported-runtime-platform`, and writes no
  receipt. `synth discover` and `synth run` work.
- **Lean evidence**: the qualified Lean distribution is the pinned Linux x86-64
  tree, so any other Lean installation's result stays blocked as unqualified
  evidence. The pinned CVC5 and Z3 binaries are Linux x86-64 too; on macOS,
  solver-backed commands run only binaries that an exact-hash tools manifest
  names, and that route is not qualified.

- **Tau import and export of an imported Tau policy**: the Python parser
  requires a 256 MiB address-space limit. The hosted macOS Python process
  cannot establish that limit, so these operations refuse without creating
  their output directory. The limit is not disabled on macOS.

The OCaml scalar consumer profile, the Tau kit runner and the repository's
acceptance, proof and packaged-workspace gates are checked only on Linux
x86-64; they are not part of the macOS evidence.

## Matching source and current library API

The current V2 API uses source/path dependencies on the source tag matching
your CLI. The historical `1.1.0` package retains its own V1 API. Follow the
release's source pin when building generated applications.

For V2.3, clone into a fresh directory:

```bash
git clone --branch v2.3.0 --depth 1 https://github.com/TheDarkLightX/ZenoFCIS.git ZenoFCIS
git -C ZenoFCIS rev-parse HEAD
# Expected: the source commit named on the v2.3.0 release page
zeno-fcis new my-counter --template durable-counter --source "$PWD/ZenoFCIS"
```

The installed CLI records no build-directory location, so `new` needs an
explicit `--source`. Its generated application binds dependencies and the
lockfile to that tree; retain the source and follow the generated README.
Historical prerelease source and binary-build revisions remain in the
[release-candidate record](RELEASE_CANDIDATES.md).

An application beside that V2.3 source checkout can use:

```toml
[dependencies]
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", default-features = false }
```

Select optional features from the [feature matrix](FEATURE_MATRIX.md).
Individual current crates likewise require paths to the same pinned tree;
keep every dependency in the graph bound to the same release source.

## Contained checker runtime

In the 2.3.0 source, `loop` commands and `transform check --offline-parallel` require
Linux, cgroup v2 with memory and pids controllers, and a running systemd user
manager. `systemd-run --user` and `systemctl --user` must work for the invoking
user. A headless host can enable that manager with `sudo loginctl enable-linger
USERNAME`; its session needs `XDG_RUNTIME_DIR=/run/user/UID` and the user bus at
`unix:path=/run/user/UID/bus`. Hosted checks establish and probe these limits
before running the acceptance suite.

The child has a 64 MiB memory cap, no swap, 64 tasks, bounded CPU quota, the
caller's CPU affinity and no higher scheduling priority. The inherited control
socket has no pathname, so a long `TMPDIR` is supported. Unavailable containment
returns an explicit `containment-unavailable` inconclusive result (exit 2).
Serial transform checks and ordinary library use do not need a user manager.
Receipt output requires a filesystem that supports exclusive hard links and
directory synchronization; an existing destination is never replaced.

The contained runtime is Linux-only. On macOS each of these commands prints one
JSON result with `"status":"inconclusive"`, `"authority":"none"`,
`"reason":"containment-unavailable:platform"` and
`"accounting_complete":false`, exits 2, and runs and writes nothing. Loop
commands add an `acquisition` record whose `observed_intake` is `null`. Serial
`transform check`, its receipts, `transform replay` and `optimize` have a
macOS route; the release's installed-binary journey checks receipt publication
on APFS. Qualification requires the matching completed host run.

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

Original artifact manifests identify the source used to build the published
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

V1's `mount-zenodex-zusd` diagnostic parity tool is not shipped in V2. The
checked full-width zUSD profile is not qualified by the supported scalar route.

## Verifying downloads

`DOWNLOADS.sha256` lists the flat release assets; the install commands check
only the files you downloaded (`sha256sum` on Linux, `shasum -a 256` on
macOS). The detached signature signs the entire
`DOWNLOADS.sha256` file: verify that signature with `RELEASE-SIGNING-KEY.pub`,
then compare the binary's hash against its entry in that signed file. Confirm
the key's fingerprint through a trusted channel; downloading it from the same
release does not independently establish trust.
