"""Fixed-budget measurement of the bounded machine-code search over every registered problem.

`run` opens a fresh session for each problem in `test-data/machine-code/` and each kernel in
`test-data/machine-code/kernels/`, runs `session.search` with the same budgets, and records per problem:
- every numeric counter the search returns, whatever its name (for example `proposed`, `concrete_refutations`,
  `objective_pruned`, `solver_checks`, and pruning counters a later search adds), and the count of each checker
  verdict;
- the wall time of each repeat, the cost found, the status, the stop reason and which budget stopped it.

Counters must be identical across repeats; a difference is recorded as nondeterminism. `compare` reads two reports
and prints the before-and-after table of solver checks, candidates and time, and whether each found cost is
identical. Times are for this machine and load only. Nothing here claims optimality or a runtime speedup.

    python3 -B -m tools.machine_qualification.search_bench run --out REPORT.json [--problem NAME]... [--repeats N]
        [--no-prune]
    python3 -B -m tools.machine_qualification.search_bench compare BEFORE.json AFTER.json

`compare` marks each found cost identical, improved (none found before, or a lower rank now) or worse, and exits 1
when any is worse, a found cost is lost, or a problem of the first report is missing from the second. A problem only
in the second report is listed as new.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
from machine_code import session  # noqa: E402 - the checker package is imported from the tools directory

SCHEMA = "zeno-fcis/machine-search-benchmark/1"
BUDGETS = {"max_candidates": 100_000, "max_checks": 128, "deadline_ms": 60_000, "timeout_ms": 1000}
# The timer and the incumbent event number are numeric fields, not counters.
COUNT_EXCLUDED = {"elapsed_seconds", "incumbent"}


def problems() -> dict[str, dict]:
    """Every registered problem by its path under test-data/machine-code, kernels with their recorded cost."""
    found = {}
    data = ROOT / "test-data/machine-code"
    for path in sorted(data.glob("*.json")):
        found[str(path.relative_to(data))] = {"problem": json.loads(path.read_text(encoding="utf-8")),
                                              "recorded_cost": None}
    for path in sorted((data / "kernels").glob("*.json")):
        entry = json.loads(path.read_text(encoding="utf-8"))
        found[str(path.relative_to(data))] = {"problem": entry["problem"], "recorded_cost": entry["cost"]}
    return found


def space(problem: dict) -> int:
    """The number of instruction sequences the enumerator can propose, before any filter."""
    return sum(len(problem["grammar"]) ** length for length in range(problem["max_instructions"] + 1))


def counters(result: dict) -> dict:
    """Every numeric top-level result field, plus verdict counts; booleans and the timer are not counters."""
    numeric = {key: value for key, value in result.items()
               if type(value) in (int, float) and key not in COUNT_EXCLUDED}
    verdicts = {}
    for verdict in result.get("checker_verdicts", []):
        verdicts[verdict["status"]] = verdicts.get(verdict["status"], 0) + 1
    return {**numeric, **{f"verdicts_{status}": count for status, count in sorted(verdicts.items())}}


def budget_hit(result: dict, seconds: float, budgets: dict) -> str | None:
    if result.get("stop_reason") != "budget-exhausted":
        return None
    if result.get("proposed", 0) >= budgets["max_candidates"]:
        return "candidates"
    if result.get("solver_checks", 0) >= budgets["max_checks"]:
        return "checks"
    if seconds * 1000 >= budgets["deadline_ms"]:
        return "deadline"
    return "other"


def measure(problem: dict, budgets: dict, repeats: int, prune: bool | None = None) -> dict:
    """Search a fresh session `repeats` times with the same budgets; `prune` is passed only when given."""
    switch = {} if prune is None else {"prune": prune}
    runs = []
    with tempfile.TemporaryDirectory(prefix="zeno-search-bench-") as temporary:
        path = Path(temporary) / "problem.json"
        path.write_text(json.dumps(problem), encoding="utf-8")
        for repeat in range(repeats):
            directory = Path(temporary) / f"session-{repeat}"
            session.open_session(path, directory)
            started = time.monotonic()
            result = session.search(directory, budgets["max_candidates"], budgets["max_checks"],
                                    budgets["deadline_ms"], budgets["timeout_ms"], **switch)
            runs.append((time.monotonic() - started, result))
    first = runs[0][1]
    observed = [counters(result) for _, result in runs]
    seconds = [round(elapsed, 4) for elapsed, _ in runs]
    return {"status": first.get("status"), "stop_reason": first.get("stop_reason"),
            "budget_hit": budget_hit(first, seconds[0], budgets), "cost": first.get("cost"),
            "reason": first.get("reason"), "incumbent": first.get("incumbent"), "pruning": first.get("pruning"),
            "counters": observed[0],
            "deterministic": all(o == observed[0] and r.get("cost") == first.get("cost")
                                 and r.get("status") == first.get("status") for o, (_, r) in zip(observed, runs)),
            "wall_seconds": seconds, "wall_median_seconds": round(statistics.median(seconds), 4)}


def source_state() -> dict:
    def git(*arguments):
        completed = subprocess.run(("git", *arguments), cwd=ROOT, capture_output=True, text=True)
        return completed.stdout.strip() if completed.returncode == 0 else None
    status = git("status", "--porcelain", "--untracked-files=no")
    return {"commit": git("rev-parse", "HEAD"), "dirty": bool(status) if status is not None else None}


def run(selected: list[str] | None, budgets: dict, repeats: int, prune: bool | None = None) -> dict:
    import z3
    registered = problems()
    unknown = sorted(set(selected or ()) - set(registered))
    if unknown:
        raise SystemExit(f"unknown problems: {unknown}")
    report = {"schema": SCHEMA, "authority": "none", **source_state(), "python": platform.python_version(),
              "z3": z3.get_version_string(), "machine": platform.machine(), "processor": platform.processor(),
              "load_average": list(os.getloadavg()), "budgets": budgets, "repeats": repeats, "prune": prune,
              "scope": "Wall times hold for this machine and load only; no optimality or speedup claim.",
              "problems": {}}
    for name, item in registered.items():
        if selected and name not in selected:
            continue
        problem = item["problem"]
        row = {"schema": problem["schema"], "target": problem["target"], "grammar": len(problem["grammar"]),
               "max_instructions": problem["max_instructions"], "space": space(problem),
               "recorded_cost": item["recorded_cost"], **measure(problem, budgets, repeats, prune)}
        report["problems"][name] = row
        print(f"{name}: {row['status']} cost={row['cost']} stop={row['stop_reason']}/{row['budget_hit']} "
              f"checks={row['counters'].get('solver_checks')} proposed={row['counters'].get('proposed')} "
              f"median={row['wall_median_seconds']}s", file=sys.stderr, flush=True)
    return report


COLUMNS = ("solver_checks", "proposed", "concrete_refutations", "objective_pruned")


def verdict(old, new) -> str:
    """Compare two found costs by bytes, then instructions: the order of the default bytes objective."""
    if old == new:
        return "identical"
    if new is None:
        return "lost"
    if old is None:
        return "improved"
    rank = lambda cost: (cost["bytes"], cost["instructions"])
    return "improved" if rank(new) < rank(old) else "worse"


def compare(before: dict, after: dict) -> tuple[str, int]:
    """The before-and-after table; exit 1 when a found cost is worse or lost, or a problem disappears."""
    names = sorted(set(before["problems"]) | set(after["problems"]))
    extra = sorted({key for row in after["problems"].values() for key in row["counters"]} - set(COLUMNS))
    header = ["problem", *(f"{c} before/after" for c in COLUMNS), "median s before/after", "cost", *extra]
    lines = [" | ".join(header)]
    code = 0
    for name in names:
        old, new = before["problems"].get(name), after["problems"].get(name)
        if old is None:
            lines.append(f"{name} | new: {new['status']} cost {new['cost']} | "
                         f"{new['counters'].get('solver_checks')} checks | {new['wall_median_seconds']} s")
            continue
        if new is None:
            lines.append(f"{name} | missing from after")
            code = 1
            continue
        found = verdict(old["cost"], new["cost"])
        code = code if found in ("identical", "improved") else 1
        cells = [name, *(f"{old['counters'].get(c)}/{new['counters'].get(c)}" for c in COLUMNS),
                 f"{old['wall_median_seconds']}/{new['wall_median_seconds']}",
                 found if found == "identical" else f"{found} {old['cost']} -> {new['cost']}",
                 *(str(new["counters"].get(key, "-")) for key in extra)]
        lines.append(" | ".join(cells))
    common = [n for n in names if n in before["problems"] and n in after["problems"]]
    totals = [sum(report["problems"][n]["counters"].get("solver_checks", 0) for n in common)
              for report in (before, after)]
    lines.append(f"solver checks over the {len(common)} common problems: {totals[0]} before, {totals[1]} after; "
                 f"commits {before.get('commit')} -> {after.get('commit')}")
    return "\n".join(lines), code


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    measured = commands.add_parser("run")
    measured.add_argument("--out", type=Path, required=True)
    measured.add_argument("--problem", action="append")
    measured.add_argument("--repeats", type=int, default=3)
    measured.add_argument("--no-prune", action="store_true",
                          help="pass prune=False to a search that has the switch; the default leaves it unset")
    for key, value in BUDGETS.items():
        measured.add_argument("--" + key.replace("_", "-"), type=int, default=value)
    compared = commands.add_parser("compare")
    compared.add_argument("before", type=Path)
    compared.add_argument("after", type=Path)
    args = parser.parse_args(argv)
    if args.command == "compare":
        text, code = compare(json.loads(args.before.read_text(encoding="utf-8")),
                             json.loads(args.after.read_text(encoding="utf-8")))
        print(text)
        return code
    if args.out.exists():
        raise SystemExit(f"{args.out} exists; the report is written only to a new file")
    if not 1 <= args.repeats <= 10:
        raise SystemExit("--repeats must be 1 to 10")
    budgets = {key: getattr(args, key) for key in BUDGETS}
    report = run(args.problem, budgets, args.repeats, False if args.no_prune else None)
    args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    nondeterministic = [name for name, row in report["problems"].items() if not row["deterministic"]]
    print(f"wrote {args.out}: {len(report['problems'])} problems"
          + (f"; nondeterministic: {nondeterministic}" if nondeterministic else ""))
    return 1 if nondeterministic else 0


if __name__ == "__main__":
    raise SystemExit(main())
