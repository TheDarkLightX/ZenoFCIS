```json
{
  "name": "loop-learn",
  "title": "Learn within a declared expression vocabulary",
  "synopsis": "zeno-fcis loop learn --spec JSON [--original PROGRAM] [--out NEWDIR]",
  "engine": "Bounded syntax enumeration proposes expressions; the library evaluator rechecks all supplied examples. With a reference, complete observed-table capture and original-bound transform check/replay qualify a program.",
  "use_when": [
    "learn a program from examples",
    "fit a bounded vocabulary",
    "ask a distinguishing example"
  ],
  "not_for": [
    "example agreement as full equivalence",
    "hidden operators or macros",
    "treating uniqueness as a proof of intent"
  ],
  "inputs": [
    "--spec: ASCII zeno-fcis/loop-learn/1 with named Bool/Int inputs, output domains, vocabulary, max_size, misses=0 and complete examples. Bounds and labels are JSON integers (Bool wire values 0/1), unlike decimal-string program JSON. Names and field sets are closed.",
    "Vocabulary: constants, arithmetic (+,-,*,div_floor,div_ceil,mul_div_floor,mul_div_ceil), macros (min,max,abs), compare (<,<=,==,!=,products_ge), boolean (!,&&,||) and conditional. Defaults: constants [1,2,3], arithmetic [+,-], other lists empty, conditional false, max_size 7.",
    "Optional --original: canonical program, same ABI, <=65,536 bytes, all evaluations successful over <=4,096 tuples. --out requires --original and a fresh directory; candidate bytes are exported only when qualified."
  ],
  "outputs": [
    "stdout zeno-fcis/loop-doors/2, door=learn, authority=none: status, detail.counts/before/by_size/question/core/candidate, work/limits; qualified results include capture and original-bound check/replay. Example fitting alone produces no equivalence receipt.",
    "Where integer_encoding is present, scalar report integers use safe-number-or-decimal-string/1. Program JSON bounds/literals remain decimal strings; node references remain JSON integers."
  ],
  "exit_codes": {
    "0": "Completed report; read the operation-specific verdict and scope.",
    "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.",
    "2": "Inconclusive, incomplete or unavailable containment; no new acceptance.",
    "3": "I/O failure where propagated.",
    "64": "Invalid command-line usage."
  },
  "verdicts": {
    "determined": "Exit 0 without reference: one proposed meaning per output fits; candidate was checked on examples only.",
    "question": "Exit 2 without reference: several meanings fit and an unanswered input distinguishes them.",
    "outside-vocabulary/ambiguous/refused": "Exit 1; no accepted program. outside-vocabulary gives a proposed minimal conflicting example subset.",
    "qualified/refuted": "With reference, exits 0/1 respectively after complete reference checks; inspect capture/check/replay.",
    "inconclusive": "Exit 2; no qualified export. Completed example-learning evidence can remain after a later qualification stop."
  },
  "limits": {
    "file_bytes": 65536,
    "inputs": 8,
    "outputs": 8,
    "constants": 32,
    "examples": 64,
    "operations": 20,
    "max_size": 12,
    "max_work": 2000000,
    "max_tuples": 4096,
    "max_meanings": 4096,
    "max_cells": 1000000,
    "max_conflict_subsets": 10000,
    "flags": "May only reduce the default enumeration caps."
  },
  "refusals": {
    "learn-limit-ceiling": "Reduce the requested cap.",
    "learn-confirmed-examples-require-zero-misses": "Use confirmed complete examples and misses=0.",
    "output-must-be-new-directory": "Choose a new output directory."
  },
  "cost": {
    "units": "Read reported work reservations and scope; these are not total CPU-time measurements.",
    "timing": "No cross-host timing claim."
  },
  "steps": [
    {
      "command": "cat > \"$WORK/learn.json\" <<'DATA'\n{\n  \"schema\": \"zeno-fcis/loop-learn/1\",\n  \"inputs\": [\n    {\n      \"name\": \"x\",\n      \"domain\": {\n        \"kind\": \"Int\",\n        \"min\": 0,\n        \"max\": 2\n      }\n    }\n  ],\n  \"outputs\": [\n    {\n      \"kind\": \"Int\",\n      \"min\": 0,\n      \"max\": 2\n    }\n  ],\n  \"vocabulary\": {\n    \"constants\": [],\n    \"arithmetic\": []\n  },\n  \"max_size\": 1,\n  \"examples\": [\n    {\n      \"input\": [\n        0\n      ],\n      \"output\": [\n        0\n      ]\n    },\n    {\n      \"input\": [\n        1\n      ],\n      \"output\": [\n        1\n      ]\n    },\n    {\n      \"input\": [\n        2\n      ],\n      \"output\": [\n        2\n      ]\n    }\n  ]\n}\nDATA",
      "exit": 0,
      "expected": {},
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis loop learn --spec \"$WORK/learn.json\"",
    "exit": 0,
    "expected": {
      "status": "determined",
      "schema": "zeno-fcis/loop-doors/2"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "crates/zeno-fcis-cli/src/loop_learn.rs; crates/zeno-fcis-loop-doors/src/learn.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "engine": "crates/zeno-fcis-cli/src/loop_learn.rs; crates/zeno-fcis-loop-doors/src/learn.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "inputs": "crates/zeno-fcis-cli/src/loop_learn.rs; crates/zeno-fcis-loop-doors/src/learn.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "outputs": "crates/zeno-fcis-cli/src/loop_learn.rs; crates/zeno-fcis-loop-doors/src/learn.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "exit_codes": "crates/zeno-fcis-cli/src/loop_learn.rs; crates/zeno-fcis-loop-doors/src/learn.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "verdicts": "crates/zeno-fcis-cli/src/loop_learn.rs; crates/zeno-fcis-loop-doors/src/learn.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "limits": "crates/zeno-fcis-cli/src/loop_learn.rs; crates/zeno-fcis-loop-doors/src/learn.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "refusals": "crates/zeno-fcis-cli/src/loop_learn.rs; crates/zeno-fcis-loop-doors/src/learn.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs"
  },
  "commands": [
    "loop learn"
  ],
  "pitfalls": "Counts and uniqueness are bounded enumeration proposals. Only complete reference checking/replay can qualify full-domain equivalence. Eager failures and all output labels are retained; no session or attempt is created."
}
```

