```json
{
  "name": "contract-check-composition",
  "title": "Check a composition of separately bound contracts",
  "synopsis": "zeno-fcis contract check-composition MANIFEST [--receipt FILE | --replay FILE] --format json",
  "engine": "Each leaf compiled as generate contract does and bound to the library Authority over its complete declared domain; local, assume and couple obligations planned with exact sizes and checked exhaustively; the compose crate judges the denoted composition specification.",
  "use_when": [
    "check a composition of contracts",
    "interface edges between leaf contracts",
    "do the leaves' guarantees compose",
    "replay a composition receipt"
  ],
  "not_for": [
    "effects or outbox deliveries across leaves (the profile has none)",
    "domains above 2^20 inputs per leaf (refused, not sampled)",
    "runtime deployment of the composition"
  ],
  "inputs": [
    "MANIFEST: zeno-fcis/checked-composition/1 or, with groups, /2; names each leaf directory relative to the manifest with the SHA-256 of its project.zeno, v2/policy.json and bound program identity, its invariant, assumption, guarantees (StateFact or TransitionRelation), owned slots, footprint, exported views and the interface edges. --receipt FILE writes the check receipt; --replay FILE replays one. --aggregate DIR / --write-aggregate DIR for the aggregate domain manifests (see the reference)."
  ],
  "outputs": [
    "stdout zeno-fcis/checked-composition-report/1 (see CLI_REFERENCE): status checked, refused, replayed or replay-mismatch, the obligations with their sizes and evaluated counts; authority none"
  ],
  "exit_codes": {
    "0": "checked, or replayed against a matching receipt",
    "1": "refused (over budget, digest or manifest failure, a failing obligation) or replay-mismatch",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage, including --receipt with --replay"
  },
  "verdicts": {
    "checked": "Every planned obligation was evaluated exhaustively within budget on the declared domains; sequential profile only.",
    "refused": "Over max_obligation (2^22) or max_total (2^25), a leaf domain above 2^20, a digest mismatch or a failing obligation; nothing is evaluated past the first refusal reason.",
    "replayed": "The receipt matches a fresh check of the same manifest.",
    "replay-mismatch": "The manifest or a leaf changed since the receipt; rerun the check and write a new receipt."
  },
  "limits": {
    "leaf_domain_inputs": 1048576,
    "max_obligation": 4194304,
    "max_total": 33554432
  },
  "refusals": {
    "aggregate-directory-refused": "Pass an empty or absent --write-aggregate directory; existing contents are never overwritten.",
    "digest-mismatch": "Recompute the leaf SHA-256 values in the manifest after any edit to a leaf."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract check-composition crates/zeno-fcis-contract/tests/fixtures/checked-composition/register-ring-n2/composition.json --format json",
      "exit": 0,
      "expected": {
        "status": "checked"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract check-composition crates/zeno-fcis-contract/tests/fixtures/checked-composition/register-ring-n2/composition.json --receipt $WORK/composition-receipt.json --format json",
    "exit": 0,
    "expected": {
      "status": "checked"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs",
    "engine": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs",
    "inputs": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs",
    "outputs": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs",
    "exit_codes": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs",
    "verdicts": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs",
    "limits": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs",
    "refusals": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs"
  },
  "commands": [
    "contract check-composition"
  ],
  "pitfalls": "One leaf decides each command; peers are seen only through views of the same pre-state. A refusal evaluates nothing (evaluated: 0) and is not a refutation. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Check a composition of separately bound contracts

## Use when

check a composition of contracts; interface edges between leaf contracts; do the leaves' guarantees compose; replay a composition receipt.

## Not for

effects or outbox deliveries across leaves (the profile has none); domains above 2^20 inputs per leaf (refused, not sampled); runtime deployment of the composition.

## Inputs

MANIFEST: zeno-fcis/checked-composition/1 or, with groups, /2; names each leaf directory relative to the manifest with the SHA-256 of its project.zeno, v2/policy.json and bound program identity, its invariant, assumption, guarantees (StateFact or TransitionRelation), owned slots, footprint, exported views and the interface edges. --receipt FILE writes the check receipt; --replay FILE replays one. --aggregate DIR / --write-aggregate DIR for the aggregate domain manifests (see the reference).

Engine: Each leaf compiled as generate contract does and bound to the library Authority over its complete declared domain; local, assume and couple obligations planned with exact sizes and checked exhaustively; the compose crate judges the denoted composition specification.

Synopsis: "zeno-fcis contract check-composition MANIFEST [--receipt FILE | --replay FILE] --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract check-composition crates/zeno-fcis-contract/tests/fixtures/checked-composition/register-ring-n2/composition.json --format json
```

Expected: exit 0; {"status": "checked"}.

## Verdicts

`checked`: Every planned obligation was evaluated exhaustively within budget on the declared domains; sequential profile only.

`refused`: Over max_obligation (2^22) or max_total (2^25), a leaf domain above 2^20, a digest mismatch or a failing obligation; nothing is evaluated past the first refusal reason.

`replayed`: The receipt matches a fresh check of the same manifest.

`replay-mismatch`: The manifest or a leaf changed since the receipt; rerun the check and write a new receipt.

Exit codes: {"0": "checked, or replayed against a matching receipt", "1": "refused (over budget, digest or manifest failure, a failing obligation) or replay-mismatch", "3": "filesystem or execution failure", "64": "invalid command-line usage, including --receipt with --replay"}

Outputs: ["stdout zeno-fcis/checked-composition-report/1 (see CLI_REFERENCE): status checked, refused, replayed or replay-mismatch, the obligations with their sizes and evaluated counts; authority none"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"leaf_domain_inputs": 1048576, "max_obligation": 4194304, "max_total": 33554432}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

One leaf decides each command; peers are seen only through views of the same pre-state. A refusal evaluates nothing (evaluated: 0) and is not a refutation. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"aggregate-directory-refused": "Pass an empty or absent --write-aggregate directory; existing contents are never overwritten.", "digest-mismatch": "Recompute the leaf SHA-256 values in the manifest after any edit to a leaf."}

## Worked example

```sh
zeno-fcis contract check-composition crates/zeno-fcis-contract/tests/fixtures/checked-composition/register-ring-n2/composition.json --receipt $WORK/composition-receipt.json --format json
```

Expected: exit 0; {"status": "checked"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs", "exit_codes": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs", "inputs": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs", "limits": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs", "outputs": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs", "refusals": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs", "synopsis": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs", "verdicts": "docs/CLI_REFERENCE.md:601-823; docs/COMPOSED_DOMAIN_COMPOSITION.md; crates/zeno-fcis-cli/tests/contract_check_composition.rs"}

Run `zeno-fcis guide find "check a composition of contracts"` for related cards.
