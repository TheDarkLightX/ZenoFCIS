```json
{
  "name": "contract-two-run",
  "title": "Compare two runs: order independence or noninterference",
  "synopsis": "zeno-fcis contract two-run DIR --commute --first P --second Q --observe PATHS [--within K] | --noninterference --secret PATHS --public PATHS",
  "engine": "Exhaustive two-run comparison over the contract's declared finite domain through the bound library Authority: both command orders (commute) or both secret values (noninterference) on every input, observations compared exactly or within K.",
  "use_when": [
    "order independence of two commands",
    "front-running or reordering sensitivity",
    "noninterference of a secret field",
    "does a public observation reveal a private value"
  ],
  "not_for": [
    "a property over runs of unbounded length (use contract runs)",
    "domains above --max-tuples (inconclusive, not sampled)",
    "a general front-running-resistance claim"
  ],
  "inputs": [
    "DIR: application or contract directory holding project.zeno and v2/policy.json. --commute: --first and --second select the two commands by predicates over command fields, --observe lists post-state paths compared after both orders, --within K allows an absolute difference of at most K per observed integer. --noninterference: --secret lists pre-state paths that may differ, --public lists post-state paths that must not. --max-tuples (default 1048576) caps the enumerated domain. --out FILE writes the packet and prints the summary; without it the packet is stdout and the summary stderr."
  ],
  "outputs": [
    "zeno-fcis/two-run/1 packet: status checked, refuted or inconclusive, the property, the compared domain and the first witness pair when refuted; authority none"
  ],
  "exit_codes": {
    "0": "checked on the complete declared domain",
    "1": "refuted: a witness pair differs",
    "2": "UNKNOWN: domain above --max-tuples or another limit; no acceptance",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage",
    "65": "a predicate or path has no contract form"
  },
  "verdicts": {
    "checked": "The property holds on every input of the declared finite domain. Advisory; it covers that domain and those observations only.",
    "refuted": "Read the witness pair: the two orders or two secret values and the differing observation. Raising K cannot repair a decision-class difference.",
    "inconclusive": "The domain exceeded --max-tuples or the comparison hit a limit; raise --max-tuples or shrink the declared domain."
  },
  "limits": {
    "max_tuples_default": 1048576,
    "within": "u128, --commute only",
    "other_caps": "unknown"
  },
  "refusals": {
    "predicate-not-contract-form": "Write --first/--second over command paths such as command.101.120 == 0; exit 65.",
    "observe-paths": "Use post.<type>.<field> paths for --observe and --public, pre.<type>.<field> for --secret."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract two-run examples/two-run/uniform-price --commute --first \"command.101.120 == 0\" --second \"command.101.120 == 1\" --observe post.100.110,post.100.111,post.100.112,post.100.113,post.100.114 --out $WORK/batch.json",
      "exit": 0,
      "expected": {},
      "contains": [
        "checked"
      ]
    }
  ],
  "example": {
    "command": "zeno-fcis contract two-run examples/two-run/ledger-leak --noninterference --secret pre.100.112 --public post.100.110,post.100.111 --out $WORK/ledger-leak.json",
    "exit": 1,
    "expected": {},
    "contains": [
      "refuted"
    ]
  },
  "sources": {
    "synopsis": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py",
    "engine": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py",
    "inputs": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py",
    "outputs": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py",
    "exit_codes": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py",
    "verdicts": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py",
    "limits": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py",
    "refusals": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py"
  },
  "commands": [
    "contract two-run"
  ],
  "pitfalls": "Refusals at a declared capacity are part of the comparison: a request refused in only one order is a difference. The check covers the declared finite domain and the listed observations; equal totals do not imply equal individual allocations unless each is observed. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Compare two runs: order independence or noninterference

## Use when

order independence of two commands; front-running or reordering sensitivity; noninterference of a secret field; does a public observation reveal a private value.

## Not for

a property over runs of unbounded length (use contract runs); domains above --max-tuples (inconclusive, not sampled); a general front-running-resistance claim.

## Inputs

DIR: application or contract directory holding project.zeno and v2/policy.json. --commute: --first and --second select the two commands by predicates over command fields, --observe lists post-state paths compared after both orders, --within K allows an absolute difference of at most K per observed integer. --noninterference: --secret lists pre-state paths that may differ, --public lists post-state paths that must not. --max-tuples (default 1048576) caps the enumerated domain. --out FILE writes the packet and prints the summary; without it the packet is stdout and the summary stderr.

Engine: Exhaustive two-run comparison over the contract's declared finite domain through the bound library Authority: both command orders (commute) or both secret values (noninterference) on every input, observations compared exactly or within K.

Synopsis: "zeno-fcis contract two-run DIR --commute --first P --second Q --observe PATHS [--within K] | --noninterference --secret PATHS --public PATHS"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract two-run examples/two-run/uniform-price --commute --first "command.101.120 == 0" --second "command.101.120 == 1" --observe post.100.110,post.100.111,post.100.112,post.100.113,post.100.114 --out $WORK/batch.json
```

Expected: exit 0; ["checked"].

## Verdicts

`checked`: The property holds on every input of the declared finite domain. Advisory; it covers that domain and those observations only.

`refuted`: Read the witness pair: the two orders or two secret values and the differing observation. Raising K cannot repair a decision-class difference.

`inconclusive`: The domain exceeded --max-tuples or the comparison hit a limit; raise --max-tuples or shrink the declared domain.

Exit codes: {"0": "checked on the complete declared domain", "1": "refuted: a witness pair differs", "2": "UNKNOWN: domain above --max-tuples or another limit; no acceptance", "3": "filesystem or execution failure", "64": "invalid command-line usage", "65": "a predicate or path has no contract form"}

Outputs: ["zeno-fcis/two-run/1 packet: status checked, refuted or inconclusive, the property, the compared domain and the first witness pair when refuted; authority none"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"max_tuples_default": 1048576, "other_caps": "unknown", "within": "u128, --commute only"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Refusals at a declared capacity are part of the comparison: a request refused in only one order is a difference. The check covers the declared finite domain and the listed observations; equal totals do not imply equal individual allocations unless each is observed. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"observe-paths": "Use post.<type>.<field> paths for --observe and --public, pre.<type>.<field> for --secret.", "predicate-not-contract-form": "Write --first/--second over command paths such as command.101.120 == 0; exit 65."}

## Worked example

```sh
zeno-fcis contract two-run examples/two-run/ledger-leak --noninterference --secret pre.100.112 --public post.100.110,post.100.111 --out $WORK/ledger-leak.json
```

Expected: exit 1; ["refuted"].

## See also

Sources: {"engine": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py", "exit_codes": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py", "inputs": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py", "limits": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py", "outputs": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py", "refusals": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py", "synopsis": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py", "verdicts": "docs/TWO_RUN_PROPERTIES.md; crates/zeno-fcis-cli/src/judge_command.rs TwoRun; tools/test_two_run.py"}

Run `zeno-fcis guide find "order independence of two commands"` for related cards.
