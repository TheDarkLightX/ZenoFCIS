#!/usr/bin/env python3
"""L1/L2 controls and equal-budget baseline comparison (stdlib, test evidence).

Native work requires explicit --cli and --baseline-cli. Self-tests exercise the
independent report predicates only and never claim a native receipt or speedup.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import subprocess
import time

import loop_composable_reference as C
import loop_learn_reference as L
import program_search_reference as IR

SCHEMA = "zeno-fcis/loop-completion-controls/1"
BOUNDS = {"max_tuples": 4096, "max_work": 2_000_000, "max_cells": 1_000_000,
          "max_meanings": 4096, "max_conflict_subsets": 10_000,
          "max_gate_pairs": 1_000_000, "max_checks": 24, "deadline_ms": 22_000}


def equal(left, right):
    """Load-bearing comparison seam; disabling it must fail the meta-control."""
    return type(left) is type(right) and left == right


def verified_report(report, rows, reference_sha):
    detail = report.get("detail", {})
    capture = detail.get("capture") or {}
    check = detail.get("best_check") or {}
    return all((equal(report.get("status"), "qualified"),
                equal(capture.get("status"), "captured"),
                equal(capture.get("complete"), True), equal(capture.get("rows"), rows),
                equal(capture.get("reference_sha256"), reference_sha),
                equal(capture.get("check", {}).get("verdict"), "equivalent"),
                equal(capture.get("check", {}).get("replay"), "matched"),
                equal(check.get("verdict"), "equivalent"), equal(check.get("replay"), "matched"),
                equal(check.get("original_sha256"), reference_sha)))


def diagnosed(report, bound, limit_key=None, limit=None):
    detail = report.get("detail", {})
    diagnosis = detail.get("search_limit") or {}
    return (equal(diagnosis.get("bound"), bound)
            and equal(diagnosis.get("action"), "request-better-primitive")
            and equal(diagnosis.get("retry_attempted"), False)
            and (limit_key is None or equal((diagnosis.get("limits") or {}).get(limit_key), limit)))


def fixture_report():
    capture = {"status": "captured", "complete": True, "rows": 9,
               "reference_sha256": "a" * 64, "check": {"verdict": "equivalent", "replay": "matched"}}
    return {"status": "qualified", "detail": {"capture": capture,
            "best_check": {"verdict": "equivalent", "replay": "matched", "original_sha256": "a" * 64}}}


def self_test(stub=False):
    global equal
    original = equal
    if stub:
        equal = lambda *args: True
    try:
        good = fixture_report()
        specimens = [("verified-positive", good, True)]
        for field, value in [("capture", None), ("best_check", {}),
                             ("capture.rows", 8), ("capture.complete", False),
                             ("capture.rows", True), ("capture.reference_sha256", "b" * 64),
                             ("capture.check.replay", "differs"), ("best_check.replay", "differs"),
                             ("best_check.verdict", "counterexample")]:
            mutant = deepcopy(good)
            cursor = mutant["detail"]
            parts = field.split(".")
            for part in parts[:-1]:
                cursor = cursor[part]
            cursor[parts[-1]] = value
            specimens.append(("planted-" + field, mutant, False))
        fitted = deepcopy(good)
        fitted["status"] = "determined"
        specimens.append(("fit-is-not-verified", fitted, False))
        failures = [name for name, report, expected in specimens
                    if verified_report(report, 9, "a" * 64) is not expected]
        diagnosis = {"detail": {"search_limit": {"bound": "work", "action": "request-better-primitive",
                     "retry_attempted": False, "limits": {"max_work": 7}}}}
        for name, report, expected in [("diagnosis-positive", diagnosis, True),
                ("missing-diagnosis", {"detail": {}}, False),
                ("wrong-limit", {"detail": {"search_limit": {**diagnosis["detail"]["search_limit"],
                                                           "limits": {"max_work": 8}}}}, False)]:
            if diagnosed(report, "work", "max_work", 7) is not expected:
                failures.append(name)
        return failures
    finally:
        equal = original


def fixtures():
    inputs = [{"kind": "Int", "min": "0", "max": "2"}] * 2
    outputs = [{"kind": "Int", "min": "-100", "max": "100"}]
    summed = {"inputs": inputs, "outputs": outputs,
              "nodes": [["Input", 0], ["Input", 1], ["Add", 0, 1]], "roots": [2]}
    learned = {"schema": L.SCHEMA, "inputs": [{"name": name, "domain": {
        "kind": "Int", "min": 0, "max": 2}} for name in ("x", "y")],
        "outputs": [{"kind": "Int", "min": -100, "max": 100}], "max_size": 3,
        "vocabulary": {"constants": [1], "arithmetic": ["+", "-"]},
        "examples": [{"input": [x, y], "output": [x + y]} for x in range(3) for y in range(3)]}
    wide = {"kind": "Int", "min": str(IR.I64_MIN), "max": str(IR.I64_MAX)}
    piecewise = {"inputs": [{"kind": "Int", "min": "0", "max": "3"}], "outputs": [wide],
        "nodes": [["Input", 0], ["Int", "2"], ["Lt", 0, 1], ["Int", "7"], ["Select", 2, 3, 0]], "roots": [4]}
    case = {"schema": "zeno-fcis/loop-strategy-problem/1", "learn": {
        "schema": "zeno-fcis/learn-terms/1", "original_sha256": hashlib.sha256(C.encoding(C.program(piecewise))).hexdigest(),
        "status": "complete", "integers": [{**piecewise, "nodes": [["Int", "7"]], "roots": [0]},
        {**piecewise, "nodes": [["Input", 0]], "roots": [0]}], "predicates": [{**piecewise,
        "outputs": [{"kind": "Bool"}], "nodes": piecewise["nodes"][:3], "roots": [2]}]},
        "config": {"initial": 2, "spot_checks": 1}}
    return [("learn-sum", "learn", learned, summed), ("split-piecewise", "split", case, piecewise)]


def campaign(cli, baseline_cli, work, reverse_order=False):
    work.mkdir(parents=True, exist_ok=False)
    def write(path, value):
        path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="ascii")
        return path
    def call(binary, folder, label, *argv):
        begun = time.monotonic()
        done = subprocess.run([str(binary), *map(str, argv)], capture_output=True, timeout=35, check=False)
        elapsed = time.monotonic() - begun
        if len(done.stdout) + len(done.stderr) > 8 << 20:
            raise ValueError("output-limit")
        (folder / (label + ".stdout")).write_bytes(done.stdout)
        (folder / (label + ".stderr")).write_bytes(done.stderr)
        report = json.loads(done.stdout)
        return done.returncode, report, elapsed
    comparisons = []
    controls = []
    control_folder = work / "controls"
    control_folder.mkdir()
    base_learn = fixtures()[0][2]
    no_work = write(control_folder / "no-work.json", base_learn)
    code, report, _ = call(cli, control_folder, "no-work", "loop", "learn", "--spec", no_work, "--max-work", 0)
    controls.append({"id": "work-limit-named", "passed": code == 2 and diagnosed(report, "work", "max_work", 0), "report": report})
    primitive = deepcopy(base_learn)
    primitive["vocabulary"]["macros"] = ["half"]
    for entry in primitive["inputs"]:
        entry["domain"]["max"] = 7
    primitive["outputs"] = [{"kind": "Int", "min": 0, "max": 3}]
    primitive["examples"] = [{"input": [x, y], "output": [(x+y)//4]} for x in range(8) for y in range(8)]
    primitive_path = write(control_folder / "unsupported-primitive.json", primitive)
    code, report, _ = call(cli, control_folder, "unsupported-primitive", "loop", "learn", "--spec", primitive_path)
    controls.append({"id": "documented-quarter-task-native-vocabulary-block-named", "passed": code == 1
                     and diagnosed(report, "supported-lowerable-vocabulary"), "report": report})
    four = {"schema": L.SCHEMA, "inputs": [{"name": name, "domain": {
        "kind": "Int", "min": -1, "max": 1}} for name in ("x", "y")],
        "outputs": [{"kind": "Int", "min": 0, "max": 2}],
        "vocabulary": {"constants": [], "arithmetic": ["+"], "macros": ["abs"]},
        "max_size": 3, "examples": [{"input": [x, y], "output": [abs(x+y)]}
                                   for x in range(-1, 2) for y in range(-1, 2)]}
    small = write(control_folder / "size-three.json", four)
    code, report, _ = call(cli, control_folder, "size-three", "loop", "learn", "--spec", small)
    controls.append({"id": "four-node-macro-block-named", "passed": code == 1
                     and diagnosed(report, "max_size-and-declared-vocabulary", "max_size", 3), "report": report})
    four["max_size"] = 4
    larger = write(control_folder / "size-four.json", four)
    code, report, _ = call(cli, control_folder, "size-four", "loop", "learn", "--spec", larger)
    controls.append({"id": "explicit-size-increase-recovers", "passed": code == 0 and L.check_report(four, report), "report": report})
    for identifier, kind, problem, program in fixtures():
        subject = work / (identifier + ".zcve")
        subject.write_bytes(C.encoding(C.program(program)))
        declaration = write(work / (identifier + ".json"), problem)
        reference_sha = hashlib.sha256(subject.read_bytes()).hexdigest()
        count = len(IR.table(IR.decode(subject.read_bytes())))
        arms = []
        order = [("baseline", baseline_cli), ("automatic", cli)]
        if reverse_order:
            order.reverse()
        for arm, binary in order:
            folder = work / (identifier + "-" + arm)
            folder.mkdir()
            export = folder / "export"
            if kind == "learn":
                argv = ["loop", "learn", "--spec", declaration, "--max-work", BOUNDS["max_work"],
                        "--max-tuples", BOUNDS["max_tuples"], "--max-cells", BOUNDS["max_cells"],
                        "--max-meanings", BOUNDS["max_meanings"], "--max-conflict-subsets", BOUNDS["max_conflict_subsets"]]
                if arm == "automatic":
                    argv += ["--original", subject, "--out", export]
            else:
                argv = ["loop", "strategy", "--kind", "split-by-cases", "--problem", declaration,
                        "--original", subject, "--out", export, "--max-tuples", BOUNDS["max_tuples"],
                        "--max-search-work", BOUNDS["max_work"], "--max-cells", BOUNDS["max_cells"],
                        "--max-gate-pairs", BOUNDS["max_gate_pairs"], "--max-checks", BOUNDS["max_checks"],
                        "--deadline-ms", BOUNDS["deadline_ms"]]
            begun = time.monotonic()
            code, report, _ = call(binary, folder, "loop", *argv)
            failures = []
            candidate = export / "candidate.zcve"
            if kind == "learn" and arm == "baseline":
                # Retain the old manual finishing workflow as the comparator.
                fitted = report.get("detail", {}).get("candidate")
                if code != 0 or not fitted or not L.check_report(problem, report):
                    failures.append("baseline-learn-fit")
                else:
                    export.mkdir()
                    candidate.write_bytes(C.encoding(C.program(fitted["program"])))
                    receipt = export / "manual-native.receipt"
                    checked, check, _ = call(binary, folder, "manual-check", "transform", "check",
                                             "--original", subject, "--candidate", candidate, "--receipt", receipt)
                    replayed, replay, _ = call(binary, folder, "manual-replay", "transform", "replay",
                                               "--original", subject, "--candidate", candidate, "--receipt", receipt)
                    if checked != 0 or replayed != 0 or check.get("status") != "equivalent" or replay.get("status") != "replayed":
                        failures.append("baseline-manual-check-replay")
            elif code != 0 or report.get("status") != "qualified":
                failures.append("loop-not-qualified")
            elif arm == "automatic" and not verified_report(report, count, reference_sha):
                failures.append("automatic-capture-check-path")
            first_verified = time.monotonic() - begun
            if first_verified > BOUNDS["deadline_ms"] / 1000 or count * 4 > BOUNDS["max_gate_pairs"]:
                failures.append("comparison-budget")
            # Independent checks are validation overhead, outside solution time.
            pairs = [("native", subject)]
            if arm == "automatic":
                pairs.append(("capture", export / "capture-reference.zcve"))
            if candidate.is_file():
                expected = C.comparison(IR.decode(subject.read_bytes()), IR.decode(candidate.read_bytes()))
                if expected["status"] != "equivalent":
                    failures.append("independent-table")
                for label, original in pairs:
                    if not original.is_file():
                        failures.append(label + "-reference-missing")
                        continue
                    receipt = export / (label + ".independent.receipt")
                    checked, check, _ = call(binary, folder, label + "-check", "transform", "check",
                                             "--original", original, "--candidate", candidate, "--receipt", receipt)
                    replayed, replay, _ = call(binary, folder, label + "-replay", "transform", "replay",
                                               "--original", original, "--candidate", candidate, "--receipt", receipt)
                    if checked != 0 or replayed != 0 or check.get("status") != "equivalent" or replay.get("status") != "replayed":
                        failures.append(label + "-independent-replay")
                    if arm == "automatic" and receipt.is_file():
                        claimed = report["detail"]["best_check"] if label == "native" else report["detail"]["capture"]["check"]
                        if hashlib.sha256(receipt.read_bytes()).hexdigest() != claimed.get("receipt_sha256"):
                            failures.append(label + "-receipt-binding")
            else:
                failures.append("candidate-missing")
            arms.append({"arm": arm, "fully_verified": not failures, "failures": failures,
                         "time_to_first_verified_seconds": first_verified if not failures else None,
                         "model_calls": 0, "model_cost": 0, "owner_questions": 0,
                         "final_program_cost": {"nodes": len(IR.decode(candidate.read_bytes()).nodes),
                                                "bytes": candidate.stat().st_size} if candidate.is_file() else None,
                         "report": report})
        by_arm = {entry["arm"]: entry for entry in arms}
        comparisons.append({"task": identifier, "bounds": BOUNDS, "arms": arms,
                            "regression": by_arm["baseline"]["fully_verified"] and not by_arm["automatic"]["fully_verified"]})
    return {"schema": SCHEMA, "authority": "none", "bounds": BOUNDS,
            "baseline_cli_sha256": hashlib.sha256(baseline_cli.read_bytes()).hexdigest(),
            "cli_sha256": hashlib.sha256(cli.read_bytes()).hexdigest(), "comparisons": comparisons,"controls":controls,
            "arm_order": [entry[0] for entry in order],
            "scope": "two fixed finite tasks; old manual learn finishing and old split qualification; automation convenience only",
            "passed": all(arm["fully_verified"] for row in comparisons for arm in row["arms"])
                      and all(row["passed"] for row in controls)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--stub-comparison", action="store_true")
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--baseline-cli", type=Path)
    parser.add_argument("--work", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--reverse-order", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        failures = self_test(args.stub_comparison)
        print(json.dumps({"authority": "none", "native_executed": False, "failures": failures}))
        return int(bool(failures))
    if args.stub_comparison or not all((args.cli, args.baseline_cli, args.work, args.out)):
        parser.error("native comparison requires --cli --baseline-cli --work --out; stubs are self-test only")
    try:
        result = campaign(args.cli.resolve(), args.baseline_cli.resolve(), args.work, args.reverse_order)
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        result = {"schema": SCHEMA, "authority": "none", "passed": False, "error": str(error)}
    with args.out.open("x", encoding="ascii") as stream:
        json.dump(result, stream, sort_keys=True, indent=2)
        stream.write("\n")
    print(json.dumps({"passed": result["passed"]}))
    return int(not result["passed"])


if __name__ == "__main__":
    raise SystemExit(main())
