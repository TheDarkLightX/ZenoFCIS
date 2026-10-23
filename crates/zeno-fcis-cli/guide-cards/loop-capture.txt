```json
{
  "name": "loop-capture",
  "title": "Capture finite reference observations and check proposals",
  "synopsis": "zeno-fcis loop capture {whole|open|packet|candidate|status} OPTIONS",
  "engine": "Complete repeated executable observations build a finite reference; transform check and matching receipt replay alone qualify a complete candidate.",
  "use_when": [
    "capture a finite reference",
    "submit a capture candidate",
    "read public capture examples"
  ],
  "not_for": [
    "authenticated owner roles",
    "forwarding private status to a proposer",
    "partial observation agreement as acceptance"
  ],
  "inputs": [
    "whole/open: --spec ABI with exactly inputs/outputs in program JSON, --reference Python source defining reference(point) -> list[int]. Files <=65,536 bytes. whole also requires --candidate-json or --expressions; it requires nonnegative Int inputs. open takes a new private --session; only reference format python-function-v1 is supported.",
    "packet/status read an existing capture session; candidate takes that session and --candidate-json (finite program JSON) or --expressions (zeno-fcis/capture-expressions/1, outputs over x0..xN, no holes/parameters/keep/output references). --full is only for private status.",
    "Select --containment-backend systemd for whole/open; keep the default/POSIX backend for packet/candidate/status. Normal open/whole execution requires Linux x86-64, the contained Python runtime, Landlock ABI >=3 and libseccomp. The POSIX finite-data backend cannot supply the Python-reference guarantees; the worked example explicitly selects it and demonstrates refusal without fallback."
  ],
  "outputs": [
    "Normal handlers emit zeno-fcis/black-box-capture/2, authority=none. Outer containment refusal/stop diagnostics use zeno-fcis/black-box-capture/1 (unavailable=refused/1, interrupted or exhausted=inconclusive/2). detail.packet exposes only profile/abi/round/examples; status is private. whole reports complete coverage and chained receipt digests, not one exported whole-program receipt. Candidate success retains its receipt privately.",
    "Where integer_encoding is present, scalar report integers use safe-number-or-decimal-string/1. Program JSON bounds/literals remain decimal strings; node references remain JSON integers."
  ],
  "exit_codes": {
    "0": "Completed report; read the operation-specific verdict and scope.",
    "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.",
    "2": "Inconclusive, incomplete or an outer containment stop; no new acceptance.",
    "64": "Invalid command-line usage."
  },
  "verdicts": {
    "opened/packet": "Exit 0 creates a private capture or returns an allowlisted packet; neither accepts a candidate.",
    "captured-exact": "Exit 0 after complete-table transform check/replay. whole covers every page; candidate closes the capture session.",
    "precheck-refuted/candidate-refused/refuted/refused": "Exit 1. Decided malformed/ABI-mismatched/example-refuted or equivalence/counterexample candidates consume an attempt; setup/checker refusal or budget stops can precede one.",
    "inconclusive": "Exit 2 for an outer stop or incomplete coverage; reservations stay charged. Unavailable containment instead exits 1 refused under the /1 diagnostic schema.",
    "status": "Uses the stored state exit class (ready/opened/success 0, decided refusal/refutation 1, inconclusive 2), subject to fresh validation/replay."
  },
  "limits": {
    "input_file_bytes": 65536,
    "open_tuples": 4096,
    "open_table_nodes": 256,
    "rounds_default": 32,
    "rounds_cap": 64,
    "examples_default_and_cap": 4096,
    "node_visits_default": 2000000,
    "node_visits_cap": 10000000,
    "session_bytes": 2097152,
    "whole_page_tuples_cap": 4096,
    "whole_pages_cap": 20,
    "invocation_control_frames": 62,
    "full_status_output_bytes": 1048576
  },
  "refusals": {
    "capture-containment-unavailable": "Use the required runtime; do not execute a reference outside containment.",
    "capture-full-status-output-bound": "Request summary status and keep owner evidence private.",
    "candidate ABI": "Use exactly the frozen capture ABI."
  },
  "cost": {
    "units": "Read reported work reservations and scope; these are not total CPU-time measurements.",
    "timing": "No cross-host timing claim."
  },
  "steps": [
    {
      "command": "cat > \"$WORK/abi.json\" <<'DATA'\n{\n  \"inputs\": [\n    {\n      \"kind\": \"Int\",\n      \"min\": \"0\",\n      \"max\": \"1\"\n    }\n  ],\n  \"outputs\": [\n    {\n      \"kind\": \"Int\",\n      \"min\": \"0\",\n      \"max\": \"1\"\n    }\n  ]\n}\nDATA\ncat > \"$WORK/reference.py\" <<'DATA'\ndef reference(point):\n    return [point[0]]\nDATA",
      "exit": 0,
      "expected": {},
      "contains": []
    }
  ],
  "example": {
    "command": "zeno-fcis --containment-backend posix loop capture open --spec \"$WORK/abi.json\" --reference \"$WORK/reference.py\" --session \"$WORK/capture\"",
    "exit": 1,
    "expected": {
      "status": "refused",
      "schema": "zeno-fcis/black-box-capture/1",
      "detail.reason": "capture-containment-unavailable"
    },
    "contains": []
  },
  "sources": {
    "synopsis": "crates/zeno-fcis-cli/src/loop_capture.rs; crates/zeno-fcis-cli/src/loop_capture_whole.rs; crates/zeno-fcis-cli/src/capture_runtime.rs; crates/zeno-fcis-cli/src/checker_process.rs",
    "engine": "crates/zeno-fcis-cli/src/loop_capture.rs; crates/zeno-fcis-cli/src/loop_capture_whole.rs; crates/zeno-fcis-cli/src/capture_runtime.rs; crates/zeno-fcis-cli/src/checker_process.rs",
    "inputs": "crates/zeno-fcis-cli/src/loop_capture.rs; crates/zeno-fcis-cli/src/loop_capture_whole.rs; crates/zeno-fcis-cli/src/capture_runtime.rs; crates/zeno-fcis-cli/src/checker_process.rs",
    "outputs": "crates/zeno-fcis-cli/src/loop_capture.rs; crates/zeno-fcis-cli/src/loop_capture_whole.rs; crates/zeno-fcis-cli/src/capture_runtime.rs; crates/zeno-fcis-cli/src/checker_process.rs",
    "exit_codes": "crates/zeno-fcis-cli/src/loop_capture.rs; crates/zeno-fcis-cli/src/loop_capture_whole.rs; crates/zeno-fcis-cli/src/capture_runtime.rs; crates/zeno-fcis-cli/src/checker_process.rs",
    "verdicts": "crates/zeno-fcis-cli/src/loop_capture.rs; crates/zeno-fcis-cli/src/loop_capture_whole.rs; crates/zeno-fcis-cli/src/capture_runtime.rs; crates/zeno-fcis-cli/src/checker_process.rs",
    "limits": "crates/zeno-fcis-cli/src/loop_capture.rs; crates/zeno-fcis-cli/src/loop_capture_whole.rs; crates/zeno-fcis-cli/src/capture_runtime.rs; crates/zeno-fcis-cli/src/checker_process.rs",
    "refusals": "crates/zeno-fcis-cli/src/loop_capture.rs; crates/zeno-fcis-cli/src/loop_capture_whole.rs; crates/zeno-fcis-cli/src/capture_runtime.rs; crates/zeno-fcis-cli/src/checker_process.rs"
  },
  "commands": [
    "loop capture",
    "loop capture open",
    "loop capture whole",
    "loop capture packet",
    "loop capture candidate",
    "loop capture status"
  ],
  "pitfalls": "Exactness is restricted to the complete observed finite table under a deterministic-reference assumption. Role labels are conventions, not authenticated identities. Keep source, full table, session and status private; public packets and example agreement grant no acceptance."
}
```

