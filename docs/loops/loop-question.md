```json
{
  "name": "loop-question",
  "title": "Ask where finite requirements leave choices open",
  "synopsis": "zeno-fcis loop question (--contract DIR [--answers JSON] | --specification JSON)",
  "engine": "Contract admissibility and answer-law checks, or bounded version-space enumeration followed by law evaluation and transform check/replay for a singleton.",
  "use_when": [
    "find owner questions",
    "ask about missing requirements",
    "distinguish finite sketch completions"
  ],
  "not_for": [
    "adopting an answer",
    "assuming determined means owner intent",
    "treating a question count as accepted code"
  ],
  "inputs": [
    "--contract reads a supported finite project.zeno and v2/policy.json directory. Optional --answers is zeno-fcis/loop-answers/1 with answers[]: where plus class Accept, Reject or CommittedFailure. Reject/failure answers require a known reason; sets maps state names to successor formulas, while case can supply case updates. For a committing answer, give complete deliveries (an array, [] for none, or \"case\"). Answers remain proposals.",
    "Alternatively --specification reads zeno-fcis/loop-version-space/1 with exactly schema, inputs, outputs, sketch and requirements. Named inputs have name/domain; Bool domains are kind=Bool, Int domains have signed-64-bit JSON integer min/max. Sketch strings contain finite holes, with no parameters or keep. Each requirement is exactly id/law or id/input/output; Bool example coordinates accept booleans or 0/1, other coordinates are signed-64-bit JSON integers. Each answer/specification JSON document <=65,536 bytes. The five --max-* flags apply to --specification. Contract mode accepts but ignores them and uses its own tuple/admissibility limits."
  ],
  "outputs": [
    "Contract stdout zeno-fcis/loop-doors/2, door=question: status, detail.stats, detail.question and detail.answers. Requirements stdout zeno-fcis/loop-version-space/2: status, counts, question/conflict and any checked singleton. Early requirements input errors can use transform-loop/1. authority=none.",
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
    "open/determined": "Contract open/determined reports exit 0 when answers agree; a disagreeing answer exits 1. Inspect every detail.answers[].status. Determination includes reason and delivery constraints.",
    "conflict/inconsistent/refused": "Contract exit 1; retain the conflicting input or fix the input.",
    "question/no-completion/determined": "Requirements determined exits 0, no-completion exits 1 and question exits 2. Question/count alone is advisory; singleton acceptance needs its explicit law/check/replay evidence.",
    "inconclusive/out-of-scope": "Exit 2; no claim beyond the retained completed evidence."
  },
  "limits": {
    "contract_tuples": 131072,
    "contract_admissibility_checks": 4000000,
    "spec_work": {
      "default": 2000000,
      "cap": 100000000
    },
    "spec_domain": {
      "default": 4096,
      "cap": 65536
    },
    "spec_meanings": {
      "default": 4096,
      "cap": 16384
    },
    "spec_cells": {
      "default": 1000000,
      "cap": 8000000
    },
    "spec_conflict_subsets": {
      "default": 10000,
      "cap": 1000000
    },
    "spec_requirements_cap": 64,
    "spec_holes_cap": 16,
    "spec_inputs_plus_outputs_cap": 32,
    "spec_outputs_range": [
      1,
      16
    ],
    "spec_lowering_bound": "4 * total sketch nodes + inputs <= 256; 4 * law nodes + inputs + outputs <= 256",
    "document_bytes": 65536,
    "search_stage_ms": 5000,
    "invocation_ms": 22000
  },
  "refusals": {
    "conflict": "Resolve contradictory owner constraints before proposing another answer.",
    "document-too-large": "Keep the answer/specification document within 65,536 bytes."
  },
  "cost": {
    "units": "Read reported work reservations and scope; these are not total CPU-time measurements.",
    "timing": "No cross-host timing claim."
  },
  "steps": [
    {
      "command": "cat > \"$WORK/specification.json\" <<'DATA'\n{\n  \"schema\": \"zeno-fcis/loop-version-space/1\",\n  \"inputs\": [\n    {\n      \"name\": \"x\",\n      \"domain\": {\n        \"kind\": \"Int\",\n        \"min\": 0,\n        \"max\": 1\n      }\n    }\n  ],\n  \"outputs\": [\n    {\n      \"kind\": \"Int\",\n      \"min\": 0,\n      \"max\": 1\n    }\n  ],\n  \"sketch\": [\n    \"?c[0..1]\"\n  ],\n  \"requirements\": [\n    {\n      \"id\": \"zero\",\n      \"input\": [\n        0\n      ],\n      \"output\": [\n        0\n      ]\n    }\n  ]\n}\nDATA",
      "exit": 0,
      "expected": {},
      "contains": []
    },
    {
      "command": "zeno-fcis loop question --specification \"$WORK/specification.json\" --max-domain 4097",
      "exit": 0,
      "expected": {
        "status": "determined",
        "schema": "zeno-fcis/loop-version-space/2"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis loop question --contract examples/guide-counter",
    "exit": 0,
    "expected": {
      "status": "open",
      "schema": "zeno-fcis/loop-doors/2"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs; crates/zeno-fcis-cli/src/loop_version_space.rs",
    "engine": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs; crates/zeno-fcis-cli/src/loop_version_space.rs",
    "inputs": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs; crates/zeno-fcis-cli/src/loop_version_space.rs",
    "outputs": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs; crates/zeno-fcis-cli/src/loop_version_space.rs",
    "exit_codes": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs; crates/zeno-fcis-cli/src/loop_version_space.rs",
    "verdicts": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs; crates/zeno-fcis-cli/src/loop_version_space.rs",
    "limits": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs; crates/zeno-fcis-cli/src/loop_version_space.rs",
    "refusals": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs; crates/zeno-fcis-cli/src/loop_version_space.rs"
  },
  "commands": [
    "loop question"
  ],
  "pitfalls": "A completed question report may contain disagreements. Contract determination includes delivery requirements beyond exported law-only constraints and need not equal a separate gaps judgment. No answer is adopted and no session or attempt is created."
}
```

