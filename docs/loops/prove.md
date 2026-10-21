```json
{
  "name": "prove",
  "title": "Run a separately pinned formal backend",
  "synopsis": "zeno-fcis prove PROJECT --claim ID --backend cvc5 --tools TOOLS",
  "engine": "Export a claim, execute the separately pinned backend, retain its report and replay a SAT model.",
  "use_when": [
    "run an SMT claim",
    "check an inductive claim with a solver",
    "inspect a replayed counterexample"
  ],
  "not_for": [
    "treating unchecked UNSAT as a proof",
    "public Lean export in this build",
    "application authorization"
  ],
  "inputs": [
    "PROJECT: .zeno <= 1048576 bytes; TOOLS: zeno-fcis/tools/2 with executable identities. Origins: authored project and separately configured pinned tools. Other caps: unknown."
  ],
  "outputs": [
    "human scope/status lines; process/evidence records under .zeno-fcis/evidence. Read scope and backend status; no stdout JSON schema."
  ],
  "exit_codes": {
    "0": "requested result completed",
    "1": "invalid specification or refutation",
    "2": "blocked or inconclusive; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "ProposedUnsat": "Unchecked CVC5 output retained; exit 2; seek independent evidence.",
    "Refuted": "Replayed model refutes the claim; inspect and revise assumptions.",
    "Undefined": "Replayed model makes strict arithmetic undefined; repair the formula.",
    "Blocked": "Tool/evidence unavailable or unsupported; inspect tools and doctor.",
    "KernelChecked": "Historical retained evidence type; current public Lean exporter refuses; do not claim this result here."
  },
  "limits": {
    "source_bytes": 1048576,
    "backend_limits": "unknown"
  },
  "refusals": {
    "tools-manifest-blocked": "Supply a zeno-fcis/tools/2 manifest; run backend inspect --tools FILE and doctor --tools FILE.",
    "unsupported-lean": "Use a supported backend; current public exporter refuses Lean."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis check examples/guide-counter/project.zeno --format json",
      "exit": 0,
      "expected": {
        "status": "valid"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis prove examples/guide-counter/project.zeno --claim 600 --backend cvc5 --tools \"$WORK/missing-tools.json\"",
    "exit": 2,
    "expected": {},
    "contains": [
      "tools manifest blocked"
    ]
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26",
    "engine": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26",
    "inputs": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26",
    "outputs": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26",
    "exit_codes": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26",
    "verdicts": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26",
    "limits": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26",
    "refusals": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26"
  },
  "commands": [
    "prove",
    "counterexample",
    "doctor",
    "backend inspect",
    "backend verify"
  ],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Run a separately pinned formal backend

## Use when

run an SMT claim; check an inductive claim with a solver; inspect a replayed counterexample.

## Not for

treating unchecked UNSAT as a proof; public Lean export in this build; application authorization.

## Inputs

PROJECT: .zeno <= 1048576 bytes; TOOLS: zeno-fcis/tools/2 with executable identities. Origins: authored project and separately configured pinned tools. Other caps: unknown.

Engine: Export a claim, execute the separately pinned backend, retain its report and replay a SAT model.

Synopsis: "zeno-fcis prove PROJECT --claim ID --backend cvc5 --tools TOOLS"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis check examples/guide-counter/project.zeno --format json
```

Expected: exit 0; {"status": "valid"}.

## Verdicts

`ProposedUnsat`: Unchecked CVC5 output retained; exit 2; seek independent evidence.

`Refuted`: Replayed model refutes the claim; inspect and revise assumptions.

`Undefined`: Replayed model makes strict arithmetic undefined; repair the formula.

`Blocked`: Tool/evidence unavailable or unsupported; inspect tools and doctor.

`KernelChecked`: Historical retained evidence type; current public Lean exporter refuses; do not claim this result here.

Exit codes: {"0": "requested result completed", "1": "invalid specification or refutation", "2": "blocked or inconclusive; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["human scope/status lines; process/evidence records under .zeno-fcis/evidence. Read scope and backend status; no stdout JSON schema."]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"backend_limits": "unknown", "source_bytes": 1048576}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"tools-manifest-blocked": "Supply a zeno-fcis/tools/2 manifest; run backend inspect --tools FILE and doctor --tools FILE.", "unsupported-lean": "Use a supported backend; current public exporter refuses Lean."}

## Worked example

```sh
zeno-fcis prove examples/guide-counter/project.zeno --claim 600 --backend cvc5 --tools "$WORK/missing-tools.json"
```

Expected: exit 2; ["tools manifest blocked"].

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26", "exit_codes": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26", "inputs": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26", "limits": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26", "outputs": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26", "refusals": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26", "synopsis": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26", "verdicts": "docs/CLI_REFERENCE.md:311-351; docs/INDUCTIVE_CLAIMS.md:1-26"}

Run `zeno-fcis guide find "run an SMT claim"` for related cards.
