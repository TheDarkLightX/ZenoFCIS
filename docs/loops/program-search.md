```json
{
  "name": "program-search",
  "title": "Search for a smaller equal scalar program",
  "synopsis": "zeno-fcis optimize --program PROGRAM",
  "engine": "E-graph equality saturation: union-find, hash-consed nodes, congruence rebuilding and exact per-class tables over minimal support; transform checker accepts results.",
  "use_when": [
    "search for a smaller program",
    "optimize an existing decision graph",
    "share redundant scalar expressions"
  ],
  "not_for": [
    "proposer-driven sessions",
    "global minimum proof",
    "automatic contract adoption"
  ],
  "inputs": [
    "Canonical finite scalar program <= 65536 bytes, from contract export-program or loop encode; optional zeno-fcis/optimize-strategy/1 <= 65536 bytes and supplied candidates."
  ],
  "outputs": [
    "stdout zeno-fcis/optimize-result/1; read status, candidate, cost and receipt. Optional candidate-out and receipt files."
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "improved": "A smaller candidate passed full-domain transform checking; replay its receipt.",
    "no-checked-improvement": "Exit 2; original retained, no smaller checked result found.",
    "inconclusive": "Domain exceeds --max-input-tuples, or a proposer run stopped before completion with no accepted candidate. Read cause/stop and next_step; no completed-search claim.",
    "invalid-strategy": "Repair the closed strategy document or profile.",
    "output-exists": "Choose fresh candidate/receipt paths; nothing was searched.",
    "io-error": "Repair regular input files or output access and retry.",
    "refused": "Unsupported input/profile; inspect reason and use supported canonical bytes.",
    "unsupported-original": "Exit 2; the proposer cannot represent the named original opcode. No checked candidate was accepted. Read unsupported_node and next_step; increasing search limits does not add opcode support."
  },
  "limits": {
    "minimal_support_table_tuples_cap": 65536,
    "table_evaluations_cap": 268435456,
    "table_memory_mib": 64,
    "default_strategy_count": 3,
    "max_input_tuples_default": 100000000,
    "step_limit_default": 256,
    "supplied_candidates_cap": 8,
    "max_enodes_default": 20000,
    "max_classes_default": 10000,
    "max_extraction_rounds_default": 8,
    "max_rewrites_per_round_default": 10000,
    "max_work_millions_default": 400,
    "strategy_limit_fields_range": [
      1,
      100000
    ],
    "other_caps": "unknown"
  },
  "refusals": {
    "program-encoding": "Export canonical program bytes; policy.zcve is not a standalone program.",
    "strategy-unavailable": "Use the supported profile and closed strategy vocabulary.",
    "unsupported-original-node": "Use transform with a supplied candidate checked against the original, or request opcode support; preserve the original domain and semantics."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis optimize --program crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve",
      "exit": 0,
      "expected": {
        "status": "improved",
        "schema": "zeno-fcis/optimize-result/1"
      },
      "contains": []
    },
    {
      "command": "zeno-fcis contract export-program examples/arithmetic-fee --out \"$WORK/fee.zcve\"",
      "exit": 0,
      "expected": {},
      "contains": [
        "wrote the decision program"
      ]
    },
    {
      "command": "zeno-fcis optimize --program \"$WORK/fee.zcve\"",
      "exit": 2,
      "expected": {
        "status": "unsupported-original",
        "detail.unsupported_node.index": 4,
        "detail.unsupported_node.opcode": "MulDivCeil"
      },
      "contains": [
        "next: Use transform"
      ]
    }
  ],
  "example": {
    "command": "zeno-fcis transform check --original crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --candidate crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve",
    "exit": 0,
    "expected": {
      "status": "equivalent"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs",
    "engine": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs",
    "inputs": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs",
    "outputs": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs",
    "exit_codes": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs",
    "verdicts": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs",
    "limits": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs",
    "refusals": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs"
  },
  "commands": [
    "optimize"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Search for a smaller equal scalar program

## Use when

search for a smaller program; optimize an existing decision graph; share redundant scalar expressions.

## Not for

proposer-driven sessions; global minimum proof; automatic contract adoption.

## Inputs

Canonical finite scalar program <= 65536 bytes, from contract export-program or loop encode; optional zeno-fcis/optimize-strategy/1 <= 65536 bytes and supplied candidates.

Engine: E-graph equality saturation: union-find, hash-consed nodes, congruence rebuilding and exact per-class tables over minimal support; transform checker accepts results.

Synopsis: "zeno-fcis optimize --program PROGRAM"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis optimize --program crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve
```

