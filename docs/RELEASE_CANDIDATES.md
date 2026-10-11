# Public release candidates

Status: 10 October 2026. Both candidates below are public prereleases. Each
advertises one host executable: the **Linux x86_64 GNU `zeno-fcis` CLI**, with
offline Rustdoc. They do not complete the original stable V2.2/V2.3 scope.

| Candidate | Release page | Source checkout revision | Binary build revision |
|---|---|---|---|
| V2.2 | [v2.2.0-rc.1](https://github.com/TheDarkLightX/ZenoFCIS/releases/tag/v2.2.0-rc.1) | `66e8e05180e0883e570e5459a5dc94f99d8e0d9b` | `c94c962f4d223f76dbc9ba9cb332cffd4b25436b` |
| V2.3 | [v2.3.0-rc.1](https://github.com/TheDarkLightX/ZenoFCIS/releases/tag/v2.3.0-rc.1) | `cb813cca3ddc4a4129c26f09f0d53e4052893c06` | `dd04cbcb3f7d8694de5f79bc1280b9252fdd147b` |

Runtime, proof, test, and dependency files are unchanged between each
candidate's source checkout revision and its binary build revision.

Use the source that matches your installed binary. The development `main`
workspace still declares Cargo version `1.1.0`; neither that version string nor
a plain checkout of `main` identifies a candidate's exact source. The current
V2 API requires pinned source or path dependencies, not the historical `1.1.0`
package. The [installation guide](INSTALLATION.md) gives the commands and
explains how a generated application binds to its source. No crates.io
publication of the candidates and no macOS or Windows executable is claimed.

## Downloads

The Linux CLI and Rustdoc archives stay on their release pages. The retained
`DOWNLOADS.sha256`, its detached signature, and the signing key verify the
binary downloads. Verify only the files you downloaded: the commands check only
the entries you requested, not every entry in the list.

| Candidate | Download checksums | Detached signature | Signing key |
|---|---|---|---|
| V2.2 | [DOWNLOADS.sha256](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/DOWNLOADS.sha256) | [DOWNLOADS.sha256.sig](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/DOWNLOADS.sha256.sig) | [RELEASE-SIGNING-KEY.pub](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/RELEASE-SIGNING-KEY.pub) |
| V2.3 | [DOWNLOADS.sha256](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/DOWNLOADS.sha256) | [DOWNLOADS.sha256.sig](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/DOWNLOADS.sha256.sig) | [RELEASE-SIGNING-KEY.pub](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/RELEASE-SIGNING-KEY.pub) |

The detached signature signs the entire `DOWNLOADS.sha256` file. Verify that
signature with `RELEASE-SIGNING-KEY.pub`, then compare the binary's hash with
its entry in the signed checksum file. Confirm the signing key's fingerprint
through a trusted channel, because downloading the key from the same release
does not independently establish trust. The Git tags are not the original
signed binary build tags. They carry no deployment authority and do not
certify application requirements.

## Available components and assurance scope

The candidate source includes the checked library program and Authority
route. It also includes the documented tools for contract generation, review,
finite transform checking, optimization, adoption, and store upgrades.
Advisory review and search proposals stay separate from the checks that admit
programs and publications. The [feature matrix](FEATURE_MATRIX.md) lists the
supported features and their boundaries.

V2.2 includes three finite component families: reservation pools,
fixed-window rate limiters, and single-request approval queues. V2.3 expands
the [catalogue](../core-components/README.md) to ten families, 77 declared
parameter instances, and 107,518 raw input tuples, including invalid but
schema-admitted prestates. Its recorded qualification covers 308 native tests
of generated applications, 17 actual law mutants, seven
certificate-tampering controls, and, for all 77 instances, proof references
that survive ordinary scaffolding.

Stored certificate references are **Identified**. Successful exhaustive replay
supports **Proved for the declared closed finite scope** under the
[named trusted base](../core-components/FINITE_PROOFS.md). Source-hash
agreement alone is not replay. This is not an unbounded family theorem, Lean
KernelChecked evidence, a universal functional-core guarantee, or
qualification of a coupled application. Supplied authorization and time remain
observations, and authenticating them is the application shell's job.

## Qualification

The recorded full local ATDD runs passed all 64 scenarios at these sources:

| Series | Full local 64/64 acceptance source | Binary build revision |
|---|---|---|
| V2.2 | `52dff015b21eaab626a38b59633a506ad61f917f` | `c94c962f4d223f76dbc9ba9cb332cffd4b25436b` |
| V2.3 | `c6a2cf5b3d274343ae24f99b35401fdac77394aa` | `dd04cbcb3f7d8694de5f79bc1280b9252fdd147b` |

Each binary build differs from its acceptance source only in two
packaging-fixture hashes; runtime and test files are unchanged. Focused package
checks, hosted assembly, the installed CLI version check, and the generated
durable-counter journey passed. A complete hosted CI run and a fresh full
acceptance run at the binary revision are not claimed.

## Current limits

These are release candidates for the supported subset; they do not complete
the full stable V2.2/V2.3 scope. Finite component certificates do not
establish a coupled application's state, interference, atomicity, or delivery
guarantees. No general symbolic certificate chain, universal core correctness,
additional platform qualification, or native experimental-search integration
is claimed.
