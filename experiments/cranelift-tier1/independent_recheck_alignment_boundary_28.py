#!/usr/bin/env python3
"""Independent AOT recheck for exact alignment boundary control experiment.

Reads complete compiled ELF .cwasm files, archived disassembly, WAT and the
original producer receipt. Recomputes byte deltas, refuses provenance drift.
No trust in reported delta, no claim that any delta must be positive.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path

from install_isle_candidate import UPSTREAM, RULE
from qualify_wasmtime_alignment_boundary_28 import (
    SCHEMA, CHECKS, MODES, PINNED_WAT_SHA256, REFUSAL, render,
)
from independent_recheck_scaling_28 import (
    independent_elf_text_size, independently_count_function_bytes,
)

INDEPENDENT_SCHEMA = "zeno-fcis/tier1-wasmtime-alignment-independent/1"

def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()

def recheck(report_path: Path) -> dict:
    source_bytes = report_path.read_bytes()
    report = json.loads(source_bytes)
    if (report.get("schema") != SCHEMA or
        report.get("status") != "real-wasmtime-aot-synthetic-alignment-probe-finite-checks" or
        report.get("target") != "x86_64-unknown-linux" or
        report.get("wasmtime_upstream_commit") != UPSTREAM or
        report.get("source", {}).get("upstream_commit") != UPSTREAM or
        report.get("original_wat_sha256") != PINNED_WAT_SHA256 or
        report.get("patch_rule_sha256") != sha(RULE.encode()) or
        len(report.get("cases", [])) != len(CHECKS) * len(MODES) or
        set(report.get("source", {}).get("compiler_binaries", {})) != {"baseline", "patched"}):
        raise ValueError("missing or different exact source/experiment identity")
    root = report_path.parent
    rows = []
    for i, (mode, count) in enumerate((m, n) for m in MODES for n in CHECKS):
        row = report["cases"][i]
        if row.get("mode") != mode or row.get("copies") != count:
            raise ValueError("omitted, duplicated, or shuffled experiment")
        folder = root / mode / f"copies-{count:04d}"
        actual_wat = (folder / "source.wat").read_bytes()
        if (actual_wat != render(count, mode).encode() or
            sha(actual_wat) != row.get("wat_sha256")):
            raise ValueError("WAT differs from independently reconstructed exact source")
        if set(row.get("results", {})) != {"baseline", "patched"}:
            raise ValueError("baseline or patched result missing")
        results = {}
        for name in ("baseline", "patched"):
            record = row["results"][name]
            path = folder / name
            obj = path / "functions.cwasm"
            data = obj.read_bytes()
            if (not data or
                sha(data) != record.get("aot_sha256") or
                len(data) != record.get("aot_bytes")):
                raise ValueError("archived AOT binary corrupted/missing")
            dis = (path / "disassembly.txt").read_bytes()
            if sha(dis) != record.get("disassembly_sha256"):
                raise ValueError("archived disassembly hash mismatch")
            actual_text = independent_elf_text_size(obj)
            if actual_text != record.get("aot_elf_text_bytes"):
                raise ValueError("independent ELF .text measurement disagreement")
            insns = independently_count_function_bytes(dis.decode("utf-8"), count)
            if insns != record.get("sum_function_costs"):
                raise ValueError("independent function disassembly count disagreement")
            coverage = record.get("finite_oracle", {})
            selected = sorted({0, count // 2, count - 1})
            if (coverage.get("passed") is not True or
                coverage.get("classification") != REFUSAL or
                coverage.get("checked_export_indices") != selected or
                coverage.get("checked_input_pairs") != 12 * len(selected)):
                raise ValueError("finite result checks are absent or incomplete")
            results[name] = {"elf_text_bytes": actual_text,
                             "serialized_cwasm_bytes": len(data),
                             "function_costs": insns, "sha256": sha(data)}
        base = results["baseline"]
        patched = results["patched"]
        delta = {
            "summed_function_region_bytes":
                base["function_costs"]["symbol_region_bytes"] -
                patched["function_costs"]["symbol_region_bytes"],
            "summed_function_bytes_through_ret":
                base["function_costs"]["bytes_through_first_return"] -
                patched["function_costs"]["bytes_through_first_return"],
            "elf_text_section_bytes":
                base["elf_text_bytes"] - patched["elf_text_bytes"],
            "entire_serialized_cwasm_bytes":
                base["serialized_cwasm_bytes"] - patched["serialized_cwasm_bytes"],
        }
        if delta != row.get("baseline_minus_patched"):
            raise ValueError("reported AOT byte savings are not independently reproducible")
        rows.append({"mode": mode, "copies": count,
                     "verified_deltas": delta, "measurements": results,
                     "synthetic_only": True})
    return {
        "schema": INDEPENDENT_SCHEMA,
        "status": "independent-ELF-and-AOT-cost-recheck-finite-test-only",
        "source_receipt_sha256": sha(source_bytes),
        "upstream_commit": UPSTREAM, "cases": rows,
        "scope": "synthetic executable alignment-boundary test; no speed or real-workload claim",
        "verification": "finite Wasm runtime checks only; pre-existing conditional ISLE proof",
    }

def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--source-receipt", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    if args.out.exists():
        p.error("will not overwrite independent receipt")
    result = recheck(args.source_receipt.resolve())
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": result["status"],
                      "deltas": [{"mode": x["mode"], "copies": x["copies"],
                                  "deltas": x["verified_deltas"]}
                                 for x in result["cases"]]}, sort_keys=True))

if __name__ == "__main__":
    main()
