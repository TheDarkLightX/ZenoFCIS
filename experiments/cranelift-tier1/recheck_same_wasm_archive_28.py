#!/usr/bin/env python3
"""Independent finite native recheck of archived same-Wasm compiler A/B.

Usage:
  python3 recheck_same_wasm_archive_28.py --archive-dir /tmp/unzipped \
    --expected-zenofcis <40-hex-head> --out /tmp/native-recheck.json
Runs exact archived executables, checks SHA-256 receipts, and parses static x86
instruction bytes independently. This is testing, NOT a compiler correctness
proof, general target speedup, or dynamic instruction-count measurement.
"""
import argparse
import hashlib
import json
import random
import re
import subprocess
from pathlib import Path

EXPECTED_UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
EXPECTED_SOURCE = "28498a86ecb6e00bd5968ee94ee001f3fa42cfa2b0a5cc9f43342c0a5c7231c3"
MASK = (1 << 64) - 1

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def signed(v):
    v &= MASK
    return v if v < 1 << 63 else v - (1 << 64)

def u64_oracle(x, y):
    q, r = divmod(x, y)
    rotated = ((r << 17) | (r >> 47)) & MASK
    return signed(q ^ rotated)

def invoke(root, mode, x, y):
    exe = root / mode / "wasmtime"
    obj = root / mode / "input.cwasm"
    return subprocess.run(
        [str(exe), "run", "--allow-precompiled", "--invoke", "work",
         str(obj), str(signed(x)), str(signed(y))],
        text=True, capture_output=True, timeout=30,
    )

def run(root, expected_zenofcis):
    receipt = json.loads((root / "qualification.json").read_text())
    if (receipt.get("schema") != "zeno-fcis/tier1-udivrem-same-wasm-compiler-ab/1"
        or receipt.get("upstream") != EXPECTED_UPSTREAM
        or receipt.get("same_wat_source") is not True
        or (root / "wasmtime-head.txt").read_text().strip() != EXPECTED_UPSTREAM
        or (root / "zenofcis-head.txt").read_text().strip() != expected_zenofcis
        or digest(root / "input.wat") != EXPECTED_SOURCE):
        raise ValueError("evidence pins or input WAT mismatch")
    for mode, divs, count in (("baseline", 2, 16), ("patched", 1, 15)):
        row = receipt["results"][mode]
        for path, key in (("wasmtime", "compiler_sha256"),
                          ("input.cwasm", "aot_sha256"),
                          ("disassembly.txt", "disassembly_sha256")):
            if digest(root / mode / path) != row[key]:
                raise ValueError(f"{mode}/{path} does not match receipt hash")
        disasm = (root / mode / "disassembly.txt").read_text()
        pattern = re.compile(r"^\s*((?:[0-9a-fA-F]{2}\s+)+)([A-Za-z][A-Za-z0-9_]*)\b")
        insns = [m.groups() for line in disasm.splitlines()
                 if (m := pattern.match(line))]
        if len(insns) != count or insns[-1][1] != "retq":
            raise ValueError(f"{mode} has unexpected instruction count or terminator")
        if sum(op == "divq" for _, op in insns) != divs:
            raise ValueError(f"{mode} has unexpected static divq count")
        expected_bytes = 43 if mode == "baseline" else 41
        if sum(len(raw.split()) for raw, _ in insns) != expected_bytes:
            raise ValueError(f"{mode} has unexpected instruction byte count")
        if (row["static_cost"]["decoded_instruction_lines"] != count
            or row["static_cost"]["through_ret_bytes"] != expected_bytes):
            raise ValueError(f"{mode} static cost does not match receipt")
    rng = random.Random(20261107)
    edges = [0,1,2,3,7,9,15,17,(1<<16)-1,1<<16,(1<<32)-1,
             1<<32,(1<<63)-1,1<<63,(1<<64)-2,MASK]
    pairs = [(x, y) for x in edges for y in edges if y]
    pairs += [(rng.getrandbits(64), rng.getrandbits(64) or 1)
              for _ in range(120)]
    for x, y in pairs:
        expected = u64_oracle(x, y)
        for mode in ("baseline", "patched"):
            p = invoke(root, mode, x, y)
            if p.returncode or int(p.stdout.strip()) != expected:
                raise AssertionError(f"{mode} failed at x={x} y={y}: {p.stdout} {p.stderr}")
    for mode in ("baseline", "patched"):
        p = invoke(root, mode, 1, 0)
        if p.returncode == 0 or "integer divide by zero" not in p.stderr.lower():
            raise AssertionError(f"{mode} did not preserve zero-divisor trap")
    return {
        "classification": "independent finite native execution and archived-static-cost audit; not proof",
        "same_wat": True,
        "input_sha256": EXPECTED_SOURCE,
        "upstream": EXPECTED_UPSTREAM,
        "zenofcis": expected_zenofcis,
        "nonzero_pairs": len(pairs),
        "native_witness_executions": 2 * len(pairs),
        "zero_divisor_traps": 2,
        "baseline_static_divq": 2,
        "patched_static_divq": 1,
        "baseline_static_instructions": 16,
        "patched_static_instructions": 15,
        "baseline_to_patched_first_ret_bytes": [43,41],
        "aot_bytes_saved": 0,
        "elf_text_bytes_saved": 0,
        "status": "passed"
    }

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--archive-dir", type=Path, required=True)
    ap.add_argument("--expected-zenofcis", required=True)
    ap.add_argument("--out", type=Path)
    a = ap.parse_args()
    result = run(a.archive_dir.resolve(), a.expected_zenofcis)
    blob = json.dumps(result, sort_keys=True, indent=2) + "\n"
    if a.out:
        a.out.write_text(blob)
    print(blob)

if __name__ == "__main__":
    main()
