#!/usr/bin/env python3
"""Heterogeneous Wasm integer-kernel AOT comparison, not an application benchmark.

Use the exact same WAT per case in unmodified and one-rule-patched Wasmtime.
Half the mixed module's functions contain a signed-i64 saturation pattern;
the other half are unrelated integer controls. The controls are not named
"saturation" semantically: satN is only the disassembly parser's stable
export naming convention, inherited from the earlier experiment.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import platform
from pathlib import Path

import qualify_wasmtime_frontend_28 as frontend
import qualify_wasmtime_scaling_28 as scaling
from install_isle_candidate import RULE, UPSTREAM

SCHEMA = "zeno-fcis/tier1-wasmtime-heterogeneous-kernels/1"
CASES = {"mixed32": (32, False), "mixed128": (128, False),
         "controls32": (32, True)}
MASK = (1 << 64) - 1
SIGN = 1 << 63
HERE = Path(__file__).resolve().parent


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def to_signed(value: int) -> int:
    return ((value & MASK) ^ SIGN) - SIGN


def salt(index: int) -> int:
    return 37 * (index + 1) + 1


def shape(case: str, index: int) -> int:
    if case not in CASES:
        raise ValueError("unknown case")
    count, controls_only = CASES[case]
    if not 0 <= index < count:
        raise ValueError("function index outside bounded case")
    return (4 + index % 4) if controls_only else index % 8


def selected_indices(case: str) -> list[int]:
    count, controls_only = CASES[case]
    width = 4 if controls_only else 8
    return sorted(set(range(width)) | set(range(count - width, count)))


def sat_body() -> str:
    """Recover the audited full original signed-overflow WAT expression."""
    data = scaling.SOURCE_WAT.read_bytes()
    if digest(data) != scaling.SOURCE_SHA256:
        raise ValueError("the original signed saturation WAT fixture drifted")
    original = data.decode("utf-8")
    start = '  (func $sat (export "sat")'
    end = '  (func (export "workload")'
    if (original.count(start) != 1 or original.count(end) != 1
            or original.count("  (func ") != 2):
        raise ValueError("unrecognized source WAT layout")
    body = original[original.index(start):original.index(end)]
    if not body.endswith("    select\n  )\n"):
        raise ValueError("original signed saturation expression changed")
    return body


def render_function(case: str, index: int, base: str) -> str:
    kind, k = shape(case, index), salt(index)
    head = f'  (func $sat{index} (export "sat{index}")'
    if kind < 4:
        copied = base.replace('  (func $sat (export "sat")', head, 1)
        if kind == 0:
            return copied
        suffix = {
            1: f"    i64.const {k}\n    i64.xor\n",
            2: ("    local.get $x\n    i64.const 7\n"
                "    i64.shr_u\n    i64.xor\n"
                f"    i64.const {k}\n    i64.add\n"),
            3: ("    local.get $y\n    i64.const 0\n    i64.lt_s\n"
                "    if (result i64)\n"
                f"      i64.const {k}\n"
                "    else\n"
                f"      i64.const {k ^ 255}\n"
                "    end\n    i64.xor\n"),
        }[kind]
        return copied[:-len("  )\n")] + suffix + "  )\n"

    body = {
        4: ("    local.get $x\n    local.get $y\n    i64.xor\n"
            f"    i64.const {k}\n    i64.add\n"),
        5: ("    local.get $x\n    i64.const 3\n    i64.mul\n"
            "    local.get $y\n    i64.add\n"
            f"    i64.const {k}\n    i64.xor\n"),
        6: ("    local.get $x\n    local.get $y\n"
            "    local.get $x\n    i64.const 0\n"
            "    i64.lt_s\n    select\n"
            f"    i64.const {k}\n    i64.add\n"),
        7: ("    local.get $x\n    local.get $y\n    i64.xor\n"
            "    i64.const 13\n    i64.rotl\n"
            f"    i64.const {k}\n    i64.xor\n"),
    }[kind]
    return (head + " (param $x i64) (param $y i64) (result i64)\n"
            + body + "  )\n")


def render(case: str) -> str:
    if case not in CASES:
        raise ValueError("unknown case")
    count = CASES[case][0]
    base = sat_body()
    wat = "(module\n" + "".join(
        render_function(case, i, base) for i in range(count)) + ")\n"
    if (wat.count('(export "sat') != count
            or wat.count("  (func ") != count):
        raise ValueError("generated export cardinality is incorrect")
    return wat


def reference(case: str, index: int, x: int, y: int) -> int:
    if not (frontend.MIN <= x <= frontend.MAX
            and frontend.MIN <= y <= frontend.MAX):
        raise ValueError("outside declared signed-i64 input domain")
    kind, k = shape(case, index), salt(index)
    sat = frontend.signed_sat(x, y)
    if kind == 0:
        return sat
    if kind == 1:
        return to_signed((sat & MASK) ^ k)
    if kind == 2:
        return to_signed(((sat & MASK) ^ ((x & MASK) >> 7)) + k)
    if kind == 3:
        return to_signed((sat & MASK) ^ (k if y < 0 else k ^ 255))
    if kind == 4:
        return to_signed(((x & MASK) ^ (y & MASK)) + k)
    if kind == 5:
        return to_signed(((3 * x + y) & MASK) ^ k)
    if kind == 6:
        return to_signed((x if x < 0 else y) + k)
    if kind == 7:
        bits = (x ^ y) & MASK
        return to_signed((((bits << 13) | (bits >> 51)) & MASK) ^ k)
    raise AssertionError("unhandled kind")


def finite_checks(bin_path: Path, aot: Path, case: str) -> dict:
    indices = selected_indices(case)
    checked = 0
    for index in indices:
        for x, y in frontend.VECTORS:
            expected = reference(case, index, x, y)
            got = frontend.invoke(bin_path, aot, f"sat{index}", x, y)
            if got != expected:
                raise ValueError(f"incorrect Wasm output: {case} export {index}, ({x},{y})")
            checked += 1
    return {"passed": True, "selected_exports": indices,
            "pairs": checked, "test_vector_count": len(frontend.VECTORS),
            "scope": "finite-Wasm-execution-not-exhaustive-proof"}


def qualify(upstream: Path, compilers: dict, out: Path) -> dict:
    if out.exists():
        raise ValueError("refusing to overwrite an existing experiment")
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("only source-pinned x86-64 host comparison supported")
    if set(compilers) != {"baseline", "patched"}:
        raise ValueError("exactly two compiler variants required")
    pins = frontend.source_pins(upstream, compilers)
    out.mkdir(parents=True)
    rows = []
    for case, (count, controls_only) in CASES.items():
        directory = out / case
        directory.mkdir()
        wat = directory / "program.wat"
        wat.write_text(render(case))
        measured = {}
        for label in ("baseline", "patched"):
            folder = directory / label
            entry = scaling.compile_case(compilers[label], folder, wat, count)
            entry["finite_check"] = finite_checks(
                compilers[label], folder / "functions.cwasm", case)
            measured[label] = entry
        b, p = measured["baseline"], measured["patched"]
        bc, pc = b["sum_function_costs"], p["sum_function_costs"]
        rows.append({
            "case": case, "functions": count, "controls_only": controls_only,
            "function_kinds": [shape(case, i) for i in range(count)],
            "wat_sha256": digest(wat.read_bytes()), "results": measured,
            "baseline_minus_patched": {
                "function_region_bytes":
                    bc["symbol_region_bytes"] - pc["symbol_region_bytes"],
                "executable_prefix_bytes":
                    bc["bytes_through_first_return"] - pc["bytes_through_first_return"],
                "actual_elf_text_bytes":
                    b["aot_elf_text_bytes"] - p["aot_elf_text_bytes"],
                "whole_aot_bytes": b["aot_bytes"] - p["aot_bytes"],
            }
        })
    return {
        "schema": SCHEMA, "status": "real-aot-heterogeneous-microkernels-finite-checked",
        "source": pins, "upstream_commit": UPSTREAM,
        "original_fixture_sha256": scaling.SOURCE_SHA256,
        "rule_sha256": digest(RULE.encode()), "target": scaling.TARGET,
        "cases": rows,
        "contract": {
            "functions": "signed i64 x signed i64 -> signed i64; each index has a declared operator sequence",
            "saturation": "mathematical clamp to [-2^63,2^63-1] before consumer operations",
            "nonsaturating": "Wasm total i64 bitvector arithmetic with modular wraparound",
            "branch_condition": "i64 signed comparison to zero, i32 boolean select/if",
            "undefined_or_poison": "none in the declared Wasm integer operators; no observable x86 condition flags",
            "finite_coverage": "selected functions and exact twelve edge/input pairs only",
            "proof_boundary": "pre-existing named ISLE check depends on provisional scalar select specification",
        },
        "nonclaims": [
            "heterogeneous constructed microkernel suite, not production Wasm workload",
            "identical WAT for baseline and candidate within each case only",
            "controls-only case is a negative control, not an assumed zero delta",
            "reported AOT cost is actual whole file and ELF section size, not extrapolated instruction bytes",
            "no speed, power, throughput or target-wide win established",
            "not an exhaustive machine, ABI, flag or poison proof",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    for flag in ("upstream", "baseline-bin", "patched-bin", "out"):
        p.add_argument("--" + flag, required=True, type=Path)
    args = p.parse_args()
    results = qualify(args.upstream.resolve(), {
        "baseline": args.baseline_bin.resolve(),
        "patched": args.patched_bin.resolve()}, args.out.resolve())
    (args.out / "qualification.json").write_text(
        json.dumps(results, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": results["status"],
                      "cost_deltas": [{r["case"]: r["baseline_minus_patched"]}
                                      for r in results["cases"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
