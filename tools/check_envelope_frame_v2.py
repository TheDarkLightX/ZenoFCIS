#!/usr/bin/env python3
"""Check original-envelope framing and exact immutable payload custody.

Framing is total over all slices and expected identities. Typed admission,
metered ingress, catalog binding and authority remain separate integration.
"""
from __future__ import annotations

from qualification_controls import ControlRefused, run_controls

import argparse
from contextlib import nullcontext
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import zipfile

from v2_proof_sources import execution_sources, adjust_specimen_source_lengths
from v2_native_dependencies import native_dependency_args, native_test_environment
from verus_coverage import profile_inventory
import check_verus as verifier
from check_finite_execution import once
from verus_contract_controls import contract_rejected
from verus_coverage import require_coverage

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/envelope-frame.json")
HARNESS = Path("verification/verus/envelope_frame.rs")
SUBJECT = Path("crates/zeno-fcis-synthesis/src/finite/canonical_v2/envelope.rs")
SPEC = SUBJECT.parent / "envelope/spec.rs"
PARENT = SUBJECT.parent / "mod.rs"
FRAMED = "crates/zeno-fcis-synthesis/src/finite/execution_v2/composition/framed.rs"
# Demonstrated rejections at the verified production caller `framed::project`:
# its postcondition (line 42) consumes frame's, and it frames every length (line 56).
CONTRACT_REJECTIONS = {
    "omit_frame_contract": frozenset({("postcondition not satisfied", FRAMED, 42)}),
    "weaken_frame_contract": frozenset({("postcondition not satisfied", FRAMED, 42)}),
    "narrow_frame_input": frozenset({("precondition not satisfied", FRAMED, 56)}),
}
UNIT_SOURCES = (HARNESS, *sorted(path.relative_to(ROOT)
                for path in (ROOT / SUBJECT.parent).rglob("*.rs")))

UNIT_SOURCES = tuple(dict.fromkeys((*UNIT_SOURCES, *execution_sources(ROOT))))
SOURCES = (*UNIT_SOURCES, PROFILE, verifier.PIN,
           Path("tools/check_envelope_frame_v2.py"), Path("tools/test_check_envelope_frame_v2.py"),
           Path("tools/verus_coverage.py"), Path("tools/check_verus.py"),
           Path("tools/check_finite_execution.py"), Path("Cargo.toml"),
           Path("rust-toolchain.toml"), Path("crates/zeno-fcis-synthesis/src/finite/mod.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite_runtime.rs"),
           Path("README.md"))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_proof_sources.py"),
                                Path("verification/verus/authority_v2_sources.json"))))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_native_dependencies.py"),
                                Path("tools/verus_contract_controls.py"))))
SOURCES = tuple(dict.fromkeys((*SOURCES, *verifier.QUALIFICATION_SOURCES)))


def snapshot() -> dict[str, str]:
    for path in SOURCES:
        if (ROOT / path).is_symlink():
            raise RuntimeError(f"envelope proof source is a symbolic link: {path}")
    return {str(path): verifier.digest(ROOT / path) for path in SOURCES}


