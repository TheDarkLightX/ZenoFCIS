```json
{
  "name": "transform",
  "title": "Check equivalence and replay its evidence",
  "synopsis": [
    "zeno-fcis transform check --original ORIGINAL --candidate CANDIDATE",
    "zeno-fcis transform replay --receipt RECEIPT --original ORIGINAL --candidate CANDIDATE"
  ],
  "engine": "Full declared-domain comparison under eager scalar semantics; byte-for-byte receipt replay. Certificate routes independently check symbolic pieces.",
  "use_when": [
    "check one replacement",
    "replay equivalence evidence",
    "verify supplied scalar certificates"
  ],
  "not_for": [
    "searching for a program",
    "equal Step usage unless explicitly checked",
    "runtime adoption"
  ],
  "inputs": [
    "Canonical programs <= 65536 bytes; transform receipt from check. Independent certificate routes additionally require pinned tools, checker manifest and certificate directory."
  ],
  "outputs": [
    "stdout zeno-fcis/transform-result/1; read status and detail input/original/candidate observations, receipt and usage as present. --receipt writes a new receipt."
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "equivalent": "Outputs/failures agree on all declared inputs; replay the receipt and separately decide usage/adoption.",
    "counterexample": "A distinguishing input was replayed; inspect detail.input, detail.original and detail.candidate (symbolic mode uses detail.replayed).",
    "replayed": "The checker recomputed the receipt byte for byte.",
    "inconclusive": "No equivalence established; inspect the bound.",
    "refused": "Repair admission or bindings; this is not a distinguishing input."
  },
  "limits": {
    "program_bytes": 65536,
    "max_input_tuples_default": 100000000,
    "step_limit_default": 256,
    "other_caps": "unknown"
  },
  "refusals": {
    "original-not-admitted": "Supply canonical program bytes from export-program, not policy.zcve."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis transform check --original crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --candidate crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --receipt \"$WORK/equivalence.json\"",
      "exit": 0,
      "expected": {
        "status": "equivalent"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis transform replay --receipt \"$WORK/equivalence.json\" --original crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --candidate crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve",
    "exit": 0,
    "expected": {
      "status": "replayed"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351",
    "engine": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351",
    "inputs": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351",
    "outputs": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351",
    "exit_codes": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351",
    "verdicts": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351",
    "limits": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351",
    "refusals": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351"
  },
  "commands": [
    "transform",
    "transform check",
    "transform replay",
    "transform check-certificates",
    "transform replay-certificates"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Check equivalence and replay its evidence

## Use when

check one replacement; replay equivalence evidence; verify supplied scalar certificates.

## Not for

searching for a program; equal Step usage unless explicitly checked; runtime adoption.

## Inputs

Canonical programs <= 65536 bytes; transform receipt from check. Independent certificate routes additionally require pinned tools, checker manifest and certificate directory.

Engine: Full declared-domain comparison under eager scalar semantics; byte-for-byte receipt replay. Certificate routes independently check symbolic pieces.

Synopsis: ["zeno-fcis transform check --original ORIGINAL --candidate CANDIDATE", "zeno-fcis transform replay --receipt RECEIPT --original ORIGINAL --candidate CANDIDATE"]

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis transform check --original crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --candidate crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --receipt "$WORK/equivalence.json"
```

Expected: exit 0; {"status": "equivalent"}.

## Verdicts

`equivalent`: Outputs/failures agree on all declared inputs; replay the receipt and separately decide usage/adoption.

`counterexample`: A distinguishing input was replayed; inspect detail.input, detail.original and detail.candidate (symbolic mode uses detail.replayed).

`replayed`: The checker recomputed the receipt byte for byte.

`inconclusive`: No equivalence established; inspect the bound.

`refused`: Repair admission or bindings; this is not a distinguishing input.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/transform-result/1; read status and detail input/original/candidate observations, receipt and usage as present. --receipt writes a new receipt."]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"max_input_tuples_default": 100000000, "other_caps": "unknown", "program_bytes": 65536, "step_limit_default": 256}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"original-not-admitted": "Supply canonical program bytes from export-program, not policy.zcve."}

## Worked example

```sh
zeno-fcis transform replay --receipt "$WORK/equivalence.json" --original crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --candidate crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve
```

Expected: exit 0; {"status": "replayed"}.

## See also

Background: Amir Pnueli, Michael Siegel and Eli Singerman, “Translation Validation”, TACAS, 1998. This route checks supplied finite programs over their declared domains; it does not prove an arbitrary optimizer.

Sources: {"engine": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351", "exit_codes": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351", "inputs": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351", "limits": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351", "outputs": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351", "refusals": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351", "synopsis": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351", "verdicts": "docs/CLI_REFERENCE.md:427-481; docs/CLI_REFERENCE.md:311-351"}

Run `zeno-fcis guide find "check one replacement"` for related cards.
