```json
{
  "name": "contract-scenario",
  "title": "Replay an intended run",
  "synopsis": "zeno-fcis contract scenario DIR --file FILE",
  "engine": "Replay ordered decision-example runs through the scalar or full-width numeric Authority; only passing runs receive mutation-kill credit.",
  "use_when": [
    "replay intended steps",
    "check a user journey",
    "test a scenario from intent"
  ],
  "not_for": [
    "proof on untested histories",
    "mutant equivalence"
  ],
  "inputs": [
    "Contract DIR; FILE: decision-example lines under # scenario NAME headers; default tests/decision-scenarios.txt. File cap: unknown."
  ],
  "outputs": [
    "stdout zeno-fcis/contract-scenario/2: read status, runs and mutation coverage; --out writes packet"
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage",
    "65": "malformed scenario or contract"
  },
  "verdicts": {
    "passed": "Only listed runs passed; inspect surviving mutants.",
    "failed": "First mismatch retained; revise rules or scenario.",
    "inconclusive": "No complete claim; inspect limits."
  },
  "limits": {
    "max_tuples_default": 1048576,
    "other_caps": "unknown"
  },
  "refusals": {
    "65": "Supply # scenario headers and valid decision-example lines; retry."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract scenario examples/guide-counter --file examples/guide-counter/tests/decision-scenarios.txt",
      "exit": 0,
      "expected": {
        "status": "passed"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract scenario examples/guide-counter --file examples/guide-counter/tests/decision-scenarios.txt",
    "exit": 0,
    "expected": {
      "status": "passed"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help",
    "engine": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help",
    "inputs": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help",
    "outputs": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help",
    "exit_codes": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help",
    "verdicts": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help",
    "limits": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help",
    "refusals": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help"
  },
  "commands": [
    "contract scenario"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Replay an intended run

## Use when

replay intended steps; check a user journey; test a scenario from intent.

## Not for

proof on untested histories; mutant equivalence.

## Inputs

Contract DIR; FILE: decision-example lines under # scenario NAME headers; default tests/decision-scenarios.txt. File cap: unknown.

Engine: Replay ordered decision-example runs through the scalar or full-width numeric Authority; only passing runs receive mutation-kill credit.

Synopsis: "zeno-fcis contract scenario DIR --file FILE"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract scenario examples/guide-counter --file examples/guide-counter/tests/decision-scenarios.txt
```

Expected: exit 0; {"status": "passed"}.

## Verdicts

`passed`: Only listed runs passed; inspect surviving mutants.

`failed`: First mismatch retained; revise rules or scenario.

`inconclusive`: No complete claim; inspect limits.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage", "65": "malformed scenario or contract"}

Outputs: ["stdout zeno-fcis/contract-scenario/2: read status, runs and mutation coverage; --out writes packet"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"max_tuples_default": 1048576, "other_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"65": "Supply # scenario headers and valid decision-example lines; retry."}

## Worked example

```sh
zeno-fcis contract scenario examples/guide-counter --file examples/guide-counter/tests/decision-scenarios.txt
```

Expected: exit 0; {"status": "passed"}.

## See also

Sources: {"engine": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help", "exit_codes": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help", "inputs": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help", "limits": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help", "outputs": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help", "refusals": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help", "synopsis": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help", "verdicts": "docs/CONTRACT_JUDGES.md:145-168; zeno-fcis contract scenario --help"}

Run `zeno-fcis guide find "replay intended steps"` for related cards.

Background: Glenford J. Myers, *The Art of Software Testing*, Wiley, 1979, describes comparing observed and expected results. This adapter reuses the existing numeric example grammar and Authority binding; no new testing method is claimed. Scope: decimal u128 values retain all 128 bits. Only the listed runs are checked; numeric steps use accept, reject or failure and channel/payload delivery expectations.
