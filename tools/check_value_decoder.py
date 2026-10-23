#!/usr/bin/env python3
"""Check the production value decoder, its refinement proof and four defects."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tempfile
import time
import zipfile

import check_verus as verifier

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/value-decoder.json")
HARNESS = Path("verification/verus/value_decoder.rs")
SUBJECT = Path("crates/zeno-fcis-codec/src/lib.rs")
STD_CONTRACTS = Path("verification/verus/value_decoder_std.rs")
STD_TESTS = Path("verification/verus/value_decoder_std_tests.rs")
UNIT_SOURCES = (
    HARNESS, STD_CONTRACTS, STD_TESTS,
    Path("verification/verus/value_decoder_vectors.rs"),
    SUBJECT,
    Path("crates/zeno-fcis-codec/src/decoder_bytes.rs"),
    Path("crates/zeno-fcis-codec/src/decoder_spec.rs"),
    Path("crates/zeno-fcis-codec/src/decoder_induction.rs"),
    Path("crates/zeno-fcis-codec/src/decoder_refinement.rs"),
    Path("crates/zeno-fcis-value/src/lib.rs"),
    Path("crates/zeno-fcis-value/src/decoder_bytes.rs"),
    Path("crates/zeno-fcis-value/src/decoder_model.rs"),
    Path("crates/zeno-fcis-value/src/decoder_refinement.rs"),
)
SOURCES = (*UNIT_SOURCES, PROFILE, verifier.PIN, Path("tools/check_verus.py"),
           Path("tools/check_value_decoder.py"), Path("tools/test_check_value_decoder.py"),
           Path("verification/verus/value_decoder_differential.rs"),
           Path("tools/capture_value_decoder_differential.py"),
           Path("Cargo.toml"), Path("Cargo.lock"), Path("rust-toolchain.toml"))
FORBIDDEN = re.compile(
    r"\b(?:assume|admit)\s*\(|\bassume_specification\b|\bexternal_body\b|"
    r"verifier\s*::\s*external\b|\bexternal_type_specification\b|"
    r"\b(?:exec_allows_no_decreases_clause|decreases_by|assume_termination)\b")
SEMANTIC_TARGETS = {
    "accept_trailing_bytes": "decode_complete_value",
    "skip_map_key_order": "decode_entries",
    "skip_depth_check": "enter",
    "little_endian_u32": "take_blob",
}


def snapshot(root: Path = ROOT) -> dict[str, str]:
    for path in SOURCES:
        if (root / path).is_symlink():
            raise RuntimeError(f"decoder evidence source is a symbolic link: {path}")
    return {str(path): verifier.digest(root / path) for path in SOURCES}


def audit_trust(root: Path, profile: dict) -> None:
    for path in UNIT_SOURCES:
        if path == STD_CONTRACTS:
            continue
        # Comments do not grant trust. Keeping them in this check also makes
        # an unexpected new trust annotation visible during review.
        source = (root / path).read_text()
        source = re.sub(r"//[^\n]*", "", source)
        if FORBIDDEN.search(source):
            raise RuntimeError(f"application proof contains a forbidden trust escape: {path}")
    source = (root / STD_CONTRACTS).read_text()
    if (verifier.digest(root / STD_CONTRACTS) != profile["std_contracts_sha256"]
            or len(re.findall(r"\bpub assume_specification\b", source)) != 5
            or source.count("#[verifier::external_body]") != 1
            or source.count("#[verifier::external_type_specification]") != 1
            or "pub struct ExUtf8Error(core::str::Utf8Error);" not in source
            or re.search(r"\b(?:assume|admit)\s*\(", source)):
        raise RuntimeError("additional standard-library contracts differ from the reviewed profile")
    harness = (root / HARNESS).read_text()
    for path in ("../../crates/zeno-fcis-value/src/lib.rs", "../../crates/zeno-fcis-codec/src/lib.rs"):
        if harness.count(f'#[path = "{path}"]') != 1:
            raise RuntimeError("decoder harness does not include the actual production files")


def replace_once(source: str, before: str, after: str) -> str:
    if source.count(before) != 1:
        raise ValueError(f"mutation anchor is not unique: {before}")
    return source.replace(before, after, 1)


def mutation_sources(source: str) -> dict[str, str]:
    trailing = replace_once(source,
        "let value = decode_value_inner(&mut cursor, state, depth)?;\n    if cursor.remaining() != 0 {",
        "let value = decode_value_inner(&mut cursor, state, depth)?;\n    if false {")
    # Both checks must change for this defect to accept an otherwise canonical
    # value followed by extra bytes. Removing only the first guard still refuses.
    trailing = replace_once(trailing, "if canonical.as_slice() != bytes {",
        "if canonical.as_slice() != &bytes[..cursor.offset] {")
    return {
        "accept_trailing_bytes": trailing,
        "skip_map_key_order": replace_once(source,
            "if match previous {\n            Some(value) => !bytes_before(value, encoded_key),\n"
            "            None => false,\n        } {",
            "if false {"),
        "skip_depth_check": replace_once(source,
            "if depth > self.limits.effective_max_depth() {", "if false {"),
        "little_endian_u32": replace_once(source, "let length = self.take_u32()?;",
            "let mut bytes = [0_u8; 4];\n        bytes.copy_from_slice(self.take(4)?);\n"
            "        let length = u32::from(bytes[0]) | (u32::from(bytes[1]) << 8)"
            " | (u32::from(bytes[2]) << 16) | (u32::from(bytes[3]) << 24);"),
    }


def accepted(proc: subprocess.CompletedProcess, report: dict, pin: dict, profile: dict) -> bool:
    report_pin = {**pin, "expected_verified": profile["expected_verified"],
                  "target_functions": profile["target_functions"]}
    return (type(profile["expected_verified"]) is int and profile["expected_verified"] > 0
            and proc.returncode == 0 and verifier.accepted(report, report_pin)
            and verifier.proof_succeeded(report, pin)
            and not verifier.ERROR_HEADER.search(proc.stderr)
            and not verifier.NON_SEMANTIC.search(proc.stderr))


def classify_mutation(proc: subprocess.CompletedProcess, report: dict, name: str,
                      source: str, specimen: Path, pin: dict, profile: dict) -> bool:
    if name not in SEMANTIC_TARGETS:
        return False
    region = verifier.function_lines(source, SEMANTIC_TARGETS[name])
    return verifier.semantic_refusal(proc, report, pin, specimen / SUBJECT, ROOT,
        region=region, target_functions=profile["target_functions"])


def retained_run(command: list[str], directory: Path, environment: dict,
                 timeout: int = 900) -> subprocess.CompletedProcess:
    directory.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    record = {"command": command, "cwd": str(ROOT), "timeout_seconds": timeout}
    try:
        result = verifier.run(command, ROOT, environment, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        stdout, stderr = error.stdout or "", error.stderr or ""
        if isinstance(stdout, bytes):
            stdout = stdout.decode(errors="replace")
        if isinstance(stderr, bytes):
            stderr = stderr.decode(errors="replace")
        result = subprocess.CompletedProcess(command, None, stdout, stderr)
        record["timed_out"] = True
    record.update({"exit_code": result.returncode,
                   "elapsed_seconds": time.monotonic() - started})
    (directory / "command.json").write_text(json.dumps(record, indent=2) + "\n")
    (directory / "stdout.log").write_text(result.stdout)
    (directory / "stderr.log").write_text(result.stderr)
    return result


def parse_report(proc: subprocess.CompletedProcess) -> dict:
    try:
        result = json.loads(proc.stdout)
    except ValueError:
        return {}
    return result if isinstance(result, dict) else {}


def check(cache: Path, install: bool, directory: Path) -> dict:
    if (platform.system(), platform.machine()) != ("Linux", "x86_64"):
        raise RuntimeError("this decoder profile qualifies Linux x86-64")
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    audit_trust(ROOT, profile)
    before = snapshot()
    (directory / "source_sha256.json").write_text(json.dumps(before, indent=2, sort_keys=True) + "\n")
    qualified = verifier.prepare_tools(cache, pin, install)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("VERUS_", "VARGO_"))
                   and key not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    environment["RUSTUP_TOOLCHAIN"] = pin["rust_toolchain"]
    environment["VERUS_Z3_PATH"] = str(qualified / "z3")
    # --no-cheating rejects the five explicitly permitted std contracts too.
    # The source audit admits only the exact reviewed std module; all native
    # bodies, termination, lifetime checking and erasure remain enabled.
    command = [str(qualified / "verus"), "--crate-type=lib", "--edition=2024",
               "--no-external-by-default", "--num-threads", "2", "--rlimit", str(profile["rlimit"]),
               "-V", "spinoff-all", "--output-json"]
    positive = retained_run([*command, str(HARNESS)], directory / "normal", environment)
    report = parse_report(positive)
    if not accepted(positive, report, pin, profile):
        raise RuntimeError("decoder proof lacks exact pinned tools, complete coverage or the expected obligation count")
    print("Value decoder: normal proof passed", flush=True)

    native_binary = directory / "std-contract-tests"
    native_build = retained_run(["rustc", f"+{pin['runtime_rust']}", "--edition=2024", "--test",
        str(STD_TESTS), "-o", str(native_binary)], directory / "std-build", environment)
    verifier.require_success(native_build)
    native = retained_run([str(native_binary)], directory / "std-samples", environment)
    verifier.require_success(native)

    controls = []
    for name, changed in mutation_sources((ROOT / SUBJECT).read_text()).items():
        specimen = directory / name / "source"
        for path in UNIT_SOURCES:
            target = specimen / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(changed.encode() if path == SUBJECT else (ROOT / path).read_bytes())
        mutant = retained_run([*command, str(specimen / HARNESS)], directory / name / "run", environment)
        parsed = parse_report(mutant)
        rejected = classify_mutation(mutant, parsed, name, changed, specimen, pin, profile)
        control = {"name": name, "rejected_semantically": rejected,
                   "obligation_function": SEMANTIC_TARGETS[name], "exit_code": mutant.returncode,
                   "normal_sha256": before[str(SUBJECT)], "mutant_sha256": verifier.digest(specimen / SUBJECT),
                   "original_after_sha256": verifier.digest(ROOT / SUBJECT),
                   "verification_results": parsed.get("verification-results", {})}
        controls.append(control)
        (directory / "controls.json").write_text(json.dumps(controls, indent=2) + "\n")
        if not rejected:
            raise RuntimeError(f"decoder control {name} survived or failed for an unrelated reason")
        print(f"Value decoder: rejected {name} at {SEMANTIC_TARGETS[name]}", flush=True)

    if before != snapshot():
        raise RuntimeError("decoder evidence sources changed during verification")
    restored = retained_run([*command, str(HARNESS)], directory / "restored", environment)
    restored_report = parse_report(restored)
    if not accepted(restored, restored_report, pin, profile):
        raise RuntimeError("unchanged production source failed the restored proof run")
    verifier.verify_tool_files(qualified, pin)
    if before != snapshot():
        raise RuntimeError("decoder evidence sources changed during the restored run")
    return {"schema": "zeno-fcis/value-decoder-evidence/1", "status": "passed",
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "working_tree_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()),
        "source_sha256": before, "toolchain": pin, "verus_report": report,
        "restored_verus_report": restored_report, "controls": controls,
        "std_native_exit_code": native.returncode,
        "trusted_base": profile["trusted_base"], "scope": profile["scope"],
        "unproved": profile["unproved"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--cache", type=Path, default=verifier.cache_directory())
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="value-decoder-run-", dir=args.out.parent)).resolve()
    try:
        receipt = check(args.cache.resolve(), args.install, directory)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        receipt = {"schema": "zeno-fcis/value-decoder-evidence/1", "status": "failed", "error": str(error)}
    receipt["logs"] = str(directory)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"Value decoder: {receipt['status']}; receipt: {args.out}")
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
