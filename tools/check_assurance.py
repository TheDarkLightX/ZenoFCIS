#!/usr/bin/env python3
"""Fail-closed repository assurance checks using only the Python standard library."""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator


ROOT = Path(__file__).resolve().parents[1]
DEPENDENCY_TABLES = {"dependencies", "dev-dependencies", "build-dependencies"}
SEMANTIC_CRATES = (
    "zeno-fcis-core",
    "zeno-fcis-value",
    "zeno-fcis-codec",
    "zeno-fcis-crypto",
    "zeno-fcis-schema",
    "zeno-fcis-project",
    "zeno-fcis-catalog",
    "zeno-fcis-transition",
    "zeno-fcis-laws",
    "zeno-fcis-authority",
    "zeno-fcis-security",
    "zeno-fcis-secret",
    "zeno-fcis-patch",
    "zeno-fcis-plan",
    "zeno-fcis-receipt",
    "zeno-fcis-shell",
    "zeno-fcis-compose",
    "zeno-fcis-domain",
    "zeno-fcis-composed-program",
    "zeno-fcis-refine",
    "zeno-fcis-profile-zenodex",
    "zeno-fcis-evidence",
    "zeno-fcis-authenticated",
    "zeno-fcis-synthesis",
    "zeno-fcis-backend",
)
DEPENDENCY_RING = {
    "zeno-fcis-core": 0,
    "zeno-fcis-value": 0,
    "zeno-fcis-codec": 0,
    "zeno-fcis-crypto": 1,
    "zeno-fcis-schema": 1,
    "zeno-fcis-project": 1,
    "zeno-fcis-patch": 1,
    "zeno-fcis-plan": 1,
    "zeno-fcis-receipt": 1,
    "zeno-fcis-shell": 1,
    "zeno-fcis-compose": 2,
    "zeno-fcis-domain": 3,
    "zeno-fcis-composed-program": 5,
    "zeno-fcis-refine": 2,
    "zeno-fcis-evidence": 2,
    "zeno-fcis-authenticated": 2,
    "zeno-fcis-synthesis": 2,
    "zeno-fcis-backend": 3,
    "zeno-fcis-catalog": 2,
    "zeno-fcis-transition": 3,
    "zeno-fcis-laws": 3,
    "zeno-fcis-authority": 4,
    "zeno-fcis-security": 2,
    "zeno-fcis-secret": 1,
    "zeno-fcis": 6,
    "zeno-fcis-profile-zenodex": 3,
    "zeno-fcis-codegen": 3,
    "zeno-fcis-codegen-fixture": 3,
    "zeno-fcis-bootstrap": 3,
    "zeno-fcis-adapter": 3,
    "zeno-fcis-adapter-zenodex": 3,
    "zeno-fcis-shell-sqlite": 5,
    "zeno-fcis-collections": 3,
}


@dataclass(frozen=True)
class ForbiddenPattern:
    name: str
    expression: re.Pattern[str]
    witness: str


