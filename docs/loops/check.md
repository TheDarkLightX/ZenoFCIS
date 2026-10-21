```json
{
  "name": "check",
  "title": "Check project declarations",
  "synopsis": "zeno-fcis check PROJECT --format json",
  "engine": "Parse and elaborate the bounded .zeno declarations; optional substance check.",
  "use_when": [
    "check a project",
    "parse declarations",
    "find unresolved fields",
    "validate a zeno specification"
  ],
  "not_for": [
    "program equivalence",
    "proof of a runtime transition"
  ],
  "inputs": [
    "PROJECT: UTF-8 .zeno, at most 1048576 bytes; authored source or new template"
  ],
  "outputs": [
    "stdout zeno-fcis/cli/1: read status, diagnostics, obligations, substance and unresolved_paths; writes nothing"
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "valid": "Declarations elaborate; read obligations, then generate.",
    "invalid": "Fix diagnostics and run check again.",
    "vacuous": "A law/claim cannot constrain a transition; revise it and rerun --require-substantive.",
    "error": "Read error.code; repair input acquisition and retry."
  },
  "limits": {
    "source_bytes": 1048576,
    "other_caps": "unknown"
  },
  "refusals": {
    "project-read-failed": "Supply a readable regular .zeno file; retry check."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis check examples/minimal/project.zeno --format json",
      "exit": 0,
      "expected": {
        "status": "valid"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis check examples/minimal/project.zeno --require-substantive --format json",
    "exit": 1,
    "expected": {
      "status": "vacuous"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185",
    "engine": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185",
    "inputs": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185",
    "outputs": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185",
    "exit_codes": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185",
    "verdicts": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185",
    "limits": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185",
    "refusals": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185"
  },
  "commands": [
    "check"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Check project declarations

## Use when

check a project; parse declarations; find unresolved fields; validate a zeno specification.

## Not for

program equivalence; proof of a runtime transition.

## Inputs

PROJECT: UTF-8 .zeno, at most 1048576 bytes; authored source or new template

Engine: Parse and elaborate the bounded .zeno declarations; optional substance check.

Synopsis: "zeno-fcis check PROJECT --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis check examples/minimal/project.zeno --format json
```

Expected: exit 0; {"status": "valid"}.

## Verdicts

`valid`: Declarations elaborate; read obligations, then generate.

`invalid`: Fix diagnostics and run check again.

`vacuous`: A law/claim cannot constrain a transition; revise it and rerun --require-substantive.

`error`: Read error.code; repair input acquisition and retry.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/cli/1: read status, diagnostics, obligations, substance and unresolved_paths; writes nothing"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"other_caps": "unknown", "source_bytes": 1048576}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"project-read-failed": "Supply a readable regular .zeno file; retry check."}

## Worked example

```sh
zeno-fcis check examples/minimal/project.zeno --require-substantive --format json
```

Expected: exit 1; {"status": "vacuous"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185", "exit_codes": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185", "inputs": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185", "limits": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185", "outputs": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185", "refusals": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185", "synopsis": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185", "verdicts": "docs/CLI_REFERENCE.md:47-87; docs/CLI_REFERENCE.md:173-185"}

Run `zeno-fcis guide find "check a project"` for related cards.
