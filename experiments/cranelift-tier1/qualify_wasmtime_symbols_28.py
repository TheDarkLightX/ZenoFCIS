#!/usr/bin/env python3
"""Pinned Wasmtime AOT same-WAT source-level cost audit: symbol versus prefix versus module."""
from __future__ import annotations
import argparse, hashlib, json, re, subprocess
from pathlib import Path
UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
SAT = "wasm[0]::function[0]::sat"
WORKLOAD = "wasm[0]::function[1]"

def sha(data): return hashlib.sha256(data).hexdigest()

def cmd(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout

def parse_symbols(text):
    found = {}
    for line in text.splitlines():
        f = line.split()
        if len(f) != 4 or f[1] not in ("t", "T"):
            continue
        name, kind, offset, size = f
        if name in found:
            raise ValueError("duplicate text symbol")
        try:
            found[name] = {"offset": int(offset, 16), "bytes": int(size, 16)}
        except ValueError as error:
            raise ValueError("invalid symbol extent") from error
    if SAT not in found or WORKLOAD not in found or len(found) < 6:
        raise ValueError("missing or incomplete Wasmtime text symbol set")
    if any(x["bytes"] <= 0 for x in found.values()):
        raise ValueError("empty symbol")
    return found

def code_prefix(dis, symbol):
    headers = list(re.finditer(r"(?m)^([0-9a-f]+) <([^>]+)>:\s*$", dis))
    indexes = [i for i, h in enumerate(headers) if h.group(2) == symbol]
    if len(indexes) != 1:
        raise ValueError("missing or duplicated symbol in disassembly")
    i = indexes[0]
    start = int(headers[i].group(1), 16)
    end = headers[i+1].start() if i+1 < len(headers) else len(dis)
    body = dis[headers[i].end():end]
    returns = re.findall(r"(?m)^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+ret(?:q)?\b", body)
    if len(returns) != 1:
        raise ValueError("expected one return in straight-line instruction body")
    prefix = int(returns[0], 16) + 1 - start
    if prefix <= 0:
        raise ValueError("invalid instruction prefix")
    return prefix

def qualify(root):
    receipt_path = root / "qualification.json"
    r = json.loads(receipt_path.read_text())
    if r.get("status") != "finite-wasm-aot-check-passed-runtime-unqualified":
        raise ValueError("finite AOT Wasmtime execution not qualified")
    if r.get("source", {}).get("upstream_commit") != UPSTREAM:
        raise ValueError("unexpected compiler source pin")
    for k in ("baseline", "patched"):
        if r.get("finite_checks", {}).get(k, {}).get("passed") is not True:
            raise ValueError("missing finite execution result")
    syms, pref = {}, {}
    for name in ("baseline", "patched"):
        obj = root / name / "satadd.cwasm"
        raw = obj.read_bytes()
        if sha(raw) != r["compiled"][name]["aot_sha256"] or len(raw) != r["compiled"][name]["aot_bytes"]:
            raise ValueError("AOT bytes differ from source-bound execution result")
        header = cmd("readelf", "-h", str(obj))
        if "ELF64" not in header or "Advanced Micro Devices X86-64" not in header:
            raise ValueError("unqualified ELF architecture")
        syms[name] = parse_symbols(cmd("nm", "-S", "--defined-only", "--format=posix", str(obj)))
        pref[name] = code_prefix(cmd("objdump", "-d", "-Mintel", str(obj)), SAT)
    a, b = syms["baseline"], syms["patched"]
    if set(a) != set(b) or sorted(x for x in a if a[x]["bytes"] != b[x]["bytes"]) != [SAT]:
        raise ValueError("unexpected missing/changed unrelated function or trampoline")
    if a[SAT]["offset"] != b[SAT]["offset"] or a[WORKLOAD] != b[WORKLOAD]:
        raise ValueError("unexpected Wasm code-location or loop change")
    n, m = a[SAT]["bytes"], b[SAT]["bytes"]
    if (n, m) != (64, 52) or pref != {"baseline": 56, "patched": 52}:
        raise ValueError("pinned known Wasmtime function size positive control drift")
    tails = {key: syms[key][SAT]["bytes"] - pref[key] for key in pref}
    if tails != {"baseline": 8, "patched": 0}:
        raise ValueError("post-ret literal/padding profile drift")
    if r["compiled"]["baseline"]["elf_text_bytes"] != r["compiled"]["patched"]["elf_text_bytes"]:
        raise ValueError("whole module .text alignment size changed")
    if r["compiled"]["baseline"]["aot_bytes"] != r["compiled"]["patched"]["aot_bytes"]:
        raise ValueError("whole module serialized size changed")
    return {
        "schema": "zeno-fcis/tier1-wasmtime-aot-function-size/1",
        "status": "qualified-function-level-same-wat-code-size",
        "upstream_pin": UPSTREAM,
        "source_wat_sha256": r["source"]["wasm_source_sha256"],
        "qualification_sha256": sha(receipt_path.read_bytes()),
        "objects": {name: r["compiled"][name]["aot_sha256"] for name in pref},
        "sat_symbol_bytes": {"baseline": n, "patched": m},
        "sat_prefix_through_ret_bytes": pref,
        "sat_after_ret_literal_padding_bytes": tails,
        "symbol_savings_bytes": n-m, "executed_prefix_savings_bytes": pref["baseline"]-pref["patched"],
        "symbol_savings_percent": 100*(n-m)/n,
        "unmodified_loop_symbol_bytes": a[WORKLOAD]["bytes"],
        "whole_aot_text_bytes_both": r["compiled"]["baseline"]["elf_text_bytes"],
        "whole_aot_file_bytes_both": r["compiled"]["baseline"]["aot_bytes"],
        "nonclaims": [
            "no whole-module disk/file-size reduction shown on this page-aligned module",
            "one small signed-i64 WebAssembly function only; not program-wide savings",
            "the post-ret bytes include literal data and/or padding, not executed instruction bytes",
            "no speed/latency/throughput improvement established",
            "finite Wasmtime runtime checks and SMT identity are not full compiler proof",
            "VeriISLE still conditional on provisional local select spec",
        ],
    }

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--root", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    if a.out.exists():
        p.error("fresh receipt only")
    result = qualify(a.root.resolve())
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(result, indent=2, sort_keys=True)+"\n")
    print(json.dumps({"status": result["status"], "symbol": result["sat_symbol_bytes"],
                      "prefix": result["sat_prefix_through_ret_bytes"]}, sort_keys=True))
if __name__ == "__main__":
    main()
