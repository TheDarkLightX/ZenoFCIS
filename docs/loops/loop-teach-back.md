```json
{
  "name": "loop-teach-back",
  "title": "Check a restatement against finite contract decisions",
  "synopsis": "zeno-fcis loop teach-back --contract DIR --rule JSON [--answers JSON]",
  "engine": "Door evaluator compares restated rules with the core on all declared inputs and reachable states, with optional requirement-admissibility analysis.",
  "use_when": [
    "check a restatement",
    "teach back a contract",
    "find exceptions in a stated rule"
  ],
  "not_for": [
    "changing laws",
    "equivalence receipts",
    "assuming exit zero means every rule is right"
  ],
  "inputs": [
    "--contract: directory with project.zeno and v2/policy.json. --rule: zeno-fcis/loop-rules/1 with rules[] using id, where, class (Accept/Reject/CommittedFailure), reason and optional sets (successor formulas). Reject/failure rules require a known reason. --answers optionally supplies zeno-fcis/loop-answers/1. Each --rule/--answers JSON document is at most 65,536 bytes."
  ],
  "outputs": [
    "stdout zeno-fcis/loop-doors/2, door=teach-back, authority=none, status=judged and detail.rules/detail.summary. Rules include verdicts, exceptions/missing regions, reachable-state coverage and optional required/open/forbidden counts. No files, sessions, attempts or equivalence receipts are produced.",
    "Where integer_encoding is present, scalar report integers use safe-number-or-decimal-string/1. Program JSON bounds/literals remain decimal strings; node references remain JSON integers."
  ],
  "exit_codes": {
    "0": "Completed report; read the operation-specific verdict and scope.",
    "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.",
    "2": "Inconclusive, incomplete or unavailable containment; no new acceptance.",
    "3": "I/O failure; see operation-specific exceptions.",
    "64": "Invalid command-line usage."
  },
  "verdicts": {
    "judged": "Exit 0 only when all rules are exact/holds; wrong or refused rules exit 1, and an empty or vacuous/incomplete rule set exits 2. Read every detail.rules[].verdict and summary.complete.",
    "exact/holds/wrong/vacuous": "Rule-level classifications; holds can miss cases, wrong provides exceptions, vacuous has no active cases.",
    "refused": "Exit 1 for an invalid input or unsupported core.",
    "inconclusive/out-of-scope": "Exit 2; required/open/forbidden analysis can also be omitted with summary.required.not_computed."
  },
  "limits": {
    "document_bytes": 65536,
    "core_tuples": 131072,
    "admissibility_checks": 4000000,
    "search_stage_ms": 5000,
    "invocation_ms": 22000
  },
  "refusals": {
    "rule syntax": "Use the supported fields/formulas; preserve precedence in where.",
    "admissibility budget": "Read not_computed; do not treat missing counts as zero."
  },
  "cost": {
    "units": "Read reported work reservations and scope; these are not total CPU-time measurements.",
    "timing": "No cross-host timing claim."
  },
  "steps": [
    {
      "command": "cat > \"$WORK/rules.json\" <<'DATA'\n{\n  \"schema\": \"zeno-fcis/loop-rules/1\",\n  \"rules\": [\n    {\n      \"id\": \"authorization\",\n      \"where\": \"!authorized\",\n      \"class\": \"Reject\",\n      \"reason\": \"not_authorized\"\n    }\n  ]\n}\nDATA",
      "exit": 0,
      "expected": {},
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis loop teach-back --contract examples/guide-counter --rule \"$WORK/rules.json\"",
    "exit": 0,
    "expected": {
      "status": "judged",
      "detail.rules.0.verdict": "exact"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs",
    "engine": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs",
    "inputs": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs",
    "outputs": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs",
    "exit_codes": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs",
    "verdicts": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs",
    "limits": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs",
    "refusals": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs"
  },
  "commands": [
    "loop teach-back"
  ],
  "pitfalls": "Rule agreement checks the implemented finite core; it does not establish owner intent or adopt the restatement. The judged status retains each rule verdict: wrong/refused exits 1 and vacuous/incomplete exits 2."
}
```

