```json
{
  "name": "synth",
  "title": "Discover and run finite language-neutral synthesis",
  "synopsis": "zeno-fcis synth run PROBLEM --target python --out OUT",
  "engine": "Complete bounded semantic checking and target emission; verify separately executes a complete finite conformance corpus.",
  "use_when": [
    "synthesize a finite core",
    "emit a Python or Rust decision program",
    "check target conformance"
  ],
  "not_for": [
    "unbounded program synthesis",
    "runtime deployment from emitted code alone"
  ],
  "inputs": [
    "PROBLEM: language-neutral finite contract JSON; schema/caps unknown here. Use synth discover before authoring; origins are supplied data."
  ],
  "outputs": [
    "Discovery stdout zeno-fcis/synthesis-discovery/1; synthesis result schemas use /2 (exact ids unknown here). Read status, bounds, candidate and conformance; writes target artifacts."
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "complete": "Only the complete declared finite problem is covered; run synth verify.",
    "inconclusive": "Budget stopped; retain no completeness claim.",
    "refused": "Repair grammar or reduce to supported finite fragment."
  },
  "limits": {
    "all_defaults_and_caps": "unknown"
  },
  "refusals": {
    "unsupported-profile": "Run synth discover; use an admitted bounded profile."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis synth discover",
      "exit": 0,
      "expected": {},
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis synth discover",
    "exit": 0,
    "expected": {},
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help",
    "engine": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help",
    "inputs": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help",
    "outputs": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help",
    "exit_codes": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help",
    "verdicts": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help",
    "limits": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help",
    "refusals": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help"
  },
  "commands": [
    "synth",
    "synth discover",
    "synth run",
    "synth verify"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Discover and run finite language-neutral synthesis

## Use when

synthesize a finite core; emit a Python or Rust decision program; check target conformance.

## Not for

unbounded program synthesis; runtime deployment from emitted code alone.

## Inputs

PROBLEM: language-neutral finite contract JSON; schema/caps unknown here. Use synth discover before authoring; origins are supplied data.

Engine: Complete bounded semantic checking and target emission; verify separately executes a complete finite conformance corpus.

Synopsis: "zeno-fcis synth run PROBLEM --target python --out OUT"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis synth discover
```

Expected: exit 0; [].

## Verdicts

`complete`: Only the complete declared finite problem is covered; run synth verify.

`inconclusive`: Budget stopped; retain no completeness claim.

`refused`: Repair grammar or reduce to supported finite fragment.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["Discovery stdout zeno-fcis/synthesis-discovery/1; synthesis result schemas use /2 (exact ids unknown here). Read status, bounds, candidate and conformance; writes target artifacts."]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"all_defaults_and_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"unsupported-profile": "Run synth discover; use an admitted bounded profile."}

## Worked example

```sh
zeno-fcis synth discover
```

Expected: exit 0; [].

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help", "exit_codes": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help", "inputs": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help", "limits": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help", "outputs": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help", "refusals": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help", "synopsis": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help", "verdicts": "docs/CLI_REFERENCE.md:353-365; zeno-fcis synth --help"}

Run `zeno-fcis guide find "synthesize a finite core"` for related cards.
