# Candidate release checklist

This checklist covers `2.2.0-rc.1`. The owner authorized publishing release
candidates first, then completing the original stable scope. It does not
authorize claiming that the stable milestones are complete. Historical V1
release checklists retain their own scope.

1. Freeze the candidate source. Cargo, the package set, all internal dependency
   pins, npm root metadata and the CLI version must agree. Keep external Cargo
   and npm dependencies unchanged. Regenerate evaluator identities and bind
   finite certificates to the exact runtime and compiler.
2. Run the appropriate source/proof and package checks. Run
   `python3 tools/atdd.py run --all` immediately before the exact-tree commit.
   Preserve every scenario verdict, resource cap and source snapshot. A
   timeout, queued job or incomplete log is not acceptance.
3. Push the candidate branch; inspect exact-head hosted CI. Create an immutable
   annotated signed tag `v2.2.0-rc.1` only for the accepted source. The tag and
   package version must match. Do not move an already published tag.
4. Assemble the reviewed `tools/rc_package.py` bundle from that clean commit
   using pinned Rust 1.97.1 and Node 22.23.1. Prefer the retained exact-source
   hosted build. Confirm its `SOURCE-MANIFEST.json` and `RC-MANIFEST.json`
   name that commit, the right version and a clean checkout.
5. Download the bundle, check every entry in `SHA256SUMS`, inspect archives
   and run the release privacy scan. Install the advertised Linux binary into
   a disposable location; check its version and run a generated application's
   checked store journey from the installed assets. Do not infer another
   platform's qualification from a Linux test.
6. Create a GitHub **prerelease**, attach the binary and complete bundle,
   manifests, checksums and retained qualification summary. Verify the remote
   tag target and uploaded asset checksums. Publish honest scope and limitations
   from [release notes](RELEASE_NOTES.md). Record the release URL.

No crates.io publication is part of this candidate procedure. Consumers may
use the pinned source or the supplied crate archives. A Git tag signature
authenticates the named signer and source; it does not prove business intent,
make `PROVENANCE-INPUTS.json` an attestation, or certify an external service.

Before stable publication, complete the original V2.2/V2.3 scope, requalify the
final source and supported platforms, review all changed assurance boundaries,
and carry out the owner's stable release procedure with coherent package
publication. A candidate's checks do not close those remaining obligations.
