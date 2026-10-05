#!/usr/bin/env python3
"""Check owned V2 protected record projection; this is not transition authorization.

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
from check_finite_execution import once
from check_metered_execution import UNIT_SOURCES as METER_SOURCES
from verus_coverage import require_coverage

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/protected-input.json")
HARNESS = Path("verification/verus/protected_input.rs")
SUBJECT = Path("crates/zeno-fcis-synthesis/src/finite/execution_v2/input_view.rs")
SPEC = SUBJECT.parent / "input_view/spec.rs"
UNIT_SOURCES = (HARNESS, *METER_SOURCES[1:])

UNIT_SOURCES = tuple(dict.fromkeys((*UNIT_SOURCES, *execution_sources(ROOT))))
SOURCES = (*UNIT_SOURCES, PROFILE, verifier.PIN, SUBJECT.parent / "tests.rs",
           SUBJECT.parent / "input_view/tests.rs", SUBJECT.parent / "composition/record_tests.rs",
           Path("tools/check_protected_input.py"), Path("tools/test_check_protected_input.py"),
           Path("tools/check_metered_execution.py"), Path("tools/check_finite_execution.py"),
           Path("tools/check_verus.py"), Path("tools/verus_coverage.py"),
           Path("Cargo.toml"), Path("rust-toolchain.toml"),
           Path("crates/zeno-fcis-synthesis/src/finite/ir.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite/mod.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite_runtime.rs"),
           Path("crates/zeno-fcis-synthesis/tests/v2_protected_input.rs"))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_proof_sources.py"),
                                Path("verification/verus/authority_v2_sources.json"))))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_native_dependencies.py"))))


def snapshot() -> dict[str, str]:
    for path in SOURCES:
        if (ROOT / path).is_symlink():
            raise RuntimeError(f"proof source is a symbolic link: {path}")
    return {str(path): verifier.digest(ROOT / path) for path in SOURCES}


def entry_contract(source: str, replacement: str) -> str:
    end = source.index("pub fn project(")
    start = source.rindex("#[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    return source[:start] + replacement + source[end:]


def mutation_sources(source: str, specification: str) -> dict[str, tuple[Path, str, str]]:
    mutations = {}
    for name, before, after in (
        ("wrong_interval_cardinality", "max as i128 - min as i128 + 1", "max as i128 - min as i128 + 2"),
        ("ignore_negative_code_bound", "variant.code < min || variant.code > max", "variant.code > max"),
        ("allow_duplicate_variant_ids", "variants[prior].id == variant.id || variants[prior].code == variant.code",
         "variants[prior].code == variant.code"),
        ("allow_duplicate_variant_codes", "variants[prior].id == variant.id || variants[prior].code == variant.code",
         "variants[prior].id == variant.id"),
        ("ignore_field_order", "index > 0 && fields[index - 1].id >= fields[index].id", "false"),
        ("wrong_enum_tag", "if tag != 0x07 {", "if tag != 0x0a {"),
        ("allow_sum_payload", "if payload != 0 {", "if false {"),
        ("wrong_type_width", "read_big_endian(bytes, offset, 4)", "read_big_endian(bytes, offset, 3)"),
        ("wrong_variant_width", "read_big_endian(bytes, after_type, 2)", "read_big_endian(bytes, after_type, 1)"),
        ("wrong_integer_value", "value > *max as i128 { return None; } Some((value as i64, end))",
         "value > *max as i128 { return None; } Some((0, end))"),
        ("wrong_unsigned_value", "value > i64::MAX as u128 { return None; } Some((value as i64, end))",
         "value > i64::MAX as u128 { return None; } Some((0, end))"),
        ("wrong_boolean_value", "Some((1, payload_offset))", "Some((0, payload_offset))"),
        ("wrong_closed_variant_mapping", "return Some(code);", "return Some(0);"),
        ("ignore_field_identity", "if id != field.id as u128 {", "if false {"),
        ("ignore_record_count", "if count != field_count as u128 {", "if false {"),
        ("wrong_record_tag", "if tag != 0x09 {", "if tag != 0x08 {"),
        ("omit_ingress_charge", "meter.charge(super::Resource::Byte, bytes.len() as u64)",
         "meter.charge(super::Resource::Byte, 0)"),
        ("omit_protected_read_charge", "meter.charge(super::Resource::Read, 1)",
         "meter.charge(super::Resource::Read, 0)"),
        ("charge_wrong_read_resource", "meter.charge(super::Resource::Read, 1)",
         "meter.charge(super::Resource::Step, 1)"),
        ("wrong_permission_observation", "permitted: charged.is_ok()", "permitted: charged.is_err()"),
        ("allow_trailing_bytes", "if end != bytes.len() {", "if false {"),
        ("leak_partial_scalars", "Err(error) => {\n            output.clear();\n            Err(error)",
         "Err(error) => {\n            Err(error)"),
        ("rollback_prior_usage", "Err(error) => {\n            output.clear();\n            Err(error)",
         "Err(error) => {\n            output.clear();\n            meter.used.counters = [0; 8];\n            Err(error)"),
        ("drop_refused_attempts", "Err(error) => {\n            output.clear();\n            Err(error)",
         "Err(error) => {\n            output.clear();\n            attempts.clear();\n            Err(error)"),
    ):
        mutations[name] = (SUBJECT, once(source, before, after), "proof")

    # These cached parses preserve exact results, usage and attempt records.
    # They genuinely verify; the separately reviewed body inventory refuses them.
    cached_header = once(source,
        "if let Err(error) = meter.charge(super::Resource::Byte, bytes.len() as u64) {",
        "let cached_header = decode_header(bytes, fields.len());\n"
        "    if let Err(error) = meter.charge(super::Resource::Byte, bytes.len() as u64) {")
    cached_header = once(cached_header,
        "let Some(offset) = decode_header(bytes, fields.len()) else {",
        "let Some(offset) = cached_header else {")
    mutations["parse_header_before_ingress"] = (SUBJECT, cached_header, "coverage")
    cached_field = once(source, "let charged = meter.charge(super::Resource::Read, 1);",
        "let cached_field = decode_field(bytes, position, &fields[index]);\n"
        "        let charged = meter.charge(super::Resource::Read, 1);")
    cached_field = once(cached_field, "match decode_field(bytes, position, &fields[index]) {",
        "match cached_field {")
    mutations["parse_field_before_read"] = (SUBJECT, cached_field, "coverage")
    mutations["weaken_projection_contract"] = (SUBJECT,
        entry_contract(source, "#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n"),
        "coverage")
    mutations["narrow_projection_domain"] = (SUBJECT,
        once(source, "ensures result.view() == projection(bytes@, fields@, limits.view(),",
             "requires bytes@.len() > 0,\n    ensures result.view() == projection(bytes@, fields@, limits.view(),"),
        "coverage")
    mutations["inject_caller_usage"] = (SUBJECT,
        once(source, "pub fn project(bytes: &[u8], fields: &[Field], limits: super::Limits)",
             "pub fn project(bytes: &[u8], fields: &[Field], limits: super::Limits, _supplied: super::Usage)"),
        "coverage")
    mutations["change_projection_specification"] = (SPEC,
        once(specification, "if !schema_valid(fields) {", "let admitted = schema_valid(fields);\n    if !admitted {"),
        "coverage")
    mutations["add_uncontracted_projection_work"] = (SUBJECT,
        source + "\npub fn uncontracted_record_probe() -> u64 { 42 }\n", "coverage")
    return mutations


def native_checks(directory: Path, pin: dict, environment: dict) -> dict:
    rust = ["rustc", f"+{pin['runtime_rust']}", "--edition=2024",
            "--check-cfg", "cfg(verus_keep_ghost)", "--check-cfg", "cfg(test)"]
    rust += native_dependency_args(ROOT, directory, environment)
    tests = directory / "native-tests"
    verifier.require_success(verifier.run([*rust, "--test", str(HARNESS), "-o", str(tests)], ROOT, environment))
    native = verifier.run([str(tests), "--test-threads=2"], ROOT, environment)
    verifier.require_success(native)
    library = directory / "libprotected_input.rlib"
    verifier.require_success(verifier.run([*rust, "--crate-type=rlib", "--crate-name=protected_input",
                                           str(HARNESS), "-o", str(library)], ROOT, environment))
    prelude = "use protected_input::execution_v2::{project_record, zero_limits, Resource, RecordProjection};\n"
    build = "let limits=zero_limits().with_limit(Resource::Byte, 5); let outcome=project_record(&[9,0,0,0,0],&[],limits);"
    specimens = {
        "positive": (prelude + "fn main() { " + build +
                     " assert_eq!(outcome.usage().used(Resource::Byte),5); assert!(outcome.attempts().is_empty());"
                     " assert_eq!(outcome.into_parts().0, Ok(vec![])); }", None),
        "forge_projection": (prelude + "fn main() { " + build +
                     " let usage=outcome.usage(); let _=RecordProjection { result: Ok(vec![123]), usage, attempts: vec![] }; }", "E0451"),
        "replace_attempts": (prelude + "fn main() { " + build +
                     " let mut changed=outcome; changed.attempts=vec![]; }", "E0616"),
        "caller_initial_usage": (prelude + "fn main() { " + build +
                     " let _=project_record(&[],&[],limits,outcome.usage()); }", "E0061"),
        "private_projection": ("fn main() { let _ = protected_input::execution_v2::input_view::project_into; }", "E0603"),
    }
    outcomes = {}
    for name, (source, error_code) in specimens.items():
        path = directory / f"consumer_{name}.rs"
        path.write_text(source)
        process = verifier.run([*rust, "--extern", f"protected_input={library}", str(path),
                                "-o", str(directory / f"consumer_{name}")], ROOT, environment)
        if error_code is None:
            verifier.require_success(process)
        elif process.returncode == 0 or error_code not in process.stderr:
            raise RuntimeError(f"protected API negative {name} did not refuse for {error_code}")
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
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-input-proof-") as temporary:
        directory = Path(temporary)
        positive_command = [*command, "--log-dir", str(directory / "positive"), str(HARNESS)]
        positive = verifier.run(positive_command, ROOT, environment)
        verifier.require_success(positive)
        report = json.loads(positive.stdout)
        if not verifier.accepted(report, report_pin):
            raise RuntimeError("metered proof report lacks pinned tools or complete obligation coverage")
        coverage = require_coverage((directory / "positive/crate.vir").read_text(), profile)
        native = native_checks(directory, pin, environment)
        print("V2 input: exact proof coverage and native/API boundaries passed", flush=True)
        for name, (path, changed, expected) in mutation_sources((ROOT / SUBJECT).read_text(), (ROOT / SPEC).read_text()).items():
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
            print(f"V2 input: caught {name} ({expected})", flush=True)
    verifier.verify_tool_files(tools, pin)
    if before != snapshot():
        raise RuntimeError("meter proof or integration source changed during verification")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
    return {"schema": "zeno-fcis/protected-input-evidence/1", "status": "passed",
            "revision": revision, "working_tree_dirty": bool(dirty), "source_sha256": before,
            "toolchain": pin, "command": positive_command, "exit_code": positive.returncode,
            "verus_report": report, "translated_function_coverage": coverage,
            "operational_order_evidence": "reviewed body inventory; cached header/field parsing before charge verifies but is refused",
            "native": native, "mutations": mutations,
            "scope": "complete canonical flat-record projection, exact retained logical counters and descriptor-based attempts",
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
        receipt = {"schema": "zeno-fcis/protected-input-evidence/1", "status": "failed", "error": str(error)}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"V2 input: {receipt['status']}; receipt: {args.out}")
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    print(f"{receipt['verus_report']['verification-results']['verified']} obligations; {len(receipt['mutations'])} mutations caught")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
