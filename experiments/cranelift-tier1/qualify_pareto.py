#!/usr/bin/env python3
"""Fail-closed synthesis of *size-qualified* and *performance-unqualified* receipts.

The initial 2.8 pilot deliberately does not convert noisy exploratory wall-clock
observations into a speed, latency, or throughput claim. It binds the existing
independent CLIF size/SMT/native receipts to exact original Cranelift ELF bytes.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
import statistics
from pathlib import Path

BASE_HASH = "fd23bd8da2b5b758112bdab485a1cd1b1f45251f7705af2cc5239fb793dff107"
CAND_HASH = "8b4e6f7fc04a4d028861cfcffff3b5652a6871ca7b09546dd4e4f77045605d17"
UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
MODES = ("chain_nonoverflow", "chain_saturating", "parallel8")

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def refuse(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)

def check_size(data: dict) -> None:
    refuse(data.get("qualification_status") == "qualified-measured-clif-only", "source CLIF qualification status")
    refuse(data.get("wasmtime_pin") == UPSTREAM, "Cranelift revision pin")
    refuse(data.get("smt", {}).get("status") == "unsat-counterexample-query", "independent QF_BV proof status")
    refuse(data.get("native_cranelift_object_execution", {}).get("status") == "pass", "native finite verification")
    refuse(data.get("contract") == "signed i64 saturating addition, two pure-result scalar CLIF expressions", "contract")
    results = data.get("results", {})
    refuse(results.get("baseline", {}).get("object_sha256") == BASE_HASH, "original baseline object identity")
    refuse(results.get("candidate", {}).get("object_sha256") == CAND_HASH, "original candidate object identity")
    a, b = results["baseline"], results["candidate"]
    refuse(a["symbol_bytes"] == 56 and b["symbol_bytes"] == 39, "original emitted byte evidence")
    refuse(a["straight_line_code_prefix_through_ret_bytes"] == 46 and
           b["straight_line_code_prefix_through_ret_bytes"] == 39, "original code prefix evidence")
    refuse(data.get("delta", {}).get("whole_symbol_bytes_baseline_minus_candidate") == 17 and
           data["delta"]["straight_line_code_bytes_baseline_minus_candidate"] == 7, "difference arithmetic")

def check_perf(data: dict) -> None:
    refuse(data.get("schema") == "zeno-fcis/tier1-cranelift-call-level-benchmark/1", "performance schema")
    refuse(data.get("status") == "exploratory-observed-not-qualified-speedup", "performance status (no unproved speed claims)")
    refuse(data.get("upstream_wasmtime_commit") == UPSTREAM, "performance upstream pin")
    hashes = data.get("original_object_sha256", {})
    refuse(hashes.get("satadd.o") == BASE_HASH and hashes.get("satadd-shift.o") == CAND_HASH,
           "performance object identities")
    count = data.get("calls_per_sample", 0)
    rounds = data.get("rounds", 0)
    refuse(isinstance(count, int) and 8000 <= count <= 200_000_000 and count % 8 == 0 and
           isinstance(rounds, int) and 4 <= rounds <= 100, "bounded workload")
    rows = data.get("raw_samples", [])
    refuse(len(rows) == rounds * 6, "full raw sample coverage")
    for mode in MODES:
        groups = {}
        for variant in ("baseline", "candidate"):
            group = [r for r in rows if r.get("mode") == mode and r.get("variant") == variant]
            refuse(len(group) == rounds and sorted(int(r["round"]) for r in group) == list(range(rounds)),
                   "complete paired sample coverage")
            refuse(all(r.get("schema") == "tier1-bench-v1" and int(r["calls"]) == count
                       and int(r["ns"]) > 0 for r in group), "sample identity and positive timings")
            groups[variant] = [int(r["ns"]) / count for r in group]
        entry = data.get("summary", {}).get(mode, {})
        baseline = statistics.median(groups["baseline"])
        candidate = statistics.median(groups["candidate"])
        for key, want in (("median_baseline", baseline), ("median_candidate", candidate),
                          ("candidate_over_baseline_ratio", candidate / baseline)):
            refuse(isinstance(entry.get(key), (float, int)) and
                   math.isclose(entry[key], want, rel_tol=1e-12), "recomputed timing summary")
        checksums = {v: {r["checksum"] for r in rows if r.get("mode") == mode and
                         r.get("variant") == v} for v in ("baseline", "candidate")}
        refuse(len(checksums["baseline"]) == 1 and checksums["baseline"] == checksums["candidate"] and
               entry.get("checksum") == next(iter(checksums["baseline"])), "identical deterministic workload")

def build(size_path: Path, perf_path: Path) -> dict:
    size = json.loads(size_path.read_text())
    perf = json.loads(perf_path.read_text())
    check_size(size)
    check_perf(perf)
    return {
        "schema": "zeno-fcis/2.8-target-cost-qualification/1",
        "status": "code-size-qualified; runtime-exploratory-inconclusive",
        "cost_dimensions": {
            "symbol_size_bytes": "independently measured against same pinned Cranelift configuration",
            "instruction_prefix_bytes": "single-block function prefix through first ret; diagnostic only",
            "dependency_chain_call_ns": "exploratory wall-clock measurement; includes call and loop overhead",
            "parallel8_call_ns": "exploratory independent-chain measurement; not peak instruction throughput",
        },
        "upstream_wasmtime_revision": UPSTREAM,
        "semantic_contract": size["contract"],
        "proof_scope": "pure-result QF_BV CLIF expressions; not flag, poison, ABI, ISLE-chain or machine-code proof",
        "size_source_receipt_sha256": sha(size_path),
        "timing_source_receipt_sha256": sha(perf_path),
        "target": size["target"],
        "host": perf["host"],
        "entries": [
            {"name": "baseline", "object_sha256": BASE_HASH, "symbol_bytes": 56,
             "straight_line_code_prefix_bytes": 46, "semantic_proof": "qualified-on-declared-CLIF-model",
             "runtime_performance_status": "unknown"},
            {"name": "candidate", "object_sha256": CAND_HASH, "symbol_bytes": 39,
             "straight_line_code_prefix_bytes": 39, "semantic_proof": "qualified-on-declared-CLIF-model",
             "runtime_performance_status": "unknown"}
        ],
        "size_comparison": {"winner": "candidate", "symbol_byte_saving": 17, "prefix_byte_saving": 7},
        "runtime_observations": {mode: {"median_baseline_call_ns": perf["summary"][mode]["median_baseline"],
                                         "median_candidate_call_ns": perf["summary"][mode]["median_candidate"],
                                         "candidate_over_baseline_median_ratio": perf["summary"][mode]["candidate_over_baseline_ratio"],
                                         "qualification": "exploratory-not-an-instruction-or-application-performance-claim"}
                                 for mode in MODES},
        "pareto_relation": "incomparable-until-application-relevant-performance-cost-is-qualified",
        "automatic_compiler_optimization": "not demonstrated",
        "nonclaims": ["no proof of speedup", "no measured hardware instruction latency", "no qualified peak throughput", "no verified generic ISLE rewrite", "no extrapolation beyond this target or workload", "no unique Pareto optimum"],
    }

def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--size", type=Path, required=True)
    p.add_argument("--timing", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    refuse(not a.out.exists(), "fresh output required")
    x = build(a.size, a.timing)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(x, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": x["status"], "size_winner":x["size_comparison"]["winner"],
                      "runtime": x["pareto_relation"]}, sort_keys=True))

if __name__ == "__main__":
    main()
