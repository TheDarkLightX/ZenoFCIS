#!/usr/bin/env python3
"""Independent fail-closed Wasmtime guest trap-site audit for two live udiv/urem pairs.

Verifies byte-identical WAT, independently calculated finite output, both guest
trap codes, distinct FIRST/SECOND original-Wasm offsets, and A/B offset equality.
This does not verify the internal machine-PC trap table or arbitrary compiler IR.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path

from qualify_udivrem_contexts_28 import CASES

HERE = Path(__file__).resolve().parent
PIN = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
MASK = (1 << 64) - 1
VALID = ((0, 1), (1, 1), (7, 3), (-1, 1), (-(1 << 63), 1),
         ((1 << 63) - 1, (1 << 63) - 1), (-1, -2), (17, 5))
INVALID = (("first", 0, 0), ("first", 1, 0), ("first", -1, 0),
           ("second", 0, -1), ("second", 1, -1),
           ("second", -1, -1), ("second", -(1 << 63), -1))
VALUE = re.compile(r"VALUE x=(-?\d+) y=(-?\d+) result=(-?\d+)")
TRAP = re.compile(
    r"TRAP site=(first|second) x=(-?\d+) y=(-?\d+) "
    r"code=IntegerDivisionByZero func_index=0 func_offset=(\d+) module_offset=(\d+)"
)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def s64(v: int) -> int:
    v &= MASK
    return v - (1 << 64) if v & (1 << 63) else v


def expected(x: int, y: int) -> int:
    a, b = x & MASK, y & MASK
    if b in (0, MASK):
        raise ValueError("test vector outside two-pair finite domain")
    q, r = divmod(a, b)
    q2, r2 = divmod((a + 1) & MASK, (b + 1) & MASK)
    return s64((q ^ ((r << 17 | r >> 47) & MASK)) ^
               (q2 ^ ((r2 << 17 | r2 >> 47) & MASK)))


def decode(raw: str) -> dict:
    values, traps = {}, {}
    for row in raw.strip().splitlines():
        if m := VALUE.fullmatch(row):
            x, y, got = map(int, m.groups())
            if (x, y) in values or (x, y) not in VALID or got != expected(x, y):
                raise ValueError("unexpected, duplicate, or wrong guest finite value")
            values[(x, y)] = got
        elif m := TRAP.fullmatch(row):
            site, sx, sy, sf, sm = m.groups()
            key = (site, int(sx), int(sy))
            fo, mo = int(sf), int(sm)
            if key in traps or key not in INVALID or mo <= 0 or mo < fo:
                raise ValueError("invalid guest trap row/source position")
            traps[key] = (fo, mo)
        else:
            raise ValueError("unqualified Wasmtime API output: " + row[:140])
    if set(values) != set(VALID) or set(traps) != set(INVALID):
        raise ValueError("missing/extra two-site test vectors")
    first = {pair for (site, _, _), pair in traps.items() if site == "first"}
    second = {pair for (site, _, _), pair in traps.items() if site == "second"}
    if len(first) != 1 or len(second) != 1 or first == second:
        raise ValueError("trap source offsets collapsed or input-dependent")
    return {
        "values": {f"{x},{y}": v for (x, y), v in values.items()},
        "traps": {f"{site},{x},{y}": {"func_offset": fo, "module_offset": mo}
                  for (site, x, y), (fo, mo) in traps.items()},
        "site_offsets": {
            "first": {"func_offset": next(iter(first))[0], "module_offset": next(iter(first))[1]},
            "second": {"func_offset": next(iter(second))[0], "module_offset": next(iter(second))[1]},
        },
    }


def audit(root: Path) -> dict:
    result = root / "second-trap-audit.json"
    if result.exists():
        raise ValueError("refusing to overwrite existing evidence")
    if (root / "wasmtime-head.txt").read_text().strip() != PIN:
        raise ValueError("unexpected pinned Wasmtime revision")
    source = root / "two-live-pairs.wat"
    if source.read_text() != CASES["two_live_pairs"]:
        raise ValueError("wrong source WAT; not the declared second-trap context")
    context = json.loads((root / "context-controls/qualification.json").read_text())
    record = context["cases"]["two_live_pairs"]
    if context.get("schema") != "zenofcis/tier1-udivrem-contexts/1" or (
        context.get("upstream_source_pin") != PIN
        or sha(source) != record["wat_sha256"]
        or record["static_div_savings"] != 2
        or record["results"]["baseline"]["static_cost"]["static_divisions"] != 4
        or record["results"]["patched"]["static_cost"]["static_divisions"] != 2
    ):
        raise ValueError("missing evidence of same-WAT native two-pair optimization")
    checked = {}
    for mode in ("baseline", "patched"):
        binary = root / mode / "wasmtime"
        if sha(binary) != context["compiler_pins"][mode]["sha256"]:
            raise ValueError("compiler binary hash changed")
        transcript = root / (mode + "-second-trap-api.txt")
        checked[mode] = decode(transcript.read_text())
    if checked["baseline"] != checked["patched"]:
        raise ValueError("guest trap or finite arithmetic mismatch between compiler builds")
    receipt = {
        "schema": "zenofcis/tier1-second-trap-guest-offsets/1",
        "status": "passed-finite-identical-WAT-two-distinct-original-Wasm-trap-sites",
        "zenofcis_head": (root / "zenofcis-head.txt").read_text().strip(),
        "wasmtime_source_pin": PIN,
        "source_wat_sha256": sha(source),
        "probe_sha256": sha(HERE / "guest_second_trap_28.rs"),
        "auditor_sha256": sha(Path(__file__)),
        "context_receipt_sha256": sha(root / "context-controls/qualification.json"),
        "compiler_sha256": {mode: sha(root / mode / "wasmtime") for mode in checked},
        "transcript_sha256": {
            mode: sha(root / (mode + "-second-trap-api.txt")) for mode in checked
        },
        "finite_values_per_compiler": len(VALID),
        "first_traps_per_compiler": 3,
        "second_traps_per_compiler": 4,
        "guest_original_wasm_site_offsets": checked["baseline"]["site_offsets"],
        "scope": [
            "guest WasmBacktrace original-Wasm function/module offset, not native fault PC",
            "same generated WAT compiled by two different pinned compiler binaries",
            "zero original divisor and wrapped-to-zero second divisor are distinct trap sites",
            "finite Wasmtime API checks; no full compiler-pass, SSA or trap-table proof",
            "no throughput, whole-AOT reduction, or synthesized discovery claimed",
        ],
    }
    result.write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
    return receipt


def self_test() -> None:
    good = "\n".join(
        [f"VALUE x={x} y={y} result={expected(x, y)}" for x, y in VALID]
        + [f"TRAP site={site} x={x} y={y} code=IntegerDivisionByZero func_index=0 "
           f"func_offset={12 if site == 'first' else 36} "
           f"module_offset={42 if site == 'first' else 66}"
           for site, x, y in INVALID]
    ) + "\n"
    assert decode(good)["site_offsets"]["first"]["func_offset"] == 12
    mutants = (
        good.replace("func_offset=36", "func_offset=12").replace("module_offset=66", "module_offset=42"),
        good.replace("IntegerDivisionByZero", "IntegerOverflow", 1),
        good.replace("result=", "result=999", 1),
        good.replace("TRAP site=second", "TRAP site=first", 1),
        good.replace("module_offset=42", "module_offset=0", 1),
        good + "EXTRA\n",
        "\n".join(good.splitlines()[:-1]) + "\n",
        good + good.splitlines()[0] + "\n",
    )
    for mutant in mutants:
        try:
            decode(mutant)
        except ValueError:
            continue
        raise AssertionError("corrupted guest source/trap/value receipt accepted")
    print("two-site guest trap auditor: positive and eight negative controls passed")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--self-test", action="store_true")
    p.add_argument("--out", type=Path)
    args = p.parse_args()
    if args.self_test:
        self_test()
    elif args.out:
        print(json.dumps({"status": audit(args.out)["status"]}))
    else:
        p.error("pass --self-test or --out")
