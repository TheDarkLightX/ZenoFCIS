# Continuing ZenoFCIS 2.7 development

ZenoFCIS 2.7.0 is released with the open rows of the
[gap register](FACTORY_GAPS_27.md). This guide explains how another developer
can review and continue that work from a clean checkout. A version string does
not by itself establish that a requirement is qualified.

The factory checkpoint is stacked on
[the composition research PR](https://github.com/TheDarkLightX/ZenoFCIS/pull/152).
Select the reviewed PR branch containing the feature you intend to continue;
later feature branches can include additional files. Fetch its complete source
and read [repository guidance](../AGENTS.md) before editing. For example, the
machine-code loop is on the following branch. Record the actual commit and
working-tree state with each result:

```sh
git fetch origin wip/2.7-machine-code-20261028
git switch --track origin/wip/2.7-machine-code-20261028
git rev-parse HEAD
git status --short
```

Use a separate worktree or branch when sharing a checkout. Keep test outputs,
build targets and evidence outside the frozen source under qualification.

## Implementation map

| Area | Source to inspect | Current boundary |
| --- | --- | --- |
| Finite ZAL meaning and controlled English | `integrations/zal/behavior2.py`, `explain2.py`, `typed2.py` | The explicit finite-FSM3 profile adds a third bounded numeric input and chained add/sub updates. Legacy profile limits remain explicit. This is one ZAL profile; ZAL also has abstraction, semantic-library, visual, category and relational representations. |
| Semantic/notation library and explained classifications | `integrations/zal/finite_library/`, `zal_notation/`; [ontology guide](../integrations/zal/zal_notation/ONTOLOGY.md) | Existing general entries, story, notation and Lenses are retained. The newer positive ontology profile has controlled-English authoring, explanations, replay and six read-only MCP operations. Classification does not certify behavioral relations or application lowering; installed and hosted tooling have separate upgrade boundaries. |
| ZAL-to-application binding | `integrations/zal/app_profile.py`, `factory.py`; [app profile](../integrations/zal/APP_PROFILE.md) | Preserve original authoring, typed model, binding, UI and source-map bytes together. Pure export supports profile3; native app provenance currently admits profile2. Profile3 complete-app qualification remains open. |
| Finite program synthesis | `integrations/zal/program_synthesis.py` | Candidate search and independent checking are separate. An eligibility projection cannot replace the complete Authority decision ABI without correspondence checks. |
| Full-width numeric types and order | `crates/zeno-fcis-value/src/profile2/number256.rs`; [numeric qualification](NUMERIC_256_FOUNDATIONS_27.md) | Actual constructors/views/sign/numeric comparisons have a whole source-bound proof and focused native qualification. Other arithmetic, Schema2 refinement and complete wide program/Authority/application integration remain open. |
| Machine-code search and synthesis | `tools/machine_code/`; [machine-code route](MACHINE_CODE_SYNTHESIS_27.md) | Experimental x86-64/RV64I word kernels use raw proposals, independent byte decoding, full-word SMT checking and replay. Native x86 conformance and RV object identification have separate scopes; general embedded application and ABI integration remain open. |
| Generated UI | `site/public/zenoui/` | Shared component/renderer code and bounded search select per-app presentation trees. The search does not synthesize arbitrary renderer code. Table-list binds existing fixed state tables through metadata/3; dynamic keyed read models remain separate work. |
| Native UI descriptor | `site/factory/src/ui.rs` | Uses actual contract declarations and Authority decisions. The new fixed-table descriptor/dependency edge needs native, Wasm and complete-app qualification. |
| Durable effects and worker boundary | `crates/zeno-fcis-shell-sqlite/src/effects/` | Protected dispatch, channel and native-worker source are included. [Isolation tasks](RUNTIME_ISOLATION_27_PROGRESS.md) identify pending host, lifetime, recovery and installed-route obligations. |
| Composition and native refinement | `crates/zeno-fcis-contract/src/checked_composition/`, `verification/verus/` | Models, restricted native checks and formal statements have distinct scopes. The complete parser-to-native-checker refinement chain remains open. |
| Practical algorithm assessment | [feasibility standard](RESEARCH_FEASIBILITY.md) | Correctness, complexity, concrete costs, crossover and assurance are evaluated independently. Metadata validation does not prove an algorithm or a speedup. |

The [thirty-one-row gap register](FACTORY_GAPS_27.md) preserves the full remaining
scope, including 2.6.0 compatibility, first-class 256-bit arithmetic, DeFi
applications, additional backend qualification, scaling, UI breadth, application
operations, platforms and release checks.

## Reproduction entry points

Use the pinned Rust toolchain and dependency locks. The examples below are
commands to run on the checked-out source; inspect their actual results before
claiming a pass. Offline Cargo commands require the matching dependency cache.
Node UI tests use Node 22. Python reference checks require Python 3.

For finite profile and app-binding changes:

```sh
cd integrations/zal
python3 -B -m unittest test_behavior3 test_app_profile test_app_profile_grouped
python3 -O -B -m unittest test_behavior3 test_app_profile test_app_profile_grouped
```

For UI admission, coverage and the focused shared-input regression, from the
repository root:

```sh
for ui_test in site/tests/zenoui-fixed-tables.test.mjs \
  site/tests/zenoui-contract.test.mjs \
  site/tests/zenoui-recursive-integration.test.mjs \
  site/tests/zenoui-idioms-review.test.mjs; do
  node --max-old-space-size=192 --test --experimental-test-isolation=none \
    --test-concurrency=1 "$ui_test" || exit
done
node --max-old-space-size=192 --test --experimental-test-isolation=none \
  --test-concurrency=1 --test-name-pattern='^mounted explicit Go-to' \
  site/tests/zenoui-grouped-workflow.test.mjs
```

The mounted test retains same-command partial text and cursor while requiring a
new command to restore its own draft, even when the shared input is focused.
It uses a fake DOM and explicit transport fixtures. Real browser behavior needs
separate qualification. Each broad-group file above has its own external Node
process, with direct case reporting inside that one file. Require counts
5, 6, 9 and 7 respectively, plus one focused mounted case. A single filename
envelope does not establish those case results. Changing the runner mode changes
the test environment; retain prior incomplete reports when diagnosing it.

For changed native UI source:

```sh
cargo +1.97.1 test --locked --offline --jobs 1 \
  --manifest-path site/Cargo.toml -p zeno-fcis-site-factory ui::tests \
  -- --test-threads=1
```

Next obtain an actual native capture through the ordinary factory route, build
the matching Wasm, generate a fresh durable application and run its native and
browser journeys. `site/tests/zenoui-fixed-table-native.test.mjs` requires a real
capture supplied through `ZENO_FIXED_TABLE_NATIVE_CAPTURE`; an absent capture
intentionally fails. A diagnostic JSON descriptor cannot substitute for it.

Follow the [runtime reproduction instructions](RUNTIME_ISOLATION_27_PROGRESS.md)
for worker/dispatch changes. Those tests require explicit host prerequisites,
resource limits and owned cleanup. Useful work and intended hostile controls
must both pass; skipped host tests leave the corresponding claim open.

The `zal-profiles` workflow runs the finite-library, notation/Lens, category,
relational-profile and feasibility **source controls**. Its category suite may
explicitly skip tests without a trusted runtime approval. Those skips do not
qualify a native category provider.

Five relational provider suites remain separate manual native gates:
`relational_profile/test_live_provider.py` and the vendor `test_backend.py`,
`test_ladder_adapter.py`, `test_operations.py` and `test_shell.py`, all under
`integrations/zal/`. They require the exact
[retained provider sources and integration route](../integrations/zal/relational_profile/README.md),
a fresh external runtime prepared from that capsule, GHC 9.4.7 and its pinned
`base` 4.17.2.0, `containers` 0.6.7, `aeson` 2.1.2.1, `text` 2.0.2 and
`bytestring` 0.11.5.2 packages. Preserve the source inventory, actual build
record and binary hashes, runtime/scratch bindings and independent replay.
The retained historical validation is not a pass on this checkout. A whole
ATDD or source-workflow pass without these native prerequisites leaves their
full-2.7 qualification open; registry reasons are not exemptions from it.

## Handoff and final qualification

Choose one gap ID and state its exact source, inputs, expected behavior and
independent falsifier before changing it. Keep source changes, tests, commands,
failures and remaining obligations in a reviewable checkpoint. Missing
prerequisites, resource stops and incomplete runs retain their own status.

An earlier test result applies only to its original inputs and assumptions.
Review the relevant differences before reusing it. Native binaries and prior
generated assets cannot qualify changed descriptor or runtime source. Runtime
source bytes also participate in reservation/implementation identities; routine
formatting can therefore require a new source-bound qualification.

After the release assembly is complete, freeze its inputs and run:

```sh
python3 tools/atdd.py run --all
```

To qualify on several hosts or continue after a resource stop, record the same
assembly as an inventory with `run --all --keep-going --run-dir DIR`, optionally
in `--shard I/N` parts, and continue it with `run --resume DIR`. A `blocked`
scenario names its missing host tool and leaves its claim open; see
[acceptance testing](ACCEPTANCE_TESTING.md#inventory-receipts-shards-and-resume).

Complete the affected proofs, platform, artifact, privacy and package checks on
that same assembly. This guide itself requests no merge, deployment or
release.

### Release assembly checklist

The checklist below closes gap row F27-17 for one release commit. Every step
names the existing tool that runs it and the evidence to retain; a step without
its evidence at that exact commit is open. Run the steps in order, because the
first one changes the inputs of all the others.

1. **Decide the version string.** The evaluator digest covers the release
   version in the workspace and crate manifests (row F27-31), so changing
   `2.7.0-rc.1` to `2.7.0` changes every contract identity, transform receipt,
   source pin and core family certificate. After a bump, refresh each pin
   through its producer (`tools/record_*.py`, with `--check` exiting 0),
   requalify the Verus gates that bind the synthesis sources, and regenerate
   the core family certificates. Evidence recorded before the bump applies to
   the earlier bytes only.
2. **Freeze one commit.** `git status --porcelain` prints nothing;
   `python3 tools/release_manifest.py --require-clean > SOURCE-MANIFEST.json`
   succeeds. Retain the commit ID and the manifest.
3. **Registry and static gates.** `python3 tools/atdd.py self-test`,
   `python3 tools/atdd.py check`, `python3 tools/rc_package.py self-test`,
   `python3 tools/rc_package.py check`, `python3 tools/check_assurance.py
   --self-test`, `python3 tools/check_assurance.py`,
   `python3 tools/security_hotspots.py self-test` and `check`,
   `python3 tools/check_probity.py` under Node 22.23.1,
   `python3 tools/check_public_docs.py`, `python3 tools/check_library_docs.py`
   and `python3 tools/check_release_privacy.py check`. Retain each exit code.
4. **Rust release gate.** The commands under
   [required release gate](RELEASE_ASSURANCE.md#required-release-gate):
   formatting, Clippy, workspace and documentation tests, Rustdoc with
   warnings denied, `cargo deny` and `cargo audit`. Retain the logs with their
   test counts.
5. **Full acceptance.** `python3 tools/atdd.py run --all --keep-going
   --run-dir DIR`, in shards when needed, continued with `run --resume DIR`.
   The inventory must show no failed scenario; retain it with its receipts.
   Scenarios run one at a time on earlier commits do not replace this run.
6. **Host-tool scenarios.** Every `blocked` scenario in that inventory names a
   missing host tool (a systemd user manager, OCaml, Lean, the pinned
   solvers, the native-worker host account, Git history). Run the same
   inventory on a host that has them and retain the receipts that replace
   each blocked entry, or state the remaining blocked claims in the release
   notes as not qualified.
7. **Proof gates.** `python3 tools/check_verus.py` with the pinned verifier
   cache, the Lean and formal-tool checks with their `ZENO_FCIS_*`
   executables set, and `python3 tools/record_gate_evidence.py --out FILE`
   for the revision-stamped record. A gate reused from an earlier commit
   needs the reviewed difference of its bound sources recorded next to it.
8. **Package.** `python3 tools/rc_package.py build --output DIR`,
   `python3 tools/rc_package.py verify-packaged` and
   `python3 tools/check_installed_cli.py --release DIR --work-dir WORK
   --report REPORT` on Linux. The macOS executable comes from the
   release-candidate workflow's host job on the tagged commit.
9. **Platform closure.** The release-candidate workflow and the packager know
   the Linux x86_64 and macOS Apple-silicon hosts only. A Windows executable
   needs its own build and installed-binary journey recorded like the other
   hosts, or the release notes and installation guide say that this release
   publishes none.
10. **Custody.** `python3 tools/rc_package.py merge-hosts`,
    `python3 tools/rc_package.py downloads-list --out DOWNLOADS.sha256` and
    `python3 tools/check_release_custody.py --release DIR --report FILE
    --bindings FILE --downloads DOWNLOADS.sha256 --tag vVERSION`. The report
    must say `passed` with all eleven checks run; `incomplete` is not a pass.
11. **Sign.** The release owner signs `DOWNLOADS.sha256` and the per-platform
    checksum lists with the release key in the `file` namespace
    (`ssh-keygen -Y sign -n file`), the namespace the published verification
    uses, and checks each signature with `ssh-keygen -Y verify -n file`.
12. **Publish.** Tag the frozen commit, publish exactly the assets named in the
    signed download list plus the signatures and key, and update the
    changelog, release notes, candidate qualification table and citation
    file to the same version.
13. **Retain and close.** Keep the items listed under
    [evidence retention](RELEASE_ASSURANCE.md#evidence-retention) and record
    the release commit in the F27-17 row of the gap register. A row closes
    only with that commit's evidence.
