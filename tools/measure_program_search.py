#!/usr/bin/env python3
"""Measure the untrusted program search on every declared core instance.

For each of the 77 core-component instances the decision program is made
afresh in a work directory outside the source tree (`instantiate_core.py`,
`zeno-fcis generate contract`, `zeno-fcis contract export-program`), then:

- (a) `zeno-fcis optimize --profile checked-i64-v1`;
- (b) the search alone (`search_program search`);
- (b+) the search seeded with (a)'s program;
- (c) `zeno-fcis optimize --profile checked-i64-v1 --with-candidate` with each
  distinct search output (the existing supplied-candidate path).

Every reported program that differs from the original goes through
`transform check --receipt` and `transform replay`, and the independent
reference (`program_search_reference.py`) compares its complete table with
the original's; a refusal or disagreement is counted as a proposer defect and
kept in its row, never dropped. As a control, copies of every search output
with one planted single-tuple fault, for each output position at the first,
the last and one seeded random accepted tuple, must be refused by
`transform check` at exactly that tuple and detected there by the reference.
The search is also run a second time to compare its bytes.

The report is measurement data with authority none. It sets no size
threshold: the exit status is 0 only when every instance was measured, no
search stopped, every reported program was accepted, replayed and confirmed by
the reference, every (b) and (b+) program has all its controls, and every
control ran and was refused at its tuple by both the checker and the
reference. `--scratch` runs in a temporary directory for acceptance checks.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

import instantiate_core as core  # noqa: E402
import program_search_reference as reference  # noqa: E402

TIMEOUT = 3600
COLUMNS = ("original", "a", "b", "b_seeded", "c")
LABELS = {"original": "original", "a": "(a) optimize", "b": "(b) search",
          "b_seeded": "(b+) search seeded with (a)", "c": "(c) optimize + search candidates"}


class MeasureError(RuntimeError):
    """A step failed in a way that leaves the row incomplete."""


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def node_count(body: bytes) -> int:
    """Instruction count of a canonical program, read from its value encoding.

    Tags: 0x01/0x02 Boolean, 0x03/0x04 128-bit integers, 0x06 text, 0x08
    tuple. A program is a 4-tuple (profile, schema, nodes, roots).
    """
    at = 0

    def take(count: int) -> bytes:
        nonlocal at
        if at + count > len(body):
            raise MeasureError("truncated program")
        chunk = body[at:at + count]
        at += count
        return chunk

    def value() -> int | None:
        tag = take(1)[0]
        if tag in (0x01, 0x02):
            return None
        if tag in (0x03, 0x04):
            take(16)
            return None
        if tag == 0x06:
            take(int.from_bytes(take(4), "big"))
            return None
        if tag == 0x08:
            count = int.from_bytes(take(4), "big")
            for _ in range(count):
                value()
            return count
        raise MeasureError(f"unknown tag {tag:#04x}")

    if take(1) != b"\x08" or int.from_bytes(take(4), "big") != 4:
        raise MeasureError("not a program tuple")
    value()
    value()
    nodes = value()
    value()
    if at != len(body) or nodes is None:
        raise MeasureError("trailing or malformed program bytes")
    return nodes


def metrics(path: Path) -> dict:
    body = path.read_bytes()
    return {"nodes": node_count(body), "bytes": len(body), "sha256": sha256(body)}


def run(command: list, log: Path, timeout: int = TIMEOUT) -> tuple[int, dict | None, float]:
    """Runs one command; returns its exit status, its last JSON line and seconds."""
    started = time.monotonic()
    try:
        completed = subprocess.run([str(part) for part in command], capture_output=True,
                                   timeout=timeout, check=False)
    except subprocess.TimeoutExpired as error:
        raise MeasureError(f"timeout after {timeout}s: {command[:3]}") from error
    seconds = time.monotonic() - started
    with log.open("ab") as stream:
        stream.write(b"$ " + " ".join(map(str, command)).encode() + b"\n")
        stream.write(completed.stdout)
        stream.write(completed.stderr)
        stream.write(f"exit {completed.returncode}\n".encode())
    report = None
    for line in reversed(completed.stdout.decode("utf-8", "replace").strip().splitlines()):
        try:
            report = json.loads(line)
            break
        except ValueError:
            continue
    if report is None:
        try:
            report = json.loads(completed.stdout)
        except ValueError:
            report = None
    return completed.returncode, report, seconds


def judge(cli: Path, original: Path, candidate: Path, receipt: Path, log: Path, tuples: int) -> dict:
    """`transform check --receipt`, receipt binding checks (coverage against the
    instance's own tuple count), then `transform replay`."""
    code, report, seconds = run([cli, "transform", "check", "--original", original,
                                 "--candidate", candidate, "--receipt", receipt], log)
    verdict = {"check_exit": code, "check_status": (report or {}).get("status"),
               "check_seconds": round(seconds, 3)}
    if code != 0 or verdict["check_status"] != "equivalent" or not receipt.is_file():
        verdict["accepted"] = False
        verdict["detail"] = (report or {}).get("detail")
        return verdict
    body = receipt.read_bytes()
    recorded = json.loads(body)
    problems = []
    if (recorded.get("schema"), recorded.get("verdict"), recorded.get("authority")) != \
            ("zeno-fcis/transform-receipt/1", "equivalent", "none"):
        problems.append("receipt-shape")
    for side, path in (("original", original), ("candidate", candidate)):
        expected = metrics(path)
        entry = recorded.get(side, {})
        if (entry.get("sha256"), entry.get("bytes"), entry.get("nodes")) != \
                (expected["sha256"], expected["bytes"], expected["nodes"]):
            problems.append(f"receipt-binding-{side}")
    if not recorded.get("inputs_checked") == recorded.get("domain", {}).get("size") == tuples:
        problems.append("receipt-coverage")
    code, replay, seconds = run([cli, "transform", "replay", "--receipt", receipt,
                                 "--original", original, "--candidate", candidate], log)
    verdict.update({"replay_exit": code, "replay_status": (replay or {}).get("status"),
                    "replay_seconds": round(seconds, 3), "receipt_sha256": sha256(body),
                    "inputs_checked": recorded.get("inputs_checked")})
    if code != 0 or verdict["replay_status"] != "replayed" or \
            (replay or {}).get("detail", {}).get("receipt_sha256") != sha256(body):
        problems.append("replay")
    verdict["problems"] = problems
    verdict["accepted"] = not problems
    return verdict


def control(cli: Path, search: Path, original: Path, output: Path, planted: Path, log: Path,
            ordinal: int, position: int) -> dict:
    """Plants one fault at `ordinal` in output `position` of `output`; the checker
    must refuse it at that tuple and the reference must find it there."""
    code, plant, _ = run([search, "plant", "--program", output, "--out", planted,
                          "--ordinal", str(ordinal), "--output", str(position)], log)
    if code != 0 or (plant or {}).get("status") != "planted" or plant.get("ordinal") != ordinal:
        return {"ran": False, "refused_at_tuple": False, "reference_at_tuple": False,
                "ordinal": ordinal, "output": position, "plant": plant}
    code, report, _ = run([cli, "transform", "check", "--original", original,
                           "--candidate", planted], log)
    detail = (report or {}).get("detail") or {}
    refused = (code == 1 and (report or {}).get("status") == "counterexample"
               and detail.get("ordinal") == ordinal and detail.get("input") == plant["input"])
    found = reference.compare(reference.decode(original.read_bytes()), reference.decode(planted.read_bytes()))
    return {"ran": True, "refused_at_tuple": refused, "ordinal": ordinal, "output": position,
            "input": plant["input"], "check_exit": code, "check_status": (report or {}).get("status"),
            "reported_ordinal": detail.get("ordinal"),
            "reference_at_tuple": found.get("status") == "different" and found.get("ordinal") == ordinal
            and found.get("input") == plant["input"]}


def control_points(program: Path, name: str) -> tuple[list[int], int, dict]:
    """The first, last and one seeded random tuple the program accepts."""
    decoded = reference.decode(program.read_bytes())
    accepted = [ordinal for ordinal, (kind, _) in enumerate(reference.table(decoded)) if kind == "ok"]
    if not accepted:
        return [], len(decoded.outputs), {"accepted": 0}
    chooser = random.Random(int.from_bytes(hashlib.sha256(name.encode()).digest()[:8], "big"))
    points = sorted({accepted[0], accepted[-1], chooser.choice(accepted)})
    return points, len(decoded.outputs), {"accepted": len(accepted), "first": accepted[0],
                                           "last": accepted[-1], "tuples": reference.size(decoded)}


def search_step(search: Path, original: Path, out: Path, seeds: list[Path], log: Path,
                work_units: int | None) -> tuple[dict, float]:
    command = [search, "search", "--program", original, "--out", out]
    for seed in seeds:
        command += ["--seed", seed]
    if work_units is not None:
        command += ["--work-units", str(work_units)]
    code, report, seconds = run(command, log)
    if report is None or code not in (0, 2):
        raise MeasureError(f"search failed with exit {code}")
    return report, seconds


def instance_rows(families: list[str]) -> list[tuple[str, dict]]:
    rows = []
    for family in core.FAMILIES:
        if families and family not in families:
            continue
        for parameters in core.definition(family)["instances"]:
            rows.append((family, parameters))
    return rows


def measure_instance(family: str, parameters: dict, cli: Path, search: Path, work: Path,
                     work_units: int | None) -> dict:
    name = f"{family}-{core.instance_id(parameters)}"
    folder = work / name
    folder.mkdir()
    log = folder / "commands.log"
    contract = folder / "contract"
    manifest = core.instantiate(family, parameters, contract, include_proof=False)
    row = {"instance": name, "family": family, "parameters": parameters,
           "input_tuples": manifest["input_tuples"], "defects": [], "controls": {}, "seconds": {}}
    code, _, _ = run([cli, "generate", "contract", contract, "--format", "json"], log)
    if code != 0:
        raise MeasureError(f"{name}: generate contract exited {code}")
    original = folder / "original.zcve"
    code, exported, _ = run([cli, "contract", "export-program", contract, "--out", original,
                             "--format", "json"], log)
    if code != 0 or not original.is_file():
        raise MeasureError(f"{name}: export-program exited {code}")
    programs = {"original": original}
    row["original"] = metrics(original)
    if (exported or {}).get("program", {}).get("nodes") not in (None, row["original"]["nodes"]):
        raise MeasureError(f"{name}: export-program reported another node count")

    a_path = folder / "a.zcve"
    code, report, seconds = run([cli, "optimize", "--program", original, "--profile", "checked-i64-v1",
                                 "--candidate-out", a_path], log)
    row["seconds"]["a"] = round(seconds, 3)
    row["a_status"] = (report or {}).get("status")
    if code == 0 and a_path.is_file():
        programs["a"] = a_path
    elif row["a_status"] != "no-checked-improvement":
        raise MeasureError(f"{name}: optimize exited {code} with {row['a_status']}")

    b_path = folder / "b.zcve"
    report, seconds = search_step(search, original, b_path, [], log, work_units)
    row["seconds"]["b"] = round(seconds, 3)
    row["b_report"] = report
    if report.get("status") == "improved":
        programs["b"] = b_path
    again = folder / "b-again.zcve"
    repeat, _ = search_step(search, original, again, [], log, work_units)
    row["search_deterministic"] = (repeat.get("status") == report.get("status") and
                                   (not b_path.is_file() or again.read_bytes() == b_path.read_bytes()) and
                                   repeat.get("provenance") == report.get("provenance"))

    if "a" in programs:
        bs_path = folder / "b-seeded.zcve"
        report, seconds = search_step(search, original, bs_path, [programs["a"]], log, work_units)
        row["seconds"]["b_seeded"] = round(seconds, 3)
        row["b_seeded_report"] = report
        if report.get("status") == "improved":
            programs["b_seeded"] = bs_path

    supplied = []
    for key in ("b", "b_seeded"):
        if key in programs and all(programs[key].read_bytes() != p.read_bytes() for p in supplied):
            supplied.append(programs[key])
    if supplied:
        c_path = folder / "c.zcve"
        command = [cli, "optimize", "--program", original, "--profile", "checked-i64-v1",
                   "--candidate-out", c_path]
        for path in supplied:
            command += ["--with-candidate", path]
        code, report, seconds = run(command, log)
        row["seconds"]["c"] = round(seconds, 3)
        row["c_status"] = (report or {}).get("status")
        if code == 0 and c_path.is_file():
            programs["c"] = c_path
        elif row["c_status"] != "no-checked-improvement":
            raise MeasureError(f"{name}: optimize --with-candidate exited {code} with {row['c_status']}")

    for key in COLUMNS[1:]:
        if key not in programs:
            row[key] = None
            continue
        row[key] = metrics(programs[key])
        if programs[key].read_bytes() == original.read_bytes():
            row[key]["check"] = {"accepted": True, "identical_to_original": True}
            continue
        verdict = judge(cli, original, programs[key], folder / f"{key}.receipt.json", log,
                        row["input_tuples"])
        agreed = reference.compare(reference.decode(original.read_bytes()),
                                   reference.decode(programs[key].read_bytes()))
        verdict["reference"] = agreed.get("status")
        if agreed.get("status") != "equal":
            verdict["accepted"] = False
            verdict.setdefault("problems", []).append("reference-disagrees")
        row[key]["check"] = verdict
        if not verdict["accepted"]:
            row["defects"].append({"column": key, "verdict": verdict})

    for key in ("b", "b_seeded"):
        if key not in programs:
            continue
        points, outputs, where = control_points(programs[key], f"{name}/{key}")
        row["control_points"] = row.get("control_points", {})
        row["control_points"][key] = {"ordinals": points, "outputs": outputs,
                                      "expected_controls": len(points) * outputs, **where}
        for position in range(outputs):
            for ordinal in points:
                label = f"{key}-o{position}-t{ordinal}"
                row["controls"][label] = control(cli, search, original, programs[key],
                                                 folder / f"{label}.planted.zcve", log, ordinal, position)
    row["best"] = best_of(row)
    return row


def effective(row: dict, key: str) -> dict:
    """A column's program, or the original when the column reported none."""
    value = row.get(key)
    if value is None:
        return row["original"]
    if not value.get("check", {}).get("accepted", False):
        return row["original"]
    return value


def missing_controls(row: dict) -> int:
    """Controls a (b) or (b+) program lacks: every output position needs a
    planted fault at its first, last and seeded random accepted tuple."""
    missing = 0
    for key in ("b", "b_seeded"):
        if row.get(key) is None or row[key].get("check", {}).get("identical_to_original"):
            continue
        points = row.get("control_points", {}).get(key)
        if not points or not points["ordinals"] or points["outputs"] < 1:
            missing += 1
            continue
        present = sum(1 for label in row["controls"] if label.startswith(f"{key}-o"))
        missing += max(points["expected_controls"] - present, 0)
    return missing


def best_of(row: dict) -> dict:
    best_key = "original"
    for key in COLUMNS[1:]:
        candidate = effective(row, key)
        current = effective(row, best_key)
        if (candidate["nodes"], candidate["bytes"]) < (current["nodes"], current["bytes"]):
            best_key = key
    chosen = effective(row, best_key)
    return {"column": best_key, "nodes": chosen["nodes"], "bytes": chosen["bytes"]}


def summarize(rows: list[dict]) -> dict:
    totals = {key: {"nodes": sum(effective(r, key)["nodes"] for r in rows),
                    "bytes": sum(effective(r, key)["bytes"] for r in rows)} for key in COLUMNS}
    totals["best"] = {"nodes": sum(r["best"]["nodes"] for r in rows),
                      "bytes": sum(r["best"]["bytes"] for r in rows)}

    def key_of(row, column):
        value = effective(row, column)
        return value["nodes"], value["bytes"]

    controls = [c for r in rows for c in r["controls"].values()]
    families = {}
    for row in rows:
        family = families.setdefault(row["family"], {"instances": 0, **{k: 0 for k in COLUMNS}, "best": 0,
                                                     "b_smaller_than_a": 0})
        family["instances"] += 1
        for key in COLUMNS:
            family[key] += effective(row, key)["nodes"]
        family["best"] += row["best"]["nodes"]
        family["b_smaller_than_a"] += key_of(row, "b") < key_of(row, "a")
    return {
        "instances": len(rows),
        "input_tuples": sum(r["input_tuples"] for r in rows),
        "totals": totals,
        "b_smaller_than_a": sum(key_of(r, "b") < key_of(r, "a") for r in rows),
        "b_larger_than_a": sum(key_of(r, "b") > key_of(r, "a") for r in rows),
        "best_smaller_than_a": sum((r["best"]["nodes"], r["best"]["bytes"]) < key_of(r, "a") for r in rows),
        "c_smaller_than_a": sum(key_of(r, "c") < key_of(r, "a") for r in rows),
        "reported_programs": sum(1 for r in rows for k in COLUMNS[1:]
                                 if r.get(k) and not r[k]["check"].get("identical_to_original")),
        "accepted_and_replayed": sum(1 for r in rows for k in COLUMNS[1:]
                                     if r.get(k) and not r[k]["check"].get("identical_to_original")
                                     and r[k]["check"].get("accepted")),
        "proposer_defects": sum(len(r["defects"]) for r in rows),
        "search_stops": sum(1 for r in rows for k in ("b_report", "b_seeded_report")
                            if (r.get(k) or {}).get("status") == "stopped"),
        "search_nondeterministic": sum(not r["search_deterministic"] for r in rows),
        "controls_run": sum(c["ran"] for c in controls),
        "controls_refused_at_tuple": sum(c["refused_at_tuple"] for c in controls),
        "controls_found_by_reference": sum(c.get("reference_at_tuple", False) for c in controls),
        "controls_not_run": sum(not c["ran"] for c in controls),
        "controls_missing": sum(missing_controls(r) for r in rows),
        "families": families,
        "search_winners": _tally(((r.get("b_report") or {}).get("provenance") or {}).get("winner")
                                 for r in rows),
        "diagram_runs": _tally("skipped: " + str(diagram["skipped"]) if diagram.get("skipped") else "ran"
                               for r in rows
                               for diagram in [((r.get("b_report") or {}).get("provenance") or {})
                                               .get("diagram") or {"skipped": "not reported"}]),
    }


def _tally(items) -> dict:
    counts: dict = {}
    for item in items:
        counts[str(item)] = counts.get(str(item), 0) + 1
    return dict(sorted(counts.items()))


def cell(row: dict, key: str) -> str:
    value = row.get(key)
    search = row.get(f"{key}_report") or {}
    if search.get("status") == "stopped":
        return f"STOP ({search.get('stop', {}).get('reason')})"
    if value is None:
        return "-"
    mark = "" if value["check"].get("accepted") else " DEFECT"
    return f"{value['nodes']} / {value['bytes']}{mark}"


def markdown(report: dict) -> str:
    summary = report["summary"]
    lines = [
        "# Program search measurement",
        "",
        f"Instances: {summary['instances']} ({summary['input_tuples']} input tuples). "
        f"Cells are nodes / bytes; `-` means the step reported no program "
        "(the original counts in the totals). Every reported program was judged by "
        "`transform check --receipt` and `transform replay`.",
        "",
        "| Instance | Tuples | Original | (a) optimize | (b) search | (b+) seeded | (c) optimize + search | Best |",
        "|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for row in report["rows"]:
        lines.append(
            f"| {row['instance']} | {row['input_tuples']} | {row['original']['nodes']} / {row['original']['bytes']} "
            f"| {cell(row, 'a')} | {cell(row, 'b')} | {cell(row, 'b_seeded')} | {cell(row, 'c')} "
            f"| {row['best']['nodes']} / {row['best']['bytes']} ({row['best']['column']}) |")
    totals = summary["totals"]
    lines.append(
        f"| **Total** | {summary['input_tuples']} | **{totals['original']['nodes']}** / {totals['original']['bytes']} "
        f"| **{totals['a']['nodes']}** / {totals['a']['bytes']} | **{totals['b']['nodes']}** / {totals['b']['bytes']} "
        f"| **{totals['b_seeded']['nodes']}** / {totals['b_seeded']['bytes']} "
        f"| **{totals['c']['nodes']}** / {totals['c']['bytes']} | **{totals['best']['nodes']}** / {totals['best']['bytes']} |")
    for failure in report.get("failures", []):
        lines.append(f"| {failure['family']} {failure['parameters']} | FAILED: {failure['error']} | | | | | | |")
    lines += ["", "## Per family (nodes)", "",
              "| Family | Instances | Original | (a) | (b) | (b+) | (c) | Best | (b) < (a) |",
              "|---|---:|---:|---:|---:|---:|---:|---:|---:|"]
    for family, values in summary["families"].items():
        lines.append(f"| {family} | {values['instances']} | {values['original']} | {values['a']} | {values['b']} "
                     f"| {values['b_seeded']} | {values['c']} | {values['best']} | {values['b_smaller_than_a']} |")
    lines += ["", "## Checks and controls", ""]
    for key in ("reported_programs", "accepted_and_replayed", "proposer_defects", "search_stops",
                "search_nondeterministic", "controls_run", "controls_refused_at_tuple",
                "controls_found_by_reference", "controls_not_run", "controls_missing",
                "b_smaller_than_a", "b_larger_than_a", "c_smaller_than_a", "best_smaller_than_a"):
        lines.append(f"- {key.replace('_', ' ')}: {summary[key]}")
    lines.append(f"- proposer of each (b) program: {summary['search_winners']}")
    lines.append(f"- decision-diagram proposer in (b): {summary['diagram_runs']}")
    lines += ["", f"CLI SHA-256 `{report['cli_sha256']}`; search binary SHA-256 `{report['search_sha256']}`.",
              "Authority: none. This is a measurement, not a pass threshold.", ""]
    return "\n".join(lines)


def outside(path: Path) -> bool:
    resolved = path.resolve()
    return resolved != ROOT and ROOT not in resolved.parents


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--search", type=Path, required=True, help="the search_program example binary")
    place = parser.add_mutually_exclusive_group(required=True)
    place.add_argument("--work", type=Path, help="new directory outside the source tree")
    place.add_argument("--scratch", action="store_true",
                       help="work and report in a new temporary directory, removed afterwards")
    parser.add_argument("--out", type=Path, help="directory for the JSON and Markdown report")
    parser.add_argument("--family", action="append", default=[], choices=core.FAMILIES)
    parser.add_argument("--work-units", type=int)
    args = parser.parse_args()
    if args.scratch:
        if args.out is not None:
            parser.error("--scratch writes its report to the temporary directory")
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-program-search-") as directory:
            return measure(args, Path(directory) / "work", Path(directory) / "report")
    if args.out is None:
        parser.error("--work needs --out")
    return measure(args, args.work, args.out)


def measure(args: argparse.Namespace, work: Path, out: Path) -> int:
    if not (outside(work) and outside(out)):
        raise SystemExit("--work and --out must be outside the source tree")
    work.mkdir(parents=True, exist_ok=False)
    out.mkdir(parents=True, exist_ok=True)
    cli, search = args.cli.resolve(), args.search.resolve()
    report = {"schema": "zeno-fcis/program-search-measurement/1", "authority": "none",
              "cli_sha256": sha256(cli.read_bytes()), "search_sha256": sha256(search.read_bytes()),
              "work_units": args.work_units, "rows": [], "failures": []}
    started = time.monotonic()
    for family, parameters in instance_rows(args.family):
        try:
            row = measure_instance(family, parameters, cli, search, work.resolve(), args.work_units)
            report["rows"].append(row)
            print(f"{row['instance']}: original {row['original']['nodes']}, a {effective(row, 'a')['nodes']}, "
                  f"b {effective(row, 'b')['nodes']}, b+ {effective(row, 'b_seeded')['nodes']}, "
                  f"c {effective(row, 'c')['nodes']}, defects {len(row['defects'])}", flush=True)
        except (MeasureError, OSError, ValueError, KeyError) as error:
            report["failures"].append({"family": family, "parameters": parameters, "error": str(error)})
            print(f"{family} {parameters}: FAILED {error}", flush=True)
    report["seconds"] = round(time.monotonic() - started, 1)
    report["summary"] = summarize(report["rows"])
    body = json.dumps(report, indent=1, sort_keys=True) + "\n"
    (out / "measurement.json").write_text(body)
    (out / "measurement.md").write_text(markdown(report))
    summary = report["summary"]
    complete = (not report["failures"] and summary["proposer_defects"] == 0
                and summary["accepted_and_replayed"] == summary["reported_programs"]
                and summary["controls_refused_at_tuple"] == summary["controls_run"]
                and summary["controls_found_by_reference"] == summary["controls_run"]
                and summary["controls_not_run"] == 0
                and summary["controls_missing"] == 0
                and summary["controls_run"] > 0
                and summary["search_nondeterministic"] == 0
                and summary["search_stops"] == 0)
    print(json.dumps({"status": "complete" if complete else "incomplete", "summary": {
        k: v for k, v in summary.items() if k != "families"}}, sort_keys=True))
    return 0 if complete else 1


if __name__ == "__main__":
    raise SystemExit(main())
