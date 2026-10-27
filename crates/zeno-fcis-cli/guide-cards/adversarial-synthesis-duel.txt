```json
{
  "name": "adversarial-synthesis-duel",
  "title": "Refine a safety controller against growing attacker histories",
  "synopsis": "python3 -B tools/trace_game/run_v11.py --certificates-only --out OUT",
  "engine": "Counterexample-guided alternating candidate and attacker-history search (double-oracle pattern); the exact finite trace solver replaces iteration on small models.",
  "use_when": [
    "synthesize an invariant safety strategy",
    "alternate strategy synthesis and attacker search",
    "stop controller oscillation",
    "double oracle safety duel"
  ],
  "not_for": [
    "checking that no player profits by deviating (equilibrium checking is future work)",
    "continuous games",
    "security against every real attack"
  ],
  "inputs": [
    "Finite declared domains, attacker move alphabet, candidate class, growing replayable history pool and explicit round/work/time budget. Candidate encoding and external proposer: unknown; no dedicated duel command."
  ],
  "outputs": [
    "Per round record candidate digest/size, pool digest/size, verifier result and replayable counterexample. Use independent source-bound trace certificates for acceptance; no publication authority."
  ],
  "exit_codes": {
    "0": "small-model exact-solver workflow completed",
    "other": "inspect verifier; no dedicated duel exit enum"
  },
  "verdicts": {
    "certificate": "Safe against every attacker history allowed by the declared model. Retain independent certificate check.",
    "checked-through-K": "No exploit found up to K; stop at declared bound or extend it without relabeling the result.",
    "UNKNOWN": "Budget stopped; report no completed safety result.",
    "repeated-candidate": "Stop and report an oscillation/bookkeeping failure.",
    "repeated-counterexample": "Stop and report failed pool enforcement or replay bookkeeping.",
    "class-exhausted": "No strategy in this declared class satisfies the pool; retain complete enumeration and replay evidence."
  },
  "limits": {
    "round_budget": "must be declared by caller",
    "candidate_class": "must be finite for finite termination argument",
    "trace_defaults": "same as trace-game card; other caps unknown"
  },
  "refusals": {
    "pool-inconsistent": "Reject candidate and preserve pool. Never clear counterexamples to make a candidate pass."
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
    "command": "python3 -B tools/trace_game/certificate_check.py \"$WORK/trace\" withdrawal-strategy.json",
    "exit": 0,
    "expected": {
      "evidence": "certified winning strategy"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card",
    "engine": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card",
    "inputs": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card",
    "outputs": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card",
    "exit_codes": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card",
    "verdicts": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card",
    "limits": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card",
    "refusals": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card"
  },
  "commands": [],
  "pitfalls": "Each new candidate must satisfy every replayable attacker history in the pool under the declared model. Recompute candidate-dependent transitions. Preserve legitimacy requirements so reject-everything cannot win. The pool only grows: a refuted candidate never returns; finite candidate classes terminate after finitely many exclusions. Stop on any repeated candidate or counterexample, or class exhaustion. Prefer the smallest consistent candidate to discourage memorization; this gives no statistical generalization guarantee. Record pool size and candidate size each round. A certificate covers the declared attacker alphabet, not every attack.",
  "protocol_steps": [
    "Freeze the discrete model: finite domains, permitted attacker moves, observation order, losing states, legitimate behavior, candidate class and round/work/time budget. The target is an invariant safety controller that never reaches a losing state.",
    "Select the smallest candidate that satisfies every history in the pool. Replay histories using this candidate's transitions. If its digest was seen before, stop and report the repetition.",
    "Challenge that candidate with bounded attacker search through the declared K, or enumerate the model and independently check its certificate when feasible.",
    "Replay each reported counterexample. If a counterexample repeats, stop and report failed pool enforcement. Otherwise append it to the durable pool, record pool size and candidate size, and repeat candidate selection. Never remove a pooled history.",
    "Stop on an independently checked certificate, the declared bound or the budget. Report a certificate as safe against every attacker history allowed by the declared model; report a bounded result as no exploit found up to K. Budget exhaustion claims no completed safety result. Complete candidate-class exhaustion means no strategy in this class.",
    "For a small enumerable model, use the exact solver instead of alternating proposals. The repository example below demonstrates this route; there is no dedicated duel driver in this build."
  ]
}
```