FORBIDDEN_PATTERNS = (
    ForbiddenPattern("unsafe-block", re.compile(r"\bunsafe\s*\{"), "unsafe { operation(); }"),
    ForbiddenPattern("unsafe-function", re.compile(r"\bunsafe\s+fn\b"), "unsafe fn operation() {}"),
    ForbiddenPattern("foreign-function", re.compile(r'\bextern\s+"C"'), 'extern "C" { }'),
    ForbiddenPattern("filesystem", re.compile(r"\bstd::fs(?:::|\b)"), "std::fs::read(path)"),
    ForbiddenPattern("network", re.compile(r"\bstd::net(?:::|\b)"), "std::net::TcpStream"),
    ForbiddenPattern("process", re.compile(r"\bstd::process(?:::|\b)"), "std::process::Command"),
    ForbiddenPattern(
        "environment",
        re.compile(r"\bstd::env(?:::|\b)|\b(?:env|option_env)!\s*\("),
        "std::env::var(name)",
    ),
    ForbiddenPattern("wall-clock", re.compile(r"\bstd::time(?:::|\b)"), "std::time::SystemTime"),
    ForbiddenPattern("system-time", re.compile(r"\b(?:SystemTime|Instant)::"), "Instant::now()"),
    ForbiddenPattern("threads", re.compile(r"\b(?:std::)?thread::"), "std::thread::spawn(f)"),
    ForbiddenPattern("async-runtime", re.compile(r"\btokio::"), "tokio::spawn(f)"),
    ForbiddenPattern("async-function", re.compile(r"\basync\s+fn\b"), "async fn operation() {}"),
    ForbiddenPattern("randomness", re.compile(r"\b(?:rand|getrandom)::"), "rand::random()"),
    ForbiddenPattern(
        "interior-mutability",
        re.compile(r"\b(?:RefCell|Mutex|RwLock|Atomic[A-Za-z0-9_]*)\s*<"),
        "Mutex<State>",
    ),
    ForbiddenPattern("floating-point", re.compile(r"\b(?:f32|f64)\b"), "let value: f64 = 1.0;"),
    ForbiddenPattern("mutable-static", re.compile(r"\bstatic\s+mut\b"), "static mut STATE: u8 = 0;"),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove every forbidden-pattern rule rejects its witness before scanning",
    )
    return parser.parse_args()


def read_toml(path: Path) -> dict[str, object]:
    with path.open("rb") as stream:
        return tomllib.load(stream)


def workspace_members() -> tuple[Path, ...]:
    document = read_toml(ROOT / "Cargo.toml")
    workspace = document.get("workspace")
    if not isinstance(workspace, dict):
        raise ValueError("Cargo.toml has no [workspace] table")
    members = workspace.get("members")
    if not isinstance(members, list) or not all(isinstance(item, str) for item in members):
        raise ValueError("workspace.members must be a string array")
    return tuple(ROOT / item for item in members)


def dependency_tables(value: object) -> Iterator[tuple[str, dict[str, object]]]:
    if not isinstance(value, dict):
        return
    for key, child in value.items():
        if key in DEPENDENCY_TABLES and isinstance(child, dict):
            yield key, child
        elif isinstance(child, dict):
            yield from dependency_tables(child)


def check_external_dependency_pins(manifest: Path) -> list[str]:
    failures: list[str] = []
    document = read_toml(manifest)
    for table_name, table in dependency_tables(document):
        for name, specification in table.items():
            location = f"{manifest.relative_to(ROOT)} [{table_name}] {name}"
            if isinstance(specification, str):
                if not specification.startswith("="):
                    failures.append(f"{location}: external version must use an exact = pin")
                continue
            if not isinstance(specification, dict):
                failures.append(f"{location}: dependency specification must be a string or table")
                continue
            if "path" in specification:
                continue
            version = specification.get("version")
            if not isinstance(version, str) or not version.startswith("="):
                failures.append(f"{location}: external dependency must have an exact = version")
    return failures


def check_dependency_ring(manifest: Path) -> list[str]:
    failures: list[str] = []
    document = read_toml(manifest)
    package = document.get("package")
    if not isinstance(package, dict) or not isinstance(package.get("name"), str):
        return [f"{manifest.relative_to(ROOT)}: package.name is missing"]
    package_name = package["name"]
    package_ring = DEPENDENCY_RING.get(package_name)
    if package_ring is None:
        return [f"{manifest.relative_to(ROOT)}: package has no dependency-ring assignment"]
    for table_name, table in dependency_tables(document):
        for dependency_name in table:
            dependency_ring = DEPENDENCY_RING.get(dependency_name)
            if dependency_ring is not None and dependency_ring > package_ring:
                failures.append(
                    f"{manifest.relative_to(ROOT)} [{table_name}] {dependency_name}: "
                    f"ring {package_ring} cannot depend on ring {dependency_ring}"
                )
    return failures


