#!/usr/bin/env python3
"""Pinned, deliberately-invalid compiler matcher reachability screening.

The exact and broad ISLE canaries replace a boolean with zero using subsume.
Those are INTENTIONALLY WRONG and are run only on constructed test programs.
A changed emitted binary is evidence of some observable compiler effect;
a wrong runtime result can establish that the diagnostic impacted values.
Unchanged output is INCONCLUSIVE about whether a rewrite matched internally.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import platform
import subprocess
from pathlib import Path

from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB
from install_overflow_match_canary_28 import MODES, BROAD_RULE, EXACT_RULE, sha, source
import qualify_rust_idiom_wasm_28 as rust
import qualify_wasmtime_frontend_28 as frontend

SCHEMA = "zeno-fcis/tier1-overflow-reachability-canaries/1"
WITNESSES = (
    (0, 0),
    (1, 2),
    (frontend.MAX, 1),
    (frontend.MIN, -1),
    (frontend.MAX, frontend.MAX),
    (frontend.MIN, frontend.MIN),
    (-1, -2),
    (frontend.MAX, frontend.MIN),
)
FUNCTIONS = ("sat_add", "sat_add_xor", "sat_add_mix", "wrap_add")


def source_pins(upstream: Path, compilers: dict, receipts: dict) -> dict:
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("only x86_64 diagnostic compilation admitted")
    if set(compilers) != set(MODES) or set(receipts) != set(MODES):
        raise ValueError("missing/extra compiler variant")
    def git(*a):
        return subprocess.check_output(
            ["git", "-C", str(upstream), *a], text=True).strip()
    if git("rev-parse", "HEAD") != UPSTREAM:
        raise ValueError("unexpected Wasmtime source HEAD")
    if git("rev-parse", "HEAD:" + SOURCE) != ORIGINAL_GIT_BLOB:
        raise ValueError("expected upstream source blob unavailable")
    if (upstream / SOURCE).read_text() != source(upstream, "broad_canary"):
        raise ValueError("final diagnostic compiler patch is not the expected broad probe")
    if git("diff", "--name-only").splitlines() != [SOURCE]:
        raise ValueError("unexpected modified tracked Wasmtime source")
    identities = {}
    for mode in MODES:
        path = compilers[mode]
        rec = json.loads(receipts[mode].read_bytes())
        if (not path.is_file() or not path.stat().st_size
                or rec.get("mode") != mode
                or rec.get("status") != "untrusted-diagnostic-only-compiler-NOT-FOR-USE"
                or rec.get("source_sha256") != sha(source(upstream, mode).encode())
                or rec.get("upstream_commit") != UPSTREAM):
            raise ValueError("untrusted or unpinned probe compiler " + mode)
        identities[mode] = {
            "sha256": sha(path.read_bytes()),
            "file_bytes": path.stat().st_size,
            "version": subprocess.check_output(
                [str(path), "--version"], text=True).strip(),
            "source_receipt_sha256": sha(receipts[mode].read_bytes()),
        }
    if len({identities[m]["version"] for m in MODES}) != 1:
        raise ValueError("mismatched compiler version")
    return {"upstream_sha": UPSTREAM, "original_isle_blob": ORIGINAL_GIT_BLOB,
            "compiler_identities": identities}


def witness(bin_path: Path, aot: Path, mode: str) -> dict:
    records = []
    for function in FUNCTIONS:
        for x, y in WITNESSES:
            expected = rust.value(function, x, y)
            result = frontend.invoke(bin_path, aot, function, x, y)
            records.append({"function": function, "x": x, "y": y,
                            "expected": expected, "observed": result,
                            "mismatch": result != expected})
    mismatches = sum(bool(row["mismatch"]) for row in records)
    if mode == "baseline" and mismatches:
        raise ValueError("original Wasmtime failed its semantic oracle")
    return {"sample_count": len(records), "mismatch_count": mismatches,
            "samples": records, "classification":
            ("reference-finite-correct" if mode == "baseline" else
             "INTENTIONALLY-UNSOUND-canary-finite-diagnostic")}


def classify(baseline: dict, exact: dict, broad: dict) -> dict:
    canary = {}
    for label, record in (("exact_canary", exact), ("broad_canary", broad)):
        canary[label] = {
            "binary_changed": record["sha256"] != baseline["sha256"],
            "disassembly_changed":
                record["disassembly_sha256"] != baseline["disassembly_sha256"],
            "observed_wrong_outputs": record["witness"]["mismatch_count"],
            "complete_aot_delta_bytes": baseline["aot_bytes"] - record["aot_bytes"],
            "elf_text_delta_bytes":
                baseline["elf_text_bytes"] - record["elf_text_bytes"],
        }
    if canary["exact_canary"]["observed_wrong_outputs"] > 0:
        verdict = "exact-pattern-probe-had-observable-incorrect-semantic-effect"
    elif canary["exact_canary"]["disassembly_changed"]:
        verdict = "exact-pattern-probe-changed-native-code-but-no-wrong-witness"
    elif canary["broad_canary"]["observed_wrong_outputs"] > 0:
        verdict = "broad-i8-xor-probe-observable-exact-probe-not-observable"
    elif canary["broad_canary"]["disassembly_changed"]:
        verdict = "broad-probe-changed-native-code-no-semantic-witness"
    else:
        verdict = "both-canaries-native-code-identical-no-reachability-inference"
    return {"canaries": canary, "verdict": verdict,
            "semantics": "diagnostic-invalid-rules-no-equivalence-proof",
            "limitation": "no native effect does not prove a rule never matched"}


def qualify(upstream: Path, compilers: dict, receipts: dict, out: Path) -> dict:
    if out.exists():
        raise ValueError("refuse existing report location")
    if not subprocess.check_output(["rustc", "--version"], text=True).startswith(
            "rustc 1.97.1 "):
        raise ValueError("Rust source-to-Wasm pin changed")
    pins = source_pins(upstream, compilers, receipts)
    out.mkdir(parents=True)
    profiles = []
    for name, optimization in rust.PROFILES.items():
        folder = out / name
        folder.mkdir()
        source_id = rust.compile_rust(folder, optimization)
        wasm = folder / "idiom.wasm"
        compiled = {}
        for mode in MODES:
            sub = folder / mode
            sub.mkdir()
            code = rust.compile_aot(compilers[mode], wasm, sub)
            code["witness"] = witness(compilers[mode], sub / "idiom.cwasm", mode)
            compiled[mode] = code
        profiles.append({"profile": name, "rust_optimization": optimization,
                         "source_wasm": source_id, "compiled": compiled,
                         "diagnostic": classify(*(compiled[x] for x in MODES))})
    return {
        "schema": SCHEMA, "status": "INTENTIONALLY-UNSOUND-RULE-REACHABILITY-DIAGNOSTIC",
        "original_rule_name": "tier1_rust_llvm_i64_overflow_xorand",
        "source": pins, "rust_source_sha256": sha(rust.SOURCE.read_bytes()),
        "producer_script_sha256": sha(Path(__file__).read_bytes()),
        "exact_canary_sha256": sha(EXACT_RULE.encode()),
        "broad_canary_sha256": sha(BROAD_RULE.encode()),
        "profiles": profiles, "modes": list(MODES),
        "limits": [
            "The two probes are deliberately INCORRECT and must never be submitted or used in production",
            "Not a sound rewrite, code size optimization, catalog candidate or SMT/VeriISLE proof",
            "A binary change demonstrates a compiler effect, not an exact match counter",
            "An absent change cannot establish absence of an internal match",
            "Execution covers finitely many Rust-generated functions and input witnesses",
            "No speed, whole-program performance, x86 flags, ABI, poison or other-target claim",
            "No Bytecode Alliance upstream change; detached compiler copies only",
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    for mode in MODES:
        parser.add_argument("--" + mode.replace("_", "-") + "-bin",
                            dest=mode + "_bin", type=Path, required=True)
        parser.add_argument("--" + mode.replace("_", "-") + "-receipt",
                            dest=mode + "_receipt", type=Path, required=True)
    args = parser.parse_args()
    report = qualify(args.upstream.resolve(),
        {m: getattr(args, m + "_bin").resolve() for m in MODES},
        {m: getattr(args, m + "_receipt").resolve() for m in MODES},
        args.out.resolve())
    (args.out / "qualification.json").write_text(
        json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": report["status"],
                      "profiles": [(p["profile"], p["diagnostic"])
                                   for p in report["profiles"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
