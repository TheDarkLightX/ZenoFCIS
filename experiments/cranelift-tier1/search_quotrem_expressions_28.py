#!/usr/bin/env python3
"""Bounded representation search for one unsigned 64-bit quotient/remainder use.

Human-authored finite WAT palette; actual pinned Cranelift compilation is the
cost oracle. This is NOT a newly Zeno-synthesized ISA sequence, an ISLE change,
or a proof of x86 machine-state/flags/ABI/trap-map equivalence.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

import audit_post_souper_inprocess_28 as identity
import qualify_udivrem_contexts_28 as matrix
import post_souper_portability_28 as portable
import stress_live_registers_28 as stress

SCHEMA = "zenofcis/tier1-quotrem-expression-palette/1"
UPSTREAM = "8b19f199f89da864c529b3fa3ca2f7e40d5de609"
TARGET = "x86_64-unknown-linux"
PALETTE = ("direct", "multiply_subtract", "commuted_product", "negative_product")
NEGATIVE = "wrong_plus"
MODES = ("baseline", "patched")
MASK = (1 << 64) - 1
PROOF_TIMEOUT_MS = 30000
SEMANTICS = {
    "x": "unsigned i64 word", "y": "unsigned i64 word",
    "domain": "all pairs of i64 bit patterns; y=0 traps at the first udiv",
    "defined": "y != 0: q=unsigned(x/y), r=unsigned(x%y), result=q XOR rotl64(r,17)",
    "overflow": "intermediate i64 multiply/subtract/add wrap modulo 2^64",
    "flags": "not observable in guest WAT; physical x86 flags not proved",
    "poison_undefined": "no poison/undefined in these Wasm integer operators; machine lowering unproved",
    "target": TARGET,
}

def digest(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        raise ValueError("missing or linked evidence: " + str(path))
    return hashlib.sha256(path.read_bytes()).hexdigest()

def code_for(name: str) -> str:
    if name == "direct":
        return "local.get $x\nlocal.get $y\ni64.rem_u"
    if name == "multiply_subtract":
        return "local.get $x\nlocal.get $q\nlocal.get $y\ni64.mul\ni64.sub"
    if name == "commuted_product":
        return "local.get $x\nlocal.get $y\nlocal.get $q\ni64.mul\ni64.sub"
    if name == "negative_product":
        return "local.get $x\nlocal.get $q\nlocal.get $y\ni64.mul\ni64.const -1\ni64.mul\ni64.add"
    if name == NEGATIVE:
        return "local.get $x\nlocal.get $q\nlocal.get $y\ni64.mul\ni64.add"
    raise ValueError("outside bounded palette")

def wat(name: str) -> str:
    remainder = "\n    ".join(code_for(name).splitlines())
    return (
        '(module\n  (func (export "work") (param $x i64) (param $y i64) (result i64)\n'
        '    (local $q i64) (local $r i64)\n'
        '    local.get $x\n    local.get $y\n    i64.div_u\n    local.set $q\n'
        '    ' + remainder + '\n    local.set $r\n'
        '    local.get $q\n    local.get $r\n    i64.const 17\n'
        '    i64.rotl\n    i64.xor\n  )\n)\n'
    )

def r_value(name: str, x: int, y: int, bits: int = 64) -> int | None:
    mask = (1 << bits) - 1
    x, y = x & mask, y & mask
    if y == 0:
        return None
    q = x // y
    if name == "direct":
        return x % y
    if name in ("multiply_subtract", "commuted_product"):
        return (x - (q * y)) & mask
    if name == "negative_product":
        return (x + ((-q * y) & mask)) & mask
    if name == NEGATIVE:
        return (x + q * y) & mask
    raise ValueError("unadmitted expression")

def exhaustive8(name: str) -> dict:
    ok = bad = 0
    witness = None
    for x in range(256):
        for y in range(256):
            got = r_value(name, x, y, 8)
            want = None if y == 0 else x % y
            if got != want:
                bad += 1
                if witness is None:
                    witness = {"x": x, "y": y, "got": got, "expected": want}
            else:
                ok += 1
    return {"status": "exhaustive-8-bit-finite", "pairs": 65536,
            "agree": ok, "disagree": bad, "first_counterexample": witness}

def proof(name: str) -> dict:
    import z3
    x, y = z3.BitVecs("x y", 64)
    q = z3.UDiv(x, y)
    if name == "direct":
        r = z3.URem(x, y)
    elif name in ("multiply_subtract", "commuted_product"):
        r = x - q * y
    elif name == "negative_product":
        r = x + (-(q * y))
    elif name == NEGATIVE:
        r = x + q * y
    else:
        raise ValueError("unadmitted expression")
    s = z3.Solver()
    s.set(timeout=PROOF_TIMEOUT_MS)
    s.add(y != z3.BitVecVal(0, 64))
    s.add(r != z3.URem(x, y))
    result = s.check()
    status = str(result)
    data = {"logic": "QF_BV", "domain": "all unsigned 64-bit x,y; y!=0",
            "counterexample_query": "candidate_remainder != URem(x,y)",
            "solver": z3.get_version_string(), "timeout_ms": PROOF_TIMEOUT_MS,
            "status": status, "qualification": (
                "proved-unsat" if result == z3.unsat else
                "counterexample-sat" if result == z3.sat else "unknown")}
    if result == z3.sat:
        m = s.model()
        data["counterexample"] = {
            "x": m.eval(x, model_completion=True).as_long(),
            "y": m.eval(y, model_completion=True).as_long()}
    if result == z3.unknown:
        data["reason_unknown"] = s.reason_unknown()
    return data

def vectors() -> tuple[tuple[int, int], ...]:
    return stress.vectors()

def oracle(x: int, y: int) -> int | None:
    ux, uy = x & MASK, y & MASK
    if uy == 0:
        return None
    q, r = divmod(ux, uy)
    return stress.signed(q ^ stress.rotl(r, 17))

def finite(exe: Path, aot: Path) -> dict:
    good = trapped = 0
    for x, y in vectors():
        p = subprocess.run(
            [str(exe), "run", "--allow-precompiled", "--invoke", "work",
             str(aot), str(x), str(y)],
            text=True, capture_output=True, timeout=40)
        expected = oracle(x, y)
        if expected is None:
            if p.returncode == 0 or "trap" not in p.stderr.lower():
                raise AssertionError("missing zero-divisor trap")
            trapped += 1
        else:
            if p.returncode != 0 or not re.fullmatch(r"-?\d+", p.stdout.strip()):
                raise AssertionError("unexpected guest error: " + p.stderr[-500:])
            if int(p.stdout.strip()) != expected:
                raise AssertionError("incorrect guest unsigned quotient/remainder")
            good += 1
    return {"value": good, "zero_divisor_trap": trapped}

def pins(root: Path) -> dict:
    receipt = identity.producer(root)
    if portable.UPSTREAM != UPSTREAM or stress.PIN != UPSTREAM:
        raise ValueError("upstream revision is inconsistent")
    return {name: receipt["compiler_pins"][name]["sha256"] for name in MODES}

def target_compile(exe: Path, watfile: Path, dest: Path, dis: Path) -> dict:
    p = subprocess.run([str(exe), "compile", "--target", TARGET,
                        "-o", str(dest), str(watfile)],
                       capture_output=True, text=True, timeout=180)
    if p.returncode or not dest.is_file():
        raise RuntimeError("pinned compiler failure: " + p.stderr[-800:])
    p = subprocess.run([str(exe), "objdump", "--funcs", "wasm",
                        "--bytes", str(dest)],
                       capture_output=True, text=True, timeout=120)
    if p.returncode:
        raise RuntimeError("pinned objdump failure: " + p.stderr[-800:])
    dis.write_text(p.stdout)
    c = matrix.cost(p.stdout)
    return {"aot_sha256": digest(dest), "aot_bytes": dest.stat().st_size,
            "disassembly_sha256": digest(dis),
            "native_instruction_bytes": portable.rendered_bytes(p.stdout),
            "decoded_instructions": c["decoded_instructions"],
            "static_divisions": c["static_divisions"]}

def stable_key(name: str) -> str:
    payload = json.dumps({"semantic": SEMANTICS, "wat": wat(name)},
                         sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(payload).hexdigest()

def run(root: Path, out: Path) -> None:
    if out.exists():
        raise ValueError("refuse to overwrite an evidence directory")
    hashes = pins(root)
    out.mkdir(parents=True)
    rejected = exhaustive8(NEGATIVE)
    negative_proof = proof(NEGATIVE)
    if rejected["disagree"] == 0 or negative_proof["status"] != "sat":
        raise AssertionError("known-incorrect palette control was not rejected")
    entries = {}
    for name in PALETTE:
        eight = exhaustive8(name)
        verdict = proof(name)
        if eight["disagree"] or verdict["status"] == "sat":
            raise AssertionError("candidate refuted; cannot retain in comparison")
        folder = out / name
        folder.mkdir()
        src = folder / "input.wat"
        src.write_text(wat(name))
        candidates = {}
        for mode in MODES:
            exe = (root / mode / "wasmtime").resolve()
            aot = folder / (mode + ".cwasm")
            dis = folder / (mode + ".disassembly.txt")
            measured = target_compile(exe, src, aot, dis)
            measured["finite"] = finite(exe, aot)
            candidates[mode] = measured
        key = stable_key(name)
        if key in entries:
            raise AssertionError("duplicate catalog identity")
        entries[key] = {
            "key": key, "name": name, "input_sha256": digest(src),
            "exhaustive8": eight, "full_width_smt": verdict,
            "verification_class": (
                "QF_BV-64-proved-and-finite-native" if verdict["status"] == "unsat"
                else "finite-tested-SMT-unknown"),
            "cost": candidates, "provenance": "human-authored finite palette",
        }
    by_name = {row["name"]: row for row in entries.values()}
    baseline = by_name["direct"]
    for row in entries.values():
        row["vs_direct"] = {mode: {
            "native_byte_delta": (baseline["cost"][mode]["native_instruction_bytes"]
                                  - row["cost"][mode]["native_instruction_bytes"]),
            "aot_byte_delta": (baseline["cost"][mode]["aot_bytes"]
                               - row["cost"][mode]["aot_bytes"])}
            for mode in MODES}
    qualified = [row for row in entries.values()
                 if row["full_width_smt"]["status"] == "unsat"]
    winner = {mode: min(qualified,
                        key=lambda row: (row["cost"][mode]["native_instruction_bytes"],
                                         row["key"]))["name"]
              for mode in MODES}
    head = subprocess.check_output(["git", "rev-parse", "HEAD"],
                                   text=True).strip()
    doc = {"schema": SCHEMA, "source_commit": head, "wasmtime_commit": UPSTREAM,
           "target": TARGET, "compiler_sha256": hashes,
           "script_sha256": digest(Path(__file__)), "semantic_contract": SEMANTICS,
           "palette": list(PALETTE), "max_candidates": len(PALETTE),
           "search_stop_reason": "candidate palette exhausted",
           "negative_control": {"exhaustive8": rejected, "smt": negative_proof},
           "entries": sorted(entries.values(), key=lambda r: r["key"]),
           "winners_on_native_bytes_only": winner,
           "limits": [
               "WAT representation search over human-authored candidates; not ZenoFCIS native instruction synthesis",
               "Even a QF_BV proof is for the WAT arithmetic expression, not a compiler-pass, flags, ISA, or trap-map proof",
               "Exhaustive 8-bit checking is not exhaustive 64-bit checking",
               "SMT unknown is not equivalence, and finite native tests are never promoted to proof",
               "Native decoded instruction bytes, AOT file bytes and CLIF costs are separate metrics",
               "One pinned x86-64 Linux compiler and workload; no representative throughput or all-target profitability",
               "Any future upstream rewrite requires independent ISLE/VeriISLE and machine-state checks",
           ]}
    (out / "catalog.json").write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": "bounded-palette-measured", "winners": winner,
                      "qualified": [r["name"] for r in qualified],
                      "smt": {r["name"]: r["full_width_smt"]["status"]
                              for r in entries.values()}}, sort_keys=True))

def audit(root: Path, out: Path) -> None:
    d = json.loads((out / "catalog.json").read_text())
    head = subprocess.check_output(["git", "rev-parse", "HEAD"],
                                   text=True).strip()
    if (d.get("schema") != SCHEMA or d.get("source_commit") != head
        or d.get("script_sha256") != digest(Path(__file__))
        or d.get("wasmtime_commit") != UPSTREAM or d.get("target") != TARGET
        or d.get("compiler_sha256") != pins(root)
        or d.get("semantic_contract") != SEMANTICS
        or d.get("palette") != list(PALETTE)
        or d.get("max_candidates") != len(PALETTE)
        or d.get("search_stop_reason") != "candidate palette exhausted"):
        raise ValueError("catalog/source/compiler/contract pin drift")
    if d.get("negative_control") != {
        "exhaustive8": exhaustive8(NEGATIVE), "smt": proof(NEGATIVE)}:
        raise ValueError("negative control receipt drift")
    entries = d.get("entries")
    if not isinstance(entries, list) or len(entries) != len(PALETTE):
        raise ValueError("candidate catalog missing entries")
    by_name = {}
    for row in entries:
        name = row["name"]
        if name not in PALETTE or name in by_name or row["key"] != stable_key(name):
            raise ValueError("duplicate or modified candidate identity")
        folder = out / name
        src = folder / "input.wat"
        if src.read_text() != wat(name) or digest(src) != row["input_sha256"]:
            raise ValueError("candidate input drift")
        if row["exhaustive8"] != exhaustive8(name) or row["full_width_smt"] != proof(name):
            raise ValueError("equivalence/unknown/counterexample receipt drift")
        expected_class = ("QF_BV-64-proved-and-finite-native"
                          if row["full_width_smt"]["status"] == "unsat"
                          else "finite-tested-SMT-unknown")
        if row["verification_class"] != expected_class:
            raise ValueError("false equivalence classification")
        for mode in MODES:
            exe = (root / mode / "wasmtime").resolve()
            aot = folder / (mode + ".cwasm")
            dis = folder / (mode + ".disassembly.txt")
            c = matrix.cost(dis.read_text())
            recorded = row["cost"][mode]
            if (digest(aot) != recorded["aot_sha256"]
                or aot.stat().st_size != recorded["aot_bytes"]
                or digest(dis) != recorded["disassembly_sha256"]
                or portable.rendered_bytes(dis.read_text()) != recorded["native_instruction_bytes"]
                or c["decoded_instructions"] != recorded["decoded_instructions"]
                or c["static_divisions"] != recorded["static_divisions"]
                or finite(exe, aot) != recorded["finite"]):
                raise ValueError("native code, cost or value/trap replay drift")
        by_name[name] = row
    if set(by_name) != set(PALETTE):
        raise ValueError("candidate palette changed")
    baseline = by_name["direct"]
    for row in entries:
        expected_delta = {mode: {
            "native_byte_delta": (baseline["cost"][mode]["native_instruction_bytes"]
                                  - row["cost"][mode]["native_instruction_bytes"]),
            "aot_byte_delta": (baseline["cost"][mode]["aot_bytes"]
                               - row["cost"][mode]["aot_bytes"])}
            for mode in MODES}
        if row.get("vs_direct") != expected_delta:
            raise ValueError("cost deltas fabricated")
    qualified = [r for r in entries if r["full_width_smt"]["status"] == "unsat"]
    winners = {mode: min(qualified,
                         key=lambda row: (row["cost"][mode]["native_instruction_bytes"],
                                          row["key"]))["name"]
               for mode in MODES}
    if winners != d["winners_on_native_bytes_only"]:
        raise ValueError("winner was not derived from verified native code")
    print("Bounded palette audited: source/solver, 8-bit exhaustion, AOT, native costs and guest replay")

def self_test() -> None:
    assert UPSTREAM == stress.PIN == portable.UPSTREAM
    assert set(PALETTE).isdisjoint({NEGATIVE})
    assert len({stable_key(x) for x in PALETTE}) == len(PALETTE)
    assert all(wat(x).count("i64.div_u") == 1 for x in PALETTE)
    assert wat("direct").count("i64.rem_u") == 1
    assert all(wat(x).count("i64.rem_u") == 0 for x in PALETTE[1:])
    assert all(exhaustive8(x)["disagree"] == 0 for x in PALETTE)
    assert exhaustive8(NEGATIVE)["disagree"] > 0
    assert oracle(7, 3) == 2 ^ stress.rotl(1, 17)
    assert oracle(7, 0) is None
    print("Four WAT representations, exhaustive-8 oracle and deliberate incorrect variant preflight passed")

def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--mode", choices=("self-test", "run", "audit"), required=True)
    p.add_argument("--root", type=Path)
    p.add_argument("--out", type=Path)
    a = p.parse_args()
    if a.mode == "self-test":
        self_test()
    elif a.root is None or a.out is None:
        p.error("--root and --out required")
    elif a.mode == "run":
        run(a.root, a.out)
    else:
        audit(a.root, a.out)

if __name__ == "__main__":
    main()