# Ask where finite requirements leave choices open

## Use when

find owner questions; ask about missing requirements; distinguish finite sketch completions.

## Not for

adopting an answer; assuming determined means owner intent; treating a question count as accepted code.

## Inputs

--contract reads a supported finite project.zeno and v2/policy.json directory. Optional --answers is zeno-fcis/loop-answers/1 with answers[]: where plus class Accept, Reject or CommittedFailure. Reject/failure answers require a known reason; sets maps state names to successor formulas, while case can supply case updates. For a committing answer, give complete deliveries (an array, [] for none, or "case"). Answers remain proposals.

Alternatively --specification reads zeno-fcis/loop-version-space/1 with exactly schema, inputs, outputs, sketch and requirements. Named inputs have name/domain; Bool domains are kind=Bool, Int domains have signed-64-bit JSON integer min/max. Sketch strings contain finite holes, with no parameters or keep. Each requirement is exactly id/law or id/input/output; Bool example coordinates accept booleans or 0/1, other coordinates are signed-64-bit JSON integers. Each answer/specification JSON document <=65,536 bytes. The five --max-* flags apply to --specification. Contract mode accepts but ignores them and uses its own tuple/admissibility limits.

Engine: Contract admissibility and answer-law checks, or bounded version-space enumeration followed by law evaluation and transform check/replay for a singleton.

Synopsis: `zeno-fcis loop question (--contract DIR [--answers JSON] | --specification JSON)`

Run from the repository root with zeno-fcis on PATH. Set WORK to a fresh writable scratch directory outside the source tree.



## Steps

