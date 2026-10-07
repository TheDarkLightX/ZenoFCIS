#!/usr/bin/env python3
"""Measure `zeno-fcis optimize` on the recorded benchmark subjects.

Subjects: the withdrawal-queue kernel, retained controller and current
decision graph; the 32 seeds of cases.json (B01-B16, I01-I16); and the 100
originals of the published corpus (calibration only). For every subject the
script runs the given binary, records the best node count, the wall time and
whether any search limit was hit, and, when a candidate was reported, replays
its receipt with `transform replay` (the checker recomputes the receipt from
both programs). With `--profile`, every output (the candidate, or the original
when nothing improved) must also pass `loop open` under that profile.

    python3 measure_optimizer.py --binary target/release/zeno-fcis \
        --out results.json [--base base.json] [--strategy S.json] \
        [--profile functional-bool-v1] [--groups kernel,controller,...] [--jobs N]

With `--base`, each subject is compared with the base run: a win is fewer
nodes, a loss more. Times are host wall times of whole command invocations,
including the in-process checker; they are not part of any claim.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
ARTIFACTS = HERE / "withdrawal-queue" / "artifacts"
CORPUS = HERE / "published-corpus" / "originals"
GROUPS = ("kernel", "controller", "decision", "B", "I", "corpus")
WITHDRAWAL = {
    "kernel": "boolean-kernel-original.zcve",
    "controller": "retained-controller-original.zcve",
    "decision": "current-decision-scalars-original.zcve",
}


def run(command):
    started = time.perf_counter()
    completed = subprocess.run(command, capture_output=True, text=True)
    elapsed = time.perf_counter() - started
    try:
        report = json.loads(completed.stdout)
    except json.JSONDecodeError:
        report = {"status": "unparsable", "stderr": completed.stderr[-2000:]}
    return completed.returncode, report, elapsed


def seeds(binary: Path, work: Path, prefix: str):
    """Encodes the cases.json originals with `loop encode`."""
    fixtures = json.loads((HERE / "cases.json").read_text())
    subjects = []
    for case in fixtures["cases"]:
        if not case["id"].startswith(prefix):
            continue
        program = {
            "inputs": case["input_domains"],
            "outputs": case["output_domains"],
            "nodes": case["original"]["nodes"],
            "roots": case["original"]["roots"],
        }
        source = work / f"{case['id']}.json"
        source.write_text(json.dumps(program))
        target = work / f"{case['id']}.zcve"
        code, report, _ = run([str(binary), "loop", "encode", "--program", str(source), "--out", str(target)])
        if code != 0:
            raise SystemExit(f"cannot encode {case['id']}: {report}")
        subjects.append((case["id"], prefix, target))
    return subjects


def subjects(binary: Path, work: Path, groups):
    found = []
    for group in groups:
        if group in WITHDRAWAL:
            found.append((group, group, ARTIFACTS / WITHDRAWAL[group]))
        elif group in ("B", "I"):
            found.extend(seeds(binary, work, group))
        elif group == "corpus":
            found.extend((path.stem, "corpus", path) for path in sorted(CORPUS.glob("*.zcve")))
        else:
            raise SystemExit(f"unknown group {group}")
    return found


def measure(binary: Path, work: Path, subject, strategy, profile):
    name, group, program = subject
    candidate = work / f"{name}.candidate.zcve"
    receipt = work / f"{name}.receipt.json"
    command = [str(binary), "optimize", "--program", str(program),
               "--candidate-out", str(candidate), "--receipt", str(receipt)]
    if strategy:
        command += ["--strategy", str(strategy)]
    if profile:
        command += ["--profile", profile]
    code, report, elapsed = run(command)
    detail = report.get("detail", {})
    original = detail.get("original", {}).get("nodes")
    best = (detail.get("best") or {}).get("nodes")
    row = {
        "subject": name,
        "group": group,
        "status": report.get("status"),
        "exit": code,
        "original_nodes": original,
        "nodes": best if best is not None else original,
        "seconds": round(elapsed, 3),
        "limit_hit": detail.get("search", {}).get("any_limit_hit"),
        "cut_evaluations": sum(phase.get("cut_evaluations", 0)
                               for phase in detail.get("search", {}).get("phases", [])),
        "work": max((phase.get("work", 0) for phase in detail.get("search", {}).get("phases", [])),
                    default=0),
        "candidate_sha256": (detail.get("best") or {}).get("sha256"),
    }
    if report.get("status") == "improved":
        code, replay, _ = run([str(binary), "transform", "replay", "--receipt", str(receipt),
                               "--original", str(program), "--candidate", str(candidate)])
        row["replay"] = replay.get("status") if code == 0 else f"failed:{replay.get('status')}"
    if profile:
        output = candidate if report.get("status") == "improved" else program
        session = work / f"{name}.session"
        code, opened, _ = run([str(binary), "loop", "open", "--original", str(output),
                               "--session", str(session), "--profile", profile])
        row["profile_gate"] = "admitted" if code == 0 else json.dumps(opened.get("detail"))
    return row


def summarize(rows, base):
    by_group = {}
    for row in rows:
        entry = by_group.setdefault(row["group"], {"subjects": 0, "original": 0, "nodes": 0,
                                                   "seconds": 0.0, "limit_hits": 0,
                                                   "cut_evaluations": 0,
                                                   "wins": 0, "ties": 0, "losses": 0})
        entry["subjects"] += 1
        entry["cut_evaluations"] += row["cut_evaluations"]
        entry["original"] += row["original_nodes"] or 0
        entry["nodes"] += row["nodes"] or 0
        entry["seconds"] = round(entry["seconds"] + row["seconds"], 3)
        entry["limit_hits"] += 1 if row["limit_hit"] else 0
        reference = base.get(row["subject"])
        if reference is not None:
            if row["nodes"] < reference:
                entry["wins"] += 1
            elif row["nodes"] > reference:
                entry["losses"] += 1
            else:
                entry["ties"] += 1
    return by_group


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--base", type=Path)
    parser.add_argument("--strategy", type=Path)
    parser.add_argument("--profile")
    parser.add_argument("--groups", default=",".join(GROUPS))
    parser.add_argument("--jobs", type=int, default=1)
    arguments = parser.parse_args()
    groups = [group for group in arguments.groups.split(",") if group]
    base = {}
    if arguments.base:
        base = {row["subject"]: row["nodes"] for row in json.loads(arguments.base.read_text())["rows"]}
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-measure-") as directory:
        work = Path(directory)
        found = subjects(arguments.binary, work, groups)
        with ThreadPoolExecutor(max_workers=max(1, arguments.jobs)) as pool:
            rows = list(pool.map(lambda subject: measure(arguments.binary, work, subject,
                                                         arguments.strategy, arguments.profile), found))
    problems = [row for row in rows
                if row["status"] not in ("improved", "no-checked-improvement")
                or row.get("replay", "replayed") != "replayed"
                or row.get("profile_gate", "admitted") != "admitted"]
    result = {
        "schema": "zeno-fcis/optimizer-measurement/1",
        "binary": str(arguments.binary),
        "strategy": str(arguments.strategy) if arguments.strategy else None,
        "profile": arguments.profile,
        "jobs": arguments.jobs,
        "groups": summarize(rows, base),
        "problems": [row["subject"] for row in problems],
        "rows": rows,
    }
    arguments.out.write_text(json.dumps(result, indent=1) + "\n")
    print(f"{'group':<12}{'n':>4}{'original':>10}{'nodes':>8}{'seconds':>10}{'caps':>6}"
          f"{'cuts':>10}{'W/T/L':>12}")
    for group, entry in result["groups"].items():
        wtl = f"{entry['wins']}/{entry['ties']}/{entry['losses']}" if base else "-"
        print(f"{group:<12}{entry['subjects']:>4}{entry['original']:>10}{entry['nodes']:>8}"
              f"{entry['seconds']:>10.2f}{entry['limit_hits']:>6}{entry['cut_evaluations']:>10}{wtl:>12}")
    for row in problems:
        print(f"PROBLEM {row['subject']}: {row}", file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