# Check a restatement against finite contract decisions

## Use when

check a restatement; teach back a contract; find exceptions in a stated rule.

## Not for

changing laws; equivalence receipts; assuming exit zero means every rule is right.

## Inputs

--contract: directory with project.zeno and v2/policy.json. --rule: zeno-fcis/loop-rules/1 with rules[] using id, where, class (Accept/Reject/CommittedFailure), reason and optional sets (successor formulas). Reject/failure rules require a known reason. --answers optionally supplies zeno-fcis/loop-answers/1. Each --rule/--answers JSON document is at most 65,536 bytes.

Engine: Door evaluator compares restated rules with the core on all declared inputs and reachable states, with optional requirement-admissibility analysis.

Synopsis: `zeno-fcis loop teach-back --contract DIR --rule JSON [--answers JSON]`

Run from the repository root with zeno-fcis on PATH. Set WORK to a fresh writable scratch directory outside the source tree.



## Steps

```sh
cat > "$WORK/rules.json" <<'DATA'
{
  "schema": "zeno-fcis/loop-rules/1",
  "rules": [
    {
      "id": "authorization",
      "where": "!authorized",
      "class": "Reject",
      "reason": "not_authorized"
    }
  ]
}
DATA
```

Expected: exit 0; {}.

## Verdicts

`judged`: Exit 0 only when all rules are exact/holds; wrong or refused rules exit 1, and an empty or vacuous/incomplete rule set exits 2. Read every detail.rules[].verdict and summary.complete.

`exact/holds/wrong/vacuous`: Rule-level classifications; holds can miss cases, wrong provides exceptions, vacuous has no active cases.

`refused`: Exit 1 for an invalid input or unsupported core.

`inconclusive/out-of-scope`: Exit 2; required/open/forbidden analysis can also be omitted with summary.required.not_computed.

Exit codes: {"0": "Completed report; read the operation-specific verdict and scope.", "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.", "2": "Inconclusive, incomplete or unavailable containment; no new acceptance.", "3": "I/O failure; see operation-specific exceptions.", "64": "Invalid command-line usage."}

Outputs: stdout zeno-fcis/loop-doors/2, door=teach-back, authority=none, status=judged and detail.rules/detail.summary. Rules include verdicts, exceptions/missing regions, reachable-state coverage and optional required/open/forbidden counts. No files, sessions, attempts or equivalence receipts are produced.

## Cost

Read reported work reservations and scope; these are not total CPU-time measurements.

Limits: {"admissibility_checks": 4000000, "core_tuples": 131072, "document_bytes": 65536, "invocation_ms": 22000, "search_stage_ms": 5000}

## Coordinate

Retain the command, exit status, verdict, declared scope and supporting receipt when passing a result to another workflow. Reports and suggestions do not grant adoption or publication authority.

## Pitfalls

Rule agreement checks the implemented finite core; it does not establish owner intent or adopt the restatement. The judged status retains each rule verdict: wrong/refused exits 1 and vacuous/incomplete exits 2.

Refusals and next steps: {"admissibility budget": "Read not_computed; do not treat missing counts as zero.", "rule syntax": "Use the supported fields/formulas; preserve precedence in where."}

## Worked example

```sh
zeno-fcis loop teach-back --contract examples/guide-counter --rule "$WORK/rules.json"
```

Expected: exit 0; {"detail.rules.0.verdict": "exact", "status": "judged"}.

## See also

**Background.** Edmund M. Clarke, E. Allen Emerson and A. Prasad Sistla, “Automatic Verification of Finite-State Concurrent Systems Using Temporal Logic Specifications,” ACM TOPLAS 8(2), 1986, is background for exhaustive finite-state checking.

**What is new here.** This card documents the implemented interface; it adds no algorithm or priority claim.

**Scope.** Rule agreement checks the implemented finite core; it does not establish owner intent or adopt the restatement. The judged status retains each rule verdict: wrong/refused exits 1 and vacuous/incomplete exits 2.

Sources: crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs

[CLI reference](../CLI_REFERENCE.md); [loop overview](loop-workspace.md).
