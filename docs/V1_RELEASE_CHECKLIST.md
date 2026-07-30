# ZenoFCIS V1 release checklist

This is the owner runbook for `1.0.0-rc.3` and the later `1.0.0` release. It
separates repository evidence from external publication authority. Passing the
repository gates makes a source revision eligible for release review. It does
not merge a pull request, sign a tag, publish a crate, create a GitHub release,
complete an independent audit, or qualify a downstream deployment.

## Release identities

For RC3, all of these values must agree:

```text
Cargo workspace version: 1.0.0-rc.3
package-set version:      1.0.0-rc.3
Git tag:                  v1.0.0-rc.3
Rust toolchain:           1.97.1
Node developer tooling:  22.23.1
Probity developer tool:  1.10.0
package count:            36 public crates
```

`release/package-set.toml` is the authority for the complete package set and
dependency-first publication order. A tag name is distribution metadata. It
does not change any canonical protocol identifier.

## A. Freeze the candidate

- [ ] Merge the reviewed RC stack onto `main` without a semantic rebase or
      conflict resolution that changes the reviewed source.
- [ ] Require all protected checks and CODEOWNERS review on the resulting exact
      `main` commit. The presence of `.github/CODEOWNERS` does not itself enable
      repository branch protection.
- [ ] Record the full 40-character source commit.
- [ ] Confirm `git status --short` is empty in a fresh checkout of that commit.
- [ ] Confirm there are no unresolved security findings scoped as RC blockers.
- [ ] Confirm `Cargo.toml`, `release/package-set.toml`, `CHANGELOG.md`,
      `docs/RC3_RELEASE_NOTES.md`, and this checklist name `1.0.0-rc.3`.
- [ ] Confirm each claimed platform, runtime mount, proof, or checker has exact
      source, toolchain, configuration, and retained-evidence identities.
- [ ] Preserve every nonclaim that remains true.

Do not tag a PR head and later merge a different commit. The reviewed source,
tag target, package source manifest, and release notes must identify one commit.

## B. Run the exact-source gate

Run from a clean checkout of the recorded commit:

```bash
python3 tools/check_assurance.py --self-test
python3 tools/check_assurance.py
python3 tools/check_library_docs.py
python3 tools/atdd.py self-test
python3 tools/atdd.py check
npm ci --ignore-scripts
NODE_BIN=<exact-node-22.23.1> python3 tools/check_probity.py
npm audit --audit-level=high
python3 tools/rc_package.py self-test
python3 tools/rc_package.py check
cargo +1.97.1 fmt --all -- --check
cargo +1.97.1 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.97.1 test --workspace --all-features --locked
cargo +1.97.1 test --workspace --doc --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo +1.97.1 doc --workspace --all-features --locked --no-deps
cargo +1.97.1 deny check
cargo +1.97.1 audit --ignore RUSTSEC-2026-0173 --deny warnings
python3 tools/atdd.py run --all
python3 tools/rc_package.py build --output /tmp/zeno-fcis-rc3
(cd /tmp/zeno-fcis-rc3 && sha256sum --check SHA256SUMS)
```

- [ ] Every permanent read-only workflow passed at the exact commit.
- [ ] The adopter-acceptance and deterministic developer-guardrail workflows
      passed at the exact commit.
- [ ] The QEMU demo workflow booted the freestanding kernel, matched the closed
      guest transcript, and reproduced the checked-in framebuffer.
- [ ] The release-candidate workflow produced 36 `.crate` packages and the
      complete retained artifact set documented in `PACKAGING.md`.
- [ ] The advisory exception for `RUSTSEC-2026-0173` still matches
      `SUPPLY_CHAIN_EXCEPTIONS.md`; no unnamed advisory was ignored.
- [ ] A second clean builder produced the same retained checksums. The excluded
      Rustdoc global search index is not part of the reproducibility claim.
- [ ] An independent reviewer verified `SHA256SUMS`, `SOURCE-MANIFEST.json`,
      `SBOM.cdx.json`, `PROVENANCE-INPUTS.json`, and `RC-MANIFEST.json`.

Any source change after this gate creates a new candidate commit and requires a
complete rerun. Evidence from an earlier commit cannot validate the new one.

## C. Create the protected RC tag

Tagging is an owner action performed only after sections A and B pass.

```bash
git tag --sign --annotate v1.0.0-rc.3 <exact-commit> \
  --message "ZenoFCIS 1.0.0-rc.3"
git push origin v1.0.0-rc.3
```

