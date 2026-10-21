```json
{
  "name": "trace-game",
  "title": "Check adversarial histories and finite safety controllers",
  "synopsis": [
    "python3 -B tools/trace_game/run.py --out OUT",
    "python3 -B tools/trace_game/run_v11.py --certificates-only --out OUT",
    "python3 -B tools/trace_game/certificate_check.py OUT withdrawal-safety.json"
  ],
  "engine": "Bounded complete prefix search; complete finite product graph and greatest fixed point; separate certificate checker reconstructs moves and closure.",
  "use_when": [
    "find an adversarial history",
    "check bounded attacker traces",
    "select a guard",
    "certify a reactive strategy"
  ],
  "not_for": [
    "additional attacker moves outside the declared alphabet",
    "real schedulers or deployed shell safety",
    "equilibrium checks"
  ],
  "inputs": [
    "Repository finite withdrawal/composition fixtures; --certificates-only omits repository-wide documentation/style checks and is not release qualification; fresh output outside committed source; cached pinned Rust dependencies. Certificate binding pins revision, all sources, binary and artifacts."
  ],
  "outputs": [
    "retained tables, certificates, binding.json and results.json; checker stdout JSON: read evidence and claim; schema: unknown"
  ],
  "exit_codes": {
    "0": "requested runner/check completed",
    "2": "certificate binding/refusal or argument error",
    "other": "unknown"
  },
  "verdicts": {
    "checked-through-K": "No exploit found up to K, only under declared moves; increase a declared bound or seek complete enumeration.",
    "counterexample": "Replay history, add it to regression pool, revise controller.",
    "certified safety": "Safe against every attacker history allowed by the declared model; retain independent check.",
    "certified guard": "Selected fixed guard in grammar G is safe in the declared model.",
    "certified winning strategy": "Declared-observation reactive strategy is safe in the declared model.",
    "certified unrealizability": "No controller in declared class satisfies the requirement; revise class/assumptions.",
    "UNKNOWN": "Budget/domain/extraction stop; no safety or impossibility claim."
  },
  "limits": {
    "product_states_default": 10000,
    "edges_default": 100000,
    "construction_seconds_default": 30,
    "solver_work_default": 1000000,
    "solver_seconds_default": 30,
    "search_check_memory_mib": 1024,
    "build_process_memory_mib": 3072,
    "build_jobs": 2,
    "other_caps": "unknown"
  },
  "refusals": {
    "binding-drift": "Rebuild evidence from the committed source and fresh output; never edit the binding to bypass refusal.",
    "empty-environment": "UNKNOWN; repair the declared environment before claiming safety."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "python3 -B tools/trace_game/run_v11.py --certificates-only --out \"$WORK/trace\"",
      "exit": 0,
      "expected": {},
      "contains": [
        "withdrawal-safety"
      ]
    }
  ],
  "example": {
    "command": "python3 -B tools/trace_game/certificate_check.py \"$WORK/trace\" withdrawal-safety.json",
    "exit": 0,
    "expected": {
      "evidence": "certified safety"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/TRACE_GAME.md:1-168",
    "engine": "docs/TRACE_GAME.md:1-168",
    "inputs": "docs/TRACE_GAME.md:1-168",
    "outputs": "docs/TRACE_GAME.md:1-168",
    "exit_codes": "docs/TRACE_GAME.md:1-168",
    "verdicts": "docs/TRACE_GAME.md:1-168",
    "limits": "docs/TRACE_GAME.md:1-168",
    "refusals": "docs/TRACE_GAME.md:1-168"
  },
  "commands": [],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority.",
  "help_commands": [
    "python3 -B tools/trace_game/run.py --help",
    "python3 -B tools/trace_game/run_v11.py --help",
    "python3 -B tools/trace_game/certificate_check.py --help"
  ]
}
```

# Check adversarial histories and finite safety controllers

## Use when

find an adversarial history; check bounded attacker traces; select a guard; certify a reactive strategy.

## Not for

additional attacker moves outside the declared alphabet; real schedulers or deployed shell safety; equilibrium checks.

## Inputs

Repository finite withdrawal/composition fixtures; --certificates-only omits repository-wide documentation/style checks and is not release qualification; fresh output outside committed source; cached pinned Rust dependencies. Certificate binding pins revision, all sources, binary and artifacts.

Engine: Bounded complete prefix search; complete finite product graph and greatest fixed point; separate certificate checker reconstructs moves and closure.

Synopsis: ["python3 -B tools/trace_game/run.py --out OUT", "python3 -B tools/trace_game/run_v11.py --certificates-only --out OUT", "python3 -B tools/trace_game/certificate_check.py OUT withdrawal-safety.json"]

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
python3 -B tools/trace_game/run_v11.py --certificates-only --out "$WORK/trace"
```

Expected: exit 0; ["withdrawal-safety"].

## Verdicts

`checked-through-K`: No exploit found up to K, only under declared moves; increase a declared bound or seek complete enumeration.

`counterexample`: Replay history, add it to regression pool, revise controller.

`certified safety`: Safe against every attacker history allowed by the declared model; retain independent check.

`certified guard`: Selected fixed guard in grammar G is safe in the declared model.

`certified winning strategy`: Declared-observation reactive strategy is safe in the declared model.

`certified unrealizability`: No controller in declared class satisfies the requirement; revise class/assumptions.

`UNKNOWN`: Budget/domain/extraction stop; no safety or impossibility claim.

Exit codes: {"0": "requested runner/check completed", "2": "certificate binding/refusal or argument error", "other": "unknown"}

Outputs: ["retained tables, certificates, binding.json and results.json; checker stdout JSON: read evidence and claim; schema: unknown"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"build_jobs": 2, "build_process_memory_mib": 3072, "construction_seconds_default": 30, "edges_default": 100000, "other_caps": "unknown", "product_states_default": 10000, "search_check_memory_mib": 1024, "solver_seconds_default": 30, "solver_work_default": 1000000}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"binding-drift": "Rebuild evidence from the committed source and fresh output; never edit the binding to bypass refusal.", "empty-environment": "UNKNOWN; repair the declared environment before claiming safety."}

## Worked example

```sh
python3 -B tools/trace_game/certificate_check.py "$WORK/trace" withdrawal-safety.json
```

Expected: exit 0; {"evidence": "certified safety"}.

## See also

Sources: {"engine": "docs/TRACE_GAME.md:1-168", "exit_codes": "docs/TRACE_GAME.md:1-168", "inputs": "docs/TRACE_GAME.md:1-168", "limits": "docs/TRACE_GAME.md:1-168", "outputs": "docs/TRACE_GAME.md:1-168", "refusals": "docs/TRACE_GAME.md:1-168", "synopsis": "docs/TRACE_GAME.md:1-168", "verdicts": "docs/TRACE_GAME.md:1-168"}

Run `zeno-fcis guide find "find an adversarial history"` for related cards.
