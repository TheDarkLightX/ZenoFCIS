#!/usr/bin/env python3
"""Conservative byte audit of a real Wasmtime AOT function disassembly.

Counts bytes rendered by Wasmtime objdump --funcs wasm --bytes for the named
fixture through its first return. This is not an ELF symbol-size, CPU cycle,
or whole-module size measurement.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
from pathlib import Path

UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
SRC_SCHEMA = "zeno-fcis/tier1-wasmtime-frontend-28/1"
SCHEMA = "zeno-fcis/tier1-wasmtime-function-disasm-cost-28/1"
HEADER = "wasm[0]::function[0]::sat:"
NEXT = "wasm[0]::function[1]:"
BYTE_LINE = re.compile(r"^ {12}((?:[0-9a-f]{2})(?: [0-9a-f]{2})*)(?: {2,}(.*)|$)")

def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def parse_function(raw: str) -> dict:
    if raw.count(HEADER) != 1 or raw.count(NEXT) != 1:
        raise ValueError("expected exactly one named sat function and following boundary")
    start = raw.index(HEADER)
    if raw[:start].strip() or raw.index(NEXT) <= start:
        raise ValueError("function order is ambiguous")
    middle = raw[start+len(HEADER):raw.index(NEXT)].strip("\n")
    if not middle.strip():
        raise ValueError("empty function disassembly")
    byte_count = 0
    ret_end = None
    mnemonic_lines = 0
    for line in middle.splitlines():
        if not line.strip():
            continue
        match = BYTE_LINE.fullmatch(line)
        if not match:
            raise ValueError("unparseable disassembly line: " + line[:80])
        octets = match.group(1).split(" ")
        if not all(re.fullmatch("[0-9a-f]{2}", b) for b in octets):
            raise ValueError("invalid machine byte")
        byte_count += len(octets)
        mnemonic = (match.group(2) or "").strip()
        if mnemonic:
            mnemonic_lines += 1
        if re.match(r"^retq(?:\s|$)", mnemonic):
            if ret_end is not None:
                raise ValueError("multiple return terminators")
            ret_end = byte_count
    if ret_end is None or ret_end < 1 or byte_count < ret_end:
        raise ValueError("missing or malformed return")
    return {"objdump_function": "wasm[0]::function[0]::sat",
            "x86_bytes_through_first_ret": ret_end,
            "x86_bytes_in_disassembler_function_region": byte_count,
            "x86_bytes_after_first_ret": byte_count - ret_end,
            "reported_instruction_lines_including_post_ret": mnemonic_lines}

def qualify(root: Path) -> dict:
    qpath = root / "qualification.json"
    source_raw = qpath.read_bytes()
    source = json.loads(source_raw)
    if (source.get("schema") != SRC_SCHEMA or
        source.get("status") != "finite-wasm-aot-check-passed-runtime-unqualified" or
        source.get("source", {}).get("upstream_commit") != UPSTREAM or
        source.get("source", {}).get("host_machine") not in ("x86_64", "AMD64") or
        set(source.get("compiled", {})) != {"baseline", "patched"} or
        set(source.get("finite_checks", {})) != {"baseline", "patched"} or
        not all(source["finite_checks"][k].get("passed") is True for k in ("baseline", "patched"))):
        raise ValueError("unqualified source report, host, or source pin")
    records = {}
    for variant in ("baseline", "patched"):
        record = source["compiled"][variant]
        dis = (root / variant / "disassembly.txt").read_bytes()
        if digest(dis) != record.get("disassembly_sha256"):
            raise ValueError("disassembly hash mismatch: " + variant)
        aot = (root / variant / "satadd.cwasm").read_bytes()
        if (digest(aot) != record.get("aot_sha256") or
            len(aot) != record.get("aot_bytes")):
            raise ValueError("AOT object hash/length mismatch: " + variant)
        parsed = parse_function(dis.decode("utf-8"))
        records[variant] = {"disassembly_sha256": digest(dis),
                            "aot_sha256": digest(aot), **parsed}
    old, new = records["baseline"], records["patched"]
    byte_delta = old["x86_bytes_through_first_ret"] - new["x86_bytes_through_first_ret"]
    region_delta = (old["x86_bytes_in_disassembler_function_region"] -
                    new["x86_bytes_in_disassembler_function_region"])
    text_delta = source["compiled"]["baseline"]["elf_text_bytes"] - source["compiled"]["patched"]["elf_text_bytes"]
    whole_delta = source["compiled"]["baseline"]["aot_bytes"] - source["compiled"]["patched"]["aot_bytes"]
    if (text_delta != source.get("aot_text_delta_baseline_minus_patched") or
        whole_delta != source.get("whole_cwasm_delta_baseline_minus_patched")):
        raise ValueError("inconsistent whole-module deltas")
    return {"schema": SCHEMA, "status": "pinned-real-wasm-disassembly-byte-audit",
            "upstream_commit": UPSTREAM, "source_qualification_sha256": digest(source_raw),
            "per_variant": records, "diff": {
                "x86_executable_prefix_byte_delta": byte_delta,
                "disassembler_region_byte_delta": region_delta,
                "aot_elf_text_section_byte_delta": text_delta,
                "whole_cwasm_file_byte_delta": whole_delta},
            "scope": "one exported Wasm function in pinned AOT objdump; byte counts through first ret include prologue, not symbol-length or throughput",
            "nonclaims": ["not an ELF function-symbol size measurement",
                          "post-ret bytes are disassembler region, not certified literal pool length",
                          "no cycle/throughput or production workload performance benefit",
                          "one WAT fixture, no whole-module file or text-section size reduction",
                          "finite Wasm oracle, not exhaustive whole-system/poison/flags/ABI proof",
                          "rule verification conditional on experimental scalar-select specification"]}

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--root", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    if args.out.exists():
        p.error("refusing to overwrite derived receipt")
    report = qualify(args.root.resolve())
    args.out.write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "diff": report["diff"]}, sort_keys=True))

if __name__ == "__main__":
    main()
