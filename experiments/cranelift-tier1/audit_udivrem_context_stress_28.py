#!/usr/bin/env python3
"""Independent finite artifact audit for the 14-context dual-result division screen.

Rehash original WAT/AOT/disassembly and both compiler binaries; independently
recount x86 divisions and verify recorded trap totals. Execute the three added
adversarial contexts against a separate Python arithmetic oracle. This is
finite evidence, not full-width SMT, a compiler-pass proof, or a speed claim.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path
from qualify_udivrem_contexts_28 import CASES, vectors

PIN = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
MASK = (1 << 64) - 1
MIN = -(1 << 63)
NOVEL = {"two_live_pairs", "exclusive_branches", "long_lived_results"}
NAMES = {
    "adjacent", "reverse_order", "swapped_rem_inputs", "quotient_only",
    "remainder_only", "signed_64", "unsigned_32", "signed_32",
    "intervening_add", "intervening_load", "branch", *NOVEL,
}
SAMPLES = (
    (0, 1), (1, 1), (7, 3), (-1, -1), (MIN, -1), (1, 0),
    (MIN, 1), (-(1 << 32), 2), (-7, 3),
)

def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def s64(n: int) -> int:
    return ((n & MASK) ^ (1 << 63)) - (1 << 63)

def rol(n: int, k: int) -> int:
    n &= MASK
    return ((n << k) | (n >> (64 - k))) & MASK

def ror(n: int, k: int) -> int:
    n &= MASK
    return ((n >> k) | (n << (64 - k))) & MASK

def traps(case: str, x: int, y: int) -> bool:
    ux, uy = x & MASK, y & MASK
    if case == "swapped_rem_inputs":
        return ux == 0 or uy == 0
    if case == "two_live_pairs":
        return uy in (0, MASK)
    if case in ("signed_32", "unsigned_32"):
        yy = y & 0xffffffff
        return yy == 0 or (
            case == "signed_32" and (x & 0xffffffff) == (1 << 31)
            and yy == 0xffffffff
        )
    return uy == 0 or (case == "signed_64" and x == MIN and y == -1)

def expected(case: str, x: int, y: int) -> int | None:
    if traps(case, x, y):
        return None
    ux, uy = x & MASK, y & MASK
    q, r = divmod(ux, uy)
    if case == "two_live_pairs":
        q2, r2 = divmod((ux + 1) & MASK, (uy + 1) & MASK)
        return s64((q ^ rol(r, 17)) ^ (q2 ^ rol(r2, 17)))
    if case == "exclusive_branches":
        return s64(q if not (ux & 1) else r)
    if case == "long_lived_results":
        return s64(q ^ rol(ux, 13) ^ ror(uy, 7) ^ rol(r, 17))
    raise ValueError("independent value oracle restricted to three novel cases")

def native_counts(disasm: str) -> dict:
    # CLI objdump's 12-space-prefixed decoded x86 instruction rows.
    rows = [line for line in disasm.splitlines()
            if re.match(r"^ {12}[0-9a-fA-F]{2}(?:\s|$)", line)]
    if not rows:
        raise ValueError("no decodable instruction lines")
    return {
        "decoded_instructions": len(rows),
        "static_divisions": sum(bool(re.search(r"\b(?:idiv|div)(?:q|l)?\b", line))
                                for line in rows),
    }

def replay(exe: Path, aot: Path, case: str) -> dict:
    counts = {"value": 0, "trap": 0}
    for x, y in SAMPLES:
        p = subprocess.run([
            str(exe), "run", "--allow-precompiled", "--invoke", "work",
            str(aot), str(x), str(y),
        ], capture_output=True, text=True, timeout=60)
        want = expected(case, x, y)
        if want is None:
            if p.returncode == 0 or not re.search(
                r"integer div(?:ide|ision) by zero", p.stderr, re.IGNORECASE
            ):
                raise AssertionError(f"{case}({x},{y}): missing precise zero-divisor trap")
            counts["trap"] += 1
        else:
            if p.returncode != 0 or p.stdout.strip() != str(want):
                raise AssertionError(
                    f"{case}({x},{y}): expected={want}, got={p.stdout!r}, err={p.stderr!r}"
                )
            counts["value"] += 1
    return counts

def audit(root: Path) -> dict:
    out = root / "context-controls"
    receipt_path = out / "qualification.json"
    result_path = out / "independent-stress-audit.json"
    if result_path.exists():
        raise ValueError("refusing to overwrite existing independent receipt")
    data = json.loads(receipt_path.read_text())
    if (data.get("schema") != "zenofcis/tier1-udivrem-contexts/1"
        or data.get("upstream_source_pin") != PIN
        or (root / "wasmtime-head.txt").read_text().strip() != PIN):
        raise ValueError("missing pinned provenance")
    if (set(data["cases"]) != NAMES or set(CASES) != NAMES
        or data.get("tests_per_mode_per_case") != len(vectors())):
        raise ValueError("changed case contract or vector count")
    binaries = {}
    for mode in ("baseline", "patched"):
        exe = (root / mode / "wasmtime").resolve()
        pin = data["compiler_pins"][mode]
        if not exe.is_file() or digest(exe) != pin["sha256"]:
            raise ValueError("compiler binary identity mismatch")
        binaries[mode] = exe
    if data["compiler_pins"]["baseline"]["version"] != data["compiler_pins"]["patched"]["version"]:
        raise ValueError("compiler version identity mismatch")

    measurements, replays = {}, {}
    for case in sorted(NAMES):
        folder = out / case
        record = data["cases"][case]
        input_wat = folder / "input.wat"
        if (input_wat.read_text() != CASES[case]
            or digest(input_wat) != record["wat_sha256"]):
            raise ValueError(f"{case}: WAT source mismatch")
        outcomes = {}
        for mode in ("baseline", "patched"):
            row = record["results"][mode]
            aot = folder / (mode + ".cwasm")
            dis = folder / (mode + ".disassembly.txt")
            if (digest(aot) != row["aot_sha256"]
                or aot.stat().st_size != row["aot_bytes"]
                or digest(dis) != row["disassembly_sha256"]):
                raise ValueError(f"{case}/{mode}: AOT/disassembly altered")
            actual = native_counts(dis.read_text())
            if actual != row["static_cost"]:
                raise ValueError(f"{case}/{mode}: native instruction receipt altered")
            expected_traps = sum(traps(case, x, y) for x, y in vectors())
            if row["finite"] != {
                "trap": expected_traps,
                "value": len(vectors()) - expected_traps,
            }:
                raise ValueError(f"{case}/{mode}: finite trap totals inconsistent")
            outcomes[mode] = actual
            if case in NOVEL:
                replays[f"{case}/{mode}"] = replay(binaries[mode], aot, case)
        saving = outcomes["baseline"]["static_divisions"] - outcomes["patched"]["static_divisions"]
        if saving != record["static_div_savings"]:
            raise ValueError(f"{case}: forged division cost difference")
        measurements[case] = saving

    evidence = {
        "schema": "zenofcis/tier1-udivrem-independent-context-stress/1",
        "status": "passed-narrow-finite-native-aot-replay-and-archive-integrity",
        "wasmtime_source_pin": PIN,
        "zenofcis_experiment_pin": (root / "zenofcis-head.txt").read_text().strip(),
        "source_context_runner_sha256": digest(Path(__file__).with_name("qualify_udivrem_contexts_28.py")),
        "auditor_sha256": digest(Path(__file__)),
        "source_receipt_sha256": digest(receipt_path),
        "total_cases": len(NAMES),
        "vectors_per_case_per_compiler": len(vectors()),
        "declared_finite_compiler_calls": len(NAMES) * len(vectors()) * 2,
        "independent_novel_case_replays": sum(sum(item.values()) for item in replays.values()),
        "new_case_replay_details": replays,
        "division_savings_by_case": measurements,
        "limitations": [
            "Independent arithmetic re-execution covers three added cases, nine chosen inputs each, two compiler AOTs",
            "Other contexts receive separately recomputed archive hashes, costs and finite trap totals, not fresh value-by-value replay",
            "Compiled code is x86-64 and pinned Wasmtime; this is not SMT or complete compiler-pass correctness",
            "Static division-count changes do not establish a native speedup, AOT reduction or a newly synthesized discovery",
        ],
    }
    result_path.write_text(json.dumps(evidence, sort_keys=True, indent=2) + "\n")
    return evidence

def self_test() -> None:
    assert set(CASES) == NAMES
    assert len(vectors()) == 40
    assert expected("two_live_pairs", 7, 3) is not None
    assert expected("two_live_pairs", 7, -1) is None
    assert expected("exclusive_branches", 7, 3) == 7 % 3
    assert expected("exclusive_branches", 8, 3) == 8 // 3
    assert expected("long_lived_results", 7, 3) is not None
    assert traps("signed_32", 1 << 31, -1)
    assert traps("signed_64", MIN, -1)
    assert not traps("long_lived_results", MIN, -1)
    print("independent novel-context arithmetic and negative-control self-tests passed")

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path)
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        self_test()
    elif args.out:
        print(json.dumps({"status": audit(args.out)["status"]}, sort_keys=True))
    else:
        ap.error("--self-test or --out is required")
