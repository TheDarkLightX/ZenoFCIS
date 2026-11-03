#!/usr/bin/env python3
"""Actual Wasmtime Wasm-frontend A/B: same WAT under pinned baseline and patched CLI."""
from __future__ import annotations
import argparse, hashlib, json, platform, re, statistics, subprocess, time
from pathlib import Path
from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB, RULE

HERE = Path(__file__).resolve().parent
WAT = HERE / "satadd_wasmtime_28.wat"
MIN, MAX = -(1 << 63), (1 << 63) - 1
VECTORS = ((0, 0), (1, 2), (-1, -2), (MAX, 1), (MIN, -1),
           (MAX, MAX), (MIN, MIN), (MAX, MIN), (MIN, MAX),
           (MAX, -1), (MIN, 1), (12345678901234, -12345678901234))
COUNTS = (0, 1, 2, 3, 7, 17, 257, 4096)
ABBA = ("baseline", "patched", "patched", "baseline")
BAAB = ("patched", "baseline", "baseline", "patched")

def digest(b):
    return hashlib.sha256(b).hexdigest()

def run(cmd, timeout=180):
    return subprocess.run(cmd, text=True, capture_output=True, check=True, timeout=timeout)

def signed_sat(x, y):
    return max(MIN, min(MAX, x + y))

def loop_oracle(n):
    if not 0 <= n <= 10_000_000:
        raise ValueError("count outside bounded domain")
    value = 1
    for i in range(n):
        value = signed_sat(value, 9223372036854775800 if i & 1 else -9223372036854775799)
    return value

def numeric_result(stdout):
    lines = stdout.strip().splitlines()
    if len(lines) != 1 or not re.fullmatch(r"-?\d+", lines[0]):
        raise ValueError("missing/ambiguous signed result")
    val = int(lines[0])
    if not MIN <= val <= MAX:
        raise ValueError("result outside i64")
    return val

def text_size(obj):
    result = run(["readelf", "-SW", str(obj)]).stdout
    hits = re.findall(r"(?m)^\s*\[\s*\d+\]\s+\.text\s+\S+\s+\S+\s+\S+\s+([0-9a-fA-F]+)\s+", result)
    if len(hits) != 1 or int(hits[0], 16) == 0:
        raise ValueError("missing/ambiguous nonempty AOT ELF .text")
    return int(hits[0], 16)

def source_pins(upstream, bins):
    if run(["git", "-C", str(upstream), "rev-parse", "HEAD"]).stdout.strip() != UPSTREAM:
        raise ValueError("upstream revision differs")
    if run(["git", "-C", str(upstream), "rev-parse", "HEAD:" + SOURCE]).stdout.strip() != ORIGINAL_GIT_BLOB:
        raise ValueError("original ISLE blob differs")
    original = run(["git", "-C", str(upstream), "show", "HEAD:" + SOURCE]).stdout
    if (upstream / SOURCE).read_text() != original.rstrip("\n") + "\n" + RULE:
        raise ValueError("incorrect isolated ISLE rule")
    if run(["git", "-C", str(upstream), "diff", "--name-only"]).stdout.splitlines() != [SOURCE]:
        raise ValueError("unexpected upstream source changes")
    identities = {}
    for label, path in bins.items():
        if not path.is_file() or path.stat().st_size == 0:
            raise ValueError("missing CLI build: " + label)
        identities[label] = {"sha256": digest(path.read_bytes()), "version":
            run([str(path), "--version"]).stdout.strip(), "bytes": path.stat().st_size}
    if identities["baseline"]["version"] != identities["patched"]["version"]:
        raise ValueError("different compiler versions")
    return {"upstream_commit": UPSTREAM, "wasm_source_sha256": digest(WAT.read_bytes()),
            "rule_sha256": digest(RULE.encode()), "compiler_binaries": identities,
            "host_machine": platform.machine()}

def compile_wat(binary, out, name):
    folder = out / name
    folder.mkdir()
    clif = folder / "clif"
    clif.mkdir()
    obj = folder / "satadd.cwasm"
    result = run([str(binary), "compile", "--emit-clif", str(clif),
                  "-o", str(obj), str(WAT)], timeout=300)
    (folder / "compile.stdout").write_text(result.stdout)
    (folder / "compile.stderr").write_text(result.stderr)
    if not obj.is_file():
        raise ValueError("compiler emitted no AOT module")
    dis = run([str(binary), "objdump", "--funcs", "wasm", "--bytes", str(obj)]).stdout
    if not dis.strip():
        raise ValueError("no Wasm function disassembly")
    (folder / "disassembly.txt").write_text(dis)
    clif_files = sorted(p for p in clif.rglob("*") if p.is_file())
    if not clif_files:
        raise ValueError("no CLIF emitted by Wasmtime frontend")
    return {"aot_sha256": digest(obj.read_bytes()), "aot_bytes": obj.stat().st_size,
            "elf_text_bytes": text_size(obj), "disassembly_sha256": digest(dis.encode()),
            "clif_files": [{"path": str(p.relative_to(clif)), "sha256": digest(p.read_bytes())}
                           for p in clif_files]}

