```json
{
  "name": "contract-review",
  "title": "Review decisions and mutations",
  "synopsis": "zeno-fcis contract review DIR --format json",
  "engine": "Bind the contract Authority, enumerate full domain or boundary inputs, then evaluate a finite mutant catalog.",
  "use_when": [
    "review contract behaviour",
    "compare decision examples",
    "find a law refusal",
    "inspect mutations"
  ],
  "not_for": [
    "complete intent recovery",
    "reachability proof",
    "equivalence outside a boundary set"
  ],
  "inputs": [
    "DIR/project.zeno: .zeno; DIR/v2/policy.json: zeno-fcis/template-declarative-policy/2; optional tests/decision-examples.txt. File caps: unknown."
  ],
  "outputs": [
    "stdout zeno-fcis/contract-review/2: read summary, input construction, findings, refusals and mutants; --out writes packet"
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "reviewed": "No reported findings on the listed inputs; inspect mutants and run scenario.",
    "disagreement": "Owner examples differ; revise rules or labels, then review.",
    "law-refusal": "A law refuses a law-consistent pre-state; inspect its witness and revise.",
    "contract-invalid": "Repair the named contract entry.",
    "inconclusive": "Gaps section stopped; no gap claim."
  },
  "limits": {
    "max_tuples_default": 1048576,
    "rows_written_cap": 131072,
    "other_caps": "unknown"
  },
  "refusals": {
    "contract-invalid": "Repair the named declaration or policy entry; generate contract DIR to diagnose."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract review examples/guide-counter --format json",
      "exit": 0,
      "expected": {
        "schema": "zeno-fcis/contract-review/2",
        "summary.findings": 0
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract profile examples/guide-counter",
    "exit": 0,
    "expected": {
      "schema": "zeno-fcis/contract-profile/2"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25",
    "engine": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25",
    "inputs": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25",
    "outputs": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25",
    "exit_codes": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25",
    "verdicts": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25",
    "limits": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25",
    "refusals": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25"
  },
  "commands": [
    "contract review",
    "contract profile"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Review decisions and mutations

## Use when

review contract behaviour; compare decision examples; find a law refusal; inspect mutations.

## Not for

complete intent recovery; reachability proof; equivalence outside a boundary set.

## Inputs

DIR/project.zeno: .zeno; DIR/v2/policy.json: zeno-fcis/template-declarative-policy/2; optional tests/decision-examples.txt. File caps: unknown.

Engine: Bind the contract Authority, enumerate full domain or boundary inputs, then evaluate a finite mutant catalog.

Synopsis: "zeno-fcis contract review DIR --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract review examples/guide-counter --format json
```

Expected: exit 0; {"schema": "zeno-fcis/contract-review/2", "summary.findings": 0}.

## Verdicts

`reviewed`: No reported findings on the listed inputs; inspect mutants and run scenario.

`disagreement`: Owner examples differ; revise rules or labels, then review.

`law-refusal`: A law refuses a law-consistent pre-state; inspect its witness and revise.

`contract-invalid`: Repair the named contract entry.

`inconclusive`: Gaps section stopped; no gap claim.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/contract-review/2: read summary, input construction, findings, refusals and mutants; --out writes packet"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"max_tuples_default": 1048576, "other_caps": "unknown", "rows_written_cap": 131072}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"contract-invalid": "Repair the named declaration or policy entry; generate contract DIR to diagnose."}

## Worked example

```sh
zeno-fcis contract profile examples/guide-counter
```

Expected: exit 0; {"schema": "zeno-fcis/contract-profile/2"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25", "exit_codes": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25", "inputs": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25", "limits": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25", "outputs": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25", "refusals": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25", "synopsis": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25", "verdicts": "docs/CLI_REFERENCE.md:1446-1570; docs/CONTRACT_JUDGES.md:1-25"}

Run `zeno-fcis guide find "review contract behaviour"` for related cards.
