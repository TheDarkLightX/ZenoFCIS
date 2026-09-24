#!/usr/bin/env python3
"""Run the local gates for one committed source revision and record the results.

The record names the exact commit and tree it tested, the tool versions, each
command with its exit code and test counts, and every ignored test with the
step that runs it, if any. Tracked files must match the commit when the run
starts. The commit, tree, and tracked files are compared with the start after
every gate and again after everything is collected, immediately before the
record is written; any difference stops the run and nothing is written. A
change made and reverted between two checks is not detected, so run the
recorder in a checkout that nothing else modifies. Pinned formal-tool checks
run only when their executables are supplied through the same environment
variables that the formal-tools workflow uses.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SUMMARY = re.compile(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored")
IGNORED = re.compile(r'#\[ignore(?: = "([^"]*)")?\]\s*(?:#\[[^\]]*\]\s*)*fn ([a-z0-9_]+)')
CARGO = ["cargo", "+1.97.1"]


def git(*args: str) -> str:
    return subprocess.run(["git", *args], cwd=ROOT, check=True, capture_output=True,
                          text=True).stdout.strip()


def source_state() -> tuple[str, str, str]:
    """The commit, its tree, and any tracked-file changes in the checkout."""
    return (git("rev-parse", "HEAD"), git("rev-parse", "HEAD^{tree}"),
            git("status", "--porcelain", "--untracked-files=no"))


def version(command: list[str]) -> str | None:
    try:
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired):
        return None
    lines = (result.stdout or result.stderr).strip().splitlines()
    return lines[0] if result.returncode == 0 and lines else None


def run(name: str, command: list[str], environment: dict[str, str] | None = None) -> dict:
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                            env={**os.environ, **(environment or {})})
    output = result.stdout + result.stderr
    counts = [0, 0, 0]
    for match in SUMMARY.finditer(output):
        for index in range(3):
            counts[index] += int(match.group(index + 1))
    print(f"{name}: exit {result.returncode}", file=sys.stderr)
    return {
        "name": name,
        "command": command,
        "exit_code": result.returncode,
        "tests": dict(zip(("passed", "failed", "ignored"), counts)),
    }


def pinned_steps() -> list[tuple[str, list[str], dict[str, str]]]:
    steps = []
    cvc5, z3 = os.environ.get("ZENO_FCIS_CVC5"), os.environ.get("ZENO_FCIS_Z3")
    lean, lean_root = os.environ.get("ZENO_FCIS_LEAN"), os.environ.get("ZENO_FCIS_LEAN_ROOT")
    formal = [*CARGO, "test", "-p", "zeno-fcis-formal-tools", "--locked"]
    if cvc5 and z3:
        steps.append(("pinned-smt-translation", [*formal, "tests::pinned_smt_translation_differential_check",
                                                 "--", "--ignored", "--exact"], {}))
        steps.append(("pinned-inductive-steps", [*formal, "tests::pinned_inductive_steps_agree_with_exhaustive_replay",
                                                 "--", "--ignored", "--exact"], {}))
    if cvc5:
        for test in ("system::tests::pinned_system_smt_agrees_with_exhaustive_check",
                     "system::tests::pinned_solver_route_agrees_with_exhaustive_route_on_small_programs"):
            name = test.rsplit("::", 1)[1].removeprefix("pinned_")
            steps.append((f"pinned-{name}", [*formal, test, "--", "--ignored", "--exact"], {}))
        steps.append(("pinned-counter-system-smt", [*CARGO, "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis",
                                                    "--locked", "synth::tests::pinned_counter_system_smt_agrees_with_exhaustive_check",
                                                    "--", "--ignored", "--exact"], {}))
    if lean and lean_root:
        steps.append(("pinned-lean-kernel", [*formal, "tests::pinned_lean_translation_kernel_checks",
                                             "--", "--ignored", "--exact"], {}))
        steps.append(("pinned-lean-corpus", [*formal, "translation_tests::operator_corpus_checks_with_existing_lean",
                                             "--", "--ignored", "--exact"], {"ZENO_FCIS_TRANSLATION_LEAN": lean}))
        steps.append(("pinned-lean-cli", [*CARGO, "test", "-p", "zeno-fcis-cli", "--locked", "--test",
                                          "cli_adopter_flow", "pinned_lean_cli_prove_is_process_level",
                                          "--", "--ignored", "--exact"], {}))
    return steps


def lean_trust_anchor() -> dict | None:
    lean_root = os.environ.get("ZENO_FCIS_LEAN_ROOT")
    if not lean_root:
        return None
    expected = (ROOT / "release/lean-4.30.0-tree.sha256").read_text().strip()
    output = subprocess.run([*CARGO, "run", "--quiet", "-p", "zeno-fcis-cli", "--locked", "--",
                             "backend", "inventory-lean", lean_root], cwd=ROOT, capture_output=True,
                            text=True).stdout
    actual = next((line.split()[2] for line in output.splitlines() if line.startswith("lean tree_sha256 ")), None)
    return {"expected_tree_sha256": expected, "actual_tree_sha256": actual, "matches": actual == expected}


def ignored_tests(steps: list[dict]) -> list[dict]:
    """Lists every ignored test and what runs it: a recorded step, an acceptance
    scenario that runs its test target with --ignored, or the parent test that
    spawns it as a helper process. `None` means nothing in this record runs it."""
    commands = [" ".join(step["command"]) for step in steps]
    atdd_lines = (ROOT / "tools/atdd.py").read_text(encoding="utf-8").splitlines()
    atdd_ran = any(step["name"] == "atdd" for step in steps)
    inventory = []
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        for match in IGNORED.finditer(path.read_text(encoding="utf-8")):
            reason, name = match.group(1) or "", match.group(2)
            run_by = next((step["name"] for step, command in zip(steps, commands) if name in command), None)
            if run_by is None and atdd_ran and path.parent.name == "tests" and any(
                    f'"--test", "{path.stem}"' in line and '"--ignored"' in line for line in atdd_lines):
                run_by = "atdd"
            if run_by is None and reason.startswith("spawned by"):
                run_by = "helper process of another test"
            inventory.append({"test": name, "file": str(path.relative_to(ROOT)), "reason": reason,
                              "run_by": run_by})
    return inventory


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True, help="evidence file to write")
    parser.add_argument("--skip-atdd", action="store_true", help="omit the full acceptance run")
    args = parser.parse_args()
    revision, tree, changes = source_state()
    if changes:
        print("record_gate_evidence: tracked files differ from the commit; commit first", file=sys.stderr)
        return 2
    gates = [] if args.skip_atdd else [("atdd", ["python3", "tools/atdd.py", "run", "--all"], {})]
    gates.append(("kernel-laws", [*CARGO, "test", "--manifest-path", "verification/Cargo.toml", "--locked"], {}))
    gates.append(("kernel-laws-supply-chain", [*CARGO, "deny", "--manifest-path", "verification/Cargo.toml",
                                               "--config", "deny.toml", "check"], {}))
    gates.extend(pinned_steps())
    steps = []
    for name, command, environment in gates:
        steps.append(run(name, command, environment))
        if source_state() != (revision, tree, ""):
            print(f"record_gate_evidence: the source changed during {name}", file=sys.stderr)
            return 2
    tools = {
        "rust": version(["rustc", "+1.97.1", "--version"]),
        "python": version(["python3", "--version"]),
        "node": version(["node", "--version"]),
        "cargo-deny": version([*CARGO, "deny", "--version"]),
        "cvc5": version([os.environ["ZENO_FCIS_CVC5"], "--version"]) if os.environ.get("ZENO_FCIS_CVC5") else None,
        "z3": version([os.environ["ZENO_FCIS_Z3"], "--version"]) if os.environ.get("ZENO_FCIS_Z3") else None,
        "lean": version([os.environ["ZENO_FCIS_LEAN"], "--version"]) if os.environ.get("ZENO_FCIS_LEAN") else None,
    }
    anchor = lean_trust_anchor()
    passed = all(step["exit_code"] == 0 for step in steps) and (anchor is None or anchor["matches"])
    record = {
        "schema": "zeno-fcis/gate-evidence/1",
        "revision": revision,
        "tree": tree,
        "tools": tools,
        "lean_trust_anchor": anchor,
        "steps": steps,
        "ignored_tests": ignored_tests(steps),
        "status": "passed" if passed else "failed",
    }
    # Everything above read the checkout. Publish only for the source the run
    # started with, checked after the last read.
    if source_state() != (revision, tree, ""):
        print("record_gate_evidence: the source changed during the run", file=sys.stderr)
        return 2
    args.out.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"record_gate_evidence: {record['status']} for {revision}", file=sys.stderr)
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
