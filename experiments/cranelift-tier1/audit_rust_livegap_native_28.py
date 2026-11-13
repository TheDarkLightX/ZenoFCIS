#!/usr/bin/env python3
"""Independent native per-export cost replay of qualified two-op live-gap patch.

Works on the *original downloaded GitHub Actions artifact*, not receipt claims.
Parses exact Wasmtime guest disassembly, hashes ELF/AOT/CLIF receipts and checks
x86 decoded instruction bytes and counts separately from AOT/.text size.
Not a proof of full compiler correctness or runtime performance.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
from pathlib import Path

BASE = "9e918603e0dda3e04177b884280db7e58086df02"
UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
EXPECTED = ("live_original_rem", "live_q_m_r", "live_reversed_mul",
            "live_two_chains", "wrong_multiplier")
POSITIVE = EXPECTED[:3]
HEADING = re.compile(r"(?m)^wasm\[0\]::function\[(\d+)\]::([^:\n]+):\s*$")
HEX = re.compile(r"^[0-9a-f]{2}$")

def hash_file(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()

def parse(disasm: str) -> dict:
    heads = list(HEADING.finditer(disasm))
    if len(heads) < 5:
        raise ValueError("missing target-specific guest function headings")
    out = {}
    for index, m in enumerate(heads):
        # The five experiment exports precede Rust runtime helpers. Helper
        # disassembly embeds noninstruction literal/data rows that cannot be
        # treated as native instructions by this deliberately narrow auditor.
        if index >= len(EXPECTED):
            break
        symbol_id = int(m.group(1))
        name = m.group(2)
        # Every observed guest function must have a unique index.
        if symbol_id != index or name in out:
            raise ValueError("nonsequential/ambiguous guest-function mapping")
        end = heads[index + 1].start() if index + 1 < len(heads) else len(disasm)
        raw = disasm[m.end():end]
        count = bytes_ = divisions = 0
        opcodes = []
        for line in raw.splitlines():
            if not re.match(r"^ {12}[0-9a-f]{2}(?:\s|$)", line):
                continue
            tokens = line.strip().split()
            k = 0
            while k < len(tokens) and HEX.fullmatch(tokens[k]):
                k += 1
            if k == 0 or k == len(tokens):
                raise ValueError("malformed native instruction row")
            count += 1
            bytes_ += k
            opcode = tokens[k]
            opcodes.append(opcode)
            divisions += int(opcode == "divq")
        if count == 0:
            raise ValueError("empty decoded function")
        out[name] = {"function_index": symbol_id,
                     "decoded_guest_instruction_bytes": bytes_,
                     "decoded_guest_instruction_count": count,
                     "static_divq": divisions,
                     "mnemonic_sha256": hashlib.sha256(("\n".join(opcodes)).encode()).hexdigest()}
    for i, name in enumerate(EXPECTED):
        if name not in out or out[name]["function_index"] != i:
            raise ValueError("wrong target-function export/index mapping")
    return out

def audit(root: Path) -> dict:
    p = root / "tier1-livegap"
    qual = json.loads((p / "results/qualification.json").read_text())
    identities = json.loads((p / "compilers/qualification.json").read_text())
    if (p / "zenofcis-head.txt").read_text().strip() != BASE:
        raise ValueError("wrong experimental ZenoFCIS commit")
    if (p / "wasmtime-head.txt").read_text().strip() != UPSTREAM:
        raise ValueError("wrong upstream Wasmtime checkout")
    if (qual["source_pin"] != UPSTREAM or qual["native_target"] != "x86_64-unknown-linux"
            or identities["upstream"] != UPSTREAM or identities["target"] != "x86_64-unknown-linux"):
        raise ValueError("unmatched compiler source/target contract")
    src = p / "results/optimized/idiom.wasm"
    if hash_file(src) != qual["profiles"]["optimized"]["wasm_sha256"]:
        raise ValueError("Wasm producer hash mismatch")
    original = root / "experiments/cranelift-tier1/rust_live_reconstruction_28.rs"
    if hash_file(original) != qual["rust_source_sha256"]:
        raise ValueError("Rust source hash mismatch")
    names = ("baseline", "patched")
    views = {}
    profile = qual["profiles"]["optimized"]
    if set(qual["profiles"]) != {"optimized"}:
        raise ValueError("unexpected Rust opt profiles")
    for name in names:
        q = profile["results"][name]
        if identities["results"][name]["compiler_sha256"] != q["compiler_sha256"]:
            raise ValueError("compiler identity receipt mismatch")
        aot = p / "results/optimized" / name / "native.cwasm"
        dis = p / "results/optimized" / name / "disassembly.txt"
        if hash_file(aot) != q["aot_sha256"] or hash_file(dis) != q["disassembly_sha256"]:
            raise ValueError("original AOT/disassembly hash changed")
        if aot.stat().st_size != q["whole_aot_bytes"]:
            raise ValueError("altered original AOT length")
        for f in q["clif"]:
            cf = p / "results/optimized" / name / "clif" / f["path"]
            if hash_file(cf) != f["sha256"] or cf.stat().st_size != f["bytes"]:
                raise ValueError("original CLIF hash changed")
        views[name] = parse(dis.read_text())
        if q["finite"] != {"value": 480, "trap_or_panic": 60}:
            raise ValueError("unexpected finite execution coverage")
    recheck = (p / "independent-recheck.json").read_text().strip().splitlines()[-1]
    result = json.loads(recheck)
    if result.get("fresh_executions") != 410 or result.get("status") != "independently-verified-finite-Wasm-and-native-cost":
        raise ValueError("independent recheck receipt mismatch")
    delta = profile["baseline_minus_patched"]
    if delta != {"elf_text_bytes": 0, "module_static_divq": 0, "whole_aot_bytes": 0}:
        raise ValueError("unexpected whole-module cost receipt")
    records = {}
    for name in EXPECTED:
        b, a = views["baseline"][name], views["patched"][name]
        row = {"function_index": b["function_index"],
               "baseline": b, "patched": a,
               "saved_decoded_guest_instruction_bytes": b["decoded_guest_instruction_bytes"]-a["decoded_guest_instruction_bytes"],
               "saved_decoded_guest_instructions": b["decoded_guest_instruction_count"]-a["decoded_guest_instruction_count"]}
        if b["static_divq"] != a["static_divq"]:
            raise ValueError("unexpected changed division count")
        expected = (9, 3) if name in POSITIVE else (0, 0)
        if (row["saved_decoded_guest_instruction_bytes"], row["saved_decoded_guest_instructions"]) != expected:
            raise ValueError("unqualified function-level cost delta: " + name)
        records[name] = row
    if (profile["aot_byte_identical"] or
            profile["results"]["baseline"]["aot_sha256"] == profile["results"]["patched"]["aot_sha256"]):
        raise ValueError("AOT identity unexpectedly unchanged")
    return {
        "schema": "zenofcis/tier1-rust-livegap-native-cost-independent-audit/1",
        "status": "independently-replayed-narrow-native-cost-and-finite-evidence",
        "zenofcis_head": BASE,
        "wasmtime_upstream": UPSTREAM,
        "rustc": qual["rustc"],
        "target": qual["native_target"],
        "wasm_sha256": hash_file(src),
        "rust_source_sha256": hash_file(original),
        "compiler_sha256": {n: identities["results"][n]["compiler_sha256"] for n in names},
        "aot_sha256": {n: profile["results"][n]["aot_sha256"] for n in names},
        "whole_aot_bytes": {n: profile["results"][n]["whole_aot_bytes"] for n in names},
        "elf_text_bytes": {n: profile["results"][n]["elf_text_bytes"] for n in names},
        "module_divq_counts": {n: profile["results"][n]["module_static_divq"] for n in names},
        "export_costs": records,
        "producer_finite": {"per_compiler_value_cases": 480, "per_compiler_trap_or_panic_cases": 60},
        "fresh_audit_executions": result["fresh_executions"],
        "limitations": [
            "Decoded native per-export instruction bytes, not CLIF cost or whole ELF size",
            "A manually developed experimental compiler; not a new ZenoFCIS search discovery",
            "Finite Wasm output/trap agreement, not compiler-pass/ISA or machine-state proof",
            "No measured runtime throughput change, cross-target validation or unconditional VeriISLE result",
            "Per-function disassembly is tied to the same optimized Rust-generated Wasm but is not a representative external corpus",
        ],
    }

def self_test() -> None:
    sample = ("wasm[0]::function[0]::live_original_rem:\n"
              "            48 f7 f1     divq %rcx\n"
              "            c3           retq\n")
    for i, name in enumerate(EXPECTED[1:], 1):
        sample += f"wasm[0]::function[{i}]::{name}:\n"
        sample += "            c3           retq\n"
    found = parse(sample)
    assert found[EXPECTED[0]]["decoded_guest_instruction_bytes"] == 4
    assert found[EXPECTED[0]]["static_divq"] == 1
    assert found[EXPECTED[4]]["decoded_guest_instruction_count"] == 1
    try:
        parse(sample.replace("wasm[0]::function[4]", "wasm[0]::function[3]"))
    except ValueError:
        pass
    else:
        raise AssertionError("fail-closed duplicate function index missing")
    print("independent native parser self tests passed")

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        self_test()
    else:
        if not args.root or not args.out:
            ap.error("--root and --out required")
        self_test()
        report = audit(args.root.resolve())
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
        print(json.dumps({n: {k: report["export_costs"][n][k] for k in (
            "saved_decoded_guest_instruction_bytes", "saved_decoded_guest_instructions")}
            for n in EXPECTED}, sort_keys=True))
