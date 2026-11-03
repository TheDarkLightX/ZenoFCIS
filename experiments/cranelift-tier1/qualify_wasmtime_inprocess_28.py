#!/usr/bin/env python3
"""Fail-closed, source-pinned in-process Wasmtime AOT performance observation.

All timings are exploratory. This checker never infers a speed winner, an ISA
latency, or an optimization-mode-agnostic improvement from shared-runner clocks.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import platform
import re
import statistics
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCHEMA = "zeno-fcis/tier1-inprocess-wasmtime/1"
PINNED_UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
PRODUCER_RUN = 41724158745
MODULES = {
    "baseline": "6bf661feb593fe40ee3a8058613100902783f9f244856c0bc68de5142843afa6",
    "patched": "16f6350d308b36641df4cc41052b8dab394d286fb7e3837ba78d085650006f75",
}
QUALIFIED_WAT = "dec49849bab1062369523d26fc30f9871aeca1c2cb2ea0670f590dcd8f0cf144"
MASK = (1 << 64) - 1
LO, HI = -(1 << 63), (1 << 63) - 1
ABBA = ("baseline", "patched", "patched", "baseline")
BAAB = ("patched", "baseline", "baseline", "patched")


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def reference(n: int) -> int:
    if not 1000 <= n <= 2_000_000:
        raise ValueError("out-of-range guest workload")
    x = 1
    for i in range(n):
        y = (HI - 7) if i & 1 else (LO + 9)
        x = max(LO, min(HI, x + y))
    return x


def parse_samples(raw: str, loops: int, reps: int, blocks: int) -> dict:
    if not 1000 <= loops <= 2_000_000 or not 1 <= reps <= 64 or not 2 <= blocks <= 32 or blocks % 2:
        raise ValueError("unqualified workload budget")
    lines = raw.splitlines()
    if len(lines) != 3 + blocks * 4:
        raise ValueError("missing, duplicate, or extra raw measurement line")
    if lines[0] != f"META,{SCHEMA},{loops},{reps},{blocks}":
        raise ValueError("invalid benchmark metadata or workload contract")
    if lines[1:3] != ["CHECK,baseline,1036,8", "CHECK,patched,1036,8"]:
        raise ValueError("missing finite correctness and signature evidence")
    want_checksum = ((reference(loops) & MASK) * reps) & MASK
    samples = []
    for block in range(blocks):
        order = ABBA if block % 2 == 0 else BAAB
        for pos, want_name in enumerate(order):
            cols = lines[3 + block * 4 + pos].split(",")
            if len(cols) != 6 or cols[0] != "SAMPLE":
                raise ValueError("malformed raw sample")
            try:
                got_block, got_pos, name, ns, checksum = (
                    int(cols[1]), int(cols[2]), cols[3], int(cols[4]), int(cols[5]))
            except ValueError as error:
                raise ValueError("invalid sample numeric encoding") from error
            if (got_block, got_pos, name) != (block, pos, want_name):
                raise ValueError("sample order, variant or block corrupted")
            if ns <= 0 or ns > 10**12:
                raise ValueError("nonpositive, impossible or unbounded timing")
            if checksum != want_checksum:
                raise ValueError("timed native result checksum incorrect")
            samples.append({"block": block, "position": pos,
                            "variant": name, "elapsed_ns": ns, "checksum": checksum})
    return {"samples": samples, "checksum": want_checksum}


def summarize(samples: list[dict], blocks: int) -> dict:
    ratios = []
    for block in range(blocks):
        four = [r for r in samples if r["block"] == block]
        if len(four) != 4:
            raise ValueError("incomplete paired block")
        a = [r["elapsed_ns"] for r in four if r["variant"] == "baseline"]
        b = [r["elapsed_ns"] for r in four if r["variant"] == "patched"]
        if len(a) != 2 or len(b) != 2:
            raise ValueError("unbalanced samples")
        ratios.append(statistics.median(b) / statistics.median(a))
    return {
        "metric": "in-process repeated guest workload duration, candidate/baseline",
        "ratios_by_block": ratios,
        "median_ratio": statistics.median(ratios),
        "median_first_half": statistics.median(ratios[:blocks // 2]),
        "median_second_half": statistics.median(ratios[blocks // 2:]),
        "min_block_ratio": min(ratios),
        "max_block_ratio": max(ratios),
        "status": "exploratory-on-one-virtualized-host-no-runtime-winner",
    }


def qualify(raw_path: Path, source_root: Path, cargo_lock: Path,
            loops: int, reps: int, blocks: int) -> dict:
    source = source_root / "wasm-check" / "qualification.json"
    q = json.loads(source.read_text())
    if (q.get("status") != "finite-wasm-aot-check-passed-runtime-unqualified"
            or q.get("source", {}).get("upstream_commit") != PINNED_UPSTREAM
            or q["source"].get("wasm_source_sha256") != QUALIFIED_WAT
            or set(q.get("compiled", {})) != set(MODULES)
            or not all(q["finite_checks"][k]["passed"] is True for k in MODULES)):
        raise ValueError("underlying pinned Wasmtime AOT correctness evidence missing")
    objects = {}
    for name, expected in MODULES.items():
        path = source_root / "wasm-check" / name / "satadd.cwasm"
        contents = path.read_bytes()
        if (sha(contents) != expected or q["compiled"][name].get("aot_sha256") != expected
                or len(contents) != q["compiled"][name].get("aot_bytes")):
            raise ValueError("incorrect AOT object identity")
        objects[name] = {"sha256": expected, "bytes": len(contents)}
    raw = raw_path.read_bytes()
    parsed = parse_samples(raw.decode("utf-8"), loops, reps, blocks)
    code_root = HERE / "inprocess_bench_28"
    build_sources = (code_root / "Cargo.toml", code_root / "src" / "main.rs", cargo_lock)
    return {
        "schema": "zeno-fcis/tier1-wasmtime-inprocess-qualification/1",
        "status": "source-bound-finite-runtime-passed-performance-inconclusive",
        "source": {"wasmtime_upstream": PINNED_UPSTREAM, "producer_run": PRODUCER_RUN,
                   "underlying_aot_receipt_sha256": sha(source.read_bytes()),
                   "objects": objects,
                   "benchmark_source_pins": {str(p.relative_to(HERE)) if p.is_relative_to(HERE) else str(p): sha(p.read_bytes())
                                             for p in build_sources},
                   "raw_measurements_sha256": sha(raw)},
        "host": {"architecture": platform.machine(),
                 "cpu": Path("/proc/cpuinfo").read_text(errors="replace").split("model name", 1)[-1][:140]
                        if Path("/proc/cpuinfo").is_file() else "unavailable"},
        "workload": {"loops_per_guest_call": loops, "calls_per_sample": reps,
                     "balanced_blocks": blocks, "finite_sat_inputs_per_variant": 1036,
                     "finite_loop_counts_per_variant": 8},
        "summary": summarize(parsed["samples"], blocks),
        "samples": parsed["samples"],
        "nonclaims": [
            "not a CPU instruction-latency, throughput, or application-wide speed measurement",
            "shared GitHub runner clock/thermal/frequency noise uncontrolled",
            "one Wasm synthetic saturating-add workload; no broad performance regression guarantee",
            "the named ISLE rule proof is conditional on local provisional select spec",
            "no supported general claim of target-optimal or faster machine code",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    for arg in ("raw", "source-root", "cargo-lock", "out"):
        p.add_argument("--" + arg, type=Path, required=True)
    p.add_argument("--loops", type=int, required=True)
    p.add_argument("--reps", type=int, required=True)
    p.add_argument("--blocks", type=int, required=True)
    a = p.parse_args()
    if a.out.exists():
        p.error("fresh receipt required")
    report = qualify(a.raw.resolve(), a.source_root.resolve(), a.cargo_lock.resolve(),
                     a.loops, a.reps, a.blocks)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "timings": report["summary"]}, sort_keys=True))


if __name__ == "__main__":
    main()
