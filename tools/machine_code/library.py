"""Checked kernel library: exact machine bytes kept with the problem they were checked against.

An entry is a reusable record, not authority. It stores the problem, the exact bytes, the cost the checker
reported and how the bytes were found. It stores no verdict or receipt: `check` rechecks every entry with the
current independent checker for its profile and rebuilds the ELF object and assembly source from the bytes.
A receipt would bind the checker identity (source, Python and Z3 hashes), so it could not survive a later
checker edit or another host; a fresh recheck can.

`add` imports only an emitted artifact that replays here, and never overwrites an entry. The library check
establishes the registered ISA-model equivalence for each entry on this checker; it is not native execution,
timing, a kernel proof or a deployment grant.
"""

from __future__ import annotations

from pathlib import Path
import re

from . import session
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


def check(library: Path, timeout_ms: int = 1000) -> dict:
    """Recheck every entry; the library passes only if every entry is equivalent at its recorded cost."""
    if library.is_symlink() or not library.is_dir():
        raise ValueError("library must be an existing non-symlink directory")
    paths = sorted(library.iterdir())
    if len(paths) > MAX_ENTRIES:
        raise ValueError("library holds more than 256 entries")
    rows = []
    for path in paths:
        row = {"file": path.name}
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
        rows.append(row)
    passed = bool(rows) and all(row["status"] == "equivalent" for row in rows)
    return {"schema": REPORT, "status": "checked" if passed else "failed", "kernels": rows,
            "identity": session.identity(), "assurance": session.ASSURANCE, "authority": "none",
            "scope": ("Each entry was rechecked now by the independent checker for its profile, at its recorded "
                      "cost, and its ELF object and assembly were rebuilt from the bytes. Not native execution, "
                      "timing, global optimality, a kernel proof or a deployment grant.")}
