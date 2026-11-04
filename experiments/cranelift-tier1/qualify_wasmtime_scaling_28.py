#!/usr/bin/env python3
"""Pinned Wasmtime multi-function code-size scaling experiment.

A controlled synthetic multiplicity experiment: it is NOT a real workload,
runtime improvement, or a new machine-code sequence discovery. The only
compiler modification is the one already pinned mid-end ISLE rule.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import platform
import re
from pathlib import Path
import qualify_wasmtime_frontend_28 as prior
from install_isle_candidate import UPSTREAM, RULE
from qualify_wasmtime_function_bytes_28 import BYTE_LINE

HERE = Path(__file__).resolve().parent
SOURCE_WAT = HERE / "satadd_wasmtime_28.wat"
SOURCE_SHA256 = "dec49849bab1062369523d26fc30f9871aeca1c2cb2ea0670f590dcd8f0cf144"
COUNTS = (1, 16, 64, 256, 1024)
TARGET = "x86_64-unknown-linux"
SCHEMA = "zeno-fcis/tier1-wasmtime-code-size-scaling/1"
HEADER_RE = re.compile(r"(?m)^wasm\[0\]::function\[(\d+)\](?:::|:)")
VECTORS = prior.VECTORS


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def render_wat(count: int) -> str:
    if count not in COUNTS:
        raise ValueError("not a declared workload size")
    data = SOURCE_WAT.read_bytes()
    if digest(data) != SOURCE_SHA256:
        raise ValueError("input Wasm source fixture changed")
    source = data.decode("utf-8")
    start_marker = '  (func $sat (export "sat")'
    stop_marker = '  (func (export "workload")'
    if (source.count(start_marker) != 1 or source.count(stop_marker) != 1
            or source.count("  (func ") != 2 or not source.startswith("(module\n")
            or not source.rstrip().endswith(")")):
        raise ValueError("unrecognized source Wasm function layout")
    start, stop = source.index(start_marker), source.index(stop_marker)
    if stop <= start:
        raise ValueError("source function boundaries invalid")
    function = source[start:stop]
    # One independent exported function per copy, preserving the exact
    # original signed-overflow expression with only its export/local name
    # changed. The parent module workload function is deliberately omitted.
    copies = []
    for i in range(count):
        copies.append(function.replace('(func $sat (export "sat")',
                                       f'(func $sat{i} (export "sat{i}")', 1))
    return "(module\n" + "".join(copies) + ")\n"


def function_headers(disassembly: str, count: int) -> None:
    headers = [int(x) for x in HEADER_RE.findall(disassembly)]
    if headers != list(range(count)):
        raise ValueError(f"expected {count} distinct Wasm functions, got {headers[:5]}... (n={len(headers)})")
    for i in (0, count // 2, count - 1):
        exact = f"wasm[0]::function[{i}]::sat{i}:"
        if disassembly.count(exact) != 1:
            raise ValueError("required exported function missing or duplicated: " + exact)


def all_function_byte_costs(disassembly: str, count: int) -> dict:
    """Aggregate observed disassembly bytes across each distinct exported function.

    "Through first return" is not a dynamic CPU-time or throughput metric.
    Padding after return belongs only to the disassembler function region.
    """
    lines = disassembly.splitlines()
    headers = [(pos, line) for pos, line in enumerate(lines)
               if line.startswith("wasm[0]::function[")]
    if len(headers) != count:
        raise ValueError("unexpected function disassembly header cardinality")
    totals = {"symbol_region_bytes": 0, "bytes_through_first_return": 0,
              "post_return_region_bytes": 0}
    first = []
    for i, (at, title) in enumerate(headers):
        if title != f"wasm[0]::function[{i}]::sat{i}:":
            raise ValueError("unexpected or missing exact function name")
        end = headers[i + 1][0] if i + 1 < len(headers) else len(lines)
        byte_count = 0
        ret_end = None
        for line in lines[at + 1:end]:
            if not line.strip():
                continue
            m = BYTE_LINE.fullmatch(line)
            if not m:
                raise ValueError("unrecognized disassembly byte line")
            byte_count += len(m.group(1).split(" "))
            mnemonic = (m.group(2) or "").strip()
            if re.match(r"^retq(?:\s|$)", mnemonic):
                if ret_end is not None:
                    raise ValueError("more than one function return")
                ret_end = byte_count
        if ret_end is None or not byte_count or byte_count < ret_end:
            raise ValueError("unqualified function return or empty emitted code")
        totals["symbol_region_bytes"] += byte_count
        totals["bytes_through_first_return"] += ret_end
        totals["post_return_region_bytes"] += byte_count - ret_end
        if i in (0, count // 2, count - 1):
            first.append({"function": i, "through_ret": ret_end,
                          "region": byte_count})
    return dict(totals, qualified_function_count=count,
                inspected_representative_functions=first)


def compile_case(bin_path: Path, folder: Path, wat: Path, count: int) -> dict:
    folder.mkdir(parents=True)
    module = folder / "functions.cwasm"
    run_args = [str(bin_path), "compile", "--target", TARGET,
                "-o", str(module), str(wat)]
    result = prior.run(run_args, timeout=300)
    (folder / "compile.stdout").write_text(result.stdout)
    (folder / "compile.stderr").write_text(result.stderr)
    if not module.is_file() or not module.stat().st_size:
        raise ValueError("wasm AOT module was not produced")
    dis = prior.run([str(bin_path), "objdump", "--funcs", "wasm",
                     "--bytes", str(module)], timeout=120).stdout
    function_headers(dis, count)
    cost = all_function_byte_costs(dis, count)
    (folder / "disassembly.txt").write_text(dis)
    return {"aot_sha256": digest(module.read_bytes()),
            "aot_bytes": module.stat().st_size,
            "aot_elf_text_bytes": prior.text_size(module),
            "disassembly_sha256": digest(dis.encode()),
            "function_headers_checked": count,
            "sum_function_costs": cost,
            "tool_command": run_args}


def check_case(bin_path: Path, module: Path, count: int) -> dict:
    checks = 0
    for index in sorted({0, count // 2, count - 1}):
        for x, y in VECTORS:
            got = prior.invoke(bin_path, module, f"sat{index}", x, y)
            if got != prior.signed_sat(x, y):
                raise ValueError(f"signed saturation counterexample for function {index} at ({x},{y})")
            checks += 1
    return {"finite_oracle_passed": True, "checked_pairs": checks,
            "checked_exports": sorted({0, count // 2, count - 1}),
            "classification": "finite-execution-only-not-exhaustive-proof"}


def qualify(upstream: Path, bins: dict, output: Path) -> dict:
    if output.exists():
        raise ValueError("cannot overwrite scaling evidence")
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("portable x86-64 test requires x86-64 host")
    if set(bins) != {"baseline", "patched"}:
        raise ValueError("missing or surplus compiler variants")
    source = prior.source_pins(upstream, bins)
    output.mkdir(parents=True)
    rows = []
    for count in COUNTS:
        folder = output / f"functions-{count:03d}"
        folder.mkdir()
        wat = folder / "fixture.wat"
        wat.write_text(render_wat(count))
        copies = {}
        for variant in ("baseline", "patched"):
            rec = compile_case(bins[variant], folder / variant, wat, count)
            rec["finite_oracle"] = check_case(bins[variant],
                                              folder / variant / "functions.cwasm", count)
            copies[variant] = rec
        base, candidate = copies["baseline"], copies["patched"]
        rows.append({
            "copies": count,
            "source_wat_sha256": digest(wat.read_bytes()),
            "results": copies,
            "baseline_minus_candidate": {
                "aot_elf_text_bytes": base["aot_elf_text_bytes"] - candidate["aot_elf_text_bytes"],
                "whole_cwasm_bytes": base["aot_bytes"] - candidate["aot_bytes"],
                "sum_function_symbol_region_bytes": base["sum_function_costs"]["symbol_region_bytes"] - candidate["sum_function_costs"]["symbol_region_bytes"],
                "sum_function_bytes_through_first_return": base["sum_function_costs"]["bytes_through_first_return"] - candidate["sum_function_costs"]["bytes_through_first_return"],
            },
        })
    return {"schema": SCHEMA,
            "status": "bounded-synthetic-multiplicity-compiler-aot-finite-checked",
            "source": source,
            "rule_sha256": digest(RULE.encode()),
            "target": TARGET, "copies": list(COUNTS), "cases": rows,
            "nonclaims": [
                "synthetic repeated exported functions, not a representative application",
                "code-size measurement is not an execution-speed measurement",
                "AOT ELF .text and entire serialized .cwasm length are different metrics",
                "function exports and trampolines contribute to whole-module size",
                "finite Wasm executions do not prove full source semantics",
                "named VeriISLE rule verification still depends on a provisional select specification",
                "observed scaling, if any, is not guaranteed by other compiler revisions",
            ]}


def main() -> None:
    p = argparse.ArgumentParser()
    for name in ("upstream", "baseline-bin", "patched-bin", "out"):
        p.add_argument("--" + name, type=Path, required=True)
    a = p.parse_args()
    result = qualify(a.upstream.resolve(),
                     {"baseline": a.baseline_bin.resolve(),
                      "patched": a.patched_bin.resolve()}, a.out.resolve())
    (a.out / "qualification.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": result["status"], "cost_scaling": [
        {"count": c["copies"], "deltas": c["baseline_minus_candidate"]}
        for c in result["cases"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
