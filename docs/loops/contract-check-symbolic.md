```json
{
  "name": "contract-check-symbolic",
  "title": "Check contract rule cases symbolically with CVC5 and Z3",
  "synopsis": "zeno-fcis contract check-symbolic DIR [--strengthening FILE] [--tools FILE] [--max-tuples N] [--out REPORT] --format json",
  "engine": "One query per rule case to the pinned CVC5 and Z3 for domains too large to enumerate; a domain within --max-tuples is enumerated exhaustively instead; every solver counterexample is replayed through the library evaluators before it is reported.",
  "use_when": [
    "check rule cases on a large domain",
    "symbolic check of a contract",
    "solver evidence for a contract's cases",
    "strengthen cases with a hypothesis file"
  ],
  "not_for": [
    "program equivalence (use transform check --symbolic)",
    "an independently checked proof (use transform check-certificates)",
    "a domain that fits enumeration when you need a proved result: it is enumerated anyway"
  ],
  "inputs": [
    "DIR: contract directory with project.zeno and v2/policy.json. --strengthening: optional hypotheses file (format in SYMBOLIC_CHECKS.md). --tools: zeno-fcis/tools/2 manifest naming cvc5 and z3 with versions and SHA-256; without solvers only enumeration can decide. --max-tuples: domains at or below it are enumerated. --out writes the zeno-fcis/symbolic-check/1 report and prints a summary; --queries DIR retains the SMT queries."
  ],
  "outputs": [
    "zeno-fcis/symbolic-check/1 report: status holds, refuted or inconclusive, evidence proved or attested, domain route, per-case targets with solver timings; authority none"
  ],
  "exit_codes": {
    "0": "holds by exhaustive enumeration of the whole domain (evidence proved)",
    "1": "refuted: a replayed counterexample (evidence checked)",
    "2": "holds attested by CVC5 and corroborated by Z3 but not proved, or inconclusive: undecided, solver disagreement without a replayed model, unavailable tools, or a planted control not refuted",
    "3": "filesystem or execution failure",
    "64": "invalid command-line usage"
  },
  "verdicts": {
    "holds": "Exit 0 only when the whole domain was enumerated. From the solvers it is attested, not proved, and exits 2 as a CVC5 unsat does for prove.",
    "refuted": "The counterexample replayed through the library evaluators; fix the case and rerun.",
    "inconclusive": "Supply the pinned tools, reduce the domain, or read the undecided targets; a planted control that was not refuted also lands here."
  },
  "limits": {
    "max_tuples": "enumeration threshold; see --help default",
    "other_caps": "unknown"
  },
  "refusals": {
    "tools-manifest-blocked": "Point --tools at a zeno-fcis/tools/2 manifest whose cvc5 and z3 entries pass the version line and SHA-256 admission.",
    "report-write-failed": "Choose a writable --out path outside the source tree."
  },
  "cost": {
    "measured_units": "unknown",
    "example_wall_ms": "unknown",
    "telemetry": "PENDING: populate from source-bound invocation telemetry"
  },
  "steps": [
    {
      "command": "zeno-fcis contract check-symbolic crates/zeno-fcis-cli/tests/fixtures/spend-approval",
      "exit": 0,
      "expected": {
        "status": "holds",
        "evidence": "proved"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis contract check-symbolic crates/zeno-fcis-cli/tests/fixtures/spend-approval --out $WORK/symbolic.json --format json",
    "exit": 0,
    "expected": {
      "status": "holds"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs",
    "engine": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs",
    "inputs": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs",
    "outputs": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs",
    "exit_codes": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs",
    "verdicts": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs",
    "limits": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs",
    "refusals": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs"
  },
  "commands": [
    "contract check-symbolic"
  ],
  "pitfalls": "Solver holds is attested evidence (exit 2), never a proof; only enumeration or the certificate route gives proved or independently checked results. Receipts from the symbolic route are not transform receipts: adopt, refresh-receipts and generation refuse them. Read the result scope before revising the input. No report grants runtime or publication authority."
}
```

# Check contract rule cases symbolically with CVC5 and Z3

## Use when

