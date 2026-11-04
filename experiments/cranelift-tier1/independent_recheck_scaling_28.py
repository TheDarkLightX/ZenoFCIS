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
            measured[variant] = {"text_bytes": actual_text, "serialized_bytes": len(data),
                                 "aot_sha256": sha(data)}
        a, b = measured["baseline"], measured["patched"]
        deltas = {"aot_elf_text_bytes": a["text_bytes"] - b["text_bytes"],
                  "whole_cwasm_bytes": a["serialized_bytes"] - b["serialized_bytes"]}
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
