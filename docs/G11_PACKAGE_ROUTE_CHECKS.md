# Shared checker package route qualification

This change routes the production CLI through the published shell-sqlite library.
It does not qualify archives by static inspection alone. The actual library route
passed pinned Verus proof and independent inventory review (901 groups, zero
errors), direct-source native tests and both consumer suites. Development archive
builds and complete compiler dep-info were executed. Final manifest/custody
reconciliation and the complete 24-control mutation qualification remain required.

## Bounded source scope

The CLI adds the existing `zeno-fcis-shell-sqlite = "=1.1.0"` dependency; the lock
changes only that edge. The shell publishes before the CLI. Behaviour/migration
and finite comparison use actual library modules. The private checker algorithm,
its exact ghost specification and the real evaluator are retained. Only plain
comparison data is exposed. The copied upgrade module exists solely in CLI tests;
its declared packaged test input remains. The obsolete equivalence-source test
input is removed. Final manifest hashes must be refreshed by the integration owner.

## Reproducible acceptance sequence

Use the qualified installed toolchains, shared auxiliary lock, MemoryMax=4G,
MemorySwapMax=0, CPUQuota=100%, one Cargo/verifier thread and a disk target selected
by the integration owner. Record commands, exits, stdout/stderr, tool identities,
and before/after hashes. Do not treat a timeout, missing tool, stale manifest or
unreviewed coverage candidate as acceptance.

1. Run the pinned whole-unit `verification/verus/checker.rs` with the existing
   `tools/check_checker.py` proof command. Inspect the actual translated inventory
   for all five `checker_api` executable wrappers and their total postconditions,
   plus its public projection relations. Independently review the full candidate;
   do not carry the previous verified-group count or candidate hash forward.
2. Compile/run the direct-source harness and the driver's actual CLI transform,
   CLI process, shell equivalence and shell upgrade tests under Rust 1.97.1.
   Include the public projection known-answer test and existing canonical receipt
   comparisons. Run the retained CLI contract migration tests to compile its
   test-only upgrade shim against actual library types and claims digest access.
3. Record a production Cargo build and a test no-run JSON build in the
   source layout (the existing `rc_package.repository_layout_reads` workflow).
   Feed their messages and actual target directory to
   `rc_package.observed_cross_package_inputs(messages, layout_root, target_dir)`.
   Require zero non-test cross-package violations. Compare its complete test-input
   set against the refreshed manifest; the CLI upgrade shim remains test-only.
   Preserve the raw `.d` files: library dependencies appear as compiled artifacts,
   not CLI-owned source prerequisites. A textual import scan cannot replace this.
4. Produce the exact declared `.crate` set using the existing release packaging
   workflow. Extract with `rc_package.extract_checked_crate` to sibling directories
   named `<package>-1.1.0`, with no `crates/` source-tree layout or source overlay.
   `write_packaged_workspace_manifest` supplies dependency patches by package name.
   Run `consumer_build_command`: Cargo +1.97.1 build, workspace libraries/binaries/
   examples/all features, locked/offline/jobs1. Preserve archive hashes, normalized
   manifests, resolution evidence and successful CLI artifact. This layout makes
   former sibling-file imports fail rather than borrowing a source checkout.
5. Run the separate packaged-test overlay lane with the checked manifest and the
   full release gate. Test overlays must not enter the production isolation build.
   The existing `check_packaged_workspace` performs these distinct lanes; do not
   weaken its non-test dep-info refusal or archive/source custody checks.
6. Execute the 24 declared proof/coverage controls, including API forwarding
   faults, with the pinned classifier. Root then regenerates identity-bound
   receipts/fixtures and runs the complete source-bound qualification suite.

The static source/graph tests may run cheaply before a heavy slot is allocated.
They test intended wiring and failure admission, not actual compiled dependencies,
proof truth, archive usability, release authority or CI success.

## Development checkpoint, 2026-10-07

The positive-only gate passed with 901 verified groups and zero errors; its actual
native checks passed 135 direct-source, 26 CLI transform, 9 CLI process, 13 shell
comparison and 27 shell upgrade tests. The gate-admission suite now has 14 tests.
The first three mutation specimens were rejected by the real prover. The complete
24-control gate remains required in CI; positive-only evidence does not cover it.

All 36 development archives built their production targets in isolation, and the
separate repository layout compiled every test target. Actual compiler dep-info
found zero non-test cross-package reads and 102 test-input entries. Independent
review accepted the 15 additions and three changed hashes in the derived manifest.
That attempt retained stable source and HEAD hashes but failed its raw-index
custody check. A follow-up must bind every retained archive member, final manifest
and compiled input to unchanged source and staged content before accepting that
development package evidence. The clean committed release archive and installed
application journey remain separate release gates.
