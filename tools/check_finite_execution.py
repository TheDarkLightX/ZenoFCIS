#!/usr/bin/env python3
"""Verify the shared finite admission/evaluator and reject missing coverage.

Verus checks the actual source used by synthesis and the finite decision route.
The receipt is scoped Verus/Z3 evidence, not an authorization or a V2 release.
"""

from __future__ import annotations

import argparse
from contextlib import nullcontext
import json
import re
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import zipfile

from v2_proof_sources import execution_sources
import scalar_arithmetic_controls as arithmetic_controls
from v2_native_dependencies import native_dependency_args
import check_verus as verifier
from verus_coverage import profile_inventory, require_coverage

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/finite-execution.json")
HARNESS = Path("verification/verus/finite_execution.rs")
SUBJECT = Path("crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs")
ADMISSION = SUBJECT.parent / "admission/mod.rs"
TYPED = SUBJECT.parent / "typed.rs"
VALUE = SUBJECT.parent.parent / "value/mod.rs"
UNIT_SOURCES = (HARNESS, SUBJECT, SUBJECT.parent / "spec.rs", ADMISSION,
                ADMISSION.parent / "spec.rs", TYPED, VALUE)
# The legacy instructions run on the typed evaluator, whose values include the
# V2 compound value, so the harness compiles the registered execution closure.
UNIT_SOURCES = tuple(dict.fromkeys((*UNIT_SOURCES, *execution_sources(ROOT))))
SOURCES = (*UNIT_SOURCES, PROFILE, verifier.PIN,
           Path("verification/verus/finite_execution_tests.rs"),
           Path("verification/verus/admission_baseline.rs"),
           Path("verification/verus/admission_differential_tests.rs"),
           Path("tools/check_finite_execution.py"), Path("tools/verus_coverage.py"),
           Path("tools/scalar_arithmetic_controls.py"), Path("tools/test_scalar_arithmetic_controls.py"),
           Path("tools/scalar_arithmetic_reference.py"), Path("tools/test-data/scalar-arithmetic/vectors.tsv"),
           Path("tools/check_verus.py"), Path("tools/test_check_finite_execution.py"),
           Path("Cargo.toml"), Path("rust-toolchain.toml"),
           Path("crates/zeno-fcis-synthesis/src/finite/ir.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite/ir/program.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite/mod.rs"),
           Path("crates/zeno-fcis-synthesis/src/finite_runtime.rs"),
           Path("crates/zeno-fcis-cli/templates/order-fulfillment/synthesized/transition.rs"),
           Path("tools/v2_proof_sources.py"), Path("verification/verus/authority_v2_sources.json"),
           Path("tools/v2_native_dependencies.py"))

SEMANTIC_TARGETS = {
    'wrong_boolean_encoding': 'legacy_instruction', 'subtract_instead_of_add': 'arithmetic',
    'invert_equality': 'node', 'swap_select_arms': 'node',
    'skip_last_eager_node': 'evaluate_nodes', 'wrong_output_projection': 'evaluate_nodes',
    'bypass_input_domain': 'evaluate_nodes', 'bypass_output_domain': 'evaluate_nodes',
    'expose_partial_results': 'evaluate_into', 'accept_wrong_operand_kind': 'require_node',
    'admit_excess_node_count': 'validate_shape', 'accept_wrong_root_kind': 'validate_roots',
}
SEMANTIC_TARGETS.update(arithmetic_controls.targets())



def snapshot() -> dict[str, str]:
    for path in SOURCES:
        if (ROOT / path).is_symlink():
            raise RuntimeError(f"proof source is a symbolic link: {path}")
    return {str(path): verifier.digest(ROOT / path) for path in SOURCES}


def once(source: str, before: str, after: str) -> str:
    """Replace the single token-equal occurrence of `before`.

    Tokens must match exactly and in order; whitespace between them may differ,
    so reformatting does not hide or duplicate an anchor.
    """
    tokens = re.findall(r"\w+|[^\w\s]", before)
    matches = list(re.finditer(r"\s*".join(re.escape(t) for t in tokens), source))
    if len(matches) != 1:
        raise RuntimeError(f"mutation anchor changed: {before}")
    found = matches[0]
    return source[:found.start()] + after + source[found.end():]


