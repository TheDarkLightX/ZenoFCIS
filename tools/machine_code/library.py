"""Checked kernel library: exact machine bytes kept with the problem they were checked against.

An entry is a reusable record, not authority. It stores the problem, the exact bytes, the cost the checker
reported and how the bytes were found. It stores no verdict or receipt: `check` rechecks every entry with the
current independent checker for its profile and rebuilds the ELF object and assembly source from the bytes.
A receipt would bind the checker identity (source, Python and Z3 hashes), so it could not survive a later
checker edit or another host; a fresh recheck can.

`add` imports only an emitted artifact that replays here, and never overwrites an entry. The library check
establishes the registered ISA-model equivalence for each entry on this checker; it is not native execution,
timing, a kernel proof or a deployment grant.

`check --native` also runs each x86 entry's rebuilt object on this processor (see `native.py`) and reports RV64I
entries as not run. `bench` times each rechecked x86 entry against GCC's code for the same expression (see
`bench.py`) and writes one table, scoped to this machine and load.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import tempfile

from . import bench as timing, native as execution, session
from .model import canonical, digest, read_json
from .targets import assembly_source, elf_object

SCHEMA = "zeno-fcis/machine-kernel/1"
REPORT = "zeno-fcis/machine-kernel-library-report/1"
FIELDS = {"schema", "name", "summary", "problem", "code_hex", "cost", "route"}
ROUTES = ("proposal", "search")
NAME = re.compile(r"[a-z0-9][a-z0-9-]{0,62}")
SUMMARY = re.compile(r"[ -~]{1,200}")
MAX_ENTRIES = 256


def _validate(value: dict, name: str | None = None) -> tuple[dict, bytes]:
    """Return the validated problem and code of one entry, or raise ValueError."""
    if not isinstance(value, dict) or set(value) != FIELDS or value["schema"] != SCHEMA:
        raise ValueError("kernel entry fields differ from the closed version-1 schema")
    if not isinstance(value["name"], str) or not NAME.fullmatch(value["name"]):
        raise ValueError("kernel name must be 1..63 lowercase letters, digits or hyphens")
    if name is not None and value["name"] != name:
        raise ValueError("kernel name differs from its file name")
    if not isinstance(value["summary"], str) or not SUMMARY.fullmatch(value["summary"]):
        raise ValueError("kernel summary must be 1..200 printable ASCII characters")
    if value["route"] not in ROUTES:
        raise ValueError("kernel route must be proposal or search")
    session._cost(value["cost"])
    if not isinstance(value["code_hex"], str) or not re.fullmatch(r"(?:[0-9a-f]{2}){1,256}", value["code_hex"]):
        raise ValueError("kernel code_hex must be 1..256 lowercase hex bytes")
    problem = session._validated(value["problem"])
    if canonical(problem) != canonical(value["problem"]):
        raise ValueError("kernel problem is not in validated form")
    return problem, bytes.fromhex(value["code_hex"])


def entry(artifact: Path, name: str, summary: str, route: str, timeout_ms: int = 1000) -> dict:
    """A library entry from an emitted artifact that replays on this checker now."""
    replayed = session.replay_artifact(artifact, timeout_ms)
    if replayed["status"] != "replayed":
        raise ValueError("artifact did not replay; nothing is added")
    value = {"schema": SCHEMA, "name": name, "summary": summary, "problem": read_json(artifact / "problem.json"),
             "code_hex": session._code(artifact / "kernel.bin").hex(), "cost": replayed["verification"]["cost"],
             "route": route}
    _validate(value)
    return value


def add(artifact: Path, library: Path, name: str, summary: str, route: str, timeout_ms: int = 1000) -> dict:
    value = entry(artifact, name, summary, route, timeout_ms)
    if library.is_symlink() or not library.is_dir():
        raise ValueError("library must be an existing non-symlink directory")
    path = library / (name + ".json")
    session._json(path, value)  # exclusive create: an existing entry is never replaced
    return {"status": "added", "path": str(path), "code_sha256": digest(bytes.fromhex(value["code_hex"])),
            "cost": value["cost"], "authority": "none"}


def _recheck(library: Path, timeout_ms: int) -> list:
    """(report row, problem, code) for every file; problem and code are None for a refused file."""
    if library.is_symlink() or not library.is_dir():
        raise ValueError("library must be an existing non-symlink directory")
    paths = sorted(library.iterdir())
    if len(paths) > MAX_ENTRIES:
        raise ValueError("library holds more than 256 entries")
    rows = []
    for path in paths:
        row, problem, code = {"file": path.name}, None, None
        try:
            if path.suffix != ".json" or path.is_symlink() or not path.is_file():
                raise ValueError("library holds a non-entry file")
            value = read_json(path)
            problem, code = _validate(value, path.stem)
            verdict = session._checker(problem).check(problem, code, timeout_ms=timeout_ms)
            elf_object(problem["target"], code)
            assembly_source(code)
            row.update(name=value["name"], target=problem["target"], code_sha256=digest(code),
                       problem_sha256=digest(canonical(problem)), cost=value["cost"], route=value["route"])
            if verdict["status"] != "equivalent":
                row.update(status=verdict["status"], reason=verdict.get("reason", ""))
            elif verdict["cost"] != value["cost"]:
                row.update(status="cost-differs", checked_cost=verdict["cost"])
            else:
                row.update(status="equivalent")
        except (ValueError, TypeError, KeyError, OSError) as error:
            row.update(status="refused", reason=str(error) or type(error).__name__)
            problem = code = None
        rows.append((row, problem, code))
    return rows


def check(library: Path, timeout_ms: int = 1000, native: bool = False) -> dict:
    """Recheck every entry; the library passes only if every entry is equivalent at its recorded cost.

    With `native`, every x86 entry must also agree natively, and RV64I entries are reported as not run.
    """
    checked = _recheck(library, timeout_ms)
    rows = [row for row, _, _ in checked]
    passed = bool(rows) and all(row["status"] == "equivalent" for row in rows)
    scope = ("Each entry was rechecked now by the independent checker for its profile, at its recorded "
             "cost, and its ELF object and assembly were rebuilt from the bytes. Not native execution, "
             "timing, global optimality, a kernel proof or a deployment grant.")
    report = {"schema": REPORT, "status": "checked" if passed else "failed", "kernels": rows,
              "identity": session.identity(), "assurance": session.ASSURANCE, "authority": "none", "scope": scope}
    if not native:
        return report
    unsupported = execution.unsupported_host()
    with tempfile.TemporaryDirectory(prefix="zeno-library-native-") as work:
        for index, (row, problem, code) in enumerate(checked):
            if problem is None:
                row["native"] = {"status": "not-run", "reason": "entry refused"}
            elif problem["target"] not in execution.X86_TARGETS:
                row["native"] = dict(execution.NOT_RV64I)
            elif unsupported:
                row["native"] = {"status": "not-run", "reason": unsupported}
            else:
                row["native"] = execution.execute(problem, code, Path(work) / str(index), row["name"])
    states = [row["native"]["status"] for row in rows]
    report["native_summary"] = {
        "agreed": states.count("agreed"), "mismatch": states.count("mismatch"), "error": states.count("error"),
        "not_run": {row["file"]: row["native"]["reason"] for row in rows if row["native"]["status"] == "not-run"},
        "vectors": sum(row["native"].get("vectors", 0) for row in rows)}
    if any(state in ("mismatch", "error") for state in states):
        report["status"] = "failed"
    elif passed and unsupported and any(p is not None and p["target"] in execution.X86_TARGETS for _, p, _ in checked):
        report["status"] = "inconclusive"
    report["scope"] = scope.replace("Not native execution, timing", "Not timing") + " " + execution.SCOPE
    return report


def bench(library: Path, output: Path, iterations: int = 1_000_000, repeats: int = 21, timeout_ms: int = 1000) -> dict:
    """Recheck every entry, then time each equivalent x86 entry against GCC's code for the same expression."""
    refused = timing.unsupported_host()
    if refused:
        return refused
    timing.budget(iterations, repeats)
    gcc, refused = timing.compiler()
    if refused:
        return refused
    checked = _recheck(library, timeout_ms)
    output.mkdir(mode=0o700)
    start, compiler = os.getloadavg(), None
    rows = []
    for row, problem, code in checked:
        line = {"kernel": row.get("name", row["file"]), "target": row.get("target"),
                "bytes": len(code) if code else None}
        if row["status"] != "equivalent":
            line.update(status="not-timed", reason="did not recheck: " + row["status"])
        elif problem["target"] not in timing.X86_TARGETS:
            line.update(status="not-run", reason=execution.NOT_RV64I["reason"])
        else:
            work = output / row["name"]
            work.mkdir(mode=0o700)
            (work / "kernel.o").write_bytes(elf_object(problem["target"], code))
            measured = timing.measure(problem, code, work / "kernel.o", work, iterations, repeats, gcc)
            line.update(status="measured", baseline_c=measured["baseline_c"],
                        ticks_per_call=measured["ticks_per_call"], ratio=measured["ratio_kernel_over_baseline"])
            compiler = measured["environment"]["compiler"]
        rows.append(line)
    environment = {"cpu": timing._cpu(), "load_average_start": start, "load_average_end": os.getloadavg(),
                   "compiler": compiler}
    passed = bool(rows) and all(line["status"] in ("measured", "not-run") for line in rows)
    report = {"schema": "zeno-fcis/machine-kernel-library-bench/1", "status": "measured" if passed else "failed",
              "kernels": rows, "iterations": iterations, "repeats": repeats, "environment": environment,
              "authority": "none", "scope": "This machine and load only. " + timing.SCOPE}
    report["table"] = _table(report)
    (output / "bench.json").write_text(json.dumps(report, indent=1, sort_keys=True) + "\n")
    (output / "bench.md").write_text(report["table"])
    return report


