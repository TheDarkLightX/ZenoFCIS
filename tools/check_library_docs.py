#!/usr/bin/env python3
"""Fail closed when the public library entry points drift or break links."""

from __future__ import annotations

import json
import re
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parents[1]
REQUIRED_FILES = (
    Path("README.md"),
    Path("CHANGELOG.md"),
    Path("CONTRIBUTING.md"),
    Path("SECURITY.md"),
    Path("llms.txt"),
    Path("package.json"),
    Path("package-lock.json"),
    Path("probity.config.ts"),
    Path(".github/CODEOWNERS"),
    Path(".github/workflows/adopter-acceptance.yml"),
    Path(".github/workflows/developer-guardrails.yml"),
    Path(".github/workflows/security-hotspots.yml"),
    Path(".github/workflows/release-candidate.yml"),
    Path(".github/workflows/formal-tools.yml"),
    Path(".github/workflows/qemu-demo.yml"),
    Path("release/formal-tools-linux-x86_64.sha256"),
    Path("release/lean-4.30.0-tree.sha256"),
    Path("docs/INDEX.md"),
    Path("docs/INSTALLATION.md"),
    Path("docs/QUICKSTART.md"),
    Path("docs/API_REFERENCE.md"),
    Path("docs/V1_PRODUCT_CONTRACT.md"),
    Path("docs/ACCEPTANCE_TESTING.md"),
    Path("docs/DEVELOPER_GUARDRAILS.md"),
    Path("docs/CRATE_MAP.md"),
    Path("docs/FEATURE_MATRIX.md"),
    Path("docs/LLM_USAGE.md"),
    Path("docs/LLM_CYBERSECURITY_REVIEW.md"),
    Path("docs/SECURITY_HOTSPOT_MODEL.md"),
    Path("docs/SECURITY_REVIEW_PLAYBOOK.md"),
    Path("docs/SECURITY_STANDARDS_SNAPSHOT.md"),
    Path("docs/RC1_RELEASE_NOTES.md"),
    Path("docs/RC2_RELEASE_NOTES.md"),
    Path("docs/RC3_RELEASE_NOTES.md"),
    Path("docs/RC3_READINESS_REVIEW.md"),
    Path("docs/RC3_AUTHORING_CONTRACT.md"),
    Path("docs/ZENO_LANGUAGE_V1.md"),
    Path("docs/TEMPORAL_LOGIC_V1.md"),
    Path("docs/FORMAL_TOOLS_RC3.md"),
    Path("docs/MINI_DETERMINATOR.md"),
    Path("docs/QEMU_MINI_DETERMINATOR.md"),
    Path("docs/CLI_REFERENCE.md"),
    Path("docs/tutorials/LANGUAGE.md"),
    Path("docs/tutorials/COMPOSITION.md"),
    Path("docs/tutorials/TEMPORAL.md"),
    Path("docs/tutorials/FORMAL_TOOLS.md"),
    Path("docs/tutorials/MINI_DETERMINATOR.md"),
    Path("docs/tutorials/CLI.md"),
    Path("docs/V1_RELEASE_CHECKLIST.md"),
    Path("docs/PACKAGING.md"),
    Path("docs/RELEASE_ASSURANCE.md"),
    Path("docs/AUTHENTICATED_AUTHORITY_BOUNDARY.md"),
    Path("crates/zeno-fcis/examples/minimal_core.rs"),
    Path("crates/zeno-fcis/examples/checked_backend.rs"),
    Path("crates/zeno-fcis-spec/examples/mini_determinator.rs"),
    Path("crates/zeno-fcis-spec/examples/temporal_walkthrough.rs"),
    Path("demos/mini-determinator-qemu/Cargo.toml"),
    Path("demos/mini-determinator-qemu/Cargo.lock"),
    Path("demos/mini-determinator-qemu/kernel/src/main.rs"),
    Path("tools/qemu_demo.py"),
    Path("docs/assets/marketing/mini-determinator-qemu-kernel.png"),
    Path("docs/assets/marketing/mini-determinator-qemu-serial.txt"),
    Path("docs/assets/marketing/mini-determinator-qemu-capture.json"),
    Path("docs/assets/marketing/accumulated-diagnostics.png"),
    Path("docs/assets/marketing/accumulated-diagnostics.svg"),
    Path("docs/assets/marketing/README.md"),
    Path("acceptance/features/adopter_journey.feature"),
    Path("acceptance/features/architecture_and_authority.feature"),
    Path("acceptance/features/release_candidate.feature"),
    Path("acceptance/features/rc3_authoring.feature"),
    Path("examples/minimal/project.zeno"),
    Path("examples/mini-determinator/project.zeno"),
    Path("examples/diagnostics-tour/project.zeno"),
    Path("tools/atdd.py"),
    Path("tools/check_probity.py"),
    Path("tools/security_hotspots.py"),
    Path("security/hotspots-baseline.json"),
    Path("security/review-report.schema.json"),
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
    Path("docs/RC3_RELEASE_NOTES.md"),
    Path("docs/RC3_READINESS_REVIEW.md"),
    Path("docs/RC3_AUTHORING_CONTRACT.md"),
    Path("docs/ZENO_LANGUAGE_V1.md"),
    Path("docs/TEMPORAL_LOGIC_V1.md"),
    Path("docs/FORMAL_TOOLS_RC3.md"),
    Path("docs/MINI_DETERMINATOR.md"),
    Path("docs/QEMU_MINI_DETERMINATOR.md"),
    Path("docs/CLI_REFERENCE.md"),
    Path("docs/V1_RELEASE_CHECKLIST.md"),
    Path("docs/PACKAGING.md"),
)
REQUIRED_README_MARKERS = (
    "use zeno_fcis::prelude::*;",
    "docs/QUICKSTART.md",
    "docs/INSTALLATION.md",
    "docs/API_REFERENCE.md",
    "docs/V1_PRODUCT_CONTRACT.md",
    "docs/ACCEPTANCE_TESTING.md",
    "docs/DEVELOPER_GUARDRAILS.md",
    "docs/CRATE_MAP.md",
    "docs/FEATURE_MATRIX.md",
    "docs/LLM_USAGE.md",
    "docs/LLM_CYBERSECURITY_REVIEW.md",
    "docs/SECURITY_HOTSPOT_MODEL.md",
    "docs/SECURITY_REVIEW_PLAYBOOK.md",
    "docs/SECURITY_STANDARDS_SNAPSHOT.md",
    "docs/RC3_RELEASE_NOTES.md",
    "docs/RC3_READINESS_REVIEW.md",
    "docs/RC3_AUTHORING_CONTRACT.md",
    "docs/ZENO_LANGUAGE_V1.md",
    "docs/TEMPORAL_LOGIC_V1.md",
    "docs/FORMAL_TOOLS_RC3.md",
    "docs/MINI_DETERMINATOR.md",
    "docs/QEMU_MINI_DETERMINATOR.md",
    "docs/CLI_REFERENCE.md",
    "docs/V1_RELEASE_CHECKLIST.md",
    "docs/PACKAGING.md",
    "--example minimal_core",
    "--example checked_backend",
    "python3 tools/atdd.py run --all",
    "python3 tools/security_hotspots.py check",
)
REQUIRED_CODEOWNER_MARKERS = (
    "* @TheDarkLightX",
    "/release/ @TheDarkLightX",
    "/crates/ @TheDarkLightX",
    "/docs/ @TheDarkLightX",
    "/security/ @TheDarkLightX",
)
REQUIRED_RELEASE_WORKFLOW_MARKERS = (
    '"v1.0.0-rc.*"',
    "contents: read",
    "python3 tools/rc_package.py build",
    "retention-days: 30",
)
REQUIRED_SECURITY_WORKFLOW_MARKERS = (
    "contents: read",
    "persist-credentials: false",
    "python3 tools/security_hotspots.py self-test",
    "python3 tools/security_hotspots.py check",
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


def check_release_markers(failures: list[str]) -> None:
    codeowners = (ROOT / ".github/CODEOWNERS").read_text(encoding="utf-8")
    for marker in REQUIRED_CODEOWNER_MARKERS:
        if marker not in codeowners:
            failures.append(f".github/CODEOWNERS: missing owner marker {marker!r}")

    workflow = (ROOT / ".github/workflows/release-candidate.yml").read_text(
        encoding="utf-8"
    )
    for marker in REQUIRED_RELEASE_WORKFLOW_MARKERS:
        if marker not in workflow:
            failures.append(
                ".github/workflows/release-candidate.yml: "
                f"missing release marker {marker!r}"
            )

    security_workflow = (
        ROOT / ".github/workflows/security-hotspots.yml"
    ).read_text(encoding="utf-8")
    for marker in REQUIRED_SECURITY_WORKFLOW_MARKERS:
        if marker not in security_workflow:
            failures.append(
                ".github/workflows/security-hotspots.yml: "
                f"missing security marker {marker!r}"
            )


def check_security_json(failures: list[str]) -> None:
    """Reject malformed or substituted public security contracts."""

    baseline_path = ROOT / "security/hotspots-baseline.json"
    schema_path = ROOT / "security/review-report.schema.json"
    try:
        baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        failures.append(f"security contract JSON is invalid: {error}")
        return
    if baseline.get("format") != "zeno-fcis/security-hotspots/1":
        failures.append("security/hotspots-baseline.json: unsupported format")
    if schema.get("$schema") != "https://json-schema.org/draft/2020-12/schema":
        failures.append("security/review-report.schema.json: unsupported schema draft")
    if schema.get("title") != "ZenoFCIS security review report":
        failures.append("security/review-report.schema.json: unexpected title")


def check_markdown_links(failures: list[str]) -> None:
    files = [ROOT / "README.md", *sorted((ROOT / "docs").rglob("*.md"))]
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
    check_release_markers(failures)
    check_security_json(failures)
    check_markdown_links(failures)
    if failures:
        for failure in failures:
            print(f"library-docs: FAIL: {failure}")
        return 1
    print(f"library-docs: PASS (workspace version {version})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
