# BDD and acceptance testing

ZenoFCIS uses behavior-driven scenarios to make the RC3 product contract
readable by adopters, reviewers, and coding agents. It uses acceptance-test
driven development to bind each scenario to a fixed executable command set.

## Roles

```text
BDD feature text
    states the user-visible requirement and nonclaim

closed ATDD registry
    maps one stable scenario ID to fixed argv arrays

Rust tests and checkers
    provide the executable evidence behind the scenario

formal/refinement evidence
    separately supports its exact theorem or bounded claim
```

BDD does not prove a transition law. ATDD does not make a finite test portfolio
complete. They close adoption requirements and detect public workflow drift.
Canonical, property, differential, mutation, Miri, crash, refinement, and formal
evidence retain their existing roles.

## Commands

Check that every feature scenario has exactly one registered executable ID and
that the registry has no hidden scenarios:

```bash
python3 tools/atdd.py self-test
python3 tools/atdd.py check
```

List or run scenarios:

```bash
python3 tools/atdd.py list
python3 tools/atdd.py run --scenario minimal-core
python3 tools/atdd.py run --all
```

The runner uses fixed argument arrays and does not execute command text from a
feature file. This keeps Gherkin prose outside execution authority.

Every run first passes the fixed preflight: `tools/test_atdd_registry.py`,
`tools/test_scaffold_versions.py`, `tools/check_authority_v2.py
--check-sources` and the `v2_evaluator_identity` test. A preflight failure
stops every mode with exit 1. An ordinary run then stops at the first failing
scenario (exit 1) and prints `atdd: complete PASS (N scenarios)` only when
every selected scenario passed.

### Inventory, receipts, shards and resume

For release qualification, record a complete inventory instead:

```bash
python3 tools/atdd.py run --all --keep-going --run-dir RUN_DIR
python3 tools/atdd.py run --all --keep-going --shard 2/4 --run-dir RUN_DIR_2
python3 tools/atdd.py run --resume RUN_DIR
```

- `--keep-going` runs every selected scenario. Each one is `pass`, `fail` (its
  first failing command ends that scenario, and the run continues) or
  `blocked`. Without `--run-dir`, a new directory is created under the
  temporary directory and printed. `--run-dir` must be absent or empty.
- A scenario is `blocked` when a host prerequisite it declares is missing,
  and none of its commands run. The receipt and the summary name the missing
  tool, the unset variable or the absent path. A blocked scenario is never a
  pass; its acceptance claim stays open. The declared prerequisites are OCaml
  4.14.1 at `/usr/bin/ocamlopt` (`ocaml-scalar-consumer`), the pinned CVC5,
  Z3 and certificate checker variables (`symbolic-per-case-checks`), Node.js
  and the `npm ci` dependencies (`zal-dialogue`), Node.js 22 through
  `NODE_BIN` or `node` (`finite-synthesis`), and `/usr/bin/bwrap`,
  `/usr/bin/prlimit` and `cc` (`native-worker-host`). Ordinary runs do not
  check them.
- `RUN_DIR/receipts/ID.json` (`zeno-fcis/atdd-receipt/1`) records the
  scenario's status, the commit, its tree, whether tracked files differ from
  it (`dirty`) and the SHA-256 of those differences, each command's argv,
  exit code and seconds, the missing tools or the failure, and the SHA-256 of
  `RUN_DIR/logs/ID.log`, which holds the commands' output.
  `RUN_DIR/summary.json` (`zeno-fcis/atdd-summary/1`) lists every selected
  scenario once with its status and receipt digest. `RUN_DIR/run.json`
  (`zeno-fcis/atdd-run/1`) binds the selection to the checkout.
- Exit 0 means every selected scenario passed, 1 that one failed or has no
  receipt, and 2 that some are blocked and none failed. `complete PASS` is
  printed only for exit 0 without `--shard`.
- `--shard I/N`, with `--all`, selects every Nth scenario of the sorted IDs
  starting at the Ith, so the N shards partition the registry. A passing shard
  prints `atdd: shard I/N PASS`, never `complete PASS`.
- `--resume RUN_DIR` reruns, in the same selection and shard, only the
  scenarios that have no passing receipt. Earlier receipts and logs are kept as
  `ID.attemptK.json` and `ID.attemptK.log`. It refuses when the checkout's
  commit, tree or tracked changes differ from those in `run.json`.
- A scenario that changes tracked files stops the run, because later receipts
  would no longer describe the recorded checkout. Restore the checkout and
  resume. Untracked files are not compared.

Build tests with optimization when they run contained loops:

```bash
export CARGO_PROFILE_DEV_OPT_LEVEL=1 CARGO_PROFILE_TEST_OPT_LEVEL=1
cargo +1.97.1 test --locked --offline --workspace
python3 tools/atdd.py run --all
```

