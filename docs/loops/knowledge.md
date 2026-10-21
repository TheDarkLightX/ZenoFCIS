```json
{
  "name": "knowledge",
  "title": "Consult scoped usage advice",
  "synopsis": "zeno-fcis kb advise --task FACTS",
  "engine": "Finite structured knowledge queries with source provenance; exact checkers retain acceptance authority.",
  "use_when": [
    "choose a checker from usage advice",
    "inspect scoped knowledge",
    "find a supported loop plan",
    "assess a knowledge candidate"
  ],
  "not_for": [
    "proof from retrieved advice",
    "kb assess in a command tree where it is absent"
  ],
  "inputs": [
    "FACTS: structured task predicates; optional external KB layers. Schema and caps: unknown here. No private KB is needed for the public example."
  ],
  "outputs": [
    "stdout zeno-fcis/usage-knowledge/1 for advise; inspect status, provenance and authority; read-only advise writes no application files."
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "complete": "Advice/query finished; run the recommended exact checker.",
    "inconclusive": "Bound or no plan; supply missing facts or choose a supported plan.",
    "refused": "Repair layer integrity or input syntax; inspect kb --help."
  },
  "limits": {
    "all_defaults_and_caps": "unknown"
  },
  "refusals": {
    "unknown-command": "This build has no kb assess; run kb --help and use a release exposing assessment before requesting it."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis kb advise --task 'task(subject,check-program).'",
      "exit": 0,
      "expected": {},
      "contains": []
    },
    {
      "command": "zeno-fcis kb assess --help",
      "exit": 64,
      "expected": {},
      "contains": [
        "unrecognized subcommand"
      ]
    }
  ],
  "example": {
    "command": "zeno-fcis kb assess --help",
    "exit": 64,
    "expected": {},
    "contains": [
      "unrecognized subcommand"
    ]
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help",
    "engine": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help",
    "inputs": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help",
    "outputs": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help",
    "exit_codes": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help",
    "verdicts": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help",
    "limits": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help",
    "refusals": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help"
  },
  "commands": [
    "kb",
    "kb advise",
    "kb plan",
    "kb replan",
    "kb explain",
    "kb check",
    "kb query",
    "kb replay"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Consult scoped usage advice

## Use when

choose a checker from usage advice; inspect scoped knowledge; find a supported loop plan; assess a knowledge candidate.

## Not for

proof from retrieved advice; kb assess in a command tree where it is absent.

## Inputs

FACTS: structured task predicates; optional external KB layers. Schema and caps: unknown here. No private KB is needed for the public example.

Engine: Finite structured knowledge queries with source provenance; exact checkers retain acceptance authority.

Synopsis: "zeno-fcis kb advise --task FACTS"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis kb advise --task 'task(subject,check-program).'
```

Expected: exit 0; [].

```sh
zeno-fcis kb assess --help
```

Expected: exit 64; ["unrecognized subcommand"].

## Verdicts

`complete`: Advice/query finished; run the recommended exact checker.

`inconclusive`: Bound or no plan; supply missing facts or choose a supported plan.

`refused`: Repair layer integrity or input syntax; inspect kb --help.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["stdout zeno-fcis/usage-knowledge/1 for advise; inspect status, provenance and authority; read-only advise writes no application files."]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"all_defaults_and_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"unknown-command": "This build has no kb assess; run kb --help and use a release exposing assessment before requesting it."}

## Worked example

```sh
zeno-fcis kb assess --help
```

Expected: exit 64; ["unrecognized subcommand"].

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help", "exit_codes": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help", "inputs": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help", "limits": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help", "outputs": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help", "refusals": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help", "synopsis": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help", "verdicts": "docs/CLI_REFERENCE.md:2774-2796; zeno-fcis kb --help"}

Run `zeno-fcis guide find "choose a checker from usage advice"` for related cards.
