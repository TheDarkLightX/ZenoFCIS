#!/usr/bin/env python3
"""Controlled Wasmtime x64 alignment-boundary probe.

Uses SAME pinned WAT for baseline and patched compiler in each variant.
Two semantic contracts: signed saturating addition, and saturating addition
followed by i64 xor 1. The second deliberately adds a live computation to
see whether an ISLE code-size win crosses an AOT allocation boundary.
This is a synthetic cost-model study, NOT a speed or real-application claim.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import platform
from pathlib import Path

from install_isle_candidate import UPSTREAM, RULE
import qualify_wasmtime_frontend_28 as frontend
import qualify_wasmtime_scaling_28 as scaling

SCHEMA = "zeno-fcis/tier1-wasmtime-alignment-boundary/1"
CHECKS = (1, 16, 256, 1024)
MODES = ("plain", "xor1")
HERE = Path(__file__).resolve().parent
PINNED_WAT_SHA256 = "dec49849bab1062369523d26fc30f9871aeca1c2cb2ea0670f590dcd8f0cf144"
TAGGED_RESULT = "signed_i64_saturating_add_then_bitwise_xor_one"
REFUSAL = "finite-tests-not-a-total-Wasm-proof"

def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()

def render(count: int, mode: str) -> str:
    if count not in CHECKS or mode not in MODES:
        raise ValueError("not an admitted function count or semantic mode")
    data = scaling.SOURCE_WAT.read_bytes()
    if sha(data) != PINNED_WAT_SHA256:
        raise ValueError("original source fixture no longer pinned")
    src = data.decode("utf-8")
    first = '  (func $sat (export "sat")'
    next_one = '  (func (export "workload")'
    if (src.count(first) != 1 or src.count(next_one) != 1
            or src.count("  (func ") != 2
            or not src.startswith("(module\n") or not src.endswith(")\n")):
        raise ValueError("unexpected source WAT layout")
    lo, hi = src.index(first), src.index(next_one)
    if not 0 <= lo < hi:
        raise ValueError("reversed WAT source function markers")
    original = src[lo:hi]
    if not original.endswith("    select\n  )\n"):
        raise ValueError("source signed-saturation final select changed")
    if mode == "xor1":
        original = original[:-len("  )\n")] + "    i64.const 1\n    i64.xor\n  )\n"
    copies = []
    for i in range(count):
        copies.append(original.replace('(func $sat (export "sat")',
            f'(func $sat{i} (export "sat{i}")', 1))
    result = "(module\n" + "".join(copies) + ")\n"
    if result.count('(export "sat') != count:
        raise ValueError("wrong number of distinct exports")
    return result

def reference(x: int, y: int, mode: str) -> int:
    if mode not in MODES:
        raise ValueError("unknown semantic mode")
    if not ((-(1 << 63)) <= x < (1 << 63) and
            (-(1 << 63)) <= y < (1 << 63)):
        raise ValueError("outside signed i64 domain")
    z = frontend.signed_sat(x, y)
    return z if mode == "plain" else z ^ 1

def finite_oracle(binary: Path, object_file: Path, count: int, mode: str) -> dict:
    selected = sorted({0, count // 2, count - 1})
    tested = 0
    for index in selected:
        for x, y in frontend.VECTORS:
            expected = reference(x, y, mode)
            actual = frontend.invoke(binary, object_file, f"sat{index}", x, y)
            if actual != expected:
                raise ValueError(f"failed {mode} witness: index={index}, x={x}, y={y}")
            tested += 1
    return {"passed": True, "checked_export_indices": selected,
            "checked_input_pairs": tested,
            "classification": REFUSAL}

def qualify(upstream: Path, bins: dict, dest: Path) -> dict:
    if dest.exists():
        raise ValueError("refusing to overwrite an existing receipt")
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("experiment is for an x86-64 host")
    if set(bins) != {"baseline", "patched"}:
        raise ValueError("expected exactly baseline and patched compilers")
    pinned = frontend.source_pins(upstream, bins)
    dest.mkdir(parents=True)
    output = []
    for mode in MODES:
        for count in CHECKS:
            folder = dest / mode / f"copies-{count:04d}"
            folder.mkdir(parents=True)
            wat = folder / "source.wat"
            wat.write_text(render(count, mode))
            measured = {}
            for name in ("baseline", "patched"):
                path = folder / name
                record = scaling.compile_case(bins[name], path, wat, count)
                record["finite_oracle"] = finite_oracle(
                    bins[name], path / "functions.cwasm", count, mode)
                measured[name] = record
            b, p = measured["baseline"], measured["patched"]
            differences = {
                "summed_function_region_bytes":
                    b["sum_function_costs"]["symbol_region_bytes"] -
                    p["sum_function_costs"]["symbol_region_bytes"],
                "summed_function_bytes_through_ret":
                    b["sum_function_costs"]["bytes_through_first_return"] -
                    p["sum_function_costs"]["bytes_through_first_return"],
                "elf_text_section_bytes":
                    b["aot_elf_text_bytes"] - p["aot_elf_text_bytes"],
                "entire_serialized_cwasm_bytes":
                    b["aot_bytes"] - p["aot_bytes"],
            }
            output.append({
                "mode": mode, "copies": count, "wat_sha256": sha(wat.read_bytes()),
                "results": measured, "baseline_minus_patched": differences,
                "slot_heuristic_only": {
                    "preferred_alignment_bytes": 32,
                    "warning": "not an allocation proof; check actual ELF section below",
                },
            })
    return {
        "schema": SCHEMA,
        "status": "real-wasmtime-aot-synthetic-alignment-probe-finite-checks",
        "source": pinned,
        "original_wat_sha256": PINNED_WAT_SHA256,
        "patch_rule_sha256": sha(RULE.encode()),
        "wasmtime_upstream_commit": UPSTREAM,
        "target": scaling.TARGET,
        "cases": output,
        "semantics": {
            "plain": "signed_i64_saturating_add_exact_no_poison_on-Wasm",
            "xor1": TAGGED_RESULT + "_exact_no_poison_on-Wasm",
            "signedness": "signed arithmetic saturation; modular bitwise XOR",
            "domain": "all signed 64-bit pairs, runtime checked on finite fixed vectors only",
            "SMT": "existing independent proof for original clamp; not a new all-input proof of entire WAT fixture",
            "undefined_and_flags": "Wasm i64 ops are total; no external flags are observable through this interface",
            "platform": "x86_64-unknown-linux, same pinned baseline/patched source otherwise",
        },
        "nonclaims": [
            "the xor1 case deliberately changes the source function's declared output contract",
            "both compilers receive identical WAT within each variant; never compare absolute sizes across variants as an optimization",
            "synthetic repeated exports do not prove prevalence in applications",
            "finite Wasm execution is not exhaustive checking or a formal equivalence proof",
            "an ELF .text or .cwasm delta may remain zero due to alignment/padding",
            "no instructions-per-cycle, throughput, timing or speed result",
            "ISLE rule proof still conditional on local provisional select specification",
            "do not submit upstream based on this single synthetic effect",
        ],
    }

def main() -> None:
    parser = argparse.ArgumentParser()
    for arg in ("upstream", "baseline-bin", "patched-bin", "out"):
        parser.add_argument("--" + arg, type=Path, required=True)
    args = parser.parse_args()
    report = qualify(args.upstream.resolve(),
                     {"baseline": args.baseline_bin.resolve(),
                      "patched": args.patched_bin.resolve()},
                     args.out.resolve())
    (args.out / "qualification.json").write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": report["status"],
                      "cases": [{"mode": x["mode"], "copies": x["copies"],
                                 "delta": x["baseline_minus_patched"]}
                                for x in report["cases"]]}, sort_keys=True))

if __name__ == "__main__":
    main()
