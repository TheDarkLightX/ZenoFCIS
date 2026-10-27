```json
{
  "name": "contract-gaps",
  "title": "Find decisions the laws leave open",
  "synopsis": "zeno-fcis contract review DIR --gaps --format json",
  "engine": "Enumerate candidate outcomes and evaluate the declared laws, excluding generated law 991.",
  "use_when": [
    "find missing requirements",
    "do laws determine decisions",
    "find reject-everything loopholes",
    "judge specification gaps"
  ],
  "not_for": [
    "owner intent proof",
    "delivery-complete gap claims for unsupported contracts"
  ],
  "inputs": [
    "Contract DIR; optional --gaps-laws FILE is proposed .zeno laws. File caps: unknown."
  ],
  "outputs": [
    "stdout zeno-fcis/contract-review/4: read gaps, summary.gaps, determined tuples, admitted mutants and reject-everything verdict"
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "reviewed": "Gap table complete; determined is uniqueness under laws, not owner intent. Add/review a law and repeat.",
    "inconclusive": "Over tuple/work budget; no gap claim. Narrow declared model or choose an adequate supported budget.",
    "disagreement": "Proposed law contradicts actual decisions; inspect the witness."
  },
  "limits": {
    "max_tuples_default": 1048576,
    "max_work_default": 60000000,
    "other_caps": "unknown"
  },
  "refusals": {
    "work-over-limit": "Inspect reported work and select a supported --max-work or narrower declared model."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract review examples/guide-counter --gaps --format json",
      "exit": 0,
      "expected": {
        "schema": "zeno-fcis/contract-review/4"
      },
      "contains": []
    },
    {
      "command": "zeno-fcis contract review examples/dual-approval --gaps --gaps-laws examples/dual-approval/proposed-laws.txt --out \"$WORK/dual-review.json\" --format json",
      "exit": 0,
      "expected": {
        "summary.gaps.full.determined": 0,
        "summary.gaps.with_proposals.full.determined": 300,
        "summary.gaps.with_proposals.valid.determined": 220
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract review examples/guide-counter --gaps --max-work 1 --format json",
    "exit": 2,
    "expected": {
      "summary.gaps.status": "inconclusive"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213",
    "engine": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213",
    "inputs": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213",
    "outputs": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213",
    "exit_codes": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213",
    "verdicts": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213",
    "limits": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213",
    "refusals": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213"
  },
  "commands": [
    "contract review"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Find decisions the laws leave open

## Use when

find missing requirements; do laws determine decisions; find reject-everything loopholes; judge specification gaps.

## Not for

owner intent proof; delivery-complete gap claims for unsupported contracts.

## Inputs

Contract DIR; optional --gaps-laws FILE is proposed .zeno laws. File caps: unknown.

Engine: Enumerate candidate outcomes and evaluate the declared laws, excluding generated law 991.

Synopsis: "zeno-fcis contract review DIR --gaps --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract review examples/guide-counter --gaps --format json
```

Expected: exit 0; {"schema": "zeno-fcis/contract-review/4"}.

```sh
zeno-fcis contract review examples/dual-approval --gaps --gaps-laws examples/dual-approval/proposed-laws.txt --out "$WORK/dual-review.json" --format json
```

Expected: exit 0; {"summary.gaps.full.determined": 0, "summary.gaps.with_proposals.full.determined": 300, "summary.gaps.with_proposals.valid.determined": 220}.

## Verdicts

`reviewed`: Gap table complete; determined is uniqueness under laws, not owner intent. Add/review a law and repeat.

`inconclusive`: Over tuple/work budget; no gap claim. Narrow declared model or choose an adequate supported budget.

`disagreement`: Proposed law contradicts actual decisions; inspect the witness.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/contract-review/4: read gaps, summary.gaps, determined tuples, admitted mutants and reject-everything verdict"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"max_tuples_default": 1048576, "max_work_default": 60000000, "other_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"work-over-limit": "Inspect reported work and select a supported --max-work or narrower declared model."}

## Worked example

```sh
zeno-fcis contract review examples/guide-counter --gaps --max-work 1 --format json
```

Expected: exit 2; {"summary.gaps.status": "inconclusive"}.

## See also

**Background.** Edmund M. Clarke, E. Allen Emerson and A. Prasad Sistla,
“Automatic Verification of Finite-State Concurrent Systems Using Temporal
Logic Specifications,” ACM Transactions on Programming Languages and Systems
8(2), 1986, is background for exhaustive finite-state checking.

**What is new here.** This release shares a typed candidate decision space
between gap judgments and question options, with independently checked laws
and region-wide successor guidance. The comparative assessment is
**prior-art search pending**.

**Scope.** Determination means uniqueness under the declared laws over the
finite domain and resource bounds; it does not establish intended behavior
or authorize adoption.


Sources: {"engine": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213", "exit_codes": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213", "inputs": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213", "limits": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213", "outputs": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213", "refusals": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213", "synopsis": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213", "verdicts": "docs/CONTRACT_JUDGES.md:72-130; docs/CONTRACT_JUDGES.md:189-213"}

Run `zeno-fcis guide find "find missing requirements"` for related cards.
