```json
{
  "name": "loop-transfer",
  "title": "Try a source program on listed target instances",
  "synopsis": "zeno-fcis loop transfer --from JSON --to-instances JSON",
  "engine": "Per-instance expression lowering or bounded sketch/repair proposes; every successful instance requires transform check and matching receipt replay.",
  "use_when": [
    "transfer a program to new instances",
    "recheck source constants on target domains"
  ],
  "not_for": [
    "unlisted instance coverage",
    "universal parameter theorems",
    "treating fitted formulas as verified transfers"
  ],
  "inputs": [
    "Manifest: ASCII zeno-fcis/loop-family-manifest/1 with family, vocabulary and 1..10 instances. Each has name, parameters (signed-64-bit JSON integers), relative program path, optional optimize path and fit Boolean (default true). Absolute paths and parent traversal are refused. Files/programs <=65,536 bytes; each instance <=131,072 tuples.",
    "Transfer --from is zeno-fcis/loop-expressions/1 with source vocabulary, literal constants and signed-64-bit source parameter metadata. Source $PARAMETERS are refused. Try direct use, constants near changed parameters as choice holes, then repair."
  ],
  "outputs": [
    "stdout zeno-fcis/loop-doors/2, door=transfer, authority=none: detail rows with programs/check/replay digests and coverage of the listed instances. No receipt files, session or attempts are exported.",
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
    "transferred": "Exit 0 only if all listed targets transfer.",
    "failed": "Exit 1 if none transfer, including per-target budget stops.",
    "partial": "Exit 2 when only some transfer; inspect every row.",
    "refused": "Exit 1 for invalid source or target input."
  },
  "limits": {
    "file_bytes": 65536,
    "program_inputs_cap": 32,
    "program_outputs_cap": 16,
    "program_nodes_cap": 256,
    "input_tuples_per_instance": 131072,
    "instances": 10,
    "output_expressions": 256,
    "search_stage_ms": 5000,
    "check_stage_ms": 2000,
    "invocation_control_frames": 62,
    "invocation_ms": 22000
  },
  "refusals": {
    "manifest-path": "Keep target paths relative to the manifest.",
    "transfer-source-parameters": "Give source parameter values and literal-constant expressions."
  },
  "cost": {
    "units": "Read reported work reservations and scope; these are not total CPU-time measurements.",
    "timing": "No cross-host timing claim."
  },
  "steps": [
    {
      "command": "cat > \"$WORK/program.json\" <<'DATA'\n{\n  \"inputs\": [\n    {\n      \"kind\": \"Int\",\n      \"min\": \"0\",\n      \"max\": \"3\"\n    }\n  ],\n  \"outputs\": [\n    {\n      \"kind\": \"Int\",\n      \"min\": \"-170141183460469231731687303715884105728\",\n      \"max\": \"170141183460469231731687303715884105727\"\n    }\n  ],\n  \"nodes\": [\n    [\n      \"Input\",\n      0\n    ]\n  ],\n  \"roots\": [\n    0\n  ]\n}\nDATA\ncat > \"$WORK/manifest.json\" <<'DATA'\n{\n  \"schema\": \"zeno-fcis/loop-family-manifest/1\",\n  \"family\": \"identity\",\n  \"vocabulary\": {\n    \"inputs\": [\n      \"x\"\n    ]\n  },\n  \"instances\": [\n    {\n      \"name\": \"small\",\n      \"parameters\": {\n        \"C\": 3\n      },\n      \"program\": \"program.zcve\"\n    }\n  ]\n}\nDATA\ncat > \"$WORK/expressions.json\" <<'DATA'\n{\n  \"schema\": \"zeno-fcis/loop-expressions/1\",\n  \"vocabulary\": {\n    \"inputs\": [\n      \"x\"\n    ]\n  },\n  \"parameters\": {\n    \"C\": 3\n  },\n  \"outputs\": [\n    \"x\"\n  ]\n}\nDATA",
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
    "command": "zeno-fcis loop transfer --from \"$WORK/expressions.json\" --to-instances \"$WORK/manifest.json\"",
    "exit": 0,
    "expected": {
      "status": "transferred",
      "schema": "zeno-fcis/loop-doors/2"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-loop-doors/src/family.rs; crates/zeno-fcis-loop-doors/src/repair.rs",
    "engine": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-loop-doors/src/family.rs; crates/zeno-fcis-loop-doors/src/repair.rs",
    "inputs": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-loop-doors/src/family.rs; crates/zeno-fcis-loop-doors/src/repair.rs",
    "outputs": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-loop-doors/src/family.rs; crates/zeno-fcis-loop-doors/src/repair.rs",
    "exit_codes": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-loop-doors/src/family.rs; crates/zeno-fcis-loop-doors/src/repair.rs",
    "verdicts": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-loop-doors/src/family.rs; crates/zeno-fcis-loop-doors/src/repair.rs",
    "limits": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-loop-doors/src/family.rs; crates/zeno-fcis-loop-doors/src/repair.rs",
    "refusals": "crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-loop-doors/src/family.rs; crates/zeno-fcis-loop-doors/src/repair.rs"
  },
  "commands": [
    "loop transfer"
  ],
  "pitfalls": "Each success is checked on its listed target reference only; there is no claim for unlisted parameters. Fitting and repair are proposals. No owner adoption, session or attempt is created."
}
```

