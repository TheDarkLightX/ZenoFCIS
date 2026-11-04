#!/usr/bin/env python3
"""Independent re-audit of heterogeneous Wasm native AOT objects and outputs.

Recalculates every recorded ELF/text/module/decoded-byte delta from archived
binaries, checks the exact WAT and compiler identities, then independently
re-executes a smaller witness set with a separate bitvector oracle.
The full producer's finite-check attestation is not treated as a proof.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from install_isle_candidate import RULE, UPSTREAM
from independent_recheck_scaling_28 import (
    independent_elf_text_size, independently_count_function_bytes,
)
import qualify_wasmtime_frontend_28 as frontend
import qualify_wasmtime_scaling_28 as scaling
from qualify_wasmtime_mixed_contexts_28 import (
    CASES, SCHEMA, digest, render, salt, selected_indices, shape,
)


def u64(value: int) -> int:
    return value % (1 << 64)


def s64(value: int) -> int:
    bits = u64(value)
    return bits if bits < (1 << 63) else bits - (1 << 64)


def independent_reference(case: str, index: int, x: int, y: int) -> int:
    if not (-(1 << 63) <= x <= (1 << 63) - 1
            and -(1 << 63) <= y <= (1 << 63) - 1):
        raise ValueError("independent oracle input outside signed-i64")
    kind, k = shape(case, index), salt(index)
    total = x + y
    if total < -(1 << 63):
        total = -(1 << 63)
    elif total >= (1 << 63):
        total = (1 << 63) - 1
    if kind == 0:
        return total
    if kind == 1:
        return s64(u64(total) ^ k)
    if kind == 2:
        return s64((u64(total) ^ (u64(x) // 128)) + k)
    if kind == 3:
        return s64(u64(total) ^ (k if y < 0 else (k ^ 255)))
    if kind == 4:
        return s64((u64(x) ^ u64(y)) + k)
    if kind == 5:
        return s64(u64((3 * x + y)) ^ k)
    if kind == 6:
        return s64((x if x < 0 else y) + k)
    if kind == 7:
        z = u64(x ^ y)
        return s64(u64((z << 13) | (z >> (64 - 13))) ^ k)
    raise AssertionError("unreachable case")


def validate(receipt: Path, compilers: dict[str, Path]) -> dict:
    source = receipt.read_bytes()
    report = json.loads(source)
    if (report.get("schema") != SCHEMA
            or report.get("status") != "real-aot-heterogeneous-microkernels-finite-checked"
            or report.get("upstream_commit") != UPSTREAM
            or report.get("source", {}).get("upstream_commit") != UPSTREAM
            or report.get("rule_sha256") != digest(RULE.encode())
            or report.get("original_fixture_sha256") != scaling.SOURCE_SHA256
            or report.get("target") != scaling.TARGET
            or report.get("producer_script_sha256") != digest(
                (Path(__file__).parent / "qualify_wasmtime_mixed_contexts_28.py").read_bytes())
            or len(report.get("cases", [])) != len(CASES)
            or set(compilers) != {"baseline", "patched"}):
        raise ValueError("wrong experiment, source pin or missing case")
    source_compilers = report.get("source", {}).get("compiler_binaries", {})
    if set(source_compilers) != {"baseline", "patched"}:
        raise ValueError("missing original compiler identity")
    for label, path in compilers.items():
        pinned = source_compilers[label]
        if (not path.is_file() or path.stat().st_size != pinned.get("bytes")
                or digest(path.read_bytes()) != pinned.get("sha256")
                or frontend.run([str(path), "--version"]).stdout.strip() != pinned.get("version")):
            raise ValueError(f"compiler identity mismatch: {label}")

    root = receipt.parent
    audit = []
    for position, (case, (count, controls_only)) in enumerate(CASES.items()):
        row = report["cases"][position]
        if (row.get("case") != case or row.get("functions") != count
                or row.get("controls_only") is not controls_only
                or row.get("function_kinds") != [shape(case, i) for i in range(count)]
                or set(row.get("results", {})) != {"baseline", "patched"}):
            raise ValueError("omitted, reordered or mistyped module")
        directory = root / case
        exact_wat = render(case).encode()
        got_wat = (directory / "program.wat").read_bytes()
        if exact_wat != got_wat or digest(got_wat) != row.get("wat_sha256"):
            raise ValueError("WAT source does not match the pinned generated program")
        verified = {}
        independent_checks = {}
        for label in ("baseline", "patched"):
            entry = row["results"][label]
            folder = directory / label
            obj = folder / "functions.cwasm"
            binary = obj.read_bytes()
            if (not binary or digest(binary) != entry.get("aot_sha256")
                    or len(binary) != entry.get("aot_bytes")
                    or entry.get("function_headers_checked") != count):
                raise ValueError("missing or substituted native AOT object")
            disassembly = (folder / "disassembly.txt").read_bytes()
            if digest(disassembly) != entry.get("disassembly_sha256"):
                raise ValueError("disassembly hash mismatch")
            actual_text = independent_elf_text_size(obj)
            if actual_text != entry.get("aot_elf_text_bytes"):
                raise ValueError("ELF .text size not independently reproducible")
            costs = independently_count_function_bytes(
                disassembly.decode("utf-8"), count)
            if costs != entry.get("sum_function_costs"):
                raise ValueError("function-byte totals differ from archived code")
            check = entry.get("finite_check", {})
            if (check.get("passed") is not True
                    or check.get("selected_exports") != selected_indices(case)
                    or check.get("test_vector_count") != len(frontend.VECTORS)
                    or check.get("pairs") != len(selected_indices(case)) * len(frontend.VECTORS)
                    or check.get("scope") != "finite-Wasm-execution-not-exhaustive-proof"):
                raise ValueError("missing producer finite-test coverage")
            verified[label] = {"aot_bytes": len(binary),
                               "elf_text_bytes": actual_text,
                               "function_costs": costs,
                               "aot_sha256": digest(binary)}
            n = 0
            # A second, distinct oracle and actual binary invocations.
            witnesses = ((0, 0), (-(1 << 63), -1), ((1 << 63) - 1, 1))
            for index in selected_indices(case):
                for x, y in witnesses:
                    actual = frontend.invoke(compilers[label], obj,
                                             f"sat{index}", x, y)
                    if actual != independent_reference(case, index, x, y):
                        raise ValueError(f"independent Wasm mismatch in {case}/{label} index {index}")
                    n += 1
            independent_checks[label] = {
                "passed": True, "pairs": n,
                "coverage": selected_indices(case),
                "classification": "additional-finite-execution-not-proof",
            }
        b, p = verified["baseline"], verified["patched"]
        diffs = {
            "function_region_bytes":
                b["function_costs"]["symbol_region_bytes"]
                - p["function_costs"]["symbol_region_bytes"],
            "executable_prefix_bytes":
                b["function_costs"]["bytes_through_first_return"]
                - p["function_costs"]["bytes_through_first_return"],
            "actual_elf_text_bytes": b["elf_text_bytes"] - p["elf_text_bytes"],
            "whole_aot_bytes": b["aot_bytes"] - p["aot_bytes"],
        }
        if diffs != row.get("baseline_minus_patched"):
            raise ValueError("fabricated or nonreproducible AOT byte delta")
        audit.append({"case": case, "functions": count,
                      "control_only": controls_only,
                      "measured_deltas": diffs, "verified": verified,
                      "independent_finite_checks": independent_checks})
    return {
        "schema": "zeno-fcis/tier1-wasmtime-heterogeneous-independent/1",
        "status": "native-AOT-remeasured-and-independent-finite-reexecution-passed",
        "producer_receipt_sha256": digest(source), "upstream_commit": UPSTREAM,
        "cases": audit,
        "limitations": [
            "the generator is shared for exact-WAT replay; AOT byte calculations are separately parsed",
            "two finite Wasm-output oracles are not an exhaustive semantic theorem",
            "this is a constructed heterogeneous kernel suite, not a production application",
            "the existing VeriISLE proof relies on a provisional scalar select specification",
            "real AOT footprint does not establish runtime speed or power",
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-receipt", type=Path, required=True)
    parser.add_argument("--baseline-bin", type=Path, required=True)
    parser.add_argument("--patched-bin", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        parser.error("refusing to overwrite existing independent receipt")
    result = validate(args.source_receipt.resolve(), {
        "baseline": args.baseline_bin.resolve(),
        "patched": args.patched_bin.resolve()})
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": result["status"],
                      "costs": [{r["case"]: r["measured_deltas"]}
                                for r in result["cases"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
