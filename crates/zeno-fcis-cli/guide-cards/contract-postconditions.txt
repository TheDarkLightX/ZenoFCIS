```json
{
  "name": "contract-postconditions",
  "title": "Propose and independently screen postconditions",
  "synopsis": "zeno-fcis contract postconditions prompt|check DIR --intent FILE [--max-tuples N] [--proposals FILE] [--mutant FILE]...",
  "engine": "prompt emits a source-bound request (interface, written intent, syntax, output schema, request digest) with the policy hash-bound but omitted; check evaluates each proposal on every committed decision of the frozen contract and against each supplied mutant policy, counting only actual failing admitted transitions as detections.",
  "use_when": [
    "screen AI-proposed postconditions",
    "does a postcondition hold and detect a planted fault",
    "request postconditions for a contract",
    "mutant policies"
  ],
  "not_for": [
    "proving that a condition captures the author's intent",
    "contracts whose complete domain exceeds 4096 tuples",
    "changing the grammar, policy, laws or adoption"
  ],
  "inputs": [
    "DIR: scalar application with project.zeno and v2/policy.json. --intent: written requirements, at most 65536 bytes; the exact bytes bind prompt and check. --max-tuples 1..=4096 (default 4096) must match between prompt and check. check: --proposals FILE in the schema the prompt states; --mutant FILE repeatable up to 8, a complete policy JSON whose only changed fields are cases and variables; duplicates refused. No writes; no model or network call."
  ],
  "outputs": [
    "prompt: request JSON on stdout containing the written intent and request digest. check: zeno-fcis/postcondition-report/1 with authority none, status and candidates[].status checked-discriminating, refused or refuted, plus no-discrimination rows"
  ],
  "exit_codes": {
    "0": "check: every proposal is checked-discriminating; prompt: request emitted",
    "1": "refused (oversize intent, domain above cap, malformed proposals, duplicate mutant) or a refuted proposal",
    "2": "inconclusive, trivial, or no discrimination against the supplied mutants",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "checked-discriminating": "Holds on every committed decision of the contract and fails on an admitted transition of at least one mutant.",
    "refuted": "Fails on a committed decision of the current contract.",
    "refused": "Compilation failure, undefined outcome, oversize input or domain; counts as no evidence.",
    "no-discrimination": "Holds on both the contract and every mutant; the condition may still be too weak."
  },
  "limits": {
    "max_tuples": 4096,
    "intent_bytes": 65536,
    "mutants": 8
  },
  "refusals": {
    "intent-too-large": "Keep the requirements file at or under 65536 bytes.",
    "domain-above-cap": "Narrow the declared domain; the prototype enumerates at most 4096 tuples.",
    "input-not-regular-file": "Pipes and FIFOs are refused without waiting."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "printf 'Increment adds exactly one; decrement subtracts exactly one.\\n' > $WORK/intent.txt && zeno-fcis contract postconditions prompt examples/guide-counter --intent $WORK/intent.txt",
      "exit": 0,
      "expected": {},
      "contains": [
        "written-intent"
      ]
    }
  ],
  "example": {
    "command": "printf 'x%.0s' $(seq 1 65537) > $WORK/too-long.txt && zeno-fcis contract postconditions prompt examples/guide-counter --intent $WORK/too-long.txt",
    "exit": 1,
    "expected": {
      "status": "refused"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs",
    "engine": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs",
    "inputs": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs",
    "outputs": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs",
    "exit_codes": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs",
    "verdicts": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs",
    "limits": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs",
    "refusals": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs"
  },
  "commands": [
    "contract postconditions"
  ],
  "pitfalls": "Prototype, advisory only. Passing the contract does not establish detection, and detection does not establish that the condition captures the intent. Compilation failures and undefined outcomes never count as detected faults. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Propose and independently screen postconditions

## Use when

screen AI-proposed postconditions; does a postcondition hold and detect a planted fault; request postconditions for a contract; mutant policies.

## Not for

proving that a condition captures the author's intent; contracts whose complete domain exceeds 4096 tuples; changing the grammar, policy, laws or adoption.

## Inputs

DIR: scalar application with project.zeno and v2/policy.json. --intent: written requirements, at most 65536 bytes; the exact bytes bind prompt and check. --max-tuples 1..=4096 (default 4096) must match between prompt and check. check: --proposals FILE in the schema the prompt states; --mutant FILE repeatable up to 8, a complete policy JSON whose only changed fields are cases and variables; duplicates refused. No writes; no model or network call.

Engine: prompt emits a source-bound request (interface, written intent, syntax, output schema, request digest) with the policy hash-bound but omitted; check evaluates each proposal on every committed decision of the frozen contract and against each supplied mutant policy, counting only actual failing admitted transitions as detections.

Synopsis: "zeno-fcis contract postconditions prompt|check DIR --intent FILE [--max-tuples N] [--proposals FILE] [--mutant FILE]..."

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
printf 'Increment adds exactly one; decrement subtracts exactly one.\n' > $WORK/intent.txt && zeno-fcis contract postconditions prompt examples/guide-counter --intent $WORK/intent.txt
```

Expected: exit 0; ["written-intent"].

## Verdicts

`checked-discriminating`: Holds on every committed decision of the contract and fails on an admitted transition of at least one mutant.

`refuted`: Fails on a committed decision of the current contract.

`refused`: Compilation failure, undefined outcome, oversize input or domain; counts as no evidence.

`no-discrimination`: Holds on both the contract and every mutant; the condition may still be too weak.

Exit codes: {"0": "check: every proposal is checked-discriminating; prompt: request emitted", "1": "refused (oversize intent, domain above cap, malformed proposals, duplicate mutant) or a refuted proposal", "2": "inconclusive, trivial, or no discrimination against the supplied mutants", "64": "invalid command-line usage"}

Outputs: ["prompt: request JSON on stdout containing the written intent and request digest. check: zeno-fcis/postcondition-report/1 with authority none, status and candidates[].status checked-discriminating, refused or refuted, plus no-discrimination rows"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"intent_bytes": 65536, "max_tuples": 4096, "mutants": 8}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Prototype, advisory only. Passing the contract does not establish detection, and detection does not establish that the condition captures the intent. Compilation failures and undefined outcomes never count as detected faults. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"domain-above-cap": "Narrow the declared domain; the prototype enumerates at most 4096 tuples.", "input-not-regular-file": "Pipes and FIFOs are refused without waiting.", "intent-too-large": "Keep the requirements file at or under 65536 bytes."}

## Worked example

```sh
printf 'x%.0s' $(seq 1 65537) > $WORK/too-long.txt && zeno-fcis contract postconditions prompt examples/guide-counter --intent $WORK/too-long.txt
```

Expected: exit 1; {"status": "refused"}.

## See also

Sources: {"engine": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs", "exit_codes": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs", "inputs": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs", "limits": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs", "outputs": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs", "refusals": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs", "synopsis": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs", "verdicts": "prototypes/postconditions/README.md; crates/zeno-fcis-cli/src/postconditions_command.rs; crates/zeno-fcis-cli/tests/postconditions_cli.rs"}

Run `zeno-fcis guide find "screen AI-proposed postconditions"` for related cards.