Expected: exit 0; {"schema": "zeno-fcis/optimize-result/1", "status": "improved"}.

```sh
zeno-fcis contract export-program examples/arithmetic-fee --out "$WORK/fee.zcve"
```

Expected: exit 0; ["wrote the decision program"].

```sh
zeno-fcis optimize --program "$WORK/fee.zcve"
```

Expected: exit 2; {"detail.unsupported_node.index": 4, "detail.unsupported_node.opcode": "MulDivCeil", "status": "unsupported-original"}.

## Verdicts

`unsupported-original`: Exit 2; the proposer cannot represent the named original opcode. No checked candidate was accepted. Read unsupported_node and next_step; increasing search limits does not add opcode support.

`improved`: A smaller candidate passed full-domain transform checking; replay its receipt.

`no-checked-improvement`: Exit 2; original retained, no smaller checked result found.

`inconclusive`: Domain exceeds --max-input-tuples, or a proposer run stopped before completion with no accepted candidate. Read cause/stop and next_step; no completed-search claim.

`invalid-strategy`: Repair the closed strategy document or profile.

`output-exists`: Choose fresh candidate/receipt paths; nothing was searched.

`io-error`: Repair regular input files or output access.

`refused`: Unsupported input/profile; inspect reason and canonical bytes.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/optimize-result/1; read status, candidate, cost and receipt. Optional candidate-out and receipt files."]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"default_strategy_count": 3, "max_classes_default": 10000, "max_enodes_default": 20000, "max_extraction_rounds_default": 8, "max_input_tuples_default": 100000000, "max_rewrites_per_round_default": 10000, "max_work_millions_default": 400, "minimal_support_table_tuples_cap": 65536, "other_caps": "unknown", "step_limit_default": 256, "strategy_limit_fields_range": [1, 100000], "supplied_candidates_cap": 8, "table_evaluations_cap": 268435456, "table_memory_mib": 64}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"program-encoding": "Export canonical program bytes; policy.zcve is not a standalone program.", "strategy-unavailable": "Use the supported profile and closed strategy vocabulary.", "unsupported-original-node": "Use transform with a supplied candidate checked against the original, or request opcode support; preserve the original domain and semantics."}

## Worked example

```sh
zeno-fcis transform check --original crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --candidate crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve
```

Expected: exit 0; {"status": "equivalent"}.

## See also

**Background.** Ross Tate, Michael Stepp, Zachary Tatlock and Sorin Lerner,
“Equality Saturation: A New Approach to Optimization,” ACM POPL, 2009, is
background for the equality-based proposal search.

**What is new here.** This release separates unsupported proposal instructions,
incomplete search and independently checked improvements, and preserves work
and partial artifact publication diagnostics. The comparative assessment is
**prior-art search pending**.

**Scope.** Only the independent equivalence check and receipt replay qualify
a candidate over its declared finite domain. Proposal search and file creation
provide no decision or publication authority.


Sources: {"engine": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs", "exit_codes": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs", "inputs": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs", "limits": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs", "outputs": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs", "refusals": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs", "synopsis": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs", "verdicts": "docs/CLI_REFERENCE.md#checked-optimizer; docs/CLI_REFERENCE.md#contract-program-export; crates/zeno-fcis-cli/src/optimize_command.rs"}

Run `zeno-fcis guide find "search for a smaller program"` for related cards.
