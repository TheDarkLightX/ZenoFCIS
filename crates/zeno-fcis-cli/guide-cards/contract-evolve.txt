```json
{
  "name": "contract-evolve",
  "title": "Replace an application's contract and record the lineage",
  "synopsis": "zeno-fcis contract evolve APP_DIR --to CONTRACT_DIR [--migration FILE [--shortcut FILE]...] --format json",
  "engine": "Classifies the change with contract diff, writes the owner-review account as v2/evolutions/N/review.txt, keeps the replaced contract and its adoptions under v2/evolutions/N/, regenerates the whole lineage from version 1 and rewrites v2/policy.json with an evolutions list binding the superseded policy and review digests.",
  "use_when": [
    "change the rules of a live application",
    "rename fields in an application",
    "record a behaviour change for store upgrade",
    "data migration of an application's layout"
  ],
  "not_for": [
    "a program successor (use contract adopt with a transform receipt)",
    "an identical or unrelated contract (refused)",
    "upgrading the store itself (the application's --upgrade follows)"
  ],
  "inputs": [
    "APP_DIR: application with project.zeno, v2/policy.json and any adoptions; written in place. --to: directory with project.zeno, v2/policy.json and optional tests/decision-examples.txt, with no adoptions or evolutions of its own. --migration FILE takes a layout change or a decision-preserving rule change by forward simulation; --shortcut FILE repeatable (see Data migrations and renames)."
  ],
  "outputs": [
    "stdout zeno-fcis/cli/1: status evolved with kind, evolution (ordinal, directory, from_version, version, path behaviour-change|rename|migration, superseded_policy_sha256, review_sha256, claims, review_path), review lines, artifacts and summary; or status error with code"
  ],
  "exit_codes": {
    "0": "evolved: files written in the documented order",
    "1": "contract-invalid or a kind no path takes (identical, unrelated, program-successor, layout change without --migration); nothing written",
    "3": "contract-read-failed or evolution-write-failed",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "evolved": "Lineage regenerated; the store still runs its version until the application's --upgrade admits the change on its current state.",
    "error": "Read error.code; a refused kind names the kind and the changed parts."
  },
  "limits": {
    "other_caps": "unknown; every replaced contract's receipts must replay as they are"
  },
  "refusals": {
    "kind-not-admitted": "identical or unrelated has no path; program-successor goes through contract adopt; a layout change needs --migration.",
    "evolution-directory-present": "An interrupted run left v2/evolutions/N; remove it (after checking version control) and rerun."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "cp -r crates/zeno-fcis-cli/tests/fixtures/escrow $WORK/app && zeno-fcis contract evolve $WORK/app --to crates/zeno-fcis-cli/tests/fixtures/escrow-dispute-30 --format json",
      "exit": 0,
      "expected": {
        "status": "evolved",
        "kind": "rule-change"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract evolve $WORK/app --to crates/zeno-fcis-cli/tests/fixtures/escrow-dispute-30 --format json",
    "exit": 1,
    "expected": {
      "status": "error"
    },
    "contains": [
      "identical"
    ]
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs",
    "engine": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs",
    "inputs": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs",
    "outputs": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs",
    "exit_codes": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs",
    "verdicts": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs",
    "limits": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs",
    "refusals": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs"
  },
  "commands": [
    "contract evolve"
  ],
  "pitfalls": "The review text and both contracts are digest-bound: neither can be edited afterwards. An interruption between project.zeno and the rules leaves them inconsistent; restore from version control. The second step of the worked example evolves the already evolved application to the same contract, which is identical and refused. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Replace an application's contract and record the lineage

## Use when

change the rules of a live application; rename fields in an application; record a behaviour change for store upgrade; data migration of an application's layout.

## Not for

a program successor (use contract adopt with a transform receipt); an identical or unrelated contract (refused); upgrading the store itself (the application's --upgrade follows).

## Inputs

APP_DIR: application with project.zeno, v2/policy.json and any adoptions; written in place. --to: directory with project.zeno, v2/policy.json and optional tests/decision-examples.txt, with no adoptions or evolutions of its own. --migration FILE takes a layout change or a decision-preserving rule change by forward simulation; --shortcut FILE repeatable (see Data migrations and renames).

Engine: Classifies the change with contract diff, writes the owner-review account as v2/evolutions/N/review.txt, keeps the replaced contract and its adoptions under v2/evolutions/N/, regenerates the whole lineage from version 1 and rewrites v2/policy.json with an evolutions list binding the superseded policy and review digests.

Synopsis: "zeno-fcis contract evolve APP_DIR --to CONTRACT_DIR [--migration FILE [--shortcut FILE]...] --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
cp -r crates/zeno-fcis-cli/tests/fixtures/escrow $WORK/app && zeno-fcis contract evolve $WORK/app --to crates/zeno-fcis-cli/tests/fixtures/escrow-dispute-30 --format json
```

Expected: exit 0; {"kind": "rule-change", "status": "evolved"}.

## Verdicts

`evolved`: Lineage regenerated; the store still runs its version until the application's --upgrade admits the change on its current state.

`error`: Read error.code; a refused kind names the kind and the changed parts.

Exit codes: {"0": "evolved: files written in the documented order", "1": "contract-invalid or a kind no path takes (identical, unrelated, program-successor, layout change without --migration); nothing written", "3": "contract-read-failed or evolution-write-failed", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/cli/1: status evolved with kind, evolution (ordinal, directory, from_version, version, path behaviour-change|rename|migration, superseded_policy_sha256, review_sha256, claims, review_path), review lines, artifacts and summary; or status error with code"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"other_caps": "unknown; every replaced contract's receipts must replay as they are"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

The review text and both contracts are digest-bound: neither can be edited afterwards. An interruption between project.zeno and the rules leaves them inconsistent; restore from version control. The second step of the worked example evolves the already evolved application to the same contract, which is identical and refused. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"evolution-directory-present": "An interrupted run left v2/evolutions/N; remove it (after checking version control) and rerun.", "kind-not-admitted": "identical or unrelated has no path; program-successor goes through contract adopt; a layout change needs --migration."}

## Worked example

```sh
zeno-fcis contract evolve $WORK/app --to crates/zeno-fcis-cli/tests/fixtures/escrow-dispute-30 --format json
```

Expected: exit 1; {"status": "error"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs", "exit_codes": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs", "inputs": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs", "limits": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs", "outputs": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs", "refusals": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs", "synopsis": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs", "verdicts": "docs/CLI_REFERENCE.md:2236-2356; docs/CLI_REFERENCE.md:2357-2566; crates/zeno-fcis-cli/tests/contract_evolve.rs"}

Run `zeno-fcis guide find "change the rules of a live application"` for related cards.
