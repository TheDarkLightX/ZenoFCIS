#!/usr/bin/env python3
"""Check the pinned list of tests that Miri does not interpret.

Every exclusion names one Miri group, package, test binary and exact test name,
the measured reason, and the native CI command that still runs the test. The
check fails unless the list equals what miri.yml skips, each listed name is
defined exactly once in the workspace, and the named native job runs it.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXCLUSIONS = ROOT / ".github" / "miri-exclusions.json"
WORKFLOW = ROOT / ".github" / "workflows" / "miri.yml"
FORMAT = "zeno-fcis/miri-exclusions/1"
ENTRY_KEYS = {"group", "package", "test_binary", "test", "reason", "native"}
NATIVE_KEYS = {"workflow", "job", "command"}
ROW_KEYS = {"group", "packages", "target", "test", "command", "miri_skip"}
NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


class ExclusionError(RuntimeError):
    """The Miri exclusion list and the workflow disagree."""


def load_exclusions(path: Path = EXCLUSIONS) -> list[dict]:
    document = json.loads(path.read_text(encoding="utf-8"))
    if set(document) != {"format", "exclusions"} or document["format"] != FORMAT:
        raise ExclusionError(f"{path.name}: unexpected format")
    entries = document["exclusions"]
    if not isinstance(entries, list):
        raise ExclusionError(f"{path.name}: exclusions must be a list")
    for entry in entries:
        if not isinstance(entry, dict) or set(entry) != ENTRY_KEYS:
            raise ExclusionError(f"{path.name}: malformed exclusion entry")
        if not isinstance(entry["native"], dict) or set(entry["native"]) != NATIVE_KEYS:
            raise ExclusionError(f"{path.name}: malformed native job for {entry.get('test')}")
        if NAME.fullmatch(entry["test"]) is None or NAME.fullmatch(entry["test_binary"]) is None:
            raise ExclusionError(f"{path.name}: exclusion names must be plain identifiers")
        if not entry["reason"].strip():
            raise ExclusionError(f"{path.name}: {entry['test']} has no measured reason")
    keys = [(entry["group"], entry["test"]) for entry in entries]
    if len(set(keys)) != len(keys):
        raise ExclusionError(f"{path.name}: duplicate exclusion")
    return entries


def workflow_rows(text: str) -> list[dict]:
    # JSON is also YAML; miri.yml keeps its matrix rows in this exact form.
    return json.loads(text.split("        include: ", 1)[1].split("    env:", 1)[0])


def workflow_skips(rows: list[dict]) -> set[tuple[str, str]]:
    skips: set[tuple[str, str]] = set()
    for row in rows:
        if not set(row) <= ROW_KEYS:
            raise ExclusionError(f"Miri row {row.get('group')} has unknown keys")
        if any("--skip" in str(row.get(key, "")) for key in ("packages", "target", "test")):
            raise ExclusionError(f"Miri row {row['group']} skips tests outside miri_skip")
        names = row.get("miri_skip", [])
        if not isinstance(names, list) or not all(isinstance(name, str) for name in names):
            raise ExclusionError(f"Miri row {row['group']} has a malformed miri_skip")
        skips.update((row["group"], name) for name in names)
    return skips


def test_definitions(root: Path, name: str) -> list[Path]:
    pattern = re.compile(
        r"#\[test\][^\n]*\n(?:\s*#\[[^\n]*\]\s*\n)*\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+"
        + re.escape(name) + r"\s*\(",
    )
    found = []
    for path in sorted((root / "crates").rglob("*.rs")):
        if "target" in path.relative_to(root).parts:
            continue
        found.extend(path for _ in pattern.finditer(path.read_text(encoding="utf-8")))
    return found


def job_block(text: str, job: str) -> str:
    lines = text.splitlines()
    try:
        start = lines.index(f"  {job}:")
    except ValueError as error:
        raise ExclusionError(f"native job {job} is absent") from error
    end = next((index for index in range(start + 1, len(lines))
                if re.match(r"^  \S", lines[index])), len(lines))
    return "\n".join(lines[start:end])


def check_native(root: Path, entry: dict) -> None:
    native = entry["native"]
    workflow = root / native["workflow"]
    if not workflow.is_file():
        raise ExclusionError(f"native workflow {native['workflow']} is absent")
    if native["command"] not in job_block(workflow.read_text(encoding="utf-8"), native["job"]):
        raise ExclusionError(f"native job {native['job']} does not run: {native['command']}")
    arguments = shlex.split(native["command"])
    covers = "--workspace" in arguments or any(
        arguments[index:index + 2] == ["-p", entry["package"]] for index in range(len(arguments)))
    if (arguments[:1] != ["cargo"] or "test" not in arguments or not covers
            or "--" in arguments or "--exclude" in arguments or "--skip" in arguments
            or any(flag in arguments for flag in ("--lib", "--doc", "--bins", "--examples"))):
        raise ExclusionError(f"native command does not run every test of {entry['package']}")


def check_static(root: Path, entries: list[dict], rows: list[dict]) -> None:
    listed = {(entry["group"], entry["test"]) for entry in entries}
    skipped = workflow_skips(rows)
    if listed != skipped:
        raise ExclusionError(
            f"Miri exclusions differ from miri.yml: unlisted {sorted(skipped - listed)}, "
            f"not skipped {sorted(listed - skipped)}")
    groups = {row["group"]: row for row in rows}
    for entry in entries:
        row = groups.get(entry["group"])
        if row is None or entry["package"] not in shlex.split(row["packages"]):
            raise ExclusionError(f"{entry['test']}: group {entry['group']} does not test {entry['package']}")
        definitions = test_definitions(root, entry["test"])
        expected = root / "crates" / entry["package"] / "tests" / f"{entry['test_binary']}.rs"
        if definitions != [expected]:
            raise ExclusionError(
                f"{entry['test']} must name exactly one test, in {expected.relative_to(root)}; "
                f"found {[str(path.relative_to(root)) for path in definitions]}")
        check_native(root, entry)


def check_listing(entries: list[dict], group: str, listing: str) -> None:
    names = [line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")]
    for entry in entries:
        if entry["group"] != group:
            continue
        matches = names.count(entry["test"])
        if matches != 1:
            raise ExclusionError(f"{entry['test']} matches {matches} tests in Miri group {group}")


def miri_listing(row: dict) -> str:
    command = ["cargo", "+nightly-2026-07-21", "miri", "test", "--locked",
               *shlex.split(row["packages"]), *shlex.split(row.get("target", "")),
               "--", "--list", "--format", "terse"]
    return subprocess.run(command, cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE).stdout


def miri_test_arguments(row: dict) -> str:
    names = row.get("miri_skip", [])
    if not names:
        return ""
    return "-- " + shlex.join(["--exact", *[part for name in names for part in ("--skip", name)]])


def check(selected: dict | None) -> str:
    entries = load_exclusions()
    rows = workflow_rows(WORKFLOW.read_text(encoding="utf-8"))
    check_static(ROOT, entries, rows)
    if selected is None or not selected.get("miri_skip"):
        return ""
    if selected not in rows:
        raise ExclusionError("selected group is absent from the checked matrix")
    check_listing(entries, selected["group"], miri_listing(selected))
    return miri_test_arguments(selected)


def self_test() -> None:
    """Planted controls: each hostile change must be refused."""

    def refused(action, label: str) -> None:
        try:
            action()
        except ExclusionError:
            return
        raise ExclusionError(f"self-test accepted {label}")

    with tempfile.TemporaryDirectory(prefix="zeno-fcis-miri-exclusions-") as raw:
        root = Path(raw)
        tests = root / "crates" / "demo" / "tests"
        tests.mkdir(parents=True)
        (tests / "big.rs").write_text("#[test]\nfn huge_case() {}\n#[test]\nfn small_case() {}\n")
        workflows = root / ".github" / "workflows"
        workflows.mkdir(parents=True)
        command = "cargo +1.97.1 test --workspace --all-features --locked"
        (workflows / "ci.yml").write_text(f"jobs:\n  rust:\n    steps:\n      - run: {command}\n")
        entry = {"group": "values", "package": "demo", "test_binary": "big", "test": "huge_case",
                 "reason": "measured", "native": {"workflow": ".github/workflows/ci.yml",
                                                  "job": "rust", "command": command}}
        row = {"group": "values", "packages": "-p demo", "miri_skip": ["huge_case"]}
        check_static(root, [entry], [row])
        check_listing([entry], "values", "huge_case: test\nsmall_case: test\n")
        if miri_test_arguments(row) != "-- --exact --skip huge_case":
            raise ExclusionError("self-test: unexpected Miri skip arguments")

        refused(lambda: check_static(root, [entry], [dict(row, miri_skip=["huge_case", "small_case"])]),
                "an unlisted skip in the workflow")
        refused(lambda: check_static(root, [entry], [{"group": "values", "packages": "-p demo"}]),
                "a listed test that the workflow does not skip")
        refused(lambda: check_static(root, [], [dict(row, target="-- --skip huge_case")]),
                "a skip written outside miri_skip")
        refused(lambda: check_static(root, [dict(entry, test="renamed_case")],
                                     [dict(row, miri_skip=["renamed_case"])]),
                "a listed test that no longer exists")
        refused(lambda: check_listing([entry], "values", "small_case: test\n"),
                "a listed test absent from the Miri listing")
        (tests / "other.rs").write_text("#[test]\nfn huge_case() {}\n")
        refused(lambda: check_static(root, [entry], [row]), "a skip that matches two tests")
        refused(lambda: check_listing([entry], "values", "huge_case: test\nhuge_case: test\n"),
                "a skip that matches two listed tests")
        (tests / "other.rs").unlink()
        refused(lambda: check_static(root, [dict(entry, native=dict(entry["native"], job="docs"))], [row]),
                "a native job that does not exist")
        filtered = "cargo +1.97.1 test -p demo --locked -- --skip huge_case"
        (workflows / "ci.yml").write_text(f"jobs:\n  rust:\n    steps:\n      - run: {filtered}\n")
        refused(lambda: check_static(root, [dict(entry, native=dict(entry["native"], command=filtered))], [row]),
                "a native command that filters the test out")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    sub.add_parser("self-test")
    checked = sub.add_parser("check")
    checked.add_argument("--selected", help="the selected matrix row as JSON")
    arguments = parser.parse_args()
    try:
        if arguments.mode == "self-test":
            self_test()
            print("miri-exclusions: self-test PASS (9 planted controls refused)")
            return 0
        selected = json.loads(arguments.selected) if arguments.selected else None
        test_arguments = check(selected)
    except (ExclusionError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"miri-exclusions: FAIL: {error}", file=sys.stderr)
        return 1
    if test_arguments:
        environment_file = os.environ.get("GITHUB_ENV")
        if environment_file:
            with Path(environment_file).open("a", encoding="utf-8") as output:
                output.write(f"MIRI_TEST_ARGS={test_arguments}\n")
        print(f"miri-exclusions: {selected['group']} runs with {test_arguments}")
    print(f"miri-exclusions: PASS ({len(load_exclusions())} pinned exclusion(s))")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
