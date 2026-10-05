#!/usr/bin/env python3
"""Check complete original schema bytes, closed metadata and private custody."""
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
from verus_coverage import require_coverage

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/schema-binding.json")
HARNESS = Path("verification/verus/schema_binding.rs")
SUBJECT = Path("crates/zeno-fcis-synthesis/src/finite/canonical_v2/schema.rs")
ADMISSION = SUBJECT.parent / "schema/admission.rs"
SPEC = SUBJECT.parent / "schema/spec.rs"
CANONICAL = SUBJECT.parent
UNIT_SOURCES = (HARNESS, *sorted(path.relative_to(ROOT)
    for path in (ROOT / CANONICAL).rglob("*.rs")))

UNIT_SOURCES = tuple(dict.fromkeys((*UNIT_SOURCES, *execution_sources(ROOT))))
PUBLIC_TEST = Path("crates/zeno-fcis-authority/tests/v2_schema_binding.rs")
NATIVE_SOURCES = tuple(sorted(path.relative_to(ROOT)
    for crate in ("zeno-fcis-schema", "zeno-fcis-codec", "zeno-fcis-value")
    for path in (ROOT / "crates" / crate / "src").rglob("*.rs")))
SOURCES = (*UNIT_SOURCES, *NATIVE_SOURCES, PUBLIC_TEST, PROFILE, verifier.PIN,
    Path("tools/check_schema_binding_v2.py"), Path("tools/test_check_schema_binding_v2.py"),
    Path("tools/verus_coverage.py"), Path("tools/check_verus.py"),
    Path("tools/check_finite_execution.py"), Path("Cargo.toml"), Path("Cargo.lock"),
    Path("rust-toolchain.toml"), Path("crates/zeno-fcis-synthesis/Cargo.toml"),
    Path("crates/zeno-fcis-authority/Cargo.toml"),
    Path("crates/zeno-fcis-synthesis/src/finite/mod.rs"),
    Path("docs/V2_SCHEMA_BINDING_STAGE.md"))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_proof_sources.py"),
                                Path("verification/verus/authority_v2_sources.json"))))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_native_dependencies.py"))))


def snapshot() -> dict[str, str]:
    for path in SOURCES:
        if (ROOT / path).is_symlink():
            raise RuntimeError(f"schema proof source is a symbolic link: {path}")
    return {str(path): verifier.digest(ROOT / path) for path in SOURCES}


