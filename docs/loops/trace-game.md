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
    "Repository withdrawal/composition fixtures, or user game JSON zeno-fcis/game/1 through tools/trace_game/game.py. Use fresh evidence outside source; --certificates-only skips repository-wide checks."
  ],
  "outputs": [
    "Retained tables, certificates and bindings; game.py writes game-binding.json, native-table.json, certificate.json and verdict.json. Read evidence and claim; fixed-game result schemas differ."
  ],
  "exit_codes": {
    "0": "game.py completed, including counterexample or no guard in class; certificate_check accepted the evidence, including UNKNOWN diagnostics",
    "2": "game.py malformed input/resource stop (UNKNOWN), or certificate_check refusal/argument error",
    "other": "legacy run.py/run_v11.py failures; inspect their retained logs"
  },
  "verdicts": {
    "checked-through-K": "Legacy prefix-search run.status: complete declared histories only through K.",
    "checked up to K": "User-game checker evidence: complete declared histories through K only.",
    "counterexample": "Replayed violation or infinite target-avoiding cycle; game.py still exits 0.",
    "certified safety": "Closed safe set covers all histories of the declared finite game.",
    "certified reachability": "Checked ranks force the target against every permitted event.",
    "no guard in class": "Every declared user-game fixed guard was refuted; game.py exits 0.",
    "exhausted fixed grammar": "Legacy fixed-guard checker exhausted its declared grammar.",
    "certified guard": "Legacy fixed or reactive controller passed the certificate checker.",
    "certified winning strategy": "Legacy fixed or reactive controller passed the certificate checker.",
    "certified unrealizability": "Legacy declared controller class cannot satisfy the requirement.",
    "UNKNOWN": "No safety/impossibility claim; certificate_check may accept only the diagnostic binding."
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
    "synopsis": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md",
    "engine": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md",
    "inputs": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md",
    "outputs": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md",
    "exit_codes": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md",
    "verdicts": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md",
    "limits": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md",
    "refusals": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md"
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

Repository withdrawal/composition fixtures, or user game JSON zeno-fcis/game/1 through tools/trace_game/game.py. Use fresh evidence outside source; --certificates-only skips repository-wide checks.

Engine: Bounded complete prefix search; complete finite product graph and greatest fixed point; separate certificate checker reconstructs moves and closure.

Synopsis: ["python3 -B tools/trace_game/run.py --out OUT", "python3 -B tools/trace_game/run_v11.py --certificates-only --out OUT", "python3 -B tools/trace_game/certificate_check.py OUT withdrawal-safety.json"]

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
python3 -B tools/trace_game/run_v11.py --certificates-only --out "$WORK/trace"
```

Expected: exit 0; ["withdrawal-safety"].

## Verdicts

`checked-through-K`: Legacy prefix-search run.status: complete declared histories only through K.

`checked up to K`: User-game checker evidence: complete declared histories through K only.

`counterexample`: Replayed violation or infinite target-avoiding cycle; game.py still exits 0.

`certified safety`: Closed safe set covers all histories of the declared finite game.

`certified reachability`: Checked ranks force the target against every permitted event.

`no guard in class`: Every declared user-game fixed guard was refuted; game.py exits 0.

`exhausted fixed grammar`: Legacy fixed-guard checker exhausted its declared grammar.

`certified guard`: Legacy fixed or reactive controller passed the certificate checker.

`certified winning strategy`: Legacy fixed or reactive controller passed the certificate checker.

`certified unrealizability`: Legacy declared controller class cannot satisfy the requirement.

`UNKNOWN`: No safety/impossibility claim; certificate_check may accept only the diagnostic binding.

Exit codes: {"0": "game.py completed, including counterexample or no guard in class; certificate_check accepted the evidence, including UNKNOWN diagnostics", "2": "game.py malformed input/resource stop (UNKNOWN), or certificate_check refusal/argument error", "other": "legacy run.py/run_v11.py failures; inspect their retained logs"}

Outputs: ["Retained tables, certificates and bindings; game.py writes game-binding.json, native-table.json, certificate.json and verdict.json. Read evidence and claim; fixed-game result schemas differ."]

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

Background: Edmund M. Clarke and E. Allen Emerson, “Design and Synthesis of Synchronization Skeletons Using Branching Time Temporal Logic”, Logics of Programs, 1981. Finite graph closure and ranked reachability stay within the declared game and bounds.

Sources: {"engine": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md", "exit_codes": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md", "inputs": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md", "limits": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md", "outputs": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md", "refusals": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md", "synopsis": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md", "verdicts": "tools/trace_game/game.py; tools/trace_game/game_check.py; tools/trace_game/certificate_check.py; docs/TRACE_GAME.md"}

Run `zeno-fcis guide find "find an adversarial history"` for related cards.
