```json
{
  "name": "loop-pipeline",
  "title": "Check a bound completion, repair or simplification pipeline",
  "synopsis": "zeno-fcis loop pipeline --original PROGRAM (--packet | --proposal JSON) [--out NEWDIR]",
  "engine": "Guarded finite completion/repair/canonicalization proposals; full-domain transform check plus matching receipt replay alone qualifies the retained result.",
  "use_when": [
    "complete repair and simplify a program",
    "obtain pipeline binding hashes",
    "compose bounded proposal stages"
  ],
  "not_for": [
    "session attempt accounting",
    "partially checked export",
    "all-failure references as successful computations"
  ],
  "inputs": [
    "--original canonical program and --proposal zeno-fcis/loop-pipeline/1, <=65,536 bytes each. Obtain original_sha256 and implementation_sha256 with --packet. Proposal uses both hashes, either program JSON or a zeno-fcis/loop-expressions/1 document, and optional stages. Up to five stages: complete, repair, canonical, resubstitute; defaults complete,repair,canonical.",
    "--packet conflicts with --out. --out must be a new directory; result.json records evidence. Only overall qualified writes original.zcve and candidate.zcve. No session/attempt is created."
  ],
  "outputs": [
    "stdout zeno-fcis/loop-doors/2, door=pipeline, authority=none. Packet detail has bindings/grammar/limits. A run reports status, binding, trace, candidates, best/best_check, work/limits and reason.",
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
    "packet": "Exit 0 gives bindings only; no search or equivalence claim.",
    "qualified": "Exit 0 requires transform check/replay; inspect improved and cost.",
    "refuted/refused": "Exit 1 retains witness or input reason.",
    "inconclusive": "Exit 2, including all-original-observations-fail and later budget stops; earlier success does not permit candidate export."
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
    "proposal-original-binding": "Use a fresh packet for these exact bytes.",
    "proposal-implementation-binding": "Recreate the proposal from the current implementation packet.",
    "output-must-be-new-directory": "Use a new --out directory."
  },
  "cost": {
    "units": "Read reported work reservations and scope; these are not total CPU-time measurements.",
    "timing": "No cross-host timing claim."
  },
  "steps": [
    {
      "command": "cat > \"$WORK/program.json\" <<'DATA'\n{\n  \"inputs\": [\n    {\n      \"kind\": \"Int\",\n      \"min\": \"0\",\n      \"max\": \"3\"\n    }\n  ],\n  \"outputs\": [\n    {\n      \"kind\": \"Int\",\n      \"min\": \"-170141183460469231731687303715884105728\",\n      \"max\": \"170141183460469231731687303715884105727\"\n    }\n  ],\n  \"nodes\": [\n    [\n      \"Input\",\n      0\n    ],\n    [\n      \"Add\",\n      0,\n      0\n    ],\n    [\n      \"Sub\",\n      1,\n      0\n    ]\n  ],\n  \"roots\": [\n    2\n  ]\n}\nDATA",
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
    },
    {
      "command": "zeno-fcis loop pipeline --original \"$WORK/program.zcve\" --packet > \"$WORK/packet.json\"\npython3 - \"$WORK\" <<'PYCODE'\nimport json, pathlib, sys\nw = pathlib.Path(sys.argv[1])\np = json.loads((w / \"packet.json\").read_text())[\"detail\"]\np.update(schema=\"zeno-fcis/loop-pipeline/1\", stages=[], program=json.loads((w / \"program.json\").read_text()))\np = {k: p[k] for k in (\"schema\", \"original_sha256\", \"implementation_sha256\", \"stages\", \"program\")}\n(w / \"proposal.json\").write_text(json.dumps(p))\nPYCODE",
      "exit": 0,
      "expected": {},
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis loop pipeline --original \"$WORK/program.zcve\" --proposal \"$WORK/proposal.json\"",
    "exit": 0,
    "expected": {
      "status": "qualified",
      "schema": "zeno-fcis/loop-doors/2"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "engine": "crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "inputs": "crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "outputs": "crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "exit_codes": "crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "verdicts": "crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "limits": "crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs",
    "refusals": "crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs"
  },
  "commands": [
    "loop pipeline"
  ],
  "pitfalls": "Stage guards and proposer ranking grant no acceptance. The reference must have a successful observation. Qualification covers only the declared finite domain and does not adopt or publish code."
}
```

# Check a bound completion, repair or simplification pipeline

## Use when

complete repair and simplify a program; obtain pipeline binding hashes; compose bounded proposal stages.

## Not for

session attempt accounting; partially checked export; all-failure references as successful computations.

## Inputs