def getter_contract(source: str, replacement: str) -> str:
    end = source.index("    pub fn original(&self)")
    start = source.rindex("    #[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    return source[:start] + replacement + source[end:]


def mutation_sources(source: str, admission: str, spec: str) -> dict[str, tuple[Path, str, str]]:
    mutations = {}
    for name, before, after in (
        ("skip_original_names", "if bytes[offset + index] != expected[index]",
         "if false && bytes[offset + index] != expected[index]"),
        ("wrong_field_type", "word(bytes, next, 4, expected.type_id as u128)",
         "word(bytes, next, 4, expected.id as u128)"),
        ("wrong_signed_lower_bound", "let next = signed(bytes, next, *min)?;",
         "let next = signed(bytes, next, *max)?;"),
        ("wrong_unsigned_upper_bound", "word(bytes, next, 16, *max)",
         "word(bytes, next, 16, *min)"),
        ("wrong_sum_payload_flag", "if sum { word(bytes, next, 1, 0)", "if sum { word(bytes, next, 1, 1)"),
        ("wrong_unused_unit", "Kind::Unit => word(bytes, offset, 1, 0)",
         "Kind::Unit => word(bytes, offset, 1, 1)"),
        ("wrong_original_magic", "90u8, 70u8, 67u8, 73u8", "91u8, 70u8, 67u8, 73u8"),
        ("admit_trailing_schema", "Some(end) => end == bytes.len()", "Some(end) => end <= bytes.len()"),
        ("refuse_exact_wire_size", "if bytes.len() as u64 > max_bytes", "if bytes.len() as u64 >= max_bytes"),
    ):
        mutations[name] = (SUBJECT, once(source, before, after), "proof")
    for name, before, after in (
        ("refuse_exact_schema_size", "if bytes.len() as u64 > limits.bytes", "if bytes.len() as u64 >= limits.bytes"),
        ("accept_bad_first_name_character", "if !matches!(first", "if false && !matches!(first"),
        ("accept_repeated_field_id", "fields[index - 1].id >= fields[index].id", "fields[index - 1].id > fields[index].id"),
        ("accept_repeated_variant_id", "spec::variants_prefix(variants@,index as nat), decreases variants.len()-index, ))] while index < variants.len() { if !name_valid(variants[index].name) || (index > 0 && variants[index - 1].id >= variants[index].id)",
         "spec::variants_prefix(variants@,index as nat), decreases variants.len()-index, ))] while index < variants.len() { if !name_valid(variants[index].name) || (index > 0 && variants[index - 1].id > variants[index].id)"),
        ("accept_repeated_type_id", "definitions[index - 1].id >= definitions[index].id", "definitions[index - 1].id > definitions[index].id"),
        ("accept_duplicate_type_name", "if equal(definitions[prior].name, definitions[index].name)",
         "if false && equal(definitions[prior].name, definitions[index].name)"),
        # accept_nested_record retired: schema admission accepts compound definitions for the V2.1
        # profile; descriptor leaves (InputLeaf) cannot name a nested record.
        ("accept_reversed_signed_range", "Kind::I128 { min, max } => min <= max", "Kind::I128 { min: _, max: _ } => true"),
        ("wrong_original_custody", "proof! { reveal(Checked::view); }\n        self.original\n", "proof! { reveal(Checked::view); }\n        &[]\n"),
        ("skip_encoding_admission", "if !super::encoding_matches(bytes, description, limits.bytes)",
         "if false && !super::encoding_matches(bytes, description, limits.bytes)"),
    ):
        mutations[name] = (ADMISSION, once(admission, before, after), "proof")
    for name, replacement in (
        ("omit_original_getter_contract", ""),
        ("weaken_original_getter_contract", "    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n"),
        ("narrow_original_getter_domain", "    #[cfg_attr(verus_keep_ghost, verus_spec(result =>\n"
         "        requires self.view().0.len() > 0,\n"
         "        ensures result@ == self.view().0,\n    ))]\n"),
    ):
        mutations[name] = (ADMISSION, getter_contract(admission, replacement), "coverage")
    mutations["equivalent_schema_specification_change"] = (SPEC,
        once(spec, "bytes.len() as u64 <= max_bytes", "!(bytes.len() as u64 > max_bytes)"), "coverage")
    mutations["add_uncontracted_schema_function"] = (SUBJECT,
        source + "\npub fn uncontracted_schema_length(n: usize) -> usize { n }\n", "coverage")
    return mutations


def native_check(directory: Path, cwd: Path, pin: dict, environment: dict) -> dict:
    executable = directory / "schema-binding-tests"
    command = ["rustc", f"+{pin['runtime_rust']}", "--edition=2024", "--test",
        "--check-cfg", "cfg(verus_keep_ghost)", "--check-cfg", "cfg(test)",
        str(HARNESS), "-o", str(executable)]
    command += native_dependency_args(ROOT, directory, environment)
    build = verifier.run(command, cwd, environment)
    (directory / "native-build.stdout").write_text(build.stdout)
    (directory / "native-build.stderr").write_text(build.stderr)
    if build.returncode:
        return {"command": command, "build_exit": build.returncode, "exit_code": None}
    run = verifier.run([str(executable)], cwd, environment)
    (directory / "native.stdout").write_text(run.stdout)
    (directory / "native.stderr").write_text(run.stderr)
    return {"command": command, "build_exit": 0, "exit_code": run.returncode,
        "executable_sha256": verifier.digest(executable), "test_output": run.stdout}


def check(cache: Path, install: bool, directory: Path) -> dict:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("this schema proof profile requires qualified Linux x86-64 tools")
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    report_pin = {**pin, "expected_verified": profile["expected_verified"],
        "target_functions": profile["target_functions"]}
    before = snapshot()
    (directory / "source_sha256.json").write_text(json.dumps(before, indent=2, sort_keys=True) + "\n")
    tools = verifier.prepare_tools(cache, pin, install)
    environment = {key: value for key, value in os.environ.items()
        if not key.startswith(("VERUS_", "VARGO_"))
        and key not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    environment["RUSTUP_TOOLCHAIN"] = pin["rust_toolchain"]
    environment["VERUS_Z3_PATH"] = str(tools / "z3")
    command = [str(tools / "verus"), "--crate-type=lib", "--edition=2024",
        "--no-cheating", "--no-external-by-default", "--num-threads", "2", "-V", "spinoff-all", "--output-json",
        "--log", "vir", "--log", "vir-option=no_span+no_type+no_fn_details"]
    positive_command = [*command, "--log-dir", str(directory / "positive"), str(HARNESS)]
    positive = verifier.run(positive_command, ROOT, environment)
    (directory / "positive.json").write_text(positive.stdout)
    (directory / "positive.stderr").write_text(positive.stderr)
    verifier.require_success(positive)
    report = json.loads(positive.stdout)
    if not verifier.accepted(report, report_pin):
        raise RuntimeError("schema proof lacks pinned tools or complete obligation coverage")
    coverage = require_coverage((directory / "positive/crate.vir").read_text(), profile)
    native = native_check(directory, ROOT, pin, environment)
    if native["build_exit"] or native["exit_code"]:
        raise RuntimeError("independent schema native comparison failed")
    public_command = ["cargo", f"+{pin['runtime_rust']}", "test", "--locked", "--offline",
        "-p", "zeno-fcis-authority", "--test", "v2_schema_binding"]
    public = verifier.run(public_command, ROOT, {**environment, "CARGO_BUILD_JOBS": "1", "RUST_TEST_THREADS": "1"})
    (directory / "public-native.stdout").write_text(public.stdout)
    (directory / "public-native.stderr").write_text(public.stderr)
    verifier.require_success(public)
    print("V2 schema: exact proof, complete coverage and independent native comparisons passed", flush=True)
    mutations = []
    for name, (path, changed, expected) in mutation_sources(
        (ROOT / SUBJECT).read_text(), (ROOT / ADMISSION).read_text(), (ROOT / SPEC).read_text()).items():
        specimen = directory / name
        for unit in UNIT_SOURCES:
            target = specimen / unit
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(changed) if unit == path else target.write_bytes((ROOT / unit).read_bytes())
        adjust_specimen_source_lengths(specimen)
        mutant = verifier.run([*command, "--log-dir", str(specimen / "logs"), str(HARNESS)], specimen, environment)
        (specimen / "verus.json").write_text(mutant.stdout)
        (specimen / "verus.stderr").write_text(mutant.stderr)
        try:
            result = json.loads(mutant.stdout).get("verification-results", {})
        except ValueError:
            result = {}
        refusal = None
        mutant_native = None
        if expected == "proof":
            killed = (mutant.returncode != 0 and result.get("success") is False
                and type(result.get("errors")) is int and result["errors"] > 0
                and result.get("encountered-vir-error") is False)
            mutant_native = native_check(specimen, specimen, pin, environment)
            killed = killed and mutant_native["build_exit"] == 0 and mutant_native["exit_code"] not in (None, 0)
        else:
            if mutant.returncode == 0:
                try:
                    require_coverage((specimen / "logs/crate.vir").read_text(), profile)
                except ValueError as error:
                    refusal = str(error)
            killed = (mutant.returncode == 0 and result.get("success") is True
                and result.get("errors") == 0 and refusal is not None)
        row = {"name": name, "expected_failure": expected, "killed": killed,
            "changed_source": str(path), "changed_sha256": verifier.digest(specimen / path),
            "exit_code": mutant.returncode, "verification_results": result,
            "coverage_refusal": refusal, "native": mutant_native, "logs": str(specimen)}
        mutations.append(row)
        (directory / "mutations.json").write_text(json.dumps(mutations, indent=2) + "\n")
        if not killed:
            raise RuntimeError(f"schema mutation {name} survived or failed for an unrelated reason")
        print(f"V2 schema: caught {name} ({expected})", flush=True)
    verifier.verify_tool_files(tools, pin)
    if before != snapshot():
        raise RuntimeError("schema proof or native integration source changed during verification")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
    return {"schema": "zeno-fcis/schema-binding-evidence/1", "status": "passed",
        "revision": revision, "working_tree_dirty": bool(dirty), "source_sha256": before,
        "toolchain": pin, "command": positive_command, "exit_code": positive.returncode,
        "verus_report": report, "translated_function_coverage": coverage,
        "native": native, "public_native": {"command": public_command, "exit_code": public.returncode, "test_output": public.stdout},
        "mutations": mutations, "logs": str(directory),
        "scope": "exact complete original schema wire, flat closed metadata and private immutable custody",
        "trusted_base": ["reviewed specifications and translated coverage manifest",
            "Verus translation/erasure and bundled vstd/Z3", "Rust compilers", "standard library", "host platform"],
        "unproved": ["catalog descriptor and channel extraction", "reviewed policy correspondence",
            "cryptographic commitment derivation", "mandatory V2 authority and replay", "source-to-binary correspondence", "shell"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/zeno-fcis/verus")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="schema-binding-run-", dir=args.out.parent))
    try:
        receipt = check(args.cache.resolve(), args.install, directory)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        receipt = {"schema": "zeno-fcis/schema-binding-evidence/1", "status": "failed", "error": str(error), "logs": str(directory)}
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"V2 schema: {receipt['status']}; receipt: {args.out}")
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    print(f"{receipt['verus_report']['verification-results']['verified']} obligations; {len(receipt['mutations'])} mutations caught")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
