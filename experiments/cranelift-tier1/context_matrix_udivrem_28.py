#!/usr/bin/env python3
"""Finite context/negative-control matrix for pinned same-Wasm Cranelift div/rem patch.

Runs the SAME WAT text under the archived baseline and patched compilers.
Independent Python unsigned/signed arithmetic oracles; no compiler-pass proof.
Static disassembly only establishes whether code changed, not execution speed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import random
import subprocess
from pathlib import Path

UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
TARGET = "x86_64-unknown-linux"
MASK64 = (1 << 64) - 1
MASK32 = (1 << 32) - 1
CASES = {
    "memory_between": """(module
  (memory 1)
  (func (export "work") (param $x i64) (param $y i64) (result i64)
    (local $q i64)
    local.get $x
    local.get $y
    i64.div_u
    local.set $q
    i32.const 0
    local.get $q
    i64.store
    local.get $x
    local.get $y
    i64.rem_u
    local.get $q
    i64.xor
  )
)
""",
    "different_blocks": """(module
  (func (export "work") (param $x i64) (param $y i64) (result i64)
    (local $q i64)
    local.get $x
    local.get $y
    i64.div_u
    local.set $q
    local.get $x
    i64.const 1
    i64.and
    i64.eqz
    if (result i64)
      local.get $x
      local.get $y
      i64.rem_u
    else
      local.get $q
    end
  )
)
""",
    "signed_i64": """(module
  (func (export "work") (param $x i64) (param $y i64) (result i64)
    local.get $x
    local.get $y
    i64.div_s
    local.get $x
    local.get $y
    i64.rem_s
    i64.xor
  )
)
""",
    "unsigned_i32": """(module
  (func (export "work") (param $x i32) (param $y i32) (result i32)
    local.get $x
    local.get $y
    i32.div_u
    local.get $x
    local.get $y
    i32.rem_u
    i32.xor
  )
)
""",
    "swapped_operands": """(module
  (func (export "work") (param $x i64) (param $y i64) (result i64)
    local.get $x
    local.get $y
    i64.div_u
    local.get $y
    local.get $x
    i64.rem_u
    i64.xor
  )
)
""",
    "remainder_first": """(module
  (func (export "work") (param $x i64) (param $y i64) (result i64)
    local.get $x
    local.get $y
    i64.rem_u
    local.get $x
    local.get $y
    i64.div_u
    i64.xor
  )
)
""",
}
# Different operation, width, or SSA argument order cannot satisfy the patch matcher.
NEGATIVE_MATCH_CONTROLS = {"signed_i64", "unsigned_i32", "swapped_operands"}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def signed(bits: int, width: int) -> int:
    bits &= (1 << width) - 1
    return bits if bits < (1 << (width - 1)) else bits - (1 << width)


def expected(name: str, x: int, y: int) -> int:
    width = 32 if name == "unsigned_i32" else 64
    mask = (1 << width) - 1
    a, b = x & mask, y & mask
    if b == 0 or (name == "swapped_operands" and a == 0):
        raise ZeroDivisionError("integer divide by zero")
    if name == "signed_i64":
        sx, sy = signed(a, 64), signed(b, 64)
        if sx == -(1 << 63) and sy == -1:
            raise OverflowError("integer overflow")
        q = abs(sx) // abs(sy)
        if (sx < 0) != (sy < 0):
            q = -q
        r = sx - q * sy
    else:
        q, r = divmod(a, b)
    if name == "swapped_operands":
        r = b % a
    if name == "different_blocks":
        result = r if (a & 1) == 0 else q
    else:
        result = q ^ r
    return result & mask


def vectors(name: str) -> list[tuple[int, int]]:
    width = 32 if name == "unsigned_i32" else 64
    edges = [0, 1, 2, 3, 7, 255, (1 << (width - 1)) - 1,
             1 << (width - 1), (1 << width) - 2, (1 << width) - 1]
    rng = random.Random(0x56232026 + width)
    pairs = [(x, y) for x in edges for y in edges]
    pairs.extend((rng.getrandbits(width), rng.getrandbits(width))
                 for _ in range(44))
    return [(x, y) for x, y in dict.fromkeys(pairs)
            if y != 0 and (x != 0 or name != "swapped_operands")
            and not (name == "signed_i64"
                     and x == 1 << 63 and y == (1 << 64) - 1)]


def invoke(exe: Path, aot: Path, x: int, y: int, width: int):
    return subprocess.run(
        [str(exe), "run", "--allow-precompiled", "--invoke", "work",
         str(aot), str(signed(x, width)), str(signed(y, width))],
        capture_output=True, text=True, timeout=30)


def check_case(root: Path, target: Path, name: str) -> dict:
    wat = CASES[name].encode()
    source = target / (name + ".wat")
    source.write_bytes(wat)
    width = 32 if name == "unsigned_i32" else 64
    report = {"wat_sha256": sha(wat), "signedness": "signed" if name == "signed_i64" else "unsigned",
              "width": width, "bounded_vectors": len(vectors(name)), "compilers": {}}
    for mode in ("baseline", "patched"):
        exe = (root / mode / "wasmtime").resolve()
        binary = (target / (name + "." + mode + ".cwasm")).resolve()
        run = subprocess.run([str(exe), "compile", "--target", TARGET,
                              "-o", str(binary), str(source.resolve())],
                             capture_output=True, text=True, timeout=300)
        if run.returncode:
            raise RuntimeError(f"{name}/{mode} compile failed: {run.stderr}")
        disasm = subprocess.run([str(exe), "objdump", "--funcs", "wasm",
                                 "--bytes", str(binary)],
                                capture_output=True, text=True, check=True, timeout=60).stdout
        (target / (name + "." + mode + ".disasm")).write_text(disasm)
        for x, y in vectors(name):
            p = invoke(exe, binary, x, y, width)
            oracle = expected(name, x, y)
            if p.returncode != 0 or (int(p.stdout.strip()) & ((1 << width) - 1)) != oracle:
                raise AssertionError(f"{name}/{mode} incorrect x={x} y={y}: {p.stdout} {p.stderr}")
        traps = [(1, 0, "integer divide by zero")]
        if name == "swapped_operands":
            traps.append((0, 1, "integer divide by zero"))
        if name == "signed_i64":
            traps.append((1 << 63, (1 << 64) - 1, "integer overflow"))
        for x, y, reason in traps:
            p = invoke(exe, binary, x, y, width)
            if p.returncode == 0 or reason not in p.stderr.lower():
                raise AssertionError(f"{name}/{mode} missing {reason} trap: {p.stderr}")
        report["compilers"][mode] = {
            "compiler_sha256": sha(exe.read_bytes()),
            "aot_sha256": sha(binary.read_bytes()),
            "disassembly_sha256": sha(disasm.encode()),
            "finite_nontrapping_executions": len(vectors(name)),
            "traps_checked": len(traps),
        }
    if name in NEGATIVE_MATCH_CONTROLS:
        a, b = (report["compilers"][m]["disassembly_sha256"] for m in ("baseline", "patched"))
        if a != b:
            raise AssertionError(f"{name}: source guard matched outside declared op/width/operands")
        report["negative_matcher_screen"] = "identical emitted disassembly"
    else:
        report["negative_matcher_screen"] = "not asserted; compiler may legally change emitted bytes"
    return report


def run(root: Path):
    root = root.resolve()
    receipt = json.loads((root / "qualification.json").read_text())
    if receipt.get("schema") != "zeno-fcis/tier1-udivrem-same-wasm-compiler-ab/1" or receipt.get("upstream") != UPSTREAM:
        raise ValueError("missing pinned same-Wasm parent receipt")
    for mode in ("baseline", "patched"):
        exe = root / mode / "wasmtime"
        if sha(exe.read_bytes()) != receipt["results"][mode]["compiler_sha256"]:
            raise ValueError("compiler does not match archived parent qualification")
    out = root / "context-matrix"
    out.mkdir(exist_ok=False)
    results = {case: check_case(root, out, case) for case in sorted(CASES)}
    summary = {
        "schema": "zeno-fcis/tier1-udivrem-context-matrix/1",
        "upstream": UPSTREAM, "target": TARGET,
        "status": "finite-native-context-and-negative-controls-passed",
        "cases": results,
        "total_nontrapping_native_executions": sum(
            2 * v["bounded_vectors"] for v in results.values()),
        "total_trap_checks": sum(
            sum(c["traps_checked"] for c in v["compilers"].values()) for v in results.values()),
        "scope": ["two pinned compilers and identical WAT per case",
                  "independent Python value oracle; finite inputs only",
                  "negative source-matcher screens via emitted code identity",
                  "no whole-compiler correctness proof or runtime-throughput claim"],
    }
    (out / "qualification.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    return summary


def self_test():
    assert expected("unsigned_i32", MASK32, 2) == ((MASK32 // 2) ^ (MASK32 % 2))
    assert expected("signed_i64", (-7) & MASK64, 3) == ((-2 ^ -1) & MASK64)
    assert expected("different_blocks", 8, 3) == 2
    assert expected("different_blocks", 7, 3) == 2
    assert expected("swapped_operands", 7, 3) == 1
    for case in CASES:
        assert len(vectors(case)) >= 60
        try:
            expected(case, 1, 0)
        except ZeroDivisionError:
            pass
        else:
            raise AssertionError("missing zero-divisor oracle")
    print("source and oracle self-tests passed")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--archive-dir", type=Path)
    p.add_argument("--self-test", action="store_true")
    a = p.parse_args()
    if a.self_test:
        self_test()
    elif a.archive_dir:
        res = run(a.archive_dir)
        print(json.dumps({"status": res["status"],
                          "native_witnesses": res["total_nontrapping_native_executions"],
                          "traps": res["total_trap_checks"]}, sort_keys=True))
    else:
        p.error("specify --self-test or --archive-dir")
