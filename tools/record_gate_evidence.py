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
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SUMMARY = re.compile(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored")
IGNORED = re.compile(r'#\[\s*ignore\s*(?:=\s*"([^"]*)"\s*)?\]\s*(?:#\[[^\]]*\]\s*)*fn ([a-z0-9_]+)')
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


def compiled_test_sources(executable: Path) -> set[Path]:
    """Read the compiler's actual source closure, excluding uncompiled archives."""
    sources = set()
    text = executable.with_suffix(".d").read_text(encoding="utf-8").replace("\\\n", "")
    for line in text.splitlines():
        if line.startswith("#") or ": " not in line:
            continue
        words, word, escaped = [], [], False
        for character in line.split(": ", 1)[1] + " ":
            if escaped:
                word.append(character)
                escaped = False
            elif character == "\\":
                escaped = True
            elif character.isspace():
                if word:
                    words.append("".join(word).replace("$$", "$"))
                    word = []
            else:
                word.append(character)
        if escaped:
            raise ValueError("incomplete compiler dependency path")
        for name in words:
            if name.endswith(".rs"):
                source = Path(name)
                sources.add((ROOT / source).resolve(strict=True))
    if not sources:
        raise ValueError(f"missing compiler source closure for {executable}")
    return sources


def compiled_ignored_inventory(output: str) -> list[dict]:
    """Enumerate real test binaries and bind listed names to compiled sources.

    Listing is not execution. Ambiguous or unreadable source mappings fail the
    inventory instead of silently treating a partial scan as complete.
    """
    inventory = []
    artifacts = []
    for line in output.splitlines():
        if not line.startswith("{"):
            continue
        artifact = json.loads(line)
        if artifact.get("reason") == "compiler-artifact" and artifact.get("executable") \
                and artifact.get("profile", {}).get("test"):
            artifacts.append(artifact)
    if not artifacts:
        raise ValueError("Cargo reported no compiled test binaries")
    for artifact in artifacts:
        manifest = Path(artifact["manifest_path"]).resolve(strict=True)
        manifest.relative_to(ROOT)
        executable = Path(artifact["executable"])
        command = [str(executable), "--ignored", "--list"]
        listed = subprocess.run(command, cwd=ROOT, check=True, capture_output=True,
                                text=True, timeout=60).stdout
        candidates = []
        for source in sorted(compiled_test_sources(executable)):
            text = source.read_text(encoding="utf-8")
            for match in IGNORED.finditer(text):
                candidates.append((source, match.group(2), match.group(1) or ""))
        for qualified in re.findall(r"^([A-Za-z_][A-Za-z_0-9:]*)\: test$", listed, re.MULTILINE):
            name = qualified.rsplit("::", 1)[-1]
            matches = [row for row in candidates if row[1] == name]
            # The real binary supplies the full module identity. Distinct
            # template modules sharing a helper name retain their own source.
            modules = set(qualified.split("::")[:-1])
            def score(row):
                return len(modules.intersection(part.replace("-", "_")
                           for part in row[0].with_suffix("").parts))
            if matches:
                best = max(map(score, matches))
                matches = [row for row in matches if score(row) == best]
            if len(matches) != 1:
                raise ValueError(f"ambiguous or missing compiled source for {qualified}")
            source, _, reason = matches[0]
            inventory.append({"test": name, "qualified_test": qualified,
                              "target": artifact["target"]["name"],
                              "file": str(source.relative_to(ROOT)), "reason": reason,
                              "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest()})
    return inventory


HELPER_PARENTS = {
    "tests::process_helper_original_lean_cli": (
        "legacy_cli::private_original_cli_process_preserves_missing_tool_refusal",
        "legacy_cli::pinned_lean_cli_prove_is_process_level"),
    "tests::process_helper_checked_copy": (
        "tests::rc3_process_output_accepts_the_exact_byte_limit",
        "tests::rc3_private_executable_preserves_the_admitted_bytes"),
    "tests::process_helper_timeout": (
        "tests::rc3_formal_fail_closed_and_model_replay",
        "tests::rc3_process_timeout_includes_blocked_stdin_delivery"),
    "tests::process_helper_crash": ("tests::rc3_formal_fail_closed_and_model_replay",),
    "tests::process_helper_output_limit": ("tests::rc3_formal_fail_closed_and_model_replay",),
    "tests::process_helper_spawns_pipe_holder": (
        "tests::rc3_process_timeout_kills_descendant_holding_solver_pipes",),
    "tests::process_helper_holds_inherited_pipes": (
        "tests::rc3_process_timeout_kills_descendant_holding_solver_pipes",),
    "tests::process_helper_spawns_detached_pipe_child": (
        "tests::rc3_process_success_kills_descendants_after_collecting_output",),
    "tests::process_helper_sleeps_without_solver_pipes": (
        "tests::rc3_process_success_kills_descendants_after_collecting_output",),
}


def run(name: str, command: list[str], environment: dict[str, str] | None = None) -> dict:
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                            env={**os.environ, **(environment or {})})
    output = result.stdout + result.stderr
    counts = [0, 0, 0]
    for match in SUMMARY.finditer(output):
        for index in range(3):
            counts[index] += int(match.group(index + 1))
    record = {
        "name": name,
        "command": command,
        "exit_code": result.returncode,
        "tests": dict(zip(("passed", "failed", "ignored"), counts)),
        "observed_passed_tests": re.findall(r"^test ([A-Za-z_0-9:]+) \.\.\. ok$", output, re.MULTILINE),
        "evidence_complete": True,
    }
    if "--exact" in command:
        record["evidence_complete"] = counts == [1, 0, 0]
        if result.returncode == 0 and record["evidence_complete"]:
            record["observed_passed_tests"].append(command[command.index("--") - 1])
    if "--no-run" in command and "--message-format=json" in command:
        try:
            if result.returncode:
                raise ValueError("ignored-test inventory compilation failed")
            record["registered_ignored_tests"] = compiled_ignored_inventory(result.stdout)
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            record["evidence_complete"] = False
            record["inventory_error"] = str(error)
    print(f"{name}: exit {result.returncode}; evidence complete {record['evidence_complete']}", file=sys.stderr)
    return record


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
        steps.append(("pinned-symbolic-checks", [*CARGO, "test", "-p", "zeno-fcis-cli", "--locked", "--test",
                                                 "symbolic_cli", "pinned_symbolic_", "--", "--ignored"], {}))
    if cvc5:
        for test in ("system::tests::pinned_system_smt_agrees_with_exhaustive_check",
                     "system::tests::pinned_solver_route_agrees_with_exhaustive_route_on_small_programs"):
            name = test.rsplit("::", 1)[1].removeprefix("pinned_")
            steps.append((f"pinned-{name}", [*formal, test, "--", "--ignored", "--exact"], {}))
        steps.append(("pinned-counter-system-smt", [*CARGO, "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis",
                                                    "--locked", "synth::tests::pinned_counter_system_smt_agrees_with_exhaustive_check",
                                                    "--", "--ignored", "--exact"], {}))
    if lean and lean_root:
        steps.append(("private-reference-pinned-lean-kernel", [*formal, "tests::pinned_lean_translation_kernel_checks",
                                             "--", "--ignored", "--exact"], {}))
        steps.append(("private-reference-pinned-lean-corpus", [*formal, "translation_tests::operator_corpus_checks_with_existing_lean",
                                             "--", "--ignored", "--exact"], {"ZENO_FCIS_TRANSLATION_LEAN": lean}))
        steps.append(("private-reference-pinned-lean-cli", [*formal, "legacy_cli::pinned_lean_cli_prove_is_process_level",
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
    """Inventory actual registered ignores; identify only observed parent runs."""
    inventory = []
    for step in steps:
        for registered in step.get("registered_ignored_tests", []):
            entry = {**registered, "run_by": None, "parent_test": None}
            qualified, reason = entry["qualified_test"], entry["reason"]
            parents = HELPER_PARENTS.get(qualified, ())
            if reason.startswith("spawned by "):
                suffix = reason.removeprefix("spawned by ")
                if re.fullmatch(r"[a-z_0-9]+", suffix):
                    parents = (*parents, qualified.rsplit("::", 1)[0] + "::" + suffix)
            for gate in steps:
                if gate["exit_code"] or not gate.get("evidence_complete", True):
                    continue
                observed = gate.get("observed_passed_tests", [])
                if qualified in observed:
                    entry["run_by"] = gate["name"]
                    break
                parent = next((parent for parent in parents if parent in observed), None)
                if parent:
                    entry["run_by"], entry["parent_test"] = gate["name"], parent
                    break
            inventory.append(entry)
    keys = [(row["target"], row["qualified_test"], row["file"]) for row in inventory]
    if len(keys) != len(set(keys)):
        raise ValueError("duplicate compiled ignored-test identity")
    return sorted(inventory, key=lambda row: (row["file"], row["qualified_test"], row["target"]))


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
    gates.append(("normal-ignored-test-inventory", [*CARGO, "test", "--workspace", "--all-features",
                  "--all-targets", "--no-run", "--message-format=json", "--locked"], {}))
    gates.append(("private-ignored-test-inventory", [*CARGO, "test", "--manifest-path", "verification/Cargo.toml",
                  "--all-features", "--all-targets", "--no-run", "--message-format=json", "--locked"], {}))
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
    passed = all(step["exit_code"] == 0 and step["evidence_complete"] for step in steps) and (anchor is None or anchor["matches"])
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
