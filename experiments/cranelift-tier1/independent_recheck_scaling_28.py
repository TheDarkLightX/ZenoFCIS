#!/usr/bin/env python3
"""Independent binary re-measurement of synthetic AOT multiplicity evidence.

Does not assume a winner: recomputes deltas from files on disk. The producer's
finite checks are attestations, not an exhaustive Wasm semantic proof.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path
from qualify_wasmtime_scaling_28 import COUNTS, SCHEMA, SOURCE_SHA256, TARGET, render_wat
from install_isle_candidate import UPSTREAM


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def independent_elf_text_size(p: Path) -> int:
    raw = subprocess.run(["readelf", "-SW", str(p)], capture_output=True, text=True, check=True).stdout
    tokens = [x.strip() for x in raw.splitlines() if re.search(r"\]\s+\.text\s+", x)]
    if len(tokens) != 1:
        raise ValueError("missing or duplicate .text sections")
    # readelf -SW: [index] Name Type Address Offset Size ES ...
    fields = tokens[0].split()
    idx = fields.index(".text")
    size = int(fields[idx + 4], 16)
    if size <= 0:
        raise ValueError("zero executable text")
    return size


def independently_count_function_bytes(raw: str, count: int) -> dict:
    """Reparse per-function bytes from archived objdump without producer parser."""
    lines = raw.splitlines()
    ix = [i for i, ln in enumerate(lines) if ln.startswith("wasm[0]::function[")]
    if len(ix) != count:
        raise ValueError("independent exported-symbol count mismatch")
    result = {"symbol_region_bytes": 0, "bytes_through_first_return": 0,
              "post_return_region_bytes": 0}
    representative = []
    for number, at in enumerate(ix):
        if lines[at] != f"wasm[0]::function[{number}]::sat{number}:":
            raise ValueError("independent symbol name mismatch")
        limit = ix[number + 1] if number + 1 < len(ix) else len(lines)
        total = 0
        returned = None
        for ln in lines[at+1:limit]:
            if not ln.strip():
                continue
            if not ln.startswith("            "):
                raise ValueError("unexpected noninstruction in disassembly")
            match = re.fullmatch(r" {12}([0-9a-f]{2}(?: [0-9a-f]{2})*)(?: {2,}(.*)|$)", ln)
            if not match:
                raise ValueError("invalid machine-code byte rendering")
            total += len(match.group(1).split(" "))
            asm = (match.group(2) or "").strip()
            if asm == "retq" or asm.startswith("retq "):
                if returned is not None:
                    raise ValueError("duplicate function return")
                returned = total
        if returned is None or total == 0 or returned > total:
            raise ValueError("no valid first-return prefix")
        result["symbol_region_bytes"] += total
        result["bytes_through_first_return"] += returned
        result["post_return_region_bytes"] += total - returned
        if number in (0, count // 2, count - 1):
            representative.append({"function": number, "through_ret": returned,
                                   "region": total})
    return dict(result, qualified_function_count=count,
                inspected_representative_functions=representative)


def validate(path: Path) -> dict:
    root = path.parent
    doc = json.loads(path.read_bytes())
    if (doc.get("schema") != SCHEMA
            or doc.get("status") != "bounded-synthetic-multiplicity-compiler-aot-finite-checked"
            or doc.get("target") != TARGET
            or doc.get("source", {}).get("upstream_commit") != UPSTREAM
            or doc.get("copies") != list(COUNTS)
            or len(doc.get("cases", [])) != len(COUNTS)):
        raise ValueError("unrecognized or incomplete compiler experiment receipt")
    output = []
    for count, item in zip(COUNTS, doc["cases"]):
        if item.get("copies") != count or set(item.get("results", {})) != {"baseline", "patched"}:
            raise ValueError("misidentified experimental case")
        fixture = (root / f"functions-{count:03d}" / "fixture.wat").read_bytes()
        expected = render_wat(count).encode()
        if fixture != expected or sha(fixture) != item.get("source_wat_sha256"):
            raise ValueError("Wasm fixture tampered or does not match the pinned template")
        measured = {}
        for variant, entry in item["results"].items():
            folder = root / f"functions-{count:03d}" / variant
            binary = folder / "functions.cwasm"
            data = binary.read_bytes()
            if (sha(data) != entry.get("aot_sha256") or len(data) != entry.get("aot_bytes")):
                raise ValueError("AOT artifact provenance mismatch")
            disassembly = (folder / "disassembly.txt").read_bytes()
            if sha(disassembly) != entry.get("disassembly_sha256"):
                raise ValueError("disassembly archive mismatch")
            if entry.get("function_headers_checked") != count:
                raise ValueError("function cardinality missing")
            coverage = entry.get("finite_oracle", {})
            if (coverage.get("finite_oracle_passed") is not True
                    or coverage.get("checked_pairs") != 12 * len({0, count // 2, count - 1})
                    or coverage.get("checked_exports") != sorted({0, count // 2, count - 1})
                    or coverage.get("classification") != "finite-execution-only-not-exhaustive-proof"):
                raise ValueError("missing or implausible finite oracle coverage")
            actual_text = independent_elf_text_size(binary)
            if actual_text != entry.get("aot_elf_text_bytes"):
                raise ValueError("AOT ELF text measurement disagrees")
            parsed_cost = independently_count_function_bytes(disassembly.decode("utf-8"), count)
            if parsed_cost != entry.get("sum_function_costs"):
                raise ValueError("independent aggregate code-byte measurement disagrees")
            measured[variant] = {"text_bytes": actual_text, "serialized_bytes": len(data),
                                 "function_costs": parsed_cost,
                                 "aot_sha256": sha(data)}
        a, b = measured["baseline"], measured["patched"]
        deltas = {"aot_elf_text_bytes": a["text_bytes"] - b["text_bytes"],
                  "whole_cwasm_bytes": a["serialized_bytes"] - b["serialized_bytes"],
                  "sum_function_symbol_region_bytes": a["function_costs"]["symbol_region_bytes"] - b["function_costs"]["symbol_region_bytes"],
                  "sum_function_bytes_through_first_return": a["function_costs"]["bytes_through_first_return"] - b["function_costs"]["bytes_through_first_return"]}
        if deltas != item.get("baseline_minus_candidate"):
            raise ValueError("reported baseline/candidate delta is false")
        output.append({"copies": count, "verified_deltas": deltas, "bytes": measured,
                       "synthetic_only": True})
    return {"schema": "zeno-fcis/tier1-wasmtime-scaling-independent-recheck/1",
            "status": "independent-aot-binary-cost-rechecked-finite-wasm-only",
            "source_wat_original_sha256": SOURCE_SHA256,
            "source_receipt_sha256": sha(path.read_bytes()), "target": TARGET,
            "cases": output,
            "scope": "synthetic function multiplicity; no real-application or runtime win",
            "semantic_scope": "finite exported-function tests; conditional VeriISLE select model"}


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--source-receipt", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    if a.out.exists():
        p.error("cannot overwrite independent receipt")
    result = validate(a.source_receipt.resolve())
    a.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": result["status"], "cost": [
        {"count": c["copies"], "deltas": c["verified_deltas"]}
        for c in result["cases"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
