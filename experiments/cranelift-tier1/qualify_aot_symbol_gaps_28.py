#!/usr/bin/env python3
"""Source-bound attribution of lost Wasmtime AOT function-size savings.

Rechecks exact previously archived AOT objects and baseline/candidate receipts
using the independent ELF audit, then compares named ELF symbol starts, sizes,
alignment and bytes between symbols. This does NOT establish performance or
make changes to any compiler. Refuses calling an alignment cause proven when
symbol names, positions or source-model assumptions cannot be checked.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

from qualify_aot_layout_28 import qualify as previous_elf_qualification

UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
SOURCE_RUN = 41769602461
COUNTS = (1, 16, 64, 256, 1024)
SCHEMA = "zeno-fcis/tier1-wasmtime-symbol-gaps/1"
FILES = {
    "cranelift/codegen/src/isa/x64/inst/mod.rs": "916e6954f51a2e11156b2b1c78d98300cb8ac36a",
    "cranelift/codegen/src/machinst/buffer.rs": "bb9f957a269eb36060d4f1d37a9cf195fb8d6f9c",
    "crates/cranelift/src/obj.rs": "632fda4c4dcbb66960fd37ee384613dbbc064fd0",
}
TEXT_PATTERN = re.compile(
    r"^\s*\[\s*(\d+)\]\s+\.text\s+\S+\s+"
    r"([0-9a-fA-F]+)\s+([0-9a-fA-F]+)\s+([0-9a-fA-F]+)\b.*?\s+(\d+)\s*$"
)
SYMBOL_PATTERN = re.compile(
    r"^\s*\d+:\s+([0-9a-fA-F]+)\s+(\d+)\s+FUNC\s+"
    r"\S+\s+\S+\s+(\d+)\s+(.+?)\s*$"
)
INDEX_PATTERN = re.compile(r"wasm\[0\]::function\[(\d+)\]")

def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def command(args: list[str]) -> str:
    return subprocess.run(args, capture_output=True, text=True,
                          check=True, timeout=60).stdout

def audit_source(upstream: Path) -> dict:
    head = command(["git", "-C", str(upstream), "rev-parse", "HEAD"]).strip()
    if head != UPSTREAM:
        raise ValueError("upstream changed after pinning")
    texts = {}
    for path, blob_sha in FILES.items():
        actual = command(["git", "-C", str(upstream), "rev-parse", "HEAD:" + path]).strip()
        if actual != blob_sha:
            raise ValueError("upstream source blob mismatch: " + path)
        content = (upstream / path).read_text()
        texts[path] = content
    inst = texts["cranelift/codegen/src/isa/x64/inst/mod.rs"]
    buffer = texts["cranelift/codegen/src/machinst/buffer.rs"]
    obj = texts["crates/cranelift/src/obj.rs"]
    if not re.search(r"fn function_alignment\(\).*?minimum:\s*1,.*?preferred:\s*32,",
                     inst, re.S):
        raise ValueError("x64 preferred alignment is not the claimed 32 bytes")
    if not re.search(r"impl<I: VCodeInst> TextSectionBuilder for MachTextSectionBuilder<I> \{"
                     r".*?fn append\(.*?self\.buf\.align_to\(align\);"
                     r".*?self\.buf\.put_data\(func\);", buffer, re.S):
        raise ValueError("compiled function body alignment path not established")
    if not re.search(r"fn text_align\(compiler: &Compiler\) -> u64 \{"
                     r".*?compiler\.page_size_align\(\)", obj, re.S):
        raise ValueError("non-Pulley page alignment path not established")
    if "set_data(text, text_align(self.compiler))" not in obj:
        raise ValueError("object emission text-section alignment path not established")
    return {
        "upstream_commit": head,
        "git_blob_pins": FILES,
        "source_facts": {
            "x64_preferred_function_alignment_bytes": 32,
            "x64_minimum_function_alignment_bytes": 1,
            "MachTextSectionBuilder_aligns_before_putting_function_bytes": True,
            "non_pulley_elf_text_alignment_from_compiler_page_size": True,
        },
    }

def parse_text_section(raw: str) -> dict:
    hits = [TEXT_PATTERN.match(line) for line in raw.splitlines()]
    hits = [m for m in hits if m]
    if len(hits) != 1:
        raise ValueError("expected exactly one ELF .text section")
    m = hits[0]
    index, address, offset, size, align = (
        int(m.group(1)), int(m.group(2), 16), int(m.group(3), 16),
        int(m.group(4), 16), int(m.group(5)))
    if size <= 0 or align <= 0 or (align & (align - 1)):
        raise ValueError("empty or invalid section alignment")
    return {"section_index": index, "address": address, "offset": offset,
            "size": size, "alignment": align}

def parse_symbols(raw: str, text: dict) -> list[dict]:
    entries = {}
    for line in raw.splitlines():
        m = SYMBOL_PATTERN.match(line)
        if not m or int(m.group(3)) != text["section_index"]:
            continue
        addr, size, name = int(m.group(1), 16), int(m.group(2)), m.group(4)
        if size == 0:
            continue
        if not text["address"] <= addr or addr + size > text["address"] + text["size"]:
            raise ValueError("function symbol out of .text range")
        record = {"name": name, "offset": addr - text["address"], "size": size}
        if name in entries and entries[name] != record:
            raise ValueError("symbol name maps to conflicting executable addresses")
        entries[name] = record
    return sorted(entries.values(), key=lambda x: (x["offset"], x["name"]))

def admit_symbols(symbols: list[dict], count: int) -> dict[int, dict]:
    found = {}
    for sym in symbols:
        match = INDEX_PATTERN.search(sym["name"])
        if not match:
            continue
        idx = int(match.group(1))
        if idx >= count or idx in found:
            raise ValueError("duplicated or out-of-scope Wasm function symbol")
        found[idx] = sym
    if set(found) != set(range(count)):
        return {}  # Missing debug symbols: refuse an alignment explanation.
    return found

def next_symbol(symbols: list[dict], target: dict) -> dict | None:
    off = target["offset"]
    later = [s for s in symbols if s["offset"] > off]
    if not later:
        return None
    nxt = min(later, key=lambda s: s["offset"])
    if target["offset"] + target["size"] > nxt["offset"]:
        raise ValueError("overlapping ELF function symbols")
    return nxt

def examine_pair(paths: dict[str, Path], count: int) -> dict:
    variants = {}
    for variant in ("baseline", "patched"):
        path = paths[variant]
        raw = path.read_bytes()
        text = parse_text_section(command(["readelf", "-SW", str(path)]))
        sym = parse_symbols(command(["readelf", "-Ws", str(path)]), text)
        variants[variant] = {"binary": raw, "text": text, "symbols": sym,
                             "selected": admit_symbols(sym, count)}
    a, b = variants["baseline"], variants["patched"]
    record = {"copies": count,
              "text_alignment": {label: v["text"]["alignment"] for label, v in variants.items()},
              "named_text_function_symbols": {label: len(v["symbols"]) for label, v in variants.items()},
              "wasm_export_symbols_admitted": {
                  label: len(v["selected"]) for label, v in variants.items()},
              "same_export_starts": None, "function_symbol_savings": None,
              "matched_inter_symbol_gap_growth": None,
              "observed_gap_examples": [],
              "classification": "unresolved-no-verified-symbol-attribution"}
    if any(len(v["selected"]) != count for v in variants.values()):
        return record

    same_starts = all(a["selected"][i]["offset"] == b["selected"][i]["offset"]
                      for i in range(count))
    record["same_export_starts"] = same_starts
    record["function_symbol_savings"] = sum(
        a["selected"][i]["size"] - b["selected"][i]["size"] for i in range(count))
    gap_growth = 0
    pair_count = 0
    matched = 0
    for i in range(count):
        left, right = a["selected"][i], b["selected"][i]
        nxt_a, nxt_b = next_symbol(a["symbols"], left), next_symbol(b["symbols"], right)
        if not nxt_a or not nxt_b:
            continue
        if nxt_a["name"] != nxt_b["name"] or nxt_a["offset"] != nxt_b["offset"]:
            continue
        gap_a = nxt_a["offset"] - left["offset"] - left["size"]
        gap_b = nxt_b["offset"] - right["offset"] - right["size"]
        delta = left["size"] - right["size"]
        if gap_b - gap_a == delta and left["offset"] == right["offset"]:
            matched += 1
            gap_growth += gap_b - gap_a
        if len(record["observed_gap_examples"]) < 5:
            record["observed_gap_examples"].append({
                "function": i,
                "source_symbol": left["name"],
                "next_symbol": nxt_a["name"],
                "baseline_size": left["size"], "patched_size": right["size"],
                "baseline_gap": gap_a, "patched_gap": gap_b,
                "baseline_start": left["offset"], "patched_start": right["offset"],
                "expected_gap_growth": delta,
                "measured_gap_growth": gap_b - gap_a,
            })
        pair_count += 1
    record["inter_symbol_pairs_compared"] = pair_count
    record["inter_symbol_pairs_explained"] = matched
    record["matched_inter_symbol_gap_growth"] = gap_growth
    record["all_export_starts_multiple_of_32"] = all(
        rec["offset"] % 32 == 0 for v in variants.values()
        for rec in v["selected"].values())
    if (same_starts and pair_count > 0 and matched == pair_count
            and record["function_symbol_savings"] > 0
            and a["text"]["size"] == b["text"]["size"]):
        record["classification"] = "symbol-shrink-absorbed-in-observed-unchanged-start-gaps"
    return record

def qualify(root: Path, upstream: Path) -> dict:
    evidence = previous_elf_qualification(root)
    source = audit_source(upstream)
    rows = []
    for count, base_case in zip(COUNTS, evidence["results"]):
        if base_case["exported_function_copies"] != count:
            raise ValueError("previous ELF audit order changed")
        paths = {
            name: next(root.glob("**/functions-%03d/%s/functions.cwasm" % (count, name)))
            for name in ("baseline", "patched")
        }
        # Require every binary to remain identical to the independent audit.
        for name, path in paths.items():
            if sha(path.read_bytes()) != base_case["variants"][name]["file_sha256"]:
                raise ValueError("archive binary differs from qualified ELF evidence")
        row = examine_pair(paths, count)
        if row["function_symbol_savings"] is not None:
            if row["function_symbol_savings"] != base_case[
                "deltas_baseline_minus_patched"]["readelf_function_symbol_size_sum"]:
                raise ValueError("Wasm-named function symbol savings conflict with earlier audit")
        rows.append(row)
    return {
        "schema": SCHEMA,
        "status": "pinned-upstream-source-and-archived-ELF-symbol-spacing-audited",
        "source": source,
        "source_run": SOURCE_RUN,
        "prior_elf_recheck_sha256": sha(json.dumps(
            evidence, sort_keys=True).encode()),
        "cases": rows,
        "nonclaims": [
            "unmodified source fact does not prove which compiler alignment preference was selected",
            "unmatched symbols, aliases, trampolines, island code and link layout may remain",
            "no instruction latency/throughput or whole-artifact size improvement",
            "finite Wasm oracle and provisional select-spec coverage remain limited",
            "no upstream-ready performance optimization identified",
        ],
    }

def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--root", type=Path, required=True)
    p.add_argument("--upstream", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    if args.out.exists():
        p.error("refuse to overwrite evidence")
    result = qualify(args.root.resolve(), args.upstream.resolve())
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({
        "status": result["status"],
        "cases": [{
            "copies": x["copies"],
            "result": x["classification"],
            "named": x["wasm_export_symbols_admitted"],
            "starts_unchanged": x["same_export_starts"],
            "symbol_savings": x["function_symbol_savings"],
            "gap_growth": x["matched_inter_symbol_gap_growth"],
            "gap_pairs": x.get("inter_symbol_pairs_explained"),
        } for x in result["cases"]]}, sort_keys=True))

if __name__ == "__main__":
    main()
