```json
{
  "name": "contract-collections",
  "title": "Locate collection checking in this build",
  "synopsis": "zeno-fcis generate contract DIR --out OUT --format json",
  "engine": "The available contract compiler validates its supported schema; a contract collections command is absent from this command tree.",
  "use_when": [
    "check contract collections",
    "validate keyed tables",
    "check bounded collection contracts"
  ],
  "not_for": [
    "claiming unsupported collection semantics",
    "treating scalar enumeration as collection coverage"
  ],
  "inputs": [
    "DIR: project.zeno and v2/policy.json; supported collection schema and caps: unknown in this build."
  ],
  "outputs": [
    "generate stdout zeno-fcis/cli/1; inspect status and generated artifacts; no collection-specific checker packet is promised"
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "generated": "Only the supplied supported contract compiled; inspect its schema and run contract review.",
    "error": "Inspect named contract entry; do not silently flatten collections."
  },
  "limits": {
    "collection_caps": "unknown"
  },
  "refusals": {
    "unknown-command": "Run zeno-fcis contract --help. Use a version whose public help exposes the needed checker."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis generate contract examples/guide-counter --out \"$WORK/collection-compile\" --format json",
      "exit": 0,
      "expected": {
        "status": "generated"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract collections --help",
    "exit": 64,
    "expected": {},
    "contains": [
      "unrecognized subcommand"
    ]
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help",
    "engine": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help",
    "inputs": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help",
    "outputs": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help",
    "exit_codes": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help",
    "verdicts": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help",
    "limits": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help",
    "refusals": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help"
  },
  "commands": [
    "generate contract"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Locate collection checking in this build

## Use when

check contract collections; validate keyed tables; check bounded collection contracts.

## Not for

claiming unsupported collection semantics; treating scalar enumeration as collection coverage.

## Inputs

DIR: project.zeno and v2/policy.json; supported collection schema and caps: unknown in this build.

Engine: The available contract compiler validates its supported schema; a contract collections command is absent from this command tree.

Synopsis: "zeno-fcis generate contract DIR --out OUT --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis generate contract examples/guide-counter --out "$WORK/collection-compile" --format json
```

Expected: exit 0; {"status": "generated"}.

## Verdicts

`generated`: Only the supplied supported contract compiled; inspect its schema and run contract review.

`error`: Inspect named contract entry; do not silently flatten collections.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["generate stdout zeno-fcis/cli/1; inspect status and generated artifacts; no collection-specific checker packet is promised"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"collection_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"unknown-command": "Run zeno-fcis contract --help. Use a version whose public help exposes the needed checker."}

## Worked example

```sh
zeno-fcis contract collections --help
```

Expected: exit 64; ["unrecognized subcommand"].

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help", "exit_codes": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help", "inputs": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help", "limits": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help", "outputs": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help", "refusals": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help", "synopsis": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help", "verdicts": "docs/CLI_REFERENCE.md:87-110; zeno-fcis contract --help"}

Run `zeno-fcis guide find "check contract collections"` for related cards.
