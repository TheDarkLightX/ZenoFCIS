#!/usr/bin/env python3
"""Run balanced ABBA/BAAB call-level screening on the exact pinned Cranelift ELF objects.

No speed claim is made. Paired per-round ratios are descriptive, and artifacts
must pass original SHA-256 and preflight semantics. The pre-existing v1 evidence
is retained unchanged; this creates a new, explicitly v2 provenance receipt.
"""
from __future__ import annotations
import argparse
import csv
import hashlib
import io
import json
import os
import platform
import statistics
import tempfile
from pathlib import Path

from benchmark_exact_objects import EXPECTED, PIN, sh

MODES = ("chain_nonoverflow", "chain_saturating", "parallel8")
V2 = "tier1-bench-balanced-v2"

def check_rows(rows: list[dict], calls: int, rounds: int) -> dict:
    """Refuse reordered, omitted or forged workload rows before deriving ratios."""
    if len(rows) != rounds * 12:
        raise ValueError("expected exactly four position-tagged measurements per mode/round")
    blocks = {}
    for r in rows:
        if (r.get("schema") != V2 or r.get("mode") not in MODES or
                r.get("variant") not in ("baseline", "candidate")):
            raise ValueError("unexpected record schema, mode or variant")
        try:
            round_n = int(r["round"])
            pos = int(r["position"])
            ns, checksum = int(r["ns"]), int(r["checksum"])
            c = int(r["calls"])
        except (ValueError, KeyError, TypeError) as exc:
            raise ValueError("invalid numeric benchmark field") from exc
        if not (0 <= round_n < rounds and 0 <= pos < 4 and ns > 0 and c == calls):
            raise ValueError("invalid round, position, timing or call count")
        blocks.setdefault((round_n, r["mode"]), []).append((pos, r["variant"], ns, checksum))
    report = {}
    if len(blocks) != rounds * 3:
        raise ValueError("missing round/mode block")
    for mode_index, mode in enumerate(MODES):
        paired = []
        checksums = set()
        for round_n in range(rounds):
            group = blocks.get((round_n, mode), [])
            if len(group) != 4 or [pos for pos, *_ in group] != [0, 1, 2, 3]:
                raise ValueError("missing, duplicate or reordered position in block")
            first = "candidate" if (round_n + mode_index) % 2 else "baseline"
            other = "baseline" if first == "candidate" else "candidate"
            if [variant for _, variant, *_ in group] != [first, other, other, first]:
                raise ValueError("ABBA/BAAB position assignment violated")
            checksums.update(checksum for _, _, _, checksum in group)
            base = sum(ns for _, v, ns, _ in group if v == "baseline")
            cand = sum(ns for _, v, ns, _ in group if v == "candidate")
            paired.append(cand / base)
        if len(checksums) != 1:
            raise ValueError("benchmark function results disagree between blocks")
        report[mode] = {
            "paired_rounds": rounds,
            "median_balanced_candidate_over_baseline_ratio": statistics.median(paired),
            "min_paired_ratio": min(paired),
            "max_paired_ratio": max(paired),
            "count_candidate_faster_in_block": sum(r < 1 for r in paired),
            "paired_round_ratios": paired,
            "checksum": next(iter(checksums)),
            "qualification": "exploratory-call-level-only; not CPU cycles or demonstrated speedup",
        }
    return report

def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--objects", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--calls", type=int, default=2_400_000)
    ap.add_argument("--rounds", type=int, default=16)
    ap.add_argument("--pin-cpu", action="store_true")
    a = ap.parse_args()
    if a.out.exists():
        ap.error("refusing to overwrite prior evidence")
    if a.calls < 8000 or a.calls > 200_000_000 or a.calls % 8 or not 4 <= a.rounds <= 100:
        ap.error("bounded calls divisible by 8 and 4..100 rounds required")
    if platform.machine().lower() not in ("x86_64", "amd64") or platform.system() != "Linux":
        ap.error("native Linux x86-64 SysV required")
    here = Path(__file__).resolve().parent
    objects = {}
    for name, (expected, *_rest) in EXPECTED.items():
        digest = hashlib.sha256((a.objects / name).read_bytes()).hexdigest()
        if digest != expected:
            raise ValueError(f"pinned ELF object hash mismatch: {name}")
        objects[name] = digest
    with tempfile.TemporaryDirectory(prefix="tier1-balanced-") as td:
        folder = Path(td)
        files = []
        for name, (_expected, old, new) in EXPECTED.items():
            target = folder / name
            sh(["objcopy", "--redefine-sym", f"{old}={new}", str(a.objects / name), str(target)])
            files.append(str(target))
        exe = folder / "benchmark"
        sh(["gcc", "-O2", "-std=gnu11", "-Wall", "-Wextra", "-Werror",
            "-fno-lto", "-fno-omit-frame-pointer", str(here / "benchmark_order_balanced.c"),
            *files, "-o", str(exe)])
        allowed = sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else []
        pinned = None
        if a.pin_cpu and allowed:
            pinned = allowed[0]
            os.sched_setaffinity(0, {pinned})
        output = sh([str(exe), str(a.calls), str(a.rounds)]).stdout
        rows = list(csv.DictReader(io.StringIO(output)))
    summary = check_rows(rows, a.calls, a.rounds)
    cpu = next((s.split(":", 1)[1].strip() for s in Path("/proc/cpuinfo").read_text().splitlines()
                if s.startswith("model name")), "unknown")
    data = {
        "schema": "zeno-fcis/cranelift-tier1-balanced-performance-screen/2",
        "status": "exploratory-order-balanced; no-verified-speedup",
        "upstream_wasmtime_commit": PIN,
        "original_object_sha256": objects,
        "benchmark_source_sha256": {
            name: hashlib.sha256((here / name).read_bytes()).hexdigest()
            for name in ("benchmark_exact_objects.c", "benchmark_order_balanced.c", "benchmark_order_balanced.py")},
        "host": {"system": platform.system(), "cpu": cpu, "pinned_cpu": pinned,
                 "allowed_cpus": allowed, "kernel": platform.release()},
        "calls_per_sample": a.calls, "rounds": a.rounds,
        "measurement_order": "ABBA / BAAB alternating; rotated workload mode order; no I/O within timed blocks",
        "clock": "CLOCK_MONOTONIC_RAW",
        "raw_samples": rows, "summary": summary,
        "nonclaims": ["not ISLE-patched compiler evidence", "not a speed claim",
                      "not an instruction-latency or throughput measurement",
                      "no multi-architecture performance guarantee", "no whole application workload"],
    }
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(data, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": data["status"], "modes": {m: {
        "paired_median": x["median_balanced_candidate_over_baseline_ratio"],
        "range": [x["min_paired_ratio"], x["max_paired_ratio"]]}
        for m, x in summary.items()}}, sort_keys=True))

if __name__ == "__main__":
    main()
