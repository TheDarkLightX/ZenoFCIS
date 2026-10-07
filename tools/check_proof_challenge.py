#!/usr/bin/env python3
"""Run a frozen, trusted-fixture experiment with the official Lean Comparator.

This is not an API for untrusted submissions or a production sandbox.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "experiments/proof-challenge"
MANIFEST_SHA256 = "17b000231fa92bd0a99b2b8f47a0cf1c821da665b69315c6fb1cd9438b99dcc5"
SUCCESS = "Your solution is okay!"
CASES = {
    "valid": None,
    "alternate-proof": None,
    "weakened-statement": "Challenge and solution theorem statement do not match",
    "changed-definition": "Const does not match between challenge and target 'ApprovalContract.approve'",
    "changed-helper": "Const does not match between challenge and target 'ApprovalContract.permission'",
    "forbidden-axiom": "Illegal axiom detected: 'ApprovalContract.assumed_authority'",
    "unfinished-proof": "Illegal axiom detected: 'sorryAx'",
    "invalid-proof": "Tactic `rfl` failed",
}
TRUSTED = {"Challenge.lean", "config.json", "lakefile.toml", "lean-toolchain", "pins.json"}
EXPECTED_FILES = TRUSTED | {f"cases/{name}.lean" for name in CASES}


class ExperimentError(ValueError):
    pass


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def file_digest(path: Path) -> str:
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def load_fixtures(root: Path = FIXTURES) -> dict[str, bytes]:
    """Check the reviewed manifest, its complete membership and the frozen bytes."""
    manifest_path = root / "manifest.json"
    if manifest_path.is_symlink():
        raise ExperimentError("manifest must be a regular file")
    raw = manifest_path.read_bytes()
    if digest(raw) != MANIFEST_SHA256:
        raise ExperimentError("reviewed manifest changed")
    files = json.loads(raw)
    if set(files) != EXPECTED_FILES:
        raise ExperimentError("incomplete fixture manifest")
    if (root / "cases").is_symlink():
        raise ExperimentError("case directory must not be a symlink")
    actual_cases = {f"cases/{p.name}" for p in (root / "cases").iterdir()}
    if actual_cases != EXPECTED_FILES - TRUSTED:
        raise ExperimentError("unexpected or missing case")
    snapshot = {}
    for name, expected in files.items():
        path = root / name
        if path.is_symlink() or not path.is_file():
            raise ExperimentError(f"fixture must be a regular file: {name}")
        data = path.read_bytes()
        if digest(data) != expected:
            raise ExperimentError(f"frozen fixture changed: {name}")
        snapshot[name] = data
    return snapshot


def judge_result(returncode: int, output: str, reason: str | None) -> None:
    """An unrelated failure is never a passing negative control."""
    if reason is None:
        if returncode != 0 or output.splitlines().count(SUCCESS) != 1:
            raise ExperimentError("valid proof was not accepted exactly once")
    elif returncode == 0 or SUCCESS in output or reason not in output:
        raise ExperimentError(f"expected refusal was not observed: {reason}")


def run(command: list[str], cwd: Path, env: dict[str, str], log: Path,
        timeout: int = 60) -> tuple[int, str]:
    # Files keep subprocess output out of Python's RAM until the bounded run ends.
    with log.open("xb") as stream:
        with subprocess.Popen(command, cwd=cwd, env=env, stdout=stream,
                              stderr=subprocess.STDOUT, start_new_session=True) as process:
            try:
                status = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired as error:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                raise ExperimentError(f"timeout: {command[0]}") from error
    return status, log.read_text(errors="replace")


def tool_identity(comparator_root: Path, lean_root: Path, pins: dict) -> dict:
    identity = {"repositories": pins["repositories"], "binaries": {}}
    for relative, pin in pins["repositories"].items():
        path = comparator_root / relative
        head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=path, text=True).strip()
        dirty = subprocess.check_output(
            ["git", "status", "--porcelain", "--untracked-files=no"], cwd=path, text=True)
        if head != pin["revision"] or dirty:
            raise ExperimentError(f"tool source does not match clean pinned revision: {relative}")
    version = subprocess.check_output([str(lean_root / "bin/lean"), "--version"], text=True)
    if f"version {pins['lean_version']}," not in version or pins["lean_commit"] not in version:
        raise ExperimentError("Lean runtime version or commit does not match")
    identity["lean_version"] = version.strip()
    paths = {
        "lean": lean_root / "bin/lean",
        "lake": lean_root / "bin/lake",
        "comparator": comparator_root / ".lake/build/bin/comparator",
        "lean4export": comparator_root / ".lake/packages/lean4export/.lake/build/bin/lean4export",
        "development_shim": comparator_root / "scripts/fake-landrun.sh",
    }
    for name, path in paths.items():
        if not path.is_file() or not os.access(path, os.X_OK):
            raise ExperimentError(f"missing executable: {name}")
        identity["binaries"][name] = file_digest(path)
    return identity


def make_project(destination: Path, snapshot: dict[str, bytes], case: str) -> None:
    destination.mkdir()
    for name in ("Challenge.lean", "config.json", "lakefile.toml", "lean-toolchain"):
        (destination / name).write_bytes(snapshot[name])
    (destination / "Solution.lean").write_bytes(snapshot[f"cases/{case}.lean"])


def experiment(comparator_root: Path, lean_root: Path, output: Path) -> dict:
    snapshot = load_fixtures()
    pins = json.loads(snapshot["pins.json"])
    tools = tool_identity(comparator_root, lean_root, pins)
    # Refuse overwriting prior evidence. No candidate path or arbitrary code is accepted.
    output.mkdir(parents=True, exist_ok=False)
    env = {"HOME": os.environ["HOME"], "PATH": str(lean_root / "bin") + ":/usr/bin:/bin",
           "LANG": "C.UTF-8",
           "LEAN_NUM_THREADS": "1", "LEAN_ABORT_ON_PANIC": "1",
           "COMPARATOR_LANDRUN": str(comparator_root / "scripts/fake-landrun.sh"),
           "COMPARATOR_LEAN4EXPORT": str(
               comparator_root / ".lake/packages/lean4export/.lake/build/bin/lean4export")}
    record = {
        "format": "zeno-fcis/proof-challenge-experiment/1",
        "status": "running",
        "scope": "frozen reviewed fixtures only; not untrusted-submission containment",
        "manifest_sha256": MANIFEST_SHA256,
        "runner_sha256": file_digest(Path(__file__)),
        "fixtures": {name: digest(data) for name, data in snapshot.items()},
        "tools": tools,
        "additional_kernel": False,
        "cases": [],
    }
    try:
        with tempfile.TemporaryDirectory(prefix="projects-", dir=output) as directory:
            base = Path(directory)
            for case, reason in CASES.items():
                baseline, checked = base / f"{case}-native", base / f"{case}-judged"
                make_project(baseline, snapshot, case)
                make_project(checked, snapshot, case)
                native_log, judge_log = output / f"{case}.native.log", output / f"{case}.judge.log"
                native_exit, native_text = run([str(lean_root / "bin/lake"), "build", "Solution"],
                                              baseline, env, native_log)
                if case == "invalid-proof":
                    if native_exit == 0 or reason not in native_text:
                        raise ExperimentError("invalid-proof control failed for an unexpected reason")
                elif native_exit != 0:
                    raise ExperimentError(f"native baseline did not compile: {case}")
                command = [str(lean_root / "bin/lake"), "env",
                           str(comparator_root / ".lake/build/bin/comparator"), "config.json"]
                judge_exit, judge_text = run(command, checked, env, judge_log)
                judge_result(judge_exit, judge_text, reason)
                record["cases"].append({
                    "name": case, "native_exit": native_exit, "comparator_exit": judge_exit,
                    "expected": "accept" if reason is None else "refuse",
                    "reason": reason, "native_log_sha256": file_digest(native_log),
                    "judge_log_sha256": file_digest(judge_log),
                })
                print(f"proof-challenge: PASS {case}: {'accept' if reason is None else 'refuse'}", flush=True)
        if (load_fixtures() != snapshot or tool_identity(comparator_root, lean_root, pins) != tools
                or file_digest(Path(__file__)) != record["runner_sha256"]):
            raise ExperimentError("source or tool identity changed during the experiment")
        record["status"] = "experiment_pass"
    except Exception as error:
        record.update(status="experiment_failed", error=str(error))
        raise
    finally:
        (output / "receipt.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--comparator-root", required=True, type=Path)
    parser.add_argument("--lean-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path, help="new disk-backed evidence directory")
    args = parser.parse_args()
    try:
        record = experiment(args.comparator_root.resolve(), args.lean_root.resolve(), args.output.resolve())
        print(f"proof-challenge: complete PASS ({len(record['cases'])} cases)")
        return 0
    except (ExperimentError, OSError, subprocess.SubprocessError, ValueError) as error:
        print(f"proof-challenge: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
