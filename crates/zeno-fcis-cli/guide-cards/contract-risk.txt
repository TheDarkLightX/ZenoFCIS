```json
{
  "name": "contract-risk",
  "title": "Count finite outcomes and bound accumulated rewards",
  "synopsis": "zeno-fcis contract risk DIR --reward 610 --steps 3 --format json",
  "engine": "Exact declared-domain counting and bounded min-plus/max-plus paths through the library Authority.",
  "use_when": [
    "count inputs satisfying a property",
    "count reachable states within K commands",
    "find best case and worst case accumulated cost"
  ],
  "not_for": [
    "unbounded horizons",
    "empirical risk probabilities",
    "runtime authority"
  ],
  "inputs": [
    "Contract DIR with finite scalar domains; --where, --states --where --steps K, or --reward ID --steps K."
  ],
  "outputs": [
    "zeno-fcis/contract-risk/1: status, claim, result, limits, measured work and replayable witnesses."
  ],
  "exit_codes": {
    "0": "checked",
    "2": "UNKNOWN",
    "3": "file failure",
    "64": "usage",
    "65": "malformed input"
  },
  "verdicts": {
    "checked": "Exact finite answer, authority none.",
    "UNKNOWN": "No exact claim; inspect reason and consumed work."
  },
  "limits": {
    "max_tuples_default": 1048576,
    "max_work_default": 16777216,
    "max_steps": 64
  },
  "refusals": {
    "unsupported": "Use the supported scalar finite fragment; no sampling fallback."
  },
  "cost": {
    "measured_units": "invocations, folds, relaxation, discovery and scalar witness copies",
    "example_wall_ms": "unknown"
  },
  "steps": [
    {
      "command": "zeno-fcis contract risk examples/quantitative-risk/swap-pool --where \"post.100.112 == 1\" --format json",
      "exit": 0,
      "expected": {
        "status": "checked",
        "schema": "zeno-fcis/contract-risk/1"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract risk examples/quantitative-risk/swap-pool --reward 610 --steps 3 --max-work 0 --format json",
    "exit": 2,
    "expected": {
      "status": "UNKNOWN"
    },
    "contains": []
  },
  "sources": {
    "engine": "docs/QUANTITATIVE_RISK.md; contract risk --help",
    "exit_codes": "zeno-fcis contract risk --help"
  },
  "commands": [
    "contract risk"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Count and bound finite risk

## Use when

Count declared inputs, count distinct reachable states within an inclusive horizon,
or bound accumulated rewards with replayable witnesses.

## Not for

Unbounded horizons, empirical risk probabilities or runtime authority.

## Inputs

Use a contract with finite scalar domains. Choose an input property, a state
property and horizon, or a declared reward ID and horizon. Run the displayed
commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract risk examples/quantitative-risk/swap-pool --where "post.100.112 == 1" --format json
```

Expected: exit 0, status checked, 36 of 120 declared inputs satisfy the property.
This includes declared inputs whose pre-states are unreachable from genesis.

## Verdicts

`checked` gives an exact finite answer for the stated scope. `UNKNOWN` gives
no exact claim; retain its reason and measured work. Inspect the complete
packet and replay its witnesses before revising the contract.

## Cost

The defaults allow 1,048,576 declared tuples and 16,777,216 charged work units;
the maximum horizon is 64. Work records invocations, folds, relaxation,
discovery and scalar witness copies. Wall time is not qualified by this card.

## Coordinate

Run Commons advise first, lease the exact loop subject, and record the command,
subject, verdict and evidence after checking. Release the lease when finished.
Advice and telemetry grant no checker authority.

## Pitfalls

Input counts and reachable-state counts use different domains. Reward extrema
include genesis and the empty run, and include runs of the requested length.
Uniform shares over a declared finite domain are not empirical probabilities.
Every result remains advisory and grants no runtime or publication authority.

## Worked example

```sh
zeno-fcis contract risk examples/quantitative-risk/swap-pool --reward 610 --steps 3 --max-work 0 --format json
```

Expected: exit 2 and status UNKNOWN because no work is available. The report
makes no exact cost claim. With the default work bound, the same reward query
has best 0 and worst 6, including a length-three witness.

## See also

[Quantitative risk](../QUANTITATIVE_RISK.md) describes the three questions,
exact arithmetic, examples and scope. Run `zeno-fcis contract risk --help`
for options and `zeno-fcis guide find "count reachable states"` for related cards.
The background references in the quantitative-risk guide remain subject to
its stated prior-art and bibliography verification limits.