# Learn within a declared expression vocabulary

## Use when

learn a program from examples; fit a bounded vocabulary; ask a distinguishing example.

## Not for

example agreement as full equivalence; hidden operators or macros; treating uniqueness as a proof of intent.

## Inputs

--spec: ASCII zeno-fcis/loop-learn/1 with named Bool/Int inputs, output domains, vocabulary, max_size, misses=0 and complete examples. Bounds and labels are JSON integers (Bool wire values 0/1), unlike decimal-string program JSON. Names and field sets are closed.

Vocabulary: constants, arithmetic (+,-,*,div_floor,div_ceil,mul_div_floor,mul_div_ceil), macros (min,max,abs), compare (<,<=,==,!=,products_ge), boolean (!,&&,||) and conditional. Defaults: constants [1,2,3], arithmetic [+,-], other lists empty, conditional false, max_size 7.

Optional --original: canonical program, same ABI, <=65,536 bytes, all evaluations successful over <=4,096 tuples. --out requires --original and a fresh directory; candidate bytes are exported only when qualified.

Engine: Bounded syntax enumeration proposes expressions; the library evaluator rechecks all supplied examples. With a reference, complete observed-table capture and original-bound transform check/replay qualify a program.

Synopsis: `zeno-fcis loop learn --spec JSON [--original PROGRAM] [--out NEWDIR]`

Run from the repository root with zeno-fcis on PATH. Set WORK to a fresh writable scratch directory outside the source tree.



## Steps

```sh
cat > "$WORK/learn.json" <<'DATA'
{
  "schema": "zeno-fcis/loop-learn/1",
  "inputs": [
    {
      "name": "x",
      "domain": {
        "kind": "Int",
        "min": 0,
        "max": 2
      }
    }
  ],
  "outputs": [
    {
      "kind": "Int",
      "min": 0,
      "max": 2
    }
  ],
  "vocabulary": {
    "constants": [],
    "arithmetic": []
  },
  "max_size": 1,
  "examples": [
    {
      "input": [
        0
      ],
      "output": [
        0
      ]
    },
    {
      "input": [
        1
      ],
      "output": [
        1
      ]
    },
    {
      "input": [
        2
      ],
      "output": [
        2
      ]
    }
  ]
}
DATA
```

Expected: exit 0; {}.

## Verdicts

`determined`: Exit 0 without reference: one proposed meaning per output fits; candidate was checked on examples only.

`question`: Exit 2 without reference: several meanings fit and an unanswered input distinguishes them.

`outside-vocabulary/ambiguous/refused`: Exit 1; no accepted program. outside-vocabulary gives a proposed minimal conflicting example subset.

`qualified/refuted`: With reference, exits 0/1 respectively after complete reference checks; inspect capture/check/replay.

`inconclusive`: Exit 2; no qualified export. Completed example-learning evidence can remain after a later qualification stop.

Exit codes: {"0": "Completed report; read the operation-specific verdict and scope.", "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.", "2": "Inconclusive, incomplete or unavailable containment; no new acceptance.", "3": "I/O failure where propagated.", "64": "Invalid command-line usage."}

Outputs: stdout zeno-fcis/loop-doors/2, door=learn, authority=none: status, detail.counts/before/by_size/question/core/candidate, work/limits; qualified results include capture and original-bound check/replay. Example fitting alone produces no equivalence receipt.

## Cost

Read reported work reservations and scope; these are not total CPU-time measurements.

Limits: {"constants": 32, "examples": 64, "file_bytes": 65536, "flags": "May only reduce the default enumeration caps.", "inputs": 8, "max_cells": 1000000, "max_conflict_subsets": 10000, "max_meanings": 4096, "max_size": 12, "max_tuples": 4096, "max_work": 2000000, "operations": 20, "outputs": 8}

## Pitfalls

Counts and uniqueness are bounded enumeration proposals. Only complete reference checking/replay can qualify full-domain equivalence. Eager failures and all output labels are retained; no session or attempt is created.

Refusals and next steps: {"learn-confirmed-examples-require-zero-misses": "Use confirmed complete examples and misses=0.", "learn-limit-ceiling": "Reduce the requested cap.", "output-must-be-new-directory": "Choose a new output directory."}

## Worked example

```sh
zeno-fcis loop learn --spec "$WORK/learn.json"
```

Expected: exit 0; {"schema": "zeno-fcis/loop-doors/2", "status": "determined"}.

## See also

**Background.** Rajeev Alur et al., “Syntax-Guided Synthesis,” FMCAD, 2013, is background for fixed-vocabulary synthesis; Armando Solar-Lezama et al., “Combinatorial Sketching for Finite Programs,” ASPLOS, 2006, is background for counterexample-guided synthesis.

**What is new here.** This card documents the implemented interface; it adds no algorithm or priority claim.

**Scope.** Counts and uniqueness are bounded enumeration proposals. Only complete reference checking/replay can qualify full-domain equivalence. Eager failures and all output labels are retained; no session or attempt is created.

Sources: crates/zeno-fcis-cli/src/loop_learn.rs; crates/zeno-fcis-loop-doors/src/learn.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs

[CLI reference](../CLI_REFERENCE.md); [loop overview](loop-workspace.md).
