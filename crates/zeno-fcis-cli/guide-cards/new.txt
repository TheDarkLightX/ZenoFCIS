```json
{
  "name": "new",
  "title": "Start a project from a supported template",
  "synopsis": "zeno-fcis new DIR --template minimal",
  "engine": "Copy a bounded template; application templates bind exact package versions to a supplied source tree.",
  "use_when": [
    "start a new project",
    "scaffold a minimal specification",
    "create an application template"
  ],
  "not_for": [
    "overwriting an existing application",
    "runtime qualification by scaffolding alone"
  ],
  "inputs": [
    "DIR must be absent or empty; application templates require a matching source tree and Cargo.lock. File caps: unknown."
  ],
  "outputs": [
    "human created/binding lines; project files written under DIR; stdout JSON schema: none"
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "created": "Template copied; check project.zeno and review requirements.",
    "refused": "Preserve existing target; choose a fresh directory or explicit matching --source."
  },
  "limits": {
    "other_caps": "unknown"
  },
  "refusals": {
    "nonempty-target": "Choose a fresh DIR.",
    "source-unavailable": "Pass a matching extracted source tree with --source."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis new \"$WORK/new-project\" --template minimal",
      "exit": 0,
      "expected": {},
      "contains": [
        "created"
      ]
    }
  ],
  "example": {
    "command": "zeno-fcis check \"$WORK/new-project/project.zeno\" --format json",
    "exit": 0,
    "expected": {
      "status": "valid"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166",
    "engine": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166",
    "inputs": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166",
    "outputs": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166",
    "exit_codes": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166",
    "verdicts": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166",
    "limits": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166",
    "refusals": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166"
  },
  "commands": [
    "new"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Start a project from a supported template

## Use when

start a new project; scaffold a minimal specification; create an application template.

## Not for

overwriting an existing application; runtime qualification by scaffolding alone.

## Inputs

DIR must be absent or empty; application templates require a matching source tree and Cargo.lock. File caps: unknown.

Engine: Copy a bounded template; application templates bind exact package versions to a supplied source tree.

Synopsis: "zeno-fcis new DIR --template minimal"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis new "$WORK/new-project" --template minimal
```

Expected: exit 0; ["created"].

## Verdicts

`created`: Template copied; check project.zeno and review requirements.

`refused`: Preserve existing target; choose a fresh directory or explicit matching --source.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["human created/binding lines; project files written under DIR; stdout JSON schema: none"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"other_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"nonempty-target": "Choose a fresh DIR.", "source-unavailable": "Pass a matching extracted source tree with --source."}

## Worked example

```sh
zeno-fcis check "$WORK/new-project/project.zeno" --format json
```

Expected: exit 0; {"status": "valid"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166", "exit_codes": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166", "inputs": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166", "limits": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166", "outputs": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166", "refusals": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166", "synopsis": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166", "verdicts": "docs/CLI_REFERENCE.md:47; docs/CLI_REFERENCE.md:111-166"}

Run `zeno-fcis guide find "start a new project"` for related cards.
