#!/usr/bin/env python3
"""Independent readelf + raw-section layout audit of previously qualified AOTs.

This uses only existing archived binary artifacts. It never recompiles,
changes source, infers speed or declares a specific padding cause.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

SOURCE_RUN = 41769602461
EXPECTED_COUNTS = (1, 16, 64, 256, 1024)
SCHEMA = "zeno-fcis/tier1-aot-elf-layout/1"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sections(path: Path) -> dict[str, dict]:
    output = subprocess.run(["readelf", "-SW", str(path)],
                            capture_output=True, text=True, check=True).stdout
    result = {}
    for line in output.splitlines():
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "[" and len(parts) > 7:
            name_idx = 2
        elif re.fullmatch(r"\[\d+\]", parts[0]) and len(parts) > 6:
            name_idx = 1
        else:
            continue
        name = parts[name_idx]
        if name in result:
            raise ValueError("duplicate ELF section name: " + name)
        try:
            offset = int(parts[name_idx + 3], 16)
            size = int(parts[name_idx + 4], 16)
        except (IndexError, ValueError) as err:
            raise ValueError("malformed ELF section record") from err
        result[name] = {"offset": offset, "bytes": size, "type": parts[name_idx+1]}
    if ".text" not in result or result[".text"]["bytes"] < 1:
        raise ValueError("missing or empty .text")
    return result


def function_symbols(path: Path) -> dict:
    output = subprocess.run(["readelf", "-Ws", str(path)],
                            capture_output=True, text=True, check=True).stdout
    entries = []
    for line in output.splitlines():
        m = re.match(
            r"^\s*\d+:\s+([0-9a-fA-F]+)\s+(\d+)\s+FUNC\s+\S+\s+\S+\s+(\S+)\s+(.*)$",
            line,
        )
        if m:
            entries.append({"value": int(m.group(1), 16),
                            "size": int(m.group(2)),
                            "section_index": m.group(3),
                            "name": m.group(4).strip()})
    return {"function_symbol_count": len(entries),
            "summed_function_symbol_sizes": sum(x["size"] for x in entries),
            "zero_size_function_symbols": sum(x["size"] == 0 for x in entries),
            "first_function_names": [x["name"] for x in entries[:5]]}


def exact_one(root: Path, pattern: str) -> Path:
    paths = list(root.glob("**/" + pattern))
    if len(paths) != 1:
        raise ValueError(f"not exactly one archived path for {pattern}: {len(paths)}")
    return paths[0]


def compare_sections(paths: dict[str, Path]) -> dict:
    data = {name: path.read_bytes() for name, path in paths.items()}
    meta = {name: sections(path) for name, path in paths.items()}
    records = {}
    for name, path in paths.items():
        secs = meta[name]
        text = secs[".text"]
        buf = data[name]
        offset, length = text["offset"], text["bytes"]
        if offset < 0 or offset+length > len(buf):
            raise ValueError("ELF .text section extends beyond stored object")
        records[name] = {
            "file_sha256": sha(buf), "file_bytes": len(buf),
            "text_sha256": sha(buf[offset:offset+length]),
            "text_bytes": length,
            "sections": {k: v["bytes"] for k, v in sorted(secs.items())},
            "function_symbols": function_symbols(path),
        }
    a, b = records["baseline"], records["patched"]
    if "patched" not in records or "baseline" not in records:
        raise ValueError("missing AOT variant")
    return {
        "variants": records,
        "deltas_baseline_minus_patched": {
            "elf_text_bytes": a["text_bytes"] - b["text_bytes"],
            "whole_cwasm_bytes": a["file_bytes"] - b["file_bytes"],
            "readelf_function_symbol_size_sum": (
                a["function_symbols"]["summed_function_symbol_sizes"]
                - b["function_symbols"]["summed_function_symbol_sizes"]
            ),
        },
        "same_elf_text_size": a["text_bytes"] == b["text_bytes"],
        "same_elf_text_content": a["text_sha256"] == b["text_sha256"],
        "same_module_content": a["file_sha256"] == b["file_sha256"],
    }


def qualify(root: Path) -> dict:
    producer_path = exact_one(root, "generated/qualification.json")
    producer = json.loads(producer_path.read_bytes())
    if (producer.get("schema") != "zeno-fcis/tier1-wasmtime-code-size-scaling/1"
        or producer.get("status") != "bounded-synthetic-multiplicity-compiler-aot-finite-checked"
        or producer.get("copies") != list(EXPECTED_COUNTS)
        or len(producer.get("cases", [])) != len(EXPECTED_COUNTS)):
        raise ValueError("unrecognized or incomplete bounded source receipt")
    independent_path = exact_one(root, "independent-recheck.json")
    independent = json.loads(independent_path.read_bytes())
    if (independent.get("status") != "independent-aot-binary-cost-rechecked-finite-wasm-only"
        or independent.get("source_receipt_sha256") != sha(producer_path.read_bytes())):
        raise ValueError("independent source receipt mismatch")
    cases = []
    for count, source_case, audited in zip(EXPECTED_COUNTS, producer["cases"], independent["cases"]):
        if source_case.get("copies") != count or audited.get("copies") != count:
            raise ValueError("misaligned function multiplicity case")
        paths = {
            name: exact_one(root, f"functions-{count:03d}/{name}/functions.cwasm")
            for name in ("baseline", "patched")
        }
        comparison = compare_sections(paths)
        for name, value in comparison["variants"].items():
            if value["file_sha256"] != source_case["results"][name]["aot_sha256"]:
                raise ValueError("archive object differs from qualified source")
            if value["text_bytes"] != source_case["results"][name]["aot_elf_text_bytes"]:
                raise ValueError("ELF text measurement differs from qualified source")
        if (comparison["deltas_baseline_minus_patched"]["elf_text_bytes"]
            != audited["verified_deltas"]["aot_elf_text_bytes"]
            or comparison["deltas_baseline_minus_patched"]["whole_cwasm_bytes"]
            != audited["verified_deltas"]["whole_cwasm_bytes"]):
            raise ValueError("third-party ELF size audit disagrees with prior")
        cases.append({"exported_function_copies": count, **comparison})
    return {
        "schema": SCHEMA,
        "status": "independent-same-artifact-section-layout-rechecked",
        "source_run": SOURCE_RUN,
        "source_receipts": {"producer_sha256": sha(producer_path.read_bytes()),
                            "independent_sha256": sha(independent_path.read_bytes())},
        "results": cases,
        "nonclaims": [
            "ELF section sizes and symbols alone do not identify why padding or allocation occurs",
            "Wasm code function bytes, ELF text section size and serialized module size differ",
            "no speed, throughput, application-level savings, or semantic proof asserted",
            "source Wasm is synthetic duplicated exported functions only",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--root", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    if a.out.exists():
        p.error("refusing overwritten audit")
    r = qualify(a.root.resolve())
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(r, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": r["status"], "layout": [
        {"count": c["exported_function_copies"],
         "text_baseline": c["variants"]["baseline"]["text_bytes"],
         "text_candidate": c["variants"]["patched"]["text_bytes"],
         "whole_baseline": c["variants"]["baseline"]["file_bytes"],
         "whole_candidate": c["variants"]["patched"]["file_bytes"],
         "text_data_differs": not c["same_elf_text_content"],
         "readelf_function_size_delta":
             c["deltas_baseline_minus_patched"]["readelf_function_symbol_size_sum"]}
        for c in r["results"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
