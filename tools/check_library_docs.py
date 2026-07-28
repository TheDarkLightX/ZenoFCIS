#!/usr/bin/env python3
"""Fail closed when the public library entry points drift or break links."""

from __future__ import annotations

import re
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parents[1]
REQUIRED_FILES = (
    Path("README.md"),
    Path("CHANGELOG.md"),
    Path("llms.txt"),
    Path("docs/INDEX.md"),
    Path("docs/INSTALLATION.md"),
    Path("docs/QUICKSTART.md"),
    Path("docs/API_REFERENCE.md"),
    Path("docs/CRATE_MAP.md"),
    Path("docs/FEATURE_MATRIX.md"),
    Path("docs/LLM_USAGE.md"),
    Path("docs/RC1_RELEASE_NOTES.md"),
    Path("docs/PACKAGING.md"),
    Path("docs/RELEASE_ASSURANCE.md"),
    Path("crates/zeno-fcis/examples/minimal_core.rs"),
    Path("crates/zeno-fcis/examples/checked_backend.rs"),
    Path("fixtures/external-consumer/Cargo.toml"),
    Path("fixtures/external-consumer/Cargo.lock"),
    Path("fixtures/external-consumer/src/main.rs"),
)
VERSIONED_DOCS = (
    Path("README.md"),
    Path("CHANGELOG.md"),
    Path("llms.txt"),
    Path("docs/INSTALLATION.md"),
    Path("docs/QUICKSTART.md"),
    Path("docs/API_REFERENCE.md"),
    Path("docs/CRATE_MAP.md"),
    Path("docs/FEATURE_MATRIX.md"),
    Path("docs/RC1_RELEASE_NOTES.md"),
    Path("docs/PACKAGING.md"),
)
REQUIRED_README_MARKERS = (
    "use zeno_fcis::prelude::*;",
    "docs/QUICKSTART.md",
    "docs/INSTALLATION.md",
    "docs/API_REFERENCE.md",
    "docs/CRATE_MAP.md",
    "docs/FEATURE_MATRIX.md",
    "docs/LLM_USAGE.md",
    "docs/RC1_RELEASE_NOTES.md",
    "docs/PACKAGING.md",
    "--example minimal_core",
    "--example checked_backend",
)
MARKDOWN_LINK = re.compile(r"\[[^\]]+\]\(([^)]+)\)")


def workspace_version() -> str:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    value = manifest["workspace"]["package"]["version"]
    if not isinstance(value, str) or not value:
        raise ValueError("workspace.package.version must be a nonempty string")
    return value


def check_required_files(failures: list[str]) -> None:
    for relative in REQUIRED_FILES:
        if not (ROOT / relative).is_file():
            failures.append(f"missing public library entry point: {relative}")


def check_version_markers(version: str, failures: list[str]) -> None:
    for relative in VERSIONED_DOCS:
        path = ROOT / relative
        if path.is_file() and version not in path.read_text(encoding="utf-8"):
            failures.append(f"{relative}: missing workspace version {version}")


def check_readme_markers(failures: list[str]) -> None:
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    for marker in REQUIRED_README_MARKERS:
        if marker not in readme:
            failures.append(f"README.md: missing consumer marker {marker!r}")


def check_markdown_links(failures: list[str]) -> None:
    files = [ROOT / "README.md", *sorted((ROOT / "docs").glob("*.md"))]
    for source in files:
        text = source.read_text(encoding="utf-8")
        for target in MARKDOWN_LINK.findall(text):
            if "://" in target or target.startswith("#"):
                continue
            local = target.split("#", 1)[0]
            if local and not (source.parent / local).resolve().exists():
                failures.append(f"{source.relative_to(ROOT)}: broken link {target}")


def main() -> int:
    failures: list[str] = []
    check_required_files(failures)
    version = workspace_version()
    check_version_markers(version, failures)
    check_readme_markers(failures)
    check_markdown_links(failures)
    if failures:
        for failure in failures:
            print(f"library-docs: FAIL: {failure}")
        return 1
    print(f"library-docs: PASS (workspace version {version})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