def final_contract(source: str, replacement: str) -> str:
    end = source.index("pub(super) fn evaluate_into(")
    start = source.rindex("#[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    return source[:start] + replacement + source[end:]


def mutation_sources(execution: str, admission: str, typed: str, value: str) -> dict[str, tuple[Path, str, str]]:
    mutations = {}
    # The legacy instruction semantics moved into the typed evaluator and the
    # width-explicit arithmetic; these controls change the legacy cases there.
    for name, path, source, before, after in (
        ("wrong_boolean_encoding", TYPED, typed,
         "Op::Bool(v) => Instruction::Literal(Atom::I128(if v { 1 } else { 0 }), Kind::Bool),",
         "Op::Bool(v) => Instruction::Literal(Atom::I128(if v { 0 } else { 1 }), Kind::Bool),"),
        ("subtract_instead_of_add", VALUE, value,
         "ArithmeticOp::Add => a.checked_add(b), ArithmeticOp::Sub => a.checked_sub(b), }; "
         "match computed { Some(value) => {",
         "ArithmeticOp::Add => a.checked_sub(b), ArithmeticOp::Sub => a.checked_sub(b), }; "
         "match computed { Some(value) => {"),
        ("invert_equality", TYPED, typed,
         "(Atom::I128(a), Atom::I128(b)) => Ok(boolean(encoding, a == b)),",
         "(Atom::I128(a), Atom::I128(b)) => Ok(boolean(encoding, a != b)),"),
        ("swap_select_arms", TYPED, typed,
         "Ok(Atom::I128(if c == 1 { a } else { b }))", "Ok(Atom::I128(if c == 1 { b } else { a }))"),
    ):
        mutations[name] = (path, once(source, before, after), "proof")
    for name, before, after in (
        ("skip_last_eager_node", "while index < nodes.len() {",
         "while index < nodes.len() && index != nodes.len() - 1 {"),
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
    mutations.update(arithmetic_controls.specimens(ROOT))
    return mutations


def final_contract_attribute(source: str) -> str:
    end = source.index("pub(super) fn evaluate_into(")
    start = source.rindex("#[cfg_attr(verus_keep_ghost, verus_spec(result =>", 0, end)
    return once(source[start:end], "    ensures match result {",
                "    requires input@.len() == 0,\n    ensures match result {")


def coverage_control(vir: str, profile: dict, name: str) -> tuple[bool, str | None]:
    """The exact planned function set and intended contract must change."""
    try:
        require_coverage(vir, profile)
    except ValueError as error:
        refusal = str(error)
    else:
        return False, None
    actual = profile_inventory(vir, profile)
    expected = profile['functions']
    prefix = 'finite_execution::evaluation::'
    target = prefix + ('unchecked_identity' if name == 'add_uncontracted_function' else 'evaluate_into')
    if name == 'add_uncontracted_function':
        intended = (set(actual) == set(expected) | {target} and target not in expected
                    and actual[target]['mode'] == 'Exec'
                    and actual[target]['requires'] == 0 and actual[target]['ensures'] == 0)
    else:
        # Contract edits can shift Verus expression IDs in later declarations.
        # Grade the intended target record even when that raw mismatch is reported first.
        field = 'requires_sha256' if name == 'narrow_execution_domain' else 'ensures_sha256'
        intended = (set(actual) == set(expected) and target in expected
                    and actual[target]['mode'] == 'Exec'
                    and actual[target][field] != expected[target][field]
                    and ((name == 'omit_execution_contract' and actual[target]['ensures'] == 0)
                         or (name == 'weaken_execution_contract' and actual[target]['ensures'] == 1)
                         or (name == 'narrow_execution_domain' and actual[target]['requires'] == 1)))
    return intended, refusal


def proof_rejected(proc, report, pin, profile, name, path, changed, specimen):
    # Normalize the exact pinned loop-entry header; unrelated errors still refuse.
    stderr = re.sub(r'^error: invariant not satisfied before loop$',
                    'error: invariant not satisfied', proc.stderr, flags=re.M)
    diagnostic = subprocess.CompletedProcess(proc.args, proc.returncode, proc.stdout, stderr)
    return verifier.semantic_refusal(diagnostic, report, pin, path, specimen,
        region=verifier.function_lines(changed, SEMANTIC_TARGETS[name]),
        target_functions=profile['target_functions'])


def native_tests(directory: Path, pin: dict, environment: dict) -> subprocess.CompletedProcess:
    """Build and run the harness tests natively against fresh codec/value oracles."""
    executable = directory / "finite-tests"
    native_command = ["rustc", f"+{pin['runtime_rust']}", "--edition=2024", "--test",
                      "--check-cfg", "cfg(verus_keep_ghost)", "--check-cfg", "cfg(test)",
                      *native_dependency_args(ROOT, directory, environment),
                      str(HARNESS), "-o", str(executable)]
    compiled = verifier.run(native_command, ROOT, environment)
    (directory / "native-compile.stdout").write_text(compiled.stdout)
    (directory / "native-compile.stderr").write_text(compiled.stderr)
    (directory / "native-compile.json").write_text(json.dumps({
        "command": native_command, "cwd": str(ROOT), "exit": compiled.returncode,
    }, indent=2) + "\n")
    verifier.require_success(compiled)
    native = verifier.run([str(executable)], ROOT, environment)
    (directory / "native.stdout").write_text(native.stdout)
    (directory / "native.stderr").write_text(native.stderr)
    (directory / "native-process.json").write_text(json.dumps({
        "command": [str(executable)], "cwd": str(ROOT), "exit": native.returncode,
    }, indent=2) + "\n")
    verifier.require_success(native)
    return native


def check(cache: Path, install: bool, retained_directory: Path | None = None) -> dict:
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
               "--no-cheating", "--no-external-by-default", "--num-threads", verifier.proof_threads('2'), "-V", "spinoff-all", "--output-json",
               "--log", "vir", "--log", "vir-option=no_span+no_type+no_fn_details"]
    mutations = []
    context = (nullcontext(str(retained_directory)) if retained_directory is not None
               else tempfile.TemporaryDirectory(prefix="zeno-fcis-finite-proof-"))
    with context as temporary:
        directory = Path(temporary)
        (directory / "source_sha256.json").write_text(json.dumps(before, indent=2, sort_keys=True) + "\n")
        positive_command = [*command, "--log-dir", str(directory / "positive"), str(HARNESS)]
        positive = verifier.run(positive_command, ROOT, environment)
        (directory / "positive.json").write_text(positive.stdout)
        (directory / "positive.stderr").write_text(positive.stderr)
        (directory / "positive-process.json").write_text(json.dumps({
            "command": positive_command, "cwd": str(ROOT), "returncode": positive.returncode,
        }, indent=2, sort_keys=True) + "\n")
        verifier.require_success(positive)
        report = json.loads(positive.stdout)
        if not verifier.accepted(report, report_pin):
            raise RuntimeError("finite proof report lacks pinned tool or complete obligation coverage")
        coverage = require_coverage((directory / "positive/crate.vir").read_text(), profile)
        native = native_tests(directory, pin, environment)
        print(f"Finite execution: {profile['expected_verified']} obligations and native boundaries passed", flush=True)
        for name, (path, changed, expected) in mutation_sources(
                (ROOT / SUBJECT).read_text(), (ROOT / ADMISSION).read_text(),
                (ROOT / TYPED).read_text(), (ROOT / VALUE).read_text()).items():
            specimen = directory / name
            for unit in UNIT_SOURCES:
                target = specimen / unit
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(changed) if unit == path else target.write_bytes((ROOT / unit).read_bytes())
            mutant_command = [*command, "--log-dir", str(specimen / "logs"), str(HARNESS)]
            mutant = verifier.run(mutant_command, specimen, environment)
            (specimen / "verus.json").write_text(mutant.stdout)
            (specimen / "verus.stderr").write_text(mutant.stderr)
            (specimen / "verus-process.json").write_text(json.dumps({
                "command": mutant_command, "cwd": str(specimen), "returncode": mutant.returncode,
            }, indent=2, sort_keys=True) + "\n")
            mutant_report = json.loads(mutant.stdout)
            result = mutant_report.get("verification-results", {})
            coverage_error = None
            if expected == "proof":
                killed = proof_rejected(mutant, mutant_report, pin, profile,
                    name, path, changed, specimen)
            else:
                intended, coverage_error = coverage_control(
                    (specimen / "logs/crate.vir").read_text(), profile, name)
                killed = (mutant.returncode == 0 and verifier.proof_succeeded(mutant_report, pin)
                          and intended
                          and not verifier.ERROR_HEADER.search(mutant.stderr)
                          and not verifier.NON_SEMANTIC.search(mutant.stderr))
            mutations.append({"name": name, "expected_failure": expected, "killed": killed,
                              "exit_code": mutant.returncode, "verification_results": result,
                              "coverage_refusal": coverage_error, "logs": str(specimen)})
            (directory / "mutations.json").write_text(json.dumps(mutations, indent=2) + "\n")
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
    parser.add_argument("--cache", type=Path, default=verifier.cache_directory())
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="finite-execution-run-", dir=args.out.parent))
    try:
        receipt = check(args.cache.resolve(), args.install, directory)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        receipt = {"schema": "zeno-fcis/finite-execution-evidence/1", "status": "failed", "error": str(error)}
    receipt["logs"] = str(directory)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"Finite execution: {receipt['status']}; receipt: {args.out}")
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    runtime = sum(record["mode"] == "Exec" for record in receipt["translated_function_coverage"].values())
    obligations = receipt["verus_report"]["verification-results"]["verified"]
    print(f"{runtime} runtime bodies; {obligations} obligations; {len(receipt['mutations'])} mutations caught")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
