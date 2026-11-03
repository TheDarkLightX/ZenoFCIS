#!/usr/bin/env python3
"""Native ARM64 finite execution audit of exact pinned Cranelift compiler outputs.

Independent C oracle computes in signed 128 bits; x86 checks and Z3/VeriISLE
proofs remain separate. No speed, ABI-universality or exhaustive claim.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
NATIVE_C = HERE / "native_aarch64_harness_28.c"
OBJECTS = {
    "baseline": {
        "filename": "aarch64_baseline_20261103.o",
        "sha256": "2daaba4e7a7f5efb303ef19bab9943e76ed2aacf5871d1d4c6d0724932091a7e",
        "size": 648, "symbol_bytes": 36, "instructions": 9, "renamed": "tier1_original",
    },
    "patched": {
        "filename": "aarch64_patched_20261103.o",
        "sha256": "02f539e0a4137a89c369eb4d1815750264c3a2bc3a1c0e7de14e1dd6233a2b9e",
        "size": 640, "symbol_bytes": 32, "instructions": 8, "renamed": "tier1_patched",
    },
}


def run(*args, timeout=180):
    return subprocess.run(args, text=True, capture_output=True,
                          check=True, timeout=timeout).stdout


def sha(data):
    return hashlib.sha256(data).hexdigest()


def inspect_object(path, expected):
    raw = path.read_bytes()
    if len(raw) != expected["size"] or sha(raw) != expected["sha256"]:
        raise ValueError("emitted object bytes differ from hosted A/B measurement")
    header = run("readelf", "-h", str(path))
    if not re.search(r"(?m)^\s*Class:\s*ELF64\s*$", header) or not re.search(
        r"(?m)^\s*Machine:\s*AArch64\s*$", header
    ):
        raise ValueError("emitted object is not AArch64 ELF64")
    symbols = [line.split() for line in run(
        "nm", "-S", "--defined-only", "--format=posix", str(path)
    ).splitlines() if line.strip()]
    if len(symbols) != 1 or len(symbols[0]) != 4:
        raise ValueError("unexpected machine-code symbols")
    name, kind, offset, size = symbols[0]
    if (name != "%tier1_signed_satadd" or kind not in ("T", "t")
            or int(offset, 16) != 0 or int(size, 16) != expected["symbol_bytes"]):
        raise ValueError("emitted function ABI/name/size changed")
    reloc = run("readelf", "-r", str(path))
    if "There are no relocations in this file." not in reloc:
        raise ValueError("unexpected relocations")
    disasm = run("objdump", "-d", str(path))
    n = len(re.findall(r"(?m)^\s*[0-9a-f]+:\s+[0-9a-f]{8}\s+\S+", disasm))
    if n != expected["instructions"]:
        raise ValueError(f"unexpected ARM64 decoded instruction count {n}")
    return {"sha256": sha(raw), "bytes": len(raw), "symbol_bytes": int(size, 16),
            "decoded_instructions": n, "disassembly_sha256": sha(disasm.encode())}


def qualify():
    machine = platform.machine().lower()
    if machine not in ("aarch64", "arm64"):
        raise ValueError(f"not a native AArch64 runner: {machine}")
    if not NATIVE_C.is_file():
        raise ValueError("missing independent C oracle")
    profiles = {}
    with tempfile.TemporaryDirectory(prefix="tier1-arm-native-") as tmp:
        tmp = Path(tmp)
        objs = []
        for name, spec in OBJECTS.items():
            source = HERE / "evidence" / "objects" / spec["filename"]
            profiles[name] = inspect_object(source, spec)
            renamed = tmp / (name + ".o")
            run("objcopy", "--redefine-sym",
                "%tier1_signed_satadd=" + spec["renamed"], str(source), str(renamed))
            renamed_symbols = run("nm", "-S", "--defined-only", "--format=posix", str(renamed))
            if renamed_symbols.count(spec["renamed"]) != 1 or "%tier1_signed_satadd" in renamed_symbols:
                raise ValueError("unexpected symbol rename outcome")
            objs.append(renamed)
        binary = tmp / "tier1_arm64_native"
        compiler = ["gcc", "-std=gnu11", "-O2", "-Wall", "-Wextra", "-Werror",
                    "-fno-lto", str(NATIVE_C), *(str(x) for x in objs), "-o", str(binary)]
        run(*compiler)
        if not binary.is_file():
            raise ValueError("failed to produce native oracle executable")
        stdout = run(str(binary), timeout=180)
        lines = stdout.strip().splitlines()
        if len(lines) != 1:
            raise ValueError("unexpected oracle stdout")
        test = json.loads(lines[0])
        if (test.get("status") != "passed-finite-native-aarch64-pair-oracle"
                or test.get("edge_inputs") != 24
                or test.get("edge_pairs") != 576
                or test.get("random_pairs") != 250000
                or test.get("checked_pairs") != 250576
                or test.get("seed") != "0x9e3779b97f4a7c15"):
            raise ValueError("oracle failed, incomplete or changed its test domain")
        return {
            "schema": "zeno-fcis/tier1-native-aarch64-execution/1",
            "status": "passed-finite-native-aarch64",
            "target": machine,
            "upstream_commit": "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49",
            "producer_experiment_commit": "85011e3b53c3551cf67f6331af6df3b6b9001825",
            "producer_run": "https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41647685920",
            "source_harness_sha256": sha(NATIVE_C.read_bytes()),
            "native_checker_binary_sha256": sha(binary.read_bytes()),
            "objects": profiles, "test": test,
            "tool_versions": {
                "gcc": run("gcc", "--version").splitlines()[0],
                "objcopy": run("objcopy", "--version").splitlines()[0],
                "objdump": run("objdump", "--version").splitlines()[0],
            },
            "nonclaims": [
                "finite native test; no exhaustive proof across all 2^128 input pairs",
                "no ARM microarchitectural latency, throughput or energy improvement",
                "signed i64 pure outcome only; no poison, undefined or generic flags-liveness proof",
                "experimental rule still conditional on local VeriISLE select model",
                "cross-target 4-byte size saving is a single-function measurement",
            ],
        }


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    if args.out.exists():
        p.error("refusing to overwrite existing evidence")
    result = qualify()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": result["status"], "checked_pairs": result["test"]["checked_pairs"],
                      "instruction_counts": {k: v["decoded_instructions"] for k, v in result["objects"].items()}},
                     sort_keys=True))


if __name__ == "__main__":
    main()
