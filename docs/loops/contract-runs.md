```json
{
  "name": "contract-runs",
  "title": "Check run properties over runs of any length",
  "synopsis": "zeno-fcis contract runs DIR [--file PROPERTIES] [--max-tuples N]",
  "engine": "Declared bounded observers (delivery counters saturating at two, ordering flags, predicates) paired with the contract; each property is checked inductively over the reachable states of the contract-observer product, exhaustive over the declared finite domains.",
  "use_when": [
    "at most once delivery",
    "never after ordering of two channels",
    "a property over runs of any length",
    "a seller is paid at most once"
  ],
  "not_for": [
    "unbounded domains or other parameter choices",
    "shell replay keys or external payment completion",
    "two-run comparisons (use contract two-run)"
  ],
  "inputs": [
    "DIR: application or contract directory. --file: zeno-fcis/run-properties/1 declarations, default v2/run-properties.json, with properties of kind at_most_once (channel, optional key_field over a finite domain of at most 64 values), never_after (a, b channels) and predicate (formula over outbox counts and post paths). --max-tuples (default 1048576) caps the observer product; above it a property is inconclusive. --out FILE writes the packet."
  ],
  "outputs": [
    "zeno-fcis/contract-runs/1 packet (see CONTRACT_JUDGES.md): per property status inductive, refuted or inconclusive with a witness run when refuted; authority none"
  ],
  "exit_codes": {
    "0": "every property inductive",
    "1": "a property is refuted by a reachable run",
    "2": "a property is inconclusive: observer product above --max-tuples or another limit",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage",
    "65": "a property or the contract has no contract form"
  },
  "verdicts": {
    "inductive": "Proved inductively over the reachable states of this contract paired with its observer, exhaustive over the declared finite domains; holds for runs of any length in that finite model.",
    "refuted": "Read the witness run; the property fails on a reachable sequence of decisions.",
    "inconclusive": "Raise --max-tuples to at least the reported product size, or narrow the observer (key_field domain)."
  },
  "limits": {
    "max_tuples_default": 1048576,
    "key_field_domain": 64,
    "other_caps": "unknown"
  },
  "refusals": {
    "properties-file-read-failed": "Supply --file or create v2/run-properties.json with schema zeno-fcis/run-properties/1."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry",
    "example_tuples": "escrow-deal payout observer declares 1244160 tuples"
  },
  "steps": [
    {
      "command": "zeno-fcis contract runs examples/apps/escrow-deal --max-tuples 2000000 --out $WORK/runs.json",
      "exit": 0,
      "expected": {},
      "contains": [
        "inductive"
      ]
    }
  ],
  "example": {
    "command": "zeno-fcis contract runs examples/apps/escrow-deal --out $WORK/runs-default.json",
    "exit": 2,
    "expected": {},
    "contains": [
      "inconclusive"
    ]
  },
  "sources": {
    "synopsis": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json",
    "engine": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json",
    "inputs": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json",
    "outputs": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json",
    "exit_codes": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json",
    "verdicts": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json",
    "limits": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json",
    "refusals": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json"
  },
  "commands": [
    "contract runs"
  ],
  "pitfalls": "A delivery is an admitted outbox obligation; the destination's payment and retry behaviour needs its own assurance. The default limit makes the escrow example inconclusive; the verdict then claims nothing. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Check run properties over runs of any length

## Use when

at most once delivery; never after ordering of two channels; a property over runs of any length; a seller is paid at most once.

## Not for

unbounded domains or other parameter choices; shell replay keys or external payment completion; two-run comparisons (use contract two-run).

## Inputs

DIR: application or contract directory. --file: zeno-fcis/run-properties/1 declarations, default v2/run-properties.json, with properties of kind at_most_once (channel, optional key_field over a finite domain of at most 64 values), never_after (a, b channels) and predicate (formula over outbox counts and post paths). --max-tuples (default 1048576) caps the observer product; above it a property is inconclusive. --out FILE writes the packet.

Engine: Declared bounded observers (delivery counters saturating at two, ordering flags, predicates) paired with the contract; each property is checked inductively over the reachable states of the contract-observer product, exhaustive over the declared finite domains.

Synopsis: "zeno-fcis contract runs DIR [--file PROPERTIES] [--max-tuples N]"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract runs examples/apps/escrow-deal --max-tuples 2000000 --out $WORK/runs.json
```

Expected: exit 0; ["inductive"].

## Verdicts

`inductive`: Proved inductively over the reachable states of this contract paired with its observer, exhaustive over the declared finite domains; holds for runs of any length in that finite model.

`refuted`: Read the witness run; the property fails on a reachable sequence of decisions.

`inconclusive`: Raise --max-tuples to at least the reported product size, or narrow the observer (key_field domain).

Exit codes: {"0": "every property inductive", "1": "a property is refuted by a reachable run", "2": "a property is inconclusive: observer product above --max-tuples or another limit", "3": "filesystem or execution failure", "64": "invalid command-line usage", "65": "a property or the contract has no contract form"}

Outputs: ["zeno-fcis/contract-runs/1 packet (see CONTRACT_JUDGES.md): per property status inductive, refuted or inconclusive with a witness run when refuted; authority none"]

## Cost

{"example_tuples": "escrow-deal payout observer declares 1244160 tuples", "example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"key_field_domain": 64, "max_tuples_default": 1048576, "other_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

A delivery is an admitted outbox obligation; the destination's payment and retry behaviour needs its own assurance. The default limit makes the escrow example inconclusive; the verdict then claims nothing. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"properties-file-read-failed": "Supply --file or create v2/run-properties.json with schema zeno-fcis/run-properties/1."}

## Worked example

```sh
zeno-fcis contract runs examples/apps/escrow-deal --out $WORK/runs-default.json
```

Expected: exit 2; ["inconclusive"].

## See also

Sources: {"engine": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json", "exit_codes": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json", "inputs": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json", "limits": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json", "outputs": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json", "refusals": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json", "synopsis": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json", "verdicts": "docs/CONTRACT_JUDGES.md:275-310; crates/zeno-fcis-cli/src/judge_command.rs Runs; examples/apps/escrow-deal/v2/run-properties.json"}

Run `zeno-fcis guide find "at most once delivery"` for related cards.
