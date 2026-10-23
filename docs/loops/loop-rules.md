```json
{
  "name": "loop-rules",
  "title": "Inspect finite rewrite qualification and refutations",
  "synopsis": "zeno-fcis loop rules --original PROGRAM [--terms JSON | --bank JSON] [--out NEWDIR]",
  "engine": "Concrete term discovery proposes; target-domain transform check and receipt replay qualify each rule and every whole-program replacement.",
  "use_when": [
    "inspect which rewrite rules were refuted",
    "mine finite domain rewrites",
    "recheck a rule bank"
  ],
  "not_for": [
    "universal algebraic identities",
    "treating bank metadata as authority",
    "assuming qualified means improved"
  ],
  "inputs": [
    "--original: canonical Bool/signed-i128 program. Optional --terms: zeno-fcis/loop-terms/1 with terms plus optional vocabulary/parameters; each term is single-output program JSON or a kind/expression object. Optional --bank: zeno-fcis/loop-rule-bank/1 with rules containing lhs/rhs program JSON. Each file <=65,536 bytes. --terms and --bank conflict. Without either, terms are mined from the reference.",
    "--out requires a new directory and writes result.json and bank.json. Overall qualification additionally exports original.zcve and candidate.zcve; the unchanged result exports the reference bytes. No session or attempts are created."
  ],
  "outputs": [
    "stdout zeno-fcis/loop-doors/2, door=rules, authority=none; detail is zeno-fcis/loop-rules-result/1. After valid bank/corpus intake and qualification processing, top-level rules_summary reports considered/qualified/refuted/refused/inconclusive/unexamined counts and refutations/refusals. Each first refutation per rule gives its zero-based rule index, stage, ordinal, input and both observations; stderr prints one line per refuted rule. Exported result.json retains the summary too.",
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
    "qualified": "Exit 0: the retained program passed transform check/replay. Read improved/result_kind; unchanged-original can qualify even when every supplied rule was refuted or no rules were mined.",
    "inconclusive": "Exit 2: baseline, rule or replacement checking/replay or a bound stopped; no candidate bytes are exported.",
    "refused/refuted": "Exit 1: repair the named input or retain the refuting witness. Existing checks and verdicts remain decisive."
  },
  "limits": {
    "file_bytes": 65536,
    "program_inputs_cap": 32,
    "program_outputs_cap": 16,
    "program_nodes_cap": 256,
    "max_tuples": {
      "default": 131072,
      "cap": 131072
    },
    "max_gate_pairs": {
      "default": 1000000,
      "cap": 4000000
    },
    "max_search_work_per_stage": {
      "default": 50000000,
      "cap": 100000000
    },
    "max_cells": {
      "default": 4194304,
      "cap": 16777216
    },
    "max_assignments": {
      "default": 2000,
      "cap": 20000
    },
    "max_terms": {
      "default": 128,
      "cap": 512
    },
    "max_checks": {
      "default": 24,
      "cap": 30
    },
    "deadline_ms": {
      "default": 22000,
      "cap": 22000
    },
    "invocation_control_frames": 62
  },
  "refusals": {
    "nondecreasing-rule": "Offer a strictly cheaper RHS.",
    "rule-bank-schema": "Use zeno-fcis/loop-rule-bank/1.",
    "output-must-be-new-directory": "Choose a new --out path."
  },
  "cost": {
    "units": "Read reported work reservations and scope; these are not total CPU-time measurements.",
    "timing": "No cross-host timing claim."
  },
  "steps": [
    {
      "command": "cat > \"$WORK/program.json\" <<'DATA'\n{\n  \"inputs\": [\n    {\n      \"kind\": \"Int\",\n      \"min\": \"0\",\n      \"max\": \"3\"\n    }\n  ],\n  \"outputs\": [\n    {\n      \"kind\": \"Int\",\n      \"min\": \"-170141183460469231731687303715884105728\",\n      \"max\": \"170141183460469231731687303715884105727\"\n    }\n  ],\n  \"nodes\": [\n    [\n      \"Input\",\n      0\n    ],\n    [\n      \"Add\",\n      0,\n      0\n    ],\n    [\n      \"Sub\",\n      1,\n      0\n    ]\n  ],\n  \"roots\": [\n    2\n  ]\n}\nDATA\ncat > \"$WORK/bank.json\" <<'DATA'\n{\n  \"schema\": \"zeno-fcis/loop-rule-bank/1\",\n  \"rules\": [\n    {\n      \"lhs\": {\n        \"inputs\": [\n          {\n            \"kind\": \"Int\",\n            \"min\": \"0\",\n            \"max\": \"3\"\n          }\n        ],\n        \"outputs\": [\n          {\n            \"kind\": \"Int\",\n            \"min\": \"-170141183460469231731687303715884105728\",\n            \"max\": \"170141183460469231731687303715884105727\"\n          }\n        ],\n        \"nodes\": [\n          [\n            \"Input\",\n            0\n          ],\n          [\n            \"Int\",\n            \"1\"\n          ],\n          [\n            \"Add\",\n            0,\n            1\n          ]\n        ],\n        \"roots\": [\n          2\n        ]\n      },\n      \"rhs\": {\n        \"inputs\": [\n          {\n            \"kind\": \"Int\",\n            \"min\": \"0\",\n            \"max\": \"3\"\n          }\n        ],\n        \"outputs\": [\n          {\n            \"kind\": \"Int\",\n            \"min\": \"-170141183460469231731687303715884105728\",\n            \"max\": \"170141183460469231731687303715884105727\"\n          }\n        ],\n        \"nodes\": [\n          [\n            \"Input\",\n            0\n          ]\n        ],\n        \"roots\": [\n          0\n        ]\n      }\n    }\n  ]\n}\nDATA",
      "exit": 0,
      "expected": {},
      "contains": []
    },
    {
      "command": "zeno-fcis loop encode --program \"$WORK/program.json\" --out \"$WORK/program.zcve\"",
      "exit": 0,
      "expected": {
        "status": "encoded"
      },
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis loop rules --original \"$WORK/program.zcve\" --bank \"$WORK/bank.json\"",
    "exit": 0,
    "expected": {
      "status": "qualified",
      "detail.result_kind": "unchanged-original",
      "rules_summary.considered": 1,
      "rules_summary.qualified": 0,
      "rules_summary.refuted": 1,
      "rules_summary.refutations.0.stage": "target-rule",
      "rules_summary.refutations.0.input": [
        "0"
      ]
    },
    "contains": []
  },
  "sources": {
    "synopsis": "crates/zeno-fcis-cli/src/loop_rules.rs; crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs; docs/CLI_REFERENCE.md#loop-rules",
    "engine": "crates/zeno-fcis-cli/src/loop_rules.rs; crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs; docs/CLI_REFERENCE.md#loop-rules",
    "inputs": "crates/zeno-fcis-cli/src/loop_rules.rs; crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs; docs/CLI_REFERENCE.md#loop-rules",
    "outputs": "crates/zeno-fcis-cli/src/loop_rules.rs; crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs; docs/CLI_REFERENCE.md#loop-rules",
    "exit_codes": "crates/zeno-fcis-cli/src/loop_rules.rs; crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs; docs/CLI_REFERENCE.md#loop-rules",
    "verdicts": "crates/zeno-fcis-cli/src/loop_rules.rs; crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs; docs/CLI_REFERENCE.md#loop-rules",
    "limits": "crates/zeno-fcis-cli/src/loop_rules.rs; crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs; docs/CLI_REFERENCE.md#loop-rules",
    "refusals": "crates/zeno-fcis-cli/src/loop_rules.rs; crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs; docs/CLI_REFERENCE.md#loop-rules"
  },
  "commands": [
    "loop rules"
  ],
  "pitfalls": "The summary is derived diagnostics, not an acceptance gate. Qualified counts term rules passing target check/replay; refuted counts rules with a target or whole-program counterexample. These counts can overlap when a qualified term rule yields a refuted replacement. Refused/inconclusive entries do not count as refutations; unexamined rules have no target-rule row. Only the listed finite domain is checked."
}
```

