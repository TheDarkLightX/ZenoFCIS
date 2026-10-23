#!/usr/bin/env python3
"""Finite learn semantic cases and planted-fault controls through the real CLI.

No refused, inconclusive, missing or unexpected row can count as a pass. All
complete counts, minimum sizes, questions, conflicts and lowered examples are
compared with a separate brute-force syntax reference. Comparison self-tests
must fail when stubbed. This is bounded test evidence, never release authority.
"""
from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path
import subprocess
import tempfile

import loop_learn_reference as R
from test_loop_learn_reference import comparison_self_test

CASES = Path(__file__).parent / "test_data/loop_learn/cases.json"
# Finite-data commands stay on POSIX even when the environment selects systemd.
FINITE_DATA = ("--containment-backend", "auto")


def call(cli, arguments):
    try:
        done = subprocess.run([str(cli), *FINITE_DATA, *map(str, arguments)], capture_output=True, timeout=35,
                              check=False)
        if len(done.stdout) > 8 << 20:
            raise ValueError("control-output-limit")
        report = json.loads(done.stdout)
        if type(report) is not dict:
            raise ValueError("control-report-shape")
        return done.returncode, report
    except (OSError, ValueError, subprocess.TimeoutExpired) as e:
        # Transport/timeout/format failure is a failed row; continue the corpus.
        return -1, {"status": "control-error", "authority": "none", "detail": {"reason": str(e)[:2000]}}


def write(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=1) + "\n")
    return path


def run(cli, work):
    rows = []
    for case in json.loads(CASES.read_text())["cases"]:
        folder = work / case["id"]
        folder.mkdir()
        spec = case["spec"]
        path = write(folder / "declaration.json", spec)
        code, report = call(cli, ["loop", "learn", "--spec", path])
        write(folder / "report.json", report)
        expected_code = {"determined": 0, "question": 2, "outside-vocabulary": 1}[case["status"]]
        passed = code == expected_code and report.get("status") == case["status"] and R.check_report(spec, report)
        rows.append({"case": case["id"], "control": "independent-reference", "passed": passed})
        if not passed:
            continue
        for key in ("counts", "question", "core"):
            mutant = copy.deepcopy(report)
            detail = mutant["detail"]
            if key == "counts":
                detail[key][0] += 1
            elif key == "question" and detail[key] is not None:
                detail[key]["ordinal"] += 1
            elif key == "core" and detail[key] is not None:
                detail[key] = [] if detail[key] else [0]
            else:
                continue
            rows.append({"case": case["id"], "control": "planted-" + key,
                         "passed": not R.check_report(spec, mutant)})
        candidate = report["detail"].get("candidate")
        if candidate is not None:
            original_json = write(folder / "candidate.json", candidate["program"])
            original = folder / "candidate.zcve"
            encoded, _ = call(cli, ["loop", "encode", "--program", original_json, "--out", original])
            if encoded != 0:
                rows.append({"case": case["id"], "control": "canonical-encoding", "passed": False})
                continue
            planted = copy.deepcopy(candidate["program"])
            output = planted["outputs"][0]
            if output["kind"] == "Bool":
                index = len(planted["nodes"])
                planted["nodes"].append(["Not", planted["roots"][0]])
            else:
                index = len(planted["nodes"]) + 1
                planted["nodes"].extend([["Int", "1"], ["Add", planted["roots"][0], index - 1]])
            planted["roots"][0] = index
            planted_json = write(folder / "fault.json", planted)
            fault = folder / "fault.zcve"
            encoded, _ = call(cli, ["loop", "encode", "--program", planted_json, "--out", fault])
            if encoded != 0:
                rows.append({"case": case["id"], "control": "planted-program", "passed": False})
                continue
            code, checked = call(cli, ["transform", "check", "--original", original, "--candidate", fault])
            write(folder / "fault-check.json", checked)
            rows.append({"case": case["id"], "control": "planted-program", "passed": code == 1
                         and checked.get("status") == "counterexample"})
        # Every requested budget path is tested even when enumeration is small.
        flags = ["max-work", "max-tuples", "max-cells", "max-meanings"]
        if case["status"] == "outside-vocabulary":
            flags.append("max-conflict-subsets")
        for flag in flags:
            code, stopped = call(cli, ["loop", "learn", "--spec", path, "--" + flag, "0"])
            detail = stopped.get("detail", {})
            rows.append({"case": case["id"], "control": flag,
                         "passed": code == 2 and stopped.get("status") == "inconclusive"
                         and all(detail.get(k) is None for k in ("counts", "question", "candidate", "core"))
                         and detail.get("attempts_spent") == 0})
    kinds = {r["control"] for r in rows if r["passed"]}
    required = {"independent-reference", "planted-counts", "planted-question", "planted-core", "planted-program",
                "max-work", "max-tuples", "max-cells", "max-meanings", "max-conflict-subsets"}
    return {"rows": rows, "required_controls": sorted(required), "applicable": len(rows),
            "passed": bool(rows) and required <= kinds and all(r["passed"] for r in rows), "authority": "none"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--work", type=Path)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    if args.self_test:
        good = comparison_self_test()
        print(json.dumps({"self_test": "passed" if good else "failed"}))
        return 0 if good else 1
    if args.cli is None or args.out is None:
        parser.error("--cli and --out are required")
    if args.work:
        args.work.mkdir(exist_ok=False, parents=True)
        report = run(args.cli, args.work)
    else:
        with tempfile.TemporaryDirectory(prefix="loop-learn-controls-") as temporary:
            report = run(args.cli, Path(temporary))
    write(args.out, report)
    print(json.dumps({"passed": report["passed"], "applicable": report["applicable"], "authority": "none"}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
