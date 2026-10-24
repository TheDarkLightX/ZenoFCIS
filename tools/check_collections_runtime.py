#!/usr/bin/env python3
"""Qualify the production collections frame/write-back closure with pinned Verus.

The two abstract Entries declarations are the storage-read premise. All other
owned VIR functions must have bodies, exact contracts, and reviewed fingerprints.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tempfile

import check_verus as verifier
from verus_coverage import _inventory, parse_vir

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path("verification/verus/collections_runtime.rs")
PROFILE = Path("verification/verus/collections-runtime.json")
RUNTIME = Path("crates/zeno-fcis-contract/src/collections/runtime.rs")
ABSENT = RUNTIME.with_name("absent.rs")
NAMESPACE = "collections_runtime::"
ABSTRACT = {
    NAMESPACE + "runtime::Entries::contents": "Spec",
    NAMESPACE + "runtime::Entries::record": "Exec",
}
PRODUCTION = (
    RUNTIME, ABSENT, RUNTIME.with_name("runtime_codec.rs"),
    Path("crates/zeno-fcis-contract/src/contract_error.rs"),
    Path("crates/zeno-fcis-contract/src/internals/declarations/source.rs"),
    Path("crates/zeno-fcis-contract/src/internals/review/domain/leaf.rs"),
)
NATIVE_OUTSIDE = {
    RUNTIME: frozenset(("read", "positions", "capacities", "scalar_state", "decode_input",
                       "encode_input", "named", "interface", "successor", "check_successor",
                       "decode_key", "source_digest", "into_parts")),
    PRODUCTION[3]: frozenset(("fmt",)),
}
SOURCES = (*PRODUCTION, HARNESS, PROFILE,
    Path("crates/zeno-fcis-contract/src/collections.rs"),
    Path("crates/zeno-fcis-contract/src/collections/judges.rs"),
    Path("crates/zeno-fcis-contract/src/collections/runtime_proof_tests.rs"),
    Path("crates/zeno-fcis-contract/src/internals/declarations.rs"),
    Path("crates/zeno-fcis-contract/src/internals/review/domain.rs"),
    Path("crates/zeno-fcis-contract/src/lib.rs"),
    Path("crates/zeno-fcis-contract/src/checked_composition/identity.rs"),
    Path("tools/check_collections_runtime.py"), Path("tools/test_check_collections_runtime.py"),
    Path("tools/check_verus.py"), Path("tools/verus_coverage.py"), verifier.PIN,
    Path("Cargo.lock"), Path("Cargo.toml"), Path("rust-toolchain.toml"))


def masked(source: str) -> str:
    """Keep offsets and newlines while removing Rust comments and literals."""
    chars = list(source)
    i, length = 0, len(source)
    def blank(a, b):
        for p in range(a, b):
            if chars[p] != "\n": chars[p] = " "
    while i < length:
        start = i
        if source.startswith("//", i):
            end = source.find("\n", i)
            i = length if end < 0 else end
            blank(start, i)
        elif source.startswith("/*", i):
            i, depth = i + 2, 1
            while i < length and depth:
                if source.startswith("/*", i): depth, i = depth + 1, i + 2
                elif source.startswith("*/", i): depth, i = depth - 1, i + 2
                else: i += 1
            if depth: raise ValueError("unclosed Rust comment")
            blank(start, i)
        elif source[i] == '"':
            i += 1
            while i < length:
                if source[i] == "\\": i += 2
                elif source[i] == '"': i += 1; break
                else: i += 1
            else: raise ValueError("unclosed Rust string")
            blank(start, i)
        elif source[i] == "r" and (match := re.match(r'r(#+)?"', source[i:])):
            hashes = match[1] or ""
            end = source.find('"' + hashes, i + len(match[0]))
            if end < 0: raise ValueError("unclosed raw Rust string")
            i = end + 1 + len(hashes)
            blank(start, i)
        elif source[i] == "'" and (match := re.match(r"'(?:\\.|[^'\\\n])'", source[i:])):
            i += len(match[0]); blank(start, i)
        else:
            i += 1
    return "".join(chars)


def bodies(source: str):
    code = masked(source)
    for match in re.finditer(r"\bfn\s+([A-Za-z_][A-Za-z_0-9]*)\b", code):
        start = match.end()
        brace = code.find("{", start)
        semicolon = code.find(";", start)
        if brace < 0 or (0 <= semicolon < brace): continue
        depth, end = 1, brace + 1
        while end < len(code) and depth:
            depth += (code[end] == "{") - (code[end] == "}")
            end += 1
        if depth: raise ValueError(f"unclosed function: {match[1]}")
        yield match[1], code[brace + 1:end - 1]


def guard_text(source: str, excluded=frozenset()) -> list[str]:
    code = masked(source)
    if re.search(r"\b(?:assume|admit|external_body|unsafe|exec_allows_no_decreases_clause|assume_termination|decreases_when)\b|verifier\s*::\s*external\b|assume_specification|\bmacro_rules\b", code):
        raise ValueError("forbidden owned-code trust or termination exemption")
    inspected = []
    for name, body in bodies(source):
        if name in excluded: continue
        inspected.append(name)
        if re.search(r"\bcfg\s*!", body): raise ValueError(f"native-dependent expression in {name}")
        attributes = []
        for match in re.finditer(r"#\s*\[\s*(cfg|cfg_attr)\b", body):
            depth, end = 1, match.end()
            while end < len(body) and depth:
                depth += (body[end] == "[") - (body[end] == "]")
                end += 1
            if depth: raise ValueError(f"unclosed conditional attribute in {name}")
            attribute = body[match.start():end]
            compact = re.sub(r"\s+", "", attribute)
            tail = body[end:].lstrip()
            if match[1] == "cfg":
                if compact != "#[cfg(verus_keep_ghost)]" or not re.match(r"(?:proof|proof_decl)\s*!", tail):
                    raise ValueError(f"conditional executable code in {name}")
            elif not compact.startswith("#[cfg_attr(verus_keep_ghost,verus_spec("):
                raise ValueError(f"conditional executable attribute in {name}")
            attributes.append((match.start(), end, attribute))
        for loop in re.finditer(r"\b(while|for|loop)\b", body):
            preceding = [item for item in attributes if item[1] <= loop.start()
                         and not body[item[1]:loop.start()].strip()]
            if loop[1] != "while" or not preceding or not re.search(r"\bdecreases\b", preceding[-1][2]):
                raise ValueError(f"loop without checked decrease in {name}")
    return inspected


def source_guard(root: Path) -> dict:
    return {str(path): guard_text((root / path).read_text(), NATIVE_OUTSIDE.get(path, frozenset()))
            for path in PRODUCTION}


def harness_bindings(root: Path) -> list[str]:
    harness = root / HARNESS
    paths = re.findall(r'#\[\s*path\s*=\s*"([^"\n]+)"\s*\]', harness.read_text())
    bound = [(harness.parent / path).resolve() for path in paths]
    expected = [(root / path).resolve() for path in PRODUCTION]
    if len(bound) != len(expected) or set(bound) != set(expected):
        raise ValueError("harness does not include exactly the production proof closure")
    return sorted(str(path.relative_to(root.resolve())) for path in bound)


def coverage(source: str) -> dict:
    nodes = parse_vir(source)
    names = tuple(node[1][2] for node in nodes if node and node[0] == "Function"
                  and node[1][2].startswith(NAMESPACE) and node[1][2] not in ABSTRACT)
    functions, declarations = _inventory(source, NAMESPACE, names, ABSTRACT, nodes)
    return {"functions": functions, "abstract_declarations": declarations}


def snapshot(root: Path) -> dict:
    result = {}
    for path in SOURCES:
        if (root / path).is_symlink(): raise ValueError(f"symbolic proof source: {path}")
        result[str(path)] = verifier.digest(root / path)
    return result


def once(source: str, before: str, after: str) -> str:
    if source.count(before) != 1: raise ValueError("mutation anchor changed or is not unique")
    return source.replace(before, after)


def check(out: Path, cache: Path, install: bool, controls: bool) -> dict:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError("qualified runtime target is Linux x86-64")
    out.mkdir(parents=True, exist_ok=False)
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    before = snapshot(ROOT)
    bindings = harness_bindings(ROOT)
    inspected = source_guard(ROOT)
    if inspected != profile["source_functions"] or verifier.digest(ROOT / HARNESS) != profile["harness_sha256"]:
        raise ValueError("reviewed harness or source function inventory changed")
    tools = verifier.prepare_tools(cache, pin, install)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("VERUS_", "VARGO_"))
                   and key not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    environment.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(tools / "z3"))
    command = [str(tools / "verus"), "--crate-type=lib", "--edition=2024", "--no-cheating",
               "--no-external-by-default", "--multiple-errors", "5", "--rlimit", "30",
               "--triggers-mode", "silent", "--output-json", "--log", "vir",
               "--log", "vir-option=no_span+no_type+no_fn_details"]
    receipt = {"schema": "zeno-fcis/collections-runtime-proof/1", "source_sha256": before,
               "source_guard": inspected, "production_path_bindings": bindings, "commands": [], "controls": [],
               "trusted_storage_port": list(ABSTRACT), "toolchain": pin["version"]}
    def save(): (out / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    def run(label, args, cwd):
        process = verifier.run(args, cwd, environment, timeout=1200)
        (out / (label + ".stdout")).write_text(process.stdout)
        (out / (label + ".stderr")).write_text(process.stderr)
        receipt["commands"].append({"label": label, "command": args, "exit": process.returncode,
            "stdout_sha256": verifier.digest(out / (label + ".stdout")),
            "stderr_sha256": verifier.digest(out / (label + ".stderr"))})
        save()
        return process
    def verify(label, root, positive=True):
        log = out / (label + "-vir")
        process = run(label, command + ["--log-dir", str(log), str(HARNESS)], root)
        report = json.loads(process.stdout)
        if positive:
            verifier.require_success(process)
            if not verifier.proof_succeeded(report, pin) or report["verification-results"]["verified"] != profile["verified_obligations"]:
                raise ValueError("incomplete or unpinned production verification")
            observed = coverage((log / "crate.vir").read_text())
            if observed != {key: profile[key] for key in ("functions", "abstract_declarations")}:
                raise ValueError("production proof inventory or contract/body fingerprint changed")
        return process, report
    positive, report = verify("normal", ROOT)
    receipt["verification_results"] = report["verification-results"]
    native = run("native-samples", ["cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-contract",
                 "--lib", "collections::runtime::proof_tests::", "--", "--nocapture"], ROOT)
    verifier.require_success(native)
    if not re.search(rf"test result: ok\. {profile['native_tests']} passed; 0 failed;", native.stdout):
        raise ValueError("native standard-library and layout samples were not executed")
    if controls:
        mutations = (
            ("unnamed-write", RUNTIME, "writes", "        Ok(writes)\n    }",
             "        writes.entries.push((u32::MAX, [255u8; 32], None));\n        Ok(writes)\n    }"),
            ("retained-absent-payload", ABSENT, "canonical_absent_view",
             "view[position] = positions[position].domain.least().unwrap_or(0);",
             "let retained = view[position];\n        view[position] = retained;"),
        )
        for name, path, function, anchor, replacement in mutations:
            with tempfile.TemporaryDirectory(prefix="collections-runtime-", dir=out) as temporary:
                specimen = Path(temporary)
                for relative in (*PRODUCTION, HARNESS):
                    (specimen / relative).parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(ROOT / relative, specimen / relative)
                original = (specimen / path).read_bytes()
                changed = once(original.decode(), anchor, replacement)
                (specimen / path).write_text(changed)
                source_guard(specimen)
                mutation, mutated_report = verify(name, specimen, positive=False)
                normalized = subprocess.CompletedProcess(mutation.args, mutation.returncode, mutation.stdout,
                    re.sub(r"(?m)^(error: aborting due to \d+ previous errors?); \d+ warnings? emitted$", r"\1", mutation.stderr))
                killed = verifier.semantic_refusal(normalized, mutated_report, pin, path, specimen,
                    region=verifier.function_lines(changed, function))
                if not killed: raise ValueError(f"intended semantic defect was not rejected: {name}")
                (specimen / path).write_bytes(original)
                restored, _ = verify(name + "-restored", specimen)
                if (specimen / path).read_bytes() != original: raise ValueError("source restoration mismatch")
                receipt["controls"].append({"name": name, "path": str(path), "function": function,
                    "normal": positive.returncode, "mutated": mutation.returncode, "restored": restored.returncode,
                    "normal_sha256": hashlib.sha256(original).hexdigest(),
                    "mutated_sha256": hashlib.sha256(changed.encode()).hexdigest(),
                    "restored_sha256": verifier.digest(specimen / path), "killed": killed})
                save()
    if snapshot(ROOT) != before: raise ValueError("source changed during production qualification")
    receipt["complete"] = True
    save()
    print(f"collections runtime: {profile['verified_obligations']} obligations, {profile['native_tests']} native tests, {len(receipt['controls'])} controls")
    return receipt


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cache", type=Path, default=verifier.cache_directory())
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--controls", action="store_true")
    args = parser.parse_args()
    check(args.out.resolve(), args.cache.resolve(), args.install, args.controls)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
