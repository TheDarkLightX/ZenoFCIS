#!/usr/bin/env python3
"""Check owned V2 instruction metering; this is not transition authorization.

Verus proves exact outcomes/counters. Reviewed executable-body fingerprints
separately guard operational charge order, which those postconditions alone
cannot prove. Neither result closes the raw-state or mandatory-authority bridge.
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

from v2_proof_sources import execution_sources, adjust_specimen_source_lengths
from v2_native_dependencies import native_dependency_args
import check_verus as verifier
from check_finite_execution import UNIT_SOURCES as SCALAR_SOURCES, once
from verus_coverage import require_coverage

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/metered-execution.json")
HARNESS = Path("verification/verus/metered_execution.rs")
SUBJECT = Path("crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs")
METER, SPEC = SUBJECT.parent / "meter.rs", SUBJECT.parent / "spec.rs"
RESOURCE = Path("crates/zeno-fcis-core/src/resource.rs")
INPUT = SUBJECT.parent / "input_view.rs"
INPUT_SPEC = SUBJECT.parent / "input_view/spec.rs"
CANONICAL = SUBJECT.parent.parent / "canonical_v2/mod.rs"
CANONICAL_SPEC = CANONICAL.parent / "spec.rs"
UNIT_SOURCES = (HARNESS, RESOURCE, *SCALAR_SOURCES[1:], SUBJECT, METER, SPEC,
                INPUT, INPUT_SPEC, CANONICAL, CANONICAL_SPEC)

UNIT_SOURCES = tuple(dict.fromkeys((*UNIT_SOURCES, *execution_sources(ROOT))))
SOURCES = (*UNIT_SOURCES, PROFILE, verifier.PIN, SUBJECT.parent / "tests.rs",
           SUBJECT.parent / "input_view/tests.rs", SUBJECT.parent / "composition/record_tests.rs",
           Path("tools/check_metered_execution.py"), Path("tools/test_check_metered_execution.py"),
           Path("tools/check_finite_execution.py"), Path("tools/check_verus.py"),
           Path("tools/verus_coverage.py"), Path("Cargo.toml"), Path("rust-toolchain.toml"),
           Path("crates/zeno-fcis-synthesis/src/finite/ir.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite/mod.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite_runtime.rs"),
           Path("crates/zeno-fcis-synthesis/tests/v2_execution.rs"))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_proof_sources.py"),
                                Path("verification/verus/authority_v2_sources.json"))))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_native_dependencies.py"))))


def snapshot() -> dict[str, str]:
    for path in SOURCES:
        if (ROOT / path).is_symlink():
            raise RuntimeError(f"proof source is a symbolic link: {path}")
    return {str(path): verifier.digest(ROOT / path) for path in SOURCES}


def entry_contract(source: str, replacement: str) -> str:
    end = source.index("pub fn execute(")
    start = source.rindex("#[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    return source[:start] + replacement + source[end:]


def mutation_sources(execution: str, meter: str, spec: str, resource: str) -> dict[str, tuple[Path, str, str]]:
    mutations = {}
    for name, before, after in (
        ("omit_step_charge", "meter.charge(Resource::Step, 1)", "meter.charge(Resource::Step, 0)"),
        ("charge_wrong_resource", "meter.charge(Resource::Step, 1)", "meter.charge(Resource::Read, 1)"),
        ("skip_unused_instruction", "while index < nodes.len() {", "while index + 1 < nodes.len() {"),
        ("rollback_consumed_usage", "if result.is_err() {", "if result.is_err() { meter.used.counters = [0; 8];"),
        ("leak_partial_buffers", "if result.is_err() {", "if false {"),
        ("wrong_budget_precedence", "return Err(Failure::Budget(error));", "return Err(Failure::Arithmetic);"),
    ):
        mutations[name] = (SUBJECT, once(execution, before, after), "proof")
    for name, before, after in (
        ("accept_exhausted_charge", "if next > limit {", "if false {"),
        ("mutate_usage_on_overflow", "let Some(next) = self.used.counters[index].checked_add(amount) else {",
         "let Some(next) = self.used.counters[index].checked_add(amount) else { self.used.counters[index] = 0;"),
    ):
        mutations[name] = (METER, once(meter, before, after), "proof")
    mutations["alias_step_counter"] = (RESOURCE, once(resource, "Self::Step => 7", "Self::Step => 6"), "proof")
    # Computing the node before charging preserves final outputs/counters. This
    # mutant must verify, then fail the independent operational body inventory.
    reordered = once(execution, "        if let Err(error) = meter.charge(Resource::Step, 1) {",
                     "        let cached = evaluation::evaluate_node(&nodes[index], input, values);\n"
                     "        if let Err(error) = meter.charge(Resource::Step, 1) {")
    reordered = once(reordered, "match evaluation::evaluate_node(&nodes[index], input, values) {", "match cached {")
    mutations["execute_before_charging"] = (SUBJECT, reordered, "coverage")
    end = execution.index("pub fn execute(")
    start = execution.rindex("#[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    attribute = execution[start:end]
    for name, replacement in (
        ("omit_metered_contract", ""),
        ("weaken_usage_contract", "#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n"),
        ("narrow_input_domain", once(attribute, "    ensures result.view()",
                                    "    requires input@.len() == 0,\n    ensures result.view()")),
    ):
        mutations[name] = (SUBJECT, entry_contract(execution, replacement), "coverage")
    mutations["inject_caller_usage_argument"] = (SUBJECT,
        once(execution, "    limits: Limits,\n) -> Outcome", "    limits: Limits,\n    caller_usage: Usage,\n) -> Outcome"), "coverage")
    mutations["change_charge_specification"] = (SPEC,
        once(spec, "used[index] as int + amount as int", "amount as int + used[index] as int"), "coverage")
    mutations["add_uncontracted_work"] = (SUBJECT,
        execution + "\nfn uncharged_identity(value: i64) -> i64 { value }\n", "coverage")
    return mutations


def native_checks(directory: Path, pin: dict, environment: dict) -> dict:
    rust = ["rustc", f"+{pin['runtime_rust']}", "--edition=2024",
            "--check-cfg", "cfg(verus_keep_ghost)", "--check-cfg", "cfg(test)"]
    rust += native_dependency_args(ROOT, directory, environment)
    executable = directory / "metered-tests"
    verifier.require_success(verifier.run([*rust, "--test", str(HARNESS), "-o", str(executable)], ROOT, environment))
    native = verifier.run([str(executable)], ROOT, environment)
    verifier.require_success(native)
    library = directory / "libmetered_execution.rlib"
    verifier.require_success(verifier.run([*rust, "--crate-type=rlib", "--crate-name", "metered_execution",
                                          str(HARNESS), "-o", str(library)], ROOT, environment))
    prelude = "use metered_execution::execution_v2::{execute, zero_limits, Resource};\n"
    valid = "fn main() { let outcome = execute(&[], &[], &[], &[], &[], zero_limits()); let (_, usage) = outcome.into_parts(); let _ = usage.used(Resource::Step); }\n"
    specimens = {
        "positive": (prelude + valid, None),
        "forge_usage": ("fn main() { let _ = metered_execution::execution_v2::Usage { counters: [0; 8] }; }", "E0451"),
        "access_meter": ("fn main() { let _ = metered_execution::execution_v2::meter::new(metered_execution::execution_v2::zero_limits()); }", "E0603"),
        "replace_report": (prelude + "fn main() { let mut outcome = execute(&[], &[], &[], &[], &[], zero_limits()); outcome.usage = outcome.usage(); }", "E0616"),
        "supply_usage": (prelude + "fn main() { let usage = execute(&[], &[], &[], &[], &[], zero_limits()).usage(); let _ = execute(&[], &[], &[], &[], &[], zero_limits(), usage); }", "E0061"),
    }
    outcomes = {}
    for name, (source, error_code) in specimens.items():
        path = directory / f"consumer_{name}.rs"
        path.write_text(source)
        process = verifier.run([*rust, "--extern", f"metered_execution={library}", str(path),
                                "-o", str(directory / f"consumer_{name}")], ROOT, environment)
        if error_code is None:
            verifier.require_success(process)
        elif process.returncode == 0 or error_code not in process.stderr:
            raise RuntimeError(f"API negative {name} did not refuse for {error_code}")
        outcomes[name] = {"exit_code": process.returncode, "expected_error": error_code}
    return {"test_output": native.stdout.strip(), "api_consumers": outcomes}


def check(cache: Path, install: bool) -> dict:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("this proof profile requires qualified Linux x86-64 tools")
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    report_pin = {**pin, "expected_verified": profile["expected_verified"], "target_functions": profile["target_functions"]}
    before = snapshot()
    tools = verifier.prepare_tools(cache, pin, install)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("VERUS_", "VARGO_")) and key not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    environment.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(tools / "z3"))
    command = [str(tools / "verus"), "--crate-type=lib", "--edition=2024", "--no-cheating",
               "--no-external-by-default", "--num-threads", "2", "-V", "spinoff-all", "--output-json", "--log", "vir",
               "--log", "vir-option=no_span+no_type+no_fn_details"]
    mutations = []
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-meter-proof-") as temporary:
        directory = Path(temporary)
        positive_command = [*command, "--log-dir", str(directory / "positive"), str(HARNESS)]
        positive = verifier.run(positive_command, ROOT, environment)
        verifier.require_success(positive)
        report = json.loads(positive.stdout)
        if not verifier.accepted(report, report_pin):
            raise RuntimeError("metered proof report lacks pinned tools or complete obligation coverage")
        coverage = require_coverage((directory / "positive/crate.vir").read_text(), profile)
        native = native_checks(directory, pin, environment)
        print("V2 meter: exact proof coverage and native/API boundaries passed", flush=True)
        for name, (path, changed, expected) in mutation_sources(
                (ROOT / SUBJECT).read_text(), (ROOT / METER).read_text(), (ROOT / SPEC).read_text(),
                (ROOT / RESOURCE).read_text()).items():
            specimen = directory / name
            for unit in UNIT_SOURCES:
                target = specimen / unit
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(changed) if unit == path else target.write_bytes((ROOT / unit).read_bytes())
            adjust_specimen_source_lengths(specimen)
            mutant = verifier.run([*command, "--log-dir", str(specimen / "logs"), str(HARNESS)], specimen, environment)
            result = json.loads(mutant.stdout).get("verification-results", {})
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
            print(f"V2 meter: caught {name} ({expected})", flush=True)
    verifier.verify_tool_files(tools, pin)
    if before != snapshot():
        raise RuntimeError("meter proof or integration source changed during verification")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
    return {"schema": "zeno-fcis/metered-execution-evidence/1", "status": "passed",
            "revision": revision, "working_tree_dirty": bool(dirty), "source_sha256": before,
            "toolchain": pin, "command": positive_command, "exit_code": positive.returncode,
            "verus_report": report, "translated_function_coverage": coverage,
            "operational_order_evidence": "reviewed executable-body inventory; execute-before-charge mutation verifies but is refused",
            "native": native, "mutations": mutations,
            "scope": "owned V2 counters, eager scalar outcomes and shared protected-record dependency closure",
            "trusted_base": ["reviewed specifications and operational body manifest", "Verus translation/erasure and bundled vstd/Z3",
                             "Rust compilers", "standard library/allocation", "host platform"],
            "unproved": ["Program convenience adapter", "catalog extraction and original-envelope/hash admission",
                         "atomic grouped operations", "complete decisions", "mandatory V2 authority and replay", "laws/genesis", "shell"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/zeno-fcis/verus")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        receipt = check(args.cache.resolve(), args.install)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        receipt = {"schema": "zeno-fcis/metered-execution-evidence/1", "status": "failed", "error": str(error)}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"V2 meter: {receipt['status']}; receipt: {args.out}")
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    print(f"{receipt['verus_report']['verification-results']['verified']} obligations; {len(receipt['mutations'])} mutations caught")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
