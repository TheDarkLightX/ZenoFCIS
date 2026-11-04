#!/usr/bin/env python3
"""Independent fail-closed qualification of portable Cranelift/Wasmtime AOT timings.

The original AOT modules were *host-ISA dependent* and required AVX512BITALG,
so this experiment uses newly built portable-target modules on the same runner.
This is one virtualized host experiment, NOT proof of a faster CPU sequence.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import platform
from pathlib import Path
from qualify_wasmtime_inprocess_28 import parse_samples, summarize, sha
import qualify_wasmtime_frontend_28 as original
from qualify_wasmtime_function_bytes_28 import parse_function
from qualify_wasmtime_portable_28 import SCHEMA as P_SCHEMA, TARGET

HERE = Path(__file__).resolve().parent
SCHEMA = "zeno-fcis/tier1-wasmtime-portable-runtime-qualification/1"
SIGNED_INPUTS = len(original.VECTORS) + 1024
LOOP_INPUTS = len(original.COUNTS)


def validate_provenance(source_root: Path) -> tuple[dict, dict]:
    report_path = source_root / "qualification.json"
    receipt = json.loads(report_path.read_bytes())
    s = receipt.get("source", {})
    if (receipt.get("schema") != P_SCHEMA
        or receipt.get("status") != "portable-finite-wasm-aot-passed-runtime-unqualified"
        or receipt.get("portable_target") != TARGET
        or s.get("upstream_commit") != original.UPSTREAM
        or s.get("wasm_source_sha256") != original.digest(original.WAT.read_bytes())
        or s.get("rule_sha256") != original.digest(original.RULE.encode())
        or s.get("requested_target") != TARGET
        or s.get("host_architecture") not in ("x86_64", "AMD64")
        or set(receipt.get("compiled", {})) != {"baseline", "patched"}
        or set(receipt.get("finite_checks", {})) != {"baseline", "patched"}
        or set(s.get("compiler_binaries", {})) != {"baseline", "patched"}):
        raise ValueError("missing or mismatched portable AOT source admission")
    for label in ("baseline", "patched"):
        b = s["compiler_binaries"][label]
        if not isinstance(b.get("sha256"), str) or len(b["sha256"]) != 64:
            raise ValueError("compiler binary SHA missing")
        if b.get("version") != s["compiler_binaries"]["baseline"].get("version"):
            raise ValueError("compiler version diverges")
        p = source_root / label
        rec = receipt["compiled"][label]
        binary = (p / "satadd.cwasm").read_bytes()
        disasm = (p / "disassembly.txt").read_bytes()
        if (sha(binary) != rec.get("aot_sha256")
            or len(binary) != rec.get("aot_bytes")
            or sha(disasm) != rec.get("disassembly_sha256")):
            raise ValueError("portable AOT binary or disassembly hash differs")
        # Independently rehash every produced post-optimization CLIF snapshot.
        for entry in rec.get("clif_files", []):
            relative = entry.get("path")
            if not isinstance(relative, str) or not relative or relative.startswith("/"):
                raise ValueError("invalid emitted CLIF relative path")
            root = (p / "clif").resolve()
            file = (root / relative).resolve()
            if not file.is_relative_to(root) or not file.is_file():
                raise ValueError("missing or escaping CLIF file")
            if sha(file.read_bytes()) != entry.get("sha256"):
                raise ValueError("emitted optimized CLIF checksum mismatch")
        if not rec.get("clif_files"):
            raise ValueError("no emitted CLIF source evidence")
        independent = parse_function(disasm.decode("utf-8"))
        if independent != rec.get("function_disassembly"):
            raise ValueError("independent function-byte parse differs from producer")
        finite = receipt["finite_checks"][label]
        if (finite.get("passed") is not True
            or finite.get("sat_pairs") != len(original.VECTORS)
            or finite.get("loop_counts") != list(original.COUNTS)):
            raise ValueError("missing/altered producer finite oracle inputs")
    c = receipt["compiled"]
    same_clif = c["baseline"]["clif_files"] == c["patched"]["clif_files"]
    if receipt.get("clif_same_file_and_content") is not same_clif:
        raise ValueError("emitted CLIF equality claim is inconsistent")
    for cost_key, computed in {
        "function_symbol_byte_delta":
        (c["baseline"]["function_disassembly"]["x86_bytes_in_disassembler_function_region"] -
         c["patched"]["function_disassembly"]["x86_bytes_in_disassembler_function_region"]),
        "function_executed_prefix_byte_delta":
        (c["baseline"]["function_disassembly"]["x86_bytes_through_first_ret"] -
         c["patched"]["function_disassembly"]["x86_bytes_through_first_ret"]),
        "aot_elf_text_byte_delta":
        (c["baseline"]["elf_text_bytes"] - c["patched"]["elf_text_bytes"]),
        "whole_cwasm_byte_delta":
        (c["baseline"]["aot_bytes"] - c["patched"]["aot_bytes"]),
    }.items():
        if receipt.get(cost_key) != computed:
            raise ValueError("cost delta not bound to archived AOT objects: " + cost_key)
    return receipt, {
        name: {"sha256": c[name]["aot_sha256"],
               "bytes": c[name]["aot_bytes"],
               "function": c[name]["function_disassembly"]}
        for name in ("baseline", "patched")
    }


def qualify(source_root: Path, raw: Path, cargo_lock: Path,
            loops: int, reps: int, blocks: int) -> dict:
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("host ISA must be x86-64 for portable AOT comparison")
    receipt, objects = validate_provenance(source_root)
    samples = parse_samples(raw.read_text(), loops, reps, blocks)
    lock = cargo_lock.read_bytes()
    if b'name = "wasmtime"' not in lock:
        raise ValueError("not a pinned Wasmtime dependency lock")
    code = HERE / "inprocess_bench_28"
    blobs = (code / "Cargo.toml", code / "src" / "main.rs", HERE / "qualify_wasmtime_portable_28.py",
             HERE / "qualify_wasmtime_portable_inprocess_28.py", cargo_lock)
    return {
        "schema": SCHEMA,
        "status": "portable-source-bound-inprocess-finite-passed-runtime-performance-exploratory",
        "source": {
            "upstream": original.UPSTREAM,
            "requested_target": TARGET,
            "portable_aot_receipt_sha256": sha((source_root / "qualification.json").read_bytes()),
            "compiler_builds": receipt["source"]["compiler_binaries"],
            "aot_objects": objects,
            "source_file_sha256": {str(p): sha(p.read_bytes()) for p in blobs},
            "raw_samples_sha256": sha(raw.read_bytes()),
        },
        "workload": {"loops_per_guest_call": loops, "calls_per_sample": reps,
                     "balanced_blocks": blocks,
                     "arithmetic_cases_per_variant": SIGNED_INPUTS,
                     "loop_counts_per_variant": LOOP_INPUTS,
                     "oracle": "independent Python/Rust i128 saturated addition"},
        "host": {"architecture": platform.machine()},
        "code_cost": {
            "function_symbol_byte_delta": receipt["function_symbol_byte_delta"],
            "function_executed_prefix_byte_delta": receipt["function_executed_prefix_byte_delta"],
            "whole_cwasm_byte_delta": receipt["whole_cwasm_byte_delta"],
            "aot_elf_text_byte_delta": receipt["aot_elf_text_byte_delta"],
        },
        "timing_summary": summarize(samples["samples"], blocks),
        "samples": samples["samples"],
        "nonclaims": [
            "one portable x86-64 target and one synthetic Wasm workload",
            "runtime cost ratio is exploratory on one GitHub virtual machine",
            "does not establish instruction latency, broad speedup or ISLE legality",
            "finite sampling does not constitute exhaustive Wasm proof",
            "VeriISLE select semantics remain provisional",
            "new portable AOT binary hashes are not interchangeable with old host-ISA AOT hashes",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    for field in ("source-root", "raw", "cargo-lock", "out"):
        p.add_argument("--" + field, required=True, type=Path)
    for field in ("loops", "reps", "blocks"):
        p.add_argument("--" + field, required=True, type=int)
    a = p.parse_args()
    if a.out.exists():
        p.error("will not overwrite prior evidence")
    report = qualify(a.source_root.resolve(), a.raw.resolve(), a.cargo_lock.resolve(),
                     a.loops, a.reps, a.blocks)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": report["status"],
                      "code_cost": report["code_cost"],
                      "timing_summary": report["timing_summary"]}, sort_keys=True))


if __name__ == "__main__":
    main()
