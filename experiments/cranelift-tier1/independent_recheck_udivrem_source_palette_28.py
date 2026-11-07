#!/usr/bin/env python3
"""Fail-closed independent audit of archived udiv/rem source-palette AOT.

Re-run readelf and Wasmtime native objdump on actual emitted binaries rather
than trusting the producer's cost and disassembly receipts. Also repeat a
second arithmetic oracle with extra boundary inputs and the zero-divisor trap.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

from install_isle_candidate import UPSTREAM
from independent_recheck_scaling_28 import independent_elf_text_size
from qualify_udivrem_source_palette_28 import MODES, SCHEMA, TARGET, WATS, MASK, ROT, s64, sha
import qualify_wasmtime_frontend_28 as frontend


def independent_static_cost(raw: str) -> dict:
    lines = raw.splitlines()
    labels = [i for i, line in enumerate(lines) if line.startswith("wasm[0]::function[")]
    if len(labels) != 1 or lines[labels[0]] not in ("wasm[0]::function[0]:", "wasm[0]::function[0]::work:"):
        raise ValueError("cannot identify unique exported function")
    total_bytes, ret_bytes, op_count, divs, ret_count = 0, None, 0, 0, 0
    for line in lines[labels[0] + 1:]:
        if not line.strip() or line.strip() == "╰─╼ trap: Normal(IntegerDivisionByZero)":
            continue
        if not line.startswith("            "):
            raise ValueError("not an expected instruction rendering")
        match = re.fullmatch(r" {12}((?:[a-f0-9]{2})(?: [a-f0-9]{2})*)(?: {2,}(.*))?", line)
        if not match:
            raise ValueError("malformed machine byte line")
        raw_bytes = match.group(1).split(" ")
        instruction = (match.group(2) or "").strip()
        total_bytes += len(raw_bytes)
        op_count += 1
        if instruction.startswith(("divq ", "divl ", "divw ", "divb ",
                                   "idivq ", "idivl ", "idivw ", "idivb ")):
            divs += 1
        if instruction == "retq" or instruction.startswith("retq "):
            ret_count += 1
            ret_bytes = total_bytes
    if ret_count != 1 or not ret_bytes:
        raise ValueError("no unambiguous native return")
    return {"region_bytes": total_bytes,
            "through_ret_bytes": ret_bytes,
            "decoded_instruction_lines": op_count,
            "static_div_instructions": divs,
            "not_dynamic": True}


def second_oracle(x: int, y: int) -> int:
    a, b = x % (1 << 64), y % (1 << 64)
    if b == 0:
        raise ZeroDivisionError
    quotient = a // b
    remainder = a - quotient * b
    rotated = ((remainder >> (64 - ROT)) | (remainder << ROT)) % (1 << 64)
    return s64(quotient ^ rotated)


def recheck(receipt: Path, exe: Path) -> dict:
    original = receipt.read_bytes()
    record = json.loads(original)
    if (record.get("schema") != SCHEMA
            or record.get("status") != "native-source-formulation-comparison-finite-checked"
            or record.get("target") != TARGET
            or record.get("pins", {}).get("upstream_commit") != UPSTREAM
            or record.get("candidate_vocabulary") != list(MODES)
            or set(record.get("results", {})) != set(MODES)):
        raise ValueError("invalid/missing original envelope")
    pin = record["pins"]
    if (not exe.is_file() or exe.stat().st_size != pin["binary_size"]
            or sha(exe.read_bytes()) != pin["binary_sha256"]
            or subprocess.check_output([str(exe), "--version"], text=True).strip()
               != pin["binary_version"]):
        raise ValueError("not the original pinned Wasmtime compiler binary")
    producer = Path(__file__).parent / "qualify_udivrem_source_palette_28.py"
    if sha(producer.read_bytes()) != pin["producer_sha256"]:
        raise ValueError("producer program changed since original run")
    measured = {}
    for mode in MODES:
        p = receipt.parent / mode
        row = record["results"][mode]
        wat = (p / "source.wat").read_bytes()
        if wat != WATS[mode].encode() or sha(wat) != row.get("wat_sha256"):
            raise ValueError("source WAT mismatch")
        obj = p / "functions.cwasm"
        binary = obj.read_bytes()
        if (not binary or sha(binary) != row.get("binary_sha256")
                or len(binary) != row.get("whole_cwasm_bytes")):
            raise ValueError("native AOT missing or changed")
        text_size = independent_elf_text_size(obj)
        if text_size != row.get("elf_text_bytes"):
            raise ValueError("ELF text size does not match original claim")
        actual_disassembly = frontend.run(
            [str(exe), "objdump", "--funcs", "wasm", "--bytes", str(obj)]).stdout
        archived_dis = (p / "disassembly.txt").read_bytes()
        if (sha(archived_dis) != row.get("disassembly_sha256")
                or actual_disassembly.encode() != archived_dis):
            raise ValueError("archived disassembly does not match actual object")
        cost = independent_static_cost(actual_disassembly)
        if cost != row.get("instruction_cost"):
            raise ValueError("decoded byte costs are not independently reproducible")
        producer_finite = row.get("finite", {})
        if (producer_finite.get("passed") is not True
                or producer_finite.get("zero_divisor_trap_checked") is not True
                or producer_finite.get("classification") != "finite-runtime-tests-not-exhaustive-i64-proof"):
            raise ValueError("producer finite output/trap controls missing")
        # A deliberately different bounded witness set from the producer.
        witnesses = [(0, 1), (-1, -1), (-(1 << 63), -1),
                     ((1 << 63) - 1, 1), (7, 3), (123456789, -7)]
        for x, y in witnesses:
            observed = frontend.invoke(exe, obj, "work", x, y)
            if observed != second_oracle(x, y):
                raise ValueError(f"second witness oracle mismatch, mode={mode}")
        zero = subprocess.run(
            [str(exe), "run", "--allow-precompiled", "--invoke",
             "work", str(obj), "1", "0"],
            text=True, capture_output=True, timeout=40, check=False)
        if zero.returncode == 0 or "trap" not in zero.stderr.lower():
            raise ValueError("zero divisor did not produce a Wasm trap")
        measured[mode] = {
            "whole_cwasm_bytes": len(binary),
            "elf_text_bytes": text_size,
            "instruction_cost": cost,
            "binary_sha256": sha(binary),
            "independent_witness_count": len(witnesses),
            "zero_divisor_trap_rechecked": True,
        }
    b, p = [measured[name] for name in MODES]
    recalculated = {
        "whole_cwasm_bytes": b["whole_cwasm_bytes"] - p["whole_cwasm_bytes"],
        "elf_text_bytes": b["elf_text_bytes"] - p["elf_text_bytes"],
        "first_ret_bytes": b["instruction_cost"]["through_ret_bytes"] - p["instruction_cost"]["through_ret_bytes"],
        "static_div_instructions": b["instruction_cost"]["static_div_instructions"] - p["instruction_cost"]["static_div_instructions"],
    }
    if recalculated != record.get("baseline_minus_candidate"):
        raise ValueError("original claimed deltas are wrong")
    # The catalog is not proof. Independently recompute exact dedup identities.
    catalog = {}
    for mode in MODES:
        row = measured[mode]
        key = sha(json.dumps({
            "contract": "unsigned i64 quotient XOR rotl(unsigned i64 remainder,17), nonzero divisor; zero divisor trap",
            "target": TARGET, "compiler_sha256": pin["binary_sha256"],
            "native_sha256": row["binary_sha256"],
        }, sort_keys=True).encode())
        if key not in catalog:
            catalog[key] = {
                "content_key": key, "modes": [],
                "native_sha256": row["binary_sha256"],
                "whole_aot_bytes": row["whole_cwasm_bytes"],
                "elf_text_bytes": row["elf_text_bytes"],
                "instruction_cost": row["instruction_cost"],
                "equivalence": "conditional Euclidean law + finite native oracle; not whole-compiler proof",
            }
        catalog[key]["modes"].append(mode)
    if record.get("catalog") != sorted(catalog.values(), key=lambda e: e["content_key"]):
        raise ValueError("catalog entries or dedup provenance forged")
    return {
        "schema": "zeno-fcis/tier1-udivrem-independent-native-audit/1",
        "status": "independently-rechecked-ELF-AOT-static-cost-finite-and-zero-trap",
        "source_receipt_sha256": sha(original),
        "upstream_commit": UPSTREAM, "measured": measured,
        "verified_deltas": recalculated,
        "limitations": [
            "two semantically equivalent but DIFFERENT source WATs, no compiler patch",
            "static division instruction counts do not measure CPU speed",
            "finite native checks do not exhaust the full 64-bit domain",
            "no full ISA flags, ABI, poison or dynamic trace proof",
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-receipt", required=True, type=Path)
    parser.add_argument("--bin", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    if args.out.exists():
        parser.error("refusing to overwrite recheck")
    result = recheck(args.source_receipt.resolve(), args.bin.resolve())
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": result["status"],
                      "verified_deltas": result["verified_deltas"]}, sort_keys=True))


if __name__ == "__main__":
    main()
