# Continuing the ZenoFCIS 2.7 development checkpoint

This branch preserves the implementation under development so another developer
can review and continue it from a clean checkout. Full 2.7 qualification remains
incomplete. Development version strings do not establish a qualified release.

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
| ZAL-to-application binding | `integrations/zal/app_profile.py`, `factory.py`; [app profile](../integrations/zal/APP_PROFILE.md) | Preserve original authoring, typed model, binding, UI and source-map bytes together. Pure export supports profile3; native app provenance currently admits profile2. Profile3 complete-app qualification remains open. |
| Finite program synthesis | `integrations/zal/program_synthesis.py` | Candidate search and independent checking are separate. An eligibility projection cannot replace the complete Authority decision ABI without correspondence checks. |
| Full-width numeric types and order | `crates/zeno-fcis-value/src/profile2/number256.rs`; [numeric qualification](NUMERIC_256_FOUNDATIONS_27.md) | Actual constructors/views/sign/numeric comparisons have a whole source-bound proof and focused native qualification. Other arithmetic, Schema2 refinement and complete wide program/Authority/application integration remain open. |
| Machine-code search and synthesis | `tools/machine_code/`; [machine-code route](MACHINE_CODE_SYNTHESIS_27.md) | Experimental x86-64/RV64I word kernels use raw proposals, independent byte decoding, full-word SMT checking and replay. Native x86 conformance and RV object identification have separate scopes; general embedded application and ABI integration remain open. |
| Generated UI | `site/public/zenoui/` | Shared component/renderer code and bounded search select per-app presentation trees. The search does not synthesize arbitrary renderer code. Table-list binds existing fixed state tables through metadata/3; dynamic keyed read models remain separate work. |
| Native UI descriptor | `site/factory/src/ui.rs` | Uses actual contract declarations and Authority decisions. The new fixed-table descriptor/dependency edge needs native, Wasm and complete-app qualification. |
| Durable effects and worker boundary | `crates/zeno-fcis-shell-sqlite/src/effects/` | Protected dispatch, channel and native-worker source are included. [Isolation tasks](RUNTIME_ISOLATION_27_PROGRESS.md) identify pending host, lifetime, recovery and installed-route obligations. |
| Composition and native refinement | `crates/zeno-fcis-contract/src/checked_composition/`, `verification/verus/` | Models, restricted native checks and formal statements have distinct scopes. The complete parser-to-native-checker refinement chain remains open. |
| Practical algorithm assessment | [feasibility standard](RESEARCH_FEASIBILITY.md) | Correctness, complexity, concrete costs, crossover and assurance are evaluated independently. Metadata validation does not prove an algorithm or a speedup. |

The [twenty-one-row gap register](FACTORY_GAPS_27.md) preserves the full remaining
scope, including first-class 256-bit arithmetic, DeFi applications, additional
backend qualification, scaling, UI breadth, platforms and release checks.

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

Complete the affected proofs, platform, artifact, privacy and package checks on
that same assembly. This development handoff leaves those release obligations
open and requests no merge, deployment or release.
