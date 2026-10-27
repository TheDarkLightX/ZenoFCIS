```json
{
  "name": "contract-distribution",
  "title": "Check finite random-outcome laws",
  "synopsis": "zeno-fcis contract distribution DIR --format json",
  "engine": "Exact finite fibers and rational weights, given uniform independent random bits independent of fixed inputs.",
  "use_when": [
    "check a probability distribution",
    "find modulo bias",
    "verify random payout weights"
  ],
  "not_for": [
    "claims about a real entropy source",
    "continuous distributions",
    "unmodeled strategic incentives"
  ],
  "inputs": [
    "Contract DIR with declared finite random inputs and distribution laws. Encodings: .zeno and zeno-fcis/template-declarative-policy/2. File caps: unknown."
  ],
  "outputs": [
    "stdout zeno-fcis/distribution-laws/1: read status, premise, measured, limits and result witness; optional packet output"
  ],
  "exit_codes": {
    "0": "checked",
    "1": "refuted",
    "2": "UNKNOWN",
    "3": "file failure",
    "64": "usage"
  },
  "verdicts": {
    "checked": "Declared finite domain only; advisory, authority none. Under stated uniformity premise; retain packet.",
    "refuted": "Exact excess-mass witness; repair the mapping or law and rerun.",
    "UNKNOWN": "Budget/fragment stopped; no probability claim."
  },
  "limits": {
    "max_tuples_default": 1048576,
    "max_work_default": 16777216,
    "other_caps": "unknown"
  },
  "refusals": {
    "unsupported": "Read reason; use an admitted finite random fragment."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract distribution examples/distribution-laws/modulo-bias --format json",
      "exit": 1,
      "expected": {
        "status": "refuted",
        "schema": "zeno-fcis/distribution-laws/1"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract distribution examples/distribution-laws/modulo-bias --max-work 1 --format json",
    "exit": 2,
    "expected": {
      "status": "UNKNOWN"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "zeno-fcis contract distribution --help; observed repository example",
    "engine": "zeno-fcis contract distribution --help; observed repository example",
    "inputs": "zeno-fcis contract distribution --help; observed repository example",
    "outputs": "zeno-fcis contract distribution --help; observed repository example",
    "exit_codes": "zeno-fcis contract distribution --help; observed repository example",
    "verdicts": "zeno-fcis contract distribution --help; observed repository example",
    "limits": "zeno-fcis contract distribution --help; observed repository example",
    "refusals": "zeno-fcis contract distribution --help; observed repository example"
  },
  "commands": [
    "contract distribution"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Check finite random-outcome laws

## Use when

check a probability distribution; find modulo bias; verify random payout weights.

## Not for

claims about a real entropy source; continuous distributions; unmodeled strategic incentives.

## Inputs

Contract DIR with declared finite random inputs and distribution laws. Encodings: .zeno and zeno-fcis/template-declarative-policy/2. File caps: unknown.

Engine: Exact finite fibers and rational weights, given uniform independent random bits independent of fixed inputs.

Synopsis: "zeno-fcis contract distribution DIR --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract distribution examples/distribution-laws/modulo-bias --format json
```

Expected: exit 1; {"schema": "zeno-fcis/distribution-laws/1", "status": "refuted"}.

## Verdicts

`checked`: Declared finite domain only; advisory, authority none. Under stated uniformity premise; retain packet.

`refuted`: Exact excess-mass witness; repair the mapping or law and rerun.

`UNKNOWN`: Budget/fragment stopped; no probability claim.

Exit codes: {"0": "checked", "1": "refuted", "2": "UNKNOWN", "3": "file failure", "64": "usage"}

Outputs: ["stdout zeno-fcis/distribution-laws/1: read status, premise, measured, limits and result witness; optional packet output"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"max_tuples_default": 1048576, "max_work_default": 16777216, "other_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"unsupported": "Read reason; use an admitted finite random fragment."}

## Worked example

```sh
zeno-fcis contract distribution examples/distribution-laws/modulo-bias --max-work 1 --format json
```

Expected: exit 2; {"status": "UNKNOWN"}.

## See also

Sources: {"engine": "zeno-fcis contract distribution --help; observed repository example", "exit_codes": "zeno-fcis contract distribution --help; observed repository example", "inputs": "zeno-fcis contract distribution --help; observed repository example", "limits": "zeno-fcis contract distribution --help; observed repository example", "outputs": "zeno-fcis contract distribution --help; observed repository example", "refusals": "zeno-fcis contract distribution --help; observed repository example", "synopsis": "zeno-fcis contract distribution --help; observed repository example", "verdicts": "zeno-fcis contract distribution --help; observed repository example"}

Run `zeno-fcis guide find "check a probability distribution"` for related cards.
