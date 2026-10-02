#!/usr/bin/env python3
"""Check owned V2 record-to-execution composition; this is not transition authorization.

Verus proves exact outcomes/counters. Reviewed executable-body fingerprints
separately guard operational charge order, which those postconditions alone
cannot prove. Catalog admission and the complete mandatory authority bridge remain open.
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
from check_finite_execution import once
from check_metered_execution import UNIT_SOURCES as METER_SOURCES
from verus_coverage import require_coverage

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/record-execution.json")
HARNESS = Path("verification/verus/record_execution.rs")
SUBJECT = Path("crates/zeno-fcis-synthesis/src/finite/execution_v2/record_execution.rs")
SPEC = SUBJECT.parent / "record_execution/spec.rs"
UNIT_SOURCES = (HARNESS, *METER_SOURCES[1:])
SOURCES = (*UNIT_SOURCES, PROFILE, verifier.PIN, SUBJECT.parent / "tests.rs",
           SUBJECT.parent / "input_view/tests.rs", SUBJECT.parent / "record_execution/tests.rs",
           Path("tools/check_record_execution.py"), Path("tools/test_check_record_execution.py"),
           Path("tools/check_metered_execution.py"), Path("tools/check_finite_execution.py"),
           Path("tools/check_verus.py"), Path("tools/verus_coverage.py"),
           Path("Cargo.toml"), Path("rust-toolchain.toml"),
           Path("crates/zeno-fcis-synthesis/src/finite/ir.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite/mod.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite_runtime.rs"),
           Path("crates/zeno-fcis-synthesis/tests/v2_record_execution.rs"))


def snapshot() -> dict[str, str]:
    for path in SOURCES:
        if (ROOT / path).is_symlink():
            raise RuntimeError(f"proof source is a symbolic link: {path}")
    return {str(path): verifier.digest(ROOT / path) for path in SOURCES}


def entry_contract(source: str, replacement: str) -> str:
    end = source.index("pub fn execute(")
    start = source.rindex("#[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    return source[:start] + replacement + source[end:]


def mutation_sources(source: str, specification: str) -> dict[str, tuple[Path, str, str]]:
    mutations = {}
    for name, before, after in (
        ("substitute_same_typed_source", "Source::State => &decoded.state,", "Source::State => &decoded.command,"),
        ("substitute_source_descriptor", "Source::State => invocation.state.fields,", "Source::State => invocation.command.fields,"),
        ("ignore_upper_domain_bound", "a == c && b == d", "a == c"),
        ("alias_boolean_and_integer_domain", "a == c && b == d,\n        _ => false,", "a == c && b == d,\n        _ => true,"),
        ("wrong_binding_scalar_position", "Some(scalars[index])", "Some(scalars[0])"),
        ("allow_unknown_binding", "let index = find_field(descriptor, binding.field)?;", "let index = 0;"),
        ("allow_duplicate_binding", "if same_source(bindings[prior].source, binding.source)", "if false"),
        ("omit_context_binding_coverage", "&& covered(invocation.context.fields, Source::Context, bindings)", "&& true"),
        ("omit_context_schema_admission", "if !input_view::validate_schema(invocation.context.fields) {", "if false {"),
        ("drop_metadata_refusal_cleanup", "output.clear();\n    metadata(invocation, program.inputs, bindings)?;", "metadata(invocation, program.inputs, bindings)?;"),
        ("mislabel_record_refusal", "source: Source::Command,\n            refusal,", "source: Source::State,\n            refusal,"),
        ("decode_wrong_original_record", "        invocation.context.bytes,", "        invocation.command.bytes,"),
        ("reset_shared_meter_before_execution", "let mut scratch = Vec::new();", "let mut scratch = Vec::new();\n    meter.used.counters = [0; 8];"),
        ("drop_prior_source_attempt_prefix", "let mut input = Vec::new();", "attempts.state.clear();\n    let mut input = Vec::new();"),
        ("rollback_record_charges_on_execution_refusal", "Err(error) => Err(Failure::Execution(error)),", "Err(error) => { meter.used.counters = [0; 8]; Err(Failure::Execution(error)) },"),
        ("misroute_abi_projection", "bound_scalar(invocation, decoded, bindings[index])", "bound_scalar(invocation, decoded, bindings[0])"),
    ):
        mutations[name] = (SUBJECT, once(source, before, after), "proof")
    start = source.index("    if let Err(refusal) = input_view::project_into(\n        invocation.context.bytes,")
    end = source.index("    #[cfg(verus_keep_ghost)]", start)
    mutations["skip_complete_context_record"] = (SUBJECT,
        once(source, source[start:end], ""), "proof")
    # Public custody stays fixed by the translated manifest. The applied typing
    # bridge is needed to prove scalar admission at the evaluator call.
    mutations["weaken_public_execution_contract"] = (SUBJECT,
        entry_contract(source, "#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n"), "coverage")
    mutations["narrow_public_execution_domain"] = (SUBJECT,
        once(source, "ensures result.view() == spec::execution(invocation, program, bindings@,",
             "requires bindings@.len() > 0,\n    ensures result.view() == spec::execution(invocation, program, bindings@,"), "coverage")
    mutations["inject_caller_usage"] = (SUBJECT,
        once(source, "    limits: super::Limits,\n) -> Outcome", "    limits: super::Limits,\n    _caller_usage: super::Usage,\n) -> Outcome"), "coverage")
    mutations["remove_typing_bridge_application"] = (SUBJECT,
        once(source, "        spec::tuple_is_typed(invocation, program.inputs@, bindings@, spec::decoded(&decoded), bindings@.len());", ""), "proof")
    mutations["cached_record_before_metadata"] = (SUBJECT,
        once(source, "    metadata(invocation, program.inputs, bindings)?;",
             "    let _cached_record = input_view::project(invocation.state.bytes, invocation.state.fields, meter.limits);\n"
             "    metadata(invocation, program.inputs, bindings)?;"), "coverage")
    mutations["change_record_execution_specification"] = (SPEC,
        once(specification, "    match metadata(invocation, program.inputs@, bindings) {",
             "    let admitted = metadata(invocation, program.inputs@, bindings);\n    match admitted {"), "coverage")
    mutations["add_uncontracted_record_work"] = (SUBJECT,
        source + "\npub fn uncontracted_record_execution_probe() -> u64 { 42 }\n", "coverage")
    return mutations


def native_checks(directory: Path, pin: dict, environment: dict) -> dict:
    rust = ["rustc", f"+{pin['runtime_rust']}", "--edition=2024",
            "--check-cfg", "cfg(verus_keep_ghost)", "--check-cfg", "cfg(test)"]
    tests = directory / "native-tests"
    verifier.require_success(verifier.run([*rust, "--test", str(HARNESS), "-o", str(tests)], ROOT, environment))
    native = verifier.run([str(tests), "--test-threads=2"], ROOT, environment)
    verifier.require_success(native)
    library = directory / "librecord_execution.rlib"
    verifier.require_success(verifier.run([*rust, "--crate-type=rlib", "--crate-name=record_execution",
                                           str(HARNESS), "-o", str(library)], ROOT, environment))
    prelude = "use record_execution::execution_v2::{execute_records, zero_limits, Resource, RawRecord, RecordInvocation, ScalarProgram, RecordExecutionOutcome};\n"
    build = "let bytes=[9,0,0,0,0]; let inv=RecordInvocation { state:RawRecord { bytes:&bytes,fields:&[] }, command:RawRecord { bytes:&bytes,fields:&[] }, context:RawRecord { bytes:&bytes,fields:&[] } }; let program=ScalarProgram { inputs:&[],outputs:&[],nodes:&[],roots:&[] }; let limits=zero_limits().with_limit(Resource::Byte,15); let outcome=execute_records(&inv,&program,&[],limits);"
    specimens = {
        "positive": (prelude + "fn main() { " + build +
                     " assert_eq!(outcome.usage().used(Resource::Byte),15); assert_eq!(outcome.into_parts().0,Ok(vec![])); }", None),
        "forge_outcome": (prelude + "fn main() { " + build +
                     " let usage=outcome.usage(); let attempts=outcome.into_parts().2; let _=RecordExecutionOutcome { result:Ok(vec![123]),usage,attempts }; }", "E0451"),
        "replace_usage": (prelude + "fn main() { " + build + " let mut changed=outcome; changed.usage=changed.usage(); }", "E0616"),
        "replace_attempts": (prelude + "fn main() { " + build + " let attempts=outcome.into_parts().2; let _=attempts.state; }", "E0616"),
        "caller_initial_usage": (prelude + "fn main() { " + build + " let _=execute_records(&inv,&program,&[],limits,outcome.usage()); }", "E0061"),
        "private_execution": ("fn main() { let _=record_execution::execution_v2::record_execution::execute_into; }", "E0603"),
        "private_meter": ("fn main() { let _=record_execution::execution_v2::meter::new; }", "E0603"),
    }
    outcomes = {}
    for name, (source, error_code) in specimens.items():
        path = directory / f"consumer_{name}.rs"
        path.write_text(source)
        process = verifier.run([*rust, "--extern", f"record_execution={library}", str(path),
                                "-o", str(directory / f"consumer_{name}")], ROOT, environment)
        if error_code is None:
            verifier.require_success(process)
            verifier.require_success(verifier.run([str(directory / f"consumer_{name}")], ROOT, environment))
        elif process.returncode == 0 or error_code not in process.stderr:
            raise RuntimeError(f"record execution API negative {name} did not refuse for {error_code}")
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
               "--no-external-by-default", "--num-threads", "2", "--output-json", "--log", "vir",
               "--log", "vir-option=no_span+no_type+no_fn_details"]
    mutations = []
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-record-proof-") as temporary:
        directory = Path(temporary)
        positive_command = [*command, "--log-dir", str(directory / "positive"), str(HARNESS)]
        positive = verifier.run(positive_command, ROOT, environment)
        verifier.require_success(positive)
        report = json.loads(positive.stdout)
        if not verifier.accepted(report, report_pin):
            raise RuntimeError("metered proof report lacks pinned tools or complete obligation coverage")
        coverage = require_coverage((directory / "positive/crate.vir").read_text(), profile)
        native = native_checks(directory, pin, environment)
        print("V2 record execution: exact proof coverage and native/API boundaries passed", flush=True)
        for name, (path, changed, expected) in mutation_sources((ROOT / SUBJECT).read_text(), (ROOT / SPEC).read_text()).items():
            specimen = directory / name
            for unit in UNIT_SOURCES:
                target = specimen / unit
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(changed) if unit == path else target.write_bytes((ROOT / unit).read_bytes())
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
            print(f"V2 record execution: caught {name} ({expected})", flush=True)
    verifier.verify_tool_files(tools, pin)
    if before != snapshot():
        raise RuntimeError("meter proof or integration source changed during verification")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
    return {"schema": "zeno-fcis/record-execution-evidence/1", "status": "passed",
            "revision": revision, "working_tree_dirty": bool(dirty), "source_sha256": before,
            "toolchain": pin, "command": positive_command, "exit_code": positive.returncode,
            "verus_report": report, "translated_function_coverage": coverage,
            "operational_order_evidence": "reviewed complete body inventory; inherited Byte/Read/Step order controls remain separate",
            "native": native, "mutations": mutations,
            "scope": "complete original State/Command/Context projection, exact source/field ABI ordering, typed tuple and eager execution with one meter",
            "trusted_base": ["reviewed specifications and operational body manifest", "Verus translation/erasure and bundled vstd/Z3",
                             "Rust compilers", "standard library/allocation", "host platform"],
            "unproved": ["catalog descriptor extraction/admission", "original-envelope framing/hash admission",
                         "atomic grouped operations", "complete decisions",
                         "mandatory V2 authority and replay", "laws/genesis", "compiler/erasure correspondence", "shell"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/zeno-fcis/verus")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        receipt = check(args.cache.resolve(), args.install)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        receipt = {"schema": "zeno-fcis/record-execution-evidence/1", "status": "failed", "error": str(error)}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"V2 record execution: {receipt['status']}; receipt: {args.out}")
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    print(f"{receipt['verus_report']['verification-results']['verified']} obligations; {len(receipt['mutations'])} mutations caught")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
