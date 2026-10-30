```json
{
  "name": "contract-refresh-receipts",
  "title": "Rebind adoption receipts to this checker",
  "synopsis": "zeno-fcis contract refresh-receipts APP_DIR --format json",
  "engine": "Checks every adopted candidate again with the current transform checker under its receipt's limits, requires the new receipt to equal the old in every field but checker, then writes the receipts, the regenerated contract and last the rules.",
  "use_when": [
    "receipt does not replay after a new release",
    "another ZenoFCIS build wrote this receipt",
    "rebind adoption receipts",
    "generate contract names refresh-receipts"
  ],
  "not_for": [
    "adopting a new candidate (use contract adopt)",
    "a replaced contract's receipts under v2/evolutions (they must replay as they are)",
    "symbolic receipts (refused)"
  ],
  "inputs": [
    "APP_DIR: application or contract directory with project.zeno, v2/policy.json and v2/adoptions/*. Each adoption's candidate and receipt are read; nothing is written unless every pair is still equivalent and only the checker field changes."
  ],
  "outputs": [
    "stdout zeno-fcis/cli/1: status current (nothing to rebind) or refreshed with the list of rebound adoption ordinals, checker, artifacts and summary; or status error"
  ],
  "exit_codes": {
    "0": "current or refreshed",
    "1": "contract-invalid: a pair is no longer equivalent or anything but the checker would change; nothing written",
    "3": "contract-read-failed or a write failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "current": "Every receipt is already this checker's; nothing was written.",
    "refreshed": "The listed adoptions were checked again and rebound; rules were rewritten in the templates' layout.",
    "error": "Read error.code; a non-equivalent pair is a finding, not a layout problem."
  },
  "limits": {
    "candidate_bytes": 65536,
    "other_caps": "receipt limits are reused as recorded"
  },
  "refusals": {
    "contract-invalid": "The old receipt's limits and digests are binding; a candidate that is no longer equivalent cannot be refreshed."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract refresh-receipts crates/zeno-fcis-cli/tests/fixtures/spend-approval --format json",
      "exit": 0,
      "expected": {
        "status": "current"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract refresh-receipts crates/zeno-fcis-cli/templates/withdrawal-queue --format json",
    "exit": 0,
    "expected": {
      "status": "current"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs",
    "engine": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs",
    "inputs": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs",
    "outputs": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs",
    "exit_codes": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs",
    "verdicts": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs",
    "limits": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs",
    "refusals": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs"
  },
  "commands": [
    "contract refresh-receipts"
  ],
  "pitfalls": "It never reuses the old verdict. A rules file kept in its own layout by contract adopt is rewritten whole in the templates' layout here. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Rebind adoption receipts to this checker

## Use when

receipt does not replay after a new release; another ZenoFCIS build wrote this receipt; rebind adoption receipts; generate contract names refresh-receipts.

## Not for

adopting a new candidate (use contract adopt); a replaced contract's receipts under v2/evolutions (they must replay as they are); symbolic receipts (refused).

## Inputs

APP_DIR: application or contract directory with project.zeno, v2/policy.json and v2/adoptions/*. Each adoption's candidate and receipt are read; nothing is written unless every pair is still equivalent and only the checker field changes.

Engine: Checks every adopted candidate again with the current transform checker under its receipt's limits, requires the new receipt to equal the old in every field but checker, then writes the receipts, the regenerated contract and last the rules.

Synopsis: "zeno-fcis contract refresh-receipts APP_DIR --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract refresh-receipts crates/zeno-fcis-cli/tests/fixtures/spend-approval --format json
```

Expected: exit 0; {"status": "current"}.

## Verdicts

`current`: Every receipt is already this checker's; nothing was written.

`refreshed`: The listed adoptions were checked again and rebound; rules were rewritten in the templates' layout.

`error`: Read error.code; a non-equivalent pair is a finding, not a layout problem.

Exit codes: {"0": "current or refreshed", "1": "contract-invalid: a pair is no longer equivalent or anything but the checker would change; nothing written", "3": "contract-read-failed or a write failure", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/cli/1: status current (nothing to rebind) or refreshed with the list of rebound adoption ordinals, checker, artifacts and summary; or status error"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"candidate_bytes": 65536, "other_caps": "receipt limits are reused as recorded"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

It never reuses the old verdict. A rules file kept in its own layout by contract adopt is rewritten whole in the templates' layout here. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"contract-invalid": "The old receipt's limits and digests are binding; a candidate that is no longer equivalent cannot be refreshed."}

## Worked example

```sh
zeno-fcis contract refresh-receipts crates/zeno-fcis-cli/templates/withdrawal-queue --format json
```

Expected: exit 0; {"status": "current"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs", "exit_codes": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs", "inputs": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs", "limits": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs", "outputs": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs", "refusals": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs", "synopsis": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs", "verdicts": "docs/CLI_REFERENCE.md:2036-2235; crates/zeno-fcis-cli/tests/contract_adopt.rs; crates/zeno-fcis-cli/src/contract_adopt.rs"}

Run `zeno-fcis guide find "receipt does not replay after a new release"` for related cards.
