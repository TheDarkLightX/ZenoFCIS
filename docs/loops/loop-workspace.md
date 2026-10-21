```json
{
  "name": "loop-workspace",
  "title": "Drive checked proposals in a durable session",
  "synopsis": [
    "zeno-fcis loop open --original PROGRAM --session DIR",
    "zeno-fcis loop candidate --session DIR --candidate PROGRAM",
    "zeno-fcis loop run --session DIR --proposer local",
    "zeno-fcis loop resume --session DIR",
    "zeno-fcis loop encode --program JSON --out PROGRAM"
  ],
  "engine": "Durable proposal ledger with full-domain transform checking, typed feedback and replayed incumbents.",
  "use_when": [
    "propose and revise a program",
    "drive a checked optimization loop",
    "resume a candidate workspace",
    "encode canonical program bytes"
  ],
  "not_for": [
    "hosted execution in this build",
    "global optimality",
    "turning provenance into acceptance"
  ],
  "inputs": [
    "Canonical finite scalar PROGRAM <= 65536 bytes; encode JSON vocabulary inputs/outputs/nodes/roots; session is created by open. Other file caps: unknown."
  ],
  "outputs": [
    "zeno-fcis/transform-loop/1; read status, detail, accounting and report; session ledger, receipts, witnesses and artifacts"
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "opened": "Request admitted; submit candidate or run proposer.",
    "equivalent": "Full-domain checker accepted; inspect selection and actual usage.",
    "different": "Counterexample retained; revise candidate against all witnesses.",
    "inconclusive": "No new accepted incumbent; inspect reason and remaining work, then resume.",
    "resume-refused": "No trusted incumbent; repair integrity or reopen from original."
  },
  "limits": {
    "attempts_cap": 8,
    "checks_cap": 8,
    "model_calls_cap": 4,
    "deadline_ms_cap": 20000,
    "worker_ms_cap": 25000,
    "search_stage_cpu_ms": 5000,
    "check_stage_cpu_ms": 2000,
    "memory_mib": 64,
    "minimum_check_work": 1000000,
    "max_check_work": 268435456,
    "other_defaults": "unknown"
  },
  "refusals": {
    "work-policy-changed": "Open a fresh session with this checker; do not rewrite request identity.",
    "work-infeasible": "Choose a smaller declared domain or supported checker budget.",
    "output-exists": "Use a new output file for encode."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis loop open --original crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --session \"$WORK/session\"",
      "exit": 0,
      "expected": {
        "status": "opened"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis loop resume --session \"$WORK/session\"",
    "exit": 0,
    "expected": {},
    "contains": [
      "zeno-fcis/transform-loop/1"
    ]
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:961-1145",
    "engine": "docs/CLI_REFERENCE.md:961-1145",
    "inputs": "docs/CLI_REFERENCE.md:961-1145",
    "outputs": "docs/CLI_REFERENCE.md:961-1145",
    "exit_codes": "docs/CLI_REFERENCE.md:961-1145",
    "verdicts": "docs/CLI_REFERENCE.md:961-1145",
    "limits": "docs/CLI_REFERENCE.md:961-1145",
    "refusals": "docs/CLI_REFERENCE.md:961-1145"
  },
  "commands": [
    "loop",
    "loop open",
    "loop candidate",
    "loop run",
    "loop resume",
    "loop encode"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Drive checked proposals in a durable session

## Use when

propose and revise a program; drive a checked optimization loop; resume a candidate workspace; encode canonical program bytes.

## Not for

hosted execution in this build; global optimality; turning provenance into acceptance.

## Inputs

Canonical finite scalar PROGRAM <= 65536 bytes; encode JSON vocabulary inputs/outputs/nodes/roots; session is created by open. Other file caps: unknown.

Engine: Durable proposal ledger with full-domain transform checking, typed feedback and replayed incumbents.

Synopsis: ["zeno-fcis loop open --original PROGRAM --session DIR", "zeno-fcis loop candidate --session DIR --candidate PROGRAM", "zeno-fcis loop run --session DIR --proposer local", "zeno-fcis loop resume --session DIR", "zeno-fcis loop encode --program JSON --out PROGRAM"]

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis loop open --original crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c2.original.zcve --session "$WORK/session"
```

Expected: exit 0; {"status": "opened"}.

## Verdicts

`opened`: Request admitted; submit candidate or run proposer.

`equivalent`: Full-domain checker accepted; inspect selection and actual usage.

`different`: Counterexample retained; revise candidate against all witnesses.

`inconclusive`: No new accepted incumbent; inspect reason and remaining work, then resume.

`resume-refused`: No trusted incumbent; repair integrity or reopen from original.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["zeno-fcis/transform-loop/1; read status, detail, accounting and report; session ledger, receipts, witnesses and artifacts"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"attempts_cap": 8, "check_stage_cpu_ms": 2000, "checks_cap": 8, "deadline_ms_cap": 20000, "max_check_work": 268435456, "memory_mib": 64, "minimum_check_work": 1000000, "model_calls_cap": 4, "other_defaults": "unknown", "search_stage_cpu_ms": 5000, "worker_ms_cap": 25000}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"output-exists": "Use a new output file for encode.", "work-infeasible": "Choose a smaller declared domain or supported checker budget.", "work-policy-changed": "Open a fresh session with this checker; do not rewrite request identity."}

## Worked example

```sh
zeno-fcis loop resume --session "$WORK/session"
```

Expected: exit 0; ["zeno-fcis/transform-loop/1"].

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:961-1145", "exit_codes": "docs/CLI_REFERENCE.md:961-1145", "inputs": "docs/CLI_REFERENCE.md:961-1145", "limits": "docs/CLI_REFERENCE.md:961-1145", "outputs": "docs/CLI_REFERENCE.md:961-1145", "refusals": "docs/CLI_REFERENCE.md:961-1145", "synopsis": "docs/CLI_REFERENCE.md:961-1145", "verdicts": "docs/CLI_REFERENCE.md:961-1145"}

Run `zeno-fcis guide find "propose and revise a program"` for related cards.
