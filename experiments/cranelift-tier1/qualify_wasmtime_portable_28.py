#!/usr/bin/env python3
"""Build/check same-source Wasmtime AOT A/B for a portable explicit x86-64 target.

Previous host-ISA AOT objects required AVX-512 BITALG and were rejected
by a different GitHub runner. Never disable host CPU compatibility checks.
"""
from __future__ import annotations
import argparse
import json
import platform
from pathlib import Path
import qualify_wasmtime_frontend_28 as previous
from qualify_wasmtime_function_bytes_28 import parse_function

TARGET = "x86_64-unknown-linux"
SCHEMA = "zeno-fcis/tier1-wasmtime-portable-aot-28/1"


def compile_portable(binary: Path, out: Path, variant: str) -> dict:
    folder = out / variant
    folder.mkdir()
    clif = folder / "clif"
    clif.mkdir()
    aot = folder / "satadd.cwasm"
    # Explicit non-host-specialized target: no inferred AVX2/AVX512 flags.
    cmd = [str(binary), "compile", "--target", TARGET,
           "--emit-clif", str(clif), "-o", str(aot), str(previous.WAT)]
    proc = previous.run(cmd, timeout=300)
    (folder / "compile.stdout").write_text(proc.stdout)
    (folder / "compile.stderr").write_text(proc.stderr)
    if not aot.is_file() or not aot.stat().st_size:
        raise ValueError("portable AOT compilation missing")
    dis = previous.run([str(binary), "objdump", "--funcs", "wasm",
                        "--bytes", str(aot)]).stdout
    (folder / "disassembly.txt").write_text(dis)
    details = parse_function(dis)
    clif_files = sorted(p for p in clif.rglob("*") if p.is_file())
    if not clif_files:
        raise ValueError("portable AOT has no actual emitted CLIF")
    return {
        "aot_sha256": previous.digest(aot.read_bytes()),
        "aot_bytes": aot.stat().st_size,
        "elf_text_bytes": previous.text_size(aot),
        "disassembly_sha256": previous.digest(dis.encode()),
        "clif_files": [{"path": str(p.relative_to(clif)),
                        "sha256": previous.digest(p.read_bytes())}
                       for p in clif_files],
        "function_disassembly": details,
    }


def qualify(upstream: Path, binaries: dict, out: Path) -> dict:
    if out.exists():
        raise ValueError("will not overwrite previous experiment results")
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("portable x86_64 benchmark requires an x86_64 host")
    pins = previous.source_pins(upstream, binaries)
    out.mkdir(parents=True)
    compiled = {name: compile_portable(binary, out, name)
                for name, binary in binaries.items()}
    finite = previous.check_finite(binaries, out)
    if set(finite) != {"baseline", "patched"}:
        raise ValueError("missing finite checks")
    for record in finite.values():
        if record.get("passed") is not True or record.get("sat_pairs") != len(previous.VECTORS):
            raise ValueError("unqualified portable finite witness")
        if record.get("loop_counts") != list(previous.COUNTS):
            raise ValueError("wrong finite loop input domain")
    a, b = compiled["baseline"], compiled["patched"]
    same_clif = a["clif_files"] == b["clif_files"]
    if not same_clif:
        raise ValueError("baseline/patch received different CLIF inputs")
    return {
        "schema": SCHEMA,
        "status": "portable-finite-wasm-aot-passed-runtime-unqualified",
        "source": dict(pins, requested_target=TARGET,
                       host_architecture=platform.machine(),
                       source_verification="pinned one-rule-only Cranelift diff"),
        "compiled": compiled,
        "finite_checks": finite,
        "portable_target": TARGET,
        "aot_elf_text_byte_delta": a["elf_text_bytes"] - b["elf_text_bytes"],
        "whole_cwasm_byte_delta": a["aot_bytes"] - b["aot_bytes"],
        "function_symbol_byte_delta": (
            a["function_disassembly"]["x86_bytes_in_disassembler_function_region"] -
            b["function_disassembly"]["x86_bytes_in_disassembler_function_region"]),
        "function_executed_prefix_byte_delta": (
            a["function_disassembly"]["x86_bytes_through_first_ret"] -
            b["function_disassembly"]["x86_bytes_through_first_ret"]),
        "clif_same_file_and_content": same_clif,
        "nonclaims": [
            "portable baseline/patch are NOT byte-identical to earlier host-ISA AOT objects",
            "x86 function bytes do not imply whole-module savings or a runtime speedup",
            "finite Wasm execution checks are not formal exhaustive semantic proof",
            "named VeriISLE proof remains conditional on a provisional select spec",
            "compiled same Wasm but only one constructed workload; not broad regression proof",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    for field in ("upstream", "baseline-bin", "patched-bin", "out"):
        p.add_argument("--" + field, type=Path, required=True)
    a = p.parse_args()
    report = qualify(a.upstream.resolve(),
                     {"baseline": a.baseline_bin.resolve(),
                      "patched": a.patched_bin.resolve()}, a.out.resolve())
    (a.out / "qualification.json").write_text(
        json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": report["status"],
                      "portable_target": TARGET,
                      "function_symbol_byte_delta": report["function_symbol_byte_delta"],
                      "function_executed_prefix_byte_delta": report["function_executed_prefix_byte_delta"],
                      "whole_cwasm_byte_delta": report["whole_cwasm_byte_delta"]},
                     sort_keys=True))


if __name__ == "__main__":
    main()
