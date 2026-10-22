#!/usr/bin/env python3
"""Check the reviewed public-document inventory without printing file contents."""

from __future__ import annotations

from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = Path("release/public-documents.txt")
DOCUMENT_SUFFIXES = {".md", ".pdf", ".tex"}
# Published documents without a document suffix, including attribution records.
EXTRA_DOCUMENTS = frozenset({
    "CITATION.cff",
    "NOTICE",
    "THIRD_PARTY_NOTICES",
    "llms.txt",
    "crates/zeno-fcis-cli/contract-app/README.md.in",
    "site/public/index.html",
})
LINK_SCAN_SUFFIXES = {".md", ".tex", ".txt", ".in", ".html"}
PRIVATE_LINK = re.compile(r"https?://(?:chatgpt\.com|chat\.openai\.com)/(?:space|share)/", re.I)


def check(root: Path) -> list[str]:
    failures: list[str] = []
    inventory = root / INVENTORY
    if not inventory.is_file():
        return ["missing reviewed public-document inventory"]
    rows = [line for line in inventory.read_text().splitlines() if line and not line.startswith("#")]
    if len(rows) != len(set(rows)):
        failures.append("duplicate document inventory entry")
    for name in rows:
        path = Path(name)
        if path.is_absolute() or ".." in path.parts or not (
            name.startswith("docs/") or path.suffix in DOCUMENT_SUFFIXES or name in EXTRA_DOCUMENTS
        ):
            failures.append("invalid document inventory path")
            continue
        target = root / path
        if target.is_symlink() or not target.is_file():
            failures.append(f"missing or nonregular approved document: {name}")
    tracked = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root
    ).decode().split("\0")
    # Ignored drafts in docs also require review if somebody adds them forcibly.
    docs = [str(path.relative_to(root)) for path in (root / "docs").rglob("*") if path.is_file()]
    # The cached index also names tracked files removed from the working tree.
    # Check the candidate contents; missing approved documents still fail the
    # independent inventory check above. Keep symlinks visible for refusal.
    actual = {name for name in [*tracked, *docs] if name and
              ((root / name).is_file() or (root / name).is_symlink()) and
              (name.startswith("docs/") or Path(name).suffix in DOCUMENT_SUFFIXES
               or name in EXTRA_DOCUMENTS)}
    for name in sorted(actual - set(rows)):
        failures.append(f"unreviewed public document: {name}")
    for name in sorted(actual):
        path = root / name
        if (path.suffix not in LINK_SCAN_SUFFIXES and name not in EXTRA_DOCUMENTS) \
                or path.is_symlink() or not path.is_file():
            continue
        if PRIVATE_LINK.search(path.read_text(encoding="utf-8")):
            failures.append(f"private workspace link in public document: {name}")
    return failures


def main() -> int:
    try:
        failures = check(ROOT)
    except (OSError, UnicodeError, subprocess.CalledProcessError):
        failures = ["document inventory could not be inspected"]
    for failure in failures:
        print(f"public-docs: FAIL: {failure}")
    if failures:
        return 1
    print("public-docs: PASS (reviewed inventory and workspace-link checks)")
    return 0


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
