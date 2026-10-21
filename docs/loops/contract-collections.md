```json
{
  "name": "contract-collections",
  "title": "Check keyed collections by data independence",
  "synopsis": "zeno-fcis contract collections DIR",
  "engine": "The library Authority decides every admissible finite view; the checker verifies alias merging, summary deltas and the collection premises.",
  "use_when": [
    "check contract collections",
    "validate keyed tables",
    "check bounded collection contracts"
  ],
  "not_for": [
    "establishing runtime framing from a finite view check",
    "treating ordinary scalar enumeration as a proof about stored maps"
  ],
  "inputs": [
    "DIR: project.zeno and v2/policy.json with key types, maps, named entries and summaries."
  ],
  "outputs": [
    "stdout zeno-fcis/collections-check/1 packet; checked, refuted or UNKNOWN; authority none."
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "checked": "Every admissible view satisfies the checked obligations, conditional on the stated premises including runtime framing.",
    "refuted": "Inspect the concrete view and failed obligation, revise the rules and rerun.",
    "UNKNOWN": "No acceptance: the declared view domain exceeds the tuple limit."
  },
  "limits": {
    "max_tuples": "--max-tuples defaults to1048576; map capacity is not enumerated."
  },
  "refusals": {
    "key-use": "Use equality between keys of the same type and access maps through named entries.",
    "view-limit": "Reduce the finite view domain or choose an explicitly reviewed tuple cap."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract collections examples/collections/token-balances",
      "exit": 0,
      "expected": {
        "status": "checked",
        "authority": "none"
      },
      "contains": []
    },
    {
      "command": "zeno-fcis contract collections examples/collections/token-balances --max-tuples 100",
      "exit": 2,
      "expected": {
        "status": "UNKNOWN"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract collections examples/collections/library",
    "exit": 0,
    "expected": {
      "status": "checked",
      "measured.admissible_views": 160
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/COLLECTIONS.md; crates/zeno-fcis-contract/src/collections.rs; zeno-fcis contract collections --help",
    "engine": "docs/COLLECTIONS.md; crates/zeno-fcis-contract/src/collections.rs; zeno-fcis contract collections --help",
    "inputs": "docs/COLLECTIONS.md; crates/zeno-fcis-contract/src/collections.rs; zeno-fcis contract collections --help",
    "outputs": "docs/COLLECTIONS.md; crates/zeno-fcis-contract/src/collections.rs; zeno-fcis contract collections --help",
    "exit_codes": "docs/COLLECTIONS.md; crates/zeno-fcis-contract/src/collections.rs; zeno-fcis contract collections --help",
    "verdicts": "docs/COLLECTIONS.md; crates/zeno-fcis-contract/src/collections.rs; zeno-fcis contract collections --help",
    "limits": "docs/COLLECTIONS.md; crates/zeno-fcis-contract/src/collections.rs; zeno-fcis contract collections --help",
    "refusals": "docs/COLLECTIONS.md; crates/zeno-fcis-contract/src/collections.rs; zeno-fcis contract collections --help"
  },
  "commands": [
    "contract collections"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Check keyed collections by data independence

## Use when

Check named-entry map rules, key aliasing, summaries and per-entry laws over every admissible view.

## Not for

Runtime framing remains a separate premise. Generated map applications follow separately.

## Inputs

A project with key types, maps, named entries and summaries, plus its v2/policy.json rules.

Engine: the library Authority decides each admissible view. The checker verifies alias merging, summary deltas and the stated premises.

Synopsis: `zeno-fcis contract collections DIR`.

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract collections examples/collections/token-balances
```

Expected: exit 0; {"status": "checked", "authority": "none"}.

```sh
zeno-fcis contract collections examples/collections/token-balances --max-tuples 100
```

Expected: exit 2; {"status": "UNKNOWN"}.

## Verdicts

`checked` covers the stated collection obligations under the documented premises, including runtime framing P4. `refuted` includes a concrete failed view to revise. `UNKNOWN` grants no acceptance; the declared view domain exceeded the cap.

The map capacity is not enumerated. The default view-domain cap is1,048,576 tuples.

## Cost

Measured work counts view tuples and admissible views. The token example checks28,672 tuples and2,304 admissible views. Telemetry is advisory observation.

## Coordinate

Run Commons `advise` first, lease `loop:<subject digest>`, and retain each checker verdict with its source and input evidence. Release the lease after the loop. Advice and telemetry grant no checker authority.

## Pitfalls

Keys support equality only. Use named entries for map access and declared summaries for aggregates. A finite view check requires the runtime to load the named entries, merge aliased writes and leave unnamed entries unchanged.

## Worked example

```sh
zeno-fcis contract collections examples/collections/library
```

Expected: exit 0; {"status": "checked", "measured.admissible_views": 160}.

## See also

[Keyed collections](../COLLECTIONS.md) gives the reduction premises and scope. [Examples](../../examples/collections/README.md) include token balances, a library and escrow.

Run `zeno-fcis guide find "check contract collections"` for related cards.