def line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def check_semantic_source(crate_name: str) -> list[str]:
    failures: list[str] = []
    source_root = ROOT / "crates" / crate_name / "src"
    if not source_root.is_dir():
        return [f"missing semantic crate source: crates/{crate_name}/src"]
    for path in sorted(source_root.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        for forbidden in FORBIDDEN_PATTERNS:
            match = forbidden.expression.search(text)
            if match is not None:
                failures.append(
                    f"{path.relative_to(ROOT)}:{line_number(text, match.start())}: "
                    f"semantic core contains forbidden {forbidden.name}"
                )
    return failures


def check_unsafe_prohibition(member: Path) -> list[str]:
    library = member / "src" / "lib.rs"
    if not library.is_file():
        return []
    text = library.read_text(encoding="utf-8")
    if "#![forbid(unsafe_code)]" not in text:
        return [f"{library.relative_to(ROOT)}: missing #![forbid(unsafe_code)]"]
    return []


def check_workflows() -> list[str]:
    failures: list[str] = []
    workflow_root = ROOT / ".github" / "workflows"
    action_pattern = re.compile(r"^\s*uses:\s*([^\s#]+)", re.MULTILINE)
    write_permission = re.compile(
        r"^\s*(?:permissions:\s*write-all|[A-Za-z0-9_-]+:\s*write)\s*$", re.MULTILINE
    )
    for path in sorted(workflow_root.glob("*.yml")):
        text = path.read_text(encoding="utf-8")
        match = write_permission.search(text)
        if match is not None:
            failures.append(
                f"{path.relative_to(ROOT)}:{line_number(text, match.start())}: write permission forbidden"
            )
        for action in action_pattern.findall(text):
            if action.startswith("./"):
                continue
            _, separator, revision = action.rpartition("@")
            if not separator or re.fullmatch(r"[0-9a-f]{40}", revision) is None:
                failures.append(
                    f"{path.relative_to(ROOT)}: action must be pinned to a 40-character commit: {action}"
                )
    return failures


def run_self_test() -> list[str]:
    failures: list[str] = []
    for forbidden in FORBIDDEN_PATTERNS:
        if forbidden.expression.search(forbidden.witness) is None:
            failures.append(f"self-test failed for {forbidden.name}")
    safe_witness = "fn transition(state: &State) -> State { state.clone() }"
    unexpected = [item.name for item in FORBIDDEN_PATTERNS if item.expression.search(safe_witness)]
    if unexpected:
        failures.append(f"safe witness rejected by: {', '.join(unexpected)}")
    return failures


def main() -> int:
    args = parse_args()
    failures: list[str] = []
    if args.self_test:
        failures.extend(run_self_test())

    try:
        members = workspace_members()
    except (OSError, tomllib.TOMLDecodeError, ValueError) as error:
        print(f"assurance: workspace parse failed: {error}", file=sys.stderr)
        return 2

    member_names = {member.name for member in members}
    missing_semantic = sorted(set(SEMANTIC_CRATES) - member_names)
    failures.extend(f"semantic crate absent from workspace: {name}" for name in missing_semantic)

    for member in members:
        failures.extend(check_unsafe_prohibition(member))
        manifest = member / "Cargo.toml"
        if not manifest.is_file():
            failures.append(f"missing workspace manifest: {manifest.relative_to(ROOT)}")
            continue
        try:
            failures.extend(check_external_dependency_pins(manifest))
            failures.extend(check_dependency_ring(manifest))
        except (OSError, tomllib.TOMLDecodeError) as error:
            failures.append(f"{manifest.relative_to(ROOT)}: cannot parse: {error}")

    for crate_name in SEMANTIC_CRATES:
        failures.extend(check_semantic_source(crate_name))
    failures.extend(check_workflows())

    if failures:
        for failure in failures:
            print(f"assurance: {failure}", file=sys.stderr)
        print(f"assurance: FAILED ({len(failures)} finding(s))", file=sys.stderr)
        return 1

    mode = "self-test + repository" if args.self_test else "repository"
    print(
        f"assurance: PASS ({mode}; {len(members)} crates; "
        f"{len(SEMANTIC_CRATES)} semantic boundaries; {len(FORBIDDEN_PATTERNS)} effect rules)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