# Capture finite reference observations and check proposals

## Use when

capture a finite reference; submit a capture candidate; read public capture examples.

## Not for

authenticated owner roles; forwarding private status to a proposer; partial observation agreement as acceptance.

## Inputs

whole/open: --spec ABI with exactly inputs/outputs in program JSON, --reference Python source defining reference(point) -> list[int]. Files <=65,536 bytes. whole also requires --candidate-json or --expressions; it requires nonnegative Int inputs. open takes a new private --session; only reference format python-function-v1 is supported.

packet/status read an existing capture session; candidate takes that session and --candidate-json (finite program JSON) or --expressions (zeno-fcis/capture-expressions/1, outputs over x0..xN, no holes/parameters/keep/output references). --full is only for private status.

Select --containment-backend systemd for whole/open; keep the default/POSIX backend for packet/candidate/status. Normal open/whole execution requires Linux x86-64, the contained Python runtime, Landlock ABI >=3 and libseccomp. The POSIX finite-data backend cannot supply the Python-reference guarantees; the worked example explicitly selects it and demonstrates refusal without fallback.

Engine: Complete repeated executable observations build a finite reference; transform check and matching receipt replay alone qualify a complete candidate.

Synopsis: `zeno-fcis loop capture {whole|open|packet|candidate|status} OPTIONS`

Run from the repository root with zeno-fcis on PATH. Set WORK to a fresh writable scratch directory outside the source tree.



Supported command forms (normal success requires the stated runtime):

