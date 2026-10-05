#!/usr/bin/env python3
"""Check original-envelope framing and exact immutable payload custody.

Framing is total over all slices and expected identities. Typed admission,
metered ingress, catalog binding and authority remain separate integration.
"""
from __future__ import annotations

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
from v2_native_dependencies import native_dependency_args
import check_verus as verifier
from check_finite_execution import once
from verus_coverage import require_coverage

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/envelope-frame.json")
HARNESS = Path("verification/verus/envelope_frame.rs")
SUBJECT = Path("crates/zeno-fcis-synthesis/src/finite/canonical_v2/envelope.rs")
SPEC = SUBJECT.parent / "envelope/spec.rs"
PARENT = SUBJECT.parent / "mod.rs"
UNIT_SOURCES = (HARNESS, *sorted(path.relative_to(ROOT)
                for path in (ROOT / SUBJECT.parent).rglob("*.rs")))

UNIT_SOURCES = tuple(dict.fromkeys((*UNIT_SOURCES, *execution_sources(ROOT))))
SOURCES = (*UNIT_SOURCES, PROFILE, verifier.PIN,
           Path("tools/check_envelope_frame_v2.py"), Path("tools/test_check_envelope_frame_v2.py"),
           Path("tools/verus_coverage.py"), Path("tools/check_verus.py"),
           Path("tools/check_finite_execution.py"), Path("Cargo.toml"),
           Path("rust-toolchain.toml"), Path("crates/zeno-fcis-synthesis/src/finite/mod.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite_runtime.rs"),
           Path("docs/V2_ENVELOPE_ADMISSION_STAGE.md"))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_proof_sources.py"),
                                Path("verification/verus/authority_v2_sources.json"))))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_native_dependencies.py"))))


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
        mutations[name] = (SUBJECT, frame_contract(source, replacement), "coverage")
    mutations["change_frame_specification"] = (SPEC,
        once(spec, "(bytes.len() - 48) as u128", "(bytes.len() - 24 - 24) as u128"), "coverage")
    mutations["add_uncontracted_function"] = (SUBJECT,
        source + "\npub fn uncontracted_frame_length(length: usize) -> usize { length }\n", "coverage")
    return mutations


def native_checks(directory: Path, pin: dict, environment: dict) -> dict:
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
        for name, (path, changed, expected) in mutation_sources(
                (ROOT / SUBJECT).read_text(), (ROOT / SPEC).read_text()).items():
            specimen = directory / name
            for unit in UNIT_SOURCES:
                target = specimen / unit
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(changed if unit == path else (ROOT / unit).read_text())
            adjust_specimen_source_lengths(specimen)
            mutant = verifier.run([*command, "--log-dir", str(specimen / "logs"),
                                   str(specimen / HARNESS)], ROOT, environment)
            (specimen / "verus.json").write_text(mutant.stdout)
            (specimen / "verus.stderr").write_text(mutant.stderr)
            try:
                result = json.loads(mutant.stdout).get("verification-results", {})
            except ValueError:
                result = {}
            refusal = None
            if expected == "proof":
                killed = (mutant.returncode != 0 and result.get("errors", 0) > 0
                          and result.get("encountered-vir-error") is False)
            else:
                if mutant.returncode == 0:
                    try:
                        require_coverage((specimen / "logs/crate.vir").read_text(), profile)
                    except ValueError as error:
                        refusal = str(error)
                killed = (mutant.returncode == 0 and result.get("success") is True
                          and result.get("errors") == 0 and refusal is not None)
            mutations.append({"name": name, "expected_failure": expected, "killed": killed,
                              "changed_source": str(path), "changed_sha256": verifier.digest(specimen / path),
                              "exit_code": mutant.returncode, "verification_results": result,
                              "coverage_refusal": refusal, "logs": str(specimen)})
            (directory / "mutations.json").write_text(json.dumps(mutations, indent=2) + "\n")
            if not killed:
                raise RuntimeError(f"envelope mutation {name} survived or failed for an unrelated reason")
            print(f"V2 envelope: caught {name} ({expected})", flush=True)
    verifier.verify_tool_files(tools, pin)
    if before != snapshot():
        raise RuntimeError("envelope proof or integration source changed during verification")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
    return {"schema": "zeno-fcis/envelope-frame-evidence/1", "status": "passed",
            "revision": revision, "working_tree_dirty": bool(dirty), "source_sha256": before,
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
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/zeno-fcis/verus")
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
    raise SystemExit(main())
