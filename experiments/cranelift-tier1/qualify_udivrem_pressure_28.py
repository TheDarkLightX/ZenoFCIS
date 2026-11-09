#!/usr/bin/env python3
"""Supplemental same-Wasm x64 i64 div/rem register-pressure screen.

Reuses HASH-VERIFIED baseline and experimental compiler binaries from the
successful exact-source run #42596373104. Does not build or patch upstream,
does not assert profit, compiler correctness, or optimizer-rule proof.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import random
import re
import subprocess
from pathlib import Path

from qualify_dualresult_inprocess_28 import validate_producer, UPSTREAM, sha

MASK = (1 << 64) - 1
TARGET = "x86_64-unknown-linux"
SCHEMA = "zeno-fcis/tier1-udivrem-register-pressure-screen/1"
# pressure-0 is the positive control; reverse and quotient-only are guards.
CASES = {
    "pressure_0": (0, "forward"),
    "pressure_4": (4, "forward"),
    "pressure_12": (12, "forward"),
    "pressure_12_reverse": (12, "reverse"),
    "pressure_12_quotient_only": (12, "quotient"),
}

def signed(x: int) -> int:
    x &= MASK
    return x if x < (1 << 63) else x - (1 << 64)

def vectors() -> list[tuple[int, int]]:
    edges = [0, 1, 2, 7, -1, -3, -(1 << 63), (1 << 63)-1, 1 << 32]
    divisors = [0, 1, 2, 3, -1, -7, -(1 << 63), (1 << 63)-1]
    result = [(x, y) for x in edges for y in divisors]
    rng = random.Random(0x56232026)
    result += [(signed(rng.getrandbits(64)), signed(rng.getrandbits(64)))
               for _ in range(24)]
    return list(dict.fromkeys(result))

def source(pressure: int, mode: str) -> str:
    if pressure not in (0, 4, 12) or mode not in ("forward", "reverse", "quotient"):
        raise ValueError("unrecognized declared case")
    body = ["(module", '  (func (export "work") (param $x i64) (param $y i64) (result i64)']
    body += [f"    (local $l{i} i64)" for i in range(pressure)]
    for i in range(pressure):
        body += [
            "    local.get $x", f"    i64.const {2*i+3}", "    i64.mul",
            "    local.get $y", f"    i64.const {13*i+11}", "    i64.xor",
            "    i64.add", f"    local.set $l{i}",
        ]
    div = ["    local.get $x", "    local.get $y", "    i64.div_u"]
    rem = ["    local.get $x", "    local.get $y", "    i64.rem_u", "    i64.const 17", "    i64.rotl"]
    if mode == "forward":
        body += div + rem + ["    i64.xor"]
    elif mode == "reverse":
        body += rem + div + ["    i64.xor"]
    else:
        body += div
    for i in range(pressure):
        body += [f"    local.get $l{i}", "    i64.xor"]
    body += ["  )", ")", ""]
    return "\n".join(body)

def oracle(pressure: int, mode: str, x: int, y: int) -> int | None:
    a, b = x & MASK, y & MASK
    if b == 0:
        return None  # Wasm integer-divide-by-zero trap; no value asserted.
    q, r = divmod(a, b)
    answer = q if mode == "quotient" else q ^ (((r << 17) | (r >> 47)) & MASK)
    for i in range(pressure):
        extra = ((a * (2*i+3)) + (b ^ (13*i+11))) & MASK
        answer ^= extra
    return signed(answer)

def command(args: list[str], timeout=60) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, capture_output=True, text=True, timeout=timeout)

def native_cost(disasm: str) -> dict:
    rows = [row for row in disasm.splitlines()
            if re.match(r"^\s{12}[0-9a-f]{2}(?:\s|$)", row)]
    if not rows:
        raise ValueError("no native instruction rows in pinned disassembly")
    return {
        "decoded_instructions": len(rows),
        "static_divisions": sum(bool(re.search(r"\b(?:divq|divl|idivq|idivl)\b", row))
                                for row in rows),
    }

def qualify(archive: Path, out: Path) -> dict:
    if out.exists():
        raise ValueError("output already exists; refuse overwrite of receipts")
    archive = archive.resolve()
    producer = validate_producer(archive)  # validates original source, CI receipt, compiler SHA256, AOT.
    if not producer.get("artifacts"):
        raise ValueError("missing validated compiler artifacts")
    out.mkdir(parents=True)
    files = {}
    for name, (pressure, mode) in CASES.items():
        wat = source(pressure, mode)
        folder = out/name
        folder.mkdir()
        wat_file = folder/"input.wat"
        wat_file.write_text(wat)
        record = {"wat_sha256":sha(wat_file), "mode":mode, "live_values":pressure,
                  "contract":"unsigned i64 division/modulus; modulo 2^64 XOR/add/mul; trap on divisor zero",
                  "compilers":{}}
        for compiler in ("baseline", "patched"):
            exe = archive/compiler/"wasmtime"
            if sha(exe) != producer["artifacts"][compiler]["compiler_sha256"]:
                raise ValueError("producer compiler bytes changed")
            aot = folder/(compiler+".cwasm")
            res = command([str(exe), "compile", "--target", TARGET, "-o", str(aot),
                           str(wat_file)], timeout=180)
            if res.returncode:
                raise RuntimeError(f"{name}/{compiler} compile failed: {res.stderr}")
            dis = command([str(exe), "objdump", "--funcs", "wasm", "--bytes", str(aot)])
            if dis.returncode:
                raise RuntimeError(f"{name}/{compiler} disassembly failed: {dis.stderr}")
            disfile = folder/(compiler+".disasm")
            disfile.write_text(dis.stdout)
            value_count, trap_count = 0, 0
            for x, y in vectors():
                run = command([str(exe), "run", "--allow-precompiled", "--invoke", "work",
                               str(aot), str(x), str(y)])
                expected = oracle(pressure, mode, x, y)
                if expected is None:
                    if run.returncode == 0 or "trap" not in run.stderr.lower():
                        raise AssertionError(f"{name}/{compiler} missing trap x={x} y={y}: {run.stderr}")
                    trap_count += 1
                else:
                    if run.returncode != 0 or run.stdout.strip() != str(expected):
                        raise AssertionError(f"{name}/{compiler} value mismatch x={x} y={y}: "
                                             f"{run.stdout} {run.stderr}")
                    value_count += 1
            record["compilers"][compiler] = {
                "compiler_sha256":sha(exe), "aot_sha256":sha(aot),
                "aot_bytes":aot.stat().st_size, "disassembly_sha256":sha(disfile),
                "static_cost":native_cost(dis.stdout),
                "finite_value_checks":value_count, "finite_zero_divisor_traps":trap_count,
            }
        b, p = (record["compilers"][c] for c in ("baseline", "patched"))
        bdiv, pdiv = (v["static_cost"]["static_divisions"] for v in (b,p))
        if name == "pressure_0" and (bdiv,pdiv) != (2,1):
            raise AssertionError("positive control failed: no one-division fusion")
        if mode in ("reverse", "quotient") and bdiv != pdiv:
            raise AssertionError("out-of-scope operation order / quotient-only incorrectly fused")
        record["baseline_minus_patched"] = {
            "static_divisions":bdiv-pdiv,
            "decoded_instructions":b["static_cost"]["decoded_instructions"]-p["static_cost"]["decoded_instructions"],
            "whole_aot_bytes":b["aot_bytes"]-p["aot_bytes"],
        }
        files[name] = record
    receipt = {
        "schema":SCHEMA, "status":"finite-register-pressure-screen-passed",
        "upstream":UPSTREAM, "target":TARGET,
        "source_script_sha256":sha(Path(__file__).resolve()),
        "producer_run":42596373104, "producer":producer,
        "cases":files, "inputs_per_compiler_case":len(vectors()),
        "total_value_checks":sum(r["finite_value_checks"] for v in files.values()
                                  for r in v["compilers"].values()),
        "total_zero_divisor_traps":sum(r["finite_zero_divisor_traps"] for v in files.values()
                                       for r in v["compilers"].values()),
        "nonclaims":[
            "finite Wasm native CLI execution; NOT exhaustive i64 or whole-pass proof",
            "different WAT across cases, IDENTICAL WAT between compiler variants within a case",
            "static div and decoded instruction cost, not dynamic retired instructions or latency",
            "no flags/poison semantics inferred beyond declared Wasm unsigned integer operations",
            "no performance speedup or whole-AOT size advantage presumed",
        ],
    }
    (out/"qualification.json").write_text(json.dumps(receipt,sort_keys=True,indent=2)+"\n")
    return receipt

def self_test():
    for name, (pressure,mode) in CASES.items():
        wat = source(pressure,mode)
        assert wat.count("i64.div_u") == 1
        assert wat.count("i64.rem_u") == (0 if mode == "quotient" else 1)
        assert wat.count("(local $l") == pressure
        assert oracle(pressure,mode,7,3) is not None
        assert oracle(pressure,mode,1,0) is None
        assert wat.endswith("\n")
    assert oracle(0,"forward",7,3) == 2 ^ (1 << 17)
    assert oracle(0,"quotient",7,3) == 2
    assert oracle(0,"reverse",7,3) == oracle(0,"forward",7,3)
    assert len(vectors()) >= 75
    print("register-pressure WAT, unsigned oracle, and trap-domain controls passed")

if __name__ == "__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--self-test",action="store_true")
    parser.add_argument("--archive",type=Path)
    parser.add_argument("--out",type=Path)
    args=parser.parse_args()
    if args.self_test:
        self_test()
    else:
        if args.archive is None or args.out is None:
            parser.error("--archive and --out required")
        print(json.dumps({k:v for k,v in qualify(args.archive,args.out).items()
                          if k in ("status","total_value_checks","total_zero_divisor_traps")},
                         sort_keys=True))