# Inspect finite rewrite qualification and refutations

## Use when

inspect which rewrite rules were refuted; mine finite domain rewrites; recheck a rule bank.

## Not for

universal algebraic identities; treating bank metadata as authority; assuming qualified means improved.

## Inputs

--original: canonical Bool/signed-i128 program. Optional --terms: zeno-fcis/loop-terms/1 with terms plus optional vocabulary/parameters; each term is single-output program JSON or a kind/expression object. Optional --bank: zeno-fcis/loop-rule-bank/1 with rules containing lhs/rhs program JSON. Each file <=65,536 bytes. --terms and --bank conflict. Without either, terms are mined from the reference.

--out requires a new directory and writes result.json and bank.json. Overall qualification additionally exports original.zcve and candidate.zcve; the unchanged result exports the reference bytes. No session or attempts are created.

Engine: Concrete term discovery proposes; target-domain transform check and receipt replay qualify each rule and every whole-program replacement.

Synopsis: `zeno-fcis loop rules --original PROGRAM [--terms JSON | --bank JSON] [--out NEWDIR]`

Run from the repository root with zeno-fcis on PATH. Set WORK to a fresh writable scratch directory outside the source tree.



## Steps

```sh
cat > "$WORK/program.json" <<'DATA'
{
  "inputs": [
    {
      "kind": "Int",
      "min": "0",
      "max": "3"
    }
  ],
  "outputs": [
    {
      "kind": "Int",
      "min": "-170141183460469231731687303715884105728",
      "max": "170141183460469231731687303715884105727"
    }
  ],
  "nodes": [
    [
      "Input",
      0
    ],
    [
      "Add",
      0,
      0
    ],
    [
      "Sub",
      1,
      0
    ]
  ],
  "roots": [
    2
  ]
}
DATA
cat > "$WORK/bank.json" <<'DATA'
{
  "schema": "zeno-fcis/loop-rule-bank/1",
  "rules": [
    {
      "lhs": {
        "inputs": [
          {
            "kind": "Int",
            "min": "0",
            "max": "3"
          }
        ],
        "outputs": [
          {
            "kind": "Int",
            "min": "-170141183460469231731687303715884105728",
            "max": "170141183460469231731687303715884105727"
          }
        ],
        "nodes": [
          [
            "Input",
            0
          ],
          [
            "Int",
            "1"
          ],
          [
            "Add",
            0,
            1
          ]
        ],
        "roots": [
          2
        ]
      },
      "rhs": {
        "inputs": [
          {
            "kind": "Int",
            "min": "0",
            "max": "3"
          }
        ],
        "outputs": [
          {
            "kind": "Int",
            "min": "-170141183460469231731687303715884105728",
            "max": "170141183460469231731687303715884105727"
          }
        ],
        "nodes": [
          [
            "Input",
            0
          ]
        ],
        "roots": [
          0
        ]
      }
    }
  ]
}
DATA
```

