#!/usr/bin/env python3
"""Check the shared runtime source with pinned Verus and ordinary Rust.

Use --install to download the pinned Linux x86-64 verifier into a user cache.
The two Rust toolchains must already be installed. Receipts are tool evidence,
not an authorization certificate or a Lean-kernel proof.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
from urllib.request import urlopen
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PIN = Path("verification/verus/toolchain.json")
HARNESS = Path("verification/verus/domain_bounds.rs")
SUBJECT = Path("verification/kernel-laws/src/oracle/authority/finite_bounds.rs")
SOURCES = (PIN, HARNESS, SUBJECT, Path("tools/check_verus.py"), Path("Cargo.toml"),
           Path("verification/kernel-laws/src/oracle/authority/mod.rs"),
           Path("verification/kernel-laws/src/oracle/authority/finite_decision.rs"))


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def snapshot() -> dict[str, str]:
    return {str(path): digest(ROOT / path) for path in SOURCES}


def verify_tool_files(directory: Path, pin: dict) -> None:
    for name, expected in pin["files"].items():
        file = directory / name
        if file.is_symlink() or not file.is_file() or digest(file) != expected:
            raise RuntimeError(f"pinned Verus file missing or changed: {file}")


def prepare_tools(cache: Path, pin: dict, install: bool) -> Path:
    base = cache / pin["version"]
    directory = base / "qualified"
    if not directory.exists() and install:
        base.mkdir(parents=True, exist_ok=True)
        archive = base / "verus-x86-linux.zip"
        if not archive.exists():
            with tempfile.NamedTemporaryFile(dir=base, delete=False) as output:
                partial = Path(output.name)
                try:
                    total = 0
                    with urlopen(pin["archive_url"], timeout=60) as response:
                        while chunk := response.read(1024 * 1024):
                            total += len(chunk)
                            if total > pin["archive_bytes"]:
                                raise RuntimeError("Verus download exceeded pinned size")
                            output.write(chunk)
                    output.flush()
                    if total != pin["archive_bytes"] or digest(partial) != pin["archive_sha256"]:
                        raise RuntimeError("Verus download size or digest mismatch")
                    partial.replace(archive)
                finally:
                    partial.unlink(missing_ok=True)
        if archive.stat().st_size != pin["archive_bytes"] or digest(archive) != pin["archive_sha256"]:
            raise RuntimeError("cached Verus archive differs from pin")
        # Only extract the explicitly qualified runtime files, never archive paths.
        with tempfile.TemporaryDirectory(dir=base) as temporary:
            extracted = Path(temporary)
            with zipfile.ZipFile(archive) as package:
                for name in pin["files"]:
                    member = f"{pin['archive_directory']}/{name}"
                    info = package.getinfo(member)
                    target = extracted / name
                    with package.open(info) as source, target.open("wb") as output:
                        shutil.copyfileobj(source, output)
                    target.chmod(0o755 if name in ("verus", "rust_verify", "z3") else 0o644)
            verify_tool_files(extracted, pin)
            extracted.rename(directory)
            # TemporaryDirectory tolerates its path being moved.
    verify_tool_files(directory, pin)
    return directory


def accepted(report: dict, pin: dict) -> bool:
    if not isinstance(report, dict):
        return False
    result = report.get("verification-results", {})
    verifier = report.get("verus", {})
    functions = report.get("func-details", {})
    if not all(isinstance(value, dict) for value in (result, verifier, functions)):
        return False
    return (result.get("success") is True
            and type(result.get("errors")) is int
            and result.get("errors") == 0
            and type(result.get("verified")) is int
            and result.get("verified") == pin["expected_verified"]
            and result.get("encountered-error") is False
            and result.get("encountered-vir-error") is False
            and result.get("is-verifying-entire-crate") is True
            and verifier.get("commit") == pin["commit"]
            and verifier.get("version") == pin["version"]
            and set(pin["target_functions"]).issubset(functions))


def run(command: list[str], cwd: Path, environment: dict, timeout: int = 120) -> subprocess.CompletedProcess:
    return subprocess.run(command, cwd=cwd, env=environment, text=True,
                          capture_output=True, timeout=timeout, check=False)


def require_success(result: subprocess.CompletedProcess) -> None:
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {result.args}\n"
                           f"{result.stdout[-2500:]}\n{result.stderr[-2500:]}")


def mutation_sources(source: str) -> dict[str, tuple[str, str]]:
    width_line = "let width = (max as i128) - (min as i128) + 1;"
    limit_line = "if product > (limit as u128) {"
    if source.count(width_line) != 1 or source.count(limit_line) != 1:
        raise RuntimeError("mutation anchors changed; review the mutation suite")
    start = source.index("#[cfg_attr(verus_keep_ghost, verus_spec(result =>")
    end = source.index("pub(crate) fn interval_width", start)
    return {
        "wrong_interval_width": (source.replace(width_line, width_line.replace("+ 1", "+ 2")), "proof"),
        "refuse_exact_limit": (source.replace(limit_line, limit_line.replace(">", ">=")), "proof"),
        "bypass_product_limit": (source.replace(limit_line, "if false {"), "proof"),
        "omit_interval_contract": (source[:start] + source[end:], "coverage"),
    }


def check(cache: Path, install: bool) -> dict:
    pin = json.loads((ROOT / PIN).read_text())
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("this qualified profile requires Linux x86-64")
    before = snapshot()
    tools = prepare_tools(cache, pin, install)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("VERUS_", "VARGO_"))
                   and key not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    environment["RUSTUP_TOOLCHAIN"] = pin["rust_toolchain"]
    environment["VERUS_Z3_PATH"] = str(tools / "z3")
    command = [str(tools / "verus"), "--crate-type=lib", "--edition=2024",
               "--no-cheating", "--output-json", str(HARNESS)]
    positive = run(command, ROOT, environment)
    require_success(positive)
    report = json.loads(positive.stdout)
    if not accepted(report, pin):
        raise RuntimeError("Verus success lacks the pinned version or expected proof coverage")
    mutations = []
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-verus-") as temporary:
        directory = Path(temporary)
        executable = directory / "domain-bounds-tests"
        native = ["rustc", f"+{pin['runtime_rust']}", "--edition=2024", "--test",
                  "--check-cfg", "cfg(verus_keep_ghost)", "--check-cfg", "cfg(test)",
                  str(HARNESS), "-o", str(executable)]
        require_success(run(native, ROOT, environment))
        native_tests = run([str(executable)], ROOT, environment)
        require_success(native_tests)
        for name, (changed, expected) in mutation_sources((ROOT / SUBJECT).read_text()).items():
            specimen = directory / name
            (specimen / HARNESS).parent.mkdir(parents=True)
            (specimen / SUBJECT).parent.mkdir(parents=True)
            (specimen / HARNESS).write_bytes((ROOT / HARNESS).read_bytes())
            (specimen / SUBJECT).write_text(changed)
            outcome = run(command, specimen, environment)
            mutant_report = json.loads(outcome.stdout)
            result = mutant_report.get("verification-results", {})
            if expected == "proof":
                killed = (outcome.returncode != 0 and result.get("success") is False
                          and result.get("errors", 0) > 0
                          and result.get("encountered-vir-error") is False)
            else:
                killed = (outcome.returncode == 0 and result.get("success") is True
                          and not accepted(mutant_report, pin))
            mutations.append({"name": name, "expected_failure": expected,
                              "killed": killed, "exit_code": outcome.returncode,
                              "verification_results": result})
            if not killed:
                raise RuntimeError(f"mutation {name} survived or failed for an unrelated reason")
    versions = {}
    for name, version_command in {
        "runtime_rust": ["rustc", f"+{pin['runtime_rust']}", "--version"],
        "verifier_rust": ["rustc", f"+{pin['rust_toolchain']}", "--version"],
        "z3": [str(tools / "z3"), "--version"],
    }.items():
        result = run(version_command, ROOT, environment)
        require_success(result)
        versions[name] = result.stdout.strip()
    verify_tool_files(tools, pin)
    if before != snapshot():
        raise RuntimeError("verification source changed during the check")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
    return {"schema": "zeno-fcis/verus-evidence/1", "status": "passed",
            "revision": revision, "working_tree_dirty": bool(dirty),
            "source_sha256": before, "toolchain": pin, "versions": versions,
            "command": command, "exit_code": positive.returncode, "verus_report": report,
            "native_test_output": native_tests.stdout.strip(), "mutations": mutations,
            "scope": "interval_width and bounded_product over all declared machine inputs",
            "trusted_base": ["Verus and its translation", "bundled vstd and Z3",
                             "Rust compilers and erasure", "host platform"],
            "unproved": ["enumeration coverage", "complete decision interpreter",
                         "law and genesis evaluation", "shell and external effects"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/zeno-fcis/verus")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        receipt = check(args.cache.resolve(), args.install)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        receipt = {"schema": "zeno-fcis/verus-evidence/1", "status": "failed", "error": str(error)}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"Verus: {receipt['status']}; receipt: {args.out}")
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    print("2 runtime functions verified; 3 obligations; 4 mutations caught; Rust boundary tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
