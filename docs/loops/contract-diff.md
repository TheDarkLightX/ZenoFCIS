```json
{
  "name": "contract-diff",
  "title": "Classify the change between two contracts",
  "synopsis": "zeno-fcis contract diff OLD_DIR NEW_DIR --format json",
  "engine": "Generates each contract's current version as generate contract does, replaying every adoption receipt, then decides exactly one kind by comparing canonical policies, schemas and generator structure in fixed precedence: identical, program-successor, rename, layout-change, rule-change, unrelated.",
  "use_when": [
    "what kind of change is this",
    "which admission path does a contract change need",
    "compare two contract versions",
    "is this a rename, a rule change or a layout change"
  ],
  "not_for": [
    "deciding whether a rule change preserves decisions (run the kind's admission path)",
    "applying the change (use contract evolve or contract adopt)"
  ],
  "inputs": [
    "OLD_DIR and NEW_DIR: contract or application directories, each with project.zeno, v2/policy.json and any adoptions. Writes nothing. Output depends only on the two contracts' files, not their location."
  ],
  "outputs": [
    "stdout zeno-fcis/contract-diff/1: kind, precedence (the six kinds in order), parts changed (names, state, interface, channels, reasons, genesis, laws, program, limits), changes and notes as item:id:change, admission (needed, refused); human output is the same account one line per change"
  ],
  "exit_codes": {
    "0": "classified, whatever the kind, including unrelated",
    "1": "a contract is invalid, as generate contract would refuse it; reported with side old or new",
    "3": "a file cannot be read; reported with side",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "identical": "Canonical policies are byte-identical; no admission needed.",
    "program-successor": "Only the decision program and Step limit differ; admit with a transform receipt and contract adopt.",
    "rename": "Only names differ; contract evolve without --migration records the exact rename.",
    "layout-change": "State or payload layout differs with protected roots unchanged; contract evolve --migration.",
    "rule-change": "Schemas and channels identical, laws/cases/reasons/genesis differ; contract evolve (behaviour change) or --migration when decisions are preserved.",
    "unrelated": "No single admission path: split the change into versions."
  },
  "limits": {
    "other_caps": "unknown; adoption receipts must replay with this checker"
  },
  "refusals": {
    "contract-invalid": "Fix the named side so generate contract accepts it.",
    "receipt-does-not-replay": "Run contract refresh-receipts on that side first when its receipts were written by another build."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract diff crates/zeno-fcis-cli/tests/fixtures/spend-approval crates/zeno-fcis-cli/tests/fixtures/spend-approval --format json",
      "exit": 0,
      "expected": {
        "kind": "identical"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract diff crates/zeno-fcis-cli/tests/fixtures/spend-approval crates/zeno-fcis-cli/tests/fixtures/escrow --format json",
    "exit": 0,
    "expected": {
      "kind": "unrelated"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs",
    "engine": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs",
    "inputs": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs",
    "outputs": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs",
    "exit_codes": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs",
    "verdicts": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs",
    "limits": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs",
    "refusals": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs"
  },
  "commands": [
    "contract diff"
  ],
  "pitfalls": "Exit 0 means classified, not approved: unrelated also exits 0. The classifier runs no decision; it never shows that a rule change preserves decisions. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Classify the change between two contracts

## Use when

what kind of change is this; which admission path does a contract change need; compare two contract versions; is this a rename, a rule change or a layout change.

## Not for

deciding whether a rule change preserves decisions (run the kind's admission path); applying the change (use contract evolve or contract adopt).

## Inputs

OLD_DIR and NEW_DIR: contract or application directories, each with project.zeno, v2/policy.json and any adoptions. Writes nothing. Output depends only on the two contracts' files, not their location.

Engine: Generates each contract's current version as generate contract does, replaying every adoption receipt, then decides exactly one kind by comparing canonical policies, schemas and generator structure in fixed precedence: identical, program-successor, rename, layout-change, rule-change, unrelated.

Synopsis: "zeno-fcis contract diff OLD_DIR NEW_DIR --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract diff crates/zeno-fcis-cli/tests/fixtures/spend-approval crates/zeno-fcis-cli/tests/fixtures/spend-approval --format json
```

Expected: exit 0; {"kind": "identical"}.

## Verdicts

`identical`: Canonical policies are byte-identical; no admission needed.

`program-successor`: Only the decision program and Step limit differ; admit with a transform receipt and contract adopt.

`rename`: Only names differ; contract evolve without --migration records the exact rename.

`layout-change`: State or payload layout differs with protected roots unchanged; contract evolve --migration.

`rule-change`: Schemas and channels identical, laws/cases/reasons/genesis differ; contract evolve (behaviour change) or --migration when decisions are preserved.

`unrelated`: No single admission path: split the change into versions.

Exit codes: {"0": "classified, whatever the kind, including unrelated", "1": "a contract is invalid, as generate contract would refuse it; reported with side old or new", "3": "a file cannot be read; reported with side", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/contract-diff/1: kind, precedence (the six kinds in order), parts changed (names, state, interface, channels, reasons, genesis, laws, program, limits), changes and notes as item:id:change, admission (needed, refused); human output is the same account one line per change"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"other_caps": "unknown; adoption receipts must replay with this checker"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Exit 0 means classified, not approved: unrelated also exits 0. The classifier runs no decision; it never shows that a rule change preserves decisions. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"contract-invalid": "Fix the named side so generate contract accepts it.", "receipt-does-not-replay": "Run contract refresh-receipts on that side first when its receipts were written by another build."}

## Worked example

```sh
zeno-fcis contract diff crates/zeno-fcis-cli/tests/fixtures/spend-approval crates/zeno-fcis-cli/tests/fixtures/escrow --format json
```

Expected: exit 0; {"kind": "unrelated"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs", "exit_codes": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs", "inputs": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs", "limits": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs", "outputs": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs", "refusals": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs", "synopsis": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs", "verdicts": "docs/CLI_REFERENCE.md:2567-2685; crates/zeno-fcis-cli/tests/fixtures/contract-diff/pairs.json; crates/zeno-fcis-cli/tests/contract_diff.rs"}

Run `zeno-fcis guide find "what kind of change is this"` for related cards.
