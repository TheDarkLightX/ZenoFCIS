# RC3 readiness review

Review date: 2026-07-30

Review base: `37faa195a05b7f843d559878ec472d15a6d9de57` plus the RC3 candidate changes in this worktree.

This review asked three developers to approach RC3 from different positions: a
first-time Rust adopter, a formal-methods and process-security reviewer, and a
systems release engineer. All three reached the same initial decision. The
candidate needed a focused polish pass before publication.

## Persona findings

### First-time Rust adopter

The adopter wanted the documented command to be the command that acceptance
testing actually runs. Earlier bindings called parser functions directly for
the Mini Determinator and compared generated values in memory for drift. The
current acceptance tests start the published `zeno-fcis` process, check the
checked-in Mini project, inspect versioned JSON, generate files, detect a local
edit with `--check`, and confirm that check mode leaves the edited file alone.

The isolated downstream consumer now enables the umbrella `authoring` feature,
parses and elaborates a complete project, and checks the public typed project
identity. This exercises the public adoption path outside the workspace package
graph.

### Formal methods and process-security reviewer

The reviewer found five boundary risks: deeply nested formulas reached the
Rust call stack, very large finite horizons could request huge translations,
input delivery happened outside the effective timeout, two source names could
map to one solver name, and the configured executable could change after its
hash check.

RC3 now stops formula nesting at depth 256, stops finite horizons above 256,
and preflights formal exports before recursive rendering. The process timeout
covers input delivery. Solver names use injective length-prefixed hexadecimal
encoding. Tool admission copies the exact hashed bytes to a private executable
and uses that copy for the version check and requested run.

### Systems release engineer

The release engineer found that both declared binaries used the same archive
path. The later binary could overwrite the earlier one. The provenance input
also named only one binary command.

The packager now derives a unique archive, top-level directory, executable
member, and provenance command from every declared binary target. Validation
rejects duplicate target names. A self-test opens each generated archive and
checks that it contains the intended executable.

## User stories and executable acceptance

| Developer story | ATDD scenario | Executable evidence |
| --- | --- | --- |
| As an adopter, I can create a project without losing an existing file. | `rc3-project-new` | Starts `zeno-fcis new`, checks exit 1, and verifies the sentinel bytes. |
| As an adopter, I can check the Mini Determinator with the installed command. | `rc3-mini-os-check` | Starts `zeno-fcis check ... --format json` twice and compares the output. |
| As an adopter, I can detect generated drift without changing my files. | `rc3-generated-drift` | Generates artifacts, edits one, runs `--check`, and verifies the edit remains. |
| As a tool author, I receive stable versioned JSON for valid and invalid input. | `rc3-cli-json-contract` | Checks schema, status, diagnostics, and canonical key order. |
| As a reviewer, hostile nesting and horizons stop inside a documented envelope. | `rc3-resource-envelopes` | Runs parser-depth, horizon, and export-preflight boundary tests. |
| As a security reviewer, timeout and executable identity cover the complete run. | `rc3-process-boundary` | Runs blocked-input timeout, injective-name, and admitted-byte tests. |
| As a release engineer, each declared binary reaches one distinct archive. | `rc3-package-binary-inventory` | Runs the hostile package mutations and archive-member self-test. |

A focused local mutation audit used `cargo-mutants 26.0.0` against the exact
boundaries added during this review. The parser and formula-shape set caught 22
mutations with 1 unviable mutation. The formal export, process, identifier, and
executable-admission set caught 45 mutations with 8 unviable mutations. No
mutation survived either final run. These results are candidate evidence and
must be repeated or retained by the exact-source release gate.

The full registry contains 25 scenarios. Each Gherkin scenario has exactly one
`@atdd-*` tag and one closed tuple of argument arrays. Feature prose never
becomes a command.

## Release decision

The repairs remain part of `1.0.0-rc.3` because that version has not been
published or tagged. A new `rc.3.1` version is unnecessary before the first RC3
publication.

RC3 becomes publishable after one candidate commit passes every item in the
external release boundary below. Stable `1.0.0` should wait for real adopter
feedback on RC3, one independent API review, and confirmation that no breaking
public change is required.

## External release boundary

Local success establishes candidate eligibility. Publication requires these
external actions against one frozen commit:

1. Merge the reviewed candidate to the exact `main` revision intended for the release.
2. Run every protected exact-head workflow, including formal tools, QEMU boot,
   no-std, Miri, fuzz smoke, supply chain, docs, ATDD, and package assembly.
3. Build the complete artifact set in two independent clean environments and
   compare the crate packages, source archive, binary archives, documentation
   archive, manifests, checksums, SBOM, and provenance inputs.
4. Obtain an independent review of the exact artifacts and public API surface.
5. Confirm that `1.0.0-rc.3` is still unpublished, create the signed immutable
   tag, and let the read-only tagged workflows check that revision.
6. Publish the 36 crates in the dependency order recorded in
   `release/package-set.toml`, then attach the checked artifacts and checksums.
7. Run the clean downstream smoke test from the published packages and record
   the resulting package identities.

The tag, signatures, hosted provenance, crates.io publication, and independent
review remain outside repository-local authority. The complete owner procedure
is in [V1_RELEASE_CHECKLIST.md](V1_RELEASE_CHECKLIST.md).

## RC3 scope and V1 follow-up

The QEMU Mini Determinator is a freestanding demonstration of the public
semantic API. It shows the deterministic join and stable conflict result in a
real guest framebuffer and serial transcript. Its evidence applies to that
bounded demo configuration. It carries no production kernel, hardware
isolation, scheduling, or deployment claim.

The following work can follow RC3 without changing the current public semantic
formats:

- grow the pinned solver corpus with more quantified, arithmetic, and temporal
  boundary claims;
- add declared predicate signatures to the project language;
- provide a smoother generated Lean theorem handoff;
- retain richer normalized counterexample traces;
- collect first-time adopter completion time and error-recovery feedback.

CVC5 Alethe output remains proposal evidence subject to independent checking.
Z3 UNSAT remains blocked. Finite temporal results keep their declared horizon.
These boundaries are part of the release contract.
