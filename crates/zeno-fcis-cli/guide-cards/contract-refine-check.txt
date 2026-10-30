```json
{
  "name": "contract-refine-check",
  "title": "Check finite contract refinement through an abstraction table",
  "synopsis": "zeno-fcis contract refine-check ABSTRACT_DIR CONCRETE_DIR --abstraction FILE [--max-checks N] --format json",
  "engine": "Both rules-compiled contracts bound to the library Authority; every concrete state/input pair is compared with its abstract image: committing successors stay related, decision class, reason, effects and deliveries agree exactly, invalid states are refused by the named law.",
  "use_when": [
    "does a concrete representation refine an abstract contract",
    "check a data-structure change against the abstract rules",
    "representation refinement",
    "compare two contracts through an abstraction"
  ],
  "not_for": [
    "migration of a running store",
    "adopted programs or evolution histories",
    "values outside the declared domains or malformed encodings"
  ],
  "inputs": [
    "ABSTRACT_DIR and CONCRETE_DIR: each holds project.zeno and v2/policy.json, at most 1 MiB per file, at most 4096 states and 64 input tuples per contract. --abstraction: zeno-fcis/contract-refinement/1 with the four lowercase SHA-256 of the exact source bytes, `abstraction` (one abstract state ordinal or null per raw concrete state) and `invalid_state_law`. --max-checks (default 262144, maximum 262144) counts completed pair comparisons. Writes nothing."
  ],
  "outputs": [
    "stdout zeno-fcis/contract-refinement-result/1: status refines, counterexample or inconclusive, the smallest failing state and input ordinals, Authority invocation counts and summed resource usage as decimal strings; authority none"
  ],
  "exit_codes": {
    "0": "refines: every pair compared",
    "1": "counterexample: initial-state, coverage or pair failure",
    "2": "inconclusive: --max-checks exhausted, domain product too large, or a meter refusal",
    "3": "a source file cannot be read",
    "64": "invalid command-line usage",
    "65": "malformed or unsupported intake: digests, layouts, bounds or a comparison limit above 262144"
  },
  "verdicts": {
    "refines": "Finite contract refinement on the declared domains only; authority none. No deployment, publication or store update is authorised.",
    "counterexample": "Initial-state and coverage failures come first; then the smallest state ordinal, then input ordinal; within a row unchanged-rejection, successor, then observation.",
    "inconclusive": "Exhaustion never reports refines; raise --max-checks up to 262144 or reduce the domain product."
  },
  "limits": {
    "max_checks_default": 262144,
    "max_checks_maximum": 262144,
    "states_per_contract": 4096,
    "input_tuples": 64,
    "source_bytes": 1048576
  },
  "refusals": {
    "digest-mismatch": "Recompute the four SHA-256 values over the exact UTF-8 bytes, whitespace included.",
    "layout-mismatch": "State roots must be records; command/context layouts, type IDs and domains must match."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry",
    "example_pairs": "reservation example: 64 concrete states x 6 commands = 384 pairs"
  },
  "steps": [
    {
      "command": "zeno-fcis contract refine-check examples/contract-refinement/reservation/abstract examples/contract-refinement/reservation/concrete --abstraction examples/contract-refinement/reservation/abstraction.json --max-checks 384 --format json",
      "exit": 0,
      "expected": {
        "status": "refines"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract refine-check examples/contract-refinement/reservation/abstract examples/contract-refinement/reservation/concrete --abstraction examples/contract-refinement/reservation/abstraction.json --max-checks 0 --format json",
    "exit": 2,
    "expected": {
      "status": "inconclusive"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs",
    "engine": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs",
    "inputs": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs",
    "outputs": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs",
    "exit_codes": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs",
    "verdicts": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs",
    "limits": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs",
    "refusals": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs"
  },
  "commands": [
    "contract refine-check"
  ],
  "pitfalls": "The abstract domain is the complete abstract carrier; every abstract state needs a concrete representative. Even a zero budget checks genesis, the initial mapping and coverage, but exhaustion is never refines. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Check finite contract refinement through an abstraction table

## Use when

does a concrete representation refine an abstract contract; check a data-structure change against the abstract rules; representation refinement; compare two contracts through an abstraction.

## Not for

migration of a running store; adopted programs or evolution histories; values outside the declared domains or malformed encodings.

## Inputs

ABSTRACT_DIR and CONCRETE_DIR: each holds project.zeno and v2/policy.json, at most 1 MiB per file, at most 4096 states and 64 input tuples per contract. --abstraction: zeno-fcis/contract-refinement/1 with the four lowercase SHA-256 of the exact source bytes, `abstraction` (one abstract state ordinal or null per raw concrete state) and `invalid_state_law`. --max-checks (default 262144, maximum 262144) counts completed pair comparisons. Writes nothing.

Engine: Both rules-compiled contracts bound to the library Authority; every concrete state/input pair is compared with its abstract image: committing successors stay related, decision class, reason, effects and deliveries agree exactly, invalid states are refused by the named law.

Synopsis: "zeno-fcis contract refine-check ABSTRACT_DIR CONCRETE_DIR --abstraction FILE [--max-checks N] --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract refine-check examples/contract-refinement/reservation/abstract examples/contract-refinement/reservation/concrete --abstraction examples/contract-refinement/reservation/abstraction.json --max-checks 384 --format json
```

Expected: exit 0; {"status": "refines"}.

## Verdicts

`refines`: Finite contract refinement on the declared domains only; authority none. No deployment, publication or store update is authorised.

`counterexample`: Initial-state and coverage failures come first; then the smallest state ordinal, then input ordinal; within a row unchanged-rejection, successor, then observation.

`inconclusive`: Exhaustion never reports refines; raise --max-checks up to 262144 or reduce the domain product.

Exit codes: {"0": "refines: every pair compared", "1": "counterexample: initial-state, coverage or pair failure", "2": "inconclusive: --max-checks exhausted, domain product too large, or a meter refusal", "3": "a source file cannot be read", "64": "invalid command-line usage", "65": "malformed or unsupported intake: digests, layouts, bounds or a comparison limit above 262144"}

Outputs: ["stdout zeno-fcis/contract-refinement-result/1: status refines, counterexample or inconclusive, the smallest failing state and input ordinals, Authority invocation counts and summed resource usage as decimal strings; authority none"]

## Cost

{"example_pairs": "reservation example: 64 concrete states x 6 commands = 384 pairs", "example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"input_tuples": 64, "max_checks_default": 262144, "max_checks_maximum": 262144, "source_bytes": 1048576, "states_per_contract": 4096}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

The abstract domain is the complete abstract carrier; every abstract state needs a concrete representative. Even a zero budget checks genesis, the initial mapping and coverage, but exhaustion is never refines. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"digest-mismatch": "Recompute the four SHA-256 values over the exact UTF-8 bytes, whitespace included.", "layout-mismatch": "State roots must be records; command/context layouts, type IDs and domains must match."}

## Worked example

```sh
zeno-fcis contract refine-check examples/contract-refinement/reservation/abstract examples/contract-refinement/reservation/concrete --abstraction examples/contract-refinement/reservation/abstraction.json --max-checks 0 --format json
```

Expected: exit 2; {"status": "inconclusive"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs", "exit_codes": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs", "inputs": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs", "limits": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs", "outputs": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs", "refusals": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs", "synopsis": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs", "verdicts": "docs/CLI_REFERENCE.md:1669-1751; docs/CONSTRUCTION.md; crates/zeno-fcis-cli/tests/contract_refine_check.rs"}

Run `zeno-fcis guide find "does a concrete representation refine an abstract contract"` for related cards.
