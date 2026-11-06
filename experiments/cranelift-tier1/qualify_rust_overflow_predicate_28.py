#!/usr/bin/env python3
"""Measured LLVM-generated Rust Wasm under unmodified vs one-rule Cranelift.

Negative, zero, and positive real native AOT deltas are equally admissible.
"""
from __future__ import annotations
import argparse
import json
import platform
import subprocess
from pathlib import Path

from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB
from install_rust_overflow_predicate_28 import MODES, RULE, RULE_NAME, sha, source
import qualify_rust_idiom_wasm_28 as rust

SCHEMA = "zeno-fcis/rust-overflow-predicate-native-AOT/1"
HERE = Path(__file__).resolve().parent


def validate_pins(upstream: Path, bins: dict, receipts: dict) -> dict:
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("experiment requires x86-64 real native codegen")
    if set(bins) != set(MODES) or set(receipts) != set(MODES):
        raise ValueError("missing source-pinned compiler")
    if subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD"],
                               text=True).strip() != UPSTREAM:
        raise ValueError("upstream Git SHA mismatch")
    if subprocess.check_output(["git", "-C", str(upstream), "rev-parse",
                                "HEAD:" + SOURCE], text=True).strip() != ORIGINAL_GIT_BLOB:
        raise ValueError("upstream original ISLE blob changed")
    if (upstream / SOURCE).read_text() != source(upstream, "xorand"):
        raise ValueError("final checked-out compiler patch is not the exact named rule")
    if subprocess.check_output(["git", "-C", str(upstream), "diff", "--name-only"],
                               text=True).splitlines() != [SOURCE]:
        raise ValueError("unexpected modified upstream source")
    identities = {}
    for mode in MODES:
        receipt = json.loads(receipts[mode].read_bytes())
        if (receipt.get("schema") != "zeno-fcis/rust-llvm-overflow-predicate-isle/1"
                or receipt.get("mode") != mode
                or receipt.get("source_sha256") != sha(source(upstream, mode).encode())
                or receipt.get("upstream_commit") != UPSTREAM):
            raise ValueError("source receipt drift for " + mode)
        binary = bins[mode]
        if not binary.is_file() or binary.stat().st_size == 0:
            raise ValueError("compiler binary missing: " + mode)
        identities[mode] = {
            "sha256": sha(binary.read_bytes()), "bytes": binary.stat().st_size,
            "version": subprocess.check_output([str(binary), "--version"], text=True).strip(),
            "source_receipt": receipt,
        }
    if identities["baseline"]["version"] != identities["xorand"]["version"]:
        raise ValueError("different compiler versions")
    return {"upstream_commit": UPSTREAM, "source_path": SOURCE,
            "upstream_original_blob": ORIGINAL_GIT_BLOB, "compilers": identities}


def qualify(upstream: Path, bins: dict, receipts: dict, output: Path) -> dict:
    if output.exists():
        raise ValueError("refusing to overwrite measured evidence")
    rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
    if not rustc.startswith("rustc 1.97.1 "):
        raise ValueError("wrong Rust to Wasm compiler version")
    pins = validate_pins(upstream, bins, receipts)
    if b"saturating_add" not in rust.SOURCE.read_bytes():
        raise ValueError("Rust source does not contain original saturation idiom")
    output.mkdir(parents=True)
    profiles, catalog, seen = [], [], set()
    for profile, optimization in rust.PROFILES.items():
        root = output / profile
        root.mkdir()
        wasm_identity = rust.compile_rust(root, optimization)
        wasm = root / "idiom.wasm"
        compiled = {}
        for mode in MODES:
            folder = root / mode
            folder.mkdir()
            code = rust.compile_aot(bins[mode], wasm, folder)
            code["finite"] = rust.finite(bins[mode], folder / "idiom.cwasm")
            compiled[mode] = code
        b, c = compiled["baseline"], compiled["xorand"]
        comparisons = {
            "elf_text_baseline_minus_candidate": b["elf_text_bytes"] - c["elf_text_bytes"],
            "complete_cwasm_baseline_minus_candidate": b["aot_bytes"] - c["aot_bytes"],
            "aot_binary_identical": b["sha256"] == c["sha256"],
            "native_disassembly_identical": b["disassembly_sha256"] == c["disassembly_sha256"],
            "runtime_performance": "not-measured",
        }
        profiles.append({"profile": profile, "rust_opt_level": optimization,
                         "wasm_source": wasm_identity,
                         "compiled": compiled, "comparison": comparisons})
        for mode in MODES:
            item = compiled[mode]
            key = sha(json.dumps({
                "wasm_sha256": wasm_identity["sha256"],
                "target": rust.TARGET, "aot_sha256": item["sha256"],
            }, sort_keys=True).encode())
            if key not in seen:
                seen.add(key)
                catalog.append({"content_key": key, "profile": profile,
                                "mode": mode, "wasm_sha256": wasm_identity["sha256"],
                                "aot_sha256": item["sha256"],
                                "aot_bytes": item["aot_bytes"],
                                "elf_text_bytes": item["elf_text_bytes"],
                                "same_native_bytes_modes": [candidate for candidate in MODES
                                  if compiled[candidate]["sha256"] == item["sha256"]],
                                "semantic_contract": "signed i64 saturation + downstream Rust wrapping operations, finite checked",
                                "source_receipt_sha256": sha(receipts[mode].read_bytes()),
                                "verification_receipt": "separate same-workflow independent QF_BV proof",
                                "cost_evidence": "real same-Wasm pinned Wasmtime AOT and ELF section"})
    return {
        "schema": SCHEMA,
        "status": "native-wasmtime-AOT-finite-checked-no-speed-claim",
        "sources": pins,
        "rustc_version": rustc,
        "rust_source_sha256": sha(rust.SOURCE.read_bytes()),
        "producer_script_sha256": sha(Path(__file__).read_bytes()),
        "installer_script_sha256": sha((HERE / "install_rust_overflow_predicate_28.py").read_bytes()),
        "rule_sha256": sha(RULE.encode()), "rule_name": RULE_NAME,
        "target": rust.TARGET, "wasm_target": rust.RUST_TARGET,
        "modes": list(MODES), "profiles": profiles, "deduplicated_catalog": catalog,
        "limits": ["finite Wasm tests not all-input executable verification",
                   "QF_BV proof of pure expressions not exact named-rule verification",
                   "source Rust is constructed for this experiment, not a production application",
                   "x86 flags/ABI/poison and full optimizer pass semantics not proven",
                   "no measured speed, throughput, power or cross-target savings",
                   "no expected native cost improvement"]}


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--upstream", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    for mode in MODES:
        p.add_argument("--" + mode + "-bin", required=True, type=Path)
        p.add_argument("--" + mode + "-receipt", required=True, type=Path)
    args = p.parse_args()
    result = qualify(args.upstream.resolve(),
      {m: getattr(args, m + "_bin").resolve() for m in MODES},
      {m: getattr(args, m + "_receipt").resolve() for m in MODES},
      args.out.resolve())
    (args.out / "qualification.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": result["status"],
      "costs": [(r["profile"], r["comparison"]) for r in result["profiles"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
