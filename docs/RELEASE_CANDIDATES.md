# Public release candidates

Status: 10 October 2026. Both candidates below are public, signed prereleases.
Each advertises one host executable: the **Linux x86_64 GNU `zeno-fcis` CLI**.
They also supply matching source, crate archives, Rustdoc and evidence manifests.
They do not complete the original stable V2.2/V2.3 scope.

| Candidate | Signed tag and release page | Exact published source |
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

| Candidate | Linux CLI archive | Matching source archive |
|---|---|---|
| V2.2 | [zeno-fcis-2.2.0-rc.1-x86_64-unknown-linux-gnu.tar.gz](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/zeno-fcis-2.2.0-rc.1-x86_64-unknown-linux-gnu.tar.gz) | [zeno-fcis-2.2.0-rc.1-source.tar.gz](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/zeno-fcis-2.2.0-rc.1-source.tar.gz) |
| V2.3 | [zeno-fcis-2.3.0-rc.1-x86_64-unknown-linux-gnu.tar.gz](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/zeno-fcis-2.3.0-rc.1-x86_64-unknown-linux-gnu.tar.gz) | [zeno-fcis-2.3.0-rc.1-source.tar.gz](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/zeno-fcis-2.3.0-rc.1-source.tar.gz) |

Each release has 14 uploaded assets. In addition to the two archives above,
there is a versioned `zeno-fcis-rustdoc-<version>.tar.gz`, a
`zeno-fcis-<version>-rc-bundle.zip` containing crate archives and retained
evidence, and these ten named assets:

- `DOWNLOADS.sha256`, `DOWNLOADS.sha256.sig`, `OWNER-SIGNING-KEY.pub`;
- `PACKAGED-APPLICATION.json`, `PROVENANCE-INPUTS.json`, `QUALIFICATION.json`;
- `RC-MANIFEST.json`, `SBOM.cdx.json`, `SHA256SUMS`, `SOURCE-MANIFEST.json`.

The release pages link every exact filename. `DOWNLOADS.sha256` checks the flat
uploaded asset names; `SHA256SUMS` checks paths inside the extracted bundle.
`PROVENANCE-INPUTS.json` records inputs, not a signature or SLSA attestation.

| Candidate | Download checksums | Detached signature | Owner public key |
|---|---|---|---|
| V2.2 | [DOWNLOADS.sha256](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/DOWNLOADS.sha256) | [DOWNLOADS.sha256.sig](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/DOWNLOADS.sha256.sig) | [OWNER-SIGNING-KEY.pub](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.2.0-rc.1/OWNER-SIGNING-KEY.pub) |
| V2.3 | [DOWNLOADS.sha256](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/DOWNLOADS.sha256) | [DOWNLOADS.sha256.sig](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/DOWNLOADS.sha256.sig) | [OWNER-SIGNING-KEY.pub](https://github.com/TheDarkLightX/ZenoFCIS/releases/download/v2.3.0-rc.1/OWNER-SIGNING-KEY.pub) |

Verify signatures separately under your trusted signing-key policy. A public key
downloaded from the same release does not independently establish trust in its
owner. The signed tag and artifact checksums bind source and bytes, not a
deployment authorization or the truth of application requirements.

## Available components and assurance scope

The candidate source includes the checked library program/Authority route and
the documented contract generation, review, finite transform checking,
optimization, adoption and store-upgrade tools. Advisory review and search
proposals remain separate from the checks that admit programs and publications.
See the [feature matrix](FEATURE_MATRIX.md) and
[factory feature plan](V2_1_V2_2_PLAN.md) for individual contracts and boundaries.

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

## Remaining stable work and separate experiments

The [V2.3 plan](V2_3_PLAN.md) retains the six coupled applications: warehouse,
expense approval, reliable jobs, document release, API quotas and equipment
booking. Each needs its own cross-component laws, binding, atomicity and
delivery evidence. Isolated component certificates do not discharge them.

Other remaining work includes the restricted symbolic encoding/refinement and
independent certificate-checking chain, a working checked public Lean route or
explicit replacement, ZAL explanation/revision/recovery journeys, another
qualified language profile and carried V2.2 obligations. Wider values/full-width
zUSD, checker throughput/accounting, the agent maintenance study, optimizer
depth/real-model evaluation, macOS qualification and minor authoring fixes retain
their original acceptance requirements. The candidate does not silently defer
or complete these items.

Crucible is a separate experimental repository. Its public bounded improvements
and newer private Library experiments are described in
[research status](RESEARCH_STATUS.md). They are not included in these candidate
binaries, and the private packets were not independently replayed for this
documentation update. Native FCIS integration, multi-profile search and a
reusable proof registry remain planned. A search result or proof reference
does not grant adoption, store or publication authority.