check rule cases on a large domain; symbolic check of a contract; solver evidence for a contract's cases; strengthen cases with a hypothesis file.

## Not for

program equivalence (use transform check --symbolic); an independently checked proof (use transform check-certificates); a domain that fits enumeration when you need a proved result: it is enumerated anyway.

## Inputs

DIR: contract directory with project.zeno and v2/policy.json. --strengthening: optional hypotheses file (format in SYMBOLIC_CHECKS.md). --tools: zeno-fcis/tools/2 manifest naming cvc5 and z3 with versions and SHA-256; without solvers only enumeration can decide. --max-tuples: domains at or below it are enumerated. --out writes the zeno-fcis/symbolic-check/1 report and prints a summary; --queries DIR retains the SMT queries.

Engine: One query per rule case to the pinned CVC5 and Z3 for domains too large to enumerate; a domain within --max-tuples is enumerated exhaustively instead; every solver counterexample is replayed through the library evaluators before it is reported.

Synopsis: "zeno-fcis contract check-symbolic DIR [--strengthening FILE] [--tools FILE] [--max-tuples N] [--out REPORT] --format json"

Set WORK to a fresh writable directory outside the source tree. Run commands from the repository root with zeno-fcis on PATH.

## Steps

```sh
zeno-fcis contract check-symbolic crates/zeno-fcis-cli/tests/fixtures/spend-approval
```

Expected: exit 0; {"evidence": "proved", "status": "holds"}.

## Verdicts

`holds`: Exit 0 only when the whole domain was enumerated. From the solvers it is attested, not proved, and exits 2 as a CVC5 unsat does for prove.

`refuted`: The counterexample replayed through the library evaluators; fix the case and rerun.

`inconclusive`: Supply the pinned tools, reduce the domain, or read the undecided targets; a planted control that was not refuted also lands here.

Exit codes: {"0": "holds by exhaustive enumeration of the whole domain (evidence proved)", "1": "refuted: a replayed counterexample (evidence checked)", "2": "holds attested by CVC5 and corroborated by Z3 but not proved, or inconclusive: undecided, solver disagreement without a replayed model, unavailable tools, or a planted control not refuted", "3": "filesystem or execution failure", "64": "invalid command-line usage"}

Outputs: ["zeno-fcis/symbolic-check/1 report: status holds, refuted or inconclusive, evidence proved or attested, domain route, per-case targets with solver timings; authority none"]

## Cost

{"example_wall_ms": "unknown", "measured_units": "unknown", "telemetry": "PENDING: populate from source-bound invocation telemetry"}

Limits: {"max_tuples": "enumeration threshold; see --help default", "other_caps": "unknown"}

## Coordinate

Run Commons `advise` for the task first. Compute the exact subject digest, take an exclusive lease `loop:<subject digest>`, and respect existing leases. After checking, add a result record with the command, subject, verdict, scope and retained evidence; release the lease. Telemetry and advice grant no checker authority.

## Pitfalls

Solver holds is attested evidence (exit 2), never a proof; only enumeration or the certificate route gives proved or independently checked results. Receipts from the symbolic route are not transform receipts: adopt, refresh-receipts and generation refuse them. Read the result scope before revising the input. No report grants runtime or publication authority.

Refusals and remedies: {"report-write-failed": "Choose a writable --out path outside the source tree.", "tools-manifest-blocked": "Point --tools at a zeno-fcis/tools/2 manifest whose cvc5 and z3 entries pass the version line and SHA-256 admission."}

## Worked example

```sh
zeno-fcis contract check-symbolic crates/zeno-fcis-cli/tests/fixtures/spend-approval --out $WORK/symbolic.json --format json
```

Expected: exit 0; {"status": "holds"}.

## See also

Sources: {"engine": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs", "exit_codes": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs", "inputs": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs", "limits": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs", "outputs": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs", "refusals": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs", "synopsis": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs", "verdicts": "docs/CLI_REFERENCE.md:577-600; docs/SYMBOLIC_CHECKS.md; crates/zeno-fcis-cli/tests/symbolic_cli.rs"}

Run `zeno-fcis guide find "check rule cases on a large domain"` for related cards.
