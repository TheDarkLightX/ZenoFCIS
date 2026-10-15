#!/usr/bin/env python3
"""Real ZenoFCIS binary smoke test: NOT the fake checker used in adapter tests.

Uses `loop encode` and `transform check/replay` from an installed exact CLI.
A missing CLI is NOT a passing test (exit 77).
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from elite_archive import (DEFAULT_TIMEOUT, RECEIPT_SCHEMA, REPORT_SCHEMA,
                           ArchiveError, bounded_file, cli_binary, digest,
                           export_elite, init_archive, offer, replay_archive,
                           review_candidate, run_cli, view)

# Declared --max-input-tuples for every suite archive. Must stay above the
# largest case domain (family b has 5 tuples) and below archive caps.
SUITE_TUPLE_CAP = 64


def program(number_of_nots: int) -> dict:
    nodes = [["Input", 0]]
    for i in range(number_of_nots):
        nodes.append(["Not", i])
    return {"inputs": [{"kind": "Bool"}], "outputs": [{"kind": "Bool"}],
            "nodes": nodes, "roots": [len(nodes) - 1]}


def encode_program(cli: Path, work: Path, name: str, value: dict) -> Path:
    src = work / f"{name}.json"
    out = work / f"{name}.zcve"
    src.write_text(json.dumps(value, sort_keys=True) + "\n", encoding="utf-8")
    proc = subprocess.run([str(cli), "loop", "encode", "--program", str(src), "--out", str(out)],
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
    if proc.returncode != 0 or not out.is_file() or not out.stat().st_size:
        raise ArchiveError(f"real `loop encode` failed for {name}: {proc.stdout[:300]!r} {proc.stderr[:300]!r}")
    return out


def encode(cli: Path, work: Path, name: str, count: int) -> Path:
    return encode_program(cli, work, name, program(count))


def run_demo(cli: Path, work: Path) -> dict:
    work.mkdir(parents=True, exist_ok=False)
    original = encode(cli, work, "original-double-not", 2)
    identity = encode(cli, work, "elite-identity", 0)
    larger = encode(cli, work, "elite-four-nots", 4)
    wrong = encode(cli, work, "counterexample-one-not", 1)
    spec = work / "operator-example-requirement.txt"
    spec.write_text("For the declared Boolean input x, output y must equal x. "
                    "This is an example requirement, not an authenticated owner approval.\n",
                    encoding="utf-8")
    archive = work / "archive"
    init_archive(archive, original, cli, spec, 4096, 2, 64)
    good_a = offer(archive, cli, identity, "handwritten-minimal")
    good_b = offer(archive, cli, larger, "handwritten-diversity")
    bad = offer(archive, cli, wrong, "mutation-control")
    if good_a["status"] != "elite" or good_b["status"] != "elite":
        raise ArchiveError(f"expected two different elite cells: {good_a} {good_b}")
    if bad["status"] != "rejected" or bad["checker_status"] != "counterexample":
        raise ArchiveError(f"incorrect mutation not rejected as counterexample: {bad}")
    if good_a["cell"] == good_b["cell"]:
        raise ArchiveError("two structurally different valid candidates must occupy separate cells")
    accepted = view(archive)
    if accepted["archive_coverage"] != 2:
        raise ArchiveError("archive did not retain exactly two elites")
    verified = replay_archive(archive, cli)
    if verified["elite_count"] != 2:
        raise ArchiveError("did not replay two elites")
    reason = work / "human-review-example.txt"
    reason.write_text("Prefer one Input node; it directly expresses the identity function. "
                      "This operator-supplied note is not an authenticated identity proof.\n",
                      encoding="utf-8")
    review_candidate(archive, good_a["sha256"], "preferred", reason, "demo-operator")
    chosen = export_elite(archive, cli, good_a["sha256"], work / "selected")
    result = {"status": "real-cli-smoke-passed", "authority": "none",
              "note": "proves candidate equality to original on 2 Boolean tuples; not a system-level proof",
              "archive_path": str(archive), "candidate_count": 3,
              "elites": [good_a, good_b], "negative_control": bad,
              "replay": verified, "manual_selection": chosen}
    (work / "smoke-result.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return result


def native_suite() -> tuple:
    """Fixed finite families in the real `loop encode` fixture vocabulary."""
    b = {"kind": "Bool"}
    narrow = {"kind": "Int", "min": "-2", "max": "2"}
    i64_max = "9223372036854775807"
    bit = {"kind": "Int", "min": "0", "max": "1"}
    return (
        {
            "name": "a-boolean-negation",
            "tuples": 2,
            "covers": "Bool identity: Not(Not(x)) == x; single Not(x) differs",
            "original": {"inputs": [b], "outputs": [b],
                         "nodes": [["Input", 0], ["Not", 0], ["Not", 1]], "roots": [2]},
            "equivalent": {"inputs": [b], "outputs": [b],
                           "nodes": [["Input", 0]], "roots": [0]},
            "counterexample": {"inputs": [b], "outputs": [b],
                               "nodes": [["Input", 0], ["Not", 0]], "roots": [1]},
        },
        {
            "name": "b-int-add-zero",
            "tuples": 5,
            "covers": "Int[-2,2] identity: Add(x,0) == x; Add(x,1) differs and "
                      "can exceed the declared output domain",
            "original": {"inputs": [narrow], "outputs": [narrow],
                         "nodes": [["Input", 0], ["Int", "0"], ["Add", 0, 1]], "roots": [2]},
            "equivalent": {"inputs": [narrow], "outputs": [narrow],
                           "nodes": [["Input", 0]], "roots": [0]},
            "counterexample": {"inputs": [narrow], "outputs": [narrow],
                               "nodes": [["Input", 0], ["Int", "1"], ["Add", 0, 1]], "roots": [2]},
        },
        {
            "name": "c-bool-repeated-roots",
            "tuples": 4,
            "covers": "ordered repeated roots [x,y,x] over two Bool inputs; "
                      "double-negation preserves them, [y,x,x] differs",
            "original": {"inputs": [b, b], "outputs": [b, b, b],
                         "nodes": [["Input", 0], ["Input", 1]], "roots": [0, 1, 0]},
            "equivalent": {"inputs": [b, b], "outputs": [b, b, b],
                           "nodes": [["Input", 0], ["Input", 1], ["Not", 0], ["Not", 2],
                                     ["Not", 1], ["Not", 4]],
                           "roots": [3, 5, 3]},
            "counterexample": {"inputs": [b, b], "outputs": [b, b, b],
                               "nodes": [["Input", 0], ["Input", 1]], "roots": [1, 0, 0]},
        },
        {
            "name": "d-unused-overflow-trap",
            "tuples": 2,
            "covers": "eager checked i64 trap on an unreachable node: "
                      "Add(i64::MAX,1) fails every tuple; removing it differs",
            "original": {"inputs": [bit], "outputs": [bit],
                         "nodes": [["Input", 0], ["Int", i64_max], ["Int", "1"], ["Add", 1, 2]],
                         "roots": [0]},
            "equivalent": {"inputs": [bit], "outputs": [bit],
                           "nodes": [["Input", 0], ["Int", i64_max], ["Int", "1"], ["Add", 1, 2],
                                     ["Int", "7"]],
                           "roots": [0]},
            "counterexample": {"inputs": [bit], "outputs": [bit],
                               "nodes": [["Input", 0]], "roots": [0]},
        },
    )


def replay_exported(cli: Path, archive: Path, exported: Path) -> dict:
    """Independent `transform replay` on the exported bundle, not on archive state."""
    receipt_bytes = bounded_file(exported / "receipt.json", 64 * 1024)
    candidate = exported / "candidate.zcve"
    code, report = run_cli(cli, ["transform", "replay",
                                 "--receipt", str(exported / "receipt.json"),
                                 "--original", str(archive / "original.zcve"),
                                 "--candidate", str(candidate),
                                 "--max-input-tuples", str(SUITE_TUPLE_CAP)], DEFAULT_TIMEOUT)
    detail = report.get("detail") if isinstance(report.get("detail"), dict) else {}
    if (code != 0 or report.get("schema") != REPORT_SCHEMA
            or report.get("status") != "replayed" or report.get("authority") != "none"
            or detail.get("receipt_sha256") != digest(receipt_bytes)):
        raise ArchiveError(f"exported receipt did not replay independently: {report}")
    return report


def output_domain_boundary(cli: Path, case: dict, work: Path) -> dict:
    """Force the output-domain witness; the broader case first differs at x=-2."""
    work.mkdir(parents=True, exist_ok=False)
    singleton = [{"kind": "Int", "min": "2", "max": "2"}]
    original = encode_program(cli, work, "original",
                              {**case["original"], "inputs": singleton})
    wrong = encode_program(cli, work, "counterexample",
                           {**case["counterexample"], "inputs": singleton})
    code, report = run_cli(cli, ["transform", "check", "--original", str(original),
                             "--candidate", str(wrong), "--max-input-tuples", "1"],
                        DEFAULT_TIMEOUT)
    detail = report.get("detail", {})
    if (code != 1 or report.get("schema") != REPORT_SCHEMA or report.get("status") != "counterexample"
            or report.get("authority") != "none" or detail.get("input") != ["2"]
            or report.get("original_sha256") != digest(bounded_file(original, 64 * 1024))
            or report.get("candidate_sha256") != digest(bounded_file(wrong, 64 * 1024))
            or detail.get("original", {}).get("ok") != ["2"]
            or detail.get("candidate", {}).get("error") != "OutputDomain"):
        raise ArchiveError(f"output-domain boundary did not produce its exact witness: {report}")
    (work / "counterexample-result.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return report


def run_native_case(cli: Path, case: dict, case_work: Path) -> dict:
    case_work.mkdir(parents=True, exist_ok=False)
    original = encode_program(cli, case_work, "original", case["original"])
    equivalent = encode_program(cli, case_work, "equivalent", case["equivalent"])
    wrong = encode_program(cli, case_work, "counterexample", case["counterexample"])
    spec = case_work / "operator-requirement.txt"
    spec.write_text(f"Native suite case {case['name']}: {case['covers']}. "
                    "Operator-supplied example requirement, not authenticated approval.\n",
                    encoding="utf-8")
    archive = case_work / "archive"
    init_archive(archive, original, cli, spec, SUITE_TUPLE_CAP, 2, 64)
    elite = offer(archive, cli, equivalent, "handwritten-equivalent")
    if elite.get("status") != "elite" or elite.get("authority") != "none":
        raise ArchiveError(f"{case['name']}: equivalent candidate not retained as elite: {elite}")
    rejected = offer(archive, cli, wrong, "mutation-control")
    if rejected.get("status") != "rejected" or rejected.get("checker_status") != "counterexample":
        raise ArchiveError(f"{case['name']}: incorrect candidate not a counterexample: {rejected}")
    accepted = view(archive)
    if accepted["archive_coverage"] != 1 or accepted["elites"][0]["sha256"] != elite["sha256"]:
        raise ArchiveError(f"{case['name']}: archive did not retain exactly the offered elite: {accepted}")
    verified = replay_archive(archive, cli)
    if verified["elite_count"] != 1 or verified["elites"][0]["sha256"] != elite["sha256"]:
        raise ArchiveError(f"{case['name']}: archive replay did not confirm the elite: {verified}")
    rationale = case_work / "demo-operator-review.txt"
    rationale.write_text("Prefer the offered equivalent for suite demonstration. Qualification is "
                         "limited to exact equivalence to the pinned original over its declared "
                         "finite domain; it is not authenticated approval, adoption, or evidence "
                         "of intended behavior.\n", encoding="utf-8")
    review_candidate(archive, elite["sha256"], "preferred", rationale, "demo-operator")
    exported = case_work / "selected"
    selection = export_elite(archive, cli, elite["sha256"], exported)
    if (selection.get("status") != "exported-for-manual-consideration"
            or selection.get("candidate_sha256") != elite["sha256"]
            or selection.get("original_sha256") != accepted["original_sha256"]
            or selection.get("human_review", {}).get("reviewer") != "demo-operator"):
        raise ArchiveError(f"{case['name']}: export selection binding mismatch: {selection}")
    receipt_bytes = bounded_file(exported / "receipt.json", 64 * 1024)
    receipt = json.loads(receipt_bytes)
    if (receipt.get("schema") != RECEIPT_SCHEMA or receipt.get("verdict") != "equivalent"
            or receipt.get("original", {}).get("sha256") != accepted["original_sha256"]
            or receipt.get("candidate", {}).get("sha256") != elite["sha256"]
            or receipt.get("inputs_checked") != case["tuples"]):
        raise ArchiveError(f"{case['name']}: exported receipt binding or coverage mismatch: {receipt}")
    replay = replay_exported(cli, archive, exported)
    boundary = (output_domain_boundary(cli, case, case_work / "output-domain-boundary")
                if case["name"] == "b-int-add-zero" else None)
    return {"case": case["name"], "covers": case["covers"],
            "input_tuples": case["tuples"],
            "output_domain_boundary": boundary,
            "original_sha256": accepted["original_sha256"],
            "equivalent": {"status": elite["status"], "sha256": elite["sha256"],
                           "cell": elite["cell"], "metrics": elite["metrics"],
                           "receipt_sha256": elite["receipt_sha256"]},
            "counterexample": {"status": rejected["status"],
                               "checker_status": rejected["checker_status"],
                               "sha256": rejected["sha256"]},
            "archive_replay": {"status": verified["status"],
                               "elite_count": verified["elite_count"]},
            "export": {"status": selection["status"],
                       "receipt_sha256": selection["receipt_sha256"],
                       "independent_replay": {"status": replay["status"],
                                              "receipt_sha256": replay["detail"]["receipt_sha256"]}}}


def run_suite(cli: Path, work: Path) -> dict:
    """Four-family native batch; each family gets a fresh pinned archive."""
    work.mkdir(parents=True, exist_ok=False)
    cases = [run_native_case(cli, case, work / case["name"]) for case in native_suite()]
    result = {"status": "real-cli-suite-passed", "authority": "none",
              "note": "bounded per-case equivalence to each pinned original over its complete "
                      "declared finite domain, checked and replayed by the real CLI only; "
                      "not optimality, not intended-behavior or release evidence",
              "archive_path": str(work), "case_count": len(cases), "cases": cases}
    (work / "native-suite-result.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return result


def main():
    args = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    args.add_argument("--cli", help="exact native zeno-fcis executable")
    args.add_argument("--work-dir", type=Path, help="new directory, retained for inspection")
    args.add_argument("--suite", action="store_true",
                      help="run the four-family native batch instead of the two-elite Boolean demo")
    ns = args.parse_args()
    try:
        cli = cli_binary(ns.cli)
    except ArchiveError as exc:
        print(json.dumps({"status": "not-run", "reason": str(exc),
                          "note": "missing native binary is not a pass"}))
        return 77
    label = "suite" if ns.suite else "smoke"
    runner = run_suite if ns.suite else run_demo
    try:
        if ns.work_dir is not None:
            result = runner(cli, ns.work_dir)
        else:
            with tempfile.TemporaryDirectory(prefix="zenofcis-elite-native-") as tmp:
                result = runner(cli, Path(tmp) / label)
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    except (ArchiveError, OSError, ValueError, KeyError) as exc:
        print(json.dumps({"status": f"real-cli-{label}-failed", "reason": str(exc)}))
        return 1


if __name__ == "__main__":
    sys.exit(main())
