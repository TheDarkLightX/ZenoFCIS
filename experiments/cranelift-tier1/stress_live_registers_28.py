#!/usr/bin/env python3
"""Finite compiler-state stress for pinned post-Souper Cranelift udiv/urem fusion.

Separate baseline and experimental Wasmtime compilers receive identical WAT.
Checks live-value pressure, arithmetic use of both results, and a mismatched-
operand negative control against an independently expressed Python oracle.
This is not universal compiler correctness, an x86 flags proof, or synthesis.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import random
import re
import subprocess
from pathlib import Path

import audit_post_souper_inprocess_28 as identity
import post_souper_portability_28 as portability
import qualify_udivrem_contexts_28 as matrix

MASK = (1 << 64) - 1
TARGET = "x86_64-unknown-linux"
PIN = "8b19f199f89da864c529b3fa3ca2f7e40d5de609"
SCHEMA = "zenofcis/tier1-register-pressure-contexts/1"
KINDS = ("pressure", "arithmetic_reuse", "mismatched_operands")
MODES = ("baseline", "patched")

def sha(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        raise ValueError("missing or linked evidence file: " + str(path))
    return hashlib.sha256(path.read_bytes()).hexdigest()

def u(x: int) -> int:
    return x & MASK

def signed(x: int) -> int:
    x &= MASK
    return x if x < (1 << 63) else x - (1 << 64)

def rotl(x: int, k: int) -> int:
    x &= MASK
    return ((x << k) | (x >> (64 - k))) & MASK if k else x

def wats(kind: str) -> str:
    if kind not in KINDS:
        raise ValueError("unadmitted fixture")
    head = '(module\n  (func (export "work") (param $x i64) (param $y i64) (result i64)\n'
    q = "local.get $x\nlocal.get $y\ni64.div_u\nlocal.set $q\n"
    r = "local.get $x\nlocal.get $y\ni64.rem_u\nlocal.set $r\n"
    if kind == "pressure":
        lines = ["(local $q i64) (local $r i64)"] + [
            "(local $v%d i64)" % i for i in range(8)]
        for i in range(8):
            lines += [f"local.get $x\ni64.const {7*i+1}\ni64.rotl\n"
                      f"local.get $y\ni64.const {101+23*i}\ni64.xor\n"
                      f"i64.add\nlocal.set $v{i}"]
        lines += [q, r, "local.get $q\nlocal.get $r\ni64.xor"]
        for i in range(8):
            lines.append(f"local.get $v{i}\ni64.xor")
    elif kind == "arithmetic_reuse":
        lines = ["(local $q i64) (local $r i64)", q, r,
                 "local.get $q\ni64.const 7\ni64.mul",
                 "local.get $r\ni64.const 13\ni64.mul",
                 "i64.add", "local.get $q\nlocal.get $r\ni64.xor", "i64.add"]
    else:
        lines = ["local.get $x\nlocal.get $y\ni64.div_u",
                 "local.get $y\nlocal.get $x\ni64.rem_u", "i64.xor"]
    return head + "".join("    " + line + "\n" for line in lines) + "  )\n)\n"

def oracle(kind: str, x: int, y: int) -> int | None:
    x, y = u(x), u(y)
    if y == 0 or (kind == "mismatched_operands" and x == 0):
        return None
    q, r = divmod(x, y)
    if kind == "pressure":
        value = q ^ r
        for i in range(8):
            value ^= u(rotl(x, 7*i+1) + (y ^ (101 + 23*i)))
    elif kind == "arithmetic_reuse":
        value = u(7*q + 13*r + (q ^ r))
    elif kind == "mismatched_operands":
        value = q ^ (y % x)
    else:
        raise ValueError("unadmitted fixture")
    return signed(value)

def vectors() -> tuple[tuple[int, int], ...]:
    edges = (0, 1, 2, 3, 7, (1 << 31) - 1, 1 << 31,
             (1 << 63) - 1, 1 << 63, MASK - 1, MASK)
    pairs = [(signed(x), signed(y)) for x in edges for y in (0, 1, 2, 7, MASK)]
    rng = random.Random(0xC1A7F2026)
    pairs += [(signed(rng.getrandbits(64)), signed(rng.getrandbits(64)))
              for _ in range(32)]
    return tuple(dict.fromkeys(pairs))

def call(exe: Path, aot: Path, x: int, y: int) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(exe), "run", "--allow-precompiled", "--invoke", "work",
         str(aot), str(x), str(y)], text=True, capture_output=True, timeout=45)

def finite_check(exe: Path, aot: Path, kind: str) -> dict:
    counts = {"value": 0, "trap": 0}
    for x, y in vectors():
        p = call(exe, aot, x, y)
        want = oracle(kind, x, y)
        if want is None:
            if p.returncode == 0 or "trap" not in p.stderr.lower():
                raise AssertionError(f"expected trap for {kind}({x},{y})")
            counts["trap"] += 1
        else:
            if p.returncode != 0 or not re.fullmatch(r"-?\d+", p.stdout.strip()):
                raise AssertionError(f"expected value for {kind}({x},{y}): {p.stderr[-400:]}")
            if int(p.stdout.strip()) != want:
                raise AssertionError(f"incorrect {kind}({x},{y}): {p.stdout.strip()} != {want}")
            counts["value"] += 1
    return counts

def compiler_pins(root: Path) -> dict:
    receipt = identity.producer(root)
    return {mode: receipt["compiler_pins"][mode]["sha256"] for mode in MODES}

def run(root: Path, out: Path) -> None:
    if out.exists():
        raise ValueError("refuse existing output")
    pins = compiler_pins(root)
    out.mkdir(parents=True)
    cases = {}
    for kind in KINDS:
        directory = out / kind
        directory.mkdir()
        src = directory / "source.wat"
        src.write_text(wats(kind))
        results = {}
        for mode in MODES:
            exe = (root / mode / "wasmtime").resolve()
            aot = directory / (mode + ".cwasm")
            dis = directory / (mode + ".disassembly.txt")
            cmd = subprocess.run(
                [str(exe), "compile", "--target", TARGET, "-o", str(aot), str(src)],
                text=True, capture_output=True, timeout=180)
            if cmd.returncode:
                raise RuntimeError("compile failure: " + cmd.stderr[-1000:])
            obj = subprocess.run(
                [str(exe), "objdump", "--funcs", "wasm", "--bytes", str(aot)],
                text=True, capture_output=True, timeout=120)
            if obj.returncode:
                raise RuntimeError("objdump failure: " + obj.stderr[-1000:])
            dis.write_text(obj.stdout)
            costs = matrix.cost(obj.stdout)
            results[mode] = {
                "aot_sha256": sha(aot), "aot_bytes": aot.stat().st_size,
                "disassembly_sha256": sha(dis),
                "decoded_native_instruction_bytes": portability.rendered_bytes(obj.stdout),
                "static_divisions": costs["static_divisions"],
                "decoded_instructions": costs["decoded_instructions"],
                "finite": finite_check(exe, aot, kind)}
        if kind == "mismatched_operands":
            if results["baseline"]["static_divisions"] != results["patched"]["static_divisions"]:
                raise AssertionError("mismatched operands were incorrectly fused")
            if results["baseline"]["aot_sha256"] != results["patched"]["aot_sha256"]:
                raise AssertionError("unmatched control AOT bytes changed")
        cases[kind] = {
            "wat_sha256": sha(src), "modes": results,
            "native_byte_saving": (results["baseline"]["decoded_native_instruction_bytes"]
                                   - results["patched"]["decoded_native_instruction_bytes"]),
            "native_div_reduction": (results["baseline"]["static_divisions"]
                                     - results["patched"]["static_divisions"])}
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    manifest = {
        "schema": SCHEMA, "status": "finite-native-qualified",
        "zenofcis_commit": head, "wasmtime_commit": PIN, "target": TARGET,
        "producer_run": identity.PRODUCER_RUN,
        "producer_receipt_sha256": identity.PRODUCER_RECEIPT_SHA,
        "compiler_sha256": pins,
        "script_sha256": sha(Path(__file__)),
        "finite_vectors_per_case_per_compiler": len(vectors()),
        "cases": cases,
        "limits": [
            "Finite guest-Wasm value/trap executions, not compiler-pass, ISA or arbitrary flag/ABI proof",
            "Single pinned x86-64 target; no cross-target or poison/undefined behavior model",
            "Decoded per-function native bytes differ from CLIF cost and whole-AOT bytes",
            "No newly synthesized instruction sequence and no representative profitability claim",
            "Negative control guards against mismatched-operand fusion"
        ]}
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": manifest["status"],
                      "vectors_per_mode": len(vectors()),
                      "deltas": {k: [v["native_byte_saving"], v["native_div_reduction"]]
                                 for k, v in cases.items()}}, sort_keys=True))

def audit(root: Path, out: Path) -> None:
    pins = compiler_pins(root)
    doc = json.loads((out / "manifest.json").read_text())
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if (doc.get("schema") != SCHEMA or doc.get("zenofcis_commit") != head
        or doc.get("wasmtime_commit") != PIN or doc.get("target") != TARGET
        or doc.get("producer_run") != identity.PRODUCER_RUN
        or doc.get("producer_receipt_sha256") != identity.PRODUCER_RECEIPT_SHA
        or doc.get("compiler_sha256") != pins
        or doc.get("script_sha256") != sha(Path(__file__))
        or doc.get("finite_vectors_per_case_per_compiler") != len(vectors())
        or set(doc.get("cases", {})) != set(KINDS)):
        raise ValueError("source, compiler, schema or fixture drift")
    for kind in KINDS:
        directory = out / kind
        src = directory / "source.wat"
        row = doc["cases"][kind]
        if src.read_text() != wats(kind) or sha(src) != row["wat_sha256"]:
            raise ValueError("WAT semantics drift")
        for mode in MODES:
            exe = (root / mode / "wasmtime").resolve()
            aot = directory / (mode + ".cwasm")
            dis = directory / (mode + ".disassembly.txt")
            entry = row["modes"][mode]
            costs = matrix.cost(dis.read_text())
            if (sha(aot) != entry["aot_sha256"] or aot.stat().st_size != entry["aot_bytes"]
                or sha(dis) != entry["disassembly_sha256"]
                or portability.rendered_bytes(dis.read_text()) != entry["decoded_native_instruction_bytes"]
                or costs["static_divisions"] != entry["static_divisions"]
                or costs["decoded_instructions"] != entry["decoded_instructions"]
                or finite_check(exe, aot, kind) != entry["finite"]):
                raise ValueError("cost, binary or independently replayed result drift")
        b, p = (row["modes"][m] for m in MODES)
        if (row["native_byte_saving"] != b["decoded_native_instruction_bytes"] - p["decoded_native_instruction_bytes"]
            or row["native_div_reduction"] != b["static_divisions"] - p["static_divisions"]):
            raise ValueError("fabricated cost delta")
        if kind == "mismatched_operands" and (b["aot_sha256"] != p["aot_sha256"]
            or row["native_div_reduction"] != 0):
            raise ValueError("negative control changed")
    print("Pinned two-compiler register stress: WAT, AOT, disassembly and finite native replay audited")

def self_test() -> None:
    assert all(wats(k).count("(module") == 1 for k in KINDS)
    assert wats("pressure").count("(local $v") == 8
    assert wats("mismatched_operands").count("i64.rem_u") == 1
    assert oracle("mismatched_operands", 0, 7) is None
    assert oracle("arithmetic_reuse", 7, 3) == signed(7*2 + 13*1 + (2 ^ 1))
    assert oracle("pressure", -1, 7) is not None
    assert len(vectors()) >= 60
    print("Deterministic, bounded live-register arithmetic/trap oracle self-test passed")

def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--mode", choices=("self-test", "run", "audit"), required=True)
    p.add_argument("--source-root", type=Path)
    p.add_argument("--out", type=Path)
    args = p.parse_args()
    if args.mode == "self-test":
        self_test()
    elif args.source_root is None or args.out is None:
        p.error("--source-root and --out required")
    elif args.mode == "run":
        run(args.source_root, args.out)
    else:
        audit(args.source_root, args.out)

if __name__ == "__main__":
    main()