```sh
cat > "$WORK/specification.json" <<'DATA'
{
  "schema": "zeno-fcis/loop-version-space/1",
  "inputs": [
    {
      "name": "x",
      "domain": {
        "kind": "Int",
        "min": 0,
        "max": 1
      }
    }
  ],
  "outputs": [
    {
      "kind": "Int",
      "min": 0,
      "max": 1
    }
  ],
  "sketch": [
    "?c[0..1]"
  ],
  "requirements": [
    {
      "id": "zero",
      "input": [
        0
      ],
      "output": [
        0
      ]
    }
  ]
}
DATA
```

Expected: exit 0; {}.

```sh
zeno-fcis loop question --specification "$WORK/specification.json" --max-domain 4097
```

Expected: exit 0; {"schema": "zeno-fcis/loop-version-space/2", "status": "determined"}.

## Verdicts

`open/determined`: Contract open/determined reports exit 0 when answers agree; a disagreeing answer exits 1. Inspect every detail.answers[].status. Determination includes reason and delivery constraints.

`conflict/inconsistent/refused`: Contract exit 1; retain the conflicting input or fix the input.

`question/no-completion/determined`: Requirements determined exits 0, no-completion exits 1 and question exits 2. Question/count alone is advisory; singleton acceptance needs its explicit law/check/replay evidence.

`inconclusive/out-of-scope`: Exit 2; no claim beyond the retained completed evidence.

Exit codes: {"0": "Completed report; read the operation-specific verdict and scope.", "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.", "2": "Inconclusive, incomplete or unavailable containment; no new acceptance.", "3": "I/O failure; see operation-specific exceptions.", "64": "Invalid command-line usage."}

Outputs: Contract stdout zeno-fcis/loop-doors/2, door=question: status, detail.stats, detail.question and detail.answers. Requirements stdout zeno-fcis/loop-version-space/2: status, counts, question/conflict and any checked singleton. Early requirements input errors can use transform-loop/1. authority=none.

## Cost

Read reported work reservations and scope; these are not total CPU-time measurements.

Limits: {"contract_admissibility_checks": 4000000, "contract_tuples": 131072, "document_bytes": 65536, "invocation_ms": 22000, "search_stage_ms": 5000, "spec_cells": {"cap": 8000000, "default": 1000000}, "spec_conflict_subsets": {"cap": 1000000, "default": 10000}, "spec_domain": {"cap": 65536, "default": 4096}, "spec_holes_cap": 16, "spec_inputs_plus_outputs_cap": 32, "spec_lowering_bound": "4 * total sketch nodes + inputs <= 256; 4 * law nodes + inputs + outputs <= 256", "spec_meanings": {"cap": 16384, "default": 4096}, "spec_outputs_range": [1, 16], "spec_requirements_cap": 64, "spec_work": {"cap": 100000000, "default": 2000000}}

## Coordinate

Use Commons advise before a checker loop and lease the exact subject. Record the command, exit, verdict, scope and retained evidence, then release the lease. Coordination records and telemetry grant no checker or publication authority.

## Pitfalls

A completed question report may contain disagreements. Contract determination includes delivery requirements beyond exported law-only constraints and need not equal a separate gaps judgment. No answer is adopted and no session or attempt is created.

Refusals and next steps: {"conflict": "Resolve contradictory owner constraints before proposing another answer.", "document-too-large": "Keep the answer/specification document within 65,536 bytes."}

## Worked example

```sh
zeno-fcis loop question --contract examples/guide-counter
```

Expected: exit 0; {"schema": "zeno-fcis/loop-doors/2", "status": "open"}.

## See also

**Background.** Dana Angluin, “Learning Regular Sets from Queries and Counterexamples,” Information and Computation 75(2), 1987, is background for query/refutation loops; it does not establish this finite contract interface.

**What is new here.** This card documents the implemented interface; it adds no algorithm or priority claim.

**Scope.** A completed question report may contain disagreements. Contract determination includes delivery requirements beyond exported law-only constraints and need not equal a separate gaps judgment. No answer is adopted and no session or attempt is created.

Sources: crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-cli/src/loop_judges.rs; crates/zeno-fcis-cli/src/loop_version_space.rs

[CLI reference](../CLI_REFERENCE.md); [loop overview](loop-workspace.md).
