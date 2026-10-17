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
import re
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

SEMANTIC_FAILURES = frozenset((
    'assertion failed', 'postcondition not satisfied', 'precondition not satisfied',
    'requires not satisfied', 'invariant not satisfied at end of loop body',
    'loop invariant not satisfied', 'invariant not satisfied',
    'invariant not satisfied before loop',
    'possible arithmetic underflow/overflow',
))
ERROR_HEADER = re.compile(r'^error(?:\[[A-Za-z0-9]+\])?: (.*)$', re.M)
NON_SEMANTIC = re.compile(
    r'\bunknown\b|resource limit|\brlimit\b|timed? out|\btimeout\b|out of memory|memory exhausted|'
    r'\bkilled\b|\bpanic(?:ked)?\b|internal compiler error|segmentation fault|'
    r'\bsignal\b|transport error|failed to (?:start|execute)', re.I)


def function_lines(source: str, name: str) -> tuple[int, int]:
    """Exact annotated function region in a retained native-source specimen."""
    functions = list(re.finditer(r'\bfn\s+([A-Za-z_][A-Za-z_0-9]*)\b', source))
    matches = [i for i, match in enumerate(functions) if match[1] == name]
    if len(matches) != 1:
        raise ValueError(f'not one intended function: {name}')
    index = matches[0]
    contracts = list(re.finditer(r'#\[\s*cfg_attr\(\s*verus_keep_ghost\s*,\s*verus_spec\(', source))
    previous = functions[index - 1].end() if index else 0
    preceding = [match.start() for match in contracts
                 if previous <= match.start() < functions[index].start()]
    start = preceding[-1] if preceding else -1
    end = len(source)
    if index + 1 < len(functions):
        end = functions[index + 1].start()
        following = [match.start() for match in contracts
                     if functions[index].end() < match.start() < end]
        if following:
            end = following[-1]
    if start < 0:
        body = source[functions[index].end():end]
        if re.search(r'\bverus_proof_expr!\s*\(\s*ensures\s*\(', body) is None:
            raise ValueError(f'intended function lacks an annotated contract: {name}')
        start = functions[index].start()
    return source.count('\n', 0, start) + 1, source.count('\n', 0, end)


def semantic_refusal(proc, report: dict, pin: dict, path: Path, cwd: Path,
                     *, region: tuple[int, int] | None = None,
                     target_functions=()) -> bool:
    """Require a complete pinned run and only intended semantic error blocks."""
    if not isinstance(report, dict):
        return False
    result, tool, functions = (report.get(k, {}) for k in
                              ('verification-results', 'verus', 'func-details'))
    if not all(isinstance(value, dict) for value in (result, tool, functions)):
        return False
    if (proc.returncode != 1 or tool.get('version') != pin['version']
            or tool.get('commit') != pin['commit']
            or str(tool.get('toolchain', '')).split(' ')[0] != pin['rust_toolchain']
            or result.get('success') is not False
            or result.get('encountered-error') is not True
            or result.get('encountered-vir-error') is not False
            or result.get('is-verifying-entire-crate') is not True
            or type(result.get('errors')) is not int or result['errors'] <= 0
            or type(result.get('verified')) is not int or result['verified'] <= 0
            or not set(target_functions).issubset(functions)
            or NON_SEMANTIC.search(proc.stderr)):
        return False
    headers = list(ERROR_HEADER.finditer(proc.stderr))
    terminal, count = None, 0
    expected_path = os.path.normpath(str(cwd / path))
    for index, header in enumerate(headers):
        summary = re.fullmatch(r'aborting due to (\d+) previous errors?', header[1])
        if summary:
            if terminal is not None or index != len(headers) - 1:
                return False
            terminal = int(summary[1])
            continue
        if header[1] not in SEMANTIC_FAILURES or not header[0].startswith('error: '):
            return False
        end = headers[index + 1].start() if index + 1 < len(headers) else len(proc.stderr)
        primary = re.search(r'^\s*-->\s+(.+):(\d+):(\d+)\s*$',
                            proc.stderr[header.end():end], re.M)
        if primary is None:
            return False
        observed = Path(primary[1])
        if not observed.is_absolute():
            observed = cwd / observed
        line = int(primary[2])
        if (os.path.normpath(str(observed)) != expected_path
                or (region is not None and not region[0] <= line <= region[1])):
            return False
        count += 1
    # One failed query may produce several intended diagnostics in this pin.
    return bool(count and terminal == count and result['errors'] <= count)


def proof_succeeded(report: dict, pin: dict) -> bool:
    """Complete pinned success; a coverage fault may change inventory/counts."""
    if not isinstance(report, dict):
        return False
    result, tool, functions = (report.get(k, {}) for k in
                              ('verification-results', 'verus', 'func-details'))
    return (all(isinstance(value, dict) for value in (result, tool, functions))
            and tool.get('version') == pin['version']
            and tool.get('commit') == pin['commit']
            and str(tool.get('toolchain', '')).split(' ')[0] == pin['rust_toolchain']
            and result.get('success') is True
            and type(result.get('errors')) is int and result['errors'] == 0
            and type(result.get('verified')) is int and result['verified'] > 0
            and result.get('encountered-error') is False
            and result.get('encountered-vir-error') is False
            and result.get('is-verifying-entire-crate') is True)


def proof_threads(default="2", environment=None) -> str:
    """Preserve each gate's default; an assigned lane may select one to six workers."""
    settings = os.environ if environment is None else environment
    value = settings.get('ZENO_FCIS_VERUS_THREADS', default)
    if value not in ('1', '2', '3', '4', '5', '6'):
        raise ValueError('proof worker count must be an integer from one to six')
    return value


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


def cache_directory() -> Path:
    """Locate installed tools; every selected file still requires its pinned hash."""
    configured = os.environ.get("ZENO_FCIS_VERUS_CACHE")
    return Path(configured) if configured else Path.home() / ".cache/zeno-fcis/verus"


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


def run(command: list[str], cwd: Path, environment: dict, timeout: int = 600) -> subprocess.CompletedProcess:
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
    parser.add_argument("--cache", type=Path, default=cache_directory())
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