```text
zeno-fcis --containment-backend systemd loop capture whole --spec ABI.json --reference REF.py (--candidate-json JSON | --expressions JSON)
zeno-fcis --containment-backend systemd loop capture open --spec ABI.json --reference REF.py --session NEWDIR
zeno-fcis loop capture packet --session DIR
zeno-fcis loop capture candidate --session DIR (--candidate-json JSON | --expressions JSON)
zeno-fcis loop capture status --session DIR [--full]
```

## Steps

```sh
cat > "$WORK/abi.json" <<'DATA'
{
  "inputs": [
    {
      "kind": "Int",
      "min": "0",
      "max": "1"
    }
  ],
  "outputs": [
    {
      "kind": "Int",
      "min": "0",
      "max": "1"
    }
  ]
}
DATA
cat > "$WORK/reference.py" <<'DATA'
def reference(point):
    return [point[0]]
DATA
```

Expected: exit 0; {}.

## Verdicts

`opened/packet`: Exit 0 creates a private capture or returns an allowlisted packet; neither accepts a candidate.

`captured-exact`: Exit 0 after complete-table transform check/replay. whole covers every page; candidate closes the capture session.

`precheck-refuted/candidate-refused/refuted/refused`: Exit 1. Decided malformed/ABI-mismatched/example-refuted or equivalence/counterexample candidates consume an attempt; setup/checker refusal or budget stops can precede one.

`inconclusive`: Exit 2 for an outer stop or incomplete coverage; reservations stay charged. Unavailable containment instead exits 1 refused under the /1 diagnostic schema.

`status`: Uses the stored state exit class (ready/opened/success 0, decided refusal/refutation 1, inconclusive 2), subject to fresh validation/replay.

Exit codes: {"0": "Completed report; read the operation-specific verdict and scope.", "1": "Refused input or refuted proposal, except the explicitly documented aggregate cases.", "2": "Inconclusive, incomplete or an outer containment stop; no new acceptance.", "64": "Invalid command-line usage."}

Outputs: Normal handlers emit zeno-fcis/black-box-capture/2, authority=none. Outer containment refusal/stop diagnostics use zeno-fcis/black-box-capture/1 (unavailable=refused/1, interrupted or exhausted=inconclusive/2). detail.packet exposes only profile/abi/round/examples; status is private. whole reports complete coverage and chained receipt digests, not one exported whole-program receipt. Candidate success retains its receipt privately.

## Cost

Read reported work reservations and scope; these are not total CPU-time measurements.

Limits: {"examples_default_and_cap": 4096, "full_status_output_bytes": 1048576, "input_file_bytes": 65536, "invocation_control_frames": 62, "node_visits_cap": 10000000, "node_visits_default": 2000000, "open_table_nodes": 256, "open_tuples": 4096, "rounds_cap": 64, "rounds_default": 32, "session_bytes": 2097152, "whole_page_tuples_cap": 4096, "whole_pages_cap": 20}

## Coordinate

Use Commons advise before a checker loop and lease the exact subject. Record the command, exit, verdict, scope and retained evidence, then release the lease. Coordination records and telemetry grant no checker or publication authority.

## Pitfalls

Exactness is restricted to the complete observed finite table under a deterministic-reference assumption. Role labels are conventions, not authenticated identities. Keep source, full table, session and status private; public packets and example agreement grant no acceptance.

Refusals and next steps: {"candidate ABI": "Use exactly the frozen capture ABI.", "capture-containment-unavailable": "Use the required runtime; do not execute a reference outside containment.", "capture-full-status-output-bound": "Request summary status and keep owner evidence private."}

## Worked example

```sh
zeno-fcis --containment-backend posix loop capture open --spec "$WORK/abi.json" --reference "$WORK/reference.py" --session "$WORK/capture"
```

Expected: exit 1; {"detail.reason": "capture-containment-unavailable", "schema": "zeno-fcis/black-box-capture/1", "status": "refused"}.

## See also

**Background.** Dana Angluin, “Learning Regular Sets from Queries and Counterexamples,” Information and Computation 75(2), 1987, is background for observations and counterexamples; Tsun S. Chow, “Testing Software Design Modeled by Finite-State Machines,” IEEE Transactions on Software Engineering 4(3), 1978, is background for finite observation-based testing.

**What is new here.** This card documents the implemented interface; it adds no algorithm or priority claim.

**Scope.** Exactness is restricted to the complete observed finite table under a deterministic-reference assumption. Role labels are conventions, not authenticated identities. Keep source, full table, session and status private; public packets and example agreement grant no acceptance.

Sources: crates/zeno-fcis-cli/src/loop_capture.rs; crates/zeno-fcis-cli/src/loop_capture_whole.rs; crates/zeno-fcis-cli/src/capture_runtime.rs; crates/zeno-fcis-cli/src/checker_process.rs

[CLI reference](../CLI_REFERENCE.md); [loop overview](loop-workspace.md).
