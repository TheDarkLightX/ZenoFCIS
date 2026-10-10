# Public release candidates

Status: 10 October 2026. Both candidates below are public prereleases.
Each advertises one host executable: the **Linux x86_64 GNU `zeno-fcis` CLI**.
Their unchanged binaries, Rustdoc and evidence manifests remain available.
Source and complete bundles containing internal documents have been withdrawn.
They do not complete the original stable V2.2/V2.3 scope.

| Candidate | Release page | Original binary build source |
|---|---|---|
| V2.2 | [v2.2.0-rc.1](https://github.com/TheDarkLightX/ZenoFCIS/releases/tag/v2.2.0-rc.1) | `c94c962f4d223f76dbc9ba9cb332cffd4b25436b` |
| V2.3 | [v2.3.0-rc.1](https://github.com/TheDarkLightX/ZenoFCIS/releases/tag/v2.3.0-rc.1) | `dd04cbcb3f7d8694de5f79bc1280b9252fdd147b` |

Use the source matching your installed binary. The development `main` workspace
still declares Cargo version `1.1.0`; neither that version string nor a plain
checkout of `main` identifies a candidate's exact source. The current V2 API
requires pinned source/path dependencies, rather than the historical `1.1.0`
package. Follow [installation](INSTALLATION.md) for commands and generated-app
source binding. No candidate crates.io publication or macOS/Windows executable
is claimed.

## Downloads

The Linux CLI archives remain on their release pages. The source tarballs and
complete bundles were withdrawn during documentation curation. Existing signed
checksum lists still describe the original artifacts, including withdrawn ones;
they have not been modified or re-signed.

The retained `DOWNLOADS.sha256`, its detached signature and the owner public key
can verify the unchanged binary downloads. Verify only files you downloaded;
missing withdrawn archives are not verification failures for those binaries.

| Candidate | Download checksums | Detached signature | Owner public key |
|---|---|---|---|
| V2.2 | [DOWNLOADS.sha256](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/DOWNLOADS.sha256) | [DOWNLOADS.sha256.sig](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/DOWNLOADS.sha256.sig) | [OWNER-SIGNING-KEY.pub](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/OWNER-SIGNING-KEY.pub) |
| V2.3 | [DOWNLOADS.sha256](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/DOWNLOADS.sha256) | [DOWNLOADS.sha256.sig](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/DOWNLOADS.sha256.sig) | [OWNER-SIGNING-KEY.pub](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/OWNER-SIGNING-KEY.pub) |

Verify signatures separately under your trusted signing-key policy. A public key
downloaded from the same release does not independently establish trust in its
owner. Artifact signatures retain their original byte bindings. Documentation-only
history curation changed Git commit IDs and removed the affected tag signatures;
the rewritten tag is not the original signed build tag. It grants no deployment
authority and does not certify application requirements.

## Available components and assurance scope

The candidate source includes the checked library program/Authority route and
the documented contract generation, review, finite transform checking,
optimization, adoption and store-upgrade tools. Advisory review and search
proposals remain separate from the checks that admit programs and publications.
See the [feature matrix](FEATURE_MATRIX.md) for supported features and boundaries.

V2.2 includes the three finite component families: reservation pools,
fixed-window rate limiters and single-request approval queues. V2.3 expands
the [catalogue](../core-components/README.md) to ten families, 77 declared
parameter instances and 107,518 raw input tuples, including invalid but
schema-admitted prestates. Its recorded qualification covers 308 generated-app
native tests, 17 actual law mutants, seven certificate-tampering controls and
proof references surviving ordinary scaffolding for all 77 instances.

Stored certificate references are **Identified**. Successful exhaustive replay
supports **Proved for the declared closed finite scope** under the
[named trusted base](../core-components/FINITE_PROOFS.md). Source-hash agreement
alone is not replay. This is not an unbounded family theorem, Lean KernelChecked
evidence, a universal functional-core guarantee or qualification of a coupled
application. Supplied authorization and time remain observations whose
authentication belongs to the application's shell.

## Qualification and publication record

The recorded full local ATDD runs passed all 64 scenarios on these exact heads:

| Series | Full local 64/64 acceptance source | Published packaging repair source |
|---|---|---|
| V2.2 | `52dff015b21eaab626a38b59633a506ad61f917f` | `c94c962f4d223f76dbc9ba9cb332cffd4b25436b` |
| V2.3 | `c6a2cf5b3d274343ae24f99b35401fdac77394aa` | `dd04cbcb3f7d8694de5f79bc1280b9252fdd147b` |

Each published repair changed only two packaging-manifest digests. The owner
explicitly permitted focused packaging checks with inherited unchanged-code
acceptance evidence for this narrow correction. Fresh package authority,
negative controls, privacy checks and exact-head hosted assembly passed.
Full hosted CI was unfinished at publication. A fresh full ATDD run at the
repair heads and an all-green hosted-CI result are not claimed by this record.

For each candidate, a durable-counter journey generated with the installed CLI
passed, all 44 internal bundle checksums and all 14 uploaded asset
digests matched, and unauthenticated public binary downloads matched. Those
checks qualify the stated candidate artifacts and journey; they do not replace
the remaining stable scope or downstream deployment qualification.

## Current limits

These are release candidates for the supported subset, not completion of the
full stable V2.2/V2.3 scope. Finite component certificates do not establish a
coupled application's state, interference, atomicity or delivery guarantees.
No general symbolic certificate chain, universal core correctness, additional
platform qualification or native experimental-search integration is claimed.

## Curated source trees

The binary build revisions above remain historical provenance; they are not the
new Git source revisions. The rewritten tags contain the same runtime, test,
proof and dependency files, with internal documentation removed or edited.
Use their current source pins from [installation](INSTALLATION.md). This privacy
operation is not a new binary build or a new qualification run. Old clones can
still hold withdrawn documents; re-clone instead of merging their history.