A contained loop starts a caller and a guardian process, and the worker must
acknowledge startup within 10 seconds. In an unoptimized build, each of those
processes spent about 5 seconds on startup work on the 2.6.0 qualification host,
so loop tests reported `containment-unavailable:posix:startup` (exit 2). The
same command in the release build started in under a second.

`tools/test_atdd_registry.py`, which runs before every scenario run, also
requires every `test_*.py` in the repository that git does not ignore to be
named by a whole-file command in a scenario or workflow step (triggers and step
conditions are not checked), or to run inside one of those: from a script that
a command invokes with the listed leading arguments, or from a test file that
imports all its test cases. A `-k` filter or a test name after the file makes a
command partial, a file run directly must call an unrestricted
`unittest.main()`, workflow matrix values come only from rows that are not
commented out or excluded, and some workflow must run
`python3 tools/atdd.py run --all`. A file that runs nowhere must be listed
there with a reviewed reason.

## RC3 scenario registry

| Scenario ID | Acceptance boundary |
|---|---|
| `bounded-collections-pipes` | Entry/item/snapshot bounds, canonical backend parity, immutable refusal, FIFO order and pending-ID pipe contracts. |
| `minimal-core` | Immutable transition and logical budget example. |
| `checked-backend` | Bounded tool-neutral backend request example. |
| `zal-dialogue` | Deterministic finite-FSM help and explanation, why-not/what-if/narrative explanations, registry-generated help, revision-bound ladder tools, paired language views, exact-revision human review, terminal/MCP shared state, UI-state doubles, planted-defect controls and actual sample-domain factory replay. Native browser and model-provider access remain separate. |
| `external-consumer` | Isolated downstream compile against the public specification API. |
| `project-bootstrap` | Deterministic generated starter and negative vectors. |
| `generated-application` | Authored schema lowering, complete invocation binding, runtime laws, dependency admission, and isolated durable lifecycle, for the durable counter and the six example applications. |
| `example-templates` | The account-lockout, order-fulfillment, inventory-reservation, compliance-gateway, withdrawal-queue, and agent-treasury-guard templates emit exactly their files, refuse to overwrite, and carry only substantive law formulas with resolved paths. |
| `composed-program` | Fixed domain-machine and global composition portfolio. |
| `production-authority` | Catalog, invocation, law, genesis, and nominal commit authority. |
| `sqlite-authority` | Crash-atomic authorized history and durable outbox. |
| `release-contract` | Local formatting, lint, tests, docs, assurance and its delegation tests, the publication gate's negative controls, packaging, and benchmark input checks. |
| `probity-guardrails` | Hostile and permitted coding-agent workflow commands. |
| `security-hotspots` | Prompt-inert deterministic EPI model and exact review-queue baseline. |
| `rc3-project-new` | Create a bounded project without overwriting files. |
| `rc3-mini-os-check` | Check the Mini Determinator project in one command. |
| `rc3-spec-canonical` | Equivalent source produces identical typed AST bytes. |
| `rc3-composition-diagnostics` | Composition blockers are complete and deterministic. |
| `rc3-mini-os-replay` | Private-workspace coordination is completion-order invariant. |
| `rc3-mini-os-conflict` | Conflicting merges reject without authoritative change. |
| `rc3-temporal-modes` | Finite execution and unbounded obligations stay distinct. |
| `rc3-formal-tools` | Formal output binds exact claim and tool family. |
| `rc3-formal-fail-closed` | Hostile outcomes block and models replay before refutation. |
| `rc3-input-inert` | Shell, traversal, environment, and instruction text stays inert. |
| `rc3-derived-views` | Graph and explanation views are deterministic and diagnostic only. |
| `rc3-generated-drift` | Generated source and manifests reproduce and detect drift. |
| `rc3-resource-envelopes` | Deep parsing, huge horizons, and oversized exports stop within fixed limits. |
| `rc3-process-boundary` | Timeout, solver names, and execution bind to exact checked bytes. |
| `rc3-cli-json-contract` | Valid and invalid projects return versioned deterministic JSON. |
| `rc3-package-binary-inventory` | Every declared binary receives one unique checked archive. |
| `symbolic-per-case-checks` | Solver queries per rule case encode what the library runs, replay every counterexample, stay inconclusive on `unknown` or a disagreement, and run on the pinned CVC5 and Z3 when `ZENO_FCIS_CVC5` and `ZENO_FCIS_Z3` are set; otherwise that part prints its documented SKIP line. The runner's own tests check that SKIP line and that every declared pinned test must pass. |

The permanent adopter-acceptance workflow runs the checker and complete
portfolio from the exact source revision.

The benchmark smoke gate uses the development profile and Criterion's `--test`
mode. It checks commitment parity and persistent-map test inputs across enabled
backends; it does not qualify performance.
