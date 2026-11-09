#!/usr/bin/env python3
"""Finite context controls for the pinned same-Wasm dual-result division experiment.

Same WAT for baseline/patched. Numeric equality, traps, and native static
division counts are checked independently. Not a whole-compiler proof or
runtime benchmark. Outputs a source/compilers/results receipt.
"""
import argparse
import hashlib
import json
import random
import re
import subprocess
from pathlib import Path

MASK = (1 << 64) - 1
MIN = -(1 << 63)
MAX = (1 << 63) - 1
TARGET = "x86_64-unknown-linux"
PIN = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"

def wat(body, memory=""):
    return '(module\n' + memory + '  (func (export "work") (param $x i64) (param $y i64) (result i64)\n' + body + '  )\n)\n'

DIV = "local.get $x\nlocal.get $y\ni64.div_u\n"
REM = "local.get $x\nlocal.get $y\ni64.rem_u\n"
ROT = "i64.const 17\ni64.rotl\n"
XOR = "i64.xor\n"
CASES = {
    "adjacent": wat(DIV + REM + ROT + XOR),
    "reverse_order": wat(REM + ROT + DIV + XOR),
    "swapped_rem_inputs": wat(DIV + "local.get $y\nlocal.get $x\ni64.rem_u\n" + ROT + XOR),
    "quotient_only": wat(DIV),
    "remainder_only": wat(REM),
    "signed_64": wat(DIV.replace("div_u", "div_s") + REM.replace("rem_u", "rem_s") + ROT + XOR),
    "unsigned_32": wat("""local.get $x
i32.wrap_i64
local.get $y
i32.wrap_i64
i32.div_u
i64.extend_i32_u
local.get $x
i32.wrap_i64
local.get $y
i32.wrap_i64
i32.rem_u
i64.extend_i32_u
i64.xor
"""),
    "signed_32": wat("""local.get $x
i32.wrap_i64
local.get $y
i32.wrap_i64
i32.div_s
i64.extend_i32_u
local.get $x
i32.wrap_i64
local.get $y
i32.wrap_i64
i32.rem_s
i64.extend_i32_u
i64.xor
"""),
    "intervening_add": wat(DIV + "i64.const 1\ni64.add\n" + REM + ROT + XOR),
    "intervening_load": wat(DIV + "i32.const 0\ni64.load\ni64.add\n" + REM + ROT + XOR, "  (memory 1)\n"),
    "branch": wat("local.get $x\ni64.const 1\ni64.and\ni64.eqz\nif (result i64)\n" +
                  DIV + REM + ROT + XOR + "else\n" + DIV + REM + ROT + XOR + "end\n"),
}

def hash_bytes(b):
    return hashlib.sha256(b).hexdigest()

def signed(bits, width):
    bits &= (1 << width) - 1
    return bits if bits < (1 << (width - 1)) else bits - (1 << width)

def rotation(x):
    x &= MASK
    return ((x << 17) | (x >> 47)) & MASK

def truncdiv(x, y):
    return (abs(x) // abs(y)) * (-1 if (x < 0) != (y < 0) else 1)

def oracle(name, x, y):
    ux, uy = x & MASK, y & MASK
    if name in ("signed_32", "unsigned_32"):
        a, b = (signed(x, 32), signed(y, 32)) if name == "signed_32" else (x & 0xffffffff, y & 0xffffffff)
        if b == 0 or (name == "signed_32" and a == -(1 << 31) and b == -1):
            return None
        q = truncdiv(a, b) if name == "signed_32" else a // b
        r = a - q * b
        return signed((q & 0xffffffff) ^ (r & 0xffffffff), 64)
    if name == "signed_64":
        if y == 0 or (x == MIN and y == -1):
            return None
        q = truncdiv(x, y)
        r = x - q * y
        return signed((q & MASK) ^ rotation(r), 64)
    if not uy or (name == "swapped_rem_inputs" and not ux):
        return None
    q, r = divmod(ux, uy)
    if name == "quotient_only":
        return signed(q, 64)
    if name == "remainder_only":
        return signed(r, 64)
    if name == "swapped_rem_inputs":
        r = uy % ux
    if name == "intervening_add":
        q = (q + 1) & MASK
    return signed(q ^ rotation(r), 64)

def vectors():
    xs = [0, 1, 2, 3, 7, -1, -2, -3, -7, MIN, MIN + 1, MAX,
          MAX - 1, 1 << 32, -(1 << 32)]
    ys = [1, 3, 2, -3, 7, -1, 0, 1, -7, -1, 3, -1, 0, 2, 1]
    seed = list(zip(xs, ys)) + [(0, 0), (1, 0), (MIN, -1), (MIN, 1),
        (MAX, MAX), (-1, -1), (1, 1), (-1, 1), (1 << 32, 1 << 32)]
    rng = random.Random(0x56232026)
    seed.extend((signed(rng.getrandbits(64), 64),
                 signed(rng.getrandbits(64), 64)) for _ in range(16))
    return tuple(dict.fromkeys(seed))

def run(cmd, timeout=60):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)