# Try a source program on listed target instances

## Use when

transfer a program to new instances; recheck source constants on target domains.

## Not for

unlisted instance coverage; universal parameter theorems; treating fitted formulas as verified transfers.

## Inputs

Manifest: ASCII zeno-fcis/loop-family-manifest/1 with family, vocabulary and 1..10 instances. Each has name, parameters (signed-64-bit JSON integers), relative program path, optional optimize path and fit Boolean (default true). Absolute paths and parent traversal are refused. Files/programs <=65,536 bytes; each instance <=131,072 tuples.

Transfer --from is zeno-fcis/loop-expressions/1 with source vocabulary, literal constants and signed-64-bit source parameter metadata. Source $PARAMETERS are refused. Try direct use, constants near changed parameters as choice holes, then repair.

Engine: Per-instance expression lowering or bounded sketch/repair proposes; every successful instance requires transform check and matching receipt replay.

Synopsis: `zeno-fcis loop transfer --from JSON --to-instances JSON`

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
    ]
  ],
  "roots": [
    0
  ]
}
DATA
cat > "$WORK/manifest.json" <<'DATA'
{
  "schema": "zeno-fcis/loop-family-manifest/1",
  "family": "identity",
  "vocabulary": {
    "inputs": [
      "x"
    ]
  },
  "instances": [
    {
      "name": "small",
      "parameters": {
        "C": 3
      },
      "program": "program.zcve"
    }
  ]
}
DATA
cat > "$WORK/expressions.json" <<'DATA'
{
  "schema": "zeno-fcis/loop-expressions/1",
  "vocabulary": {
    "inputs": [
      "x"
    ]
  },
  "parameters": {
    "C": 3
  },
  "outputs": [
    "x"
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

`transferred`: Exit 0 only if all listed targets transfer.

`failed`: Exit 1 if none transfer, including per-target budget stops.

`partial`: Exit 2 when only some transfer; inspect every row.

`refused`: Exit 1 for invalid source or target input.

Exit codes: {"0": "Completed report; read the operation-specific verdict and scope.", "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.", "2": "Inconclusive, incomplete or unavailable containment; no new acceptance.", "3": "I/O failure; see operation-specific exceptions.", "64": "Invalid command-line usage."}

Outputs: stdout zeno-fcis/loop-doors/2, door=transfer, authority=none: detail rows with programs/check/replay digests and coverage of the listed instances. No receipt files, session or attempts are exported.

## Cost

Read reported work reservations and scope; these are not total CPU-time measurements.

Limits: {"check_stage_ms": 2000, "file_bytes": 65536, "input_tuples_per_instance": 131072, "instances": 10, "invocation_control_frames": 62, "invocation_ms": 22000, "output_expressions": 256, "program_inputs_cap": 32, "program_nodes_cap": 256, "program_outputs_cap": 16, "search_stage_ms": 5000}

## Pitfalls

Each success is checked on its listed target reference only; there is no claim for unlisted parameters. Fitting and repair are proposals. No owner adoption, session or attempt is created.

Refusals and next steps: {"manifest-path": "Keep target paths relative to the manifest.", "transfer-source-parameters": "Give source parameter values and literal-constant expressions."}

## Worked example

```sh
zeno-fcis loop transfer --from "$WORK/expressions.json" --to-instances "$WORK/manifest.json"
```

Expected: exit 0; {"schema": "zeno-fcis/loop-doors/2", "status": "transferred"}.

## See also

**Background.** Sumit Gulwani, Susmit Jha, Ashish Tiwari and Ramarathnam Venkatesan, “Synthesis of Loop-free Programs,” ACM PLDI, 2011, is background for finite straight-line program synthesis; it does not establish extrapolation to unlisted instances.

**What is new here.** This card documents the implemented interface; it adds no algorithm or priority claim.

**Scope.** Each success is checked on its listed target reference only; there is no claim for unlisted parameters. Fitting and repair are proposals. No owner adoption, session or attempt is created.

Sources: crates/zeno-fcis-cli/src/loop_doors_command.rs; crates/zeno-fcis-loop-doors/src/family.rs; crates/zeno-fcis-loop-doors/src/repair.rs

[CLI reference](../CLI_REFERENCE.md); [loop overview](loop-workspace.md).