def invoke(binary, obj, name, *args):
    return numeric_result(run([str(binary), "run", "--invoke", name, str(obj),
                               *[str(a) for a in args]], timeout=120).stdout)

def check_finite(bins, out):
    rows = {}
    for name, exe in bins.items():
        module = out / name / "satadd.cwasm"
        for x, y in VECTORS:
            if invoke(exe, module, "sat", x, y) != signed_sat(x, y):
                raise ValueError("incorrect signed saturation for " + name)
        for n in COUNTS:
            if invoke(exe, module, "workload", n) != loop_oracle(n):
                raise ValueError("incorrect Wasm loop for " + name)
        rows[name] = {"sat_pairs": len(VECTORS), "loop_counts": list(COUNTS), "passed": True}
    return rows

def time_exploratory(bins, out, blocks, count):
    if not 1 <= blocks <= 12 or not 1000 <= count <= 10_000_000:
        raise ValueError("timing work budget invalid")
    expected = loop_oracle(count)
    measures = []
    for block in range(blocks):
        for position, name in enumerate(ABBA if block % 2 == 0 else BAAB):
            t = time.perf_counter_ns()
            got = invoke(bins[name], out / name / "satadd.cwasm", "workload", count)
            ns = time.perf_counter_ns() - t
            if got != expected or ns <= 0:
                raise ValueError("incorrect or invalid timed result")
            measures.append({"block": block, "position": position, "variant": name,
                             "elapsed_ns": ns, "value": got})
    base = [m["elapsed_ns"] for m in measures if m["variant"] == "baseline"]
    patch = [m["elapsed_ns"] for m in measures if m["variant"] == "patched"]
    return {"status": "exploratory-only-no-speed-winner",
            "scope": "whole Wasmtime CLI process: AOT loading/startup/invocation, not CPU latency or throughput",
            "blocks": blocks, "loop_iterations": count, "raw_samples": measures,
            "median_candidate_over_baseline": statistics.median(patch) / statistics.median(base)}

def qualify(upstream, bins, out, blocks, count):
    if out.exists():
        raise ValueError("refusing to overwrite evidence")
    pins = source_pins(upstream, bins)
    out.mkdir(parents=True)
    results = {name: compile_wat(exe, out, name) for name, exe in bins.items()}
    finite = check_finite(bins, out)
    timing = time_exploratory(bins, out, blocks, count)
    old, new = results["baseline"], results["patched"]
    return {"schema": "zeno-fcis/tier1-wasmtime-frontend-28/1",
            "status": "finite-wasm-aot-check-passed-runtime-unqualified",
            "source": pins, "compiled": results, "finite_checks": finite,
            "timing": timing,
            "aot_text_delta_baseline_minus_patched": old["elf_text_bytes"] - new["elf_text_bytes"],
            "whole_cwasm_delta_baseline_minus_patched": old["aot_bytes"] - new["aot_bytes"],
            "clif_same_file_and_content": old["clif_files"] == new["clif_files"],
            "nonclaims": ["one constructed Wasm workload, not a production benchmark suite",
                          "AOT module text contains trampolines and other functions",
                          "no native instruction latency or throughput speedup",
                          "finite execution not exhaustive Wasm semantic proof",
                          "conditional VeriISLE proof uses provisional select spec",
                          "synthetic CLIF size savings do not automatically transfer to Wasm"]}

def main():
    p = argparse.ArgumentParser()
    for name in ("upstream", "baseline-bin", "patched-bin", "out"):
        p.add_argument("--" + name, type=Path, required=True)
    p.add_argument("--blocks", type=int, default=6)
    p.add_argument("--loops", type=int, default=2000000)
    a = p.parse_args()
    bins = {"baseline": a.baseline_bin.resolve(), "patched": a.patched_bin.resolve()}
    report = qualify(a.upstream.resolve(), bins, a.out.resolve(), a.blocks, a.loops)
    (a.out / "qualification.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": report["status"], "text_delta": report["aot_text_delta_baseline_minus_patched"],
                      "clif_same": report["clif_same_file_and_content"],
                      "timing_only_exploratory_ratio": report["timing"]["median_candidate_over_baseline"]}))

if __name__ == "__main__":
    main()
