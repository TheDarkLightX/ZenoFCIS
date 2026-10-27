#!/usr/bin/env python3
"""Verify the production V2 integer byte readers, not schema admission.

Contracts cover every byte slice, offset and requested width. Native standard-
library comparisons and mutation controls supplement the source-bound proof.
This unit does not implement a protected view or authorize a transition.
"""
from __future__ import annotations

from qualification_controls import ControlRefused, run_controls

import argparse
from contextlib import nullcontext
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tempfile
import zipfile

from v2_proof_sources import execution_sources, adjust_specimen_source_lengths
from v2_native_dependencies import native_dependency_args, native_test_environment
import check_verus as verifier
from check_finite_execution import once
from verus_coverage import parse_vir, profile_inventory, require_coverage
from verus_contract_controls import contract_rejected

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/canonical-bytes.json")
HARNESS = Path("verification/verus/canonical_bytes.rs")
SUBJECT = Path("crates/zeno-fcis-synthesis/src/finite/canonical_v2/mod.rs")
SPEC = SUBJECT.parent / "spec.rs"
ENVELOPE = SUBJECT.parent / "envelope.rs"
SCHEMA = SUBJECT.parent / "schema.rs"
SEMANTIC_TARGETS = {
    **dict.fromkeys(("reverse_byte_order", "wrong_radix", "wrong_byte_offset",
                    "refuse_empty_width", "refuse_full_width", "permit_excessive_width",
                    "wrong_next_offset"), (SUBJECT, "read_big_endian")),
    **dict.fromkeys(("wrong_negative_sign", "wrong_signed_boundary"),
                   (SUBJECT, "read_signed_128")),
}
# Exact production callers demonstrated by the pinned selected narrow-domain run.
CONTRACT_REJECTIONS = {
    **dict.fromkeys(("omit_signed_contract", "weaken_signed_contract"), frozenset({
        ("postcondition not satisfied", str(SCHEMA), 150),
        ("postcondition not satisfied", str(SUBJECT.parent.parent / "execution_v2/compound.rs"), 107),
        ("postcondition not satisfied", str(SUBJECT.parent.parent / "execution_v2/input_view/typed.rs"), 208),
    })),
    "narrow_signed_input": frozenset({
        ("precondition not satisfied", str(SCHEMA), 153),
        ("precondition not satisfied", str(SUBJECT.parent.parent / "execution_v2/compound.rs"), 109),
        ("precondition not satisfied", str(SUBJECT.parent.parent / "execution_v2/input_view/typed.rs"), 230),
        ("precondition not satisfied", str(SUBJECT.parent.parent / "execution_v2/input_view/typed.rs"), 289),
    }),
}

COVERAGE_TARGETS = {
    "omit_unused_getter_contract": ("canonical_v2::envelope::impl&%0::original", "ensures_sha256"),
    "weaken_unused_getter_contract": ("canonical_v2::envelope::impl&%0::original", "ensures_sha256"),
    "narrow_unused_getter_domain": ("canonical_v2::envelope::impl&%0::original", "requires_sha256"),
    "change_signed_specification": ("canonical_v2::spec::signed", "body_sha256"),
    "add_uncontracted_function": ("canonical_v2::uncontracted_width", "inventory"),
}
UNIT_SOURCES = (HARNESS, Path("verification/verus/canonical_bytes_tests.rs"),
                *sorted(path.relative_to(ROOT) for path in (ROOT / SUBJECT.parent).rglob("*.rs")))

UNIT_SOURCES = tuple(dict.fromkeys((*UNIT_SOURCES, *execution_sources(ROOT))))
SOURCES = (*UNIT_SOURCES, PROFILE, verifier.PIN,
           Path("tools/check_canonical_bytes.py"), Path("tools/test_check_canonical_bytes.py"),
           Path("tools/verus_coverage.py"), Path("tools/check_verus.py"),
           Path("tools/check_finite_execution.py"), Path("Cargo.toml"),
           Path("rust-toolchain.toml"), Path("crates/zeno-fcis-synthesis/src/finite/mod.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite_runtime.rs"),
           Path("crates/zeno-fcis-synthesis/tests/v2_execution.rs"))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_proof_sources.py"),
                                Path("verification/verus/authority_v2_sources.json"))))

SOURCES = tuple(dict.fromkeys((*SOURCES, Path("tools/v2_native_dependencies.py"),
                                Path("tools/verus_contract_controls.py"))))