- [ ] Verify the tag signature and confirm the peeled tag target equals the
      recorded commit.
- [ ] Confirm the read-only `release-candidate` workflow ran from
      `refs/tags/v1.0.0-rc.3` and passed at that exact commit.
- [ ] Download the tag workflow artifact and verify every checksum again.
- [ ] Compare the tag-built checksums with both clean pre-tag builders.
- [ ] Do not move, delete, or recreate a published tag to repair a failure.
      Correct source is released under a new RC version.

The workflow assembles and uploads review artifacts. It has `contents: read`
and does not sign, attest, publish, or create a release.

## D. Publish crates in dependency order

Crates.io publication is irreversible for a version. An owner with the release
credential processes `publish_order` in `release/package-set.toml` one entry at
a time using the tagged clean checkout:

```bash
cargo +1.97.1 publish --locked --package <next-package>
```

- [ ] Before each command, confirm the package name is the next unpublished
      entry in the reviewed manifest.
- [ ] After each command, confirm crates.io serves exactly version
      `1.0.0-rc.3` and wait until dependencies are visible before continuing.
- [ ] Record the crates.io response and published package checksum.
- [ ] Publish `zeno-fcis` last.
- [ ] Run the checked external-consumer fixture against crates.io without a
      workspace path override after the umbrella package becomes available.

The post-publication smoke test uses a disposable copy and removes only its
local path override and lockfile:

```bash
rc_smoke_dir="$(mktemp -d)"
cp -R fixtures/external-consumer/. "$rc_smoke_dir/"
sed -i '/^[[:space:]]*path = /d' "$rc_smoke_dir/Cargo.toml"
rm "$rc_smoke_dir/Cargo.lock"
cargo +1.97.1 check --manifest-path "$rc_smoke_dir/Cargo.toml"
```

Inspect the generated lockfile and confirm every ZenoFCIS crate was resolved
from crates.io at `1.0.0-rc.3` before deleting the disposable directory.

Do not automate this loop with an unreviewed script. Do not publish from a
dirty tree, another commit, an unreviewed archive, or a local dependency
override.

## E. Publish release evidence

- [ ] Create the GitHub prerelease from the immutable signed tag.
- [ ] Attach the RC bundle, individual retained artifacts, `SHA256SUMS`, and the
      owner-selected detached signature or signed transparency-log reference.
- [ ] Attach or link the hosted provenance/attestation for the exact tagged
      artifacts. `PROVENANCE-INPUTS.json` is input evidence, not an attestation.
- [ ] Link `RC3_RELEASE_NOTES.md`, `SECURITY.md`, the independent review report,
      and known limitations.
- [ ] Verify the GitHub release commit and all artifact digests from a separate
      machine.
- [ ] Verify docs.rs generated all 36 public crate API pages and that the
      examples and feature flags render correctly.
- [ ] Announce only the claims in the release notes.

## F. Failure and rollback policy

If any pre-publication check fails, stop and repair in a new commit. If the RC3
tag already exists, use `1.0.0-rc.4`; never retarget `v1.0.0-rc.3`.

If publication is partially complete:

1. stop before publishing additional dependents;
2. record exactly which immutable crate versions exist;
3. do not reuse `1.0.0-rc.3` for changed source;
4. yank an affected crate version when it is unsafe or unusable, while
   preserving the public incident record;
5. repair and publish the complete coherent set as the next RC version.

If a security defect is discovered after publication, use the private process
in `SECURITY.md`, coordinate disclosure, yank affected packages when needed,
and issue a fixed release. Deleting artifacts or moving tags is not recovery.

## G. Promotion from RC3 to 1.0.0

Final `1.0.0` requires a separate reviewed commit and tag. Before promotion:

- [ ] Resolve RC API feedback and document every intentional breaking change.
- [ ] Complete an independent exact-head review of the final source and
      authority topology.
- [ ] Close every issue designated as a core-library V1 blocker.
- [ ] Reconfirm production-facing storage and outbox-delivery-interpreter nonclaims;
      downstream qualification remains project specific.
- [ ] Change the workspace and package-set versions to `1.0.0`, update all
      versioned documentation and external-consumer pins, and regenerate the
      lockfile and package evidence.
- [ ] Run this complete checklist again using tag `v1.0.0`.

Final Cargo API stability begins at `1.0.0`. Existing canonical protocol
identifiers do not change merely because the Cargo version changes.
