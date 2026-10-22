```json
{
  "name": "zal",
  "title": "Check and explain a finite specified behaviour",
  "synopsis": "python3 integrations/zal/run.py check FILE",
  "engine": "Deterministic finite machine enumeration; profile 2 temporal checks cover executions under its explicit fairness premises.",
  "use_when": [
    "review intended state machine behaviour",
    "translate controlled English and symbolic rules",
    "explain a concrete state transition"
  ],
  "not_for": [
    "authenticated user identity",
    "unbounded real-world liveness",
    "agreement supplied by an MCP argument"
  ],
  "inputs": [
    "FILE: zal/finite-fsm/1 or zal/finite-fsm/2 source; use shipped order examples. Other supported typed profiles and size caps: unknown here."
  ],
  "outputs": [
    "check stdout JSON: status, profile, scope, checks, counterexamples, authority and factory; no runtime publication"
  ],
  "exit_codes": {
    "0": "completed result; check can report pass-with-scope and diff can still contain differences",
    "1": "counterexample or unfinished (finite-fsm); counterexample or fail (typed/effects)",
    "2": "refused input, I/O or malformed command-line arguments",
    "3": "unsupported"
  },
  "verdicts": {
    "pass-with-scope": "Only reported finite checks hold; inspect missing invariants and factory:not-run.",
    "counterexample": "Inspect the retained witness, revise requirements, then check.",
    "unfinished / fail": "Finite/typed checks did not pass; inspect profile and failed checks.",
    "unsupported": "The selected profile/operation has no supported implementation.",
    "refused": "Repair grammar, profile or input access."
  },
  "limits": {
    "order_example_raw_inputs": 24,
    "other_defaults_and_caps": "unknown"
  },
  "refusals": {
    "unsupported-lowering": "Temporal properties/fairness have no automatic factory lowering; retain separate model evidence."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "python3 integrations/zal/run.py check integrations/zal/examples/order2.zal",
      "exit": 0,
      "expected": {
        "status": "pass-with-scope",
        "authority": "none"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "python3 integrations/zal/run.py check integrations/zal/examples/order.zal",
    "exit": 0,
    "expected": {
      "status": "pass-with-scope",
      "authority": "none"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help",
    "engine": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help",
    "inputs": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help",
    "outputs": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help",
    "exit_codes": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help",
    "verdicts": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help",
    "limits": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help",
    "refusals": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help"
  },
  "commands": [],
  "pitfalls": "Read the result scope before revising the input. No report grants runtime or publication authority.",
  "help_commands": [
    "python3 -B integrations/zal/run.py --help",
    "python3 -B integrations/zal/run.py check --help"
  ]
}
```

# Check and explain a finite specified behaviour

## Use when

review intended state machine behaviour; translate controlled English and symbolic rules; explain a concrete state transition.

## Not for

authenticated user identity; unbounded real-world liveness; agreement supplied by an MCP argument.

## Inputs

FILE: zal/finite-fsm/1 or zal/finite-fsm/2 source; use shipped order examples. Other supported typed profiles and size caps: unknown here.

Engine: Deterministic finite machine enumeration; profile 2 temporal checks cover executions under its explicit fairness premises.

Synopsis: "python3 integrations/zal/run.py check FILE"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
python3 integrations/zal/run.py check integrations/zal/examples/order2.zal
```

Expected: exit 0; {"authority": "none", "status": "pass-with-scope"}.

## Verdicts

`pass-with-scope`: Only reported finite checks hold; inspect missing invariants and factory:not-run.

`counterexample`: Inspect the retained witness, revise requirements, then check.

`unfinished / fail`: Finite/typed checks did not pass; inspect profile and failed checks.

`unsupported`: The selected profile/operation has no supported implementation.

`refused`: Repair grammar, profile or input access.

Exit codes: {"0": "completed result; check can report pass-with-scope and diff can still contain differences", "1": "counterexample or unfinished (finite-fsm); counterexample or fail (typed/effects)", "2": "refused input, I/O or malformed command-line arguments", "3": "unsupported"}

Outputs: ["check stdout JSON: status, profile, scope, checks, counterexamples, authority and factory; no runtime publication"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"order_example_raw_inputs": 24, "other_defaults_and_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"unsupported-lowering": "Temporal properties/fairness have no automatic factory lowering; retain separate model evidence."}

## Worked example

```sh
python3 integrations/zal/run.py check integrations/zal/examples/order.zal
```

Expected: exit 0; {"authority": "none", "status": "pass-with-scope"}.

## See also

Background: Edmund M. Clarke, Orna Grumberg and Doron A. Peled, “Model Checking”, MIT Press, 1999. These are finite model checks under the reported assumptions, without runtime publication authority.

Sources: {"engine": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help", "exit_codes": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help", "inputs": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help", "limits": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help", "outputs": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help", "refusals": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help", "synopsis": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help", "verdicts": "docs/zal/README.md:1-30; docs/zal/README.md:398-470; python3 integrations/zal/run.py --help"}

Run `zeno-fcis guide find "review intended state machine behaviour"` for related cards.