SOURCES = tuple(dict.fromkeys((*SOURCES, *verifier.QUALIFICATION_SOURCES)))


def snapshot() -> dict[str, str]:
    for path in SOURCES:
        if (ROOT / path).is_symlink():
            raise RuntimeError(f"byte proof source is a symbolic link: {path}")
    return {str(path): verifier.digest(ROOT / path) for path in SOURCES}


def signed_contract(source: str, replacement: str) -> str:
    end = source.index("pub fn read_signed_128(")
    start = source.rindex("#[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    return source[:start] + replacement + source[end:]


def mutation_sources(source: str, spec: str) -> dict[str, tuple[Path, str, str]]:
    mutations = {}
    for name, before, after in (
        ("reverse_byte_order", "bytes[offset + index]", "bytes[offset + width - 1 - index]"),
        ("wrong_radix", "value.checked_mul(256)", "value.checked_mul(255)"),
        ("wrong_byte_offset", "bytes[offset + index]", "bytes[offset + index + 1]"),
        ("refuse_empty_width", "if width > 16 ||", "if width == 0 || width > 16 ||"),
        ("refuse_full_width", "if width > 16 ||", "if width >= 16 ||"),
        ("permit_excessive_width", "if width > 16 ||", "if false ||"),
        ("wrong_next_offset", "Some((value, offset + width))", "Some((value, offset + width - 1))"),
        ("wrong_negative_sign", "-((u128::MAX - value) as i128) - 1", "-((u128::MAX - value) as i128) + 1"),
        ("wrong_signed_boundary", "value <= i128::MAX as u128", "value < i128::MAX as u128"),
    ):
        mutations[name] = (SUBJECT, once(source, before, after), "proof")
    for name, replacement in (
        ("omit_signed_contract", ""),
        ("weaken_signed_contract", "#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n"),
        ("narrow_signed_input", "#[cfg_attr(verus_keep_ghost, verus_spec(result =>\n"
         "    requires bytes@.len() >= 16,\n"
         "    ensures result == spec::read_signed(bytes@, offset),\n))]\n"),
    ):
        # The schema reader consumes this postcondition. Removing it now
        # fails caller proofs, so retain these as explicit contract controls.
        mutations[name] = (SUBJECT, signed_contract(source, replacement), "proof_contract")
    # Catalog proofs now consume the schema getter's postcondition. The
    # envelope payload's original getter has no verified caller, so changing
    # only its contract must still verify and then fail coverage.
    envelope = (ROOT / ENVELOPE).read_text()
    end = envelope.index("    pub fn original(&self)")
    start = envelope.rindex("    #[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    for name, replacement in (
        ("omit_unused_getter_contract", ""),
        ("weaken_unused_getter_contract", "    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures true,))]\n"),
        ("narrow_unused_getter_domain", "    #[cfg_attr(verus_keep_ghost, verus_spec(result =>\n"
         "        requires self.view().0.len() > 0,\n"
         "        ensures result@ == self.view().0,\n    ))]\n"),
    ):
        mutations[name] = (ENVELOPE, envelope[:start] + replacement + envelope[end:], "coverage")
    mutations["change_signed_specification"] = (SPEC,
        once(spec, "value as int - (u128::MAX as int + 1)",
             "value as int - u128::MAX as int - 1"), "coverage")
    mutations["add_uncontracted_function"] = (SUBJECT,
        source + "\npub fn uncontracted_width(width: usize) -> usize { width }\n", "coverage")
    return mutations


def same_read_identity_structure(left, right, forward, reverse, return_identity=None):
    """Compare exact VIR trees with a bijection of pinned read-place identities."""
    if return_identity is not None and (left == return_identity[0] or right == return_identity[1]):
        return left == return_identity[0] and right == return_identity[1]
    if isinstance(left, list) and isinstance(right, list):
        if (len(left) == len(right) == 5 and left[:2] == right[:2]
                == ["UnfinalizedReadKind", ":preliminary_kind"]
                and left[3] == right[3] == ":id"):
            a, b = left[4], right[4]
            if (not isinstance(a, str) or not isinstance(b, str)
                    or not a.isascii() or not b.isascii()
                    or not a.isdecimal() or not b.isdecimal()
                    or left[2] != right[2]):
                return False
            if a in forward or b in reverse:
                return forward.get(a) == b and reverse.get(b) == a
            forward[a], reverse[b] = b, a
            return True
        return len(left) == len(right) and all(
            same_read_identity_structure(a, b, forward, reverse, return_identity)
            for a, b in zip(left, right))
    return type(left) is type(right) and left == right


def coverage_structure(vir, reference_vir, profile, name):
    """Retain exact positive fingerprints; compare only the owned mutation."""
    require_coverage(reference_vir, profile)
    def records(text):
        return {node[1][2]: dict(zip(node[2::2], node[3::2]))
                for node in parse_vir(text)
                if node and node[0] == "Function"
                and node[1][2].startswith(profile["namespace"])}
    before, after = records(reference_vir), records(vir)
    suffix, field = COVERAGE_TARGETS[name]
    target = "canonical_bytes::" + suffix
    if field == "inventory":
        if set(after) != set(before) | {target}:
            return False
        if (after[target][":mode"] != "Exec" or after[target][":require"]
                or after[target][":ensure"]):
            return False
    elif set(before) != set(after):
        return False
    changed_field = {"ensures_sha256": ":ensure", "requires_sha256": ":require",
                     "body_sha256": ":body"}.get(field)
    return_identity = None
    if name == "narrow_unused_getter_domain":
        # Adding a requires clause renumbers the getter's named ghost result.
        # Compare its binder and references together, within this function only.
        returns = (before[target][":ret"], after[target][":ret"])
        for ret in returns:
            if ret[:2] != ["Param", ":name"]:
                return False
            ident = ret[2]
            if (len(ident) != 3 or ident[:2] != ["VarIdent", '"result"']
                    or len(ident[2]) != 3
                    or ident[2][:2] != ["VarIdentDisambiguate", "RustcId"]
                    or not isinstance(ident[2][2], str)
                    or not ident[2][2].isascii() or not ident[2][2].isdecimal()):
                return False
        return_identity = tuple(ret[2] for ret in returns)
    forward, reverse = {}, {}
    for function in sorted(before):
        if set(before[function]) != set(after[function]):
            return False
        for key in before[function]:
            if function == target and key == changed_field:
                continue
            left, right = before[function][key], after[function][key]
            if function == target and key == ":ret" and name == "omit_unused_getter_contract":
                # Contract removal replaces the unused named ghost result.
                if (left[:2] != ["Param", ":name"] or right[:2] != ["Param", ":name"]
                        or left[2][:2] != ["VarIdent", '"result"']
                        or right[2] != ["VarIdent", '"%return"', ["VarIdentDisambiguate", "AirLocal"]]):
                    return False
                left, right = left[3:], right[3:]
            identity = return_identity if function == target and key in (":ret", ":ensure") else None
            if not same_read_identity_structure(left, right, forward, reverse, identity):
                return False
    if field == "inventory":
        return True
    return not same_read_identity_structure(before[target][changed_field],
        after[target][changed_field], dict(forward), dict(reverse))


def coverage_control(vir: str, profile: dict, name: str,
                     reference_vir: str | None = None) -> tuple[bool, str | None]:
    """Only the declared function, contract field or added probe may differ."""
    try:
        require_coverage(vir, profile)
    except ValueError as error:
        refusal = str(error)
    else:
        return False, None
    try:
        actual = profile_inventory(vir, profile)
    except ValueError:
        return False, refusal
    suffix, field = COVERAGE_TARGETS[name]
    target = "canonical_bytes::" + suffix
    expected = profile["functions"]
    if field == "inventory":
        matched = (set(actual) == set(expected) | {target} and target not in expected
                   and all(actual[key] == expected[key] for key in expected)
                   and actual[target]["mode"] == "Exec" and actual[target]["requires"] == 0
                   and actual[target]["ensures"] == 0)
    else:
        changed = {key for key in expected if key in actual and actual[key] != expected[key]}
        matched = (set(actual) == set(expected) and changed == {target}
                   and actual[target][field] != expected[target][field]
                   and all(actual[target][key] == expected[target][key]
                           for key in ("mode", "signature_sha256")
                           + (("requires_sha256", "ensures_sha256") if field == "body_sha256"
                              else ("body_sha256", "requires_sha256" if field == "ensures_sha256"
                                    else "ensures_sha256"))))
    if not matched and reference_vir is not None:
        try:
            matched = coverage_structure(vir, reference_vir, profile, name)
        except (ValueError, KeyError, TypeError, IndexError, RecursionError):
            matched = False
    return bool(matched and target in refusal), refusal


def classify_mutation(proc, report: dict, name: str, changed: str, specimen: Path,
                      pin: dict, profile: dict, *, vir: str | None = None,
                      reference_vir: str | None = None,
                      timed_out: bool = False) -> tuple[bool, str, str | None]:
    """Operational failures are inconclusive, even beside an intended error."""
    if timed_out or verifier.NON_SEMANTIC.search(proc.stderr) or proc.returncode not in (0, 1):
        return False, "inconclusive", None
    refusal = None
    loop_entries = [m.group(1) for m in verifier.ERROR_HEADER.finditer(proc.stderr)
                    if m.group(1).startswith("invariant not satisfied before loop")]
    if loop_entries and (name != "permit_excessive_width" or any(
            text != "invariant not satisfied before loop" for text in loop_entries)):
        return False, "survived-or-unrelated-failure", None
    if name in SEMANTIC_TARGETS:
        path, function = SEMANTIC_TARGETS[name]
        source = changed if path == SUBJECT else (specimen / path).read_text()
        region = verifier.function_lines(source, function)
        if name == "permit_excessive_width":
            # Exact pinned loop-entry spelling; raw diagnostic evidence stays unchanged.
            stderr = re.sub(r'^error: invariant not satisfied before loop$',
                            'error: invariant not satisfied', proc.stderr, flags=re.M)
            proc = subprocess.CompletedProcess(proc.args, proc.returncode, proc.stdout, stderr)
        elif name == "wrong_byte_offset":
            # Exact pinned bounds spelling; raw diagnostic evidence stays unchanged.
            stderr = re.sub(r'^error: precondition not met: index in bounds for this access$',
                            'error: precondition not satisfied', proc.stderr, flags=re.M)
            proc = subprocess.CompletedProcess(proc.args, proc.returncode, proc.stdout, stderr)
        killed = verifier.semantic_refusal(proc, report, pin, path, specimen,
            region=region, target_functions=profile["target_functions"])
    elif name in CONTRACT_REJECTIONS:
        if not isinstance(report, dict):
            return False, "survived-or-unrelated-failure", None
        outcome = contract_rejected(proc.returncode, proc.stdout, proc.stderr, pin,
                                    CONTRACT_REJECTIONS[name])
        result, tool, functions = (report.get(k, {}) for k in
                                  ("verification-results", "verus", "func-details"))
        count = len(outcome["observed"])
        headers = list(verifier.ERROR_HEADER.finditer(proc.stderr))
        killed = (outcome["accepted"]
            and all(isinstance(v, dict) for v in (result, tool, functions))
            and str(tool.get("toolchain", "")).split(" ")[0] == pin["rust_toolchain"]
            and result.get("encountered-error") is True
            and type(result.get("verified")) is int and result["verified"] > 0
            and result["errors"] <= count
            and set(profile["target_functions"]).issubset(functions)
            and len(headers) == count + 1
            and headers[-1][1] == f"aborting due to {count} previous errors")
    else:
        killed = False
        if (proc.returncode == 0 and verifier.proof_succeeded(report, pin)
                and not verifier.ERROR_HEADER.search(proc.stderr) and vir is not None):
            killed, refusal = coverage_control(vir, profile, name, reference_vir)
    return bool(killed), "killed" if killed else "survived-or-unrelated-failure", refusal


def native_checks(directory: Path, pin: dict, environment: dict) -> dict:
    environment = native_test_environment(ROOT, environment)
    executable = directory / "canonical-byte-tests"
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
        raise RuntimeError("this byte proof profile requires qualified Linux x86-64 tools")
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
               else tempfile.TemporaryDirectory(prefix="zeno-fcis-byte-proof-"))
    with context as temporary:
        directory = Path(temporary)
        (directory / "source_sha256.json").write_text(json.dumps(before, indent=2, sort_keys=True) + "\n")
        positive_command = [*command, "--log-dir", str(directory / "positive"), str(HARNESS)]
        positive = verifier.run(positive_command, ROOT, environment)
        (directory / "positive.json").write_text(positive.stdout)
        (directory / "positive.stderr").write_text(positive.stderr)
        verifier.require_success(positive)
        report = json.loads(positive.stdout)
        if (not verifier.accepted(report, report_pin) or not verifier.proof_succeeded(report, pin)
                or verifier.ERROR_HEADER.search(positive.stderr)
                or verifier.NON_SEMANTIC.search(positive.stderr)):
            raise RuntimeError("byte proof report lacks pinned tools or complete obligation coverage")
        reference_vir = (directory / "positive/crate.vir").read_text()
        coverage = require_coverage(reference_vir, profile)
        native = native_checks(directory, pin, environment)
        print("V2 bytes: exact proof coverage and independent native comparisons passed", flush=True)
        def run_control(control_item):
            (name, (path, changed, expected)) = control_item
            specimen = directory / name
            for unit in UNIT_SOURCES:
                target = specimen / unit
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(changed.encode("utf-8") if unit == path else (ROOT / unit).read_bytes())
            adjust_specimen_source_lengths(specimen)
            mutant_command = [*command, "--log-dir", str(specimen / "logs"), str(specimen / HARNESS)]
            timed_out = False
            try:
                mutant = verifier.run(mutant_command, ROOT, environment, timeout=900)
            except subprocess.TimeoutExpired as error:
                stdout, stderr = error.stdout or "", error.stderr or ""
                if isinstance(stdout, bytes):
                    stdout = stdout.decode(errors="replace")
                if isinstance(stderr, bytes):
                    stderr = stderr.decode(errors="replace")
                mutant = subprocess.CompletedProcess(mutant_command, None, stdout, stderr)
                timed_out = True
            (specimen / "verus.json").write_text(mutant.stdout)
            (specimen / "verus.stderr").write_text(mutant.stderr)
            (specimen / "verus-process.json").write_text(json.dumps({
                "command": mutant_command, "cwd": str(ROOT), "returncode": mutant.returncode,
                "timeout_seconds": 900, "timed_out": timed_out}, indent=2) + "\n")
            try:
                parsed = json.loads(mutant.stdout)
            except ValueError:
                parsed = {}
            result = parsed.get("verification-results", {}) if isinstance(parsed, dict) else {}
            vir_path = specimen / "logs/crate.vir"
            vir = vir_path.read_text() if expected == "coverage" and vir_path.is_file() else None
            killed, classification, refusal = classify_mutation(
                mutant, parsed, name, changed, specimen, pin, profile, vir=vir,
                reference_vir=reference_vir, timed_out=timed_out)
            control_result = {"name": name, "expected_failure": expected, "killed": killed,
                              "classification": classification, "timed_out": timed_out,
                              "changed_source": str(path), "changed_sha256": verifier.digest(specimen / path),
                              "exit_code": mutant.returncode, "verification_results": result,
                              "coverage_refusal": refusal, "logs": str(specimen)}
            if not killed:
                raise ControlRefused(control_result, f"byte mutation {name}: {classification}; inspect retained diagnostics")
            print(f"V2 bytes: caught {name} ({expected})", flush=True)
            return control_result
        def publish_control(control_result):
            mutations.append(control_result)
            (directory / "mutations.json").write_text(json.dumps(mutations, indent=2) + "\n")
        run_controls(run_control, mutation_sources((ROOT / SUBJECT).read_text(), (ROOT / SPEC).read_text()).items(), timings=directory / 'control-timings.json', publish=publish_control)
    verifier.verify_tool_files(tools, pin)
    if before != snapshot():
        raise RuntimeError("byte proof or integration source changed during verification")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
    return {"schema": "zeno-fcis/canonical-byte-evidence/1", "status": "passed",
            "revision": revision, "working_tree_dirty": bool(dirty), "source_sha256": before, "source_bindings": verifier.evidence_bindings(before, HARNESS),
            "toolchain": pin, "command": positive_command, "exit_code": positive.returncode,
            "verus_report": report, "translated_function_coverage": coverage,
            "native": native, "mutations": mutations,
            "scope": "exact unsigned base-256 and signed i128 interpretation; full current canonical module dependency coverage",
            "trusted_base": ["reviewed specifications and translated coverage manifest",
                             "Verus translation/erasure and bundled vstd/Z3", "Rust compilers",
                             "standard library", "host platform"],
            "unproved": ["schema/record/envelope admission", "protected raw-state view",
                         "metered decoding", "complete decisions", "law/genesis checks",
                         "mandatory V2 authority and replay", "source-to-binary correspondence", "shell"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--cache", type=Path, default=verifier.cache_directory())
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="canonical-byte-run-", dir=args.out.parent))
    try:
        receipt = check(args.cache.resolve(), args.install, directory)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        receipt = {"schema": "zeno-fcis/canonical-byte-evidence/1", "status": "failed", "error": str(error)}
    receipt["logs"] = str(directory)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"V2 bytes: {receipt['status']}; receipt: {args.out}")
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
