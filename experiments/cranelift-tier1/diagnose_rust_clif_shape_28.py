#!/usr/bin/env python3
"""Inspect actual Cranelift IR emitted for pinned Rust->Wasm saturation idioms.

IR trace/shape diagnosis only. The presence or absence of a printed token is
NOT proof that a particular ISLE rewrite applied, nor that it can or cannot
apply after subsequent optimization. Native AOT cost was qualified separately.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

from install_isle_candidate import UPSTREAM, RULE
from qualify_rust_idiom_wasm_28 import (
    PROFILES, SCHEMA as RUST_SCHEMA, TARGET, SOURCE, RUST_TARGET, digest,
)
import qualify_wasmtime_frontend_28 as frontend
import qualify_wasmtime_scaling_28 as scaling

SCHEMA = "zeno-fcis/tier1-rust-wasm-clif-shape-diagnosis/1"
TOKENS = (
    "select", "icmp", "sadd_overflow", "uadd_overflow",
    "ushr", "sshr", "iadd", "isub", "bxor", "bor", "band", "brif", "jump",
)
TOKEN_PATTERN = re.compile(r"(?<![a-zA-Z_])(" + "|".join(map(re.escape, TOKENS))
                           + r")(?![a-zA-Z_])")
MAX_FILE_SIZE = 2_000_000


def describe_clif(text: str) -> dict:
    if not text.strip():
        raise ValueError("empty emitted CLIF file")
    lines = text.splitlines()
    counts = {name: 0 for name in TOKENS}
    sample = []
    for index, line in enumerate(lines, 1):
        # Descriptive token search; do not confuse it with graph matching.
        code = line.split(";", 1)[0]
        found = TOKEN_PATTERN.findall(code)
        for token in found:
            counts[token] += 1
        if found and len(sample) < 25:
            sample.append({"line": index, "tokens": found,
                           "source": code.strip()[:220]})
    return {"line_count": len(lines), "token_counts": counts,
            "sample_instructions": sample,
            "scope": "textual-observation-not-ISLE-match-or-verifier-result"}


def compile_with_clif(bin_path: Path, wasm: Path, root: Path) -> dict:
    root.mkdir(parents=True)
    output = root / "native.cwasm"
    directory = root / "clif"
    directory.mkdir()
    command = [
        str(bin_path), "compile", "--target", TARGET,
        "--emit-clif", str(directory), "-o", str(output), str(wasm),
    ]
    result = subprocess.run(command, check=True, text=True,
                            capture_output=True, timeout=360)
    (root / "compile.stdout").write_text(result.stdout)
    (root / "compile.stderr").write_text(result.stderr)
    if not output.is_file() or output.stat().st_size == 0:
        raise ValueError("CLIF diagnostic emitted no real native AOT")
    files = sorted(p for p in directory.rglob("*") if p.is_file())
    if not files:
        raise ValueError("no actual Cranelift IR emitted")
    outputs = []
    for file in files:
        content = file.read_bytes()
        if not content or len(content) > MAX_FILE_SIZE:
            raise ValueError("missing or oversized emitted CLIF")
        text = content.decode("utf-8")
        outputs.append({
            "path": file.relative_to(directory).as_posix(),
            "sha256": digest(content), "bytes": len(content),
            "structure": describe_clif(text),
        })
    binary = output.read_bytes()
    return {"wasm_sha256": digest(wasm.read_bytes()), "argv": command,
            "native_aot_sha256": digest(binary),
            "native_aot_bytes": len(binary),
            "clif_files": outputs,
            "qualified": "diagnostic-IR-capture-only"}


def validate_preexisting_receipt(receipt: Path, bins: dict[str, Path],
                                 upstream: Path) -> dict:
    raw = receipt.read_bytes()
    original = json.loads(raw)
    if (original.get("schema") != RUST_SCHEMA
            or original.get("upstream_commit") != UPSTREAM
            or original.get("rust_source_sha256") != digest(SOURCE.read_bytes())
            or original.get("rust_target") != RUST_TARGET
            or original.get("compiler_target") != TARGET
            or original.get("rule_sha256") != digest(RULE.encode())
            or len(original.get("cases", [])) != len(PROFILES)
            or set(bins) != {"baseline", "patched"}):
        raise ValueError("original executable Rust-AOT receipt missing/drifted")
    pins = frontend.source_pins(upstream, bins)
    if pins != original.get("wasmtime_source_pins"):
        raise ValueError("compiler pins changed after original qualification")
    for index, (profile, opt) in enumerate(PROFILES.items()):
        case = original["cases"][index]
        wasm = receipt.parent / profile / "idiom.wasm"
        if (case.get("profile") != profile or case.get("opt_level") != opt
                or digest(wasm.read_bytes()) != case.get("rust_wasm", {}).get("sha256")
                or len(wasm.read_bytes()) != case.get("rust_wasm", {}).get("bytes")
                or set(case.get("results", {})) != {"baseline", "patched"}):
            raise ValueError("Rust Wasm profile/byte provenance has drifted")
        for label in bins:
            path = receipt.parent / profile / label / "idiom.cwasm"
            measured = case["results"][label]
            raw_aot = path.read_bytes()
            if (digest(raw_aot) != measured.get("sha256")
                    or len(raw_aot) != measured.get("aot_bytes")):
                raise ValueError("previously qualified native AOT was changed")
    return original


def qualify(upstream: Path, bins: dict[str, Path], receipt: Path,
            out: Path) -> dict:
    if out.exists():
        raise ValueError("cannot overwrite earlier diagnostic")
    original = validate_preexisting_receipt(receipt, bins, upstream)
    out.mkdir(parents=True)
    cases = []
    for profile in PROFILES:
        wasm = receipt.parent / profile / "idiom.wasm"
        results = {label: compile_with_clif(exe, wasm, out / profile / label)
                   for label, exe in bins.items()}
        def catalog(row: dict) -> dict:
            return {f["path"]: f["sha256"] for f in row["clif_files"]}
        cases.append({
            "profile": profile,
            "rust_wasm_sha256": digest(wasm.read_bytes()),
            "baseline_and_patched_clif_byte_identical": (
                catalog(results["baseline"]) == catalog(results["patched"])),
            "results": results,
            "already_qualified_native_aot_binary_identical":
                original["cases"][list(PROFILES).index(profile)]["binary_identical"],
        })
    # Positive *codegen* control. Not a guarantee that emitted IR text
    # directly exposes the exact post-optimization ISLE rule's LHS.
    fixture_bytes = scaling.SOURCE_WAT.read_bytes()
    if digest(fixture_bytes) != scaling.SOURCE_SHA256:
        raise ValueError("handwritten WAT control source pin changed")
    control = {
        label: compile_with_clif(exe, scaling.SOURCE_WAT, out / "control" / label)
        for label, exe in bins.items()
    }
    return {
        "schema": SCHEMA,
        "status": "source-pinned-CLIF-capture-and-structural-diagnosis",
        "upstream_commit": UPSTREAM,
        "producer_script_sha256": digest(Path(__file__).read_bytes()),
        "original_rust_source_sha256": digest(SOURCE.read_bytes()),
        "original_qualified_aot_receipt_sha256": digest(receipt.read_bytes()),
        "original_WAT_positive_codegen_control_sha256": digest(fixture_bytes),
        "rust_wasm_cases": cases,
        "positive_codegen_control": {
            "description": "source-pinned original Zeno WAT; not external application",
            "native_AOT_changed": control["baseline"]["native_aot_sha256"]
                                  != control["patched"]["native_aot_sha256"],
            "results": control,
        },
        "interpretation": [
            "CLIF dump is an emitted compiler artifact, not an ISLE hit counter",
            "absence of an instruction string does not prove absence of a later canonicalized pattern",
            "byte-identical AOT in Rust negative tests does not prove the rule was never evaluated",
            "control WAT is experiment-authored and not a proxy for real application prevalence",
            "existing VeriISLE rule proof is conditional on a provisional local select specification",
            "no new whole-program, flag, poison, ABI, or performance proof",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    for arg in ("upstream", "baseline-bin", "patched-bin",
                "source-receipt", "out"):
        p.add_argument("--" + arg, required=True, type=Path)
    a = p.parse_args()
    report = qualify(a.upstream.resolve(),
        {"baseline": a.baseline_bin.resolve(),
         "patched": a.patched_bin.resolve()},
        a.source_receipt.resolve(), a.out.resolve())
    (a.out / "diagnosis.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": report["status"], "rust_cases": [
        {"profile": c["profile"], "clif_same": c["baseline_and_patched_clif_byte_identical"],
         "baseline_clif": [x["structure"]["token_counts"]
                           for x in c["results"]["baseline"]["clif_files"]]}
        for c in report["rust_wasm_cases"]],
        "control_aot_changed": report["positive_codegen_control"]["native_AOT_changed"]},
        sort_keys=True))


if __name__ == "__main__":
    main()
