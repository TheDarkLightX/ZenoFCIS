# Packaged application qualification

Base: `e69ac108c4a56afe7f4e0c6384399e3ba9bfe0f3`.
Branch: `agent/packaged-application-20260907`.

## Contract and selected change

The existing development journey builds the generator and dependencies from the
checkout. The release packager compiles unpacked crates, but does not execute
their generator and its resulting application. Its new offline lockfile also
does not establish agreement with the reviewed external dependency graph.

Extend the existing unpacked-workspace check before its cleanup: build the CLI
from the actual `.crate` source set, use that executable to generate the durable
counter, then compile and execute the generated application using only the
unpacked internal packages. Reuse the existing application assertions and one
temporary build target. Share dependency admission and the application journey
with the development checker so the two paths retain the same contract.

Both temporary workspaces must resolve external identities and checksums from
the reviewed lockfile. Every internal manifest must belong to the explicit
source allowlist at the declared version. Preserve the emitted source hashes
before adding an archive-only resolver overlay. Retain a deterministic JSON
receipt containing package identities, the packaged generator identity,
generated files, dependency identities, commands, outcomes, and source status.
The normal release artifact inventory must include this receipt before cleanup.

This is release-tooling work. The counter's laws, canonical formats, nominal
authority, SQLite protocol, Rust APIs, package version, and dependency pins stay
unchanged. Lean stays at `4.30.0` and is neither invoked nor copied. Package-source
qualification does not establish crates.io installation, independent review,
production deployment, or stable `1.0.0` promotion.

## Evidence plan

First add failing dependency-admission cases for external version/checksum drift,
an internal version mismatch, and a fallback to checkout sources. Validate the
new path using locally assembled `.crate` archives, including the existing
bounded decision table and durable lifecycle. Check that required source
omissions fail the packaged generator build. Keep development checks in ATDD;
the complete package journey also runs inside the release packager. Run all
ATDD scenarios immediately before committing and retain exact-source packaging
evidence after the commit. Record any unavailable release gates separately.

No Lean/dependency installation, publication, tag, or merge is part of this
stage. Temporary build outputs belong only to the newly created verification
directory; existing worktrees, toolchains, and caches remain intact.

## Implemented gate

`tools/rc_package.py build` runs the package journey and includes
`PACKAGED-APPLICATION.json` in its manifest, checksums, and bundle. The
`verify-packaged --packages <directory> --output <new-directory>` subcommand
replays the same checker on the checkout identified by the archives.
The release workflow installs the existing Rust pin's Clippy and rustfmt
components for the shared consumer checks. No Lean tooling is needed.

The checker now admits one unambiguous source root per archive, rejects duplicate
normalized paths, and verifies the exact declared package inventory. Temporary
staging uses an owned context that cleans up on both success and failure.
Receipts hash both checker scripts, the reviewed lock and package configuration,
and require those inputs and source status to remain unchanged during the run.
Archive VCS metadata is descriptive; archive hashes still need comparison with
independently retained source and release evidence.

Focused evidence includes twelve dependency, emitted-pin, archive-layout,
cleanup, and compiler-flag tests. Two separate draft staging directories on one
host produced the same packaged generator hash, admitted graph, and application
evidence. The complete 36-package target compilation and generated application's two law tests and
three lifecycle/decision-table tests passed. A required template was then
omitted from the extracted CLI during a real rebuild: compilation failed. After
restoring its bytes, the generator rebuilt to the original hash.

Those draft checks identify their dirty source status. Final pre-commit ATDD,
clean-commit packaging, and retained checksum results belong in the stage's
validation receipt. This local work does not substitute for the final version's
independent review, required platform/formal-tool workflows, two clean release
builders, registry smoke test, or owner release procedure.

## Compiler flag follow-up

A real pinned-Cargo probe found that a staging path containing spaces is split
when the remapping argument is passed through `RUSTFLAGS`. Use Cargo's encoded
Rust and Rustdoc argument variables in both package checks and documentation
assembly. Replace inherited compiler flags so documentation warnings remain
errors. A tiny dependency-free crate under a spaced path checks successful
compilation/documentation and rejection of a real documentation warning. It uses
the installed Rust toolchain and removes only its own temporary directory.

Fable 5.1's supplied-source review also identified inherited compiler/wrapper
overrides, renamed dependency keys missing the early pin checks, and the need
to recheck the clean commit at the end of assembly. The follow-up removes those
environment overrides, resolves aliases to their actual package names for pin
validation and dependency closure, records compiler argument vectors, and
rechecks the source commit before retaining the final manifest. The regression
executes a sibling path dependency and checks its remapped `file!()` output,
then verifies that a real documentation warning still fails. The model review
is advisory; local checks determine whether the proposed changes pass.