--original canonical program and --proposal zeno-fcis/loop-pipeline/1, <=65,536 bytes each. Obtain original_sha256 and implementation_sha256 with --packet. Proposal uses both hashes, either program JSON or a zeno-fcis/loop-expressions/1 document, and optional stages. Up to five stages: complete, repair, canonical, resubstitute; defaults complete,repair,canonical.

--packet conflicts with --out. --out must be a new directory; result.json records evidence. Only overall qualified writes original.zcve and candidate.zcve. No session/attempt is created.

Engine: Guarded finite completion/repair/canonicalization proposals; full-domain transform check plus matching receipt replay alone qualifies the retained result.

Synopsis: `zeno-fcis loop pipeline --original PROGRAM (--packet | --proposal JSON) [--out NEWDIR]`

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
```

Expected: exit 0; {}.

```sh
zeno-fcis loop encode --program "$WORK/program.json" --out "$WORK/program.zcve"
```

Expected: exit 0; {"status": "encoded"}.

```sh
zeno-fcis loop pipeline --original "$WORK/program.zcve" --packet > "$WORK/packet.json"
python3 - "$WORK" <<'PYCODE'
import json, pathlib, sys
w = pathlib.Path(sys.argv[1])
p = json.loads((w / "packet.json").read_text())["detail"]
p.update(schema="zeno-fcis/loop-pipeline/1", stages=[], program=json.loads((w / "program.json").read_text()))
p = {k: p[k] for k in ("schema", "original_sha256", "implementation_sha256", "stages", "program")}
(w / "proposal.json").write_text(json.dumps(p))
PYCODE
```

Expected: exit 0; {}.

## Verdicts

`packet`: Exit 0 gives bindings only; no search or equivalence claim.

`qualified`: Exit 0 requires transform check/replay; inspect improved and cost.

`refuted/refused`: Exit 1 retains witness or input reason.

`inconclusive`: Exit 2, including all-original-observations-fail and later budget stops; earlier success does not permit candidate export.

Exit codes: {"0": "Completed report; read the operation-specific verdict and scope.", "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.", "2": "Inconclusive, incomplete or unavailable containment; no new acceptance.", "3": "I/O failure; see operation-specific exceptions.", "64": "Invalid command-line usage."}

Outputs: stdout zeno-fcis/loop-doors/2, door=pipeline, authority=none. Packet detail has bindings/grammar/limits. A run reports status, binding, trace, candidates, best/best_check, work/limits and reason.

## Cost

Read reported work reservations and scope; these are not total CPU-time measurements.

Limits: {"deadline_ms": {"cap": 22000, "default": 22000}, "file_bytes": 65536, "invocation_control_frames": 62, "max_assignments": {"cap": 20000, "default": 2000}, "max_cells": {"cap": 16777216, "default": 4194304}, "max_checks": {"cap": 30, "default": 24}, "max_gate_pairs": {"cap": 4000000, "default": 1000000}, "max_search_work_per_stage": {"cap": 100000000, "default": 50000000}, "max_terms": {"cap": 512, "default": 128}, "max_tuples": {"cap": 131072, "default": 131072}, "program_inputs_cap": 32, "program_nodes_cap": 256, "program_outputs_cap": 16}

## Coordinate

Use Commons advise before a checker loop and lease the exact subject. Record the command, exit, verdict, scope and retained evidence, then release the lease. Coordination records and telemetry grant no checker or publication authority.

## Pitfalls

Stage guards and proposer ranking grant no acceptance. The reference must have a successful observation. Qualification covers only the declared finite domain and does not adopt or publish code.

Refusals and next steps: {"output-must-be-new-directory": "Use a new --out directory.", "proposal-implementation-binding": "Recreate the proposal from the current implementation packet.", "proposal-original-binding": "Use a fresh packet for these exact bytes."}

## Worked example

```sh
zeno-fcis loop pipeline --original "$WORK/program.zcve" --proposal "$WORK/proposal.json"
```

Expected: exit 0; {"schema": "zeno-fcis/loop-doors/2", "status": "qualified"}.

## See also

**Background.** Armando Solar-Lezama, Liviu Tancau, Rastislav Bodik, Sanjit Seshia and Vijay Saraswat, “Combinatorial Sketching for Finite Programs,” ASPLOS, 2006, is background for guarded completion/refutation loops.

**What is new here.** This card documents the implemented interface; it adds no algorithm or priority claim.

**Scope.** Stage guards and proposer ranking grant no acceptance. The reference must have a successful observation. Qualification covers only the declared finite domain and does not adopt or publish code.

Sources: crates/zeno-fcis-cli/src/loop_pipeline.rs; crates/zeno-fcis-cli/src/loop_doors_command.rs

[CLI reference](../CLI_REFERENCE.md); [loop overview](loop-workspace.md).
