#!/usr/bin/env python3
"""Measure actual Wasmtime AOT for a closed, QF_BV-qualified LLVM palette.

No expected saving: positive, zero and negative changes are all reported.
Source Rust is the already pinned Rust 1.97.1 idiom experiment. Each of the
THREE separately compiled Wasmtime binaries gets EXACTLY the same per-profile
LLVM-produced WebAssembly input bytes. Never confuse symbol bytes, ELF .text
section bytes, serialized .cwasm bytes, or throughput.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import platform
import subprocess
from pathlib import Path

import qualify_rust_idiom_wasm_28 as rust
from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB
from install_rust_signmask_palette_28 import PALETTE, RULES, expected, sha

SCHEMA = "zeno-fcis/tier1-rust-llvm-signmask-target-cost/1"
HERE = Path(__file__).resolve().parent


def source_check(upstream: Path, bins: dict, source_receipts: dict) -> dict:
    if subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD"], text=True).strip() != UPSTREAM:
        raise ValueError("wrong Wasmtime checkout")
    if subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD:" + SOURCE], text=True).strip() != ORIGINAL_GIT_BLOB:
        raise ValueError("wrong original ISLE source blob")
    if set(bins) != set(PALETTE) or set(source_receipts) != set(PALETTE):
        raise ValueError("must include all three palette candidates")
    for mode in PALETTE:
        info = json.loads(source_receipts[mode].read_bytes())
        if (info.get("mode") != mode or info.get("upstream_commit") != UPSTREAM
                or info.get("source_sha256") != sha(expected(upstream, mode).encode())
                or info.get("rule_sha256") != sha(RULES[mode].encode())
                or info.get("original_git_blob") != ORIGINAL_GIT_BLOB):
            raise ValueError("compiler source patch not bound to expected candidate")
    if (upstream / SOURCE).read_text() != expected(upstream, "sub"):
        raise ValueError("the final staged compiler must use exactly the sub variant")
    changed = subprocess.check_output(
        ["git", "-C", str(upstream), "diff", "--name-only"], text=True).splitlines()
    if changed != [SOURCE]:
        raise ValueError("unexpected final upstream changes")
    models = {}
    for mode, binary in bins.items():
        if not binary.is_file() or binary.stat().st_size <= 0:
            raise ValueError("missing compiler binary")
        models[mode] = {
            "sha256": sha(binary.read_bytes()),
            "bytes": binary.stat().st_size,
            "version": subprocess.check_output([str(binary), "--version"], text=True).strip(),
            "source": json.loads(source_receipts[mode].read_bytes()),
        }
    if len(set(x["version"] for x in models.values())) != 1:
        raise ValueError("compiler versions differ")
    if len(set(x["sha256"] for x in models.values())) != 3:
        raise ValueError("one of the candidate compilers is byte-identical to another")
    return {"upstream_commit": UPSTREAM, "source_path": SOURCE,
            "original_source_blob": ORIGINAL_GIT_BLOB,
            "compilers": models}


def qualify(upstream: Path, bins: dict, receipts: dict, out: Path) -> dict:
    if out.exists():
        raise ValueError("cannot overwrite a previous qualification")
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("only x86-64 native codegen experiment admitted")
    version = subprocess.check_output(["rustc", "--version"], text=True).strip()
    if not version.startswith("rustc 1.97.1 "):
        raise ValueError("incorrect Rust toolchain")
    pins = source_check(upstream, bins, receipts)
    original_source = rust.SOURCE.read_bytes()
    if b"saturating_add" not in original_source:
        raise ValueError("missing idiomatic saturation source")
    out.mkdir(parents=True)
    rows = []
    for profile, opt in rust.PROFILES.items():
        folder = out / profile
        folder.mkdir()
        wasm_receipt = rust.compile_rust(folder, opt)
        wasm = folder / "idiom.wasm"
        output = {}
        for name in PALETTE:
            directory = folder / name
            directory.mkdir()
            rec = rust.compile_aot(bins[name], wasm, directory)
            rec["finite"] = rust.finite(bins[name], directory / "idiom.cwasm")
            output[name] = rec
        baseline = output["original"]
        comparisons = {}
        for mode in PALETTE[1:]:
            item = output[mode]
            comparisons[mode] = {
                "baseline_minus_candidate_elf_text_bytes":
                    baseline["elf_text_bytes"] - item["elf_text_bytes"],
                "baseline_minus_candidate_whole_cwasm_bytes":
                    baseline["aot_bytes"] - item["aot_bytes"],
                "actual_aot_identical": baseline["sha256"] == item["sha256"],
                "native_disassembly_identical":
                    baseline["disassembly_sha256"] == item["disassembly_sha256"],
                "speed_or_throughput_result": "not-measured",
            }
        rows.append({
            "profile": profile, "rust_opt_level": opt,
            "wasm_source": wasm_receipt,
            "compiled": output,
            "comparisons": comparisons,
            "measurement_type": "actual-compiled-AOT-and-ELF-section-not-symbol-estimate",
        })
    catalog = []
    fingerprints = set()
    for row in rows:
        for mode in PALETTE:
            rec = row["compiled"][mode]
            key = sha(json.dumps({
                "profile": row["profile"], "compiler_target": rust.TARGET,
                "wasm_sha256": row["wasm_source"]["sha256"],
                "compiled_aot_sha256": rec["sha256"],
            }, sort_keys=True).encode())
            if key not in fingerprints:
                fingerprints.add(key)
                catalog.append({
                    "content_key": key, "profile": row["profile"], "mode": mode,
                    "wasm_sha256": row["wasm_source"]["sha256"],
                    "aot_sha256": rec["sha256"], "aot_bytes": rec["aot_bytes"],
                    "elf_text_bytes": rec["elf_text_bytes"],
                    "binary_same_optimization_modes": [
                        candidate for candidate in PALETTE
                        if row["compiled"][candidate]["sha256"] == rec["sha256"]],
                    "source_receipt_sha256": sha(receipts[mode].read_bytes()),
                    "semantic_contract": "signed-i64 Rust saturation and explicit total wrapping derivatives; finite tested",
                    "verification_receipt": "external QF_BV preflight in same workflow required",
                    "cost_evidence": "actual pinned Wasmtime CLI AOT + ELF .text",
                })
    return {
        "schema": SCHEMA, "status": "finite-wasm-checked-real-palette-native-AOT-cost",
        "source": pins, "rustc_version": version, "rust_target": rust.RUST_TARGET,
        "native_target": rust.TARGET, "rust_source_sha256": sha(original_source),
        "producer_script_sha256": sha(Path(__file__).read_bytes()),
        "palette_install_source_sha256": sha((HERE / "install_rust_signmask_palette_28.py").read_bytes()),
        "candidates": list(PALETTE), "profiles": rows, "deduplicated_catalog": catalog,
        "scope": [
            "bounded human-proposed equivalent sign-mask alternatives, not ZenoFSIS machine search discovery",
            "finite native Wasmtime arithmetic witnesses not all-input compiled-machine proof",
            "SMT proof is of pure total value expressions, not optimizer proof or poison/ABI/flags",
            "no target throughput, power, latency or execution speed measurement",
            "not a real application workload and no expected saving",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--upstream", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    for mode in PALETTE:
        p.add_argument("--" + mode + "-bin", type=Path, required=True)
        p.add_argument("--" + mode + "-receipt", type=Path, required=True)
    args = p.parse_args()
    report = qualify(args.upstream.resolve(),
        {m: getattr(args, m + "_bin").resolve() for m in PALETTE},
        {m: getattr(args, m + "_receipt").resolve() for m in PALETTE},
        args.out.resolve())
    (args.out / "qualification.json").write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "comparisons":
          [{"profile": x["profile"], "comparison": x["comparisons"]}
           for x in report["profiles"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