def cost(disassembly):
    rows = [row for row in disassembly.splitlines()
            if re.match(r"^\s{12}[0-9a-f]{2}(?:\s|$)", row)]
    return {"decoded_instructions": len(rows),
            "static_divisions": sum(bool(re.search(r"\b(?:divq|divl|idivq|idivl)\b", row))
                                    for row in rows)}

def check(exe, aot, case, x, y):
    result = run([str(exe), "run", "--allow-precompiled", "--invoke", "work",
                  str(aot), str(x), str(y)])
    expected = oracle(case, x, y)
    if expected is None:
        if result.returncode == 0 or "trap" not in result.stderr.lower():
            raise AssertionError(f"missing trap: {case}({x},{y})")
        return "trap"
    if result.returncode or result.stdout.strip() != str(expected):
        raise AssertionError(f"incorrect {case}({x},{y}): {result.returncode}: {result.stdout} {result.stderr}")
    return "value"

def qualify(baseline, patched, output):
    if output.exists():
        raise ValueError("refusing to overwrite results")
    output.mkdir(parents=True)
    binaries = {"baseline": baseline.resolve(), "patched": patched.resolve()}
    pins = {name: {"sha256": hash_bytes(exe.read_bytes()), "version":
            run([str(exe), "--version"]).stdout.strip()} for name, exe in binaries.items()}
    if pins["baseline"]["version"] != pins["patched"]["version"]:
        raise ValueError("compiler revisions differ")
    items = {}; tests = vectors()
    for name, source in CASES.items():
        folder = output / name
        folder.mkdir()
        watfile = folder / "input.wat"
        watfile.write_text(source)
        variants = {}
        for mode, exe in binaries.items():
            aot = folder / (mode + ".cwasm")
            compile_result = run([str(exe), "compile", "--target", TARGET,
                                  "-o", str(aot), str(watfile)], timeout=180)
            if compile_result.returncode:
                raise RuntimeError(f"{name}/{mode} failed compile: {compile_result.stderr}")
            dis = run([str(exe), "objdump", "--funcs", "wasm", "--bytes", str(aot)])
            if dis.returncode:
                raise RuntimeError(f"{name}/{mode} failed disassembly")
            (folder / (mode + ".disassembly.txt")).write_text(dis.stdout)
            finite = {"value": 0, "trap": 0}
            for x, y in tests:
                finite[check(exe, aot, name, x, y)] += 1
            variants[mode] = {"aot_sha256": hash_bytes(aot.read_bytes()),
                              "aot_bytes": aot.stat().st_size,
                              "disassembly_sha256": hash_bytes(dis.stdout.encode()),
                              "static_cost": cost(dis.stdout), "finite": finite}
        b = variants["baseline"]["static_cost"]["static_divisions"]
        p = variants["patched"]["static_cost"]["static_divisions"]
        if name == "adjacent" and (b, p) != (2, 1):
            raise AssertionError("no positive adjacent-pair fusion")
        if name in ("reverse_order", "swapped_rem_inputs", "quotient_only",
                    "remainder_only", "signed_64", "signed_32", "unsigned_32") and b != p:
            raise AssertionError("incorrect out-of-scope fusion: " + name)
        items[name] = {"wat_sha256": hash_bytes(source.encode()),
                       "results": variants, "static_div_savings": b - p}
    receipt = {"schema": "zenofcis/tier1-udivrem-contexts/1",
               "upstream_source_pin": PIN, "compiler_pins": pins,
               "status": "finite-context-controls-passed",
               "tests_per_mode_per_case": len(tests), "cases": items,
               "assurance": "finite Wasm value/trap tests; static native costs; not exhaustive proof or speed measurement"}
    (output / "qualification.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": receipt["status"], "tests_per_mode_per_case": len(tests),
        "div_savings_by_context": {n: r["static_div_savings"] for n, r in items.items()}}, sort_keys=True))

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline-bin", type=Path, required=True)
    parser.add_argument("--patched-bin", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    a = parser.parse_args()
    qualify(a.baseline_bin, a.patched_bin, a.out)