Expected: exit 0; {}.

```sh
zeno-fcis loop encode --program "$WORK/program.json" --out "$WORK/program.zcve"
```

Expected: exit 0; {"status": "encoded"}.

## Verdicts

`qualified`: Exit 0: the retained program passed transform check/replay. Read improved/result_kind; unchanged-original can qualify even when every supplied rule was refuted or no rules were mined.

`inconclusive`: Exit 2: baseline, rule or replacement checking/replay or a bound stopped; no candidate bytes are exported.

`refused/refuted`: Exit 1: repair the named input or retain the refuting witness. Existing checks and verdicts remain decisive.

Exit codes: {"0": "Completed report; read the operation-specific verdict and scope.", "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.", "2": "Inconclusive, incomplete or unavailable containment; no new acceptance.", "3": "I/O failure; see operation-specific exceptions.", "64": "Invalid command-line usage."}

Outputs: stdout zeno-fcis/loop-doors/2, door=rules, authority=none; detail is zeno-fcis/loop-rules-result/1. After valid bank/corpus intake and qualification processing, top-level rules_summary reports considered/qualified/refuted/refused/inconclusive/unexamined counts and refutations/refusals. Each first refutation per rule gives its zero-based rule index, stage, ordinal, input and both observations; stderr prints one line per refuted rule. Exported result.json retains the summary too.

## Cost

Read reported work reservations and scope; these are not total CPU-time measurements.

Limits: {"deadline_ms": {"cap": 22000, "default": 22000}, "file_bytes": 65536, "invocation_control_frames": 62, "max_assignments": {"cap": 20000, "default": 2000}, "max_cells": {"cap": 16777216, "default": 4194304}, "max_checks": {"cap": 30, "default": 24}, "max_gate_pairs": {"cap": 4000000, "default": 1000000}, "max_search_work_per_stage": {"cap": 100000000, "default": 50000000}, "max_terms": {"cap": 512, "default": 128}, "max_tuples": {"cap": 131072, "default": 131072}, "program_inputs_cap": 32, "program_nodes_cap": 256, "program_outputs_cap": 16}

## Coordinate

Use Commons advise before a checker loop and lease the exact subject. Record the command, exit, verdict, scope and retained evidence, then release the lease. Coordination records and telemetry grant no checker or publication authority.

## Pitfalls

The summary is derived diagnostics, not an acceptance gate. Qualified counts term rules passing target check/replay; refuted counts rules with a target or whole-program counterexample. These counts can overlap when a qualified term rule yields a refuted replacement. Refused/inconclusive entries do not count as refutations; unexamined rules have no target-rule row. Only the listed finite domain is checked.

Refusals and next steps: {"nondecreasing-rule": "Offer a strictly cheaper RHS.", "output-must-be-new-directory": "Choose a new --out path.", "rule-bank-schema": "Use zeno-fcis/loop-rule-bank/1."}

## Worked example

```sh
zeno-fcis loop rules --original "$WORK/program.zcve" --bank "$WORK/bank.json"
```

Expected: exit 0; {"detail.result_kind": "unchanged-original", "rules_summary.considered": 1, "rules_summary.qualified": 0, "rules_summary.refutations.0.input": ["0"], "rules_summary.refutations.0.stage": "target-rule", "rules_summary.refuted": 1, "status": "qualified"}.

## See also

**Background.** Ross Tate, Michael Stepp, Zachary Tatlock and Sorin Lerner, “Equality Saturation: A New Approach to Optimization,” ACM POPL, 2009. This is background for rewrite proposals, not a proof of their applicability.

**What is new here.** This card documents the implemented interface; it adds no algorithm or priority claim.

**Scope.** The summary is derived diagnostics, not an acceptance gate. Qualified counts term rules passing target check/replay; refuted counts rules with a target or whole-program counterexample. These counts can overlap when a qualified term rule yields a refuted replacement. Refused/inconclusive entries do not count as refutations; unexamined rules have no target-rule row. Only the listed finite domain is checked.

Sources: crates/zeno-fcis-cli/src/loop_rules.rs; crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs; docs/CLI_REFERENCE.md#loop-rules

[CLI reference](../CLI_REFERENCE.md); [loop overview](loop-workspace.md).
