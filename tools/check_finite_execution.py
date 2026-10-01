#!/usr/bin/env python3
"""Verify the shared finite admission/evaluator and reject missing coverage.

Verus checks the actual source used by synthesis and the finite decision route.
The receipt is scoped Verus/Z3 evidence, not an authorization or a V2 release.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import zipfile

import check_verus as verifier
from verus_coverage import require_coverage

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/finite-execution.json")
HARNESS = Path("verification/verus/finite_execution.rs")
SUBJECT = Path("crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs")
ADMISSION = SUBJECT.parent / "admission/mod.rs"
UNIT_SOURCES = (HARNESS, SUBJECT, SUBJECT.parent / "spec.rs", ADMISSION,
                ADMISSION.parent / "spec.rs")
SOURCES = (*UNIT_SOURCES, PROFILE, verifier.PIN,
           Path("verification/verus/finite_execution_tests.rs"),
           Path("verification/verus/admission_baseline.rs"),
           Path("verification/verus/admission_differential_tests.rs"),
           Path("tools/check_finite_execution.py"), Path("tools/verus_coverage.py"),
           Path("tools/check_verus.py"), Path("tools/test_check_finite_execution.py"),
           Path("Cargo.toml"), Path("rust-toolchain.toml"),
           Path("crates/zeno-fcis-synthesis/src/finite/ir.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite/mod.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite_runtime.rs"))


def snapshot() -> dict[str, str]:
    for path in SOURCES:
        if (ROOT / path).is_symlink():
            raise RuntimeError(f"proof source is a symbolic link: {path}")
    return {str(path): verifier.digest(ROOT / path) for path in SOURCES}


def once(source: str, before: str, after: str) -> str:
    if source.count(before) != 1:
        raise RuntimeError(f"mutation anchor changed: {before}")
    return source.replace(before, after)


def final_contract(source: str, replacement: str) -> str:
    end = source.index("pub(super) fn evaluate_into(")
    start = source.rindex("#[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    return source[:start] + replacement + source[end:]


def mutation_sources(execution: str, admission: str) -> dict[str, tuple[Path, str, str]]:
    mutations = {}
    for name, before, after in (
        ("wrong_boolean_encoding", "Op::Bool(value) => Ok(value as i64)", "Op::Bool(value) => Ok(1 - value as i64)"),
        ("subtract_instead_of_add", "left.checked_add(right)", "left.checked_sub(right)"),
        ("invert_equality", "Ok((left == right) as i64)", "Ok((left != right) as i64)"),
        ("swap_select_arms", "if condition == 1 { yes } else { no }", "if condition == 1 { no } else { yes }"),
        ("skip_last_eager_node", "while index < nodes.len() {", "while index + 1 < nodes.len() {"),
        ("wrong_output_projection", "output.push(value);", "output.push(0);"),
        ("bypass_input_domain", "if !admitted(inputs, input) {", "if false {"),
        ("bypass_output_domain", "if !admitted(outputs, output) {", "if false {"),
        ("expose_partial_results", "if result.is_err() {", "if false {"),
    ):
        mutations[name] = (SUBJECT, once(execution, before, after), "proof")
    for name, before, after in (
        ("accept_wrong_operand_kind", "if actual == expected {", "if true {"),
        ("admit_excess_node_count", "|| nodes > MAX_NODES", "|| false"),
        ("accept_wrong_root_kind", "if root >= kinds.len() || kinds[root] != outputs[index].boolean() {",
         "if root >= kinds.len() {"),
    ):
        mutations[name] = (ADMISSION, once(admission, before, after), "proof")
    for name, replacement in (
        ("omit_execution_contract", ""),
        ("weaken_execution_contract", "#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n"),
        ("narrow_execution_domain", final_contract_attribute(execution)),
    ):
        mutations[name] = (SUBJECT, final_contract(execution, replacement), "coverage")
    mutations["add_uncontracted_function"] = (
        SUBJECT, execution + "\nfn unchecked_identity(value: i64) -> i64 { value }\n", "coverage")
    return mutations


def final_contract_attribute(source: str) -> str:
    end = source.index("pub(super) fn evaluate_into(")
    start = source.rindex("#[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    return once(source[start:end], "    ensures match result {",
                "    requires input@.len() == 0,\n    ensures match result {")


def check(cache: Path, install: bool) -> dict:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("this proof profile requires qualified Linux x86-64 tools")
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    report_pin = {**pin, "expected_verified": profile["expected_verified"],
                  "target_functions": profile["target_functions"]}
    before = snapshot()
    tools = verifier.prepare_tools(cache, pin, install)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("VERUS_", "VARGO_"))
                   and key not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    environment["RUSTUP_TOOLCHAIN"] = pin["rust_toolchain"]
    environment["VERUS_Z3_PATH"] = str(tools / "z3")
    command = [str(tools / "verus"), "--crate-type=lib", "--edition=2024",
               "--no-cheating", "--no-external-by-default", "--num-threads", "2", "--output-json",
               "--log", "vir", "--log", "vir-option=no_span+no_type+no_fn_details"]
    mutations = []
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-finite-proof-") as temporary:
        directory = Path(temporary)
        positive_command = [*command, "--log-dir", str(directory / "positive"), str(HARNESS)]
        positive = verifier.run(positive_command, ROOT, environment)
        verifier.require_success(positive)
        report = json.loads(positive.stdout)
        if not verifier.accepted(report, report_pin):
            raise RuntimeError("finite proof report lacks pinned tool or complete obligation coverage")
        coverage = require_coverage((directory / "positive/crate.vir").read_text(), profile)
        executable = directory / "finite-tests"
        native_command = ["rustc", f"+{pin['runtime_rust']}", "--edition=2024", "--test",
                          "--check-cfg", "cfg(verus_keep_ghost)", "--check-cfg", "cfg(test)",
                          str(HARNESS), "-o", str(executable)]
        verifier.require_success(verifier.run(native_command, ROOT, environment))
        native = verifier.run([str(executable)], ROOT, environment)
        verifier.require_success(native)
        print(f"Finite execution: {profile['expected_verified']} obligations and native boundaries passed", flush=True)
        for name, (path, changed, expected) in mutation_sources(
                (ROOT / SUBJECT).read_text(), (ROOT / ADMISSION).read_text()).items():
            specimen = directory / name
            for unit in UNIT_SOURCES:
                target = specimen / unit
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(changed) if unit == path else target.write_bytes((ROOT / unit).read_bytes())
            mutant = verifier.run([*command, "--log-dir", str(specimen / "logs"), str(HARNESS)],
                                  specimen, environment)
            mutant_report = json.loads(mutant.stdout)
            result = mutant_report.get("verification-results", {})
            coverage_error = None
            if expected == "proof":
                killed = (mutant.returncode != 0 and result.get("success") is False
                          and type(result.get("errors")) is int and result["errors"] > 0
                          and result.get("encountered-vir-error") is False)
            else:
                try:
                    require_coverage((specimen / "logs/crate.vir").read_text(), profile)
                except ValueError as error:
                    coverage_error = str(error)
                killed = (mutant.returncode == 0 and result.get("success") is True
                          and result.get("errors") == 0 and coverage_error is not None)
            mutations.append({"name": name, "expected_failure": expected, "killed": killed,
                              "exit_code": mutant.returncode, "verification_results": result,
                              "coverage_refusal": coverage_error})
            if not killed:
                raise RuntimeError(f"mutation {name} survived or failed for an unrelated reason")
            print(f"Finite execution: caught {name} ({expected})", flush=True)
    versions = {}
    for name, version_command in {
        "runtime_rust": ["rustc", f"+{pin['runtime_rust']}", "--version"],
        "verifier_rust": ["rustc", f"+{pin['rust_toolchain']}", "--version"],
        "z3": [str(tools / "z3"), "--version"],
    }.items():
        version = verifier.run(version_command, ROOT, environment)
        verifier.require_success(version)
        versions[name] = version.stdout.strip()
    verifier.verify_tool_files(tools, pin)
    if before != snapshot():
        raise RuntimeError("proof or production integration source changed during verification")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
    return {
        "schema": "zeno-fcis/finite-execution-evidence/1", "status": "passed",
        "revision": revision, "working_tree_dirty": bool(dirty), "source_sha256": before,
        "toolchain": pin, "versions": versions, "command": positive_command,
        "exit_code": positive.returncode,
        "verus_report": report, "translated_function_coverage": coverage,
        "native_test_output": native.stdout.strip(), "mutations": mutations,
        "scope": "all eager finite scalar instructions, structural admission, domains, projection and refusal cleanup",
        "trusted_base": ["Verus translation and erasure", "bundled vstd and Z3",
                         "Rust compilers", "standard library and allocation", "host platform",
                         "reviewed mathematical specifications and coverage manifest"],
        "unproved": ["canonical codec and schema projection", "production diagnostic wrappers",
                     "complete decision construction", "mandatory V2 meter and state view",
                     "law and genesis evaluation", "authority composition", "shell and effects"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/zeno-fcis/verus")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        receipt = check(args.cache.resolve(), args.install)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        receipt = {"schema": "zeno-fcis/finite-execution-evidence/1", "status": "failed", "error": str(error)}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"Finite execution: {receipt['status']}; receipt: {args.out}")
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    print(f"20 runtime bodies; 34 obligations; {len(receipt['mutations'])} mutations caught")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
