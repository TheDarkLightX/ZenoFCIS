#!/usr/bin/env python3
"""Replay the exact balanced 2.8 timing receipt without asserting a speed winner.

Consumes the original, independently qualified CLIF code-size record and the
unchanged, source-pinned ABBA/BAAB timing receipt. Recomputes every per-round
ratio from raw rows, checks the run and source identities, and verifies the
original object bytes. This is an observational cost catalog, not a speed proof.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import math
import statistics
from pathlib import Path

from benchmark_order_balanced import MODES, V2, check_rows
from qualify_pareto import BASE_HASH, CAND_HASH, UPSTREAM, check_size

SCHEMA = "zeno-fcis/2.8-balanced-cost-qualification/1"
RAW_SHA256 = "2b760abd2567fa96d2c60bd51438dcb0605cb9927d3e61a7aae4d96e860da4c8"
EXPECTED_OBJECTS = {"satadd.o": BASE_HASH, "satadd-shift.o": CAND_HASH}
SOURCES = ("benchmark_exact_objects.c", "benchmark_order_balanced.c",
           "benchmark_order_balanced.py")
ROW_KEYS = {"schema", "round", "mode", "position", "variant", "calls", "ns", "checksum"}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def refuse(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def check_balanced(data: dict, root: Path) -> dict:
    """Fail closed on identity, measurement order and recomputed summaries."""
    refuse(data.get("schema") == "zeno-fcis/cranelift-tier1-balanced-performance-screen/2",
           "balanced receipt schema")
    refuse(data.get("status") == "exploratory-order-balanced; no-verified-speedup",
           "no qualified speed claim permitted")
    refuse(data.get("upstream_wasmtime_commit") == UPSTREAM, "upstream compiler pin")
    refuse(data.get("original_object_sha256") == EXPECTED_OBJECTS,
           "object SHA-256 identities")
    refuse(data.get("clock") == "CLOCK_MONOTONIC_RAW", "timing clock identity")
    refuse(data.get("measurement_order") ==
           "ABBA / BAAB alternating; rotated workload mode order; no I/O within timed blocks",
           "measurement order identity")
    host = data.get("host", {})
    refuse(host.get("system") == "Linux" and isinstance(host.get("cpu"), str) and
           bool(host["cpu"].strip()), "measured host identity")
    allowed = host.get("allowed_cpus")
    refuse(isinstance(allowed, list) and allowed and
           all(type(c) is int and c >= 0 for c in allowed) and
           len(set(allowed)) == len(allowed) and
           type(host.get("pinned_cpu")) is int and host["pinned_cpu"] in allowed,
           "pinned CPU identity")
    pins = data.get("benchmark_source_sha256", {})
    refuse(isinstance(pins, dict) and set(pins) == set(SOURCES),
           "complete benchmark source identities")
    for name in SOURCES:
        refuse((root / name).is_file() and pins[name] == digest((root / name).read_bytes()),
               f"benchmark source hash mismatch: {name}")
    calls, rounds = data.get("calls_per_sample"), data.get("rounds")
    refuse(type(calls) is int and 8000 <= calls <= 200_000_000 and calls % 8 == 0 and
           type(rounds) is int and 4 <= rounds <= 100, "bounded benchmark parameters")
    rows = data.get("raw_samples")
    refuse(isinstance(rows, list) and len(rows) == 12 * rounds and
           all(isinstance(r, dict) and set(r) == ROW_KEYS and r["schema"] == V2
               for r in rows), "exact raw sample coverage and schema")
    computed = check_rows(rows, calls, rounds)
    saved = data.get("summary")
    refuse(isinstance(saved, dict) and set(saved) == set(MODES), "complete recorded summary")
    for mode in MODES:
        a, b = saved[mode], computed[mode]
        refuse(isinstance(a, dict) and set(a) == set(b), "recorded summary structure")
        for key, want in b.items():
            got = a[key]
            if isinstance(want, float):
                refuse(type(got) in (int, float) and math.isfinite(got) and
                       math.isclose(got, want, rel_tol=1e-12, abs_tol=0),
                       f"forged summary: {mode}.{key}")
            elif isinstance(want, list):
                refuse(isinstance(got, list) and len(got) == len(want) and
                       all(type(x) in (int, float) and math.isfinite(x) and
                           math.isclose(x, y, rel_tol=1e-12, abs_tol=0)
                           for x, y in zip(got, want)),
                       f"forged per-round ratios: {mode}.{key}")
            else:
                refuse(got == want, f"forged summary field: {mode}.{key}")
    nonclaims = data.get("nonclaims")
    refuse(isinstance(nonclaims, list) and "not a speed claim" in nonclaims and
           "not ISLE-patched compiler evidence" in nonclaims,
           "original performance scope restrictions")
    return computed


def read_receipt(path: Path) -> tuple[dict, str]:
    compressed = path.read_bytes()
    raw = gzip.decompress(compressed) if path.name.endswith(".gz") else compressed
    refuse(len(raw) <= 2_000_000 and digest(raw) == RAW_SHA256,
           "historical balanced timing byte identity")
    return json.loads(raw), digest(raw)


def build(size_path: Path, timing_path: Path, objects: Path, root: Path) -> dict:
    size_raw = size_path.read_bytes()
    size = json.loads(size_raw)
    check_size(size)
    timing, timing_sha = read_receipt(timing_path)
    summaries = check_balanced(timing, root)
    for name, want in EXPECTED_OBJECTS.items():
        refuse((objects / name).is_file() and digest((objects / name).read_bytes()) == want,
               f"original emitted object bytes: {name}")
    observations = {}
    for mode in MODES:
        values = summaries[mode]["paired_round_ratios"]
        half = len(values) // 2
        observations[mode] = {
            "paired_round_count": len(values),
            "median_candidate_over_baseline": statistics.median(values),
            "min_paired_ratio": min(values), "max_paired_ratio": max(values),
            "first_half_median": statistics.median(values[:half]),
            "second_half_median": statistics.median(values[half:]),
            "candidate_faster_round_count": sum(x < 1 for x in values),
            "interpretation": "descriptive, call-level, order-balanced; no speed determination",
        }
    return {
        "schema": SCHEMA,
        "status": "code-size-qualified; exploratory-runtime-unqualified",
        "upstream_wasmtime_commit": UPSTREAM,
        "source_object_sha256": EXPECTED_OBJECTS,
        "source_size_receipt_sha256": digest(size_raw),
        "source_balanced_receipt_sha256": timing_sha,
        "benchmark_source_sha256": timing["benchmark_source_sha256"],
        "target": size["target"],
        "host": timing["host"],
        "contract": size["contract"],
        "code_size": {"baseline_function_bytes": 56, "candidate_function_bytes": 39,
                      "candidate_symbol_bytes_saved": 17,
                      "baseline_executable_prefix_bytes": 46,
                      "candidate_executable_prefix_bytes": 39},
        "runtime_observations": observations,
        "pareto_relation": "unknown: no application-relevant qualified speed, latency or throughput costs",
        "compiler_rewrite_status": "unqualified: manually different CLIF, not patched compiler",
        "nonclaims": ["no speedup or slowdown claim", "not isolated instruction latency",
                      "not peak instruction throughput", "no CPU-cycle measurement",
                      "not VeriISLE rule verification", "no compiler patch performance claim",
                      "not complete ABI/ISA equivalence", "not multi-target generalization"],
    }


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--size", type=Path, required=True)
    p.add_argument("--timing", type=Path, required=True)
    p.add_argument("--objects", type=Path, required=True)
    p.add_argument("--root", type=Path, default=Path(__file__).resolve().parent)
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    refuse(not a.out.exists(), "fresh output required")
    result = build(a.size, a.timing, a.objects, a.root)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": result["status"],
                      "size_bytes_saved": result["code_size"]["candidate_symbol_bytes_saved"],
                      "runtime": result["pareto_relation"]}, sort_keys=True))

if __name__ == "__main__":
    main()
