```json
{
  "name": "contract-review",
  "title": "Review decisions and mutations",
  "synopsis": "zeno-fcis contract review DIR --format json",
  "engine": "Bind the contract Authority, enumerate full domain or use symbolic law checking above the cap, report exhaustive scalar case coverage and evaluate a finite mutant catalog.",
  "use_when": [
    "review contract behaviour",
    "compare decision examples",
    "find a law refusal",
    "inspect mutations",
    "find cases that never decide"
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
    "stdout scalar zeno-fcis/contract-review/2 (or /4 with --gaps): read case_coverage, findings, summary, refusals and mutants; --out writes packet. Above the cap read case_coverage.status = not checked."
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or reported findings, including case-coverage",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "reviewed": "No reported findings on the listed inputs; inspect mutants and run scenario.",
    "disagreement": "Owner examples differ; revise rules or labels, then review.",
    "law-refusal": "A law refuses a law-consistent pre-state; inspect its witness and revise.",
    "contract-invalid": "Repair the named contract entry.",
    "inconclusive": "Gaps section stopped; no gap claim.",
    "case-coverage": "A case never decides, or selects only pre-states the state laws exclude; inspect its cause and witnesses. Exit 1, like other findings."
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
        "summary.findings": 0,
        "case_coverage.status": "checked"
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
    "synopsis": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139",
    "engine": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139",
    "inputs": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139",
    "outputs": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139",
    "exit_codes": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139",
    "verdicts": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139",
    "limits": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139",
    "refusals": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139"
  },
  "commands": [
    "contract review",
    "contract profile"
  ],
  "pitfalls": "Read the result scope before revising the input. Solver executable copies and working directories are claimed exclusively for each run; occupied names are skipped, with a limit of 128 creation attempts. Exhaustion or any other I/O refusal gives no solver evidence. No report grants runtime or publication authority."
}
```

# Review decisions and mutations

## Use when

review contract behaviour; compare decision examples; find a law refusal; inspect mutations; find cases that never decide.

## Not for

complete intent recovery; reachability proof; equivalence outside a boundary set.

## Inputs

DIR/project.zeno: .zeno; DIR/v2/policy.json: zeno-fcis/template-declarative-policy/2; optional tests/decision-examples.txt. File caps: unknown.

Engine: Bind the contract Authority, enumerate full domain or use symbolic law checking above the cap, report exhaustive scalar case coverage and evaluate a finite mutant catalog.

Synopsis: "zeno-fcis contract review DIR --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract review examples/guide-counter --format json
```

Expected: exit 0; {"schema": "zeno-fcis/contract-review/2", "summary.findings": 0, "case_coverage.status": "checked"}.

## Verdicts

`reviewed`: No reported findings on the listed inputs; inspect mutants and run scenario.

`disagreement`: Owner examples differ; revise rules or labels, then review.

`law-refusal`: A law refuses a law-consistent pre-state; inspect its witness and revise.

`case-coverage`: A case never decides, or selects only pre-states the state laws exclude; inspect its cause and witnesses. Exit 1.

`contract-invalid`: Repair the named contract entry.

`inconclusive`: Gaps section stopped; no gap claim.

Exit codes: {"0": "requested result completed", "1": "invalid specification or reported findings, including case-coverage", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["stdout scalar zeno-fcis/contract-review/2 (or /4 with --gaps): read case_coverage, findings, summary, refusals and mutants; --out writes packet. Above the cap read case_coverage.status = not checked."]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"max_tuples_default": 1048576, "other_caps": "unknown", "rows_written_cap": 131072}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. Solver executable copies and working directories are claimed exclusively for each run; occupied names are skipped, with a limit of 128 creation attempts. Exhaustion or any other I/O refusal gives no solver evidence. No report grants runtime or publication authority.

Refusals and remedies: {"contract-invalid": "Repair the named declaration or policy entry; generate contract DIR to diagnose."}

## Worked example

```sh
zeno-fcis contract profile examples/guide-counter
```

Expected: exit 0; {"schema": "zeno-fcis/contract-profile/2"}.

Full-domain scalar packets have `case_coverage.status: "checked"`,
`scope: "full-domain"`, `tuples`, and `cases[]` with `case`, `rule`, `when`,
`guard_true`, `decided`, `law_consistent`. Counts measure first-match case
selection before Authority admission. `case-never-decides` has cause `guard
never true` or `covered by earlier cases`; `covered_by[]` names each earlier
`case`, `when` and `witness.{ordinal,input}` (decimal strings in program
order). The weaker
`case-only-on-excluded-pre-states` has positive `decided`, zero
`law_consistent`, and `first.{ordinal,input,unsatisfied_state_law}`. Findings
include `text`, and count in `summary.case_findings` and `summary.findings`.
Above the enumeration cap, the symbolic path reports
`case_coverage.status: "not checked"`, `reason`, `cases: null`; it makes no
live/dead claim. Undefined guard evaluation also leaves coverage unchecked.

For example, inserting `status == 151 -> Reject 200` immediately after case
0 `status != 150` in dual approval yields `case-never-decides`, case 1,
`cause: "covered by earlier cases"`, with case 0's witness
`151 160 160 140 160`. Its text is:

```text
dead case: cases[1] `status == 151` decides no tuple: covered by earlier cases; cases[0] `status != 150` at 151 160 160 140 160
```

Lineage: rule subsumption, overlaps and missing-rule checks are established
decision-table analyses. A DMN bibliography could not be confirmed offline;
no unverified citation is asserted. This integration is exhaustive only over
declared finite scalar domains; prior-art search pending.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139", "exit_codes": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139", "inputs": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139", "limits": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139", "outputs": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139", "refusals": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139", "synopsis": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139", "verdicts": "docs/CLI_REFERENCE.md:1642-1795; docs/CONTRACT_JUDGES.md:86-139"}

Run `zeno-fcis guide find "review contract behaviour"` for related cards.

Background: The Open Group, *POSIX.1-2024*, `open()` and exclusive creation, 2024 ([specification](https://pubs.opengroup.org/onlinepubs/9799919799/functions/open.html)). This repair applies that established file-ownership mechanism; no new method is claimed. Scope: fresh shell scratch does not change solver admission, query semantics, evidence checks or Authority decisions.
