```json
{
  "name": "generate",
  "title": "Generate deterministic derived artifacts",
  "synopsis": "zeno-fcis generate PROJECT --out OUT --format json",
  "engine": "Compile admitted source into deterministic Rust/manifest files; --check compares without writing.",
  "use_when": [
    "generate Rust declarations",
    "check generated drift",
    "compile a contract policy"
  ],
  "not_for": [
    "proof of human requirements",
    "atomic replacement of a whole output directory"
  ],
  "inputs": [
    "PROJECT: .zeno <= 1048576 bytes; contract form reads project.zeno and policy.json. Other caps: unknown."
  ],
  "outputs": [
    "stdout zeno-fcis/cli/1: status, artifacts, drift and error.code; generated Rust and canonical policy files"
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "generated": "Derived files written; rerun with --check.",
    "current": "All compared bytes match; continue review.",
    "drift": "Missing/changed artifact; regenerate and review differences.",
    "invalid": "Fix source diagnostics.",
    "error": "Inspect error.code; repair I/O or contract entry."
  },
  "limits": {
    "source_bytes": 1048576,
    "scalar_nodes_caps": [
      256,
      4096
    ],
    "scalar_input_caps": [
      32,
      64
    ],
    "scalar_output_caps": [
      16,
      64
    ],
    "other_caps": "unknown"
  },
  "refusals": {
    "artifact-write-failed": "Repair output access and regenerate; previous successfully replaced files may remain."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis generate examples/minimal/project.zeno --out \"$WORK/generated\" --format json",
      "exit": 0,
      "expected": {
        "status": "generated"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis generate examples/minimal/project.zeno --out \"$WORK/generated\" --check --format json",
    "exit": 0,
    "expected": {
      "status": "current"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425",
    "engine": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425",
    "inputs": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425",
    "outputs": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425",
    "exit_codes": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425",
    "verdicts": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425",
    "limits": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425",
    "refusals": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425"
  },
  "commands": [
    "generate",
    "generate contract"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Generate deterministic derived artifacts

## Use when

generate Rust declarations; check generated drift; compile a contract policy.

## Not for

proof of human requirements; atomic replacement of a whole output directory.

## Inputs

PROJECT: .zeno <= 1048576 bytes; contract form reads project.zeno and policy.json. Other caps: unknown.

Engine: Compile admitted source into deterministic Rust/manifest files; --check compares without writing.

Synopsis: "zeno-fcis generate PROJECT --out OUT --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis generate examples/minimal/project.zeno --out "$WORK/generated" --format json
```

Expected: exit 0; {"status": "generated"}.

## Verdicts

`generated`: Derived files written; rerun with --check.

`current`: All compared bytes match; continue review.

`drift`: Missing/changed artifact; regenerate and review differences.

`invalid`: Fix source diagnostics.

`error`: Inspect error.code; repair I/O or contract entry.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/cli/1: status, artifacts, drift and error.code; generated Rust and canonical policy files"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"other_caps": "unknown", "scalar_input_caps": [32, 64], "scalar_nodes_caps": [256, 4096], "scalar_output_caps": [16, 64], "source_bytes": 1048576}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"artifact-write-failed": "Repair output access and regenerate; previous successfully replaced files may remain."}

## Worked example

```sh
zeno-fcis generate examples/minimal/project.zeno --out "$WORK/generated" --check --format json
```

Expected: exit 0; {"status": "current"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425", "exit_codes": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425", "inputs": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425", "limits": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425", "outputs": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425", "refusals": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425", "synopsis": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425", "verdicts": "docs/CLI_REFERENCE.md:87-110; docs/CLI_REFERENCE.md:233-248; docs/CLI_REFERENCE.md:409-425"}

Run `zeno-fcis guide find "generate Rust declarations"` for related cards.
