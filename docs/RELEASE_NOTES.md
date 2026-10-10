# ZenoFCIS 2.2.0-rc.1

This is a release candidate for the current checked application path. It is
not the completed stable V2.2 or V2.3 milestone. The owner selected candidate
publication so users can try the available code while the original stable
scope is completed. The published V1.1 release remains a separate historical
API; V2 is a breaking Rust API change.

The candidate includes the simplified, library-owned V2 Authority, complete
declarative decisions and laws, generated durable applications, typed delivery,
reviewed upgrades and migration admission, checked finite optimization, ZAL
authoring, and three closed finite component families. Reading a component's
stored certificate establishes identity only. Exhaustive replay establishes
its finite claim for the declared instances under the named trusted base.

The advertised host binary is the `zeno-fcis` authoring CLI for Linux x86_64
GNU. The bundle also contains all 36 crate archives, source, offline Rustdoc,
checksums, an SBOM and source/provenance input manifests. Those manifests name
build inputs; they are not themselves signed attestations. No macOS or Windows
binary, crates.io publication, or Python/JavaScript host implementation is
advertised by this candidate. Generated Python and JavaScript conformance
checks remain within their documented scope.

Installation instructions are in [Installation](INSTALLATION.md); the source
pin and asset checksums belong to the matching GitHub prerelease. Use only
assets whose source manifest matches the candidate's immutable tag. A checksum
establishes byte identity; verify the signed tag's key independently.

## Assurance and compatibility

The core computes the decision, successor, laws, resource usage and publication
capability. The durable shell interprets genuine capabilities. A formal claim
is always limited to its checked specification, input domain, execution path
and trusted assumptions. Supplied authorization and time are data, not caller
authentication or clock attestation. Neither formal checks nor a release
candidate establish that requirements capture human intent or that an external
receiver performs an effect correctly.

Changing the package version refreshes evaluator/source identities. Existing
stores must retain their original policy and matching executable or follow the
explicit reviewed upgrade path; opening them with a different identity is not
an implicit migration. Candidate APIs and stored formats may change before
stable release. Preserve an export and the original executable before testing
an upgrade. Protocol identifiers remain governed separately from Cargo versions.

## Work still required for stable release

The original [V2.2 plan](V2_1_V2_2_PLAN.md) and [V2.3 plan](V2_3_PLAN.md) remain
the milestone definitions. In particular, the larger compound/full-width
execution path and its final integrated proof, review and acceptance evidence
are unfinished. Roadmap items and experimental examples are not promoted by
this candidate. Stable platform qualification and the final complete release
ceremony are also still required. The Zeno-Crucible handoff is a separate
2.3.1 integration proposal with unresolved admission and semantic-review gates.

The release checklist records checks for the exact candidate and does not
convert inherited or queued evidence into a passing result.
