# Installation

ZenoFCIS `2.5.0` is the current release source. Final qualification and
publication remain pending. ZenoFCIS 2.3.0 introduced the first stable V2 line.
The initial 2.5.0 release set includes the **Linux x86_64 GNU** CLI archive
and offline Rustdoc. The **macOS on Apple silicon** archive is deferred until
native platform validation is complete. After qualification and
publication, the signed `DOWNLOADS.sha256` on the release page will list the
assets available to install.
[Supported hosts](#supported-hosts) gives each host's requirements and
qualification boundary. The [2.5.0 release notes](RELEASE_NOTES.md) describe
the release's scope.

Applications use the V2 API from the matching source tag, as
[Matching source and current library API](#matching-source-and-current-library-api)
describes. Rust `1.97.1` is the supported toolchain for building the matching
source and generated applications.

The earlier public `2.2.0-rc.1` and `2.3.0-rc.1` prereleases each advertised
only the Linux x86_64 GNU CLI, and crates.io publication of the candidates is
not claimed. The [release candidates](RELEASE_CANDIDATES.md) page keeps their
exact source pins, signed checksum assets and qualification limits.

## Supported hosts

- **Linux x86_64 GNU** needs glibc 2.39 or later. Both published candidates'
  executables require the `GLIBC_2.39` symbol version (a non-weak version
  need). Ubuntu 24.04, where they were built, ships glibc 2.39. On an older
  glibc, such as Ubuntu 22.04's 2.35, the dynamic loader refuses to start the
  executable and names the missing `GLIBC_2.39` version.
- **macOS on Apple silicon** (`aarch64-apple-darwin`): deferred from the initial
  publication. The optional release workflow builds this archive on GitHub's macOS 15 arm64 image
  (`macos-15`), with a minimum-version field of macOS 11.0. The archive joins
  the release set only if an installed-binary journey passes on that image.
  Qualification is bound to the matching workflow run, and other macOS
  versions are not qualified.
- Intel Macs, Windows and other hosts have no executable and are not qualified.
  Building from the matching source tag on those hosts may work, but it is not
  qualified.

## Install the 2.5.0 CLI

Once the qualified 2.5.0 release is published and its signed `DOWNLOADS.sha256`
lists the Linux archive, use the commands below. In the directory where you
want to keep the archive and the executable, set `export VERSION=2.5.0`, then
run:

<!-- installed-journey: x86_64-unknown-linux-gnu -->
```bash
: "${VERSION:?set VERSION to the release to install, without the leading v}"
ARCHIVE="zeno-fcis-$VERSION-x86_64-unknown-linux-gnu"
curl --fail --location --output "$ARCHIVE.tar.gz" \
  "https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v$VERSION/$ARCHIVE.tar.gz"
curl --fail --location --output DOWNLOADS.sha256 \
  "https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v$VERSION/DOWNLOADS.sha256"
awk -v archive="$ARCHIVE.tar.gz" '$2 == archive { print; found=1 } END { if (!found) exit 1 }' \
  DOWNLOADS.sha256 | sha256sum --check -
tar -xzf "$ARCHIVE.tar.gz"
export PATH="$PWD/$ARCHIVE:$PATH"
zeno-fcis describe
```

The executable sits in the archive's top-level directory, beside its README
and licenses. Keep that directory and add it to your shell's `PATH` to use the
CLI in later sessions.

The exact archive filename must appear in `DOWNLOADS.sha256`; a missing entry
or checksum mismatch stops installation. Other assets need not be downloaded. Verify
the published release's [detached checksum signature](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.5.0/DOWNLOADS.sha256.sig)
separately, under your trusted signing-key policy. That published release must
also provide the [signing key](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.5.0/RELEASE-SIGNING-KEY.pub).
Downloading it beside the artifacts does not independently establish trust, so
confirm its fingerprint through a trusted channel. See
[Verify downloads](#verify-downloads).

The older prerelease downloads, source pins and signatures stay in the
[release-candidate record](RELEASE_CANDIDATES.md).

## Install on macOS (Apple silicon)

After the qualified 2.5.0 release is published, check that its signed
`DOWNLOADS.sha256` lists `zeno-fcis-2.5.0-aarch64-apple-darwin.tar.gz`. Then set
`export VERSION=2.5.0` in your shell and run:

<!-- installed-journey: aarch64-apple-darwin -->
```bash
: "${VERSION:?set VERSION to a release that lists the macOS archive}"
ARCHIVE="zeno-fcis-$VERSION-aarch64-apple-darwin"
curl --fail --location --output "$ARCHIVE.tar.gz" \
  "https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v$VERSION/$ARCHIVE.tar.gz"
curl --fail --location --output DOWNLOADS.sha256 \
  "https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v$VERSION/DOWNLOADS.sha256"
awk -v archive="$ARCHIVE.tar.gz" '$2 == archive { print; found=1 } END { if (!found) exit 1 }' \
  DOWNLOADS.sha256 | shasum -a 256 -c -
tar -xzf "$ARCHIVE.tar.gz"
export PATH="$PWD/$ARCHIVE:$PATH"
zeno-fcis describe
```

`shasum -a 256 -c -` checks the exact archive filename selected by `awk`;
a missing entry or checksum mismatch stops installation. The executable is arm64 only, and its
minimum-version field is macOS 11.0. The release workflow runs it only on the
macOS 15 arm64 image.

The executable carries only the linker's ad-hoc code signature. It is not
signed with a Developer ID and is not notarized. Files that `curl` downloads
carry no quarantine attribute, so the commands above run without a Gatekeeper
prompt. A browser marks its downloads as quarantined, and Gatekeeper then
refuses to run the executable. For a browser download, first verify the
checksum and the signature, then remove the attribute before you extract the
archive:

```bash
xattr -d com.apple.quarantine "$ARCHIVE.tar.gz"
```

If you already extracted it, use `xattr -dr com.apple.quarantine "$ARCHIVE"`
on the extracted directory instead.

One `DOWNLOADS.sha256`, signature and signing key cover both platforms;
`ssh-keygen -Y verify` is available on macOS and Linux.

Generated applications on macOS need Rust `1.97.1` and the Xcode Command Line
Tools (`xcode-select --install`), whose C compiler builds the bundled SQLite.

On every platform, the SQLite shell sets `synchronous=EXTRA`. Each commit
therefore also syncs the folder after it deletes its rollback journal, the
step that makes the commit final. On Apple platforms, the shell also turns on
SQLite's `fullfsync` setting, so every SQLite file sync, checkpoints included,
uses `F_FULLFSYNC`. SQLite's folder syncs, including the one after the journal
is deleted, use a plain `fsync`. The generated application's journal and
directory syncs use Rust's `sync_all`, which also uses `F_FULLFSYNC` on Apple
platforms. `F_FULLFSYNC` asks the drive to flush its write cache. On every
platform, durability against power loss still depends on the drive honouring
the syncs it is asked for.

### Runtime conformance and Linux-only features on macOS

Source builds support `synth verify` on Linux and macOS for Rust 1.97.1,
Python 3 and Node.js 22. The runner checks the regenerated artifact, executes
the exact emitted function over every admitted input, and records a separate
finite-conformance receipt with no application authority. Each subprocess has
a 30-second startup-inclusive deadline and a 32-MiB bound per output stream.
On macOS, an owned live process-group member retains the group identity until
cleanup, including when the target exits first. Failed setup, execution,
capture or cleanup cannot produce a passing receipt.

This is bounded process supervision, not a security sandbox for arbitrary
executables. Use trusted compilers/interpreters and the registered emitters;
the command executes regenerated code, not a supplied replacement source.
Dynamic dependencies and runtime correctness remain external premises.
Processes that deliberately leave the owned group and abrupt termination of
the supervisor are outside its cleanup guarantee. A finite-conformance pass
does not qualify the separate contained checker runtime or production trust.

On macOS these features are unavailable or unqualified:

- **Contained checker runtime** (`loop` commands and
  `transform check --offline-parallel`): exits 2 with an inconclusive result
  and does nothing; see [contained checker runtime](#contained-checker-runtime).
- **Lean evidence**: the qualified Lean distribution is the pinned Linux x86-64
  tree, so a result from any other Lean installation stays blocked as
  unqualified evidence. The pinned CVC5 and Z3 binaries are also Linux x86-64.
  On macOS, solver-backed commands run only binaries that an exact-hash tools
  manifest names, and that route is not qualified.
- **Tau import and export of an imported Tau policy**: the Python parser
  requires a 256 MiB address-space limit. The hosted macOS Python process
  cannot establish that limit, so these operations refuse without creating
  their output directory. The limit is not disabled on macOS.

The OCaml scalar consumer profile, the Tau kit runner and the repository's
acceptance, proof and packaged-workspace gates are checked only on Linux
x86-64. They are not part of the macOS evidence.

## Verify downloads

For a published release, `DOWNLOADS.sha256` lists its flat assets. The install
commands check only the files you downloaded: `sha256sum` on Linux, `shasum -a 256` on macOS.
The detached signature signs the entire `DOWNLOADS.sha256` file. Verify that
signature with `RELEASE-SIGNING-KEY.pub`, then compare the binary's hash with
its entry in the signed file. Confirm the key's fingerprint through a trusted
channel, because downloading it from the same release does not independently
establish trust.

## Matching source and current library API

The current V2 API uses source or path dependencies on the source tag that
matches your CLI. The historical `1.1.0` package keeps its own V1 API. Follow
the release's source pin when you build generated applications.

Before publication, use an explicitly pinned, reviewed source checkout. Once
the matching 2.5.0 release and its source tag are published, clone into a fresh
directory:

```bash
git clone --branch v2.5.0 --depth 1 https://github.com/TheDarkLightX/ZenoFCIS.git ZenoFCIS
git -C ZenoFCIS rev-parse HEAD
# Expected: the source commit named on the v2.5.0 release page
zeno-fcis new my-counter --template durable-counter --source "$PWD/ZenoFCIS"
```

The installed CLI records no build-directory location, so `new` needs an
explicit `--source`. The generated application binds its dependencies and
lockfile to that tree; keep the source and follow the generated README. The
historical prerelease source and binary-build revisions stay in the
[release-candidate record](RELEASE_CANDIDATES.md).

An application beside that 2.5.0 source checkout can use:

```toml
[dependencies]
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", default-features = false }
```

Select optional features from the [feature matrix](FEATURE_MATRIX.md).
Individual current crates also need paths to the same pinned tree. Keep every
dependency in the graph bound to the same release source.

## Contained checker runtime

### Backend selection in the 2.5 integration source

Automatic mode uses direct POSIX children for `finite-data/1` and never selects
systemd. Commands requiring `python-reference/1` (capture open/whole) or
`group-resources/1` (including offline parallel transform checks) refuse before
work with `containment-unavailable:auto:explicit-backend-required`.

Choose a backend with the global `--containment-backend systemd|posix|auto`
flag, or set `ZENO_FCIS_CONTAINMENT_BACKEND=systemd|posix|auto`. An explicit flag
wins over the environment variable; without either, the default is `auto`.
Malformed environment values refuse. The `containment-result/1` record names
the selection source: `flag`, `environment` or `default`.

For example, on a host where the operator accepts a detached service:

```bash
export ZENO_FCIS_CONTAINMENT_BACKEND=systemd
zeno-fcis --no-kb loop capture open --help
# Or choose for one invocation:
zeno-fcis --containment-backend systemd --no-kb loop capture open --help
```

CI and test harnesses requiring systemd capture or offline parallel checking
should set that variable on hosts with no enclosing restrictions to escape.
Tests for automatic selection should unset it or pass `--containment-backend auto`.
Finite-data commands also use `--containment-backend auto` when that variable
selects systemd; the legacy systemd launcher lacks their minimal-environment guarantee.
POSIX currently refuses Python and group-resource profiles; selection never
falls back to a weaker profile.

A sibling service launched by a user manager does not inherit the caller's
sandbox. Namespace and LSM equality cannot establish that roots, Landlock,
I/O limits, CPU period ceilings or launch-time context are preserved. Automatic
qualification cannot close this boundary, so systemd is an informed choice by
the person running the machine. No namespace, LSM or enclosing-budget
qualification gate is applied to that explicit choice.

The explicit launcher retains its own 64 MiB memory/no-swap/64-task policy,
affinity and nice restrictions. It carries the caller's 16 soft:hard rlimits,
CPU scheduling policy/priority and the selected CPU quota ratio/period.
Independent kernel readbacks must succeed before startup acknowledgement and
work. A limit reader that observes no CPU limit refuses instead of inventing
a default; unsupported or unrepresentable carry also refuses. Leaves tighten
each inherited soft and hard limit separately and validate the result.
Supervisor observations are diagnostic evidence, not proof of sandbox equality.
Startup, owned cleanup and receipt-publication checks remain required.

The detached service may escape caller namespaces, filesystem root, LSM,
seccomp, Landlock and cgroup restrictions. Its work is not charged jointly
with existing caller-scope usage: even one checker can exceed a shared budget.
Copying one CPU ratio and period does not preserve every ancestor's burst
ceiling, and enclosing I/O limits are not inherited. The result records these
residuals. These integration-source rules establish no release qualification
or general preservation of the caller's security context.

### Historical 2.4.0 containment requirements

In the historical 2.4.0 source, `loop` commands and `transform check --offline-parallel`
require Linux, cgroup v2 with the memory and pids controllers, and a running
systemd user manager. `systemd-run --user` and `systemctl --user` must work
for the invoking user. On a headless host, `sudo loginctl enable-linger
USERNAME` enables that manager; the user's session then needs
`XDG_RUNTIME_DIR=/run/user/UID` and the user bus at
`unix:path=/run/user/UID/bus`. Hosted checks establish and probe these limits
before they run the acceptance suite.

The child process has a 64 MiB memory cap, no swap, 64 tasks, a bounded CPU
quota, the caller's CPU affinity and no higher scheduling priority. The
inherited control socket has no pathname, so a long `TMPDIR` is supported.
When containment is unavailable, the command returns an explicit
`containment-unavailable` inconclusive result (exit 2). Serial transform
checks and ordinary library use do not need a user manager. Receipt output
needs a filesystem that supports exclusive hard links and directory
synchronization, and an existing destination is never replaced.

The contained runtime is Linux-only. On macOS, each of these commands prints
one JSON result with `"status":"inconclusive"`, `"authority":"none"`,
`"reason":"containment-unavailable:platform"` and
`"accounting_complete":false`, exits 2, and runs and writes nothing. Loop
commands add an `acquisition` record whose `observed_intake` is `null`. Serial
`transform check`, its receipts, `transform replay` and `optimize` have a
macOS route, and the release's installed-binary journey checks receipt
publication on APFS. Qualification requires the matching completed host run.

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
`1.1.0` specification CLI for its historical workflows:

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
