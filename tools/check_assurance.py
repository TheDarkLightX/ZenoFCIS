#!/usr/bin/env python3
"""Repository guardrails with semantic inspection delegated to the Rust resolver."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator

from resolved_purity import ResolvedChecker, ResolutionError
from workflow_contexts import check_contexts


ROOT = Path(__file__).resolve().parents[1]
DEPENDENCY_TABLES = {"dependencies", "dev-dependencies", "build-dependencies"}
SEMANTIC_CRATES = (
    "zeno-fcis-core",
    "zeno-fcis-value",
    "zeno-fcis-codec",
    "zeno-fcis-spec",
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
    "zeno-fcis-authenticated-authority",
    "zeno-fcis-synthesis",
    "zeno-fcis-backend",
)
DEPENDENCY_RING = {
    "zeno-fcis-core": 0,
    "zeno-fcis-value": 0,
    "zeno-fcis-codec": 0,
    "zeno-fcis-spec": 1,
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
    "zeno-fcis-authenticated-authority": 5,
    "zeno-fcis-synthesis": 2,
    "zeno-fcis-backend": 3,
    "zeno-fcis-formal-tools": 4,
    "zeno-fcis-cli": 5,
    "zeno-fcis-catalog": 2,
    "zeno-fcis-transition": 3,
    "zeno-fcis-laws": 3,
    "zeno-fcis-authority": 4,
    "zeno-fcis-security": 2,
    "zeno-fcis-secret": 1,
    "zeno-fcis": 6,
    "zeno-fcis-profile-zenodex": 3,
    "zeno-fcis-codegen": 3,
    "zeno-fcis-generated-code-tests": 3,
    "zeno-fcis-bootstrap": 3,
    "zeno-fcis-adapter": 3,
    "zeno-fcis-adapter-zenodex": 3,
    "zeno-fcis-shell-sqlite": 5,
    "zeno-fcis-collections": 3,
}


@dataclass(frozen=True)
class DecoderAllocationRegion:
    path: str
    start_marker: str
    end_marker: str
    required_patterns: tuple[str, ...]


DECODER_ALLOCATION_REGIONS = (
    DecoderAllocationRegion(
        "crates/zeno-fcis-codec/src/lib.rs",
        "fn decode_value_inner(",
        "fn initial_collection_capacity(",
        (
            r"Vec::with_capacity\(initial_collection_capacity\(count,cursor\.remaining\(\),1,?\)\?\)",
            r"Vec::with_capacity\(initial_collection_capacity\(count,cursor\.remaining\(\),3,?\)\?\)",
            r"Vec::with_capacity\(initial_collection_capacity\(count,cursor\.remaining\(\),10,?\)\?\)",
        ),
    ),
    DecoderAllocationRegion(
        "crates/zeno-fcis-patch/src/lib.rs",
        "pub fn decode_canonical_patch(",
        "fn decode_patch_operation(",
        (
            r"Vec::with_capacity\(initial_collection_capacity\(operation_count,cursor\.remaining\(\),4,?\)\?\)",
        ),
    ),
    DecoderAllocationRegion(
        "crates/zeno-fcis-patch/src/lib.rs",
        "fn decode_value_path(",
        "fn decode_patch_value(",
        (
            r"Vec::with_capacity\(initial_collection_capacity\(segment_count,cursor\.remaining\(\),1,?\)\?\)",
        ),
    ),
    DecoderAllocationRegion(
        "crates/zeno-fcis-plan/src/lib.rs",
        "pub fn decode_commit_plan(",
        "pub fn decode_outbox_plan(",
        (
            r"Vec::with_capacity\(initial_collection_capacity\(effect_count,cursor\.remaining\(\),4,?\)\?\)",
        ),
    ),
    DecoderAllocationRegion(
        "crates/zeno-fcis-plan/src/lib.rs",
        "pub fn decode_outbox_plan(",
        "fn enforce_plan_input_limit(",
        (
            r"Vec::with_capacity\(initial_collection_capacity\(entry_count,cursor\.remaining\(\),4,?\)\?\)",
        ),
    ),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="exercise the actual resolver and independent guardrail witnesses before scanning",
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


def check_semantic_sources(checker: ResolvedChecker) -> list[str]:
    paths = [ROOT / "crates" / name for name in SEMANTIC_CRATES]
    missing = [f"missing semantic crate source: crates/{name}/src"
               for name, path in zip(SEMANTIC_CRATES, paths) if not (path / "src").is_dir()]
    if missing:
        return missing
    try:
        report = checker.check(paths)
    except ResolutionError as error:
        return [f"resolved semantic inspection refused: {error}"]
    return semantic_report_failures(report)


def semantic_report_failures(report: dict) -> list[str]:
    failures = [f"{entry['file']}: unreadable: {entry['message']}"
                for entry in report["unreadable"]]
    for finding in report["findings"]:
        # Assurance's semantic boundary also forbids floating-point and
        # unstable hashes, which the interactive purity command calls warnings.
        if finding["severity"] == "error" or finding["rule"] in {"floating-point", "unstable-hash"}:
            failures.append(f"{finding['file']}:{finding['line']}:{finding['column']}: "
                            f"semantic core {finding['rule']}: {finding['subject']}")
    if report["status"] in {"violations", "unreadable"} and not failures:
        failures.append("resolved checker refused without a corresponding diagnostic")
    return failures


def check_decoder_region_text(
    label: str,
    text: str,
    required_patterns: tuple[str, ...],
) -> list[str]:
    failures: list[str] = []
    compact = re.sub(r"\s+", "", text)
    capacity_count = compact.count("Vec::with_capacity(")
    if capacity_count != len(required_patterns):
        failures.append(
            f"{label}: expected {len(required_patterns)} guarded collection allocation(s), "
            f"found {capacity_count}"
        )
    for pattern in required_patterns:
        count = len(re.findall(pattern, compact))
        if count != 1:
            failures.append(
                f"{label}: required wire-bounded allocation pattern occurs {count} time(s): {pattern}"
            )
    return failures


def check_decoder_allocation_guards() -> list[str]:
    failures: list[str] = []
    for requirement in DECODER_ALLOCATION_REGIONS:
        path = ROOT / requirement.path
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as error:
            failures.append(f"{requirement.path}: cannot read decoder source: {error}")
            continue
        start = text.find(requirement.start_marker)
        if start < 0:
            failures.append(f"{requirement.path}: missing marker {requirement.start_marker}")
            continue
        end = text.find(requirement.end_marker, start + len(requirement.start_marker))
        if end < 0:
            failures.append(f"{requirement.path}: missing marker {requirement.end_marker}")
            continue
        label = f"{requirement.path}:{line_number(text, start)}"
        failures.extend(
            check_decoder_region_text(label, text[start:end], requirement.required_patterns)
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


# The Pages deploy job needs exactly these two scopes for `actions/deploy-pages`:
# `pages` to create the deployment and `id-token` for its OIDC token. No other
# workflow, and no other scope, may write.
PAGES_WORKFLOW = ".github/workflows/pages.yml"
PAGES_DEPLOY_SCOPES = frozenset({"pages", "id-token"})


def check_workflow_text(path: str, text: str) -> list[str]:
    failures: list[str] = []
    action_pattern = re.compile(r"^\s*uses:\s*([^\s#]+)", re.MULTILINE)
    write_permission = re.compile(
        r"^\s*(?:permissions:\s*write-all|([A-Za-z0-9_-]+):\s*write)\s*$", re.MULTILINE
    )
    for match in write_permission.finditer(text):
        if path == PAGES_WORKFLOW and match.group(1) in PAGES_DEPLOY_SCOPES:
            continue
        failures.append(f"{path}:{line_number(text, match.start())}: write permission forbidden")
    for action in action_pattern.findall(text):
        if action.startswith("./"):
            continue
        _, separator, revision = action.rpartition("@")
        if not separator or re.fullmatch(r"[0-9a-f]{40}", revision) is None:
            failures.append(f"{path}: action must be pinned to a 40-character commit: {action}")
    failures.extend(check_contexts(path, text))
    return failures


def check_workflows() -> list[str]:
    failures: list[str] = []
    workflow_root = ROOT / ".github" / "workflows"
    for path in sorted(workflow_root.glob("*.yml")):
        relative = path.relative_to(ROOT).as_posix()
        failures.extend(check_workflow_text(relative, path.read_text(encoding="utf-8")))
    return failures


RESOLVER_NEGATIVES = (
    ("unsafe-block", "fn operation(){} fn f(){unsafe { operation(); }}", "unsafe"),
    ("unsafe-function", "unsafe fn operation() {}", "unsafe"),
    ("foreign-function", 'unsafe extern "C" { }', "foreign-code"),
    ("filesystem", 'fn f(){std::fs::read("x");}', "filesystem"),
    ("network", "fn f(){std::net::TcpStream::connect(\"x\");}", "network"),
    ("process", 'fn f(){std::process::Command::new("x");}', "process"),
    ("environment", 'fn f(){std::env::var("x");}', "environment"),
    ("wall-clock", "fn f(){std::time::SystemTime::now();}", "clock"),
    ("system-time", "use std::time::Instant; fn f(){Instant::now();}", "clock"),
    ("threads", "fn f(){std::thread::spawn(||0);}", "threads"),
    ("async-runtime", "fn f(){tokio::spawn(async {});}", "threads"),
    ("async-function", "async fn operation() {}", "threads"),
    ("randomness", "fn f(){rand::random::<u64>();}", "randomness"),
    ("interior-mutability", "use std::sync::Mutex; struct State; type X=Mutex<State>;", "shared-state"),
    ("floating-point", "fn f(){let value: f64 = 1.0;}", "floating-point"),
    ("mutable-static", "static mut STATE: u8 = 0;", "shared-state"),
    ("standard-io", "fn f(){std::io::stdin();}", "io"),
    ("grouped-std-import", "use std::{fs, io::Read};", "filesystem"),
    ("std-alias", "use std as platform; fn f(){platform::time::Instant::now();}", "clock"),
    ("thread-local", "thread_local! { static DEPTH: u8 = 0; }", "shared-state"),
    ("atomic-type", "use core::sync::atomic::AtomicU64; static COUNT: AtomicU64 = AtomicU64::new(0);", "shared-state"),
    ("cell-type", "use core::cell::RefCell; fn f(){let state = RefCell::new(0);}", "shared-state"),
    ("hash-ordered-collection", "use std::collections::HashMap; fn f(){let seen = HashMap::<u8,u8>::new();}", "hash-order"),
    ("nested-alias", "use std::{time::{Instant as I}}; use I as Clock; fn f(){Clock::now();}", "clock"),
    ("module-alias", "mod bridge{pub use std::env::var as read;} use bridge::read as get; fn f(){get(\"x\");}", "environment"),
    ("scope-shadow", "use std::time::Instant as Clock; mod pure{pub struct Clock;} fn f(){Clock::now();}", "clock"),
    ("type-alias", "type Counter=core::sync::atomic::AtomicU64; fn f(){Counter::new(0);}", "shared-state"),
    ("macro-argument", "use std::time::Instant as Clock; fn f(){let _=vec![Clock::now()];}", "clock"),
    ("unknown-expansion", "macro_rules! hidden{()=>{std::time::Instant::now()}} fn f(){hidden!();}", "resolution-macro"),
    ("external-glob", "use std::time::*; fn f(){Instant::now();}", "resolution-glob"),
    ("raw-address", "fn f(p:*const u8)->usize{p.addr()}", "address"),
)

RESOLVER_POSITIVES = (
    "#[derive(Clone)] struct State; fn transition(state: &State) -> State { state.clone() }",
    "use core::{cmp::Ordering, fmt::Write};",
    "use std::{collections::BTreeSet, vec::Vec};",
    "use alloc::collections::BTreeMap; struct WorkspaceCell; fn f(){let cells: BTreeMap<u32, WorkspaceCell> = BTreeMap::new();}",
    "enum Mode { Atomic, Staged }",
    "mod std{pub mod time{pub fn now()->u8{0}}} fn f()->u8{std::time::now()}",
    "fn f(random:u64)->u64{let time=random+1;time}",
    "fn f(){let _=stringify!(std::time::Instant::now());}",
)


RESOLVER_REPAIR_NEGATIVES = (
    ("inherited-user-builtin", "macro_rules! stringify { () => { 7u8 }; }\npub mod child { pub fn value()->u8 { stringify!() } }\n", "resolution-macro"),
    ("nested-user-builtin", "macro_rules! stringify { () => { 7u8 }; } mod outer { mod inner { pub fn value()->u8 { stringify!() } } }", "resolution-macro"),
    ("block-user-builtin", "pub fn value()->u8 { macro_rules! stringify { () => { 7u8 }; } { stringify!() } }", "resolution-macro"),
    ("exported-user-builtin", "pub fn value()->u8 { crate::stringify!() } mod definitions { #[macro_export] macro_rules! stringify { () => { 7u8 }; } }", "resolution-macro"),
    ("imported-user-builtin", "macro_rules! local_value { () => { 7u8 }; } pub(crate) use local_value as stringify; mod child { use super::stringify; pub fn value()->u8 { stringify!() } }", "resolution-macro"),
    ("qualified-attribute-prefix", "#![no_std]\n#![forbid(unsafe_code)]\n#[doc::noop]\npub fn value()->u8{7}\n", "resolution-attribute"),
    ("qualified-tool-unknown", "#[rustfmt::noop] pub fn value()->u8{7}", "resolution-attribute"),
    ("conditional-qualified-attribute", "#[cfg_attr(not(test), doc::noop)] pub fn value()->u8{7}", "resolution-attribute"),
    ("raw-const-borrow", "pub fn address(x: &u8) -> usize { (&raw const *x) as usize }\n", "address"),
    ("raw-mut-borrow", "pub fn address(x: &mut u8) -> usize { (&raw mut *x) as usize }\n", "address"),
    ("generic-alias-ambient-argument", "type Identity<T> = T; pub fn value(x:Identity<std::time::Instant>){let _=x;}", "clock"),
    ("generic-alias-shared-state", "type Identity<T> = T; pub fn value(x:Identity<core::sync::atomic::AtomicU64>){let _=x;}", "shared-state"),
    ("generic-alias-unknown", "type Identity<T> = Unknown<T>; pub fn value()->Identity<u8>{7}", "resolution-unresolved"),
)

RESOLVER_REPAIR_POSITIVES = (
    "pub fn value()->&'static str { stringify!(seven) } macro_rules! stringify { () => { 7u8 }; }",
    "mod child { pub fn value()->&'static str { stringify!(seven) } } macro_rules! stringify { () => { 7u8 }; }",
    "macro_rules! stringify { () => { 7u8 }; } pub fn value()->&'static str { core::stringify!(seven) }",
    "mod builtin { pub use core::stringify as show; } use builtin::show; pub fn value()->&'static str { show!(seven) }",
    "type Identity<T> = T; pub fn value()->Identity<u8> { 7 }\n",
    "type Identity<std> = std; pub fn value()->Identity<u8> { 7 }",
    "type Identity<T> = T; type Second<T> = Identity<T>; pub fn value()->Second<u8> { 7 }",
    "type Array<const N:usize> = [u8;N]; pub fn value()->Array<1>{[7]}",
    "#![no_std] #![forbid(unsafe_code)] #[doc=\"state\"] #[allow(dead_code)] #[derive(Clone, Copy)] #[repr(C)] #[non_exhaustive] pub struct State { value:u8 } #[rustfmt::skip] pub fn value()->u8{7}",
    "#[cfg_attr(test, doc::noop)] pub fn value()->u8{7}",
    "pub fn shared(x:&u8)->&u8{x} pub fn exclusive(x:&mut u8)->&mut u8{x}",
)


def check_resolver_self_test(checker: ResolvedChecker) -> list[str]:
    with tempfile.TemporaryDirectory(prefix="zeno-assurance-resolver-") as directory:
        base = Path(directory)
        expected = {}
        positives = set()
        paths = []
        for name, source, rule in RESOLVER_NEGATIVES + RESOLVER_REPAIR_NEGATIVES:
            path = base / f"negative-{name}.rs"
            path.write_text(source, encoding="utf-8")
            paths.append(path)
            expected[str(path)] = rule
        for index, source in enumerate(RESOLVER_POSITIVES + RESOLVER_REPAIR_POSITIVES):
            path = base / f"positive-{index}.rs"
            path.write_text(source, encoding="utf-8")
            paths.append(path)
            positives.add(str(path))
        try:
            report = checker.check(paths)
        except ResolutionError as error:
            return [f"semantic self-test could not run the actual resolver: {error}"]
        by_file = {str(path): [] for path in paths}
        for finding in report["findings"]:
            by_file.setdefault(finding["file"], []).append(finding)
        failures = [f"semantic self-test unreadable: {entry}" for entry in report["unreadable"]]
        for path, rule in expected.items():
            if not any(finding["rule"] == rule and
                       (finding["severity"] == "error" or rule in {"floating-point", "unstable-hash"})
                       for finding in by_file[path]):
                failures.append(f"semantic self-test missed {Path(path).stem}: required {rule}")
        for path in positives:
            if by_file[path]:
                failures.append(f"semantic self-test rejected {Path(path).stem}: {by_file[path]}")
        return failures


def check_resolver_package_self_test(checker: ResolvedChecker) -> list[str]:
    """An unsupported package closure must refuse without an established root."""
    fixtures = {
        "custom": {
            "Cargo.toml": "[package]\nname='custom_root_fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[lib]\npath='src/entry.rs'\n",
            "src/entry.rs": "#![forbid(unsafe_code)]\nmacro_rules! local_value { () => { 7u8 }; }\npub fn value()->u8 { local_value!() }\n",
        },
        "binary": {
            "Cargo.toml": "[package]\nname='bin_root_fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
            "src/bin/worker.rs": "macro_rules! local_value { () => { 7u8 }; }\nfn main(){ let _ = local_value!(); }\n",
        },
        "inactive": {
            "Cargo.toml": "[package]\nname='inactive_fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
            "src/lib.rs": "#![cfg(test)]\n#![no_std]\n#![forbid(unsafe_code)]\nunsupported!();\n",
        },
    }
    with tempfile.TemporaryDirectory(prefix="zeno-assurance-package-") as directory:
        base = Path(directory)
        for name, files in fixtures.items():
            for filename, source in files.items():
                path = base / name / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(source, encoding="utf-8")
        try:
            report = checker.check([base / name for name in fixtures])
        except ResolutionError as error:
            return [f"package self-test could not run the actual resolver: {error}"]
        failures = [f"package self-test unreadable: {entry}" for entry in report["unreadable"]]
        for name in ("custom", "binary"):
            manifest = str(base / name / "Cargo.toml")
            if not any(f["file"] == manifest and f["rule"] == "resolution-target" and
                       f["severity"] == "error" for f in report["findings"]):
                failures.append(f"package self-test omitted the {name} manifest target refusal")
        if any(f["file"].startswith(str(base / "inactive") + "/") for f in report["findings"]):
            failures.append("package self-test confused a known inactive root with an absent root")
        if not any(str(base / "inactive" / "src/lib.rs") in location
                   for location in report["resolution"]["skipped_cfg"]):
            failures.append("package self-test omitted custody of the recognized inactive root")
        if not semantic_report_failures(report):
            failures.append("package self-test unsupported closure became assurance success")
        return failures


RESOLVER_TOOL_NEGATIVES = (
    "mod rustfmt {} #[rustfmt::skip] pub fn value()->u8{7}",
    "use core as rustfmt; #[rustfmt::skip] pub fn value()->u8{7}",
    "extern crate core as rustfmt; #[rustfmt::skip] pub fn value()->u8{7}",
    "type rustfmt=u8; #[rustfmt::skip] pub fn value()->u8{7}",
    "fn value()->u8 {use core as rustfmt; #[rustfmt::skip] fn inner()->u8{7} inner()}",
    "mod bridge{pub use core as rustfmt;} use bridge::*; #[rustfmt::skip] pub fn value()->u8{7}",
    "#[cfg(feature=\"optional\")] use core as rustfmt; #[rustfmt::skip] pub fn value()->u8{7}",
    "use std::fmt::*; #[rustfmt::skip] pub fn value()->u8{7}",
)

RESOLVER_TOOL_POSITIVES = (
    "fn rustfmt(){} #[rustfmt::skip] pub fn value()->u8{7}",
    "macro_rules! rustfmt {()=>{7u8};} #[rustfmt::skip] pub fn value()->u8{7}",
    "#[cfg(test)] use core as rustfmt; #[rustfmt::skip] pub fn value()->u8{7}",
    "mod child{pub mod rustfmt{}} #[rustfmt::skip] pub fn value()->u8{7}",
)


def check_resolver_tool_self_test(checker: ResolvedChecker) -> list[str]:
    """Compiler-tool spelling cannot authorize a shadowed or unknown qualifier."""
    with tempfile.TemporaryDirectory(prefix="zeno-assurance-tool-") as directory:
        base = Path(directory)
        paths = []
        negatives = set()
        positives = set()
        for label, cases, expected in (("negative", RESOLVER_TOOL_NEGATIVES, negatives),
                                       ("positive", RESOLVER_TOOL_POSITIVES, positives)):
            for index, source in enumerate(cases):
                path = base / f"{label}-{index}.rs"
                path.write_text(source, encoding="utf-8")
                paths.append(path)
                expected.add(str(path))
        try:
            report = checker.check(paths)
        except ResolutionError as error:
            return [f"compiler-tool self-test could not run actual resolver: {error}"]
        by_file = {str(path): [] for path in paths}
        for finding in report["findings"]:
            by_file.setdefault(finding["file"], []).append(finding)
        failures = [f"compiler-tool self-test unreadable: {entry}" for entry in report["unreadable"]]
        for path in negatives:
            if not any(f["rule"] == "resolution-attribute" and f["severity"] == "error"
                       for f in by_file[path]):
                failures.append(f"compiler-tool self-test missed qualifier refusal: {path}")
        for path in positives:
            if by_file[path]:
                failures.append(f"compiler-tool self-test rejected true tool: {path}: {by_file[path]}")
        return failures


def check_resolver_tool_package_self_test(checker: ResolvedChecker) -> list[str]:
    """Declaring, renaming or importing a procedural namespace stays blocking."""
    package = "[package]\nname='tool_fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n"
    source = "#[rustfmt::skip] pub fn value()->u8{7}\n"
    cases = {
        "declared": ("[dependencies]\nrustfmt={path='stub'}\n", source),
        "dependency-table": ("[dependencies.rustfmt]\npath='stub'\n", source),
        "renamed": ("[dependencies]\nrustfmt={package='benign_formatter',path='stub'}\n", source),
        "target": ("[target.'cfg(not(unix))'.dependencies]\nrustfmt={path='stub'}\n", source),
        "imported": ("[dependencies]\nformatter={path='stub'}\n",
                     "use formatter as rustfmt; " + source),
        "genuine": ("", source),
    }
    with tempfile.TemporaryDirectory(prefix="zeno-assurance-tool-package-") as directory:
        base = Path(directory)
        for name, (declarations, body) in cases.items():
            path = base / name
            (path / "src").mkdir(parents=True)
            (path / "Cargo.toml").write_text(package + declarations, encoding="utf-8")
            (path / "src/lib.rs").write_text(body, encoding="utf-8")
        try:
            report = checker.check([base / name for name in cases])
        except ResolutionError as error:
            return [f"compiler-tool package self-test could not run actual resolver: {error}"]
        failures = [f"compiler-tool package self-test unreadable: {entry}" for entry in report["unreadable"]]
        for name in cases.keys() - {"genuine"}:
            source_path = str(base / name / "src/lib.rs")
            if not any(f["file"] == source_path and f["rule"] == "resolution-attribute"
                       and f["severity"] == "error" for f in report["findings"]):
                failures.append(f"compiler-tool package self-test omitted {name} refusal")
        if any(f["file"].startswith(str(base / "genuine") + "/") for f in report["findings"]):
            failures.append("compiler-tool package self-test rejected genuine tool context")
        if not semantic_report_failures(report):
            failures.append("compiler-tool package refusal became assurance success")
        return failures


def check_resolver_relative_self_test(checker: ResolvedChecker) -> list[str]:
    """The real process cwd cannot hide an enclosing compiler namespace."""
    if checker.binary is None:
        return ["relative-path self-test requires the actual built resolver"]
    package = "[package]\nname='relative_fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n"
    source = "#[rustfmt::skip] pub fn value()->u8{7}\n"
    failures = []
    with tempfile.TemporaryDirectory(prefix="zeno-assurance-relative-") as directory:
        base = Path(directory)
        for name, manifest in (("collision", package + "[dependencies]\nrustfmt={path='stub'}\n"),
                               ("genuine", package), ("unknown", "[workspace]\n"),
                               ("unreadable", None)):
            path = base / name
            (path / "src").mkdir(parents=True)
            (path / "src/lib.rs").write_text(source, encoding="utf-8")
            if manifest is None:
                (path / "Cargo.toml").mkdir()
            else:
                (path / "Cargo.toml").write_text(manifest, encoding="utf-8")
        standalone = base / "standalone"
        standalone.mkdir()
        (standalone / "lib.rs").write_text(source, encoding="utf-8")
        paths = ("lib.rs", "./lib.rs", "../src/lib.rs", ".", "./", "../src", "..")
        cases = []
        for name in ("collision", "genuine", "unknown", "unreadable"):
            cwd = base / name / "src"
            for argument in paths + (str(cwd / "lib.rs"),):
                cases.append((name, cwd, argument, name != "genuine"))
        for argument in ("lib.rs", "./lib.rs", ".", "./", str(standalone / "lib.rs")):
            cases.append(("standalone", standalone, argument, False))
        cases.append(("escape-collision", base / "collision/src", "../../standalone/lib.rs", False))
        removed = base / "removed"
        removed.mkdir()
        wrapper = "import os,sys; os.chdir(sys.argv[2]); os.rmdir(sys.argv[2]); os.execv(sys.argv[1], [sys.argv[1],sys.argv[3]])"
        for label, cwd, argument, negative in cases + [("removed-cwd", base, str(standalone / "lib.rs"), True)]:
            command = ([sys.executable, "-B", "-c", wrapper, str(checker.binary), str(removed), argument]
                       if label == "removed-cwd" else [str(checker.binary), argument])
            try:
                result = subprocess.run(command, cwd=cwd, env=checker.environment, text=True,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
                report = json.loads(result.stdout)
                semantic = semantic_report_failures(report)
                blocked = result.returncode in (1, 2) and bool(semantic) and any(
                    finding["rule"] == "resolution-attribute" and finding["severity"] == "error"
                    for finding in report["findings"])
                positive = result.returncode == 0 and not semantic and not report["findings"] and not report["unreadable"]
                if report.get("schema") != "zeno-fcis/purity-report/2" or not (blocked if negative else positive):
                    failures.append(f"relative-path self-test {label}/{argument}: unexpected actual result {report}")
            except (OSError, ValueError, KeyError, TypeError) as error:
                failures.append(f"relative-path self-test {label}/{argument}: actual resolver unavailable: {error}")
    return failures


def run_self_test(checker: ResolvedChecker | None = None) -> list[str]:
    failures: list[str] = []
    if checker is not None:
        failures.extend(check_resolver_self_test(checker))
        failures.extend(check_resolver_package_self_test(checker))
        failures.extend(check_resolver_tool_self_test(checker))
        failures.extend(check_resolver_tool_package_self_test(checker))
        failures.extend(check_resolver_relative_self_test(checker))
    else:
        failures.append("semantic self-test requires the actual built resolver")

    guarded = "Vec::with_capacity(initial_collection_capacity(count, cursor.remaining(), 1)?)"
    requirement = (
        r"Vec::with_capacity\(initial_collection_capacity\(count,cursor\.remaining\(\),1,?\)\?\)",
    )
    guarded_failures = check_decoder_region_text("self-test", guarded, requirement)
    if guarded_failures:
        failures.append("decoder allocation self-test rejected its guarded witness")
    mutant = "Vec::with_capacity(count)"
    if not check_decoder_region_text("self-test-mutant", mutant, requirement):
        failures.append("decoder allocation self-test accepted a raw-count mutant")

    deploy_scopes = "permissions:\n  pages: write\n  id-token: write\n"
    if check_workflow_text(PAGES_WORKFLOW, deploy_scopes):
        failures.append("workflow self-test rejected the Pages deploy scopes in pages.yml")
    for witness in (deploy_scopes + "  contents: write\n", "permissions: write-all\n"):
        if not check_workflow_text(PAGES_WORKFLOW, witness):
            failures.append("workflow self-test accepted a write scope beyond the Pages deploy scopes")
    if not check_workflow_text(".github/workflows/ci.yml", deploy_scopes):
        failures.append("workflow self-test accepted the Pages deploy scopes outside pages.yml")
    if not check_workflow_text(PAGES_WORKFLOW, "        uses: actions/deploy-pages@v5\n"):
        failures.append("workflow self-test accepted an action pinned to a tag")
    failures.extend(check_workflow_context_self_test())
    return failures


# GitHub rejects a whole workflow file, so none of its jobs run, when a key
# names a context it does not allow. Each witness pairs a refused form with the
# accepted form that carries the same value.
WORKFLOW_CONTEXT_WITNESSES = (
    ("job-level env reads runner",
     "jobs:\n  gate:\n    env:\n      OUT: ${{ runner.temp }}/out\n    steps:\n      - run: true\n",
     "jobs:\n  gate:\n    steps:\n      - env:\n          OUT: ${{ runner.temp }}/out\n        run: true\n"),
    ("job condition reads steps",
     "jobs:\n  gate:\n    if: steps.a.outcome == 'success'\n",
     "jobs:\n  gate:\n    steps:\n      - if: steps.a.outcome == 'success'\n        run: true\n"),
    ("workflow env reads matrix",
     "env:\n  GROUP: ${{ matrix.group }}\n",
     "jobs:\n  gate:\n    env:\n      GROUP: ${{ matrix.group }}\n"),
    ("job name calls hashFiles",
     "jobs:\n  gate:\n    name: ${{ hashFiles('Cargo.lock') }}\n",
     "jobs:\n  gate:\n    steps:\n      - name: ${{ hashFiles('Cargo.lock') }}\n        run: true\n"),
    ("action reference is an expression",
     "jobs:\n  gate:\n    steps:\n      - uses: ${{ github.action }}\n",
     "jobs:\n  gate:\n    steps:\n      - with:\n          ref: ${{ github.sha }}\n        uses: ./local\n"),
    ("run block comment reads an unknown context",
     "jobs:\n  gate:\n    steps:\n      - run: |\n          true\n          # ${{ unknown.value }}\n",
     "jobs:\n  gate:\n    steps:\n      - run: |\n          true\n          # ${{ runner.temp }}\n"),
)


def check_workflow_context_self_test() -> list[str]:
    failures: list[str] = []
    for name, refused, accepted in WORKFLOW_CONTEXT_WITNESSES:
        if not check_contexts("self-test.yml", refused):
            failures.append(f"workflow context self-test accepted: {name}")
        if check_contexts("self-test.yml", accepted):
            failures.append(f"workflow context self-test refused the valid form of: {name}")
    return failures


def main() -> int:
    args = parse_args()
    print("assurance scope: declared package namespaces or dependency-free standalone default extern prelude; "
          "undeclared --extern, RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS and .cargo compiler namespace overrides are excluded")
    failures: list[str] = []
    checker = ResolvedChecker()
    try:
        checker.prepare()
    except ResolutionError as error:
        failures.append(f"actual resolved checker unavailable: {error}")
        checker = None
    if args.self_test:
        failures.extend(run_self_test(checker))

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

    if checker is not None:
        failures.extend(check_semantic_sources(checker))
    else:
        failures.append("semantic source inspection not performed: actual resolver unavailable")
    failures.extend(check_decoder_allocation_guards())
    failures.extend(check_workflows())

    if failures:
        for failure in failures:
            print(f"assurance: {failure}", file=sys.stderr)
        print(f"assurance: FAILED ({len(failures)} finding(s))", file=sys.stderr)
        return 1

    mode = "self-test + repository" if args.self_test else "repository"
    print(
        f"assurance: PASS ({mode}; {len(members)} crates; "
        f"{len(SEMANTIC_CRATES)} semantic boundaries; scoped resolved-use checker)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