def _table(report: dict) -> str:
    environment = report["environment"]
    load = lambda values: ", ".join(f"{value:.2f}" for value in values)
    lines = [f"Paired timing on {environment['cpu']}, load average {load(environment['load_average_start'])} at the "
             f"start and {load(environment['load_average_end'])} at the end, {environment['compiler'] or 'gcc'}, "
             f"{report['iterations']} iterations and {report['repeats']} repeats. This machine and load only; not a "
             "portable speedup claim.", "",
             "| Kernel | Target | Bytes | Latency ticks, kernel / GCC | Ratio | Throughput ticks, kernel / GCC | Ratio |",
             "| --- | --- | --- | --- | --- | --- | --- |"]
    for row in report["kernels"]:
        if row["status"] != "measured":
            lines.append(f"| {row['kernel']} | {row['target'] or ''} | {row['bytes'] or ''} | "
                         f"{row['status']}: {row['reason']} | | | |")
            continue
        ticks, ratio = row["ticks_per_call"], row["ratio"]
        lines.append(f"| {row['kernel']} | {row['target']} | {row['bytes']} | "
                     f"{ticks['kernel_latency']:.2f} / {ticks['baseline_latency']:.2f} | {ratio['latency']:.2f} | "
                     f"{ticks['kernel_throughput']:.2f} / {ticks['baseline_throughput']:.2f} | {ratio['throughput']:.2f} |")
    return "\n".join(lines) + "\n"
