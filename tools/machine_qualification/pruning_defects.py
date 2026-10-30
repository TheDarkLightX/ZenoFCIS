"""Planted defects in the search pruning, each judged by the pruning exactness control.

Each defect is one named edit to `tools/machine_code/footprint.py`, applied to an isolated specimen: a copy of the
checker package and of this qualification package. The exactness control (`pruning_exactness run`) then runs inside
the specimen, under `python3 -B` and `python3 -O -B`, against the specimen's own footprint module. Outcomes:

- `detected`: the control completed and reported at least one removed sequence without a proved witness;
- `survived`: the control completed and every case was exact;
- `inconclusive`: the control crashed or timed out;
- `invalid-specimen`: the edit does not apply exactly once, the copy does not compile, or the control imported
  another checker package.

Crashes and timeouts never count as detections; no checker identity or receipt is involved.

    python3 -B -m tools.machine_qualification.pruning_defects run [--defect NAME]... [--out REPORT.json]

Exit 0 when every defect is detected in both modes and the unmodified specimen is exact, 1 when a defect survives or
the unmodified specimen is not exact, 2 otherwise.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import py_compile
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
MODES = (("-B",), ("-O", "-B"))
TIMEOUT = 600
MARKER = "ZENO_EXACTNESS "


@dataclass(frozen=True)
class Defect:
    name: str
    anchor: str
    replacement: str
    why: str


DEFECTS = (
    Defect("flag-writes-omitted",
           'return _effects(z, {**entry, **flags}, {**final, **final_flags}, declared)',
           'return _effects(z, {**entry, **flags}, {**final, **flags}, declared)',
           "Every x86 flag keeps its entry value in the footprint, so no instruction writes a flag."),
    Defect("conditional-write-as-overwrite",
           'used = {variable[4:] for variable in _variables(z, value)}',
           'used = {variable[4:] for variable in _variables(z, value)} - ({name} if z.is_app_of(value, z.Z3_OP_ITE)'
           ' else set())',
           "A conditional move neither reads its destination nor keeps it: it is treated as a full overwrite."),
    Defect("write-write-conflict-ignored",
           'independent = (not first.writes & (second.reads | second.writes)',
           'independent = (not first.writes & second.reads',
           "Two instructions that write the same location are treated as independent and reordered."),
    Defect("write-after-read-ignored",
           '\n                               and not second.writes & first.reads)',
           ')',
           "An instruction that overwrites what its neighbour reads is treated as independent of it."),
    Defect("second-result-unobserved",
           'return frozenset(verify2.RESULTS[problem["target"]][:len(problem["outputs"])])',
           'return frozenset(verify2.RESULTS[problem["target"]][:1])',
           "On profile 2 only the first result register is observed, so writes to the second look dead."),
    Defect("profile1-result-unobserved",
           'return frozenset({"rax" if problem["target"] == verify.X86 else "a0"})',
           'return frozenset()',
           "On profile 1 the result register is treated as unobserved at the end."),
)


def build(directory: Path, defect: Defect | None) -> dict:
    tools = directory / "tools"
    for package in ("machine_code", "machine_qualification"):
        shutil.copytree(ROOT / "tools" / package, tools / package, ignore=shutil.ignore_patterns("__pycache__"))
    if defect is None:
        return {}
    target = tools / "machine_code/footprint.py"
    source = target.read_text(encoding="utf-8")
    count = source.count(defect.anchor)
    if count != 1:
        return {"invalid": f"anchor occurs {count} times"}
    target.write_text(source.replace(defect.anchor, defect.replacement), encoding="utf-8")
    try:
        py_compile.compile(str(target), cfile=str(directory / "check.pyc"), doraise=True)
    except py_compile.PyCompileError as error:
        return {"invalid": "edited footprint.py does not compile: " + str(error)[:200]}
    return {}


def judge(directory: Path, mode: tuple, timeout: int = TIMEOUT) -> tuple[str, str, float]:
    """Run the exactness control inside one specimen and classify the outcome."""
    environment = {k: v for k, v in os.environ.items() if k not in ("PYTHONPATH", "PYTHONSAFEPATH", "PYTHONOPTIMIZE")}
    probe = ("import json, runpy, sys; import machine_code; "
             "print('ZENO_PACKAGE ' + machine_code.__file__, flush=True); "
             "sys.argv = ['pruning_exactness', 'run']; "
             "runpy.run_module('machine_qualification.pruning_exactness', run_name='__main__')")
    started = time.monotonic()
    try:
        completed = subprocess.run([sys.executable, *mode, "-c", probe], cwd=directory / "tools", env=environment,
                                   capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return "inconclusive", f"timeout after {timeout} s", round(time.monotonic() - started, 1)
    seconds = round(time.monotonic() - started, 1)
    lines = completed.stdout.splitlines()
    package = next((line.split(" ", 1)[1] for line in lines if line.startswith("ZENO_PACKAGE ")), "")
    if Path(package).resolve().parent != (directory / "tools/machine_code").resolve():
        return "invalid-specimen", "the control imported another checker package: " + package, seconds
    result = next((json.loads(line[len(MARKER):]) for line in lines if line.startswith(MARKER)), None)
    if result is None:
        return "inconclusive", "crash: " + completed.stderr.strip()[-300:], seconds
    failures = {case: count for case, count in result["failures"].items() if count}
    if result["exact"]:
        return "survived", "every case exact", seconds
    return "detected", "unwitnessed removals: " + ", ".join(f"{c} {n}" for c, n in failures.items()), seconds


def campaign(defects=DEFECTS, modes=MODES, timeout: int = TIMEOUT) -> dict:
    report = {"schema": "zeno-fcis/search-pruning-defects/1", "authority": "none", "positive": {}, "defects": []}
    with tempfile.TemporaryDirectory(prefix="zeno-pruning-defects-") as temporary:
        base = Path(temporary) / "base"
        build(base, None)
        for mode in modes:
            outcome, evidence, seconds = judge(base, mode, timeout)
            report["positive"][" ".join(mode)] = {"exact": outcome == "survived", "evidence": evidence,
                                                  "seconds": seconds}
        for index, defect in enumerate(defects):
            specimen = Path(temporary) / f"defect-{index}"
            binding = build(specimen, defect)
            row = {"defect": defect.name, "why": defect.why, "outcomes": {}}
            for mode in modes:
                if "invalid" in binding:
                    outcome, evidence, seconds = "invalid-specimen", binding["invalid"], 0.0
                else:
                    outcome, evidence, seconds = judge(specimen, mode, timeout)
                row["outcomes"][" ".join(mode)] = {"outcome": outcome, "evidence": evidence, "seconds": seconds}
            report["defects"].append(row)
            shutil.rmtree(specimen)
    outcomes = [o["outcome"] for row in report["defects"] for o in row["outcomes"].values()]
    if not all(p["exact"] for p in report["positive"].values()) or "survived" in outcomes:
        report["status"] = "failed"
    elif any(o != "detected" for o in outcomes):
        report["status"] = "inconclusive"
    else:
        report["status"] = "all-detected"
    return report


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    measured = commands.add_parser("run")
    measured.add_argument("--defect", action="append", choices=[d.name for d in DEFECTS])
    measured.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    report = campaign(tuple(d for d in DEFECTS if not args.defect or d.name in args.defect))
    if args.out:
        args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    modes = list(report["positive"])
    print("defect | " + " | ".join(modes) + " | evidence")
    for row in report["defects"]:
        print(" | ".join([row["defect"], *(row["outcomes"][m]["outcome"] for m in modes),
                          row["outcomes"][modes[0]]["evidence"]]))
    print("unmodified pruning exact: " + ", ".join(f"{m} {p['exact']}" for m, p in report["positive"].items()))
    print("status: " + report["status"])
    return {"all-detected": 0, "failed": 1}.get(report["status"], 2)


if __name__ == "__main__":
    raise SystemExit(main())
