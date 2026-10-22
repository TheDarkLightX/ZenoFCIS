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
    "PROBLEM: zeno-fcis/synthesis-problem/1 JSON, at most 262,144 bytes; profile zeno-fcis/finite-i128/1. Run synth discover for operations, adapters and limits."
  ],
  "outputs": [
    "discover: zeno-fcis/synthesis-discovery/1; run: zeno-fcis/synthesis-result/2 and zeno-fcis/synthesis-artifact/2 manifest; verify: zeno-fcis/synthesis-conformance/1. Read status, semantic scope and runtime_conformance."
  ],
  "exit_codes": {
    "0": "discover completed, run selected/current, or verify passed",
    "1": "invalid-problem, unrealizable, no-solution, contract-trap, artifact-drift or conformance-failed",
    "2": "incomplete, limit-exceeded, indeterminate or conformance-unknown",
    "3": "I/O failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "selected": "Finite solution emitted; runtime_conformance remains not-run until verify.",
    "current": "run --check recomputed the exact existing artifact set.",
    "passed": "verify ran the target on every admitted input and matched interpreter/relation.",
    "unrealizable": "A declared input has no allowed output.",
    "no-solution": "No candidate in the complete admitted search space satisfied the finite relation.",
    "contract-trap": "A finite contract observation failed; inspect input/output.",
    "artifact-drift": "Recomputed files differ from the supplied artifacts.",
    "incomplete / limit-exceeded / indeterminate": "Budget or checker stop; no complete result.",
    "conformance-unknown / conformance-failed": "Target execution incomplete or observations disagreed.",
    "invalid-problem / refused": "Inspect the admission diagnostic; no synthesis acceptance."
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
    "synopsis": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis",
    "engine": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis",
    "inputs": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis",
    "outputs": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis",
    "exit_codes": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis",
    "verdicts": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis",
    "limits": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis",
    "refusals": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis"
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

PROBLEM: zeno-fcis/synthesis-problem/1 JSON, at most 262,144 bytes; profile zeno-fcis/finite-i128/1. Run synth discover for operations, adapters and limits.

Engine: Complete bounded semantic checking and target emission; verify separately executes a complete finite conformance corpus.

Synopsis: "zeno-fcis synth run PROBLEM --target python --out OUT"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis synth discover
```

Expected: exit 0; [].

## Verdicts

`selected`: Finite solution emitted; runtime_conformance remains not-run until verify.

`current`: run --check recomputed the exact existing artifact set.

`passed`: verify ran the target on every admitted input and matched interpreter/relation.

`unrealizable`: A declared input has no allowed output.

`no-solution`: No candidate in the complete admitted search space satisfied the finite relation.

`contract-trap`: A finite contract observation failed; inspect input/output.

`artifact-drift`: Recomputed files differ from the supplied artifacts.

`incomplete / limit-exceeded / indeterminate`: Budget or checker stop; no complete result.

`conformance-unknown / conformance-failed`: Target execution incomplete or observations disagreed.

`invalid-problem / refused`: Inspect the admission diagnostic; no synthesis acceptance.

Exit codes: {"0": "discover completed, run selected/current, or verify passed", "1": "invalid-problem, unrealizable, no-solution, contract-trap, artifact-drift or conformance-failed", "2": "incomplete, limit-exceeded, indeterminate or conformance-unknown", "3": "I/O failure", "64": "invalid command-line usage"}

Outputs: ["discover: zeno-fcis/synthesis-discovery/1; run: zeno-fcis/synthesis-result/2 and zeno-fcis/synthesis-artifact/2 manifest; verify: zeno-fcis/synthesis-conformance/1. Read status, semantic scope and runtime_conformance."]

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

Background: Armando Solar-Lezama et al., “Combinatorial Sketching for Finite Programs”, ASPLOS, 2006. This card describes bounded finite enumeration and separate target conformance; it makes no new method claim.

Sources: {"engine": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis", "exit_codes": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis", "inputs": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis", "limits": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis", "outputs": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis", "refusals": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis", "synopsis": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis", "verdicts": "crates/zeno-fcis-cli/src/synth/mod.rs; crates/zeno-fcis-cli/src/synth/problem.rs; docs/CLI_REFERENCE.md#finite-synthesis"}

Run `zeno-fcis guide find "synthesize a finite core"` for related cards.