def frame_contract(source: str, replacement: str) -> str:
    end = source.index("pub fn frame<")
    start = source.rindex("#[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    return source[:start] + replacement + source[end:]


def mutation_sources(source: str, spec: str) -> dict[str, tuple[Path, str, str]]:
    mutations = {}
    for name, before, after in (
        ("refuse_equal_size_limit", "if bytes.len() as u64 > max_bytes", "if bytes.len() as u64 >= max_bytes"),
        ("refuse_empty_framed_payload", "if bytes.len() < 48", "if bytes.len() <= 48"),
        ("skip_root_binding", "if root != root_type as u128", "if false && root != root_type as u128"),
        ("skip_schema_binding", "if !matches(bytes, 12, schema_hash)", "if false && !matches(bytes, 12, schema_hash)"),
        ("admit_trailing_payload", "if length != (bytes.len() - 48) as u128", "if length > (bytes.len() - 48) as u128"),
        ("wrong_borrowed_payload", "payload: &bytes[48..]", "payload: &bytes[47..]"),
        ("lose_complete_original", "original: bytes,", "original: &bytes[48..],"),
        ("refuse_generic_root_zero", "if root != root_type as u128", "if root_type == 0 || root != root_type as u128"),
    ):
        mutations[name] = (SUBJECT, once(source, before, after), "proof")
    for name, replacement in (
        ("omit_frame_contract", ""),
        ("weaken_frame_contract", "#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n"),
        ("narrow_frame_input", "#[cfg_attr(verus_keep_ghost, verus_spec(result =>\n"
         "    requires bytes@.len() >= 48,\n"
         "    ensures (match result { Ok(p) => Ok(p.view()), Err(e) => Err(e) })\n"
         "        == spec::frame(bytes@, root_type, schema_hash@, max_bytes),\n))]\n"),
    ):
        # The framed projection consumes this contract, so changing it must fail
        # that verified caller rather than verify for a coverage refusal.
        mutations[name] = (SUBJECT, frame_contract(source, replacement), "proof_contract")
    mutations["change_frame_specification"] = (SPEC,
        once(spec, "(bytes.len() - 48) as u128", "(bytes.len() - 24 - 24) as u128"), "coverage")
    mutations["add_uncontracted_function"] = (SUBJECT,
        source + "\npub fn uncontracted_frame_length(length: usize) -> usize { length }\n", "coverage")
    return mutations


def classify_mutation(proc, report, name, path, changed, specimen, pin, profile,
                      *, expected, vir=None):
    """Require complete pinned evidence and only intended semantic diagnostics."""
    if type(proc.returncode) is not int or proc.returncode not in (0, 1) or verifier.NON_SEMANTIC.search(proc.stderr):
        return False, None, None
    if expected == "proof":
        if path != SUBJECT:
            return False, None, None
        try:
            region = verifier.function_lines(changed, "frame")
        except ValueError:
            return False, None, None
        return verifier.semantic_refusal(proc, report, pin, SUBJECT, specimen,
            region=region, target_functions=profile["target_functions"]), None, None
    if expected == "proof_contract":
        expected_diagnostics = CONTRACT_REJECTIONS.get(name)
        if path != SUBJECT or expected_diagnostics is None:
            return False, None, None
        outcome = contract_rejected(proc.returncode, proc.stdout, proc.stderr, pin, expected_diagnostics)
        killed = outcome["accepted"] and verifier.semantic_refusal(proc, report, pin,
            Path(FRAMED), specimen, target_functions=profile["target_functions"])
        return bool(killed), None, outcome
    if (expected != "coverage" or proc.returncode != 0 or vir is None
            or not verifier.proof_succeeded(report, pin) or verifier.ERROR_HEADER.search(proc.stderr)):
        return False, None, None
    intended, refusal = coverage_control(vir, profile, name)
    if intended:
        return True, refusal, None
    return False, None, None


def coverage_control(vir, profile, name):
    """Require the exact specification edit or uncontracted function."""
    try:
        require_coverage(vir, profile)
    except ValueError as error:
        refusal = str(error)
    else:
        return False, None
    actual = profile_inventory(vir, profile)
    expected = profile['functions']
    prefix = profile['namespace'] + 'envelope::'
    if name == 'add_uncontracted_function':
        target = prefix + 'uncontracted_frame_length'
        intended = (set(actual) == set(expected) | {target}
            and target not in expected and actual[target]['mode'] == 'Exec'
            and actual[target]['requires'] == 0 and actual[target]['ensures'] == 0)
    else:
        target = prefix + 'spec::frame'
        intended = (set(actual) == set(expected)
            and actual[target]['body_sha256'] != expected[target]['body_sha256'])
    return intended, refusal

def native_checks(directory: Path, pin: dict, environment: dict) -> dict:
    environment = native_test_environment(ROOT, environment)
    executable = directory / "envelope-frame-tests"
    command = ["rustc", f"+{pin['runtime_rust']}", "--edition=2024", "--test",
               "--check-cfg", "cfg(verus_keep_ghost)", "--check-cfg", "cfg(test)",
               str(HARNESS), "-o", str(executable)]
    command += native_dependency_args(ROOT, directory, environment)
    verifier.require_success(verifier.run(command, ROOT, environment))
    result = verifier.run([str(executable)], ROOT, environment)
    verifier.require_success(result)
    return {"command": command, "exit_code": result.returncode, "test_output": result.stdout}


def check(cache: Path, install: bool, retained_directory: Path | None = None) -> dict:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("this envelope proof profile requires qualified Linux x86-64 tools")
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
               "--no-cheating", "--no-external-by-default", "--num-threads", "2", "-V", "spinoff-all", "--output-json",
               "--log", "vir", "--log", "vir-option=no_span+no_type+no_fn_details"]
    mutations = []
    context = (nullcontext(str(retained_directory)) if retained_directory is not None
               else tempfile.TemporaryDirectory(prefix="zeno-fcis-envelope-proof-"))
    with context as temporary:
        directory = Path(temporary)
        (directory / "source_sha256.json").write_text(json.dumps(before, indent=2, sort_keys=True) + "\n")
        positive_command = [*command, "--log-dir", str(directory / "positive"), str(HARNESS)]
        positive = verifier.run(positive_command, ROOT, environment)
        (directory / "positive.json").write_text(positive.stdout)
        (directory / "positive.stderr").write_text(positive.stderr)
        verifier.require_success(positive)
        report = json.loads(positive.stdout)
        if not verifier.accepted(report, report_pin):
            raise RuntimeError("envelope proof report lacks pinned tools or complete obligation coverage")
        coverage = require_coverage((directory / "positive/crate.vir").read_text(), profile)
        native = native_checks(directory, pin, environment)
        print("V2 envelope: exact proof coverage and independent native comparisons passed", flush=True)
        def run_control(control_item):
            (name, (path, changed, expected)) = control_item
            specimen = directory / name
            for unit in UNIT_SOURCES:
                target = specimen / unit
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(changed.encode("utf-8") if unit == path else (ROOT / unit).read_bytes())
            adjust_specimen_source_lengths(specimen)
            mutant_command = [*command, "--log-dir", str(specimen / "logs"), str(specimen / HARNESS)]
            mutant = verifier.run(mutant_command, ROOT, environment)
            (specimen / "verus.json").write_text(mutant.stdout)
            (specimen / "verus.stderr").write_text(mutant.stderr)
            try:
                mutant_report = json.loads(mutant.stdout)
            except ValueError:
                mutant_report = {}
            result = mutant_report.get("verification-results", {})
            vir = ((specimen / "logs/crate.vir").read_text()
                   if expected == "coverage" and mutant.returncode == 0 else None)
            killed, refusal, outcome = classify_mutation(mutant, mutant_report, name, path,
                changed, specimen, pin, profile, expected=expected, vir=vir)
            control_result = {"name": name, "expected_failure": expected, "killed": killed,
                              "changed_source": str(path), "original_sha256": before[str(path)],
                              "changed_sha256": verifier.digest(specimen / path), "command": mutant_command,
                              "exit_code": mutant.returncode, "verification_results": result,
                              "coverage_refusal": refusal, "contract_outcome": outcome, "logs": str(specimen)}
            if not killed:
                raise ControlRefused(control_result, f"envelope mutation {name} survived or failed for an unrelated reason")
            print(f"V2 envelope: caught {name} ({expected})", flush=True)
            return control_result
        def publish_control(control_result):
            mutations.append(control_result)
            (directory / "mutations.json").write_text(json.dumps(mutations, indent=2) + "\n")
        run_controls(run_control, mutation_sources((ROOT / SUBJECT).read_text(), (ROOT / SPEC).read_text()).items(), timings=directory / 'control-timings.json', publish=publish_control)
    verifier.verify_tool_files(tools, pin)
    if before != snapshot():
        raise RuntimeError("envelope proof or integration source changed during verification")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
    return {"schema": "zeno-fcis/envelope-frame-evidence/1", "status": "passed",
            "revision": revision, "working_tree_dirty": bool(dirty), "source_sha256": before, "source_bindings": verifier.evidence_bindings(before, HARNESS),
            "toolchain": pin, "command": positive_command, "exit_code": positive.returncode,
            "verus_report": report, "translated_function_coverage": coverage,
            "native": native, "mutations": mutations,
            "scope": "exact original frame/root/schema/suffix custody with total ordered refusal on arbitrary slices",
            "trusted_base": ["reviewed specifications and translated coverage manifest",
                             "Verus translation/erasure and bundled vstd/Z3", "Rust compilers",
                             "standard library", "host platform"],
            "unproved": ["catalog/schema extraction and payload typing", "cryptographic commitment derivation",
                         "metered decoding", "complete decisions", "law/genesis checks",
                         "mandatory V2 authority and replay", "source-to-binary correspondence", "shell"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--cache", type=Path, default=verifier.cache_directory())
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="envelope-frame-run-", dir=args.out.parent))
    try:
        receipt = check(args.cache.resolve(), args.install, directory)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        receipt = {"schema": "zeno-fcis/envelope-frame-evidence/1", "status": "failed", "error": str(error)}
    receipt["logs"] = str(directory)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"V2 envelope: {receipt['status']}; receipt: {args.out}")
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    print(f"{receipt['verus_report']['verification-results']['verified']} obligations; {len(receipt['mutations'])} mutations caught")
    return 0


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
