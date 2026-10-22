#!/usr/bin/env python3
"""LLM-generated deferred CLI/reference controls and synthetic owner families.

This driver compares bounded finite cases with an independent scalar reference;
every candidate's acceptance result is decided by the Rust CLI.
"""
from __future__ import annotations
import argparse
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import version_space_reference as O


def cases():
    def make(text, n=2, output=None):
        return {"schema": "zeno-fcis/loop-version-space/1", "inputs": [{"name": "x", "domain": {"kind": "Int", "min": 0, "max": n-1}}],
                "outputs": [output or {"kind": "Bool"}], "sketch": [text], "requirements": []}
    out = [("duplicate-boolean", {"schema": "zeno-fcis/loop-version-space/1", "inputs": [{"name": "x", "domain": {"kind": "Bool"}}],
            "outputs": [{"kind": "Bool"}], "sketch": ["?{x | x && true | false | true}"], "requirements": []})]
    out += [(f"threshold-{n}", make(f"x < ?c[0..{n}]", n)) for n in (4, 5, 7, 9)]
    out += [(f"piecewise-{n}", make(f"x < ?c[0..{n}] ? ?c[0..2] : ?c[0..2]", n, {"kind": "Int", "min": 0, "max": 2})) for n in (3, 5)]
    out.append(("two-output-routing", {"schema": "zeno-fcis/loop-version-space/1", "inputs": [{"name": n, "domain": {"kind": "Bool"}} for n in ("x", "y")],
                "outputs": [{"kind": "Bool"}]*2, "sketch": ["x ? ?c : ?c", "y ? ?c : ?c"], "requirements": []}))
    return out


def execute(cli, spec, directory, flags=()):
    file = directory / "specification.json"
    file.write_text(json.dumps(spec, sort_keys=True))
    # Finite-data commands stay on POSIX even when the environment selects systemd.
    result = subprocess.run([str(cli), "--containment-backend", "auto", "loop", "question", "--specification",
                             str(file), *flags], text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=35)
    return result.returncode, json.loads(result.stdout)


def controls(cli, directory):
    spec = cases()[0][1]
    failures = []
    rows = []
    for name, initial in cases():
        owners = O.universe(initial)
        for owner_n, hidden in enumerate(owners):
            s = copy.deepcopy(initial)
            rounds = 0
            while True:
                code, report = execute(cli, s, directory)
                ok = code == 0 and report.get("status") in ("question", "determined", "no-completion") and O.compare(s, report)
                rows.append({"case": name, "owner": owner_n, "round": rounds, "matches_reference": ok, "status": report.get("status")})
                if not ok:
                    failures.append(rows[-1])
                    break
                detail = report["detail"]
                if report["status"] == "determined":
                    if O.program_table(detail["candidate"]["program"]) != hidden:
                        failures.append({"case": name, "failure": "hidden-owner-mismatch"})
                    break
                if report["status"] != "question" or rounds >= len(hidden):
                    failures.append({"case": name, "failure": "owner-not-recovered"})
                    break
                q = O.report_question(detail["question"])
                s["requirements"].append({"id": f"answer_{rounds}", "input": q["input"], "output": list(hidden[q["ordinal"]])})
                rounds += 1
    # Minimum conflicts, empty grammar, eager traps and intent outside the
    # grammar are semantic controls, not vacuous all-refusal successes.
    simple = {"schema": "zeno-fcis/loop-version-space/1", "inputs": [{"name": "x", "domain": {"kind": "Bool"}}],
              "outputs": [{"kind": "Int", "min": 0, "max": 2}], "sketch": ["?c[0..1]"], "requirements": []}
    extra = []
    for label, requirements in [
        ("minimum-tie", [{"id": i, "input": [x], "output": [y]} for i, x, y in (("d", 0, 1), ("c", 0, 0), ("b", 1, 1), ("a", 1, 0))]),
        ("law-example-conflict", [{"id": "zero", "law": "out0 == 0"}, {"id": "one", "input": [0], "output": [1]}]),
        ("outside-grammar", [{"id": "outside", "input": [0], "output": [2]}]),
        ("law-overflow", [{"id": "overflow", "law": "out0 < 170141183460469231731687303715884105727 + x"}]),
    ]:
        s = copy.deepcopy(simple)
        # The overflow control needs an integer input to keep strict kinds.
        if label == "law-overflow":
            s["inputs"][0]["domain"] = {"kind": "Int", "min": 0, "max": 1}
        s["requirements"] = requirements
        extra.append((label, s))
    empty = copy.deepcopy(simple); empty["sketch"] = ["?{3 | 4}"]
    extra.append(("empty-grammar", empty))
    trap = copy.deepcopy(simple); trap["sketch"] = ["?{0 | true ? 0 : 170141183460469231731687303715884105727 + 1}"]
    extra.append(("eager-dead-branch-trap", trap))
    for label, s in extra:
        code, report = execute(cli, s, directory)
        ok = code == 0 and report.get("status") in ("determined", "no-completion") and O.compare(s, report)
        row = {"case": label, "passed": ok, "status": report.get("status")}
        rows.append(row)
        if not ok: failures.append(row)
    for flag in ("--max-work", "--max-domain", "--max-meanings", "--max-cells"):
        code, r = execute(cli, spec, directory, (flag, "0"))
        detail = r.get("detail", {})
        ok = code == 2 and r.get("status") == "inconclusive" and all(detail.get(k) is None for k in ("count", "question", "candidate")) and detail.get("attempts_spent") == 0
        row = {"case": "budget", "flag": flag, "passed": ok}
        rows.append(row)
        if not ok:
            failures.append(row)
    return {"authority": "none", "rows": rows, "failures": failures, "applicable_rows": len(rows)}


def main(arguments=None):
    p = argparse.ArgumentParser()
    p.add_argument("--cli", type=Path)
    p.add_argument("--out", type=Path, help="new evidence file; omit to retain the full report in stdout")
    p.add_argument("--self-test", action="store_true")
    p.add_argument("--stub-comparison", action="store_true")
    a = p.parse_args(arguments)
    if a.self_test:
        O.self_test(a.stub_comparison)
        print(json.dumps({"self_test": "passed", "authority": "none"}))
        return 0
    if not a.cli:
        p.error("supply --cli [--out FILE], or --self-test")
    if a.out is not None and a.out.exists():
        p.error("refuse existing evidence output")
    with tempfile.TemporaryDirectory(prefix="zeno-version-space-") as temp:
        report = controls(a.cli, Path(temp))
    evidence = json.dumps(report, sort_keys=True, indent=2) + "\n"
    if a.out is not None:
        a.out.write_text(evidence)
    else:
        print(evidence, end="")
    print(json.dumps({"rows": len(report["rows"]), "failures": len(report["failures"]), "authority": "none"}))
    return 1 if report["failures"] or not report["applicable_rows"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