# Refine a safety controller against growing attacker histories

## Use when

synthesize an invariant safety strategy; alternate strategy synthesis and attacker search; stop controller oscillation; double oracle safety duel.

## Not for

checking that no player profits by deviating (equilibrium checking is future work); continuous games; security against every real attack.

## Inputs

Finite declared domains, attacker move alphabet, candidate class, growing replayable history pool and explicit round/work/time budget. Candidate encoding and external proposer: unknown; no dedicated duel command.

Engine: Counterexample-guided alternating candidate and attacker-history search (double-oracle pattern); the exact finite trace solver replaces iteration on small models.

Synopsis: "python3 -B tools/trace_game/run_v11.py --certificates-only --out OUT"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

1. Freeze the discrete model: finite domains, permitted attacker moves, observation order, losing states, legitimate behavior, candidate class and round/work/time budget. The target is an invariant safety controller that never reaches a losing state.

2. Select the smallest candidate that satisfies every history in the pool. Replay histories using this candidate's transitions. If its digest was seen before, stop and report the repetition.

3. Challenge that candidate with bounded attacker search through the declared K, or enumerate the model and independently check its certificate when feasible.

4. Replay each reported counterexample. If a counterexample repeats, stop and report failed pool enforcement. Otherwise append it to the durable pool, record pool size and candidate size, and repeat candidate selection. Never remove a pooled history.

5. Stop on an independently checked certificate, the declared bound or the budget. Report a certificate as safe against every attacker history allowed by the declared model; report a bounded result as no exploit found up to K. Budget exhaustion claims no completed safety result. Complete candidate-class exhaustion means no strategy in this class.

6. For a small enumerable model, use the exact solver instead of alternating proposals. The repository example below demonstrates this route; there is no dedicated duel driver in this build.

```sh
python3 -B tools/trace_game/run_v11.py --certificates-only --out "$WORK/trace"
```

Expected: exit 0; ["withdrawal-safety"].

## Verdicts

`certificate`: Safe against every attacker history allowed by the declared model. Retain independent certificate check.

`checked-through-K`: No exploit found up to K; stop at declared bound or extend it without relabeling the result.

`UNKNOWN`: Budget stopped; report no completed safety result.

`repeated-candidate`: Stop and report an oscillation/bookkeeping failure.

`repeated-counterexample`: Stop and report failed pool enforcement or replay bookkeeping.

`class-exhausted`: No strategy in this declared class satisfies the pool; retain complete enumeration and replay evidence.

Exit codes: {"0": "small-model exact-solver workflow completed", "other": "inspect verifier; no dedicated duel exit enum"}

Outputs: ["Per round record candidate digest/size, pool digest/size, verifier result and replayable counterexample. Use independent source-bound trace certificates for acceptance; no publication authority."]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"candidate_class": "must be finite for finite termination argument", "round_budget": "must be declared by caller", "trace_defaults": "same as trace-game card; other caps unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Each new candidate must satisfy every replayable attacker history in the pool under the declared model. Recompute candidate-dependent transitions. Preserve legitimacy requirements so reject-everything cannot win. The pool only grows: a refuted candidate never returns; finite candidate classes terminate after finitely many exclusions. Stop on any repeated candidate or counterexample, or class exhaustion. Prefer the smallest consistent candidate to discourage memorization; this gives no statistical generalization guarantee. Record pool size and candidate size each round. A certificate covers the declared attacker alphabet, not every attack.

Refusals and remedies: {"pool-inconsistent": "Reject candidate and preserve pool. Never clear counterexamples to make a candidate pass."}

## Worked example

```sh
python3 -B tools/trace_game/certificate_check.py "$WORK/trace" withdrawal-strategy.json
```

Expected: exit 0; {"evidence": "certified winning strategy"}.

## See also

Sources: {"engine": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card", "exit_codes": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card", "inputs": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card", "limits": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card", "outputs": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card", "refusals": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card", "synopsis": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card", "verdicts": "docs/TRACE_GAME.md:1-148; finite exclusion argument stated in this card"}

Run `zeno-fcis guide find "synthesize an invariant safety strategy"` for related cards.
